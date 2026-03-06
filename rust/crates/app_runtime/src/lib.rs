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
        let mut halted_symbols: std::collections::HashMap<core_types::SymbolId, u64> = std::collections::HashMap::new();
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
                        EventKind::Halt => {
                            // §20.3: If open position exists, start 5-minute wait timer.
                            // The bot will attempt EMERGENCY_EXIT on resume.
                            // For now: log + set symbol as halted in a local set.
                            log::warn!("HALT received for symbol {:?} — monitoring for resume", event.symbol_id);
                            halted_symbols.insert(event.symbol_id, event.ts_src);
                        }
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
    let risk_state_slow = risk_state.clone();
    task::spawn(async move {
        log::info!("SlowLoop Task started");
        let mut watchlist = watchlist_engine::Watchlist::new();

        // Regime engine — one global instance
        let regime_params = regime_engine::RegimeParams {
            atr_normal_max: config.regime.atr_normal_max,
            atr_caution_max: config.regime.atr_caution_max,
            breadth_normal_min: config.regime.breadth_normal_min,
            breadth_caution_min: config.regime.breadth_caution_min,
            widening_caution_pct: config.regime.widening_caution_pct,
            widening_riskoff_pct: config.regime.widening_riskoff_pct,
        };
        let mut regime_eng = regime_engine::RegimeEngine::new(regime_params);

        // Per-symbol context and MTF engines
        let mut context_engines: std::collections::HashMap<
            core_types::SymbolId,
            context_engine::ContextEngine,
        > = std::collections::HashMap::new();
        let mut mtf_engines: std::collections::HashMap<
            core_types::SymbolId,
            mtf_engine::MtfEngine,
        > = std::collections::HashMap::new();
        // Price history for churn detection
        let mut price_history: std::collections::HashMap<
            core_types::SymbolId,
            core_types::TimeRingBuffer<f64>,
        > = std::collections::HashMap::new();

        let context_params = context_engine::ContextParams {
            volume_multiplier_2x: config.context.volume_multiplier_2x,
            volume_multiplier_3x: config.context.volume_multiplier_3x,
            sector_momentum_min_pct: config.context.sector_momentum_min_pct,
            churn_max_move_pct: config.context.churn_max_move_pct,
            churn_window_minutes: config.context.churn_window_minutes,
        };

        let mut subscription_count: u32 = 0;
        let sub_limit = config.ibkr.subscription_budget;
        let sub_warn_threshold = (sub_limit as f64 * config.ibkr.subscription_warn_pct) as u32;

        while let Some(event) = slow_loop_rx.recv().await {
            match event.kind {
                EventKind::Tick(tick) => {
                    // Subscription count tracking
                    if watchlist.get_tier(event.symbol_id) == Some(core_types::Tier::A) {
                        // Already Tier A — count it
                    } else {
                        // Attempt Tier B/A promotion
                        watchlist.update_tick_count(event.symbol_id);

                        // Check subscription budget before promoting to Tier A
                        if subscription_count >= sub_limit {
                            log::warn!(
                                "IBKR subscription limit ({}) reached — cannot promote {:?}",
                                sub_limit,
                                event.symbol_id
                            );
                        } else {
                            if subscription_count >= sub_warn_threshold {
                                log::warn!(
                                    "IBKR subscriptions at {}% — slowing promotions",
                                    (subscription_count * 100 / sub_limit)
                                );
                            }
                            match watchlist.promote(event.symbol_id) {
                                Ok(()) => {
                                    subscription_count += 1;
                                    log::info!(
                                        "Promoted {:?} to Tier A (subs: {}/{})",
                                        event.symbol_id,
                                        subscription_count,
                                        sub_limit
                                    );
                                }
                                Err("Not TickReady") | Err("Already in Tier A") => {}
                                Err(e) => {
                                    log::warn!("Promotion failed for {:?}: {}", event.symbol_id, e);
                                }
                            }
                        }
                    }

                    // Update per-symbol context engine
                    let ctx_eng = context_engines
                        .entry(event.symbol_id)
                        .or_insert_with(|| {
                            context_engine::ContextEngine::new(
                                event.symbol_id,
                                context_params.clone(),
                            )
                        });

                    // Update price window for churn detection
                    let ring_buffer = price_history.entry(event.symbol_id).or_insert_with(|| {
                        core_types::TimeRingBuffer::new(
                            1000, // capacity
                            (context_params.churn_window_minutes * 60 * 1_000_000) as u64,
                        )
                    });
                    ring_buffer.push(event.ts_src, tick.price);
                    let (min_p, max_p) = ring_buffer.min_max();
                    ctx_eng.update_price_window(max_p.unwrap_or(0.0), min_p.unwrap_or(0.0));

                    // Volume will be updated via Snapshot events — tick just triggers recompute
                    let daily_ctx = ctx_eng.compute_context();

                    // Update MTF engine price
                    let mtf_eng = mtf_engines
                        .entry(event.symbol_id)
                        .or_insert_with(|| {
                            mtf_engine::MtfEngine::new(
                                event.symbol_id,
                                mtf_engine::MtfParams::default(),
                            )
                        });
                    mtf_eng.update_price(tick.price);
                    let mtf_result = mtf_eng.evaluate();

                    // Push updated context to the watchlist for FastLoop consumption
                    watchlist.update_symbol_context(event.symbol_id, daily_ctx, mtf_result);
                }
                EventKind::Snapshot(snap) => {
                    watchlist.touch(event.symbol_id, event.ts_src);

                    // Update context engine volume from snapshot
                    if let Some(ctx_eng) = context_engines.get_mut(&event.symbol_id) {
                        ctx_eng.update_volume(
                            snap.volume,
                            snap.avg_volume_20d,
                            snap.volume
                                > (snap.avg_volume_20d as f64
                                    * context_params.volume_multiplier_3x) as u64,
                        );
                        ctx_eng.update_news(snap.has_news_today);
                    }
                }
                EventKind::Heartbeat => {
                    // Update regime engine from any available SPY metrics
                    // In production, SPY ATR and breadth come from Python bridge via Snapshot
                    let regime = regime_eng.state();
                    watchlist.update_regime(regime);

                    // Check monitor_only flag from regime
                    if regime == core_types::RegimeState::RiskOff {
                        if let Ok(mut guard) = risk_state_slow.lock() {
                            guard.monitor_only = true;
                        }
                    }
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

        let mut halted_symbols_fast: std::collections::HashMap<core_types::SymbolId, u64> = std::collections::HashMap::new();
        let mut disabled_symbols_session: std::collections::HashSet<core_types::SymbolId> = std::collections::HashSet::new();

        let session_guard = risk_engine::session::SessionGuard::new(config.session.pre_after_enabled);

        while let Some(event) = fast_loop_rx.recv().await {
            if let core_types::EventKind::Halt = event.kind {
                halted_symbols_fast.insert(event.symbol_id, event.ts_src);
            }
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

            // Pre-calculate sizing for validation
            let mut calculated_qty = 0;
            let mut calculated_stop_price = 0.0;
            let mut calculated_stop_dist = 0.0;

            if let core_types::EventKind::Tick(tick) = event.kind {
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

                // §16.1 StopDistance: max(Stop_Guards, Stop_ATR, MinStopDistance)
                let tick_size = 0.01_f64; // penny stocks
                let state_atr = tape_engine.symbol_states
                    .get(&event.symbol_id)
                    .map(|s| s.tape.atr_1m)
                    .unwrap_or(0.0);
                let spread_half = tape_engine.symbol_states
                    .get(&event.symbol_id)
                    .map(|s| s.tape.spread_cents / 100.0)
                    .unwrap_or(0.0);
                let stop_guards = (2.0 * tick_size).max(0.25 * spread_half * 2.0);
                let stop_atr = if state_atr > 0.0 {
                    state_atr * config.pricing.k_atr
                } else {
                    0.0
                };
                let min_stop = config.pricing.min_stop_abs_usd
                    .max(config.pricing.min_stop_pct * tick.price)
                    .max(0.8 * state_atr);
                let stop_dist = stop_guards.max(stop_atr).max(min_stop)
                    .max(position_sizer.config.min_stop_distance_cents);
                let stop_price = tick.price - stop_dist;

                // Cache for order execution
                calculated_stop_dist = stop_dist;
                calculated_stop_price = stop_price;

                let today_ordinal = (event.ts_src / 1_000_000 / 86400) as u32;
                let available_cash = if let Ok(guard) = tape_engine.risk_state.lock() {
                    guard.available_cash(today_ordinal, account_capital)
                } else {
                    0.0
                };

                calculated_qty = position_sizer.calculate_size(
                    account_capital,
                    tick.price,
                    stop_price,
                    daily_volume,
                    available_cash,
                );

                // §16.3 — reduce size after daily target
                if tape_engine.daily_target_reached {
                    calculated_qty = (calculated_qty / 2).max(1);
                }
                // After 10% daily gross (≈ $2500 on $25k account), reduce to 25%
                let daily_gross_10pct = account_capital * 0.10;
                if tape_engine.global_realized_pnl >= daily_gross_10pct && daily_gross_10pct > 0.0 {
                    calculated_qty = (calculated_qty / 4).max(1);
                }

                tape_engine.last_sizing_shares = calculated_qty;
            }

            // Session timing check — no entries outside allowed windows (§25)
            if let core_types::EventKind::Tick(_) = event.kind {
                if !session_guard.entry_allowed(event.ts_src) {
                    // Still process state updates but skip order generation
                    // We allow tape_engine.on_event to run for state tracking
                    // but must not send orders — set flag
                    let _ = tape_engine.on_event(&event); // state update only
                    continue;
                }
            }

            // §20.3: Emergency exit on LULD resume (halt > 5 min)
            if let core_types::EventKind::Tick(ref tick) = event.kind {
                if let Some(&halt_ts) = halted_symbols_fast.get(&event.symbol_id) {
                    let halt_duration_secs = (event.ts_src.saturating_sub(halt_ts)) / 1_000_000;
                    if halt_duration_secs >= 300 { // 5 minutes
                        // Emergency exit — Marketable Limit
                        let state_atr = tape_engine.symbol_states
                            .get(&event.symbol_id)
                            .map(|s| s.tape.atr_1m)
                            .unwrap_or(0.01);
                        let tick_size = 0.01_f64;
                        let exit_price = (tick.price - 0.25 * state_atr)
                            .max(tick.price - 2.0 * tick_size)
                            .max(0.01);
                        let pos = tape_engine.symbol_states
                            .get(&event.symbol_id)
                            .map(|s| s.position)
                            .unwrap_or(0);
                        if pos > 0 {
                            let req = core_types::OrderRequest {
                                symbol_id: event.symbol_id,
                                side: core_types::Side::Ask,
                                qty: pos as u32,
                                order_type: core_types::OrderType::Limit,
                                limit_price: Some(exit_price),
                                stop_price: None,
                                tif: core_types::TimeInForce::IOC,
                                idempotency_key: format!("EMERGENCY-EXIT-{}-{}", event.symbol_id.0, event.ts_src),
                                take_profit_price: None,
                                stop_loss_price: None,
                            };
                            let _ = order_tx.try_send(req);
                            log::warn!("EMERGENCY EXIT sent for {:?} after {}s halt", event.symbol_id, halt_duration_secs);
                        }
                        // Disable symbol for rest of session per §20.3
                        disabled_symbols_session.insert(event.symbol_id);
                    }
                    halted_symbols_fast.remove(&event.symbol_id);
                }
            }

            // Skip disabled symbols for the rest of the session
            if disabled_symbols_session.contains(&event.symbol_id) {
                continue;
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

                if decision_result.is_ok() && calculated_qty > 0 {
                    let request = OrderRequest {
                        symbol_id: event.symbol_id,
                        side: Side::Bid,
                        qty: calculated_qty,
                        order_type: OrderType::Limit,
                        limit_price: Some(tick.price),
                        stop_price: None,
                        tif: TimeInForce::IOC,
                        idempotency_key: format!("{}-{}", event.symbol_id.0, event.ts_src),
                        take_profit_price: Some(tick.price + 2.0 * calculated_stop_dist),
                        stop_loss_price: Some(calculated_stop_price),
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
                EventKind::Halt => {
                    // Route to OMS (manages open positions), Risk (Monitor Only), and Fast (state reset)
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    let _ = fast_tx.try_send(event.clone());
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
