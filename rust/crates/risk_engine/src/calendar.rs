//! Calendar Risk — §27.
//! Loads upcoming high-impact events from configs/calendar.toml.
//! Returns Calendar = ON within ±window of each event.
//! Falls back to Calendar = ON all day if file > 7 days stale.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::US::Eastern;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalendarEvent {
    date: String, // "YYYY-MM-DD"
    event: String,
    window_before_min: u64,
    window_after_min: u64,
}

#[derive(Debug, Deserialize)]
struct CalendarFile {
    last_updated: String,
    #[serde(default)]
    events: Vec<CalendarEvent>,
}

pub struct CalendarRisk {
    path: PathBuf,
    events: Vec<CalendarEvent>,
    last_updated: Option<NaiveDate>,
    last_loaded: SystemTime,
}

impl CalendarRisk {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let mut cr = Self {
            path: path.into(),
            events: Vec::new(),
            last_updated: None,
            last_loaded: SystemTime::UNIX_EPOCH,
        };
        cr.load();
        cr
    }

    /// Returns true if calendar risk is active at `ts_micros`.
    /// Falls back to true (conservative) if file is stale.
    pub fn is_active(&self, ts_micros: u64) -> bool {
        // Check file staleness (§27)
        let age = self
            .last_loaded
            .elapsed()
            .unwrap_or(Duration::from_secs(u64::MAX));
        if age > Duration::from_secs(7 * 24 * 3600) {
            log::warn!("Calendar file > 7 days old — assuming Calendar = ON");
            return true;
        }

        let secs = (ts_micros / 1_000_000) as i64;
        let dt_utc = DateTime::<Utc>::from_timestamp(secs, 0).unwrap_or_else(Utc::now);
        let dt_et = dt_utc.with_timezone(&Eastern);
        let today_str = dt_et.format("%Y-%m-%d").to_string();
        let time_et = dt_et.time();

        for event in &self.events {
            if event.date != today_str {
                continue;
            }
            // Find event time — assume market open time as proxy if not specified
            // For simplicity: 08:30 for NFP/CPI, 14:00 for FOMC
            let event_time = self.event_time(&event.event);
            let before_secs = event.window_before_min * 60;
            let after_secs = event.window_after_min * 60;
            let start = event_time - chrono::Duration::seconds(before_secs as i64);
            let end = event_time + chrono::Duration::seconds(after_secs as i64);
            if time_et >= start && time_et <= end {
                log::debug!("Calendar ON: event={} at ET {}", event.event, event_time);
                return true;
            }
        }
        false
    }

    fn event_time(&self, event_name: &str) -> NaiveTime {
        match event_name {
            "NFP" | "CPI" | "PPI" | "GDP" => NaiveTime::from_hms_opt(8, 30, 0).unwrap_or_default(),
            "FOMC" | "FOMC Minutes" => NaiveTime::from_hms_opt(14, 0, 0).unwrap_or_default(),
            "OPEC" => NaiveTime::from_hms_opt(9, 0, 0).unwrap_or_default(),
            _ => NaiveTime::from_hms_opt(9, 30, 0).unwrap_or_default(),
        }
    }

    fn load(&mut self) {
        if !self.path.exists() {
            log::warn!("calendar.toml not found at {:?}", self.path);
            self.last_loaded = SystemTime::now();
            return;
        }
        match std::fs::read_to_string(&self.path) {
            Ok(content) => match toml::from_str::<CalendarFile>(&content) {
                Ok(file) => {
                    self.events = file.events;
                    self.last_updated =
                        NaiveDate::parse_from_str(&file.last_updated, "%Y-%m-%d").ok();
                    self.last_loaded = SystemTime::now();
                    log::info!(
                        "Calendar loaded: {} events (last_updated: {})",
                        self.events.len(),
                        file.last_updated
                    );
                }
                Err(e) => log::error!("Failed to parse calendar.toml: {}", e),
            },
            Err(e) => log::error!("Failed to read calendar.toml: {}", e),
        }
    }
}
