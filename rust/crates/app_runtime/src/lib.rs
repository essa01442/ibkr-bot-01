//! App Runtime Crate (Orchestration).
//!
//! Wires together all the components:
//! 1. Starts the EventBus.
//! 2. Spawns the Bridge Receiver (BridgeRx).
//! 3. Spawns the SlowLoop (Watchlist management).
//! 4. Spawns the FastLoop (Tape reading).
//! 5. Spawns the OMS.
//! 6. Spawns the Metrics logger.

use tokio::task;
use event_bus::EventBus;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let bus = EventBus::new(1024);

    // 1. Spawn Bridge Rx
    // task::spawn(bridge_rx::start_listener("/tmp/rps_uds.sock", &bus));

    // 2. Spawn FastLoop (Tape Engine)
    // task::spawn(async move {
    //     let mut tape_engine = TapeEngine::new();
    //     while let Some(event) = bus.rx.recv().await {
    //         tape_engine.on_event(&event);
    //     }
    // });

    // 3. Spawn SlowLoop (Watchlist)

    // 4. Spawn Metrics

    Ok(())
}
