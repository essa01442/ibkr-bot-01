//! Session Timing Guard per §25.
//! Enforces trading windows: no entry before 09:45 ET, no entry after 15:45 ET.
//! Pre/After-Hours allowed only if config.session.pre_after_enabled = true.

use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::US::Eastern;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Before pre-market or after after-hours — no activity
    Closed,
    /// Pre-market (04:00–09:30 ET) — Monitor Only unless pre_after_enabled
    PreMarket,
    /// Open volatility window (09:30–09:45 ET) — Monitor Only always
    OpenVolatility,
    /// Regular trading window (09:45–15:45 ET) — full trading
    TradingHours,
    /// Closing volatility window (15:45–16:00 ET) — no new entries
    CloseVolatility,
    /// After-hours (16:00–20:00 ET) — Monitor Only unless pre_after_enabled
    AfterHours,
}

pub struct SessionGuard {
    pub pre_after_enabled: bool,
}

impl SessionGuard {
    pub fn new(pre_after_enabled: bool) -> Self {
        Self { pre_after_enabled }
    }

    /// Returns the current session state based on the given UTC timestamp (microseconds).
    pub fn session_state(&self, ts_micros: u64) -> SessionState {
        let secs = (ts_micros / 1_000_000) as i64;
        let dt_utc = DateTime::<Utc>::from_timestamp(secs, 0).unwrap_or_else(Utc::now);
        let dt_et = dt_utc.with_timezone(&Eastern);
        let t = dt_et.time();

        let t0930 = NaiveTime::from_hms_opt(9, 30, 0).unwrap_or_default();
        let t0945 = NaiveTime::from_hms_opt(9, 45, 0).unwrap_or_default();
        let t1545 = NaiveTime::from_hms_opt(15, 45, 0).unwrap_or_default();
        let t1600 = NaiveTime::from_hms_opt(16, 0, 0).unwrap_or_default();
        let t2000 = NaiveTime::from_hms_opt(20, 0, 0).unwrap_or_default();
        let t0400 = NaiveTime::from_hms_opt(4, 0, 0).unwrap_or_default();

        if t < t0400 || t >= t2000 {
            SessionState::Closed
        } else if t < t0930 {
            SessionState::PreMarket
        } else if t < t0945 {
            SessionState::OpenVolatility
        } else if t < t1545 {
            SessionState::TradingHours
        } else if t < t1600 {
            SessionState::CloseVolatility
        } else {
            SessionState::AfterHours
        }
    }

    /// Returns true if a new entry is allowed right now.
    ///
    /// # Example
    /// ```
    /// use risk_engine::session::SessionGuard;
    /// let guard = SessionGuard::new(false);
    /// // Example: 2026-03-10 10:00 ET = 15:00 UTC
    /// let ts_trading = 1741615200_u64 * 1_000_000;
    /// assert!(guard.entry_allowed(ts_trading));
    /// ```
    pub fn entry_allowed(&self, ts_micros: u64) -> bool {
        match self.session_state(ts_micros) {
            SessionState::TradingHours => true,
            SessionState::PreMarket | SessionState::AfterHours => self.pre_after_enabled,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_states() {
        let guard = SessionGuard::new(false);
        // 2026-03-10 10:00 ET = 15:00 UTC
        let ts_trading = 1741615200_u64 * 1_000_000;
        assert_eq!(guard.session_state(ts_trading), SessionState::TradingHours);
        assert!(guard.entry_allowed(ts_trading));

        // 2026-03-10 09:35 ET = 14:35 UTC
        let ts_open_vol = 1741613700_u64 * 1_000_000;
        assert_eq!(
            guard.session_state(ts_open_vol),
            SessionState::OpenVolatility
        );
        assert!(!guard.entry_allowed(ts_open_vol));
    }
}
