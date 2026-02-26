//! Event Bus Crate.
//!
//! Manages communication channels between different parts of the system.
//! Defines the topology of the application's task graph.

use core_types::Event;
use tokio::sync::mpsc;

/// Configuration for channel sizes.
pub struct ChannelConfig {
    pub bridge_rx_size: usize,
    pub fast_loop_size: usize,
    pub slow_loop_size: usize,
    pub oms_size: usize,
    pub metrics_size: usize,
    pub risk_size: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            bridge_rx_size: 4096,
            fast_loop_size: 16384, // Larger buffer for ticks
            slow_loop_size: 8192,
            oms_size: 1024,
            metrics_size: 8192,
            risk_size: 1024,
        }
    }
}

/// The central hub for channels.
/// In practice, we might pass specific Senders/Receivers to tasks rather than this whole struct,
/// but this struct helps organize the creation.
pub struct SystemChannels {
    // BridgeRx -> DataRouter
    pub bridge_tx: mpsc::Sender<Event>,
    pub bridge_rx: mpsc::Receiver<Event>,

    // DataRouter -> FastLoop (Ticks, L2)
    pub fast_loop_tx: mpsc::Sender<Event>,
    pub fast_loop_rx: mpsc::Receiver<Event>,

    // DataRouter -> SlowLoop (Snapshots, Ticks for Agg)
    pub slow_loop_tx: mpsc::Sender<Event>,
    pub slow_loop_rx: mpsc::Receiver<Event>,

    // DataRouter -> OMS (Fills, OrderStatus) + FastLoop -> OMS (Orders)
    // Note: OMS might need a different message type for Orders vs MarketData events.
    // For now, we assume Event covers market data, but we need a command channel for orders.
    // We'll stick to Event for market data routing here.
    pub oms_market_tx: mpsc::Sender<Event>,
    pub oms_market_rx: mpsc::Receiver<Event>,

    // DataRouter -> Metrics
    pub metrics_tx: mpsc::Sender<Event>,
    pub metrics_rx: mpsc::Receiver<Event>,

    // DataRouter -> Risk (if Risk needs direct feed)
    pub risk_tx: mpsc::Sender<Event>,
    pub risk_rx: mpsc::Receiver<Event>,
}

impl SystemChannels {
    pub fn new(config: ChannelConfig) -> Self {
        let (bridge_tx, bridge_rx) = mpsc::channel(config.bridge_rx_size);
        let (fast_loop_tx, fast_loop_rx) = mpsc::channel(config.fast_loop_size);
        let (slow_loop_tx, slow_loop_rx) = mpsc::channel(config.slow_loop_size);
        let (oms_market_tx, oms_market_rx) = mpsc::channel(config.oms_size);
        let (metrics_tx, metrics_rx) = mpsc::channel(config.metrics_size);
        let (risk_tx, risk_rx) = mpsc::channel(config.risk_size);

        Self {
            bridge_tx,
            bridge_rx,
            fast_loop_tx,
            fast_loop_rx,
            slow_loop_tx,
            slow_loop_rx,
            oms_market_tx,
            oms_market_rx,
            metrics_tx,
            metrics_rx,
            risk_tx,
            risk_rx,
        }
    }
}

// Temporary compatibility struct for BridgeRxTask until it's refactored to take just Sender
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
