//! Event Bus Crate.
//!
//! Manages communication between different parts of the system using Tokio channels.
//! This ensures decoupled components.

use tokio::sync::mpsc;
use core_types::Event;

pub struct EventBus {
    pub tx: mpsc::Sender<Event>,
    pub rx: mpsc::Receiver<Event>,
}

impl EventBus {
    pub fn new(buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        Self { tx, rx }
    }
}
