//! Core Types Crate.
//!
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
