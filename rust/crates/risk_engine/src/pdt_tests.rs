use core_types::market_day_boundary;
use chrono::{TimeZone, Utc};
use super::pdt::PdtGuard;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdt_5_day_rolling_window_in_et() {
        let mut guard = PdtGuard::new(3);

        // Day 1: 2024-06-10 00:01:00 UTC (which is June 9 ET)
        let dt1 = Utc.with_ymd_and_hms(2024, 6, 10, 0, 1, 0).unwrap();
        let ord1 = market_day_boundary(dt1.timestamp() as u64 * 1_000_000);
        guard.record_day_trade(ord1, 1);

        // Day 2: 2024-06-11 23:59:00 UTC (which is June 11 ET)
        let dt2 = Utc.with_ymd_and_hms(2024, 6, 11, 23, 59, 0).unwrap();
        let ord2 = market_day_boundary(dt2.timestamp() as u64 * 1_000_000);
        guard.record_day_trade(ord2, 2);

        // Day 3: 2024-06-12 14:00:00 UTC (which is June 12 ET)
        let dt3 = Utc.with_ymd_and_hms(2024, 6, 12, 14, 0, 0).unwrap();
        let ord3 = market_day_boundary(dt3.timestamp() as u64 * 1_000_000);
        guard.record_day_trade(ord3, 3);

        // Now evaluate on June 14 ET (ord1 was June 9 ET, so difference is 5 days -> drops off!)
        // 2024-06-14 14:00:00 UTC (June 14 ET)
        let dt_eval = Utc.with_ymd_and_hms(2024, 6, 14, 14, 0, 0).unwrap();
        let ord_eval = market_day_boundary(dt_eval.timestamp() as u64 * 1_000_000);

        // We have recorded 3 trades, but one (ord1) was 5 days ago (June 14 - June 9 = 5).
        // Since PDT window is last 5 days (difference < 5), ord1 is excluded.
        // Remaining active trades: ord2 (diff 3), ord3 (diff 2). Total = 2.
        assert_eq!(guard.day_trade_count_last_5_days(ord_eval), 2);

        // Thus, we would NOT violate if we take a 3rd trade today.
        assert!(!guard.would_violate(ord_eval));
    }
}
