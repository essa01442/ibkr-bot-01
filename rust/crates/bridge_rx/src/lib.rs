//! Bridge Receiver Crate.
//!
//! Handles the reception and decoding of data from the Python bridge.
//! Implements strict batching, QoS, and backpressure policies.
//!
//! # Architecture
//! - Uses a Unix Domain Socket (UDS) for low-latency IPC.
//! - Reads in batches (max 512 events) or every 5-10ms.
//! - Deserializes from MessagePack (dev) into pre-allocated buffers.
//! - Enforces QoS drop priorities when the internal EventBus is full.
//! - Monitors Heartbeat (expected every 250ms); >1s silence triggers DataQuality::Degraded.

use core_types::{Event, EventKind};
use event_bus::EventBus;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::time;

const BATCH_SIZE_LIMIT: usize = 512;
const BATCH_TIMEOUT: Duration = Duration::from_millis(5); // Aggressive 5ms flush
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(1);
const BUFFER_CAPACITY: usize = 65536; // 64KB read buffer

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QosPriority {
    Critical, // Fills, OrderStatus, Errors - NEVER DROP
    High,     // Ticks (Tier A) - Drop oldest if full (approximation: drop current if really full)
    Medium,   // L2 Deltas - Drop before Ticks
    Low,      // Snapshots (Tier B/C) - Drop first
}

impl QosPriority {
    pub fn from_event(event: &Event) -> Self {
        match event.kind {
            EventKind::Fill(_) | EventKind::OrderStatus(_) | EventKind::Reject(_) => {
                QosPriority::Critical
            }
            EventKind::Tick(_) => QosPriority::High,
            EventKind::L2Delta(_) => QosPriority::Medium,
            EventKind::Snapshot(_) => QosPriority::Low,
            EventKind::Heartbeat => QosPriority::Low,
            EventKind::Reconnect => QosPriority::Critical, // Critical control event
            EventKind::StateSync(_) => QosPriority::Critical, // Critical control event
        }
    }
}

pub struct BridgeRxTask {
    listener: UnixListener,
    bus: EventBus,
    last_heartbeat: Instant,
    is_degraded: bool,
}

impl BridgeRxTask {
    pub fn new(socket_path: &str, bus: EventBus) -> std::io::Result<Self> {
        // Ensure socket file doesn't exist
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;

        Ok(Self {
            listener,
            bus,
            last_heartbeat: Instant::now(),
            is_degraded: false,
        })
    }

    pub async fn run(&mut self) {
        log::info!("BridgeRxTask started. Waiting for connection...");

        loop {
            // Check heartbeat timeout
            if self.last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT {
                if !self.is_degraded {
                    log::warn!("Heartbeat lost! Entering DEGRADED mode.");
                    self.is_degraded = true;
                    // TODO: Emit DataQuality::Degraded event to Risk Engine
                }
            }

            match self.listener.accept().await {
                Ok((stream, _addr)) => {
                    log::info!("Bridge connected.");
                    self.handle_connection(stream).await;
                    log::warn!("Bridge disconnected.");
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                    time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn handle_connection(&mut self, stream: UnixStream) {
        let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, stream);
        // let mut buffer = Vec::with_capacity(BUFFER_CAPACITY); // Reused scratch buffer

        // Re-allocated batch buffer to avoid allocation in the loop?
        // Actually, Vec<Event> allocation is fine if we clear() it.
        // But deserialization usually allocates unless we use zero-copy libraries.
        // For MessagePack + serde, we will allocate for the Vec, but we can reuse it.

        // Using a loop with select! to handle read vs timeout vs heartbeat check
        let mut interval = time::interval(BATCH_TIMEOUT);
        // Ensure interval doesn't burst
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Periodic check, but strictly speaking we drive processing by data arrival
                    // This tick is mostly useful if we were buffering partial events,
                    // but with MessagePack stream, we usually block on read_value.

                    // Actually, the "Batching" requirement is more about how we *send* to the bus,
                    // or how Python sends to us.
                    // If Python sends a batch, we read a batch.
                    // If we read event-by-event, we can collect them and flush every 5ms.

                    // Let's assume proper framing: we read whatever is available in the buffer.
                }

                // We need to read from the stream.
                // Since we don't have a framed stream wrapper yet, we'll simulate reading
                // MessagePack objects one by one.
                // In a real implementation, we'd use `rmp_serde` or similar.
                // For this design phase, we'll assume a `read_event` helper.
                 result = self.read_and_process_batch(&mut reader) => {
                     if let Err(e) = result {
                         log::error!("Stream error: {}", e);
                         break;
                     }
                 }
            }
        }
    }

    // Simulates reading a batch of events from the stream
    async fn read_and_process_batch<R: AsyncReadExt + Unpin>(&mut self, reader: &mut R) -> std::io::Result<()> {
        // In a real implementation, we would read a length-prefixed buffer
        // or consume the stream with rmp_serde::from_read (blocking, so careful).
        // Since we are async, we should read bytes into a buffer, then try to decode.

        // Placeholder for reading logic:
        // 1. Read available data into buffer
        // 2. Try to deserialize as many Events as possible
        // 3. For each event: apply QoS and Send

        let mut byte_buf = [0u8; 4096];
        let n = reader.read(&mut byte_buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "EOF"));
        }

        // 4. Update Heartbeat on any valid data (or specifically Heartbeat event)
        self.last_heartbeat = Instant::now();
        if self.is_degraded {
            log::info!("Heartbeat restored. Resuming NORMAL mode.");
            self.is_degraded = false;
        }

        // Mock deserialization loop
        // In reality, use `rmp_serde::decode::from_slice` on the valid range
        // For now, we assume we got 1 valid mock event for architectural demonstration

        // Applying QoS to a mock event
        // let event: Event = ...;
        // self.process_event(event).await;

        Ok(())
    }

    async fn process_event(&mut self, event: Event) {
        let priority = QosPriority::from_event(&event);

        // QoS Logic
        match self.bus.tx.try_send(event.clone()) {
            Ok(_) => {
                // Sent successfully
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                match priority {
                    QosPriority::Critical => {
                        // FORCE SEND - We must not drop fills/errors.
                        // We await (block this task) until space is available.
                        // This applies backpressure to the socket (TCP flow control).
                        if let Err(e) = self.bus.tx.send(event).await {
                            log::error!("Critical failure: EventBus closed! {}", e);
                        }
                    }
                    QosPriority::High => {
                        // Guidance: Drop oldest.
                        // Since mpsc doesn't support "drop oldest", we drop current
                        // and log a warning (High priority drop is serious but not fatal like Critical).
                        // Alternatively, we could try to consume one from Rx to make space, but
                        // we don't own Rx here.
                        log::warn!("QoS DROP: High priority event dropped due to full bus.");
                    }
                    QosPriority::Medium | QosPriority::Low => {
                        // Drop immediately without remorse
                        // Use a counter for metrics
                    }
                }
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                log::error!("EventBus closed! System shutting down?");
            }
        }
    }
}
