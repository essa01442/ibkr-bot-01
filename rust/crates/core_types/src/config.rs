use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub risk: RiskConfig,
    pub universe: UniverseConfig,
    pub tape: TapeConfig,
    pub execution: ExecutionConfig,
    pub pricing: PricingConfig,
    pub regime: RegimeConfig,
    pub session: SessionConfig,
    pub ibkr: IbkrConfig,
    pub context: ContextConfig,
    pub mtf: MtfConfig,
    pub correlation: CorrelationConfig,
    pub watchlist: WatchlistConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WatchlistConfig {
    pub demotion_cycles: u32,
    pub eviction_cycles: u32,
    pub min_quality_score: f64,
    pub min_volume: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExecutionConfig {
    pub cancel_timeout_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RiskConfig {
    pub max_daily_loss_usd: f64,
    pub risk_per_trade_usd: f64,
    pub max_position_pct: f64,
    pub budget_cap_pct: f64,
    pub account_capital_usd: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UniverseConfig {
    pub min_avg_daily_volume: u64,
    pub min_avg_weekly_volume: u64,
    pub min_addv_usd: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TapeConfig {
    pub tape_threshold_normal: f64,
    pub tape_threshold_post_target: f64,
    pub tape_threshold_warm: f64,
    pub weights: TapeWeights,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TapeWeights {
    pub w_r: f64,
    pub w_a: f64,
    pub w_lp: f64,
    pub w_spr: f64,
    pub w_abs: f64,
    pub w_bls: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PricingConfig {
    pub k_atr: f64,
    pub min_stop_pct: f64,
    pub min_stop_abs_usd: f64,
    pub anti_chase_runup_pct: f64,
    pub slippage_alpha: f64,
    pub slippage_beta: f64,
    pub sec_fee_rate: f64,
    pub taf_rate: f64,
    pub commission_per_share: f64,
    pub min_net_profit_usd: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegimeConfig {
    pub atr_normal_max: f64,
    pub atr_caution_max: f64,
    pub breadth_normal_min: f64,
    pub breadth_caution_min: f64,
    pub widening_caution_pct: f64,
    pub widening_riskoff_pct: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SessionConfig {
    pub regular_open_et: String,
    pub trading_start_et: String,
    pub trading_end_et: String,
    pub regular_close_et: String,
    pub pre_after_enabled: bool,
    pub pre_after_min_volume: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IbkrConfig {
    pub subscription_budget: u32,
    pub subscription_warn_pct: f64,
    pub slow_loop_pacing_per_min: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContextConfig {
    pub volume_multiplier_2x: f64,
    pub volume_multiplier_3x: f64,
    pub sector_momentum_min_pct: f64,
    pub churn_window_minutes: u64,
    pub churn_max_move_pct: f64,
    pub snap_min_trade_count: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MtfConfig {
    pub require_all: bool,
    pub stale_data_threshold_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CorrelationConfig {
    pub threshold: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use toml;

    #[test]
    fn test_config_parse() {
        let toml_str = r#"
[risk]
max_daily_loss_usd = 100.0
risk_per_trade_usd = 25.0
max_position_pct = 0.15
budget_cap_pct = 0.20
account_capital_usd = 25000.0

[universe]
min_avg_daily_volume = 2_000_000
min_avg_weekly_volume = 5_000_000
min_addv_usd = 50_000.0

[tape]
tape_threshold_normal = 72.0
tape_threshold_post_target = 82.0
tape_threshold_warm = 67.0

[tape.weights]
w_r = 0.30
w_a = 0.22
w_lp = 0.22
w_spr = 0.13
w_abs = 0.08
w_bls = 0.05

[execution]
cancel_timeout_ms = 5000

[pricing]
k_atr = 2.0
min_stop_pct = 0.012
min_stop_abs_usd = 0.02
anti_chase_runup_pct = 0.02
slippage_alpha = 0.5
slippage_beta = 0.3
sec_fee_rate = 0.0000278
taf_rate = 0.000166
commission_per_share = 0.005
min_net_profit_usd = 0.10

[regime]
atr_normal_max = 0.0018
atr_caution_max = 0.0028
breadth_normal_min = 0.45
breadth_caution_min = 0.35
widening_caution_pct = 0.25
widening_riskoff_pct = 0.50

[session]
regular_open_et = "09:30"
trading_start_et = "09:45"
trading_end_et = "15:45"
regular_close_et = "16:00"
pre_after_enabled = false
pre_after_min_volume = 100000

[ibkr]
subscription_budget = 80
subscription_warn_pct = 0.80
slow_loop_pacing_per_min = 30

[context]
volume_multiplier_2x = 2.0
volume_multiplier_3x = 3.0
sector_momentum_min_pct = 2.0
churn_window_minutes = 10
churn_max_move_pct = 0.01
snap_min_trade_count = 5

[mtf]
require_all = true
stale_data_threshold_ms = 3600000

[correlation]
threshold = 0.40

[watchlist]
demotion_cycles = 3
eviction_cycles = 5
min_quality_score = 45.0
min_volume = 500000
"#;
        let config: AppConfig = toml::from_str(toml_str).expect("config must parse");

        // Verify existing fields
        assert_eq!(config.risk.max_daily_loss_usd, 100.0);
        assert_eq!(config.tape.tape_threshold_normal, 72.0);
        assert_eq!(config.tape.tape_threshold_post_target, 82.0);
        assert!((config.tape.weights.w_r - 0.30).abs() < 1e-9);

        // Verify new fields based on the updated request
        assert_eq!(config.pricing.k_atr, 2.0);
        assert_eq!(config.pricing.sec_fee_rate, 0.0000278);
        assert_eq!(config.regime.atr_normal_max, 0.0018);
        assert_eq!(config.session.pre_after_enabled, false);
        assert_eq!(config.ibkr.subscription_budget, 80);
        assert_eq!(config.correlation.threshold, 0.40);
        assert_eq!(config.context.churn_window_minutes, 10);
        assert_eq!(config.context.snap_min_trade_count, 5);
        assert_eq!(config.watchlist.demotion_cycles, 3);
        assert_eq!(config.watchlist.eviction_cycles, 5);
        assert_eq!(config.watchlist.min_quality_score, 45.0);
        assert_eq!(config.watchlist.min_volume, 500000);
    }

    #[test]
    fn test_load_default_config_file() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        // Adjust path to point to configs/default.toml relative to crate root
        // CARGO_MANIFEST_DIR points to rust/crates/core_types
        // configs/ is at repo root
        let config_path = PathBuf::from(manifest_dir)
            .parent() // crates
            .unwrap()
            .parent() // rust
            .unwrap()
            .parent() // repo root
            .unwrap()
            .join("configs/default.toml");

        let toml_str = fs::read_to_string(config_path).expect("Failed to read config file");
        let config: AppConfig = toml::from_str(&toml_str).expect("Failed to parse default.toml");

        // Smoke test a few values
        assert_eq!(config.risk.max_daily_loss_usd, 100.0);
        assert_eq!(config.pricing.k_atr, 2.0);
        assert_eq!(config.regime.atr_normal_max, 0.0018);
        assert_eq!(config.session.regular_open_et, "09:30");
    }
}
