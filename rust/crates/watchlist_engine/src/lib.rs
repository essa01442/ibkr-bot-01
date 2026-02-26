//! Watchlist Engine Crate (Slow Loop).
//!
//! Manages the tiered watchlist system (Tier A, Tier B, Tier C).
//! Handles pacing, upgrades, downgrades, and slow-moving context analysis (MTF, Correlation).
//!
//! This logic runs outside the FastLoop.

use core_types::SymbolId;
use std::collections::HashMap;

pub struct Watchlist {
    pub tier_a: HashMap<SymbolId, TierData>,
    pub tier_b: HashMap<SymbolId, TierData>,
}

pub struct TierData {
    // metadata for slow analysis
}

impl Watchlist {
    pub fn new() -> Self {
        Self {
            tier_a: HashMap::new(),
            tier_b: HashMap::new(),
        }
    }
}
