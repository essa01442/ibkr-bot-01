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
