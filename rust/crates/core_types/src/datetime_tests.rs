use super::datetime::*;
use chrono::{TimeZone, Utc};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_2359_utc_maps_to_correct_et_day() {
        // 2024-06-10T23:59:00Z -> 2024-06-10 19:59:00 EDT (same day)
        let dt = Utc.with_ymd_and_hms(2024, 6, 10, 23, 59, 0).unwrap();
        let ts_micros = dt.timestamp() as u64 * 1_000_000;

        let et_ordinal = market_day_boundary(ts_micros);
        // Expecting ordinal for 2024-06-10
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2024, 6, 10)
            .unwrap()
            .signed_duration_since(epoch)
            .num_days() as u32;
        assert_eq!(et_ordinal, expected);
    }

    #[test]
    fn test_0001_utc_maps_to_previous_et_day() {
        // 2024-06-11T00:01:00Z -> 2024-06-10 20:01:00 EDT (previous day)
        let dt = Utc.with_ymd_and_hms(2024, 6, 11, 0, 1, 0).unwrap();
        let ts_micros = dt.timestamp() as u64 * 1_000_000;

        let et_ordinal = market_day_boundary(ts_micros);
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2024, 6, 10)
            .unwrap()
            .signed_duration_since(epoch)
            .num_days() as u32;
        assert_eq!(et_ordinal, expected);
    }

    #[test]
    fn test_dst_spring_forward() {
        // DST starts: 2024-03-10 02:00:00 EST -> 03:00:00 EDT
        // 2024-03-10T06:59:00Z -> 01:59:00 EST
        let dt1 = Utc.with_ymd_and_hms(2024, 3, 10, 6, 59, 0).unwrap();
        let ts_micros1 = dt1.timestamp() as u64 * 1_000_000;
        let et1 = market_day_boundary(ts_micros1);

        // 2024-03-10T07:01:00Z -> 03:01:00 EDT
        let dt2 = Utc.with_ymd_and_hms(2024, 3, 10, 7, 1, 0).unwrap();
        let ts_micros2 = dt2.timestamp() as u64 * 1_000_000;
        let et2 = market_day_boundary(ts_micros2);

        // Both should map to the same ordinal day
        assert_eq!(et1, et2);

        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2024, 3, 10)
            .unwrap()
            .signed_duration_since(epoch)
            .num_days() as u32;
        assert_eq!(et1, expected);
    }

    #[test]
    fn test_dst_fall_back() {
        // DST ends: 2024-11-03 02:00:00 EDT -> 01:00:00 EST
        // 2024-11-03T05:59:00Z -> 01:59:00 EDT
        let dt1 = Utc.with_ymd_and_hms(2024, 11, 3, 5, 59, 0).unwrap();
        let ts_micros1 = dt1.timestamp() as u64 * 1_000_000;
        let et1 = market_day_boundary(ts_micros1);

        // 2024-11-03T06:01:00Z -> 01:01:00 EST
        let dt2 = Utc.with_ymd_and_hms(2024, 11, 3, 6, 1, 0).unwrap();
        let ts_micros2 = dt2.timestamp() as u64 * 1_000_000;
        let et2 = market_day_boundary(ts_micros2);

        // Both should map to the same ordinal day
        assert_eq!(et1, et2);

        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2024, 11, 3)
            .unwrap()
            .signed_duration_since(epoch)
            .num_days() as u32;
        assert_eq!(et1, expected);
    }
}
