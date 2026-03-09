#![deny(clippy::unwrap_in_result)]
//! MTF Engine Crate.
//!
//! Evaluates Multi-Timeframe Confirmation (Weekly, Daily, 4H, 15m).
//! Designed to run in the SlowLoop.

use core_types::{MtfAnalysis, SymbolId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtfParams {
    pub require_all: bool, // Strict mode?
}

impl Default for MtfParams {
    fn default() -> Self {
        Self { require_all: true }
    }
}

pub struct MtfEngine {
    params: MtfParams,
    #[allow(dead_code)]
    symbol_id: SymbolId,

    // Inputs
    pub weekly_ema: f64,
    pub daily_resistance: f64,
    pub structure_4h_bullish: bool,
    pub pullback_15m_valid: bool,

    // Current Price (from SlowLoop updates)
    pub current_price: f64,
}

impl MtfEngine {
    pub fn new(symbol_id: SymbolId, params: MtfParams) -> Self {
        Self {
            params,
            symbol_id,
            weekly_ema: 0.0,
            daily_resistance: f64::MAX, // Assume resistance is high initially
            structure_4h_bullish: false,
            pullback_15m_valid: false,
            current_price: 0.0,
        }
    }

    pub fn update_price(&mut self, price: f64) {
        self.current_price = price;
    }

    pub fn update_weekly_ema(&mut self, ema: f64) {
        self.weekly_ema = ema;
    }

    pub fn update_daily_resistance(&mut self, resistance: f64) {
        self.daily_resistance = resistance;
    }

    pub fn update_4h_structure(&mut self, is_bullish: bool) {
        self.structure_4h_bullish = is_bullish;
    }

    pub fn update_15m_pullback(&mut self, is_valid: bool) {
        self.pullback_15m_valid = is_valid;
    }

    pub fn evaluate(&self) -> MtfAnalysis {
        let weekly_trend_confirmed = if self.weekly_ema > 0.0 {
            self.current_price > self.weekly_ema
        } else {
            false
        };

        // Daily Resistance: Cleared if price > resistance
        let daily_resistance_cleared = if self.daily_resistance < f64::MAX {
            self.current_price > self.daily_resistance
        } else {
            false // No resistance data
        };

        let mut score = 0;
        if weekly_trend_confirmed {
            score += 1;
        }
        if daily_resistance_cleared {
            score += 1;
        }
        if self.structure_4h_bullish {
            score += 1;
        }
        if self.pullback_15m_valid {
            score += 1;
        }

        let mtf_pass = if self.params.require_all {
            score == 4
        } else {
            score >= 3
        };

        MtfAnalysis {
            weekly_trend_confirmed,
            daily_resistance_cleared,
            structure_4h_bullish: self.structure_4h_bullish,
            pullback_15m_valid: self.pullback_15m_valid,
            mtf_pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mtf_evaluation() {
        let params = MtfParams { require_all: true };
        let symbol = SymbolId(1);
        let mut engine = MtfEngine::new(symbol, params);

        engine.update_price(10.0);

        // 1. Weekly EMA Check
        engine.update_weekly_ema(9.0); // Price > EMA -> Pass

        // 2. Daily Resistance
        engine.update_daily_resistance(9.5); // Price > Res -> Cleared (Breakout)

        // 3. 4H Structure
        engine.update_4h_structure(true);

        // 4. 15m Pullback
        engine.update_15m_pullback(true);

        let analysis = engine.evaluate();
        assert!(analysis.weekly_trend_confirmed);
        assert!(analysis.daily_resistance_cleared);
        assert!(analysis.structure_4h_bullish);
        assert!(analysis.pullback_15m_valid);
        assert!(analysis.mtf_pass);
    }

    #[test]
    fn test_mtf_fail() {
        let params = MtfParams { require_all: true };
        let mut engine = MtfEngine::new(SymbolId(1), params);
        engine.update_price(10.0);

        engine.update_weekly_ema(11.0); // Fail (Downtrend)
        engine.update_daily_resistance(9.5); // Pass
        engine.update_4h_structure(true); // Pass
        engine.update_15m_pullback(true); // Pass

        let analysis = engine.evaluate();
        assert!(!analysis.weekly_trend_confirmed);
        assert!(!analysis.mtf_pass);
    }
}
