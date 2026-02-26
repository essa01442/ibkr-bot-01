//! Regime Engine Crate.
//!
//! Determines the global market regime (Normal, Caution, Risk-Off).
//! Aggregates inputs from various sensors (Breadth, ATR, Data Quality).

use core_types::{DataQuality, RegimeState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeParams {
    pub max_atr_spy: f64,
    pub min_breadth: f64,
    pub max_breadth: f64,
    pub max_spread_width: f64,
}

impl Default for RegimeParams {
    fn default() -> Self {
        Self {
            max_atr_spy: 2.5,
            min_breadth: -2000.0,
            max_breadth: 2000.0,
            max_spread_width: 0.10, // 10 cents avg spread ? or percent? assuming absolute for now, but context matters.
                                    // Prompt says "Spread widening". Let's assume a normalized metric or avg spread.
        }
    }
}

pub struct RegimeEngine {
    params: RegimeParams,

    // Inputs
    pub spy_atr_1m: f64,
    pub market_breadth: f64,
    pub avg_spread_width: f64,
    pub is_calendar_risk: bool,
    pub data_quality: DataQuality,

    // Current State
    current_state: RegimeState,
}

impl RegimeEngine {
    pub fn new(params: RegimeParams) -> Self {
        Self {
            params,
            spy_atr_1m: 0.0,
            market_breadth: 0.0,
            avg_spread_width: 0.0,
            is_calendar_risk: false,
            data_quality: DataQuality::Ok,
            current_state: RegimeState::Normal,
        }
    }

    pub fn update_atr(&mut self, spy_atr: f64) {
        self.spy_atr_1m = spy_atr;
        self.recalc();
    }

    pub fn update_breadth(&mut self, breadth: f64) {
        self.market_breadth = breadth;
        self.recalc();
    }

    pub fn update_spread_width(&mut self, avg_spread: f64) {
        self.avg_spread_width = avg_spread;
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
        // 1. Critical Overrides
        if self.data_quality != DataQuality::Ok {
            return RegimeState::RiskOff;
        }

        if self.is_calendar_risk {
            // Calendar risk might be Caution or RiskOff depending on severity.
            // Requirement doesn't specify, but usually high impact news = RiskOff or at least Caution.
            // Let's be conservative: RiskOff for the duration of the event window.
            return RegimeState::RiskOff;
        }

        // 2. Market Metrics
        if self.spy_atr_1m > self.params.max_atr_spy {
            return RegimeState::Caution;
        }

        if self.market_breadth < self.params.min_breadth
            || self.market_breadth > self.params.max_breadth
        {
            return RegimeState::Caution;
        }

        if self.avg_spread_width > self.params.max_spread_width {
            return RegimeState::Caution;
        }

        // 3. Default
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

        // Initial state
        assert_eq!(engine.state(), RegimeState::Normal);

        // Data Quality Issue
        engine.update_data_quality(DataQuality::Degraded);
        assert_eq!(engine.state(), RegimeState::RiskOff);
        engine.update_data_quality(DataQuality::Ok);
        assert_eq!(engine.state(), RegimeState::Normal);

        // Calendar Risk
        engine.update_calendar_risk(true);
        assert_eq!(engine.state(), RegimeState::RiskOff);
        engine.update_calendar_risk(false);
        assert_eq!(engine.state(), RegimeState::Normal);

        // ATR Breach
        engine.update_atr(3.0); // Default max is 2.5
        assert_eq!(engine.state(), RegimeState::Caution);
        engine.update_atr(1.0);
        assert_eq!(engine.state(), RegimeState::Normal);

        // Breadth Breach
        engine.update_breadth(-2500.0); // Default min is -2000
        assert_eq!(engine.state(), RegimeState::Caution);
        engine.update_breadth(0.0);
        assert_eq!(engine.state(), RegimeState::Normal);
    }
}
