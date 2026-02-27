//! App Runtime Crate (Orchestration).
//!
//! Wires together all the components and spawns the task graph.

use arc_swap::ArcSwap;
use core_types::config::AppConfig;
use core_types::{EventKind, OrderRequest, OrderType, Side, TimeInForce};
use event_bus::{ChannelConfig, EventBus, SystemChannels};
use metrics_observability::{
    log_decision, DecisionAction, DecisionLog, LatencyTracker, SLA_LIMIT_MICROS,
};
use risk_engine::sizing::{PositionSizer, PricingModel, SizingConfig};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task;
use tokio_util::sync::CancellationToken;
use watchlist_engine::WatchlistSnapshot;

const RISK_STATE_PATH: &str = "/var/run/rps/risk_state.json";

fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

pub async fn run(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure runtime directory exists
    if let Some(parent) = Path::new(RISK_STATE_PATH).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let channel_config = ChannelConfig::default();
    let channels = SystemChannels::new(channel_config);

    // Dedicated channel for Decision Logs (FastLoop -> Metrics)
    let (decision_tx, mut decision_rx) = mpsc::channel::<DecisionLog>(8192);

    // Shared State: Watchlist Snapshot
    let watchlist_snapshot = Arc::new(ArcSwap::new(Arc::new(WatchlistSnapshot::default())));

    let account_capital = config.risk.account_capital_usd;

    // Load Risk State from file or create new
    let risk_state_inner = if Path::new(RISK_STATE_PATH).exists() {
        log::info!("Loading persistent risk state from {}", RISK_STATE_PATH);
        match risk_engine::RiskState::load_from_file(Path::new(RISK_STATE_PATH)) {
            Ok(mut state) => {
                // Override config values
                state.initial_max_daily_loss = config.risk.max_daily_loss_usd;
                state.liquidity_config = core_types::LiquidityConfig {
                    min_avg_daily_volume: config.universe.min_avg_daily_volume,
                    min_addv_usd: config.universe.min_addv_usd,
                    ..core_types::LiquidityConfig::default()
                };
                state
            }
            Err(e) => {
                log::error!("Failed to load risk state: {}. Starting fresh.", e);
                risk_engine::RiskState::new(
                    config.risk.max_daily_loss_usd,
                    core_types::LiquidityConfig {
                        min_avg_daily_volume: config.universe.min_avg_daily_volume,
                        min_addv_usd: config.universe.min_addv_usd,
                        ..Default::default()
                    },
                )
            }
        }
    } else {
        risk_engine::RiskState::new(
            config.risk.max_daily_loss_usd,
            core_types::LiquidityConfig {
                min_avg_daily_volume: config.universe.min_avg_daily_volume,
                min_addv_usd: config.universe.min_addv_usd,
                ..Default::default()
            },
        )
    };

    let risk_state = Arc::new(Mutex::new(risk_state_inner));
    let shutdown_token = CancellationToken::new();
    let system_monitor_only = Arc::new(AtomicBool::new(false));
    let smo_clone = system_monitor_only.clone();

    // 1. Spawn Metrics Task
    let mut metrics_rx = channels.metrics_rx;
    task::spawn(async move {
        log::info!("Metrics Task started");
        let mut latency_tracker = LatencyTracker::new(1000);

        loop {
            tokio::select! {
                Some(_event) = metrics_rx.recv() => {
                    // Process raw events for data quality metrics
                }
                Some(log) = decision_rx.recv() => {
                    // 1. Log Decision (Structured)
                    log_decision(&log);

                    // 2. Track Latency (P95)
                    let total_latency = log.latency_src_rx + log.latency_rx_proc + log.latency_proc_decision;
                    latency_tracker.record(total_latency);

                    // 3. SLA Breach Check
                    let p95 = latency_tracker.p95();
                    if p95 > SLA_LIMIT_MICROS {
                        log::warn!("SLA BREACH! P95 Latency: {}us > Limit: {}us", p95, SLA_LIMIT_MICROS);
                    }
                }
            }
        }
    });

    // 2. Spawn OMS Task
    let mut oms_rx = channels.oms_market_rx;
    let mut oms_order_rx = channels.oms_order_rx;
    let _oms_tx = channels.oms_market_tx.clone();
    task::spawn(async move {
        log::info!("OMS Task started");
        let mut oms = execution_engine::OrderManagementSystem::new();
        let mut timeout_check = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = timeout_check.tick() => {
                    let now = now_micros();
                    let timeouts = oms.check_timeouts(now, 30_000_000); // 30s timeout
                    for order_id in &timeouts {
                        oms.cancel_order(*order_id, now);
                        log::warn!("Order {} timed out after 30s — marked Cancelled locally. \
                                    Python bridge must cancel with broker.", order_id);
                        // TODO: A dedicated Rust→Python cancel command channel is required for production.
                        // When an order times out, a CancelOrder(broker_order_id) message must be sent to
                        // the Python bridge, which calls ib.cancelOrder(). This requires adding:
                        // 1. A new UDS message type: { "type": "cancel", "broker_order_id": "..." }
                        // 2. A Sender<CancelCommand> passed into OmsTask
                        // 3. Python UDS receiver loop that handles cancel commands alongside market data
                    }
                    if !timeouts.is_empty() {
                        log::warn!("Timed out orders: {:?}", timeouts);
                    }
                }
                Some(event) = oms_rx.recv() => {
                    match event.kind {
                        EventKind::Fill(fill) => oms.handle_fill(fill, event.ts_rx),
                        EventKind::OrderStatus(status) => oms.handle_status(status, event.ts_rx),
                        EventKind::StateSync(sync) => {
                            let stale = oms.reconcile_state(sync);
                            if !stale.is_empty() {
                                log::warn!("Stale orders found during sync: {:?}", stale);
                            }
                        },
                        EventKind::Reconnect => {
                            log::warn!("OMS received Reconnect - waiting for StateSync");
                        },
                        _ => {}
                    }
                }
                Some(request) = oms_order_rx.recv() => {
                    let now = now_micros();
                    if let Err(e) = oms.place_order(request, now) {
                        log::error!("OMS rejected order: {}", e);
                    } else {
                        log::info!("OMS placed order");
                    }
                }
            }
        }
    });

    // 3. Spawn Risk Task
    let mut risk_rx = channels.risk_rx;
    let risk_state_clone = risk_state.clone();
    let shutdown_token_clone = shutdown_token.clone();

    task::spawn(async move {
        log::info!("Risk Task started");

        struct RiskPosition {
            avg_cost: f64,
            qty: i32,
            realized_pnl: f64,
        }
        let mut positions: HashMap<core_types::SymbolId, RiskPosition> = HashMap::new();
        let mut intraday_buy_tracker: HashMap<core_types::SymbolId, (u32, bool)> = HashMap::new();
        let mut global_realized_pnl = 0.0;

        while let Some(event) = risk_rx.recv().await {
            match event.kind {
                EventKind::Fill(fill) => {
                    // Calculate PnL logic (Same as TapeEngine)
                    let state = positions.entry(event.symbol_id).or_insert(RiskPosition {
                        avg_cost: 0.0,
                        qty: 0,
                        realized_pnl: 0.0,
                    });

                    let fill_size = fill.size as i32;
                    let signed_fill_size = if fill.side == core_types::Side::Bid {
                        fill_size
                    } else {
                        -fill_size
                    };
                    let fill_price = fill.price;

                    if state.qty == 0 {
                        state.qty = signed_fill_size;
                        state.avg_cost = fill_price;
                    } else {
                        let same_side = (state.qty > 0 && signed_fill_size > 0)
                            || (state.qty < 0 && signed_fill_size < 0);

                        if same_side {
                            let total_cost = (state.qty as f64 * state.avg_cost)
                                + (signed_fill_size as f64 * fill_price);
                            state.qty += signed_fill_size;
                            state.avg_cost = total_cost / state.qty as f64;
                        } else {
                            let close_qty = std::cmp::min(state.qty.abs(), signed_fill_size.abs());
                            let signed_close_qty =
                                if state.qty > 0 { -close_qty } else { close_qty };

                            let trade_pnl =
                                (fill_price - state.avg_cost) * (-signed_close_qty as f64);
                            state.realized_pnl += trade_pnl;
                            global_realized_pnl += trade_pnl;

                            let prev_qty = state.qty;
                            state.qty += signed_fill_size;

                            if state.qty == 0 {
                                state.avg_cost = 0.0;
                            } else if (prev_qty > 0 && state.qty < 0)
                                || (prev_qty < 0 && state.qty > 0)
                            {
                                state.avg_cost = fill_price;
                            }
                        }
                    }

                    // PDT Logic & T+1 Settlement
                    let today_ordinal = (event.ts_src / 1_000_000 / 86400) as u32;

                    if fill.side == core_types::Side::Bid {
                        intraday_buy_tracker.insert(event.symbol_id, (today_ordinal, true));
                    } else {
                        // Sell side
                        // T+1 Settlement
                        let proceeds = (fill.size as f64) * fill.price;
                        let settle_date = today_ordinal + 1; // T+1

                        let mut guard = match risk_state_clone.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                log::error!("Mutex poisoned in risk_state: {}", e);
                                continue;
                            }
                        };
                        let current = guard
                            .unsettled_proceeds
                            .get(&settle_date)
                            .copied()
                            .unwrap_or(0.0);
                        guard
                            .unsettled_proceeds
                            .insert(settle_date, current + proceeds);

                        // PDT Check
                        if let Some((buy_date, has_buy)) =
                            intraday_buy_tracker.get(&event.symbol_id)
                        {
                            if *has_buy && *buy_date == today_ordinal {
                                guard
                                    .pdt_guard
                                    .record_day_trade(today_ordinal, event.symbol_id.0);
                                intraday_buy_tracker.remove(&event.symbol_id);
                            }
                        }
                    }

                    // Update RiskState PnL
                    if let Ok(mut guard) = risk_state_clone.lock() {
                        guard.update_pnl(global_realized_pnl);
                    }
                }
                EventKind::Heartbeat => {
                    if let Ok(guard) = risk_state_clone.lock() {
                        if guard.should_terminate() {
                            log::error!("Risk Termination Triggered!");
                            shutdown_token_clone.cancel();
                        }
                    }
                }
                EventKind::StateSync(sync) => {
                    if let Ok(mut guard) = risk_state_clone.lock() {
                        guard.rebuild_state(sync.positions);
                    }
                }
                _ => {}
            }
        }
    });

    // 4. Spawn SlowLoop Task
    let mut slow_loop_rx = channels.slow_loop_rx;
    let slow_snapshot_writer = watchlist_snapshot.clone();
    task::spawn(async move {
        log::info!("SlowLoop Task started");
        let mut watchlist = watchlist_engine::Watchlist::new();
        while let Some(event) = slow_loop_rx.recv().await {
            match event.kind {
                EventKind::Tick(_) => {
                    watchlist.update_tick_count(event.symbol_id);
                    match watchlist.promote(event.symbol_id) {
                        Ok(()) => {
                            log::info!("Promoted symbol {:?}", event.symbol_id);
                        }
                        Err("Not TickReady") | Err("Already in Tier A") => {}
                        Err(e) => {
                            log::warn!("Promotion failed for {:?}: {}", event.symbol_id, e);
                        }
                    }
                }
                EventKind::Snapshot(_) => {
                    watchlist.touch(event.symbol_id, event.ts_src);
                }
                _ => {}
            }
            slow_snapshot_writer.store(Arc::new(watchlist.snapshot()));
        }
    });

    // 5. Spawn FastLoop Task
    let mut fast_loop_rx = channels.fast_loop_rx;
    let fast_snapshot_reader = watchlist_snapshot.clone();
    let risk_state_fast = risk_state.clone();
    let order_tx = channels.oms_order_tx;

    task::spawn(async move {
        log::info!("FastLoop Task started");

        let guard_config = risk_engine::guards::GuardConfig::default();

        let pricing_model = PricingModel {
            commission_per_share: config.pricing.commission_per_share,
            sec_fee_rate: config.pricing.sec_fee_rate,
            taf_rate: config.pricing.taf_rate,
            slippage_alpha: config.pricing.slippage_alpha,
            slippage_beta: config.pricing.slippage_beta,
            min_net_profit_usd: config.pricing.min_net_profit_usd,
        };

        let mut tape_engine =
            tape_engine::TapeEngine::new(risk_state_fast, guard_config, config.tape.clone(), pricing_model);

        let sizing_config = SizingConfig {
            risk_per_trade_usd: config.risk.risk_per_trade_usd,
            min_stop_distance_cents: config.pricing.min_stop_abs_usd,
            max_position_pct_nav: config.risk.max_position_pct,
            liquidity_cap_pct: 0.001, // 0.1% of ADDV per §16.2
            budget_cap_usd: account_capital * config.risk.budget_cap_pct,
        };
        let position_sizer = PositionSizer::new(sizing_config);

        while let Some(event) = fast_loop_rx.recv().await {
            let ts_proc_start = now_micros();

            // Zero-allocation path
            let _snapshot = fast_snapshot_reader.load();

            // Check if we should terminate immediately (Risk-Off)
            if tape_engine.should_terminate() {
                log::error!("RISK LIMIT BREACHED! HALTING TRADING IMMEDIATELY.");

                let open_positions: Vec<(core_types::SymbolId, i32)> = tape_engine
                    .symbol_states
                    .iter()
                    .filter(|(_, s)| s.position != 0)
                    .map(|(id, s)| (*id, s.position))
                    .collect();

                if !open_positions.is_empty() {
                    log::warn!(
                        "RISK LIMIT BREACHED — Sending CLOSE ALL for {} open positions",
                        open_positions.len()
                    );
                    for (symbol_id, qty) in open_positions {
                        let close_side = if qty > 0 { Side::Ask } else { Side::Bid };
                        let close_qty = qty.unsigned_abs();

                        let request = OrderRequest {
                            symbol_id,
                            side: close_side,
                            qty: close_qty,
                            order_type: OrderType::Market,
                            limit_price: None,
                            stop_price: None,
                            tif: TimeInForce::IOC,
                            idempotency_key: format!("CLOSE-ALL-{}-{}", symbol_id.0, event.ts_src),
                            take_profit_price: None,
                            stop_loss_price: None,
                        };

                        if let Err(e) = order_tx.try_send(request) {
                            log::error!(
                                "Failed to send CLOSE ALL order for {:?}: {:?}",
                                symbol_id,
                                e
                            );
                        }
                    }
                }
                break;
            }

            let decision_result = tape_engine.on_event(&event);

            let ts_decision_end = now_micros();

            // Emit Decision Log
            if let core_types::EventKind::Tick(tick) = event.kind {
                let log = DecisionLog {
                    symbol_id: event.symbol_id,
                    timestamp: ts_decision_end,
                    action: if decision_result.is_ok() {
                        DecisionAction::Enter
                    } else {
                        DecisionAction::Reject
                    },
                    reject_reason: decision_result.err(),
                    latency_src_rx: event.ts_rx.saturating_sub(event.ts_src),
                    latency_rx_proc: ts_proc_start.saturating_sub(event.ts_rx),
                    latency_proc_decision: ts_decision_end.saturating_sub(ts_proc_start),
                    price: tick.price,
                    tape_score: 0.0,
                };
                let _ = decision_tx.try_send(log);

                if decision_result.is_ok() {
                    let daily_volume =
                        if let Some(state) = tape_engine.symbol_states.get(&event.symbol_id) {
                            state
                                .daily_context
                                .as_ref()
                                .map(|c| c.volume_profile.avg_20d_volume)
                                .unwrap_or(0)
                        } else {
                            0
                        };

                    let stop_dist = position_sizer.config.min_stop_distance_cents;
                    let stop_price = tick.price - stop_dist;

                    let today_ordinal = (event.ts_src / 1_000_000 / 86400) as u32;
                    let available_cash = if let Ok(guard) = tape_engine.risk_state.lock() {
                        guard.available_cash(today_ordinal, account_capital)
                    } else {
                        0.0
                    };

                    let qty = position_sizer.calculate_size(
                        account_capital,
                        tick.price,
                        stop_price,
                        daily_volume,
                        available_cash,
                    );

                    if qty > 0 {
                        let request = OrderRequest {
                            symbol_id: event.symbol_id,
                            side: Side::Bid,
                            qty,
                            order_type: OrderType::Limit,
                            limit_price: Some(tick.price),
                            stop_price: None,
                            tif: TimeInForce::IOC,
                            idempotency_key: format!("{}-{}", event.symbol_id.0, event.ts_src),
                            take_profit_price: Some(tick.price + 2.0 * stop_dist),
                            stop_loss_price: Some(stop_price),
                        };

                        if let Err(e) = order_tx.try_send(request) {
                            match e {
                                mpsc::error::TrySendError::Full(_) => {
                                    log::warn!("OMS Order Queue Full!")
                                }
                                mpsc::error::TrySendError::Closed(_) => {
                                    log::error!("OMS Order Queue Closed!")
                                }
                            }
                        }
                    }
                }
            }

            // Persist RiskState on Fills (critical updates)
            if let core_types::EventKind::Fill(_) = event.kind {
                if tape_engine.global_realized_pnl >= 50.0 && !tape_engine.daily_target_reached {
                    tape_engine.set_daily_target_reached(true);
                    log::info!(
                        "Daily profit target reached (${:.2}). Raising TapeScore threshold to {}.",
                        tape_engine.global_realized_pnl,
                        tape_engine.config.tape_threshold_post_target
                    );
                }

                // Access mutex to save
                if let Ok(guard) = tape_engine.risk_state.lock() {
                    if let Err(e) = guard.save_to_file(Path::new(RISK_STATE_PATH)) {
                        log::error!("Failed to persist risk state: {}", e);
                    }
                }
            }
        }
    });

    // 6. Spawn DataRouter Task
    let mut bridge_rx = channels.bridge_rx;
    let fast_tx = channels.fast_loop_tx;
    let slow_tx = channels.slow_loop_tx;
    let oms_tx = channels.oms_market_tx;
    let metrics_tx = channels.metrics_tx;
    let risk_tx = channels.risk_tx;

    task::spawn(async move {
        log::info!("DataRouter Task started");
        while let Some(event) = bridge_rx.recv().await {
            // Fan-out
            // Fire-and-forget fan-out: receivers are non-blocking, drops are acceptable under backpressure
            let _ = metrics_tx.try_send(event.clone());

            match event.kind {
                EventKind::Tick(_) | EventKind::L2Delta(_) => {
                    let _ = fast_tx.try_send(event.clone());
                    let _ = slow_tx.try_send(event.clone());
                }
                EventKind::Snapshot(_) => {
                    let _ = slow_tx.try_send(event.clone());
                }
                EventKind::Fill(_) | EventKind::OrderStatus(_) | EventKind::Reject(_) => {
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    if let EventKind::Fill(_) = event.kind {
                        let _ = fast_tx.try_send(event.clone());
                    }
                }
                EventKind::Heartbeat => {
                    let _ = risk_tx.try_send(event.clone());
                }
                EventKind::Reconnect => {
                    log::warn!("Bridge Reconnected - Triggering Sync");
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    let _ = fast_tx.try_send(event.clone());
                }
                EventKind::StateSync(_) => {
                    log::info!("Received StateSync");
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    let _ = fast_tx.try_send(event.clone());
                }
            }
        }
    });

    // 7. Spawn BridgeRx Task
    let bridge_tx = channels.bridge_tx;
    let (_dummy_tx, dummy_rx) = mpsc::channel(1);
    let (degraded_tx, mut degraded_rx) = mpsc::channel::<bool>(8);

    let bridge_bus = EventBus {
        tx: bridge_tx,
        rx: dummy_rx,
    };

    let mut bridge_task = bridge_rx::BridgeRxTask::new("/var/run/rps/rps_uds.sock", bridge_bus)?;
    bridge_task.set_degraded_notifier(degraded_tx);
    task::spawn(async move {
        bridge_task.run().await;
    });

    // Keep main alive until shutdown triggered
    tokio::select! {
        _ = shutdown_token.cancelled() => {
            log::warn!("Shutdown triggered by Risk Task!");
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Shutdown triggered by Ctrl+C");
        }
        Some(is_degraded) = degraded_rx.recv() => {
            if let Ok(mut guard) = risk_state.lock() {
                if is_degraded {
                    guard.set_monitor_only(true);
                    smo_clone.store(true, Ordering::SeqCst);
                    log::warn!("DataQuality DEGRADED — entering Monitor Only automatically.");
                } else if smo_clone.load(Ordering::SeqCst) {
                    // Only auto-restore if WE set it
                    guard.set_monitor_only(false);
                    smo_clone.store(false, Ordering::SeqCst);
                    log::info!("DataQuality RESTORED — exiting Monitor Only.");
                }
            }
        }
    }

    Ok(())
}
