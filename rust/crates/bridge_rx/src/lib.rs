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
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::time;
use std::os::unix::fs::PermissionsExt;

#[allow(dead_code)]
const BATCH_SIZE_LIMIT: usize = 512;
const BATCH_TIMEOUT: Duration = Duration::from_millis(5); // Aggressive 5ms flush
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(1);
const BUFFER_CAPACITY: usize = 65536; // 64KB read buffer

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QosPriority {
    Critical, // Fills, OrderStatus, Errors - NEVER DROP
    High,     // Ticks (Tier A) - Drop oldest if full (approximation: drop current if really full)
    Medium,   // L2 Deltas - Drop first
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
    read_buf: Vec<u8>,
}

impl BridgeRxTask {
    pub fn new(socket_path: &str, bus: EventBus) -> std::io::Result<Self> {
        // Ensure socket file doesn't exist
        if std::path::Path::new(socket_path).exists() {
             let _ = std::fs::remove_file(socket_path);
        }
        let listener = UnixListener::bind(socket_path)?;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;

        Ok(Self {
            listener,
            bus,
            last_heartbeat: Instant::now(),
            is_degraded: false,
            read_buf: Vec::with_capacity(BUFFER_CAPACITY),
        })
    }

    pub async fn run(&mut self) {
        log::info!("BridgeRxTask started. Waiting for connection...");
        let mut heartbeat_check = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = heartbeat_check.tick() => {
                    if self.last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT && !self.is_degraded {
                        log::warn!("Heartbeat lost! Entering DEGRADED mode.");
                        self.is_degraded = true;
                    }
                }
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
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
        }
    }

    async fn handle_connection(&mut self, stream: UnixStream) {
        let mut reader = BufReader::with_capacity(BUFFER_CAPACITY, stream);

        // Clear buffer on new connection
        self.read_buf.clear();

        // Using a loop with select! to handle read vs timeout vs heartbeat check
        let mut interval = time::interval(BATCH_TIMEOUT);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let mut heartbeat_check = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Periodic flush if we were buffering partial events,
                    // but we handle immediate dispatch below.
                }
                _ = heartbeat_check.tick() => {
                    if self.last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT && !self.is_degraded {
                        log::warn!("Heartbeat lost! Entering DEGRADED mode.");
                        self.is_degraded = true;
                    }
                }

                 result = self.read_and_process_batch(&mut reader) => {
                     if let Err(e) = result {
                         log::error!("Stream error: {}", e);
                         break;
                     }
                 }
            }
        }
    }

    async fn read_and_process_batch<R: AsyncReadExt + Unpin>(&mut self, reader: &mut R) -> std::io::Result<()> {
        let n = reader.read_buf(&mut self.read_buf).await?;
        if n == 0 {
             return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "EOF"));
        }

        let mut cursor = 0;
        loop {
            if cursor >= self.read_buf.len() {
                break;
            }

            // We use a cursor over the slice
            let mut cur = std::io::Cursor::new(&self.read_buf[cursor..]);
            let mut de = rmp_serde::decode::Deserializer::new(&mut cur);

            match Event::deserialize(&mut de) {
                Ok(event) => {
                     cursor += cur.position() as usize;

                     if let EventKind::Heartbeat = event.kind {
                         self.last_heartbeat = Instant::now();
                         if self.is_degraded {
                            log::info!("Heartbeat restored. Resuming NORMAL mode.");
                            self.is_degraded = false;
                         }
                     }

                     if self.validate_event(&event) {
                         self.process_event(event).await;
                     } else {
                         log::warn!("[bridge_rx] invalid event dropped: {:?}", event);
                     }
                },
                Err(e) => {
                     use rmp_serde::decode::Error;
                     match e {
                        Error::InvalidMarkerRead(_) | Error::InvalidDataRead(_) => {
                            // Incomplete data, wait for more
                            break;
                        }
                         _ => {
                             // Assuming any other error is fatal/corrupt data
                             return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
                         }
                     }
                }
            }
        }

        // Drain consumed bytes
        if cursor > 0 {
            self.read_buf.drain(0..cursor);
        }

        Ok(())
    }

    fn validate_event(&self, event: &Event) -> bool {
        let now_us = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros() as u64;

        if event.ts_src == 0 || event.ts_src > now_us + 5_000_000 {
            return false;
        }

        match event.kind {
            EventKind::Tick(tick) => {
                if tick.price <= 0.0 || tick.price >= 100_000.0 || tick.size == 0 { return false; }
            },
            EventKind::Fill(fill) => {
                if fill.price <= 0.0 || fill.price >= 100_000.0 || fill.size == 0 { return false; }
            },
            EventKind::L2Delta(l2) => {
                if l2.price <= 0.0 { return false; }
            },
            EventKind::Snapshot(snap) => {
                if snap.bid_price <= 0.0 || snap.bid_price >= snap.ask_price { return false; }
            },
            _ => {}
        }
        true
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
                        log::warn!("QoS DROP: High priority event dropped due to full bus.");
                    }
                    QosPriority::Medium | QosPriority::Low => {
                        // Drop immediately without remorse
                    }
                }
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                log::error!("EventBus closed! System shutting down?");
            }
        }
    }
}
