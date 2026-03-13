#![deny(clippy::unwrap_in_result)]
//! Watchlist Engine Crate (Slow Loop).
//!
//! Manages the tiered watchlist system (Tier A, Tier B, Tier C).
//! Handles pacing, upgrades, downgrades, and slow-moving context analysis (MTF, Correlation).

use core_types::{
    ColdStartState, DailyContext, MtfAnalysis, RegimeState, SubscriptionStatus, SymbolId, Tier,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub regime: RegimeState,
    pub contexts: HashMap<SymbolId, DailyContext>,
    pub mtf_results: HashMap<SymbolId, MtfAnalysis>,
}

#[derive(Debug, Clone)]
pub struct TierData {
    pub tier: Tier,
    pub tick_count: u64,
    pub subscription_status: SubscriptionStatus,
    pub last_activity: u64, // timestamp
    pub cold_start_state: ColdStartState,
    pub ticks_in_warm_state: u64,
    pub daily_context: Option<DailyContext>,
    pub mtf_analysis: Option<MtfAnalysis>,

    // Lifecycle properties
    pub inactive_cycles: u32,
    pub quality_score: f64,
    pub volume: u64,
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
            daily_context: None,
            mtf_analysis: None,
            inactive_cycles: 0,
            quality_score: 0.0,
            volume: 0,
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
                self.cold_start_state = ColdStartState::WarmActive;
                self.ticks_in_warm_state = 0;
            }
            ColdStartState::WarmActive => {
                self.ticks_in_warm_state += 1;
                if self.ticks_in_warm_state >= WARM_BUFFER_TICKS {
                    self.cold_start_state = ColdStartState::FullActive;
                }
            }
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
    pub current_regime: RegimeState,
    pub recently_evicted: HashMap<SymbolId, u32>, // symbol_id -> remaining cooldown cycles
}

impl Default for Watchlist {
    fn default() -> Self {
        Self::new()
    }
}

impl Watchlist {
    pub fn new() -> Self {
        Self {
            tier_a: HashMap::new(),
            tier_b: HashMap::new(),
            tier_c: HashMap::new(),
            current_regime: RegimeState::Normal,
            recently_evicted: HashMap::new(),
        }
    }

    pub fn snapshot(&self) -> WatchlistSnapshot {
        let mut contexts: HashMap<SymbolId, DailyContext> = HashMap::new();
        let mut mtf_results: HashMap<SymbolId, MtfAnalysis> = HashMap::new();

        for (id, data) in self
            .tier_a
            .iter()
            .chain(self.tier_b.iter())
            .chain(self.tier_c.iter())
        {
            if let Some(ctx) = &data.daily_context {
                contexts.insert(*id, ctx.clone());
            }
            if let Some(mtf) = &data.mtf_analysis {
                mtf_results.insert(*id, mtf.clone());
            }
        }

        WatchlistSnapshot {
            tier_a_count: self.tier_a.len(),
            tier_b_count: self.tier_b.len(),
            tier_c_count: self.tier_c.len(),
            total_subscriptions: self.total_subscriptions(),
            regime: self.current_regime,
            contexts,
            mtf_results,
        }
    }

    pub fn update_regime(&mut self, regime: RegimeState) {
        self.current_regime = regime;
    }

    pub fn update_symbol_context(
        &mut self,
        symbol_id: SymbolId,
        daily_ctx: DailyContext,
        mtf: MtfAnalysis,
    ) {
        if let Some(data) = self.get_data_mut(symbol_id) {
            data.daily_context = Some(daily_ctx);
            data.mtf_analysis = Some(mtf);
        }
    }

    pub fn get_tier(&self, symbol_id: SymbolId) -> Option<Tier> {
        if self.tier_a.contains_key(&symbol_id) {
            Some(Tier::A)
        } else if self.tier_b.contains_key(&symbol_id) {
            Some(Tier::B)
        } else if self.tier_c.contains_key(&symbol_id) {
            Some(Tier::C)
        } else {
            None
        }
    }

    pub fn get_data(&self, symbol_id: SymbolId) -> Option<&TierData> {
        if let Some(d) = self.tier_a.get(&symbol_id) {
            return Some(d);
        }
        if let Some(d) = self.tier_b.get(&symbol_id) {
            return Some(d);
        }
        if let Some(d) = self.tier_c.get(&symbol_id) {
            return Some(d);
        }
        None
    }

    pub fn get_data_mut(&mut self, symbol_id: SymbolId) -> Option<&mut TierData> {
        if let Some(d) = self.tier_a.get_mut(&symbol_id) {
            return Some(d);
        }
        if let Some(d) = self.tier_b.get_mut(&symbol_id) {
            return Some(d);
        }
        if let Some(d) = self.tier_c.get_mut(&symbol_id) {
            return Some(d);
        }
        None
    }

    pub fn add_candidate(&mut self, symbol_id: SymbolId, eviction_cycles: u32, metrics: &mut metrics_observability::MetricsCollector) -> Result<(), &'static str> {
        if self.recently_evicted.contains_key(&symbol_id) {
            return Err("Recently evicted, cannot re-promote yet");
        }
        if self.get_tier(symbol_id).is_some() {
            return Err("Already in watchlist");
        }

        if self.tier_c.len() >= MAX_TIER_C {
            // FIFO fairness — oldest cold symbol evicted first
            let mut oldest: Option<(SymbolId, u64)> = None;
            for (id, data) in &self.tier_c {
                if oldest.is_none() || data.last_activity < oldest.unwrap().1 {
                    oldest = Some((*id, data.last_activity));
                }
            }
            if let Some((oldest_id, _)) = oldest {
                self.tier_c.remove(&oldest_id);
                self.recently_evicted.insert(oldest_id, eviction_cycles); // use eviction_cycles for cooldown
                metrics.symbols_evicted += 1;
            } else {
                return Err("Tier C full and no cold symbols to evict");
            }
        }

        // Candidates start in Tier C
        self.tier_c.insert(symbol_id, TierData::new(Tier::C));
        Ok(())
    }

    pub fn promote(&mut self, symbol_id: SymbolId, metrics: &mut Option<&mut metrics_observability::MetricsCollector>) -> Result<(), &'static str> {
        let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

        let res = match current_tier {
            Tier::C => {
                // C -> B
                if self.tier_b.len() >= MAX_TIER_B {
                    return Err("Tier B full");
                }

                if self.total_subscriptions() >= MAX_TOTAL_SUBSCRIPTIONS {
                    return Err("Max subscriptions reached");
                }

                let mut data = self
                    .tier_c
                    .remove(&symbol_id)
                    .ok_or("Data missing in Tier C")?;

                if data.tick_count < TICK_READY_THRESHOLD {
                    self.tier_c.insert(symbol_id, data);
                    return Err("Not TickReady");
                }

                data.tier = Tier::B;
                data.subscription_status = SubscriptionStatus::Pending;
                self.tier_b.insert(symbol_id, data);
                Ok(())
            }
            Tier::B => {
                // B -> A
                if self.tier_a.len() >= MAX_TIER_A {
                    return Err("Tier A full");
                }
                // A also consumes a subscription, but B already has one.
                // Assuming A and B both count as 1 subscription (just different processing).

                let mut data = self
                    .tier_b
                    .remove(&symbol_id)
                    .ok_or("Data missing in Tier B")?;

                // Reuse TickReady check or strict check
                if data.tick_count < TICK_READY_THRESHOLD {
                    self.tier_b.insert(symbol_id, data);
                    return Err("Not TickReady for Tier A");
                }

                data.tier = Tier::A;
                self.tier_a.insert(symbol_id, data);
                Ok(())
            }
            Tier::A => Err("Already in Tier A"),
        };

        if res.is_ok() {
            if let Some(m) = metrics {
                m.symbols_promoted += 1;
            }
        }
        res
    }

    pub fn demote(&mut self, symbol_id: SymbolId) -> Result<(), &'static str> {
        let current_tier = self.get_tier(symbol_id).ok_or("Symbol not in watchlist")?;

        match current_tier {
            Tier::A => {
                let mut data = self
                    .tier_a
                    .remove(&symbol_id)
                    .ok_or("Data missing in Tier A")?;
                data.tier = Tier::B;
                self.tier_b.insert(symbol_id, data);
            }
            Tier::B => {
                let mut data = self
                    .tier_b
                    .remove(&symbol_id)
                    .ok_or("Data missing in Tier B")?;
                data.tier = Tier::C;
                data.subscription_status = SubscriptionStatus::None;
                self.tier_c.insert(symbol_id, data);
            }
            Tier::C => {
                self.tier_c.remove(&symbol_id);
            }
        }
        Ok(())
    }

    pub fn process_lifecycle(
        &mut self,
        demotion_cycles: u32,
        eviction_cycles: u32,
        min_quality_score: f64,
        min_volume: u64,
        metrics: &mut metrics_observability::MetricsCollector,
    ) {
        // Increment inactive cycles for all symbols
        for data in self.tier_a.values_mut().chain(self.tier_b.values_mut()).chain(self.tier_c.values_mut()) {
            data.inactive_cycles += 1;
        }

        // Cooldown processing
        self.recently_evicted.retain(|_, cycles| {
            if *cycles > 0 {
                *cycles -= 1;
            }
            *cycles > 0
        });

        // Demotion/Eviction tracking
        let mut to_demote = Vec::new();
        let mut to_evict = Vec::new();

        let check_demotion = |_id: &SymbolId, d: &TierData| -> bool {
            d.quality_score < min_quality_score || d.volume < min_volume || d.inactive_cycles >= demotion_cycles
        };
        let check_eviction = |_id: &SymbolId, d: &TierData| -> bool {
            d.inactive_cycles >= eviction_cycles
        };

        for (id, data) in &self.tier_a {
            if check_demotion(id, data) {
                to_demote.push(*id);
            }
        }
        for (id, data) in &self.tier_b {
            if check_demotion(id, data) {
                to_demote.push(*id);
            }
        }
        for (id, data) in &self.tier_c {
            if check_eviction(id, data) {
                to_evict.push(*id);
            }
        }

        for id in to_demote {
            if let Ok(_) = self.demote(id) {
                metrics.symbols_demoted += 1;
            }
        }

        for id in to_evict {
            self.tier_c.remove(&id);
            self.recently_evicted.insert(id, eviction_cycles); // add to cooldown
            metrics.symbols_evicted += 1;
        }

        metrics.cold_symbol_count = self.tier_c.len() as u64;
        metrics.current_count = (self.tier_c.len() + self.tier_b.len() + self.tier_a.len()) as u64;
        metrics.budget = (MAX_TIER_A + MAX_TIER_B + MAX_TIER_C) as u64;
    }

    pub fn update_tick_count(&mut self, symbol_id: SymbolId) {
        if let Some(data) = self.get_data_mut(symbol_id) {
            data.tick_count += 1;
            data.inactive_cycles = 0; // reset inactive cycles on tick update
        }
    }

    pub fn touch(&mut self, symbol_id: SymbolId, ts: u64) {
        if let Some(d) = self.tier_a.get_mut(&symbol_id) {
            d.last_activity = ts;
            return;
        }
        if let Some(d) = self.tier_b.get_mut(&symbol_id) {
            d.last_activity = ts;
            return;
        }
        if let Some(d) = self.tier_c.get_mut(&symbol_id) {
            d.last_activity = ts;
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
        let mut metrics = metrics_observability::MetricsCollector::default();
        let sym = SymbolId(1);

        wl.add_candidate(sym, 5, &mut metrics).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::C));

        // Try promote without ticks
        assert!(wl.promote(sym, &mut Some(&mut metrics)).is_err());

        // Add ticks
        for _ in 0..TICK_READY_THRESHOLD {
            wl.update_tick_count(sym);
        }

        // Promote C -> B
        wl.promote(sym, &mut Some(&mut metrics)).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::B));

        // Promote B -> A
        wl.promote(sym, &mut Some(&mut metrics)).unwrap();
        assert_eq!(wl.get_tier(sym), Some(Tier::A));

        // Promote A -> Error
        assert!(wl.promote(sym, &mut Some(&mut metrics)).is_err());
    }

    #[test]
    fn test_demotion_path() {
        let mut wl = Watchlist::new();
        let mut metrics = metrics_observability::MetricsCollector::default();
        let sym = SymbolId(1);

        wl.add_candidate(sym, 5, &mut metrics).unwrap();
        // Fake ticks
        if let Some(d) = wl.get_data_mut(sym) {
            d.tick_count = 100;
        }

        wl.promote(sym, &mut Some(&mut metrics)).unwrap(); // B
        wl.promote(sym, &mut Some(&mut metrics)).unwrap(); // A

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
        let mut metrics = metrics_observability::MetricsCollector::default();
        // Fill up subscriptions
        for i in 0..MAX_TOTAL_SUBSCRIPTIONS {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym, 5, &mut metrics).unwrap();
            if let Some(d) = wl.get_data_mut(sym) {
                d.tick_count = 100;
            }
            wl.promote(sym, &mut Some(&mut metrics)).unwrap(); // To Tier B
        }

        let extra = SymbolId(1000);
        wl.add_candidate(extra, 5, &mut metrics).unwrap();
        if let Some(d) = wl.get_data_mut(extra) {
            d.tick_count = 100;
        }

        // Should fail
        assert!(wl.promote(extra, &mut Some(&mut metrics)).is_err());
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

    #[test]
    fn test_saturation_and_eviction() {
        let mut wl = Watchlist::new();
        let mut metrics = metrics_observability::MetricsCollector::default();
        let eviction_cycles = 5;

        // Fill Tier C budget
        for i in 0..MAX_TIER_C {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym, eviction_cycles, &mut metrics).unwrap();
            wl.touch(sym, i as u64); // Different timestamps to distinguish oldest
        }

        assert_eq!(wl.tier_c.len(), MAX_TIER_C);

        // Add one more, should evict the oldest (SymbolId(0) with timestamp 0)
        let sym_new = SymbolId(MAX_TIER_C as u32);
        wl.add_candidate(sym_new, eviction_cycles, &mut metrics).unwrap();

        assert_eq!(wl.tier_c.len(), MAX_TIER_C);
        assert!(!wl.tier_c.contains_key(&SymbolId(0))); // Oldest was evicted
        assert!(wl.tier_c.contains_key(&sym_new));
        assert_eq!(metrics.symbols_evicted, 1);
    }

    #[test]
    fn test_churn_prevention() {
        let mut wl = Watchlist::new();
        let mut metrics = metrics_observability::MetricsCollector::default();
        let eviction_cycles = 5;
        let sym = SymbolId(1);

        wl.add_candidate(sym, eviction_cycles, &mut metrics).unwrap();

        // Artificially evict and add to cooldown
        wl.tier_c.remove(&sym);
        wl.recently_evicted.insert(sym, 3); // 3 cycles cooldown

        // Cannot re-promote immediately
        assert!(wl.add_candidate(sym, eviction_cycles, &mut metrics).is_err());

        // Process 3 cycles to clear cooldown
        for _ in 0..3 {
            wl.process_lifecycle(3, eviction_cycles, 45.0, 500000, &mut metrics);
        }

        // Verify it was removed from recently_evicted
        assert!(!wl.recently_evicted.contains_key(&sym));

        // Now we can promote
        assert!(wl.add_candidate(sym, eviction_cycles, &mut metrics).is_ok());
    }

    #[test]
    fn test_cold_timeout_eviction() {
        let mut wl = Watchlist::new();
        let mut metrics = metrics_observability::MetricsCollector::default();
        let demotion_cycles = 3;
        let eviction_cycles = 5;
        let sym = SymbolId(1);

        wl.add_candidate(sym, eviction_cycles, &mut metrics).unwrap();

        // Process lifecycle for less than eviction cycles
        for _ in 0..4 {
            wl.process_lifecycle(demotion_cycles, eviction_cycles, 45.0, 500000, &mut metrics);
        }

        // Still there
        assert!(wl.tier_c.contains_key(&sym));

        // One more cycle -> eviction
        wl.process_lifecycle(demotion_cycles, eviction_cycles, 45.0, 500000, &mut metrics);

        assert!(!wl.tier_c.contains_key(&sym));
        assert_eq!(metrics.symbols_evicted, 1);

        // Should be in recently_evicted cooldown
        assert!(wl.recently_evicted.contains_key(&sym));
    }

    #[test]
    fn test_fifo_fairness() {
        let mut wl = Watchlist::new();
        let mut metrics = metrics_observability::MetricsCollector::default();
        let eviction_cycles = 5;

        // Fill Tier C budget
        for i in 0..MAX_TIER_C {
            let sym = SymbolId(i as u32);
            wl.add_candidate(sym, eviction_cycles, &mut metrics).unwrap();

            // Set arbitrary timestamps: oldest is i = 10 (timestamp 1)
            let ts = if i == 10 { 1 } else { (i + 100) as u64 };
            wl.touch(sym, ts);
        }

        // The oldest symbol is SymbolId(10) with timestamp 1
        let sym_new = SymbolId(9999);
        wl.add_candidate(sym_new, eviction_cycles, &mut metrics).unwrap();

        assert!(!wl.tier_c.contains_key(&SymbolId(10)));
        assert!(wl.tier_c.contains_key(&sym_new));
        assert_eq!(metrics.symbols_evicted, 1);
    }
}
