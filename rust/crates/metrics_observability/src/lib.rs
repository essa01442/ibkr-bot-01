//! Metrics & Observability Crate.
//!
//! Handles Decision Logging, Trade Journaling, and Latency Monitoring.
//! Must not block the hot path (should run in its own task).

use core_types::{ColdStartState, RegimeState, RejectReason, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const SLA_LIMIT_MICROS: u64 = 10_000; // 10ms per spec §22

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum DecisionAction {
    Enter = 0,
    Reject = 1,
}

impl std::fmt::Display for DecisionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionAction::Enter => write!(f, "Enter"),
            DecisionAction::Reject => write!(f, "Reject"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionLog {
    // Identity
    pub symbol_id: SymbolId,
    pub timestamp: u64,

    // Decision
    pub action: DecisionAction,
    pub reject_reason: Option<RejectReason>,
    /// Up to 3 secondary reject reasons (minor contributing factors)
    pub secondary_reasons: [Option<RejectReason>; 3],

    // Gate results (§24.1)
    pub gate_blocklist: bool,
    pub gate_corporate: bool,
    pub gate_price_range: bool,
    pub gate_universe: bool,
    pub gate_regime: bool,
    pub gate_daily_context: bool,
    pub gate_mtf: bool,
    pub gate_anti_chase: bool,
    pub gate_guards: bool,

    // TapeScore components
    pub r_score: f64,
    pub a_score: f64,
    pub lp_score: f64,
    pub spr_score: f64,
    pub abs_score: f64,
    pub bls_score: f64,
    pub tape_score: f64,
    pub tape_score_threshold: f64,

    // Pricing
    pub price: f64,
    pub expected_net: f64,
    pub expected_gross: f64,
    pub total_fees: f64,
    pub expected_slippage: f64,


    // Pricing
    pub price: f64,
    pub expected_net: f64,
    pub expected_gross: f64,
    pub total_fees: f64,
    pub expected_slippage: f64,

    // Latencies (microseconds)
    pub latency_src_rx: u64,
    pub latency_rx_proc: u64,
    pub latency_proc_decision: u64,

    // State
    pub cold_start_state: ColdStartState,
    pub regime_state: RegimeState,
}

impl DecisionLog {
    pub fn new_reject(symbol_id: SymbolId, timestamp: u64, reason: RejectReason) -> Self {
        Self {
            symbol_id,
            timestamp,
            action: DecisionAction::Reject,
            reject_reason: Some(reason),
            secondary_reasons: [None; 3],
            gate_blocklist: true,
            gate_corporate: true,
            gate_price_range: true,
            gate_universe: true,
            gate_regime: true,
            gate_daily_context: true,
            gate_mtf: true,
            gate_anti_chase: true,
            gate_guards: true,
            r_score: 0.0, a_score: 0.0, lp_score: 0.0, spr_score: 0.0,
            abs_score: 0.0, bls_score: 0.0,
            tape_score: 0.0, tape_score_threshold: 0.0,
            price: 0.0, expected_net: 0.0, expected_gross: 0.0,
            total_fees: 0.0, expected_slippage: 0.0,
            latency_src_rx: 0, latency_rx_proc: 0, latency_proc_decision: 0,
            cold_start_state: ColdStartState::ColdStart,
            regime_state: RegimeState::Normal,
        }
    }
}

/// Per §24.2 — full record of every executed trade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeJournal {
    pub symbol_id: SymbolId,
    pub entry_ts: u64,
    pub exit_ts: Option<u64>,

    // Entry snapshot (full DecisionLog at entry moment)
    pub entry_decision: DecisionLog,

    // Fill data
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub shares: u32,
    pub avg_fill_price: f64,
    pub fill_count: u32,

    // PnL
    pub gross_pnl: f64,
    pub total_fees: f64,
    pub actual_slippage: f64,
    pub net_pnl: f64,
    pub expected_slippage: f64,  // For calibration comparison

    // Exit reason
    pub exit_reason: ExitReason,

    // Loss attribution (§24.6)
    pub loss_attribution: Option<LossAttributionCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    Target,        // Hit $50 gross or 10% move
    Stop,          // Server-side stop triggered
    Manual,        // Manual close
    LuldHalt,      // LULD/Halt emergency exit
    RegimeChange,  // Regime turned Risk-Off
    TapeReversal,  // R < 0.7 + spread widening
    SessionClose,  // End of session
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossAttributionCode {
    EntryModel,  // Wrong signal / bad threshold
    Context,     // Daily or MTF filter failure
    Guards,      // Spread/depth/volatility structural issue
    Execution,   // Slippage / delay / partial fills
    Risk,        // Stop / ladder / kill switch
    Data,        // Data degradation / missing packets
}

pub struct TradeJournalStore {
    trades: std::collections::VecDeque<TradeJournal>,
    capacity: usize,
}

impl TradeJournalStore {
    pub fn new(capacity: usize) -> Self {
        Self { trades: std::collections::VecDeque::with_capacity(capacity), capacity }
    }

    pub fn record_entry(&mut self, journal: TradeJournal) {
        if self.trades.len() >= self.capacity {
            self.trades.pop_front();
        }
        self.trades.push_back(journal);
    }

    pub fn close_trade(
        &mut self,
        symbol_id: SymbolId,
        exit_ts: u64,
        exit_price: f64,
        actual_slippage: f64,
        exit_reason: ExitReason,
        loss_attribution: Option<LossAttributionCode>,
    ) {
        if let Some(trade) = self.trades.iter_mut().rev()
            .find(|t| t.symbol_id == symbol_id && t.exit_ts.is_none())
        {
            trade.exit_ts = Some(exit_ts);
            trade.exit_price = Some(exit_price);
            trade.actual_slippage = actual_slippage;
            trade.exit_reason = exit_reason;
            trade.loss_attribution = loss_attribution;
            let pnl = (exit_price - trade.entry_price) * trade.shares as f64;
            trade.gross_pnl = pnl;
            trade.net_pnl = pnl - trade.total_fees - actual_slippage;
        }
    }

    pub fn recent(&self, n: usize) -> impl Iterator<Item = &TradeJournal> {
        self.trades.iter().rev().take(n)
    }
}

pub struct LatencyTracker {
    // Stores last N latency measurements in microseconds
    window: VecDeque<u64>,
    capacity: usize,
    sorted_cache: Vec<u64>,
    p95_cache: u64,
    calls_since_last_p95: u32,
}

impl LatencyTracker {
    pub fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            sorted_cache: Vec::with_capacity(capacity),
            p95_cache: 0,
            calls_since_last_p95: 100, // Force calc on first call
        }
    }

    pub fn record(&mut self, latency_us: u64) {
        if self.window.len() >= self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(latency_us);
    }

    pub fn p95(&mut self) -> u64 {
        self.calls_since_last_p95 += 1;
        if self.calls_since_last_p95 < 100 {
            return self.p95_cache;
        }
        self.calls_since_last_p95 = 0;

        if self.window.is_empty() {
            return 0;
        }
        // Copy and sort
        self.sorted_cache.clear();
        self.sorted_cache.extend(self.window.iter());
        self.sorted_cache.sort_unstable();

        let idx =
            ((self.sorted_cache.len() as f64 * 0.95) as usize).min(self.sorted_cache.len() - 1);
        self.p95_cache = self.sorted_cache[idx];
        self.p95_cache
    }
}

pub fn log_decision(log: &DecisionLog) {
    // In prod, structured logging (JSON)
    if let Ok(json) = serde_json::to_string(log) {
        log::info!(target: "decision_log", "{}", json);
    }
}
