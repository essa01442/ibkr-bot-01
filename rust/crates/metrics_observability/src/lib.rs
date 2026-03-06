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
