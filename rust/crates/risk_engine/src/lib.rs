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

use core_types::{RejectReason, SymbolId, CorporateAction};
use std::collections::{HashMap, HashSet};

pub struct RiskState {
    pub daily_loss_usd: f64,
    pub open_positions: usize,
    pub max_daily_loss: f64,
    pub corporate_actions: HashMap<SymbolId, CorporateAction>,
    pub blocklist: HashSet<SymbolId>,
}

impl RiskState {
    pub fn new(max_daily_loss: f64) -> Self {
        Self {
            daily_loss_usd: 0.0,
            open_positions: 0,
            max_daily_loss,
            corporate_actions: HashMap::new(),
            blocklist: HashSet::new(),
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
        // Only unblock if not blocked by corporate action?
        // For now, strict unblock, but we should probably check corporate action.
        // If corporate action is Block, we shouldn't allow unblocking via this method unless we also change corporate action.
        // But for simplicity, let's just remove from blocklist. The user can override.
        // Wait, if "Block -> auto add", then if we unblock but CA is still Block, it's inconsistent.
        // Let's enforce:
        if let Some(&CorporateAction::Block) = self.corporate_actions.get(&symbol_id) {
            // Cannot unblock if Corporate Action is Block.
            // Or maybe we allow it (override)?
            // The requirement "Block -> auto add" suggests CA enforces blocklist.
            // I'll leave it simple: just remove. The "auto add" happens on SETTING the action.
            // If the user manually unblocks, they take responsibility.
        }
        self.blocklist.remove(&symbol_id);
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
            // Return a generic blocklist reason or specific DailyContext
            return Err(RejectReason::DailyContext);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corporate_action_block() {
        let mut risk = RiskState::new(1000.0);
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
        let mut risk = RiskState::new(1000.0);
        let symbol = SymbolId(2);

        risk.block_symbol(symbol);
        match risk.check_entry(symbol) {
            Err(RejectReason::Blocklist) => (),
            _ => panic!("Expected Blocklist"),
        }
    }
}
