use chrono::{TimeZone, Utc};
use chrono_tz::US::Eastern;

/// Converts a UTC timestamp in microseconds into a single source-of-truth ordinal day
/// representing the market day in US Eastern Time (EST/EDT).
///
/// This handles DST transitions correctly and guarantees that T+1 settlements
/// and PDT calculations use the same localized day boundaries.
pub fn market_day_boundary(ts_micros: u64) -> u32 {
    let secs = (ts_micros / 1_000_000) as i64;
    let nsecs = ((ts_micros % 1_000_000) * 1000) as u32;

    if let Some(utc_dt) = Utc.timestamp_opt(secs, nsecs).single() {
        let et_dt = utc_dt.with_timezone(&Eastern);
        // Calculate days since unix epoch for the ET date (ordinal day)
        // A simple way is to use the ordinal day of the year combined with the year,
        // but to get a continuous sequence of days since epoch, we can take the naive date
        // and calculate the number of days since 1970-01-01.
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let days_since_epoch = et_dt.date_naive().signed_duration_since(epoch).num_days();
        days_since_epoch as u32
    } else {
        0
    }
}
