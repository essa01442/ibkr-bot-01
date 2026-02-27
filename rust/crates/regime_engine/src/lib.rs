//! Regime Engine Crate.
//!
//! Determines the global market regime (Normal, Caution, Risk-Off).
//! Aggregates inputs from various sensors (Breadth, ATR, Data Quality).

use core_types::{DataQuality, RegimeState};
use serde::{Deserialize, Serialize};

/// Regime thresholds per §11. Values are loaded from AppConfig.regime at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeParams {
    /// ATR(SPY 1m) threshold for Normal regime (§11) — e.g. 0.0018 = 0.18%
    pub atr_normal_max: f64,
    /// ATR(SPY 1m) threshold for Caution → Risk-Off boundary
    pub atr_caution_max: f64,
    /// Market breadth (fraction 0..1) minimum for Normal
    pub breadth_normal_min: f64,
    /// Market breadth minimum for Caution → Risk-Off boundary
    pub breadth_caution_min: f64,
    /// Spread widening fraction → Caution (e.g. 0.25 = +25%)
    pub widening_caution_pct: f64,
    /// Spread widening fraction → Risk-Off (e.g. 0.50 = +50%)
    pub widening_riskoff_pct: f64,
}

impl Default for RegimeParams {
    fn default() -> Self {
        Self {
            atr_normal_max: 0.0018,
            atr_caution_max: 0.0028,
            breadth_normal_min: 0.45,
            breadth_caution_min: 0.35,
            widening_caution_pct: 0.25,
            widening_riskoff_pct: 0.50,
        }
    }
}

pub struct RegimeEngine {
    params: RegimeParams,
    pub spy_atr_1m: f64,        // As fraction (0.0018 = 0.18%)
    pub market_breadth: f64,    // As fraction (0.45 = 45%)
    pub spy_spread_baseline: f64, // Baseline SPY spread (set at session open)
    pub avg_spread_current: f64,  // Current average spread width
    pub is_calendar_risk: bool,
    pub data_quality: DataQuality,
    current_state: RegimeState,
}

impl RegimeEngine {
    pub fn new(params: RegimeParams) -> Self {
        Self {
            params,
            spy_atr_1m: 0.0,
            market_breadth: 0.45, // Default to Normal-range until first update
            spy_spread_baseline: 0.0,
            avg_spread_current: 0.0,
            is_calendar_risk: false,
            data_quality: DataQuality::Ok,
            current_state: RegimeState::Normal,
        }
    }

    pub fn set_spread_baseline(&mut self, baseline: f64) {
        self.spy_spread_baseline = baseline;
        self.recalc();
    }

    pub fn update_current_spread(&mut self, current: f64) {
        self.avg_spread_current = current;
        self.recalc();
    }

    pub fn update_atr(&mut self, spy_atr: f64) {
        self.spy_atr_1m = spy_atr;
        self.recalc();
    }

    pub fn update_breadth(&mut self, breadth: f64) {
        self.market_breadth = breadth;
        self.recalc();
    }

    pub fn update_calendar_risk(&mut self, is_risk: bool) {
        self.is_calendar_risk = is_risk;
        self.recalc();
    }

    pub fn update_data_quality(&mut self, quality: DataQuality) {
        self.data_quality = quality;
        self.recalc();
    }

    fn recalc(&mut self) {
        self.current_state = self.calculate_state();
    }

    pub fn calculate_state(&self) -> RegimeState {
        // DataQuality degraded → Risk-Off immediately (§11 table)
        if self.data_quality != DataQuality::Ok {
            return RegimeState::RiskOff;
        }

        // Compute spread widening vs baseline
        let widening = if self.spy_spread_baseline > 0.0 {
            (self.avg_spread_current - self.spy_spread_baseline) / self.spy_spread_baseline
        } else {
            0.0
        };

        // Risk-Off conditions (any one → Risk-Off) per §11
        if self.spy_atr_1m >= self.params.atr_caution_max
            || self.market_breadth <= self.params.breadth_caution_min
            || widening >= self.params.widening_riskoff_pct
        {
            return RegimeState::RiskOff;
        }

        // Calendar risk → Caution per §11 (not Risk-Off, unless other conditions)
        if self.is_calendar_risk {
            return RegimeState::Caution;
        }

        // Caution conditions (any one → Caution) per §11
        if self.spy_atr_1m > self.params.atr_normal_max
            || self.market_breadth < self.params.breadth_normal_min
            || widening >= self.params.widening_caution_pct
        {
            return RegimeState::Caution;
        }

        RegimeState::Normal
    }

    pub fn state(&self) -> RegimeState {
        self.current_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transitions() {
        let mut engine = RegimeEngine::new(RegimeParams::default());
        assert_eq!(engine.state(), RegimeState::Normal);

        engine.update_data_quality(DataQuality::Degraded);
        assert_eq!(engine.state(), RegimeState::RiskOff);
        engine.update_data_quality(DataQuality::Ok);
        assert_eq!(engine.state(), RegimeState::Normal);

        engine.update_calendar_risk(true);
        assert_eq!(engine.state(), RegimeState::Caution); // Calendar → Caution, not RiskOff
        engine.update_calendar_risk(false);
        assert_eq!(engine.state(), RegimeState::Normal);

        engine.update_atr(0.0020); // > 0.0018 → Caution
        assert_eq!(engine.state(), RegimeState::Caution);
        engine.update_atr(0.0030); // > 0.0028 → RiskOff
        assert_eq!(engine.state(), RegimeState::RiskOff);
        engine.update_atr(0.0015);
        assert_eq!(engine.state(), RegimeState::Normal);

        engine.update_breadth(0.40); // < 0.45 → Caution
        assert_eq!(engine.state(), RegimeState::Caution);
        engine.update_breadth(0.30); // < 0.35 → RiskOff
        assert_eq!(engine.state(), RegimeState::RiskOff);
        engine.update_breadth(0.50);
        assert_eq!(engine.state(), RegimeState::Normal);
    }
}
