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
//! Contains shared domain types and error definitions.
//! This crate must not depend on any other crate in the workspace.
//! It serves as the common vocabulary for the system.

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RejectReason {
    Blocklist,
    CorporateActionBlock,
    PriceRange,
    Liquidity,
    Regime,
    DailyContext,
    MtfVeto,
    AntiChase,
    GuardSpread,
    GuardImbalance,
    GuardStale,
    GuardSlippage,
    GuardL2Vacuum,
    GuardFlicker,
    TapeScoreLow,
    NetNegative,
    Exposure,
    TapeReversal,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts_src: u64,
    pub ts_rx: u64,
    pub symbol_id: SymbolId,
    pub seq: u64,
    // data payload would be here
}
