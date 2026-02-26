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

use core_types::RejectReason;

pub struct RiskState {
    pub daily_loss_usd: f64,
    pub open_positions: usize,
    pub max_daily_loss: f64,
}

impl RiskState {
    pub fn new(max_daily_loss: f64) -> Self {
        Self {
            daily_loss_usd: 0.0,
            open_positions: 0,
            max_daily_loss,
        }
    }

    pub fn check_entry(&self) -> Result<(), RejectReason> {
        if self.daily_loss_usd <= -self.max_daily_loss {
            // This logic is actually a kill switch, but for illustration:
            return Err(RejectReason::Blocklist); // Placeholder
        }
        Ok(())
    }
}
