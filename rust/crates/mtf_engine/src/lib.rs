#![deny(clippy::unwrap_in_result)]
//! MTF Engine Crate.
//!
//! Evaluates Multi-Timeframe Confirmation (Weekly, Daily, 4H, 15m).
//! Designed to run in the SlowLoop.

use core_types::{MtfAnalysis, SymbolId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtfParams {
    pub require_all: bool,            // Strict mode?
    pub stale_data_threshold_ms: u64, // Threshold for data staleness
}

impl Default for MtfParams {
    fn default() -> Self {
        Self {
            require_all: true,
            stale_data_threshold_ms: 3600000, // 1 hour default
        }
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

    // Timestamps for staleness
    pub last_weekly_ema_ts: u64,
    pub last_daily_res_ts: u64,
    pub last_4h_ts: u64,
    pub last_15m_ts: u64,

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
            last_weekly_ema_ts: 0,
            last_daily_res_ts: 0,
            last_4h_ts: 0,
            last_15m_ts: 0,
            current_price: 0.0,
        }
    }

    pub fn update_price(&mut self, price: f64) {
        self.current_price = price;
    }

    pub fn update_weekly_ema(&mut self, ema: f64, ts_src: u64) {
        // Prevent zero-default from registering as a valid update
        if ema > 0.0 {
            self.weekly_ema = ema;
            self.last_weekly_ema_ts = ts_src;
        }
    }

    pub fn update_daily_resistance(&mut self, resistance: f64, ts_src: u64) {
        // Prevent zero-default from registering as a valid update
        if resistance > 0.0 {
            self.daily_resistance = resistance;
            self.last_daily_res_ts = ts_src;
        }
    }

    pub fn update_4h_structure(&mut self, is_bullish: bool, ts_src: u64) {
        self.structure_4h_bullish = is_bullish;
        self.last_4h_ts = ts_src;
    }

    pub fn update_15m_pullback(&mut self, is_valid: bool, ts_src: u64) {
        self.pullback_15m_valid = is_valid;
        self.last_15m_ts = ts_src;
    }

    pub fn evaluate(&self, current_ts: u64) -> MtfAnalysis {
        // Check staleness. Convert ts_src (micros) to ms for comparison with threshold.
        let is_stale = |last_ts: u64| -> bool {
            if last_ts == 0 {
                return true; // Never updated
            }
            let elapsed_ms = current_ts.saturating_sub(last_ts) / 1000;
            elapsed_ms > self.params.stale_data_threshold_ms
        };

        let ema_stale = is_stale(self.last_weekly_ema_ts);
        let res_stale = is_stale(self.last_daily_res_ts);
        let h4_stale = is_stale(self.last_4h_ts);
        let m15_stale = is_stale(self.last_15m_ts);

        // Force to neutral (false) if stale or zero (EMA/Res default)
        let weekly_trend_confirmed = if !ema_stale && self.weekly_ema > 0.0 {
            self.current_price > self.weekly_ema
        } else {
            if ema_stale && self.last_weekly_ema_ts > 0 {
                log::warn!(
                    "MTF Engine: Weekly EMA data is stale for symbol {:?}",
                    self.symbol_id
                );
            }
            false
        };

        // Daily Resistance: Cleared if price > resistance
        let daily_resistance_cleared =
            if !res_stale && self.daily_resistance > 0.0 && self.daily_resistance < f64::MAX {
                self.current_price > self.daily_resistance
            } else {
                if res_stale && self.last_daily_res_ts > 0 {
                    log::warn!(
                        "MTF Engine: Daily Resistance data is stale for symbol {:?}",
                        self.symbol_id
                    );
                }
                false // No resistance data or stale
            };

        let structure_4h_bullish = if !h4_stale {
            self.structure_4h_bullish
        } else {
            if h4_stale && self.last_4h_ts > 0 {
                log::warn!(
                    "MTF Engine: 4H structure data is stale for symbol {:?}",
                    self.symbol_id
                );
            }
            false
        };

        let pullback_15m_valid = if !m15_stale {
            self.pullback_15m_valid
        } else {
            if m15_stale && self.last_15m_ts > 0 {
                log::warn!(
                    "MTF Engine: 15m pullback data is stale for symbol {:?}",
                    self.symbol_id
                );
            }
            false
        };

        let mut score = 0;
        if weekly_trend_confirmed {
            score += 1;
        }
        if daily_resistance_cleared {
            score += 1;
        }
        if structure_4h_bullish {
            score += 1;
        }
        if pullback_15m_valid {
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
            structure_4h_bullish,
            pullback_15m_valid,
            mtf_pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mtf_evaluation() {
        let params = MtfParams {
            require_all: true,
            stale_data_threshold_ms: 3600000,
        };
        let symbol = SymbolId(1);
        let mut engine = MtfEngine::new(symbol, params);

        engine.update_price(10.0);

        // 1. Weekly EMA Check
        engine.update_weekly_ema(9.0, 1000); // Price > EMA -> Pass

        // 2. Daily Resistance
        engine.update_daily_resistance(9.5, 1000); // Price > Res -> Cleared (Breakout)

        // 3. 4H Structure
        engine.update_4h_structure(true, 1000);

        // 4. 15m Pullback
        engine.update_15m_pullback(true, 1000);

        let analysis = engine.evaluate(2000); // Not stale
        assert!(analysis.weekly_trend_confirmed);
        assert!(analysis.daily_resistance_cleared);
        assert!(analysis.structure_4h_bullish);
        assert!(analysis.pullback_15m_valid);
        assert!(analysis.mtf_pass);
    }

    #[test]
    fn test_mtf_fail() {
        let params = MtfParams {
            require_all: true,
            stale_data_threshold_ms: 3600000,
        };
        let mut engine = MtfEngine::new(SymbolId(1), params);
        engine.update_price(10.0);

        engine.update_weekly_ema(11.0, 1000); // Fail (Downtrend)
        engine.update_daily_resistance(9.5, 1000); // Pass
        engine.update_4h_structure(true, 1000); // Pass
        engine.update_15m_pullback(true, 1000); // Pass

        let analysis = engine.evaluate(2000);
        assert!(!analysis.weekly_trend_confirmed);
        assert!(!analysis.mtf_pass);
    }

    #[test]
    fn test_mtf_stale_data() {
        let params = MtfParams {
            require_all: true,
            stale_data_threshold_ms: 1000,
        }; // 1 sec threshold
        let mut engine = MtfEngine::new(SymbolId(1), params);
        engine.update_price(10.0);

        engine.update_weekly_ema(9.0, 1_000_000); // Data from t=1sec
        engine.update_daily_resistance(9.5, 1_000_000);
        engine.update_4h_structure(true, 1_000_000);
        engine.update_15m_pullback(true, 1_000_000);

        // Evaluate at 1.5 seconds -> Not stale
        let analysis_ok = engine.evaluate(1_500_000);
        assert!(analysis_ok.mtf_pass);

        // Evaluate at 3 seconds -> Stale (> 1 sec threshold)
        let analysis_stale = engine.evaluate(3_000_000);
        assert!(!analysis_stale.weekly_trend_confirmed);
        assert!(!analysis_stale.daily_resistance_cleared);
        assert!(!analysis_stale.structure_4h_bullish);
        assert!(!analysis_stale.pullback_15m_valid);
        assert!(!analysis_stale.mtf_pass);
    }

    #[test]
    fn test_zero_default_prevention() {
        let params = MtfParams {
            require_all: true,
            stale_data_threshold_ms: 3600000,
        };
        let mut engine = MtfEngine::new(SymbolId(1), params);
        engine.update_price(10.0);

        // Update with zeros
        engine.update_weekly_ema(0.0, 1000);
        engine.update_daily_resistance(0.0, 1000);
        engine.update_4h_structure(true, 1000);
        engine.update_15m_pullback(true, 1000);

        let analysis = engine.evaluate(2000);

        // Zero defaults must yield false/neutral
        assert!(!analysis.weekly_trend_confirmed);
        assert!(!analysis.daily_resistance_cleared);
        assert!(!analysis.mtf_pass);
    }
}
