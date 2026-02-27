//! Context Engine Crate.
//!
//! Computes the "Daily Context" for a symbol.
//! Determines if a symbol is in "Play" based on Volume, News, and Sector Momentum.

use core_types::{ContextState, DailyContext, SectorMomentum, SymbolId, VolumeProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextParams {
    pub volume_multiplier_2x: f64,   // 2.0
    pub volume_multiplier_3x: f64,   // 3.0
    pub sector_momentum_min_pct: f64,// 2.0 (percent)
    pub churn_max_move_pct: f64,     // 0.01 (1%)
    pub churn_window_minutes: u64,   // 10
}

impl Default for ContextParams {
    fn default() -> Self {
        Self {
            volume_multiplier_2x: 2.0,
            volume_multiplier_3x: 3.0,
            sector_momentum_min_pct: 2.0,
            churn_max_move_pct: 0.01,
            churn_window_minutes: 10,
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

    // Churning detection
    pub price_high_window: f64,
    pub price_low_window: f64,
    pub is_churning: bool,
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
            price_high_window: 0.0,
            price_low_window: 0.0,
            is_churning: false,
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

    /// Called by SlowLoop every tick/minute with window high/low prices.
    /// Sets is_churning=true if High–Low < churn_max_move_pct for churn_window_minutes.
    pub fn update_price_window(&mut self, window_high: f64, window_low: f64) {
        self.price_high_window = window_high;
        self.price_low_window = window_low;
        if window_high > 0.0 {
            let range_pct = (window_high - window_low) / window_high;
            self.is_churning = range_pct < self.params.churn_max_move_pct;
        }
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
        // Churning → reject always (§9.1)
        if self.is_churning {
            return ContextState::NoPlay;
        }

        if self.avg_20d_volume == 0 {
            return ContextState::Undetermined;
        }

        let vol_ratio = self.current_volume as f64 / self.avg_20d_volume as f64;

        // §9.2: Volume ≥ 3× in first 2 hours = strong trigger alone
        // Note: 'in first 2 hours' condition is likely implicitly handled by the timing of this check
        // or is_volume_surge flag if that flag captures the time window.
        // The prompt says: "§9.2: Volume ≥ 3× in first 2 hours = strong trigger alone"
        // But the implementation requested in the prompt is:
        // if self.is_volume_surge && vol_ratio >= self.params.volume_multiplier_3x
        // Let's stick to the prompt's explicit code for this check.
        if self.is_volume_surge && vol_ratio >= self.params.volume_multiplier_3x {
            return ContextState::Play;
        }

        // §9.2: Volume < 2× = no play
        if vol_ratio < self.params.volume_multiplier_2x {
            return ContextState::NoPlay;
        }

        // Volume is between 2× and 3×: needs at least ONE qualifier (§9.2)
        let has_sector = self.sector_momentum.as_ref()
            .map(|s| s.is_favorable && s.change_pct.abs() >= self.params.sector_momentum_min_pct)
            .unwrap_or(false);

        if self.has_news || has_sector {
            return ContextState::Play;
        }

        // Volume ≥ 2× but no qualifier → Undetermined → reject (§9.2)
        ContextState::Undetermined
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

        // 2. Set Volume - Low (< 2x)
        engine.update_volume(1_500_000, 1_000_000, false); // 1.5x
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);

        // 3. Set Volume - Medium (2.5x) but no qualifiers
        engine.update_volume(2_500_000, 1_000_000, false); // 2.5x
        assert_eq!(engine.compute_context().state, ContextState::Undetermined);

        // 4. Medium Volume + News -> Play
        engine.update_news(true);
        assert_eq!(engine.compute_context().state, ContextState::Play);
        engine.update_news(false); // Reset

        // 5. Medium Volume + Sector -> Play
        engine.update_sector_momentum(SectorMomentum {
            etf_symbol: "XLF".to_string(),
            change_pct: 2.5, // > 2.0%
            is_favorable: true,
        });
        assert_eq!(engine.compute_context().state, ContextState::Play);

        // 6. Medium Volume + Weak Sector -> Undetermined
        engine.update_sector_momentum(SectorMomentum {
            etf_symbol: "XLF".to_string(),
            change_pct: 1.0, // < 2.0%
            is_favorable: true,
        });
        assert_eq!(engine.compute_context().state, ContextState::Undetermined);

        // 7. High Volume (3x) + Surge -> Play (even without qualifiers)
        engine.update_volume(3_000_000, 1_000_000, true); // 3.0x, surge=true
        assert_eq!(engine.compute_context().state, ContextState::Play);

        // 8. Churning -> NoPlay
        engine.update_price_window(100.0, 99.95); // range < 0.05% < 1%
        assert!(engine.is_churning);
        assert_eq!(engine.compute_context().state, ContextState::NoPlay);

        // 9. Not Churning -> Play (revert to previous valid state)
        engine.update_price_window(100.0, 98.0); // range 2% > 1%
        assert!(!engine.is_churning);
        assert_eq!(engine.compute_context().state, ContextState::Play);
    }
}
