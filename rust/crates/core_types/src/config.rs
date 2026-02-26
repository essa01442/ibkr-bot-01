use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub risk: RiskConfig,
    pub universe: UniverseConfig,
    pub tape: TapeConfig,
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

#[cfg(test)]
mod tests {
    use super::*;
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
"#;
        let config: AppConfig = toml::from_str(toml_str).expect("config must parse");
        assert_eq!(config.risk.max_daily_loss_usd, 100.0);
        assert_eq!(config.tape.tape_threshold_normal, 72.0);
        assert_eq!(config.tape.tape_threshold_post_target, 82.0);
        assert!((config.tape.weights.w_r - 0.30).abs() < 1e-9);
        let weight_sum = config.tape.weights.w_r
            + config.tape.weights.w_a
            + config.tape.weights.w_lp
            + config.tape.weights.w_spr
            + config.tape.weights.w_abs
            + config.tape.weights.w_bls;
        assert!(
            (weight_sum - 1.0).abs() < 1e-9,
            "weights must sum to 1.0, got {}",
            weight_sum
        );
    }
}
