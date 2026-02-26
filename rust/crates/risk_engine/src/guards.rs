use core_types::{RejectReason, SymbolId, TimeRingBuffer};
use std::collections::HashMap;

/// Configuration for Microstructure Guards.
#[derive(Debug, Clone)]
pub struct GuardConfig {
    /// Maximum allowed spread in price units (e.g., cents).
    pub max_spread_cents: f64,

    /// Maximum allowed imbalance ratio.
    /// Calculated as `abs(BidSize - AskSize) / (BidSize + AskSize)`.
    /// If > threshold, reject.
    pub max_imbalance_ratio: f64,

    /// Maximum time since last update (staleness) in milliseconds.
    pub max_staleness_ms: u64,

    /// Minimum liquidity (size) required at BBO to avoid "L2 Vacuum".
    /// If (BidSize + AskSize) < threshold, reject.
    pub min_liquidity_shares: u32,

    /// Maximum number of updates allowed within `flicker_window_ms`.
    pub max_flicker_count: usize,

    /// Window size for flicker detection in milliseconds.
    pub flicker_window_ms: u64,

    /// Maximum allowed price deviation (slippage proxy) in price units.
    /// Checks `abs(MidPrice - LastTradePrice)`.
    pub slippage_tol_cents: f64,

    /// Hysteresis TTL: If a guard trips, block for this many milliseconds.
    pub ttl_ms: u64,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            max_spread_cents: 0.05,
            max_imbalance_ratio: 0.7,
            max_staleness_ms: 3000,
            min_liquidity_shares: 200,
            max_flicker_count: 10,
            flicker_window_ms: 1000,
            slippage_tol_cents: 0.03,
            ttl_ms: 2000,
        }
    }
}

struct GuardState {
    last_update_ts: u64,
    blocked_until: u64,
    block_reason: Option<RejectReason>,
    // Using TimeRingBuffer for zero-allocation flicker detection.
    // We store () as the item since we only care about timestamps.
    // The buffer is unit-agnostic regarding time, as long as we are consistent (ms here).
    flicker_buffer: TimeRingBuffer<()>,
}

impl GuardState {
    fn new(flicker_capacity: usize, flicker_window_unit: u64) -> Self {
        Self {
            last_update_ts: 0,
            blocked_until: 0,
            block_reason: None,
            flicker_buffer: TimeRingBuffer::new(flicker_capacity, flicker_window_unit),
        }
    }
}

pub struct GuardEvaluator {
    config: GuardConfig,
    states: HashMap<SymbolId, GuardState>,
}

impl GuardEvaluator {
    pub fn new(config: GuardConfig) -> Self {
        Self {
            config,
            states: HashMap::new(),
        }
    }

    /// Tracks a market data event to update flicker detection and activity monitoring.
    ///
    /// This should be called for every relevant market data update (tick, L2 delta, snapshot).
    /// If the update rate exceeds `max_flicker_count` within `flicker_window_ms`,
    /// the symbol is blocked with `GuardFlicker`.
    pub fn track_event(&mut self, symbol: SymbolId, timestamp_ms: u64) -> Result<(), RejectReason> {
        let config = &self.config;
        let state = self.states.entry(symbol).or_insert_with(|| {
            GuardState::new(config.max_flicker_count * 2, config.flicker_window_ms)
        });

        // Update last update timestamp
        state.last_update_ts = timestamp_ms;

        // 0. TTL Hysteresis Check (Short-circuit if already blocked)
        if timestamp_ms < state.blocked_until {
            if let Some(reason) = state.block_reason {
                return Err(reason);
            }
        }

        // 1. Update Flicker Buffer
        state.flicker_buffer.push(timestamp_ms, ());
        state.flicker_buffer.prune_expired(timestamp_ms);

        if state.flicker_buffer.len() > config.max_flicker_count {
            state.blocked_until = timestamp_ms + config.ttl_ms;
            state.block_reason = Some(RejectReason::GuardFlicker);
            return Err(RejectReason::GuardFlicker);
        }

        Ok(())
    }

    /// Evaluates all microstructure guards for a given symbol and market state.
    ///
    /// This method does NOT update the flicker buffer (call `track_event` for that).
    /// It checks if the symbol is blocked (TTL) or violates any guard (Spread, Liquidity, etc.).
    ///
    /// # Arguments
    /// * `symbol` - The symbol to check.
    /// * `timestamp_ms` - Current system time in milliseconds.
    /// * `data_timestamp_ms` - Timestamp of the market data event (e.g., exchange time).
    /// * `bid` - Best Bid Price.
    /// * `ask` - Best Ask Price.
    /// * `bid_size` - Size at Best Bid.
    /// * `ask_size` - Size at Best Ask.
    /// * `last_trade_price` - Price of the last trade (for slippage check).
    #[allow(clippy::too_many_arguments)]
    pub fn check_execution(
        &mut self,
        symbol: SymbolId,
        timestamp_ms: u64,
        data_timestamp_ms: u64,
        bid: f64,
        ask: f64,
        bid_size: u32,
        ask_size: u32,
        last_trade_price: f64,
    ) -> Result<(), RejectReason> {
        let config = &self.config;

        // Ensure state exists (if check called before track)
        let state = self.states.entry(symbol).or_insert_with(|| {
            GuardState::new(config.max_flicker_count * 2, config.flicker_window_ms)
        });

        // 0. TTL Hysteresis Check
        if timestamp_ms < state.blocked_until {
            if let Some(reason) = state.block_reason {
                return Err(reason);
            }
        }

        // 1. Stale Data Check
        // If the data timestamp is too old relative to system time.
        if timestamp_ms.saturating_sub(data_timestamp_ms) > config.max_staleness_ms {
            return self.reject(symbol, timestamp_ms, RejectReason::GuardStale);
        }

        // 2. Spread Check
        let spread = ask - bid;
        if spread > config.max_spread_cents || spread <= 0.0 {
            return self.reject(symbol, timestamp_ms, RejectReason::GuardSpread);
        }

        // 3. L2 Vacuum (Liquidity) Check
        // "Vacuum" if total liquidity at BBO is too low.
        let total_liquidity = bid_size + ask_size;
        if total_liquidity < config.min_liquidity_shares {
            return self.reject(symbol, timestamp_ms, RejectReason::GuardL2Vacuum);
        }

        // 4. Imbalance Check
        // Avoid division by zero
        if total_liquidity > 0 {
            let imb = (bid_size as f64 - ask_size as f64).abs() / (total_liquidity as f64);
            if imb > config.max_imbalance_ratio {
                return self.reject(symbol, timestamp_ms, RejectReason::GuardImbalance);
            }
        }

        // 5. Slippage Check (Price Deviation)
        // Check if Mid Price is far from Last Trade Price
        let mid = (bid + ask) / 2.0;
        let deviation = (mid - last_trade_price).abs();
        if deviation > config.slippage_tol_cents {
            return self.reject(symbol, timestamp_ms, RejectReason::GuardSlippage);
        }

        // Note: We do NOT clear block_reason automatically here,
        // relying on TTL expiration in Step 0.
        // If we passed all checks, we are good.
        // Update state on success
        state.last_update_ts = timestamp_ms;
        state.block_reason = None; // Clear block if we passed (and TTL expired)

        Ok(())
    }

    fn reject(
        &mut self,
        symbol: SymbolId,
        timestamp_ms: u64,
        reason: RejectReason,
    ) -> Result<(), RejectReason> {
        if let Some(state) = self.states.get_mut(&symbol) {
            state.blocked_until = timestamp_ms + self.config.ttl_ms;
            state.block_reason = Some(reason);
        }
        Err(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spread_guard() {
        let mut evaluator = GuardEvaluator::new(GuardConfig::default());
        let symbol = SymbolId(1);
        let now = 1000;
        let data_ts = now;

        // Spread 0.04 (OK)
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.00, 10.04, 500, 500, 10.02)
            .is_ok());

        // Spread 0.06 (Fail)
        match evaluator.check_execution(symbol, now, data_ts, 10.00, 10.06, 500, 500, 10.03) {
            Err(RejectReason::GuardSpread) => (),
            _ => panic!("Expected GuardSpread"),
        }
    }

    #[test]
    fn test_ttl_hysteresis() {
        let mut evaluator = GuardEvaluator::new(GuardConfig::default());
        let symbol = SymbolId(1);
        let mut now = 1000;
        let mut data_ts = now;

        // Trigger failure (Spread)
        let _ = evaluator.check_execution(symbol, now, data_ts, 10.00, 10.06, 500, 500, 10.03);

        // Fix spread, but TTL should still block
        now += 100; // +100ms
        data_ts = now;
        match evaluator.check_execution(symbol, now, data_ts, 10.00, 10.04, 500, 500, 10.02) {
            Err(RejectReason::GuardSpread) => (), // Still Spread error due to latch
            _ => panic!("Expected GuardSpread persistence"),
        }

        // Wait for TTL (2000ms default)
        now += 2000;
        data_ts = now;
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.00, 10.04, 500, 500, 10.02)
            .is_ok());
    }

    #[test]
    fn test_imbalance_guard() {
        let mut evaluator = GuardEvaluator::new(GuardConfig::default());
        let symbol = SymbolId(2);
        let now = 1000;
        let data_ts = now;

        // Balanced (500/500 = 0.0) -> OK
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.00, 10.01, 500, 500, 10.005)
            .is_ok());

        // Imbalanced (900/100 = 800/1000 = 0.8) -> Fail (Limit 0.7)
        match evaluator.check_execution(symbol, now, data_ts, 10.00, 10.01, 900, 100, 10.005) {
            Err(RejectReason::GuardImbalance) => (),
            _ => panic!("Expected GuardImbalance"),
        }
    }

    #[test]
    fn test_l2_vacuum() {
        let mut evaluator = GuardEvaluator::new(GuardConfig::default());
        let symbol = SymbolId(3);
        let now = 1000;
        let data_ts = now;

        // Liquidity 100+100 = 200 (OK, limit 200 inclusive?)
        // Limit is min_liquidity_shares: 200. If < 200 reject.
        // 200 is OK.
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.00, 10.01, 100, 100, 10.005)
            .is_ok());

        // Liquidity 50+50 = 100 (Fail)
        match evaluator.check_execution(symbol, now, data_ts, 10.00, 10.01, 50, 50, 10.005) {
            Err(RejectReason::GuardL2Vacuum) => (),
            _ => panic!("Expected GuardL2Vacuum"),
        }
    }

    #[test]
    fn test_slippage_guard() {
        let mut evaluator = GuardEvaluator::new(GuardConfig::default());
        let symbol = SymbolId(4);
        let now = 1000;
        let data_ts = now;

        // Mid 10.005, Last 10.005 -> Diff 0.0 (OK)
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.00, 10.01, 500, 500, 10.005)
            .is_ok());

        // Mid 10.005, Last 10.10 -> Diff 0.095 > 0.03 (Fail)
        match evaluator.check_execution(symbol, now, data_ts, 10.00, 10.01, 500, 500, 10.10) {
            Err(RejectReason::GuardSlippage) => (),
            _ => panic!("Expected GuardSlippage"),
        }
    }

    #[test]
    fn test_flicker_guard() {
        let mut config = GuardConfig::default();
        config.max_flicker_count = 3;
        config.flicker_window_ms = 100;
        let mut evaluator = GuardEvaluator::new(config);
        let symbol = SymbolId(5);
        let mut now = 1000;

        // 3 updates OK
        assert!(evaluator.track_event(symbol, now).is_ok());
        now += 10;
        assert!(evaluator.track_event(symbol, now).is_ok());
        now += 10;
        assert!(evaluator.track_event(symbol, now).is_ok());

        // 4th update in window -> Fail
        now += 10;
        match evaluator.track_event(symbol, now) {
            Err(RejectReason::GuardFlicker) => (),
            _ => panic!("Expected GuardFlicker"),
        }

        // Check that check_execution also reports block
        match evaluator.check_execution(symbol, now, now, 10.0, 10.04, 100, 100, 10.02) {
            Err(RejectReason::GuardFlicker) => (),
            _ => panic!("Expected GuardFlicker persistence in check_execution"),
        }
    }

    #[test]
    fn test_stale_guard() {
        let mut config = GuardConfig::default();
        config.max_staleness_ms = 1000;
        let mut evaluator = GuardEvaluator::new(config);
        let symbol = SymbolId(6);
        let now = 2000;

        // Data is recent (1500 vs 2000, 500ms diff) -> OK
        let data_ts = 1500;
        assert!(evaluator
            .check_execution(symbol, now, data_ts, 10.0, 10.04, 100, 100, 10.02)
            .is_ok());

        // Data is old (500 vs 2000, 1500ms diff) -> Fail
        let data_ts_old = 500;
        match evaluator.check_execution(symbol, now, data_ts_old, 10.0, 10.04, 100, 100, 10.02) {
            Err(RejectReason::GuardStale) => (),
            _ => panic!("Expected GuardStale"),
        }
    }
}
