//! Tape Engine Crate (Fast Loop Logic).
//!
//! Contains the core trading logic: Tape Reading, Microstructure Guards, and Entry Triggers.
//!
//! # Constraints
//! - **NO Allocations** in the hot path. Use fixed-size ring buffers.
//! - **O(1)** complexity for all event handlers.
//! - **Deterministic** execution.

use core_types::{Event, EventKind, RejectReason, SymbolId, TickData, TapeComponentScores};

// Hardcoded weights from config (0.30, 0.22, 0.22, 0.13, 0.08, 0.05)
const W_R: f64 = 0.30;
const W_A: f64 = 0.22;
const W_LP: f64 = 0.22;
const W_SPR: f64 = 0.13;
const W_ABS: f64 = 0.08;
const W_BLS: f64 = 0.05;

// --- Mock Implementations for State ---
// These will be fully fleshed out in Phase 3/4.
pub struct Tape {
    pub price: f64,
    pub volume: u64,
    // Aggressive metrics for scoring
    pub rate_ticks_per_sec: f64,
    pub aggressive_buy_ratio: f64,
    pub large_print_score: f64,
    pub absorption_score: f64,
    pub buy_limit_support_score: f64,
    pub spread_cents: f64,
    pub is_reversal: bool,
}

pub struct Guards {
    pub spread: f64,
    pub imbalance: f64,
}

pub struct TapeEngine {
    // In a real implementation, this would be a fixed-size map (array) indexed by symbol_id
    // For now, we just pretend we have state for the current symbol being processed.
    tape: Tape,
    guards: Guards,
}

impl TapeEngine {
    pub fn new() -> Self {
        Self {
            tape: Tape {
                price: 0.0,
                volume: 0,
                rate_ticks_per_sec: 0.0,
                aggressive_buy_ratio: 0.0,
                large_print_score: 0.0,
                absorption_score: 0.0,
                buy_limit_support_score: 0.0,
                spread_cents: 0.0,
                is_reversal: false,
            },
            guards: Guards { spread: 0.01, imbalance: 0.5 },
        }
    }

    pub fn on_event(&mut self, event: &Event) -> Result<(), RejectReason> {
        match event.kind {
            EventKind::Tick(tick) => self.process_tick(event.symbol_id, tick),
            _ => Ok(()),
        }
    }

    fn process_tick(&mut self, symbol: SymbolId, tick: TickData) -> Result<(), RejectReason> {
        // Update local state (mock update for now, in real life updates buffers)
        self.tape.price = tick.price;
        self.evaluate_entry_logic(symbol, tick)
    }

    /// The 12-Step Locked Decision Pipeline
    fn evaluate_entry_logic(&self, _symbol: SymbolId, tick: TickData) -> Result<(), RejectReason> {
        // 1. Blocklist Check
        if self.is_blocked() { return Err(RejectReason::Blocklist); }

        // 2. Corporate Actions Gate
        if self.has_corporate_action() { return Err(RejectReason::CorporateActionBlock); }

        // 3. Price Range Gate
        if tick.price < 0.30 || tick.price > 25.00 {
            return Err(RejectReason::PriceRange);
        }

        // 4. Universe Liquidity Gate
        if !self.check_liquidity() { return Err(RejectReason::Liquidity); }

        // 5. Regime Gate
        if !self.check_regime() { return Err(RejectReason::Regime); }

        // 6. Daily Context Gate
        if !self.check_daily_context() { return Err(RejectReason::DailyContext); }

        // 7. MTF Confirmation Gate
        if !self.check_mtf() { return Err(RejectReason::MtfVeto); }

        // 8. Anti-Chase Filter
        if self.run_up_too_high() { return Err(RejectReason::AntiChase); }

        // 9. Microstructure Guards
        if !self.check_guards() { return Err(RejectReason::GuardSpread); }

        // 10. TapeScore Calculation
        // New logic: Check Reversal Veto first
        if self.tape.is_reversal {
            return Err(RejectReason::TapeReversal);
        }

        let scores = self.calculate_scores();
        if scores.total_score < 72.0 { return Err(RejectReason::TapeScoreLow); }

        // 11. ExpectedNet Validation
        if self.expected_net() <= 0.0 { return Err(RejectReason::NetNegative); }

        // 12. Exposure / Correlation Check
        if self.check_exposure() { return Err(RejectReason::Exposure); }

        Ok(())
    }

    pub fn calculate_scores(&self) -> TapeComponentScores {
        // R: Rate (ticks/sec). Normalize 0-100. Let's assume max 100 ticks/sec = 100.
        let r_score = (self.tape.rate_ticks_per_sec).min(100.0).max(0.0);

        // A: Aggression (ratio). 0.0-1.0 -> 0-100.
        let a_score = (self.tape.aggressive_buy_ratio * 100.0).min(100.0).max(0.0);

        // LP: Large Print. Already normalized in state for this mock.
        let lp_score = self.tape.large_print_score.min(100.0).max(0.0);

        // Spr: Spread. Lower is better. 0.01 = 100, 0.05 = 0.
        // Let's implement a simple linear mapping: 100 - (spread - 0.01) * factor
        // Or just mock it: if spread <= 0.01, 100. else decay.
        let spr_score = if self.tape.spread_cents <= 0.01 {
            100.0
        } else {
             (100.0 - (self.tape.spread_cents - 0.01) * 2000.0).max(0.0)
        };

        // Abs: Absorption.
        let abs_score = self.tape.absorption_score.min(100.0).max(0.0);

        // BLS: Buy Limit Support.
        let bls_score = self.tape.buy_limit_support_score.min(100.0).max(0.0);

        let total = (r_score * W_R) +
                    (a_score * W_A) +
                    (lp_score * W_LP) +
                    (spr_score * W_SPR) +
                    (abs_score * W_ABS) +
                    (bls_score * W_BLS);

        TapeComponentScores {
            r_score,
            a_score,
            lp_score,
            spr_score,
            abs_score,
            bls_score,
            total_score: total, // Weights sum to 1.0 (0.3+0.22+0.22+0.13+0.08+0.05=1.00)
        }
    }

    // --- Mock Helpers ---
    fn is_blocked(&self) -> bool { false }
    fn has_corporate_action(&self) -> bool { false }
    fn check_liquidity(&self) -> bool { true }
    fn check_regime(&self) -> bool { true }
    fn check_daily_context(&self) -> bool { true }
    fn check_mtf(&self) -> bool { true }
    fn run_up_too_high(&self) -> bool { false }
    fn check_guards(&self) -> bool { true }
    fn expected_net(&self) -> f64 { 0.05 }
    fn check_exposure(&self) -> bool { false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoring_weights() {
        let mut engine = TapeEngine::new();
        // Set all components to max (100)
        engine.tape.rate_ticks_per_sec = 100.0;
        engine.tape.aggressive_buy_ratio = 1.0;
        engine.tape.large_print_score = 100.0;
        engine.tape.spread_cents = 0.01;
        engine.tape.absorption_score = 100.0;
        engine.tape.buy_limit_support_score = 100.0;

        let scores = engine.calculate_scores();
        assert!((scores.total_score - 100.0).abs() < 0.001, "Expected 100, got {}", scores.total_score);
    }

    #[test]
    fn test_scoring_partial() {
        let mut engine = TapeEngine::new();
        // R=50 (0.3*50=15)
        engine.tape.rate_ticks_per_sec = 50.0;
        // A=50 (0.22*50=11)
        engine.tape.aggressive_buy_ratio = 0.5;
        // Others 0
        engine.tape.large_print_score = 0.0;
        engine.tape.spread_cents = 1.0; // Bad spread -> 0 score
        engine.tape.absorption_score = 0.0;
        engine.tape.buy_limit_support_score = 0.0;

        let scores = engine.calculate_scores();
        let expected = 15.0 + 11.0;
        assert!((scores.total_score - expected).abs() < 0.001, "Expected {}, got {}", expected, scores.total_score);
    }

    #[test]
    fn test_reversal_veto() {
        let mut engine = TapeEngine::new();
        // Perfect score setup
        engine.tape.rate_ticks_per_sec = 100.0;
        engine.tape.aggressive_buy_ratio = 1.0;
        engine.tape.large_print_score = 100.0;
        engine.tape.spread_cents = 0.01;

        // But reversal is active
        engine.tape.is_reversal = true;

        let tick = TickData { price: 10.0, size: 100, flags: 0 };
        let result = engine.evaluate_entry_logic(SymbolId(1), tick);
        assert_eq!(result, Err(RejectReason::TapeReversal));
    }
}
