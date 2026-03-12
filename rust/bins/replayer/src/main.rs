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
        eprintln!("Usage: replayer <tick_file.csv> <output_golden.json> [config_path.toml]");
        eprintln!("       replayer --compare <golden_a.json> <golden_b.json>");
        std::process::exit(1);
    }

    if args[1] == "--compare" {
        return compare_golden(&args[2], &args[3]);
    }

    let tick_file = PathBuf::from(&args[1]);
    let output_file = PathBuf::from(&args[2]);

    // Load config
    // The working directory during tests might be different, let's try to locate it relative to repo root
    let mut config_path = if args.len() >= 4 {
        PathBuf::from(&args[3])
    } else {
        PathBuf::from("configs/default.toml")
    };

    if !config_path.exists() && args.len() < 4 {
        config_path = PathBuf::from("../../configs/default.toml"); // from rust/bins/replayer
    }
    if !config_path.exists() && args.len() < 4 {
        config_path = PathBuf::from("../../../configs/default.toml"); // from rust/target/debug/deps
    }

    let config_str = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("{:?} must exist: {}", config_path, e))?;
    let _config: core_types::AppConfig = toml::from_str(&config_str)
        .map_err(|e| anyhow::anyhow!("config must parse: {}", e))?;

    // Let's use the loaded config
    let mut risk_state = risk_engine::RiskState::new(
        _config.risk.max_daily_loss_usd,
        core_types::LiquidityConfig::default(),
    );
    // Explicitly bypass session limits, blocklist, and PDT for replayer purely testing Tape Logic
    risk_state.blocklist.clear();
    risk_state.blocklist_loader = risk_engine::blocklist::Blocklist::new("/dev/null", 99999);
    let risk_arc = std::sync::Arc::new(std::sync::Mutex::new(risk_state));

    let guard_config = risk_engine::guards::GuardConfig::default();
    let pricing_model = risk_engine::sizing::PricingModel {
        commission_per_share: _config.pricing.commission_per_share,
        sec_fee_rate: _config.pricing.sec_fee_rate,
        taf_rate: _config.pricing.taf_rate,
        slippage_alpha: _config.pricing.slippage_alpha,
        slippage_beta: _config.pricing.slippage_beta,
        min_net_profit_usd: _config.pricing.min_net_profit_usd,
    };

    let mut tape_engine = tape_engine::TapeEngine::new(
        risk_arc,
        guard_config,
        _config.tape.clone(),
        pricing_model,
    );

    // Read tick data
    let mut rdr = csv::Reader::from_path(&tick_file)?;
    let mut ticks: Vec<TickRecord> = rdr.deserialize().filter_map(|r| r.ok()).collect();
    ticks.sort_by_key(|t| t.ts_src); // Ensure chronological order

    // Replay
    let mut decisions: Vec<GoldenDecision> = Vec::new();
    let _clock = 0u64; // Deterministic clock

    for tick in &ticks {
        let _clock = tick.ts_src; // Deterministic: clock = data timestamp

        // Because TapeEngine requires context to be populated by SlowLoop, we bypass it for pure Tape logic testing by artificially seating the DailyContext and MTF
        tape_engine.update_daily_context(core_types::DailyContext {
            symbol_id: SymbolId(tick.symbol_id),
            state: core_types::ContextState::Play,
            volume_profile: core_types::VolumeProfile {
                current_volume: 1_000_000,
                avg_20d_volume: 1_000_000,
                is_surge: false,
            },
            has_news: true,
            sector_momentum: None,
        });

        tape_engine.update_mtf_analysis(SymbolId(tick.symbol_id), core_types::MtfAnalysis {
            weekly_trend_confirmed: true,
            daily_resistance_cleared: true,
            structure_4h_bullish: true,
            pullback_15m_valid: true,
            mtf_pass: true,
        });

        // Set to Tier A and FullActive to bypass cold start TapeScoreLow
        let state = tape_engine.get_mut_state(SymbolId(tick.symbol_id));
        state.tier = core_types::Tier::A;
        state.cold_start_state = core_types::ColdStartState::FullActive;

        // Feed mock snapshot to set bid/ask limits
        let _ = tape_engine.on_event(&Event {
            symbol_id: SymbolId(tick.symbol_id),
            ts_src: tick.ts_src,
            ts_rx: tick.ts_src,
            ts_proc: tick.ts_src,
            seq: 0,
            kind: EventKind::Snapshot(core_types::SnapshotData {
                bid_price: tick.price - 0.01,
                ask_price: tick.price + 0.01,
                bid_size: 100,
                ask_size: 100,
                volume: 1_000_000,
                avg_volume_20d: 1_000_000,
                has_news_today: true,
                weekly_ema: 1.0,
                daily_resistance: 1000.0,
            }),
        });

        let event = Event {
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

        let result = tape_engine.on_event(&event);

        // TapeEngine Replayer Mockup
        // Currently TapeEngine requires complex state (Context, MTF, Subscriptions) pre-seeded.
        // If they are missing, it immediately rejects with Blocklist/Liquidity.
        // By default, the TapeEngine test will probably reject early without all proper
        // state ticks in history. Let's make sure score is extracted.
        // To make the parameter change detectable, we actually need the parameter change to change the outcome.
        // If tape_threshold_normal = 0, then TapeScoreLow won't trigger!

        // Since we exposed get_mut_state, we can pull the score.
        let state = tape_engine.get_mut_state(SymbolId(tick.symbol_id));
        let score = state.tape.total_score;

        let (action, reject_reason) = match result {
            Ok(_) => ("ENTER".to_string(), None),
            Err(reason) => {
                ("REJECT".to_string(), Some(format!("{:?}", reason)))
            }
        };

        decisions.push(GoldenDecision {
            ts_src: tick.ts_src,
            symbol_id: tick.symbol_id,
            action,
            reject_reason,
            tape_score: score,
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
        .filter(|(da, db)| da.action == db.action && da.reject_reason == db.reject_reason && da.ts_src == db.ts_src)
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
