//! App Runtime Crate (Orchestration).
//!
//! Wires together all the components and spawns the task graph.
//!
//! # Topology
//!
//! ```text
//! BridgeRx --> [DataRouter] --+--> FastLoop (Ticks/L2)
//!                             |--> SlowLoop (Ticks/Snapshots)
//!                             |--> OMS (Fills/Status)
//!                             |--> Risk (Monitor)
//!                             +--> Metrics (Log)
//!
//! FastLoop --(OrderRequest)--> OMS
//! SlowLoop --(ArcSwap Watchlist)--> FastLoop
//! ```

use core_types::{Event, EventKind};
use event_bus::{ChannelConfig, SystemChannels, EventBus};
use tokio::task;
use tokio::sync::mpsc;
use std::sync::Arc;
use arc_swap::ArcSwap;
use watchlist_engine::WatchlistSnapshot;
use std::path::Path;
use metrics_observability::{DecisionLog, LatencyTracker, log_decision, SLA_LIMIT_MICROS};

const RISK_STATE_PATH: &str = "risk_state.json";

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = ChannelConfig::default();
    let channels = SystemChannels::new(config);

    // Dedicated channel for Decision Logs (FastLoop -> Metrics)
    let (decision_tx, mut decision_rx) = mpsc::channel::<DecisionLog>(8192);

    // Shared State: Watchlist Snapshot
    let watchlist_snapshot = Arc::new(ArcSwap::new(Arc::new(WatchlistSnapshot::default())));

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
                    // We care about Source -> Decision latency
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
    let _oms_tx = channels.oms_market_tx.clone(); // If OMS needs to self-send
    task::spawn(async move {
        log::info!("OMS Task started");
        while let Some(_event) = oms_rx.recv().await {
            // oms.on_market_data(event);
        }
    });

    // 3. Spawn Risk Task
    let mut risk_rx = channels.risk_rx;
    task::spawn(async move {
        log::info!("Risk Task started");
        while let Some(_event) = risk_rx.recv().await {
            // risk.check(event);
        }
    });

    // 4. Spawn SlowLoop Task
    let mut slow_loop_rx = channels.slow_loop_rx;
    let slow_snapshot_writer = watchlist_snapshot.clone();
    task::spawn(async move {
        log::info!("SlowLoop Task started");
        let mut _watchlist = watchlist_engine::Watchlist::new();
        while let Some(_event) = slow_loop_rx.recv().await {
            // watchlist.update(event);
            // slow_snapshot_writer.store(Arc::new(watchlist.snapshot()));
        }
    });

    // 5. Spawn FastLoop Task
    let mut fast_loop_rx = channels.fast_loop_rx;
    let fast_snapshot_reader = watchlist_snapshot.clone();
    // FastLoop needs to send orders to OMS.
    // We should probably give it `oms_market_tx` or a dedicated `oms_order_tx`.
    // For now, let's just log.
    task::spawn(async move {
        log::info!("FastLoop Task started");

        // Load Risk State from file or create new
        let risk_state = if Path::new(RISK_STATE_PATH).exists() {
            log::info!("Loading persistent risk state from {}", RISK_STATE_PATH);
            match risk_engine::RiskState::load_from_file(Path::new(RISK_STATE_PATH)) {
                Ok(state) => state,
                Err(e) => {
                    log::error!("Failed to load risk state: {}. Starting fresh.", e);
                    risk_engine::RiskState::new(100.0, core_types::LiquidityConfig::default())
                }
            }
        } else {
            risk_engine::RiskState::new(100.0, core_types::LiquidityConfig::default())
        };

        let guard_config = risk_engine::guards::GuardConfig::default();
        let mut tape_engine = tape_engine::TapeEngine::new(risk_state, guard_config);

        while let Some(event) = fast_loop_rx.recv().await {
            let ts_proc_start = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;

            // Zero-allocation path
            // Read snapshot without locking
            let _snapshot = fast_snapshot_reader.load();

            // Check if we should terminate immediately (Risk-Off)
            if tape_engine.should_terminate() {
                log::error!("RISK LIMIT BREACHED! HALTING TRADING IMMEDIATELY.");
                // Break the loop to stop processing new events.
                // In a real system, we would also trigger a "Close All" command to OMS.
                break;
            }

            let decision_result = tape_engine.on_event(&event);

            let ts_decision_end = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;

            // Emit Decision Log
            if let core_types::EventKind::Tick(tick) = event.kind {
                 let log = DecisionLog {
                    symbol_id: event.symbol_id,
                    timestamp: ts_decision_end,
                    action: if decision_result.is_ok() { "Enter".to_string() } else { "Reject".to_string() },
                    reject_reason: decision_result.err(),
                    latency_src_rx: event.ts_rx.saturating_sub(event.ts_src),
                    latency_rx_proc: ts_proc_start.saturating_sub(event.ts_rx),
                    latency_proc_decision: ts_decision_end.saturating_sub(ts_proc_start),
                    price: tick.price,
                    tape_score: 0.0, // TapeEngine needs to expose score if we want it here
                };
                let _ = decision_tx.try_send(log);
            }

            if let Err(_reason) = decision_result {
                // Reject logic handled by DecisionLog
            } else {
                // Signal entry
                // send_order(...)
            }

            // Persist RiskState on Fills (critical updates)
            // Ideally this is async or offloaded, but for now we do it inline or check event type
            if let core_types::EventKind::Fill(_) = event.kind {
                if let Err(e) = tape_engine.risk_state.save_to_file(Path::new(RISK_STATE_PATH)) {
                    log::error!("Failed to persist risk state: {}", e);
                }
            }
        }
    });

    // 6. Spawn DataRouter Task
    // Routes events from BridgeRx to downstream consumers.
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
                    // Send fills to FastLoop for PnL tracking and Risk updates
                    if let EventKind::Fill(_) = event.kind {
                        let _ = fast_tx.try_send(event.clone());
                    }
                }
                EventKind::Heartbeat => {
                     let _ = risk_tx.try_send(event.clone());
                }
                EventKind::Reconnect => {
                    log::warn!("Bridge Reconnected - Triggering Sync");
                    // Route to all components that need reset
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    let _ = fast_tx.try_send(event.clone());
                }
                EventKind::StateSync(_) => {
                    log::info!("Received StateSync");
                    // Sync events go to OMS and Risk primarily
                    let _ = oms_tx.try_send(event.clone());
                    let _ = risk_tx.try_send(event.clone());
                    // Also FastLoop needs to know about positions for PnL
                    let _ = fast_tx.try_send(event.clone());
                }
            }
        }
    });

    // 7. Spawn BridgeRx Task
    let bridge_tx = channels.bridge_tx;
    // Construct a specialized EventBus for BridgeRx (it only needs TX)
    // Creating a dummy RX channel just to satisfy the struct
    let (_dummy_tx, dummy_rx) = mpsc::channel(1);

    let bridge_bus = EventBus {
        tx: bridge_tx,
        rx: dummy_rx,
    };

    let mut bridge_task = bridge_rx::BridgeRxTask::new("/tmp/rps_uds.sock", bridge_bus)?;
    task::spawn(async move {
        bridge_task.run().await;
    });

    // Keep main alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}
