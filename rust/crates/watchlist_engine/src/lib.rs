//! Watchlist Engine Crate (Slow Loop).
//!
//! Manages the tiered watchlist system (Tier A, Tier B, Tier C).
//! Handles pacing, upgrades, downgrades, and slow-moving context analysis (MTF, Correlation).

use core_types::{SymbolId, Tier, SubscriptionStatus, ColdStartState};
use core_types::{SymbolId, Tier, SubscriptionStatus};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// Configuration constants
const TICK_READY_THRESHOLD: u64 = 50;
const MAX_TIER_A: usize = 20;
const MAX_TIER_B: usize = 100;
const MAX_TIER_C: usize = 200;
// IBKR limit: typically 100 concurrent lines. We reserve some buffer.
const MAX_TOTAL_SUBSCRIPTIONS: usize = 95;
const WARM_BUFFER_TICKS: u64 = 100;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistSnapshot {
    pub tier_a_count: usize,
    pub tier_b_count: usize,
    pub tier_c_count: usize,
    pub total_subscriptions: usize,
}

#[derive(Debug, Clone)]
pub struct TierData {
    pub tier: Tier,
    pub tick_count: u64,
    pub subscription_status: SubscriptionStatus,
    pub last_activity: u64, // timestamp
    pub cold_start_state: ColdStartState,
    pub ticks_in_warm_state: u64,
}

impl TierData {
    pub fn new(tier: Tier) -> Self {
        Self {
            tier,
            tick_count: 0,
            subscription_status: SubscriptionStatus::None,
            last_activity: 0,
            cold_start_state: ColdStartState::ColdStart,
            ticks_in_warm_state: 0,
        }
    }

    pub fn update_cold_start(&mut self, is_surge: bool) {
        if is_surge {
            self.cold_start_state = ColdStartState::FullActive;
            return;
        }

        match self.cold_start_state {
            ColdStartState::ColdStart => {
                // Transition to WarmActive on first tick/update
                // Or maybe after a few ticks? Let's say immediate for now as per "Cold Start" implies awaiting initial data.
                // Assuming this method is called on tick or update.
                self.cold_start_state = ColdStartState::WarmActive;
                self.ticks_in_warm_state = 0;
            },
            ColdStartState::WarmActive => {
                self.ticks_in_warm_state += 1;
                if self.ticks_in_warm_state >= WARM_BUFFER_TICKS {
                    self.cold_start_state = ColdStartState::FullActive;
                }
            },
            ColdStartState::FullActive => {
                // Already fully active
            }
        }
    }

    pub fn acceleration_weight(&self) -> f64 {
        match self.cold_start_state {
            ColdStartState::ColdStart => 0.0,
            ColdStartState::WarmActive => 0.5, // Adjusted acceleration weight
            ColdStartState::FullActive => 1.0,
        }
    }
}

pub struct Watchlist {
    pub tier_a: HashMap<SymbolId, TierData>,
    pub tier_b: HashMap<SymbolId, TierData>,
    pub tier_c: HashMap<SymbolId, TierData>,
}

impl Watchlist {
    pub fn new() -> Self {
        Self {
            tier_a: HashMap::new(),
            tier_b: HashMap::new(),
            tier_c: HashMap::new(),
        }
    }

    pub fn snapshot(&self) -> WatchlistSnapshot {
        WatchlistSnapshot {
            tier_a_count: self.tier_a.len(),
            tier_b_count: self.tier_b.len(),
            tier_c_count: self.tier_c.len(),
            total_subscriptions: self.total_subscriptions(),
        }
    }

    pub fn get_tier(&self, symbol_id: SymbolId) -> Option<Tier> {
        if self.tier_a.contains_key(&symbol_id) { Some(Tier::A) }
        else if self.tier_b.contains_key(&symbol_id) { Some(Tier::B) }
        else if self.tier_c.contains_key(&symbol_id) { Some(Tier::C) }
        else { None }
    }

    pub fn get_data(&self, symbol_id: SymbolId) -> Option<&TierData> {
        if let Some(d) = self.tier_a.get(&symbol_id) { return Some(d); }
        if let Some(d) = self.tier_b.get(&symbol_id) { return Some(d); }
        if let Some(d) = self.tier_c.get(&symbol_id) { return Some(d); }
        None
    }

    pub fn get_data_mut(&mut self, symbol_id: SymbolId) -> Option<&mut TierData> {
         if let Some(d) = self.tier_a.get_mut(&symbol_id) { return Some(d); }
         if let Some(d) = self.tier_b.get_mut(&symbol_id) { return Some(d); }
         if let Some(d) = self.tier_c.get_mut(&symbol_id) { return Some(d); }
         None
    }

    pub fn add_candidate(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
        if self.get_tier(symbol_id).is_some() {
            return Err("Already in watchlist");
        }
        if self.tier_c.len() >= MAX_TIER_C {
            return Err("Tier C full");
        }

        // Candidates start in Tier C
        self.tier_c.insert(symbol_id, TierData::new(Tier::C));
        Ok(())
    }

    pub fn promote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
        let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

        match current_tier {
            Tier::C => {
                // C -> B
                if self.tier_b.len() >= MAX_TIER_B {
                    return Err("Tier B full");
                }

                if self.total_subscriptions() >= MAX_TOTAL_SUBSCRIPTIONS {
                    return Err("Max subscriptions reached");
                }

                let mut data = self.tier_c.remove(&symbol_id).unwrap();

                if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_c.insert(symbol_id, data);
                    return Err("Not TickReady");
                }

                data.tier = Tier::B;
                data.subscription_status = SubscriptionStatus::Pending;
                self.tier_b.insert(symbol_id, data);
            },
            Tier::B => {
                // B -> A
                 if self.tier_a.len() >= MAX_TIER_A {
                    return Err("Tier A full");
                }
                // A also consumes a subscription, but B already has one.
                // Assuming A and B both count as 1 subscription (just different processing).

                 let mut data = self.tier_b.remove(&symbol_id).unwrap();

                 // Reuse TickReady check or strict check
                 if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_b.insert(symbol_id, data);
                     return Err("Not TickReady for Tier A");
                 }

                data.tier = Tier::A;
                self.tier_a.insert(symbol_id, data);
            },
            Tier::A => return Err("Already in Tier A"),
        }
        Ok(())
    }

    pub fn demote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
         let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

         match current_tier {
             Tier::A => {
                 let mut data = self.tier_a.remove(&symbol_id).unwrap();
                 data.tier = Tier::B;
                 self.tier_b.insert(symbol_id, data);
             },
             Tier::B => {
                 let mut data = self.tier_b.remove(&symbol_id).unwrap();
                 data.tier = Tier::C;
                 data.subscription_status = SubscriptionStatus::None;
                 self.tier_c.insert(symbol_id, data);
             },
             Tier::C => {
                 self.tier_c.remove(&symbol_id);
             }
         }
         Ok(())
    }

    pub fn update_tick_count(&mut self, symbol_id: SymbolId) {
        if let Some(data) = self.get_data_mut(symbol_id) {
            data.tick_count += 1;
        }

        // Candidates start in Tier C
        self.tier_c.insert(symbol_id, TierData::new(Tier::C));
        Ok(())
    }

    pub fn promote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
        let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

        match current_tier {
            Tier::C => {
                // C -> B
                if self.tier_b.len() >= MAX_TIER_B {
                    return Err("Tier B full");
                }

                if self.total_subscriptions() >= MAX_TOTAL_SUBSCRIPTIONS {
                    return Err("Max subscriptions reached");
                }

                let mut data = self.tier_c.remove(&symbol_id).unwrap();

                if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_c.insert(symbol_id, data);
                    return Err("Not TickReady");
                }

                data.tier = Tier::B;
                data.subscription_status = SubscriptionStatus::Pending;
                self.tier_b.insert(symbol_id, data);
            },
            Tier::B => {
                // B -> A
                 if self.tier_a.len() >= MAX_TIER_A {
                    return Err("Tier A full");
                }
                // A also consumes a subscription, but B already has one.
                // Assuming A and B both count as 1 subscription (just different processing).

                 let mut data = self.tier_b.remove(&symbol_id).unwrap();

                 // Reuse TickReady check or strict check
                 if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_b.insert(symbol_id, data);
                     return Err("Not TickReady for Tier A");
                 }

                data.tier = Tier::A;
                self.tier_a.insert(symbol_id, data);
            },
            Tier::A => return Err("Already in Tier A"),
        }
        Ok(())
    }

    pub fn demote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
         let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

         match current_tier {
             Tier::A => {
                 let mut data = self.tier_a.remove(&symbol_id).unwrap();
                 data.tier = Tier::B;
                 self.tier_b.insert(symbol_id, data);
             },
             Tier::B => {
                 let mut data = self.tier_b.remove(&symbol_id).unwrap();
                 data.tier = Tier::C;
                 data.subscription_status = SubscriptionStatus::None;
                 self.tier_c.insert(symbol_id, data);
             },
             Tier::C => {
                 self.tier_c.remove(&symbol_id);
             }
         }
         Ok(())
    }

    pub fn update_tick_count(&mut self, symbol_id: SymbolId) {
        if let Some(data) = self.get_data_mut(symbol_id) {
            data.tick_count += 1;
        }
        if self.tier_c.len() >= MAX_TIER_C {
            return Err("Tier C full");
        }

        // Candidates start in Tier C
        self.tier_c.insert(symbol_id, TierData::new(Tier::C));
        Ok(())
    }

    pub fn promote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
        let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

        match current_tier {
            Tier::C => {
                // C -> B
                if self.tier_b.len() >= MAX_TIER_B {
                    return Err("Tier B full");
                }

                if self.total_subscriptions() >= MAX_TOTAL_SUBSCRIPTIONS {
                    return Err("Max subscriptions reached");
                }

                let mut data = self.tier_c.remove(&symbol_id).unwrap();

                if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_c.insert(symbol_id, data);
                    return Err("Not TickReady");
                }

                data.tier = Tier::B;
                data.subscription_status = SubscriptionStatus::Pending;
                self.tier_b.insert(symbol_id, data);
            },
            Tier::B => {
                // B -> A
                 if self.tier_a.len() >= MAX_TIER_A {
                    return Err("Tier A full");
                }
                // A also consumes a subscription, but B already has one.
                // Assuming A and B both count as 1 subscription (just different processing).

                 let mut data = self.tier_b.remove(&symbol_id).unwrap();

                 // Reuse TickReady check or strict check
                 if data.tick_count < TICK_READY_THRESHOLD {
                     self.tier_b.insert(symbol_id, data);
                     return Err("Not TickReady for Tier A");
                 }

                data.tier = Tier::A;
                self.tier_a.insert(symbol_id, data);
            },
            Tier::A => return Err("Already in Tier A"),
        }
        Ok(())
    }

    pub fn demote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
         let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

         match current_tier {
             Tier::A => {
                 let mut data = self.tier_a.remove(&symbol_id).unwrap();
                 data.tier = Tier::B;
                 self.tier_b.insert(symbol_id, data);
             },
             Tier::B => {
                 let mut data = self.tier_b.remove(&symbol_id).unwrap();
                 data.tier = Tier::C;
                 data.subscription_status = SubscriptionStatus::None;
                 self.tier_c.insert(symbol_id, data);
             },
             Tier::C => {
                 self.tier_c.remove(&symbol_id);
             }
         }
         Ok(())
    }

    pub fn update_tick_count(&mut self, symbol_id: SymbolId) {
        if let Some(data) = self.get_data_mut(symbol_id) {
            data.tick_count += 1;
        }
    }

    pub fn total_subscriptions(&self) -> usize {
        // Assuming Tier A and B consume subscriptions
        self.tier_a.len() + self.tier_b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        // Try promote without ticks
        assert!(wl.promote(sym).is_err());

        // Add ticks
        for _ in 0..TICK_READY_THRESHOLD {
            wl.update_tick_count(sym);
        }

        // Promote C -> B
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        // Promote B -> A
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::A));

        // Promote A -> Error
        assert!(wl.promote(sym).is_err());
    }

    #[test]
    fn test_demotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        // Fake ticks
        if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }

        wl.promote(sym).unwrap(); // B
        wl.promote(sym).unwrap(); // A

        wl.demote(sym).unwrap(); // B
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        wl.demote(sym).unwrap(); // C
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        wl.demote(sym).unwrap(); // Removed
        assert_eq!(wl.get_tier(sym), None);
    }

    #[test]
    fn test_subscription_limits() {
        let mut wl = Watchlist::new();
        // Fill up subscriptions
        for i in 0..MAX_TOTAL_SUBSCRIPTIONS {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym).unwrap();
            if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }
            wl.promote(sym).unwrap(); // To Tier B
        }

        let extra = SymbolId(1000);
        wl.add_candidate(extra).unwrap();
        if let Some(d) = wl.get_data_mut(extra) { d.tick_count = 100; }

        // Should fail
        assert!(wl.promote(extra).is_err());
    }

    pub fn total_subscriptions(&self) -> usize {
        // Assuming Tier A and B consume subscriptions
        self.tier_a.len() + self.tier_b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        // Try promote without ticks
        assert!(wl.promote(sym).is_err());

        // Add ticks
        for _ in 0..TICK_READY_THRESHOLD {
            wl.update_tick_count(sym);
        }

        // Promote C -> B
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        // Promote B -> A
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::A));

        // Promote A -> Error
        assert!(wl.promote(sym).is_err());
    }

    #[test]
    fn test_demotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        // Fake ticks
        if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }

        wl.promote(sym).unwrap(); // B
        wl.promote(sym).unwrap(); // A

        wl.demote(sym).unwrap(); // B
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        wl.demote(sym).unwrap(); // C
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        wl.demote(sym).unwrap(); // Removed
        assert_eq!(wl.get_tier(sym), None);
    }

    #[test]
    fn test_subscription_limits() {
        let mut wl = Watchlist::new();
        // Fill up subscriptions
        for i in 0..MAX_TOTAL_SUBSCRIPTIONS {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym).unwrap();
            if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }
            wl.promote(sym).unwrap(); // To Tier B
        }

        let extra = SymbolId(1000);
        wl.add_candidate(extra).unwrap();
        if let Some(d) = wl.get_data_mut(extra) { d.tick_count = 100; }

        // Should fail
        assert!(wl.promote(extra).is_err());
    }

    #[test]
    fn test_cold_start_controller() {
        let mut data = TierData::new(Tier::B);
        assert_eq!(data.cold_start_state, ColdStartState::ColdStart);
        assert_eq!(data.acceleration_weight(), 0.0);

        // First update -> WarmActive
        data.update_cold_start(false);
        assert_eq!(data.cold_start_state, ColdStartState::WarmActive);
        assert_eq!(data.acceleration_weight(), 0.5);

        // Update loop until FullActive
        for _ in 0..WARM_BUFFER_TICKS {
            data.update_cold_start(false);
        }
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }

    #[test]
    fn test_surge_override() {
        let mut data = TierData::new(Tier::B);
        // Surge -> FullActive immediately
        data.update_cold_start(true);
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }

    pub fn total_subscriptions(&self) -> usize {
        // Assuming Tier A and B consume subscriptions
        self.tier_a.len() + self.tier_b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        // Try promote without ticks
        assert!(wl.promote(sym).is_err());

        // Add ticks
        for _ in 0..TICK_READY_THRESHOLD {
            wl.update_tick_count(sym);
        }

        // Promote C -> B
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        // Promote B -> A
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::A));

        // Promote A -> Error
        assert!(wl.promote(sym).is_err());
    }

    #[test]
    fn test_demotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        // Fake ticks
        if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }

        wl.promote(sym).unwrap(); // B
        wl.promote(sym).unwrap(); // A

        wl.demote(sym).unwrap(); // B
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        wl.demote(sym).unwrap(); // C
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        wl.demote(sym).unwrap(); // Removed
        assert_eq!(wl.get_tier(sym), None);
    }

    #[test]
    fn test_subscription_limits() {
        let mut wl = Watchlist::new();
        // Fill up subscriptions
        for i in 0..MAX_TOTAL_SUBSCRIPTIONS {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym).unwrap();
            if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }
            wl.promote(sym).unwrap(); // To Tier B
        }

        let extra = SymbolId(1000);
        wl.add_candidate(extra).unwrap();
        if let Some(d) = wl.get_data_mut(extra) { d.tick_count = 100; }

        // Should fail
        assert!(wl.promote(extra).is_err());
    }

    #[test]
    fn test_cold_start_controller() {
        let mut data = TierData::new(Tier::B);
        assert_eq!(data.cold_start_state, ColdStartState::ColdStart);
        assert_eq!(data.acceleration_weight(), 0.0);

        // First update -> WarmActive
        data.update_cold_start(false);
        assert_eq!(data.cold_start_state, ColdStartState::WarmActive);
        assert_eq!(data.acceleration_weight(), 0.5);

        // Update loop until FullActive
        for _ in 0..WARM_BUFFER_TICKS {
            data.update_cold_start(false);
        }
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }

    #[test]
    fn test_surge_override() {
        let mut data = TierData::new(Tier::B);
        // Surge -> FullActive immediately
        data.update_cold_start(true);
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }

    pub fn total_subscriptions(&self) -> usize {
        // Assuming Tier A and B consume subscriptions
        self.tier_a.len() + self.tier_b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        // Try promote without ticks
        assert!(wl.promote(sym).is_err());

        // Add ticks
        for _ in 0..TICK_READY_THRESHOLD {
            wl.update_tick_count(sym);
        }

        // Promote C -> B
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        // Promote B -> A
        wl.promote(sym).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::A));

        // Promote A -> Error
        assert!(wl.promote(sym).is_err());
    }

    #[test]
    fn test_demotion_path() {
        let mut wl = Watchlist::new();
        let sym = SymbolId(1);

        wl.add_candidate(sym).unwrap();
        // Fake ticks
        if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }

        wl.promote(sym).unwrap(); // B
        wl.promote(sym).unwrap(); // A

        wl.demote(sym).unwrap(); // B
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        wl.demote(sym).unwrap(); // C
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        wl.demote(sym).unwrap(); // Removed
        assert_eq!(wl.get_tier(sym), None);
    }

    #[test]
    fn test_subscription_limits() {
        let mut wl = Watchlist::new();
        // Fill up subscriptions
        for i in 0..MAX_TOTAL_SUBSCRIPTIONS {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym).unwrap();
            if let Some(d) = wl.get_data_mut(sym) { d.tick_count = 100; }
            wl.promote(sym).unwrap(); // To Tier B
        }

        let extra = SymbolId(1000);
        wl.add_candidate(extra).unwrap();
        if let Some(d) = wl.get_data_mut(extra) { d.tick_count = 100; }

        // Should fail
        assert!(wl.promote(extra).is_err());
    }

    #[test]
    fn test_cold_start_controller() {
        let mut data = TierData::new(Tier::B);
        assert_eq!(data.cold_start_state, ColdStartState::ColdStart);
        assert_eq!(data.acceleration_weight(), 0.0);

        // First update -> WarmActive
        data.update_cold_start(false);
        assert_eq!(data.cold_start_state, ColdStartState::WarmActive);
        assert_eq!(data.acceleration_weight(), 0.5);

        // Update loop until FullActive
        for _ in 0..WARM_BUFFER_TICKS {
            data.update_cold_start(false);
        }
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }

    #[test]
    fn test_surge_override() {
        let mut data = TierData::new(Tier::B);
        // Surge -> FullActive immediately
        data.update_cold_start(true);
        assert_eq!(data.cold_start_state, ColdStartState::FullActive);
        assert_eq!(data.acceleration_weight(), 1.0);
    }
}
