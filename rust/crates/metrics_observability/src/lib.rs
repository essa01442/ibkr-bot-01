//! Metrics & Observability Crate.
//!
//! Handles Decision Logging, Trade Journaling, and Latency Monitoring.
//! Must not block the hot path (should run in its own task).

use core_types::{RejectReason, SymbolId};
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
    pub symbol_id: SymbolId,
    pub timestamp: u64, // System time of decision
    pub action: DecisionAction,
    pub reject_reason: Option<RejectReason>,

    // Latencies in Microseconds
    pub latency_src_rx: u64,
    pub latency_rx_proc: u64,
    pub latency_proc_decision: u64,

    // Context Snapshot (Simplified)
    pub price: f64,
    pub tape_score: f64,
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
