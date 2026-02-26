//! Context Engine Crate.
//!
//! Computes the "Daily Context" for a symbol.
//! Determines if a symbol is in "Play" based on Volume, News, and Sector Momentum.

use core_types::{ContextState, DailyContext, SectorMomentum, SymbolId, VolumeProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextParams {
    pub volume_multiplier_threshold: f64, // e.g. 2.0 for 2x Avg20D
    pub min_sector_momentum_pct: f64,     // e.g. 0.5%
}

impl Default for ContextParams {
    fn default() -> Self {
        Self {
            volume_multiplier_threshold: 2.0,
            min_sector_momentum_pct: 0.5,
        }
    }
}

pub struct ContextEngine {
    params: ContextParams,
    symbol_id: SymbolId,

    // Inputs
    current_volume: u64,
    avg_20d_volume: u64,
    has_news: bool,
    sector_momentum: Option<SectorMomentum>,
    is_volume_surge: bool,
}

impl ContextEngine {
    pub fn new(symbol_id: SymbolId, params: ContextParams) -> Self {
        Self {
            params,
            symbol_id,
            current_volume: 0,
            avg_20d_volume: 0,
            has_news: false,
            sector_momentum: None,
            is_volume_surge: false,
        }
    }

    pub fn update_volume(&mut self, current: u64, avg_20d: u64, is_surge: bool) {
        self.current_volume = current;
        self.avg_20d_volume = avg_20d;
        self.is_volume_surge = is_surge;
    }

    pub fn update_news(&mut self, has_news: bool) {
        self.has_news = has_news;
    }

    pub fn update_sector_momentum(&mut self, momentum: SectorMomentum) {
        self.sector_momentum = Some(momentum);
    }

    pub fn compute_context(&self) -> DailyContext {
        let state = self.evaluate_state();

        DailyContext {
            symbol_id: self.symbol_id,
            state,
            volume_profile: VolumeProfile {
                current_volume: self.current_volume,
                avg_20d_volume: self.avg_20d_volume,
                is_surge: self.is_volume_surge,
            },
            has_news: self.has_news,
            sector_momentum: self.sector_momentum.clone(),
        }
    }

    fn evaluate_state(&self) -> ContextState {
        // 1. Check Data Availability
        if self.avg_20d_volume == 0 {
            // Cannot determine context without baseline volume
            return ContextState::Undetermined;
        }

        // 2. Volume Rule: Today >= 2x Avg20D OR Volume Surge
        let vol_ratio = self.current_volume as f64 / self.avg_20d_volume as f64;
        let volume_condition =
            vol_ratio >= self.params.volume_multiplier_threshold || self.is_volume_surge;

        if !volume_condition {
            return ContextState::NoPlay;
        }

        // 3. News Event Trigger
        if !self.has_news {
            // Requirement: "News event trigger"
            // If no news, is it NoPlay? Usually high volume without news is suspicious or just technical breakout.
            // Strict interpretation: Must have news.
            return ContextState::NoPlay;
        }

        // 4. Sector ETF Momentum
        if let Some(ref sector) = self.sector_momentum {
            // Check if sector is supportive?
            // "Sector ETF momentum" listed as input. Logic usually: if sector is crashing, maybe don't go long?
            // Or maybe we just need significant momentum (up or down).
            // Let's assume we want supportive momentum (is_favorable flag handles direction).
            if !sector.is_favorable {
                return ContextState::NoPlay;
            }
        } else {
            // Missing sector data
            return ContextState::Undetermined;
        }

        ContextState::Play
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_computation() {
        let params = ContextParams::default();
        let symbol = SymbolId(1);
        let mut engine = ContextEngine::new(symbol, params);

        // 1. Initial State - Undetermined due to no volume
        let ctx = engine.compute_context();
        assert_eq!(ctx.state, ContextState::Undetermined);

        // 2. Set Volume - Low
        engine.update_volume(100_000, 1_000_000, false);

        // Volume condition failed (0.1x). The logic:
        // vol_condition = false -> returns NoPlay immediately.
        // It does NOT check sector data if volume is low.
        // So we expect NoPlay here, NOT Undetermined.
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);

        // Set Sector Data
        engine.update_sector_momentum(SectorMomentum {
            etf_symbol: "XLF".to_string(),
            change_pct: 1.0,
            is_favorable: true,
        });

        // Now we have volume (low) and sector. Should be NoPlay.
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);

        // 3. High Volume, No News -> NoPlay
        engine.update_volume(2_500_000, 1_000_000, false); // 2.5x
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);

        // 4. High Volume + News + Sector -> Play
        engine.update_news(true);
        assert_eq!(engine.compute_context().state, ContextState::Play);

        // 5. Volume Surge + News + Sector -> Play
        engine.update_volume(500_000, 1_000_000, true); // Low ratio (0.5x) but SURGE=true
        assert_eq!(engine.compute_context().state, ContextState::Play);

        // 6. Unfavorable Sector -> NoPlay
        engine.update_sector_momentum(SectorMomentum {
            etf_symbol: "XLF".to_string(),
            change_pct: -1.0,
            is_favorable: false,
        });
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);
    }
}
