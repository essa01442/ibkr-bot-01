//! Exposure Validator.
//!
//! Enforces correlation-based exposure limits.
//! - Default: Max 1 position.
//! - Max 2 positions allowed ONLY if:
//!   - Correlation (20d) < 0.5
//!   - Different Sectors.

use core_types::{SymbolId, RejectReason};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct ExposureValidator {
    /// Maps (SymbolA, SymbolB) -> Correlation Coefficient (-1.0 to 1.0)
    /// Key is sorted (min(A,B), max(A,B)) to handle symmetry.
    pub correlation_matrix: HashMap<(SymbolId, SymbolId), f64>,

    /// Maps SymbolId -> Sector Name
    pub sector_map: HashMap<SymbolId, String>,
}

impl ExposureValidator {
    pub fn new() -> Self {
        Self {
            correlation_matrix: HashMap::new(),
            sector_map: HashMap::new(),
        }
    }

    pub fn set_correlation(&mut self, a: SymbolId, b: SymbolId, corr: f64) {
        let k = if a.0 < b.0 { (a, b) } else { (b, a) };
        self.correlation_matrix.insert(k, corr);
    }

    pub fn set_sector(&mut self, symbol: SymbolId, sector: String) {
        self.sector_map.insert(symbol, sector);
    }

    /// Calculates Pearson correlation from two slices of returns.
    /// Expects equal length.
    pub fn calculate_pearson(xs: &[f64], ys: &[f64]) -> Option<f64> {
        if xs.len() != ys.len() || xs.is_empty() {
            return None;
        }

        let n = xs.len() as f64;
        let sum_x: f64 = xs.iter().sum();
        let sum_y: f64 = ys.iter().sum();
        let sum_xy: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| x * y).sum();
        let sum_x2: f64 = xs.iter().map(|x| x * x).sum();
        let sum_y2: f64 = ys.iter().map(|y| y * y).sum();

        let numerator = sum_xy - (sum_x * sum_y / n);
        let denominator = ((sum_x2 - (sum_x * sum_x / n)) * (sum_y2 - (sum_y * sum_y / n))).sqrt();

        if denominator == 0.0 {
            return None;
        }

        Some(numerator / denominator)
    }

    /// Updates the correlation between two symbols given their recent returns.
    pub fn update_correlation_from_returns(&mut self, a: SymbolId, returns_a: &[f64], b: SymbolId, returns_b: &[f64]) {
        if let Some(corr) = Self::calculate_pearson(returns_a, returns_b) {
            self.set_correlation(a, b, corr);
        }
    }

    /// Checks if a new position can be opened given the current open positions.
    pub fn check_new_position(&self, new_symbol: SymbolId, open_positions: &[SymbolId]) -> Result<(), RejectReason> {
        let count = open_positions.len();

        // 1. Default Limit: Max 1 Position
        if count == 0 {
            return Ok(());
        }

        // 2. Max 2 Positions Strict Condition
        if count >= 2 {
            return Err(RejectReason::Exposure); // Hard limit 2
        }

        // We have exactly 1 open position. Check conditions for the 2nd.
        let existing_symbol = open_positions[0];

        // Condition A: Different Sectors
        let sector_new = self.sector_map.get(&new_symbol);
        let sector_existing = self.sector_map.get(&existing_symbol);

        match (sector_new, sector_existing) {
            (Some(s1), Some(s2)) => {
                if s1 == s2 {
                    return Err(RejectReason::Exposure); // Same sector
                }
            },
            _ => {
                // If sector info is missing, be conservative and reject?
                // Or allow? Prompt says "Sector restriction", implying we must know they are different.
                return Err(RejectReason::Exposure);
            }
        }

        // Condition B: Correlation (20d) < 0.5
        // If not in matrix, assume high correlation (conservative)
        let k = if new_symbol.0 < existing_symbol.0 { (new_symbol, existing_symbol) } else { (existing_symbol, new_symbol) };
        let corr = self.correlation_matrix.get(&k).unwrap_or(&1.0); // Default to 1.0 (fail) if unknown

        if *corr >= 0.5 {
            return Err(RejectReason::Exposure);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_position_default() {
        let validator = ExposureValidator::new();
        let sym1 = SymbolId(1);

        // 0 positions -> OK
        assert!(validator.check_new_position(sym1, &[]).is_ok());
    }

    #[test]
    fn test_max_2_strict() {
        let mut validator = ExposureValidator::new();
        let sym1 = SymbolId(1);
        let sym2 = SymbolId(2);
        let sym3 = SymbolId(3); // Candidate

        // 2 positions open -> Reject
        assert_eq!(validator.check_new_position(sym3, &[sym1, sym2]), Err(RejectReason::Exposure));
    }

    #[test]
    fn test_sector_restriction() {
        let mut validator = ExposureValidator::new();
        let sym1 = SymbolId(1); // Tech
        let sym2 = SymbolId(2); // Tech
        let sym3 = SymbolId(3); // Energy

        validator.set_sector(sym1, "Tech".to_string());
        validator.set_sector(sym2, "Tech".to_string());
        validator.set_sector(sym3, "Energy".to_string());

        // Same sector -> Reject
        // (Assume correlation is low for this test, set it explicitly)
        validator.set_correlation(sym1, sym2, 0.1);
        assert_eq!(validator.check_new_position(sym2, &[sym1]), Err(RejectReason::Exposure));

        // Different sector -> Check correlation next
        validator.set_correlation(sym1, sym3, 0.1);
        assert!(validator.check_new_position(sym3, &[sym1]).is_ok());
    }

    #[test]
    fn test_correlation_restriction() {
        let mut validator = ExposureValidator::new();
        let sym1 = SymbolId(1);
        let sym2 = SymbolId(2);

        validator.set_sector(sym1, "Tech".to_string());
        validator.set_sector(sym2, "Energy".to_string());

        // High Correlation -> Reject
        validator.set_correlation(sym1, sym2, 0.8);
        assert_eq!(validator.check_new_position(sym2, &[sym1]), Err(RejectReason::Exposure));

        // Low Correlation -> Accept
        validator.set_correlation(sym1, sym2, 0.4);
        assert!(validator.check_new_position(sym2, &[sym1]).is_ok());

        // Unknown Correlation -> Reject (Conservative)
        let sym3 = SymbolId(3);
        validator.set_sector(sym3, "Energy".to_string());
        assert_eq!(validator.check_new_position(sym3, &[sym1]), Err(RejectReason::Exposure));
    }
}
