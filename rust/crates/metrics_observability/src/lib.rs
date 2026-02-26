//! Metrics & Observability Crate.
//!
//! Handles Decision Logging, Trade Journaling, and Latency Monitoring.
//! Must not block the hot path (should run in its own task).

use serde::Serialize;
use core_types::RejectReason;

#[derive(Serialize)]
pub struct DecisionLog<'a> {
    pub symbol_id: u32,
    pub timestamp: u64,
    pub decision: &'a str,
    pub reject_reason: Option<RejectReason>,
    // Other metrics (TapeScore, R, A, etc.)
}

pub fn log_decision(log: &DecisionLog) {
    // Structured logging implementation (e.g. to JSON file or stdout)
    // In prod, this would send to a channel to be written async.
    // log::info!("{}", serde_json::to_string(log).unwrap());
}
