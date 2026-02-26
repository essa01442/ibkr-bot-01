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

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = ChannelConfig::default();
    let channels = SystemChannels::new(config);

    // Shared State: Watchlist Snapshot
    let watchlist_snapshot = Arc::new(ArcSwap::new(Arc::new(WatchlistSnapshot::default())));

    // 1. Spawn Metrics Task
    let mut metrics_rx = channels.metrics_rx;
    task::spawn(async move {
        log::info!("Metrics Task started");
        while let Some(_event) = metrics_rx.recv().await {
            // log_decision(&event);
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
        // Initialize Risk and Tape Engine with MaxDailyLoss = 100.0
        let risk_state = risk_engine::RiskState::new(
            100.0,
            core_types::LiquidityConfig::default()
        );
        let guard_config = risk_engine::guards::GuardConfig::default();
        let mut tape_engine = tape_engine::TapeEngine::new(risk_state, guard_config);

        while let Some(event) = fast_loop_rx.recv().await {
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

            if let Err(reason) = tape_engine.on_event(&event) {
                // Reject logic
                 log::debug!("FastLoop reject: {:?}", reason);
            } else {
                // Signal entry
                // send_order(...)
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
