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
            r_score: 0.0,
            a_score: 0.0,
            lp_score: 0.0,
            spr_score: 0.0,
            abs_score: 0.0,
            bls_score: 0.0,
            tape_score: 0.0,
            tape_score_threshold: 0.0,
            price: 0.0,
            expected_net: 0.0,
            expected_gross: 0.0,
            total_fees: 0.0,
            expected_slippage: 0.0,
            latency_src_rx: 0,
            latency_rx_proc: 0,
            latency_proc_decision: 0,
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
    pub expected_slippage: f64, // For calibration comparison

    // Exit reason
    pub exit_reason: ExitReason,

    // Loss attribution (§24.6)
    pub loss_attribution: Option<LossAttributionCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExitReason {
    Target,       // Hit $50 gross or 10% move
    Stop,         // Server-side stop triggered
    Manual,       // Manual close
    LuldHalt,     // LULD/Halt emergency exit
    RegimeChange, // Regime turned Risk-Off
    TapeReversal, // R < 0.7 + spread widening
    SessionClose, // End of session
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossAttributionCode {
    EntryModel, // Wrong signal / bad threshold
    Context,    // Daily or MTF filter failure
    Guards,     // Spread/depth/volatility structural issue
    Execution,  // Slippage / delay / partial fills
    Risk,       // Stop / ladder / kill switch
    Data,       // Data degradation / missing packets
}

pub struct TradeJournalStore {
    trades: std::collections::VecDeque<TradeJournal>,
    capacity: usize,
}

impl TradeJournalStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            trades: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
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
        if let Some(trade) = self
            .trades
            .iter_mut()
            .rev()
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
    calls_since_last_p95: u64,

    // SLA enforcement (§22)
    pub consecutive_breach_start: Option<u64>, // timestamp when P95 first exceeded 25ms
    pub hard_fail_triggered: bool,
}

impl LatencyTracker {
    pub fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            sorted_cache: Vec::with_capacity(capacity),
            p95_cache: 0,
            calls_since_last_p95: 100, // Force calc on first call
            consecutive_breach_start: None,
            hard_fail_triggered: false,
        }
    }

    /// Returns true if SLA hard fail has been triggered (P95 > 25ms for > 5 seconds continuously).
    pub fn check_sla_hard_fail(&mut self, now_micros: u64) -> bool {
        const SLA_HARD_FAIL_MICROS: u64 = 25_000; // 25ms
        const SLA_HARD_FAIL_DURATION_MICROS: u64 = 5_000_000; // 5 seconds

        let p95 = self.p95();
        if p95 > SLA_HARD_FAIL_MICROS {
            match self.consecutive_breach_start {
                None => {
                    self.consecutive_breach_start = Some(now_micros);
                }
                Some(start) => {
                    if now_micros.saturating_sub(start) >= SLA_HARD_FAIL_DURATION_MICROS {
                        if !self.hard_fail_triggered {
                            log::error!(
                                "SLA HARD FAIL: P95={}µs for {}s — entering Monitor Only",
                                p95,
                                SLA_HARD_FAIL_DURATION_MICROS / 1_000_000
                            );
                            self.hard_fail_triggered = true;
                        }
                        return true;
                    }
                }
            }
        } else {
            // P95 recovered — reset breach timer but do NOT clear hard_fail until session reset
            self.consecutive_breach_start = None;
        }
        self.hard_fail_triggered
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

/// Operational metrics per §24.3
#[derive(Debug, Default, Clone)]
pub struct MetricsCollector {
    pub reject_counts: std::collections::HashMap<u8, u64>, // RejectReason as u8 → count
    pub total_decisions: u64,
    pub total_entries: u64,
    pub api_error_count: u64,
    pub reconnect_count: u64,
    pub data_quality_events: u64,
    pub ibkr_subscription_count: u32,
    pub sla_breach_count: u64,
}

impl MetricsCollector {
    pub fn record_decision(&mut self, log: &DecisionLog) {
        self.total_decisions += 1;
        if let DecisionAction::Enter = log.action {
            self.total_entries += 1;
        }
        if let Some(reason) = log.reject_reason {
            *self.reject_counts.entry(reason as u8).or_insert(0) += 1;
        }
    }

    /// Returns reject rate for a given reason (0.0–1.0).
    pub fn reject_rate(&self, reason: RejectReason) -> f64 {
        if self.total_decisions == 0 {
            return 0.0;
        }
        let count = self
            .reject_counts
            .get(&(reason as u8))
            .copied()
            .unwrap_or(0);
        count as f64 / self.total_decisions as f64
    }
}

/// Alert types per §24.4
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    DataApiDown,
    HeartbeatMissing { duration_secs: u64 },
    SlaBreach { p95_micros: u64, duration_secs: u64 },
    DailyLossLimitReached { loss_usd: i64 },
    LossLadderActivated { level: u32 },
    OrderAnomaly { order_id: u64, kind: &'static str },
    IbkrSubscriptionHigh { current: u32, limit: u32 },
    MtfRejectRateHigh { rate_pct: u32 },
}

pub struct AlertManager {
    /// Callback: in production, replace with actual notification (log/email/etc.)
    pub alerts: std::collections::VecDeque<Alert>,
    capacity: usize,
}

impl AlertManager {
    pub fn new(capacity: usize) -> Self {
        Self {
            alerts: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn raise(&mut self, alert: Alert) {
        log::error!("ALERT: {:?}", alert);
        if self.alerts.len() >= self.capacity {
            self.alerts.pop_front();
        }
        self.alerts.push_back(alert);
    }

    pub fn recent(&self, n: usize) -> impl Iterator<Item = &Alert> {
        self.alerts.iter().rev().take(n)
    }
}

/// Per §26.4 — tracks predicted vs actual slippage for α/β calibration.
/// After ≥ 20 trades: if actual_avg > 1.5 × predicted_avg → raise alert to update config.
#[derive(Debug, Default)]
pub struct CalibrationLogger {
    pub records: Vec<SlippageRecord>,
    pub min_trades_for_eval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageRecord {
    pub symbol_id: u32,
    pub ts: u64,
    pub shares: u32,
    pub entry_price: f64,
    pub predicted_slippage: f64,
    pub actual_slippage: f64,
    pub ratio: f64,  // actual / predicted
}

impl CalibrationLogger {
    pub fn new(min_trades: usize) -> Self {
        Self { records: Vec::new(), min_trades_for_eval: min_trades }
    }

    pub fn record(&mut self, symbol_id: u32, ts: u64, shares: u32, entry_price: f64,
                  predicted: f64, actual: f64) {
        let ratio = if predicted > 0.0 { actual / predicted } else { 0.0 };
        self.records.push(SlippageRecord {
            symbol_id, ts, shares, entry_price,
            predicted_slippage: predicted,
            actual_slippage: actual,
            ratio,
        });
    }

    /// Returns Some(avg_ratio) if we have enough data, None otherwise.
    /// Per §26.4: if avg_ratio > 1.5 → update α/β.
    pub fn evaluate(&self) -> Option<f64> {
        if self.records.len() < self.min_trades_for_eval { return None; }
        let avg = self.records.iter().map(|r| r.ratio).sum::<f64>() / self.records.len() as f64;
        Some(avg)
    }

    /// Per §26.4: checks if calibration threshold is breached.
    pub fn needs_recalibration(&self) -> bool {
        self.evaluate().map(|r| r > 1.5).unwrap_or(false)
    }

    /// Save to JSON for analysis.
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.records)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
}

pub fn log_decision(log: &DecisionLog) {
    if let DecisionAction::Enter = log.action {
        log::info!(
            "ENTER sym={:?} price={:.4} tape={:.1}/{:.1} net={:.4} lat={}µs",
            log.symbol_id,
            log.price,
            log.tape_score,
            log.tape_score_threshold,
            log.expected_net,
            log.latency_proc_decision
        );
    } else {
        log::debug!(
            "REJECT sym={:?} reason={:?} price={:.4} tape={:.1} net={:.4}",
            log.symbol_id,
            log.reject_reason,
            log.price,
            log.tape_score,
            log.expected_net
        );
    }
}
