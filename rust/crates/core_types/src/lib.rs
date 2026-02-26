//! Core Types Crate.
//!
//! # Canonical Event Schema
//!
//! This module defines the canonical memory layout for the `Event` structure.
//! It is designed for zero-allocation processing in the FastLoop.
//!
//! ## Memory Alignment
//! - The `Event` struct uses `#[repr(C)]` to ensure predictable memory layout.
//! - The `EventKind` enum is `#[repr(u8)]` to keep the discriminant small.
//! - All timestamps are `u64` (microseconds).
//! - `SymbolId` is `u32`.
//! - `Seq` is `u64`.
//!
//! ## Serialization
//! - `serde` derives are included for MessagePack (dev/debug).
//! - In production, this struct maps directly to a FlatBuffers schema.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    Bid = 0,
    Ask = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum OrderStatus {
    Pending = 0,
    Live = 1,
    Filled = 2,
    Cancelled = 3,
    Rejected = 4,
}

/// Fixed-size reject reason for zero-allocation passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RejectReason {
    Blocklist = 0,
    CorporateActionBlock = 1,
    PriceRange = 2,
    Liquidity = 3,
    Regime = 4,
    DailyContext = 5,
    MtfVeto = 6,
    AntiChase = 7,
    GuardSpread = 8,
    GuardImbalance = 9,
    GuardStale = 10,
    GuardSlippage = 11,
    GuardL2Vacuum = 12,
    GuardFlicker = 13,
    TapeScoreLow = 14,
    NetNegative = 15,
    Exposure = 16,
    TapeReversal = 17,
    Unknown = 255,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CorporateAction {
    Allowed = 0,
    Watch = 1,
    Block = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityConfig {
    pub target_price_min: f64,
    pub target_price_max: f64,
    pub max_spread_pct: f64,
    pub min_avg_daily_volume: u64,
    pub min_addv_usd: f64,
}

impl Default for LiquidityConfig {
    fn default() -> Self {
        Self {
            target_price_min: 1.0,
            target_price_max: 20.0,
            max_spread_pct: 0.05, // 5%
            min_avg_daily_volume: 500_000,
            min_addv_usd: 1_000_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RegimeState {
    Normal = 0,
    Caution = 1,
    RiskOff = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DataQuality {
    Ok = 0,
    Degraded = 1,
    Halted = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ContextState {
    Undetermined = 0,
    Play = 1,
    NoPlay = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorMomentum {
    pub etf_symbol: String,
    pub change_pct: f64,
    pub is_favorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProfile {
    pub current_volume: u64,
    pub avg_20d_volume: u64,
    pub is_surge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyContext {
    pub symbol_id: SymbolId,
    pub state: ContextState,
    pub volume_profile: VolumeProfile,
    pub has_news: bool,
    pub sector_momentum: Option<SectorMomentum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtfAnalysis {
    pub weekly_trend_confirmed: bool,
    pub daily_resistance_cleared: bool,
    pub structure_4h_bullish: bool,
    pub pullback_15m_valid: bool,
    pub mtf_pass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Event {
    /// Source timestamp (exchange time) in microseconds.
    pub ts_src: u64,
    /// Receipt timestamp (system time when UDS packet arrived) in microseconds.
    pub ts_rx: u64,
    /// Processing start timestamp (FastLoop start) in microseconds.
    pub ts_proc: u64,
    /// Monotonic sequence number per symbol.
    pub seq: u64,
    /// Symbol identifier (mapped in Watchlist).
    pub symbol_id: SymbolId,
    /// The event payload.
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)] // Ensures the enum is laid out as a tagged union with C compatibility
pub enum EventKind {
    Tick(TickData),
    L2Delta(L2DeltaData),
    Snapshot(SnapshotData),
    Fill(FillData),
    OrderStatus(OrderStatusData),
    Reject(RejectData),
    Heartbeat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct TickData {
    pub price: f64,
    pub size: u32,
    pub flags: u8, // e.g., printable, past_limit
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct L2DeltaData {
    pub price: f64,
    pub size: u32,
    pub side: Side,
    pub level: u8, // 0 = best
    pub is_delete: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct SnapshotData {
    pub bid_price: f64,
    pub ask_price: f64,
    pub bid_size: u32,
    pub ask_size: u32,
    // Add more levels if needed, or keep it minimal for Tier B
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct FillData {
    pub order_id: u64,
    pub price: f64,
    pub size: u32,
    pub side: Side,
    pub liquidity: u8, // 0=add, 1=remove
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct OrderStatusData {
    pub order_id: u64,
    pub status: OrderStatus,
    pub filled_qty: u32,
    pub remaining_qty: u32,
    pub avg_fill_price: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct RejectData {
    pub order_id: u64,
    pub reason: RejectReason,
    pub code: u16, // Optional error code from exchange
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn print_layout() {
        println!("Layout of Event:");
        println!("Size: {} bytes", mem::size_of::<Event>());
        println!("Align: {} bytes", mem::align_of::<Event>());

        println!("Size of EventKind: {} bytes", mem::size_of::<EventKind>());
        println!("Align of EventKind: {} bytes", mem::align_of::<EventKind>());

        println!("Size of TickData: {} bytes", mem::size_of::<TickData>());
    }
}
