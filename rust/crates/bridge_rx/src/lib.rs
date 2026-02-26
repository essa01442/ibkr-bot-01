//! Bridge Receiver Crate.
//!
//! Handles the reception and decoding of data from the Python bridge.
//! Communicates with the rest of the system via the Event Bus.

use tokio::net::UnixListener;
use event_bus::EventBus;

pub async fn start_listener(socket_path: &str, _bus: &EventBus) -> std::io::Result<()> {
    let _listener = UnixListener::bind(socket_path)?;
    // loop { accept, read, decode, send to bus }
    Ok(())
}
