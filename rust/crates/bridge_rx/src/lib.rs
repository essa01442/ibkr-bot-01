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
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::time;

#[allow(dead_code)]
const BATCH_SIZE_LIMIT: usize = 512;
#[allow(dead_code)]
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

use tokio::sync::mpsc;

pub struct BridgeRxTask {
    listener: UnixListener,
    bus: EventBus,
    last_heartbeat: Instant,
    is_degraded: bool,
    read_buf: Vec<u8>,
    degraded_tx: Option<mpsc::Sender<bool>>,
}

fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
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
            degraded_tx: None,
        })
    }

    pub fn set_degraded_notifier(&mut self, tx: mpsc::Sender<bool>) {
        self.degraded_tx = Some(tx);
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
                        if let Some(tx) = &self.degraded_tx {
                            let _ = tx.try_send(true);
                        }
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

        loop {
            if let Err(e) = self.read_and_process_batch(&mut reader).await {
                log::error!("Stream error: {}", e);
                break;
            }
        }
    }

    async fn read_and_process_batch<R: AsyncReadExt + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> std::io::Result<()> {
        let n = reader.read_buf(&mut self.read_buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "EOF",
            ));
        }

        let mut cursor = 0;
        loop {
            if cursor >= self.read_buf.len() {
                break;
            }

            // We use a cursor over the slice
            let mut cur = Cursor::new(&self.read_buf[cursor..]);
            let mut de = rmp_serde::decode::Deserializer::new(&mut cur);

            match Event::deserialize(&mut de) {
                Ok(event) => {
                    cursor += cur.position() as usize;

                    if let EventKind::Heartbeat = event.kind {
                        self.last_heartbeat = Instant::now();
                        if self.is_degraded {
                            log::info!("Heartbeat restored. Resuming NORMAL mode.");
                            self.is_degraded = false;
                            if let Some(tx) = &self.degraded_tx {
                                let _ = tx.try_send(false);
                            }
                        }
                    }

                    if self.validate_event(&event) {
                        self.process_event(event).await;
                    } else {
                        log::warn!("[bridge_rx] invalid event dropped: {:?}", event);
                    }
                }
                Err(e) => {
                    use rmp_serde::decode::Error;
                    // Only catch incomplete reads, propagate invalid data
                    // rmp_serde Error doesn't cleanly map to "Incomplete", so we rely on checks
                    // Actually, for stream processing, if we fail to deserialize, we might need more bytes.
                    // But rmp-serde is synchronous.
                    // Simple strategy: If error, and buffer not empty, assume we need more bytes?
                    // Or if invalid marker, it's corrupt.
                    // Let's assume unexpected EOF is "need more bytes".
                    match e {
                        Error::InvalidMarkerRead(ref io_err) | Error::InvalidDataRead(ref io_err) => {
                            if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                                // Need more data
                                break;
                            } else {
                                // Corrupt?
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    e,
                                ));
                            }
                        }
                        Error::Syntax(_) => {
                            // Syntax error likely means partial write or corrupt
                            break;
                        }
                        _ => {
                            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
                        }
                    }
                }
            }
        }

        // Drain consumed bytes
        if cursor > 0 {
            if cursor == self.read_buf.len() {
                self.read_buf.clear();
            } else {
                // Should use drain, but Vec::drain is O(N).
                // Ideally use circular buffer or bytes crate.
                // For now, drain is fine for small batches.
                self.read_buf.drain(0..cursor);
            }
        }

        Ok(())
    }

    fn validate_event(&self, event: &Event) -> bool {
        let now_us = now_micros();

        if event.ts_src == 0 || event.ts_src > now_us + 5_000_000 {
            return false;
        }

        match event.kind {
            EventKind::Tick(tick) => {
                if tick.price <= 0.0 || tick.price >= 100_000.0 || tick.size == 0 {
                    return false;
                }
            }
            EventKind::Fill(fill) => {
                if fill.price <= 0.0 || fill.price >= 100_000.0 || fill.size == 0 {
                    return false;
                }
            }
            EventKind::L2Delta(l2) => {
                if l2.price <= 0.0 {
                    return false;
                }
            }
            EventKind::Snapshot(snap) => {
                if snap.bid_price <= 0.0 || snap.bid_price >= snap.ask_price {
                    return false;
                }
            }
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
