//! Watchlist Engine Crate (Slow Loop).
//!
//! Manages the tiered watchlist system (Tier A, Tier B, Tier C).
//! Handles pacing, upgrades, downgrades, and slow-moving context analysis (MTF, Correlation).

use core_types::SymbolId;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistSnapshot {
    pub tier_a_count: usize,
    // Add more fields as needed for FastLoop to read
}

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

    pub fn snapshot(&self) -> WatchlistSnapshot {
        WatchlistSnapshot {
            tier_a_count: self.tier_a.len(),
        }
    }
}
