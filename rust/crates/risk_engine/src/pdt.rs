use std::collections::VecDeque;

/// Pattern Day Trader guard.
/// A "day trade" = buying and selling (or selling short and buying) the same security
/// on the same trading day.
/// Rule: 4+ day trades in any rolling 5-business-day window → PDT flag.
/// Applies only to margin accounts with < $25,000 equity.
/// This implementation tracks day-trade count conservatively regardless of account type.
#[derive(Debug)]
pub struct PdtGuard {
    /// Ring buffer of (date_ordinal: u32, symbol_id: u32) for completed day trades.
    trades: VecDeque<(u32, u32)>,
    /// Max day trades allowed in the rolling 5-day window before blocking.
    max_day_trades: usize,
}

impl Default for PdtGuard {
    fn default() -> Self {
        Self {
            trades: VecDeque::new(),
            max_day_trades: 3, // Default to strict 3-trade rule
        }
    }
}

impl PdtGuard {
    pub fn new(max_day_trades: usize) -> Self {
        Self { trades: VecDeque::new(), max_day_trades }
    }

    /// Returns the number of day trades in the last 5 business days.
    pub fn day_trade_count_last_5_days(&self, today_ordinal: u32) -> usize {
        self.trades.iter()
            .filter(|(d, _)| today_ordinal.saturating_sub(*d) < 5)
            .count()
    }

    /// Records a completed day trade. Call when a buy and same-day sell are both filled.
    pub fn record_day_trade(&mut self, date_ordinal: u32, symbol_id: u32) {
        self.trades.push_back((date_ordinal, symbol_id));
        // Prune entries older than 5 days
        while let Some(&(d, _)) = self.trades.front() {
            if date_ordinal.saturating_sub(d) >= 5 { self.trades.pop_front(); } else { break; }
        }
    }

    /// Returns true if a new entry would violate PDT (would become the 4th day trade).
    pub fn would_violate(&self, today_ordinal: u32) -> bool {
        self.day_trade_count_last_5_days(today_ordinal) >= self.max_day_trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdt_counting() {
        let mut guard = PdtGuard::new(3); // Max 3 day trades
        let today = 100;

        // Day 1: 1 trade
        guard.record_day_trade(today - 4, 1);
        assert_eq!(guard.day_trade_count_last_5_days(today), 1);
        assert!(!guard.would_violate(today));

        // Day 2: 1 trade
        guard.record_day_trade(today - 3, 2);
        assert_eq!(guard.day_trade_count_last_5_days(today), 2);
        assert!(!guard.would_violate(today));

        // Day 3: 1 trade
        guard.record_day_trade(today - 2, 3);
        assert_eq!(guard.day_trade_count_last_5_days(today), 3);
        // Next trade would violate (be the 4th)
        assert!(guard.would_violate(today));

        // Record it anyway (simulate breach)
        guard.record_day_trade(today - 1, 4);
        assert_eq!(guard.day_trade_count_last_5_days(today), 4);
    }

    #[test]
    fn test_pdt_expiry() {
        let mut guard = PdtGuard::new(3);
        let today = 100;

        // Trade on day 94 (6 days ago)
        guard.record_day_trade(today - 6, 1);
        assert_eq!(guard.day_trade_count_last_5_days(today), 0);

        // Trade on day 95 (5 days ago) - strictly < 5 check means 0..4 diff.
        // If diff is 5, it is 6th day?
        // "Rolling 5-business-day window". Today is day 5. Day 1,2,3,4,5.
        // today - d < 5.
        // 100 - 95 = 5. Not < 5. So expired.
        guard.record_day_trade(today - 5, 2);
        assert_eq!(guard.day_trade_count_last_5_days(today), 0);

        // Trade on day 96 (4 days ago)
        guard.record_day_trade(today - 4, 3);
        assert_eq!(guard.day_trade_count_last_5_days(today), 1);
    }
}
