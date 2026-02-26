//! Risk Engine Crate.
//!
//! Enforces pre-trade risk checks, Loss Ladders, and Kill Switches.
//! This is the final authority before an order is sent to the OMS.
//!
//! # Responsibilities
//! - Max Daily Loss
//! - Position Limits
//! - Exposure / Correlation checks (using data from SlowLoop)
//! - PDT Rules
//! - Liquidity Validation

use core_types::{RejectReason, SymbolId, CorporateAction, LiquidityConfig};
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs::File;
use std::io::BufReader;

pub mod guards;
pub mod sizing;
pub mod exposure;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLadderStep {
    pub profit_threshold: f64,
    pub max_daily_loss: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskState {
    pub daily_pnl_usd: f64, // Realized + Unrealized PnL. Negative means loss.
    pub open_positions: usize,
    pub initial_max_daily_loss: f64,
    pub current_max_daily_loss: f64,
    pub corporate_actions: HashMap<SymbolId, CorporateAction>,
    pub blocklist: HashSet<SymbolId>,
    pub liquidity_config: LiquidityConfig,
    pub monitor_only: bool,
    pub risk_ladder: Vec<RiskLadderStep>,
    #[serde(skip)] // Do not persist exposure cache
    pub exposure_validator: exposure::ExposureValidator,
}

impl RiskState {
    pub fn new(max_daily_loss: f64, liquidity_config: LiquidityConfig) -> Self {
        Self {
            daily_pnl_usd: 0.0,
            open_positions: 0,
            initial_max_daily_loss: max_daily_loss,
            current_max_daily_loss: max_daily_loss,
            corporate_actions: HashMap::new(),
            blocklist: HashSet::new(),
            liquidity_config,
            monitor_only: false,
            risk_ladder: Vec::new(),
            exposure_validator: exposure::ExposureValidator::new(),
        }
    }

    pub fn set_risk_ladder(&mut self, ladder: Vec<RiskLadderStep>) {
        self.risk_ladder = ladder;
        // Sort by profit threshold just in case
        self.risk_ladder.sort_by(|a, b| a.profit_threshold.partial_cmp(&b.profit_threshold).unwrap_or(std::cmp::Ordering::Equal));
        self.update_risk_limits();
    }

    pub fn update_pnl(&mut self, pnl: f64) {
        self.daily_pnl_usd = pnl;
        self.update_risk_limits();
    }

    fn update_risk_limits(&mut self) {
        // Iterate ladder to find the highest threshold we've crossed
        let mut calculated_limit = self.initial_max_daily_loss;

        for step in &self.risk_ladder {
            if self.daily_pnl_usd >= step.profit_threshold {
                calculated_limit = step.max_daily_loss;
            }
        }

        // Ratcheting logic: Only allow max loss to tighten (decrease).
        // e.g. 100 -> 50 -> 0 -> -50.
        if calculated_limit < self.current_max_daily_loss {
            self.current_max_daily_loss = calculated_limit;
        }
    }

    pub fn set_corporate_action(&mut self, symbol_id: SymbolId, action: CorporateAction) {
        self.corporate_actions.insert(symbol_id, action);
        if action == CorporateAction::Block {
            self.blocklist.insert(symbol_id);
        }
    }

    pub fn block_symbol(&mut self, symbol_id: SymbolId) {
        self.blocklist.insert(symbol_id);
    }

    pub fn unblock_symbol(&mut self, symbol_id: SymbolId) {
        if let Some(&CorporateAction::Block) = self.corporate_actions.get(&symbol_id) {
            // Cannot unblock if Corporate Action is Block.
        } else {
            self.blocklist.remove(&symbol_id);
        }
    }

    pub fn set_monitor_only(&mut self, monitor_only: bool) {
        self.monitor_only = monitor_only;
    }

    pub fn check_entry(&self, symbol_id: SymbolId, open_symbols: &[SymbolId]) -> Result<(), RejectReason> {
        if self.monitor_only {
            return Err(RejectReason::MonitorOnly);
        }

        if self.blocklist.contains(&symbol_id) {
            // Distinguish reason
            if let Some(&CorporateAction::Block) = self.corporate_actions.get(&symbol_id) {
                return Err(RejectReason::CorporateActionBlock);
            }
            return Err(RejectReason::Blocklist);
        }

        if self.should_terminate() {
            return Err(RejectReason::MaxDailyLoss);
        }

        // Check Exposure
        self.exposure_validator.check_new_position(symbol_id, open_symbols)?;

        Ok(())
    }

    pub fn should_terminate(&self) -> bool {
        // If current_max_daily_loss is 100, we stop at -100.
        // If current_max_daily_loss is -50 (protect profit), we stop at +50.
        // Logic: pnl <= -current_max_daily_loss
        self.daily_pnl_usd <= -self.current_max_daily_loss
    }

    /// Validates if the trade parameters meet liquidity requirements.
    /// This is typically called with current market data snapshot values.
    pub fn check_liquidity(&self, price: f64, spread_pct: f64, avg_daily_volume: u64, addv_usd: f64) -> Result<(), RejectReason> {
        if price < self.liquidity_config.target_price_min || price > self.liquidity_config.target_price_max {
            return Err(RejectReason::PriceRange);
        }

        if spread_pct > self.liquidity_config.max_spread_pct {
            return Err(RejectReason::Liquidity);
        }

        if avg_daily_volume < self.liquidity_config.min_avg_daily_volume {
             return Err(RejectReason::Liquidity);
        }

        if addv_usd < self.liquidity_config.min_addv_usd {
             return Err(RejectReason::Liquidity);
        }

        Ok(())
    }

    /// Rebuilds risk state from broker positions.
    /// This should be called during the Reconnect phase.
    /// Note: Does not reconstruct daily PnL automatically unless provided.
    pub fn rebuild_state(&mut self, positions: Vec<core_types::PositionData>) {
        self.open_positions = positions.len();
        // In a real system, we might query Realized PnL from the broker here.
        // For now, we assume local state (persistence) is the source of truth for PnL,
        // and we only sync open position count for exposure checks.

        // Also update exposure validator if needed (not fully mapped here without symbol details)
    }

    // Persistence
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let file = File::create(path)?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let state = serde_json::from_reader(reader)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn default_risk_state() -> RiskState {
        RiskState::new(100.0, LiquidityConfig::default())
    }

    #[test]
    fn test_corporate_action_block() {
        let mut risk = default_risk_state();
        let symbol = SymbolId(1);

        // Initially allowed (implicitly)
        assert!(risk.check_entry(symbol, &[]).is_ok());

        // Set to Watch - should still be allowed
        risk.set_corporate_action(symbol, CorporateAction::Watch);
        assert!(risk.check_entry(symbol, &[]).is_ok());
        assert!(!risk.blocklist.contains(&symbol));

        // Set to Block - should be blocked
        risk.set_corporate_action(symbol, CorporateAction::Block);
        assert!(risk.blocklist.contains(&symbol));

        match risk.check_entry(symbol, &[]) {
            Err(RejectReason::CorporateActionBlock) => (),
            _ => panic!("Expected CorporateActionBlock"),
        }
    }

    #[test]
    fn test_manual_block() {
        let mut risk = default_risk_state();
        let symbol = SymbolId(2);

        risk.block_symbol(symbol);
        match risk.check_entry(symbol, &[]) {
            Err(RejectReason::Blocklist) => (),
            _ => panic!("Expected Blocklist"),
        }
    }

    #[test]
    fn test_monitor_only() {
        let mut risk = default_risk_state();
        let symbol = SymbolId(1);

        assert!(risk.check_entry(symbol, &[]).is_ok());

        risk.set_monitor_only(true);
        match risk.check_entry(symbol, &[]) {
            Err(RejectReason::MonitorOnly) => (),
            _ => panic!("Expected MonitorOnly"),
        }

        risk.set_monitor_only(false);
        assert!(risk.check_entry(symbol, &[]).is_ok());
    }

    #[test]
    fn test_max_daily_loss() {
        let mut risk = default_risk_state(); // Max loss 100
        let symbol = SymbolId(1);

        risk.update_pnl(-50.0);
        assert!(risk.check_entry(symbol, &[]).is_ok());


        risk.update_pnl(-50.0);
        assert!(risk.check_entry(symbol, &[]).is_ok());

        risk.update_pnl(-100.0);
        match risk.check_entry(symbol, &[]) {
            Err(RejectReason::MaxDailyLoss) => (),
            _ => panic!("Expected MaxDailyLoss at -100"),
        }
        assert!(risk.should_terminate());

        risk.update_pnl(-101.0);
        match risk.check_entry(symbol, &[]) {
            Err(RejectReason::MaxDailyLoss) => (),
            _ => panic!("Expected MaxDailyLoss at -101"),
        }
        assert!(risk.should_terminate());
    }

    #[test]
    fn test_risk_ladder() {
        let mut risk = default_risk_state(); // Max loss 100
        // Ladder:
        // At 50 profit, max loss becomes 50 (stop at -50).
        // At 100 profit, max loss becomes 0 (stop at 0).
        // At 200 profit, max loss becomes -50 (stop at +50).
        let ladder = vec![
            RiskLadderStep { profit_threshold: 50.0, max_daily_loss: 50.0 },
            RiskLadderStep { profit_threshold: 100.0, max_daily_loss: 0.0 },
            RiskLadderStep { profit_threshold: 200.0, max_daily_loss: -50.0 },
        ];
        risk.set_risk_ladder(ladder);

        // Scenario 1: No profit
        risk.update_pnl(0.0);
        assert!(!risk.should_terminate()); // stop at -100
        risk.update_pnl(-90.0);
        assert!(!risk.should_terminate());
        risk.update_pnl(-100.0);
        assert!(risk.should_terminate());

        // Scenario 2: +60 profit (crossed 50 threshold) -> max loss 50 (stop at -50)
        risk.update_pnl(60.0);
        assert!(!risk.should_terminate());
        // Drop to -40
        risk.update_pnl(-40.0);
        assert!(!risk.should_terminate());
        // Drop to -50
        risk.update_pnl(-50.0);
        assert!(risk.should_terminate());

        // Scenario 3: +150 profit (crossed 100 threshold) -> max loss 0 (stop at 0)
        risk.update_pnl(150.0);
        assert!(!risk.should_terminate());
        // Drop to 10
        risk.update_pnl(10.0);
        assert!(!risk.should_terminate());
        // Drop to 0
        risk.update_pnl(0.0);
        assert!(risk.should_terminate());

        // Scenario 4: +250 profit (crossed 200 threshold) -> max loss -50 (stop at +50)
        risk.update_pnl(250.0);
        assert!(!risk.should_terminate());
        // Drop to 60
        risk.update_pnl(60.0);
        assert!(!risk.should_terminate());
        // Drop to 50
        risk.update_pnl(50.0);
        assert!(risk.should_terminate());
    }

    #[test]
    fn test_persistence() {
        let mut risk = default_risk_state();
        risk.update_pnl(123.45);
        risk.set_monitor_only(true);
        let ladder = vec![RiskLadderStep { profit_threshold: 100.0, max_daily_loss: 50.0 }];
        risk.set_risk_ladder(ladder);

        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        risk.save_to_file(path).unwrap();

        let loaded = RiskState::load_from_file(path).unwrap();
        assert_eq!(loaded.daily_pnl_usd, 123.45);
        assert_eq!(loaded.monitor_only, true);
        assert_eq!(loaded.risk_ladder.len(), 1);
        assert_eq!(loaded.risk_ladder[0].profit_threshold, 100.0);
    }
}
