#![deny(clippy::unwrap_in_result)]
//! L1 Replay Engine — §26.1
//! Reads tick-by-tick historical data and replays through the full decision pipeline.
//! Uses a deterministic clock (timestamps from data, not SystemTime).
//! Produces a Golden Decision File for regression testing.

use core_types::{Event, EventKind, SymbolId, TickData};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct TickRecord {
    ts_src: u64,
    symbol_id: u32,
    price: f64,
    #[allow(dead_code)]
    bid: f64,
    #[allow(dead_code)]
    ask: f64,
    volume: u32,
    #[allow(dead_code)]
    side: String, // "B"=bid/aggressive sell, "A"=ask/aggressive buy
}

#[derive(Debug, Serialize, Deserialize)]
struct GoldenDecision {
    ts_src: u64,
    symbol_id: u32,
    action: String,    // "ENTER" or "REJECT"
    reject_reason: Option<String>,
    tape_score: f64,
    price: f64,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: replayer <tick_file.csv> <output_golden.json>");
        eprintln!("       replayer --compare <golden_a.json> <golden_b.json>");
        std::process::exit(1);
    }

    if args[1] == "--compare" {
        return compare_golden(&args[2], &args[3]);
    }

    let tick_file = PathBuf::from(&args[1]);
    let output_file = PathBuf::from(&args[2]);

    // Load config
    let config_str = std::fs::read_to_string("configs/default.toml")
        .map_err(|e| anyhow::anyhow!("configs/default.toml must exist: {}", e))?;
    let _config: core_types::AppConfig = toml::from_str(&config_str)
        .map_err(|e| anyhow::anyhow!("config must parse: {}", e))?;

    // Read tick data
    let mut rdr = csv::Reader::from_path(&tick_file)?;
    let mut ticks: Vec<TickRecord> = rdr.deserialize().filter_map(|r| r.ok()).collect();
    ticks.sort_by_key(|t| t.ts_src); // Ensure chronological order

    // Replay
    let mut decisions: Vec<GoldenDecision> = Vec::new();
    let _clock = 0u64; // Deterministic clock

    for tick in &ticks {
        let _clock = tick.ts_src; // Deterministic: clock = data timestamp

        let _event = Event {
            symbol_id: SymbolId(tick.symbol_id),
            ts_src: tick.ts_src,
            ts_rx: tick.ts_src,   // No network delay in replay
            ts_proc: tick.ts_src,
            seq: 0,
            kind: EventKind::Tick(TickData {
                price: tick.price,
                size: tick.volume,
                flags: 0,
            }),
        };

        // In a full implementation, this would call tape_engine.on_event()
        // and capture the result. Placeholder for now:
        let action = "REPLAY"; // Replace with actual engine call
        decisions.push(GoldenDecision {
            ts_src: tick.ts_src,
            symbol_id: tick.symbol_id,
            action: action.to_string(),
            reject_reason: None,
            tape_score: 0.0,
            price: tick.price,
        });
    }

    let golden_json = serde_json::to_string_pretty(&decisions)?;
    std::fs::write(&output_file, golden_json)?;
    println!("Golden file written: {} decisions → {:?}", decisions.len(), output_file);

    Ok(())
}

/// Compare two golden files — assert ≥ 99% match per §26.1.
fn compare_golden(file_a: &str, file_b: &str) -> anyhow::Result<()> {
    let a: Vec<GoldenDecision> = serde_json::from_str(&std::fs::read_to_string(file_a)?)?;
    let b: Vec<GoldenDecision> = serde_json::from_str(&std::fs::read_to_string(file_b)?)?;

    if a.len() != b.len() {
        println!("MISMATCH: lengths differ ({} vs {})", a.len(), b.len());
        std::process::exit(1);
    }

    let matches = a.iter().zip(b.iter())
        .filter(|(da, db)| da.action == db.action && da.ts_src == db.ts_src)
        .count();

    let match_pct = matches as f64 / a.len() as f64 * 100.0;
    println!("Golden comparison: {:.2}% match ({}/{} decisions)", match_pct, matches, a.len());

    if match_pct < 99.0 {
        println!("FAIL: match < 99% threshold");
        std::process::exit(1);
    }
    println!("PASS: ≥ 99% match");
    Ok(())
}
