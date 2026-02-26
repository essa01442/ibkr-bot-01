//! Tape Engine Crate (Fast Loop Logic).
//!
//! Contains the core trading logic: Tape Reading, Microstructure Guards, and Entry Triggers.
//!
//! # Constraints
//! - **NO Allocations** in the hot path. Use fixed-size ring buffers.
//! - **O(1)** complexity for all event handlers.
//! - **Deterministic** execution.

use core_types::{
    Event, EventKind, RejectReason, SymbolId, TickData, TapeComponentScores,
    Tier, RegimeState, DailyContext, MtfAnalysis, ContextState,
    config::TapeConfig, ColdStartState,
};
use risk_engine::{RiskState, guards::{GuardEvaluator, GuardConfig}};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// --- State for a Single Symbol ---
#[derive(Debug)]
pub struct SymbolState {
    pub tier: Tier,
    pub daily_context: Option<DailyContext>,
    pub mtf_analysis: Option<MtfAnalysis>,
    pub tape: TapeMetrics,
    pub last_trade_price: f64,
    pub cold_start_state: ColdStartState,
    pub ticks_in_warm_state: u64,
    // PnL Tracking
    pub position: i32,
    pub avg_cost: f64,
    pub realized_pnl: f64,
    pub current_unrealized_pnl: f64,
}

impl Default for SymbolState {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolState {
    pub fn new() -> Self {
        Self {
            tier: Tier::C, // Default to lowest tier
            daily_context: None,
            mtf_analysis: None,
            tape: TapeMetrics::default(),
            last_trade_price: 0.0,
            cold_start_state: ColdStartState::ColdStart,
            ticks_in_warm_state: 0,
            position: 0,
            avg_cost: 0.0,
            realized_pnl: 0.0,
            current_unrealized_pnl: 0.0,
        }
    }
}

#[derive(Debug, Default)]
pub struct TapeMetrics {
    pub price: f64,
    pub bid: f64,
    pub ask: f64,
    pub bid_size: u32,
    pub ask_size: u32,
    pub volume: u64,

    // Aggressive metrics for scoring
    pub rate_ticks_per_sec: f64,
    pub aggressive_buy_ratio: f64,
    pub large_print_score: f64,
    pub absorption_score: f64,
    pub buy_limit_support_score: f64,
    pub spread_cents: f64,
    pub is_reversal: bool,

    // For Anti-Chase (simplified)
    pub vwap: f64,
    pub atr: f64,
}

pub struct TapeEngine {
    // Global State
    pub risk_state: Arc<Mutex<RiskState>>,
    pub regime_state: RegimeState,
    pub guard_evaluator: GuardEvaluator,
    pub config: TapeConfig,

    // Per-Symbol State
    pub symbol_states: HashMap<SymbolId, SymbolState>,

    // Global PnL Cache
    pub global_realized_pnl: f64,
    pub global_unrealized_pnl: f64,

    pub daily_target_reached: bool,
}

impl TapeEngine {
    pub fn new(risk_state: Arc<Mutex<RiskState>>, guard_config: GuardConfig, config: TapeConfig) -> Self {
        Self {
            risk_state,
            regime_state: RegimeState::Normal, // Default
            guard_evaluator: GuardEvaluator::new(guard_config),
            config,
            symbol_states: HashMap::new(),
            global_realized_pnl: 0.0,
            global_unrealized_pnl: 0.0,
            daily_target_reached: false,
        }
    }

    // --- State Update Methods (Slow Loop Interface) ---
    pub fn update_tier(&mut self, symbol: SymbolId, tier: Tier) {
        self.get_mut_state(symbol).tier = tier;
    }

    pub fn update_daily_context(&mut self, ctx: DailyContext) {
        let symbol_id = ctx.symbol_id;
        self.get_mut_state(symbol_id).daily_context = Some(ctx);
    }

    pub fn update_mtf_analysis(&mut self, symbol: SymbolId, analysis: MtfAnalysis) {
        self.get_mut_state(symbol).mtf_analysis = Some(analysis);
    }

    pub fn update_regime(&mut self, regime: RegimeState) {
        self.regime_state = regime;
    }

    pub fn should_terminate(&self) -> bool {
        self.risk_state.lock().unwrap().should_terminate()
    }

    pub fn set_daily_target_reached(&mut self, reached: bool) {
        self.daily_target_reached = reached;
    }

    // --- Event Processing (Fast Loop Interface) ---
    pub fn on_event(&mut self, event: &Event) -> Result<(), RejectReason> {
        // Track event activity (Flicker check)
        self.guard_evaluator.track_event(event.symbol_id, event.ts_src)?;

        match event.kind {
            EventKind::Tick(tick) => {
                 // Convert ts_src (micros) to day ordinal roughly
                 // This assumes ts_src is system time or close to it.
                 // For now, we will pass 0 or compute it if needed in check_entry.
                 // Actually, tape_engine doesn't know "today".
                 // We should pass today_ordinal into on_event or TapeEngine::new?
                 // Let's compute it from ts_src for now, assuming ts_src is unix epoch micros
                 let secs = event.ts_src / 1_000_000;
                 let days = (secs / 86400) as u32; // Rough approximation (UTC)
                 self.process_tick(event.symbol_id, event.ts_src, tick, days)
            },
            EventKind::Snapshot(snap) => {
                let state = self.get_mut_state(event.symbol_id);
                state.tape.bid = snap.bid_price;
                state.tape.ask = snap.ask_price;
                state.tape.bid_size = snap.bid_size;
                state.tape.ask_size = snap.ask_size;
                state.tape.spread_cents = snap.ask_price - snap.bid_price;
                Ok(())
            },
            EventKind::Fill(fill) => self.process_fill(event.symbol_id, fill),
            EventKind::Reconnect => {
                log::warn!("TapeEngine: Reconnect received — resetting ColdStart for all symbols.");
                for state in self.symbol_states.values_mut() {
                    state.cold_start_state = ColdStartState::ColdStart;
                    state.tape = TapeMetrics::default();
                }
                Ok(())
            }
            EventKind::StateSync(ref sync) => {
                log::info!("TapeEngine: StateSync received — reconciling {} positions.",
                    sync.positions.len());
                // Reset all positions first
                for state in self.symbol_states.values_mut() {
                    state.position = 0;
                    state.avg_cost = 0.0;
                    state.current_unrealized_pnl = 0.0;
                }
                self.global_unrealized_pnl = 0.0;
                // Rebuild from broker truth
                for pos in &sync.positions {
                    let state = self.symbol_states.entry(pos.symbol_id)
                        .or_default();
                    state.position = pos.qty;
                    state.avg_cost = pos.avg_cost;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn process_tick(&mut self, symbol: SymbolId, ts_src: u64, tick: TickData, day_ordinal: u32) -> Result<(), RejectReason> {
        // We cannot use get_mut_state directly because we need to update global pnl
        // which requires mutable self.

        let state = self.symbol_states.entry(symbol).or_default();
        state.tape.price = tick.price;
        state.last_trade_price = tick.price; // Simplified for now

        // Update Unrealized PnL
        if state.position != 0 {
            let new_unrealized = (tick.price - state.avg_cost) * state.position as f64;
            let delta = new_unrealized - state.current_unrealized_pnl;
            state.current_unrealized_pnl = new_unrealized;

            // Update Global
            self.global_unrealized_pnl += delta;
            self.risk_state.lock().unwrap().update_pnl(self.global_realized_pnl + self.global_unrealized_pnl);
        }

        self.evaluate_entry_logic(symbol, ts_src, day_ordinal)
    }

    fn process_fill(&mut self, symbol: SymbolId, fill: core_types::FillData) -> Result<(), RejectReason> {
        let state = self.symbol_states.entry(symbol).or_default();

        let fill_size = fill.size as i32;
        let signed_fill_size = if fill.side == core_types::Side::Bid { fill_size } else { -fill_size };
        let fill_price = fill.price;

        if state.position == 0 {
            state.position = signed_fill_size;
            state.avg_cost = fill_price;
        } else {
            // Check if increasing or reducing
            let same_side = (state.position > 0 && signed_fill_size > 0) || (state.position < 0 && signed_fill_size < 0);

            if same_side {
                // Weighted Average Cost
                let total_cost = (state.position as f64 * state.avg_cost) + (signed_fill_size as f64 * fill_price);
                state.position += signed_fill_size;
                state.avg_cost = total_cost / state.position as f64;
            } else {
                // Realize PnL
                // Portion of position closed is min(abs(pos), abs(fill))
                let close_qty = std::cmp::min(state.position.abs(), signed_fill_size.abs());
                // The signed amount of closing
                let signed_close_qty = if state.position > 0 { -close_qty } else { close_qty };

                let trade_pnl = (fill_price - state.avg_cost) * (-signed_close_qty as f64);
                state.realized_pnl += trade_pnl;

                // Update Global Realized
                self.global_realized_pnl += trade_pnl;

                let prev_position = state.position;
                state.position += signed_fill_size;

                if state.position == 0 {
                    state.avg_cost = 0.0;
                } else if (prev_position > 0 && state.position < 0) || (prev_position < 0 && state.position > 0) {
                    // Position flipped. The remaining part is new open.
                    // If flipped, avg_cost should reset to fill_price for the remainder.
                    state.avg_cost = fill_price;
                }
            }
        }

        // Update Risk State
        let current_price = if state.tape.price > 0.0 { state.tape.price } else { fill_price };
        let new_unrealized = if state.position != 0 {
            (current_price - state.avg_cost) * state.position as f64
        } else {
            0.0
        };

        let delta_unrealized = new_unrealized - state.current_unrealized_pnl;
        state.current_unrealized_pnl = new_unrealized;
        self.global_unrealized_pnl += delta_unrealized;

        self.risk_state.lock().unwrap().update_pnl(self.global_realized_pnl + self.global_unrealized_pnl);

        Ok(())
    }

    pub fn update_cold_start(&mut self, symbol: SymbolId, is_surge: bool) {
        let state = self.get_mut_state(symbol);
        if is_surge {
            state.cold_start_state = ColdStartState::FullActive;
            return;
        }

        match state.cold_start_state {
            ColdStartState::ColdStart => {
                state.cold_start_state = ColdStartState::WarmActive;
                state.ticks_in_warm_state = 0;
            },
            ColdStartState::WarmActive => {
                state.ticks_in_warm_state += 1;
                if state.ticks_in_warm_state >= 100 {
                    state.cold_start_state = ColdStartState::FullActive;
                }
            },
            ColdStartState::FullActive => {}
        }
    }

    /// The 12-Step Locked Decision Pipeline
    pub fn evaluate_entry_logic(&mut self, symbol: SymbolId, ts_src: u64, day_ordinal: u32) -> Result<(), RejectReason> {
        let state = self.symbol_states.get(&symbol).expect("Symbol state should exist");

        // 0. Pre-Gate: Tier A Only (User Requirement)
        if state.tier != Tier::A {
            return Err(RejectReason::Blocklist);
        }

        // 1. Blocklist Check
        // Pass open positions to check_entry for exposure validation
        // In simulation, we need a list of open symbols.
        // We can iterate symbol_states where position != 0
        let mut open_symbols_buf = [SymbolId(0u32); 2];
        let mut open_count = 0usize;
        for (id, state) in &self.symbol_states {
            if state.position != 0 {
                if open_count < 2 {
                    open_symbols_buf[open_count] = *id;
                }
                open_count += 1;
            }
        }
        let open_symbols = &open_symbols_buf[..open_count.min(2)];

        self.risk_state.lock().unwrap().check_entry(symbol, open_symbols, day_ordinal)?;

        // 2. Corporate Actions Gate (Covered by check_entry)

        // 3. Price Range & 4. Liquidity (RiskState)
        let (adv, addv_usd) = match &state.daily_context {
            Some(ctx) => {
                let adv = ctx.volume_profile.avg_20d_volume;
                (adv, adv as f64 * state.tape.price)
            },
            None => (0, 0.0)
        };

        self.risk_state.lock().unwrap().check_liquidity(
            state.tape.price,
            state.tape.spread_cents / state.tape.price, // spread pct
            adv,
            addv_usd
        )?;

        // 5. Regime Gate (Normal Only)
        if self.regime_state != RegimeState::Normal {
            return Err(RejectReason::Regime);
        }

        // 6. Daily Context Gate
        match &state.daily_context {
            Some(ctx) => {
                if ctx.state != ContextState::Play {
                    return Err(RejectReason::DailyContext);
                }
            },
            None => return Err(RejectReason::DailyContext),
        }

        // 7. MTF Confirmation Gate
        match &state.mtf_analysis {
            Some(mtf) => {
                if !mtf.mtf_pass {
                    return Err(RejectReason::MtfVeto);
                }
            },
            None => return Err(RejectReason::MtfVeto),
        }

        // 8. Anti-Chase Filter
        if self.check_anti_chase(state) {
            return Err(RejectReason::AntiChase);
        }

        // 9. Microstructure Guards
        self.guard_evaluator.check_execution(
            symbol,
            ts_src,
            ts_src, // System time assumed to be ts_src for simulation
            state.tape.bid,
            state.tape.ask,
            state.tape.bid_size,
            state.tape.ask_size,
            state.last_trade_price
        )?;

        // 10. TapeScore Calculation
        if state.tape.is_reversal {
            return Err(RejectReason::TapeReversal);
        }
        let scores = self.calculate_scores(&state.tape);

        let threshold = match state.tier {
            Tier::A => {
                match state.cold_start_state {
                    ColdStartState::WarmActive => self.config.tape_threshold_warm,
                    ColdStartState::FullActive if self.daily_target_reached => self.config.tape_threshold_post_target,
                    ColdStartState::FullActive => self.config.tape_threshold_normal,
                    ColdStartState::ColdStart => return Err(RejectReason::TapeScoreLow),
                }
            }
            _ => return Err(RejectReason::Blocklist),
        };

        if scores.total_score < threshold {
            return Err(RejectReason::TapeScoreLow);
        }

        // 11. ExpectedNet Validation
        if self.expected_net(&state.tape) <= 0.0 {
            return Err(RejectReason::NetNegative);
        }

        // 12. Exposure / Correlation Check
        if !self.check_exposure(symbol, open_symbols) {
            return Err(RejectReason::Exposure);
        }

        Ok(())
    }

    fn get_mut_state(&mut self, symbol: SymbolId) -> &mut SymbolState {
        self.symbol_states.entry(symbol).or_default()
    }

    pub fn calculate_scores(&self, tape: &TapeMetrics) -> TapeComponentScores {
        let r_score = tape.rate_ticks_per_sec.clamp(0.0, 100.0);
        let a_score = (tape.aggressive_buy_ratio * 100.0).clamp(0.0, 100.0);
        let lp_score = tape.large_print_score.clamp(0.0, 100.0);

        let spr_score = if tape.spread_cents <= 0.01 {
            100.0
        } else {
             (100.0 - (tape.spread_cents - 0.01) * 2000.0).max(0.0)
        };

        let abs_score = tape.absorption_score.clamp(0.0, 100.0);
        let bls_score = tape.buy_limit_support_score.clamp(0.0, 100.0);

        let total = (r_score * self.config.weights.w_r) +
                    (a_score * self.config.weights.w_a) +
                    (lp_score * self.config.weights.w_lp) +
                    (spr_score * self.config.weights.w_spr) +
                    (abs_score * self.config.weights.w_abs) +
                    (bls_score * self.config.weights.w_bls);

        TapeComponentScores {
            r_score,
            a_score,
            lp_score,
            spr_score,
            abs_score,
            bls_score,
            total_score: total,
        }
    }

    fn check_anti_chase(&self, state: &SymbolState) -> bool {
        // Simple logic: If price is > VWAP + 2 * ATR, consider it extended/chasing.
        // Assuming ATR and VWAP are populated.
        if state.tape.vwap > 0.0
            && state.tape.atr > 0.0
            && state.tape.price > state.tape.vwap + (2.0 * state.tape.atr)
        {
            return true;
        }
        false
    }

    fn expected_net(&self, tape: &TapeMetrics) -> f64 {
        let scores = self.calculate_scores(tape);
        if scores.total_score > 72.0 { 0.10 } else { -0.10 }
    }

    fn check_exposure(&self, symbol: SymbolId, open_symbols: &[SymbolId]) -> bool {
        self.risk_state.lock().unwrap().exposure_validator
            .check_new_position(symbol, open_symbols)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::LiquidityConfig;

    fn default_engine() -> TapeEngine {
        let config = TapeConfig {
            tape_threshold_normal: 72.0,
            tape_threshold_post_target: 82.0,
            tape_threshold_warm: 67.0,
            weights: core_types::config::TapeWeights {
                w_r: 0.30, w_a: 0.22, w_lp: 0.22, w_spr: 0.13, w_abs: 0.08, w_bls: 0.05
            }
        };
        TapeEngine::new(
            Arc::new(Mutex::new(RiskState::new(1000.0, LiquidityConfig::default()))),
            GuardConfig::default(),
            config
        )
    }

    fn set_valid_tape(engine: &mut TapeEngine, sym: SymbolId) {
        let state = engine.get_mut_state(sym);
        state.tape.price = 10.0;
        state.last_trade_price = 10.0;
        state.tape.bid = 9.99;
        state.tape.ask = 10.01;
        state.tape.spread_cents = 0.02;
        state.tape.bid_size = 500;
        state.tape.ask_size = 500;
        state.tape.volume = 600_000;
        state.tape.rate_ticks_per_sec = 80.0;
        state.tape.aggressive_buy_ratio = 0.8;
        state.tape.large_print_score = 80.0;
        // For AntiChase
        state.tape.vwap = 10.0;
        state.tape.atr = 0.10;
        // For threshold
        state.cold_start_state = ColdStartState::FullActive;
    }

    #[test]
    fn test_tier_a_requirement() {
        let mut engine = default_engine();
        let sym = SymbolId(1);

        // Tier C (default) -> Blocklist (Reject)
        set_valid_tape(&mut engine, sym); // Init with valid price/liquidity
        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::Blocklist)); // Tier Check

        // Promote to Tier A
        engine.update_tier(sym, Tier::A);
        // Fail on Context (None) because we haven't set it yet.
        // This causes Liquidity check (Step 4) to fail because ADV=0.
        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::Liquidity));
    }

    #[test]
    fn test_regime_requirement() {
        let mut engine = default_engine();
        let sym = SymbolId(1);
        set_valid_tape(&mut engine, sym);
        engine.update_tier(sym, Tier::A);
        engine.update_daily_context(DailyContext {
             symbol_id: sym,
             state: ContextState::Play,
             volume_profile: mock_volume_profile(),
             has_news: false,
             sector_momentum: None
        });

        // Regime RiskOff
        engine.update_regime(RegimeState::RiskOff);
        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::Regime));
    }

    #[test]
    fn test_full_pass() {
        let mut engine = default_engine();
        let sym = SymbolId(1);

        // Setup passing state
        engine.update_tier(sym, Tier::A);
        engine.update_daily_context(DailyContext {
             symbol_id: sym,
             state: ContextState::Play,
             volume_profile: core_types::VolumeProfile { current_volume: 1_000_000, avg_20d_volume: 500_000, is_surge: false },
             has_news: true,
             sector_momentum: None
        });
        engine.update_mtf_analysis(sym, MtfAnalysis {
            weekly_trend_confirmed: true,
            daily_resistance_cleared: true,
            structure_4h_bullish: true,
            pullback_15m_valid: true,
            mtf_pass: true,
        });

        let state = engine.get_mut_state(sym);
        state.tape.price = 10.0;
        state.last_trade_price = 10.0;
        state.tape.bid = 9.99;
        state.tape.ask = 10.01;
        state.tape.spread_cents = 0.02; // < 0.05
        state.tape.bid_size = 500;
        state.tape.ask_size = 500;
        state.tape.volume = 600_000;
        state.cold_start_state = ColdStartState::FullActive;

        // Scoring
        state.tape.rate_ticks_per_sec = 100.0;
        state.tape.aggressive_buy_ratio = 1.0;
        state.tape.large_print_score = 100.0;
        state.tape.absorption_score = 100.0; // To ensure passing

        // Guards
        // Spread 0.02 (OK), Imb 0.0 (OK), etc.

        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert!(res.is_ok(), "Expected Ok, got {:?}", res);
    }

    fn mock_volume_profile() -> core_types::VolumeProfile {
        core_types::VolumeProfile {
            current_volume: 1_000_000,
            avg_20d_volume: 500_000,
            is_surge: false,
        }
    }

    fn mock_mtf_pass() -> MtfAnalysis {
        MtfAnalysis {
            weekly_trend_confirmed: true,
            daily_resistance_cleared: true,
            structure_4h_bullish: true,
            pullback_15m_valid: true,
            mtf_pass: true,
        }
    }

    #[test]
    fn test_mtf_veto() {
        let mut engine = default_engine();
        let sym = SymbolId(1);
        set_valid_tape(&mut engine, sym);
        engine.update_tier(sym, Tier::A);
        engine.update_daily_context(DailyContext {
             symbol_id: sym,
             state: ContextState::Play,
             volume_profile: mock_volume_profile(),
             has_news: true,
             sector_momentum: None
        });
        // MTF Fail
        engine.update_mtf_analysis(sym, MtfAnalysis {
            weekly_trend_confirmed: false,
            daily_resistance_cleared: false,
            structure_4h_bullish: false,
            pullback_15m_valid: false,
            mtf_pass: false,
        });

        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::MtfVeto));
    }

    #[test]
    fn test_guard_failure() {
        let mut engine = default_engine();
        let sym = SymbolId(1);
        set_valid_tape(&mut engine, sym);
        engine.update_tier(sym, Tier::A);
        engine.update_daily_context(DailyContext {
             symbol_id: sym,
             state: ContextState::Play,
             volume_profile: mock_volume_profile(),
             has_news: true,
             sector_momentum: None
        });
        engine.update_mtf_analysis(sym, mock_mtf_pass());

        let state = engine.get_mut_state(sym);
        state.tape.price = 10.0;
        state.tape.bid = 9.90;
        state.tape.ask = 10.10; // Spread 0.20 > 0.05
        state.tape.spread_cents = 0.20;
        // Need to override volume/size after set_valid_tape if needed, but set_valid_tape sets them.
        state.tape.bid_size = 500;
        state.tape.ask_size = 500;

        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::GuardSpread));
    }

    #[test]
    fn test_tapescore_low() {
        let mut engine = default_engine();
        let sym = SymbolId(1);
        set_valid_tape(&mut engine, sym);
        engine.update_tier(sym, Tier::A);
        engine.update_daily_context(DailyContext {
             symbol_id: sym,
             state: ContextState::Play,
             volume_profile: mock_volume_profile(),
             has_news: true,
             sector_momentum: None
        });
        engine.update_mtf_analysis(sym, mock_mtf_pass());

        let state = engine.get_mut_state(sym);
        // Reset valid tape but degrade scoring metrics
        state.tape.rate_ticks_per_sec = 0.0;
        state.tape.aggressive_buy_ratio = 0.0;
        state.tape.large_print_score = 0.0;
        state.tape.spread_cents = 0.02; // Valid spread contributes some score (approx 26 pts), but threshold is 72.

        let res = engine.evaluate_entry_logic(sym, 1000, 0);
        assert_eq!(res, Err(RejectReason::TapeScoreLow));
    }
}
