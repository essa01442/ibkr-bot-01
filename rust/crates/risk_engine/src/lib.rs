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

pub mod guards;
pub mod sizing;
pub struct RiskState {
    pub daily_loss_usd: f64,
    pub open_positions: usize,
    pub max_daily_loss: f64,
    pub corporate_actions: HashMap<SymbolId, CorporateAction>,
    pub blocklist: HashSet<SymbolId>,
    pub liquidity_config: LiquidityConfig,
}

impl RiskState {
    pub fn new(max_daily_loss: f64, liquidity_config: LiquidityConfig) -> Self {
        Self {
            daily_loss_usd: 0.0,
            open_positions: 0,
            max_daily_loss,
            corporate_actions: HashMap::new(),
            blocklist: HashSet::new(),
            liquidity_config,
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

    pub fn check_entry(&self, symbol_id: SymbolId) -> Result<(), RejectReason> {
        if self.blocklist.contains(&symbol_id) {
            // Distinguish reason
            if let Some(&CorporateAction::Block) = self.corporate_actions.get(&symbol_id) {
                return Err(RejectReason::CorporateActionBlock);
            }
            return Err(RejectReason::Blocklist);
        }

        if self.daily_loss_usd <= -self.max_daily_loss {
            return Err(RejectReason::DailyContext); // Using DailyContext as generic 'Stop Trading' reason for now
        }
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_risk_state() -> RiskState {
        RiskState::new(1000.0, LiquidityConfig::default())
    }

    #[test]
    fn test_corporate_action_block() {
        let mut risk = default_risk_state();
        let symbol = SymbolId(1);

        // Initially allowed (implicitly)
        assert!(risk.check_entry(symbol).is_ok());

        // Set to Watch - should still be allowed
        risk.set_corporate_action(symbol, CorporateAction::Watch);
        assert!(risk.check_entry(symbol).is_ok());
        assert!(!risk.blocklist.contains(&symbol));

        // Set to Block - should be blocked
        risk.set_corporate_action(symbol, CorporateAction::Block);
        assert!(risk.blocklist.contains(&symbol));

        match risk.check_entry(symbol) {
            Err(RejectReason::CorporateActionBlock) => (),
            _ => panic!("Expected CorporateActionBlock"),
        }
    }

    #[test]
    fn test_manual_block() {
        let mut risk = default_risk_state();
        let symbol = SymbolId(2);

        risk.block_symbol(symbol);
        match risk.check_entry(symbol) {
            Err(RejectReason::Blocklist) => (),
            _ => panic!("Expected Blocklist"),
        }
    }

    #[test]
    fn test_liquidity_validation() {
        let config = LiquidityConfig {
            target_price_min: 1.0,
            target_price_max: 20.0,
            max_spread_pct: 0.05,
            min_avg_daily_volume: 500_000,
            min_addv_usd: 1_000_000.0,
        };
        let risk = RiskState::new(1000.0, config);

        // Valid case
        assert!(risk.check_liquidity(10.0, 0.01, 600_000, 2_000_000.0).is_ok());

        // Invalid Price (Low)
        match risk.check_liquidity(0.5, 0.01, 600_000, 2_000_000.0) {
            Err(RejectReason::PriceRange) => (),
            _ => panic!("Expected PriceRange error (low)"),
        }

        // Invalid Price (High)
        match risk.check_liquidity(25.0, 0.01, 600_000, 2_000_000.0) {
            Err(RejectReason::PriceRange) => (),
            _ => panic!("Expected PriceRange error (high)"),
        }

        // Invalid Spread
        match risk.check_liquidity(10.0, 0.06, 600_000, 2_000_000.0) {
            Err(RejectReason::Liquidity) => (),
            _ => panic!("Expected Liquidity error (spread)"),
        }

        // Invalid Volume
        match risk.check_liquidity(10.0, 0.01, 400_000, 2_000_000.0) {
            Err(RejectReason::Liquidity) => (),
            _ => panic!("Expected Liquidity error (volume)"),
        }

        // Invalid ADDV
        match risk.check_liquidity(10.0, 0.01, 600_000, 500_000.0) {
            Err(RejectReason::Liquidity) => (),
            _ => panic!("Expected Liquidity error (addv)"),
        }
    }
}
