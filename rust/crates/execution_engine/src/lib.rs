//! Execution Engine Crate (OMS).
//!
//! Manages Order State, Fills, Timeouts, and Idempotency.
//! Handles Bracket Orders and Trailing Stops.

use core_types::SymbolId;
use std::collections::HashMap;

pub struct OrderManagementSystem {
    pub open_orders: HashMap<String, OrderState>,
}

pub enum OrderState {
    Pending,
    Live,
    Filled,
    Cancelled,
}

impl OrderManagementSystem {
    pub fn new() -> Self {
        Self { open_orders: HashMap::new() }
    }

    pub fn submit_order(&mut self, _symbol: SymbolId, _qty: u32, _price: f64) {
        // Logic to send order via Bridge
    }
}
