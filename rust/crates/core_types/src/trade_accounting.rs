use crate::{FillData, Side};

/// Computes the new weighted average cost when adding to a position.
/// Returns the new average cost.
pub fn compute_weighted_avg_cost(
    current_qty: u32,
    current_avg_cost: f64,
    new_qty: u32,
    new_price: f64,
) -> f64 {
    if current_qty == 0 {
        return new_price;
    }
    let total_cost = (current_qty as f64 * current_avg_cost) + (new_qty as f64 * new_price);
    let total_qty = current_qty + new_qty;
    total_cost / (total_qty as f64)
}

/// Computes the realized PnL of a closing fill against an open position.
/// Returns the realized PnL in USD. Negative means loss.
pub fn compute_realized_pnl(
    position_qty: i32,
    position_avg_cost: f64,
    close_fill: &FillData,
) -> f64 {
    // Only compute realized if it's actually closing the position (opposing side)
    // If position > 0 (long), closing side is Ask.
    // If position < 0 (short), closing side is Bid.
    let is_closing = (position_qty > 0 && close_fill.side == Side::Ask)
        || (position_qty < 0 && close_fill.side == Side::Bid);

    if !is_closing {
        return 0.0;
    }

    let fill_qty = close_fill.size as i32;
    let qty_to_close = fill_qty.min(position_qty.abs());

    let realized_per_share = if position_qty > 0 {
        close_fill.price - position_avg_cost // Long
    } else {
        position_avg_cost - close_fill.price // Short
    };

    realized_per_share * (qty_to_close as f64)
}

/// Computes the unrealized PnL of an open position at the current market price.
pub fn compute_unrealized_pnl(
    position_qty: i32,
    position_avg_cost: f64,
    current_price: f64,
) -> f64 {
    if position_qty == 0 {
        return 0.0;
    }

    if position_qty > 0 {
        (current_price - position_avg_cost) * (position_qty as f64) // Long
    } else {
        (position_avg_cost - current_price) * (position_qty.abs() as f64) // Short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_fill_initializes_cost_basis() {
        let avg_cost = compute_weighted_avg_cost(0, 0.0, 100, 10.0);
        assert!((avg_cost - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partial_fills_weighted_avg_cost() {
        let cost1 = compute_weighted_avg_cost(0, 0.0, 100, 10.0);
        let cost2 = compute_weighted_avg_cost(100, cost1, 50, 13.0);
        let cost3 = compute_weighted_avg_cost(150, cost2, 50, 11.0);

        let expected = ((100.0 * 10.0) + (50.0 * 13.0) + (50.0 * 11.0)) / 200.0;
        assert!((cost3 - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reversal_realized_pnl() {
        let fill = FillData {
            order_id: 1,
            price: 15.0,
            size: 50,
            side: Side::Ask,
            liquidity: 0,
        };

        // Long 100 shares at $10.00
        let pnl = compute_realized_pnl(100, 10.0, &fill);
        // Selling 50 shares at $15 -> $5 profit per share * 50 = $250
        assert!((pnl - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_flat_to_open_initializes_correctly() {
        // If we are flat and receive a fill, realized PnL should be 0.0
        // (weighted avg handles the cost basis)
        let fill = FillData {
            order_id: 1,
            price: 15.0,
            size: 50,
            side: Side::Bid,
            liquidity: 0,
        };

        let pnl = compute_realized_pnl(0, 0.0, &fill);
        assert!((pnl - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fill_after_cancel_no_double_counting() {
        // This is a behavioral flow check rather than pure math, but we test the math constraint
        // that if a position was closed (size 0), a late fill just opens a new position,
        // rather than incorrectly computing realized PnL against an old price.
        let late_fill = FillData {
            order_id: 2,
            price: 10.5,
            size: 100,
            side: Side::Bid,
            liquidity: 0,
        };
        // Position is 0 (closed previously)
        let pnl = compute_realized_pnl(0, 15.0, &late_fill);
        assert!((pnl - 0.0).abs() < f64::EPSILON); // No rogue PnL generated
    }
}

#[cfg(test)]
mod regression_tests {
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_no_rogue_pnl_math() {
        // Scans the source tree to ensure manual PnL math like `* state.position` or `fill_price - avg_cost`
        // doesn't exist outside of trade_accounting.rs
        let mut violations = Vec::new();

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let crates_dir = PathBuf::from(manifest_dir).parent().unwrap().to_path_buf();

        let targets = vec![
            crates_dir.join("app_runtime/src/lib.rs"),
            crates_dir.join("tape_engine/src/lib.rs"),
            crates_dir.join("risk_engine/src/lib.rs"),
        ];

        let patterns = [
            "fill.price - avg_cost",
            "fill_price - state.avg_cost",
            "- state.avg_cost",
            "state.position as f64 * state.avg_cost",
            "state.qty as f64 * state.avg_cost",
            "trade_pnl = ",
        ];

        for target in targets {
            if let Ok(content) = fs::read_to_string(&target) {
                for (line_no, line) in content.lines().enumerate() {
                    let text = line.trim();
                    if text.starts_with("//") {
                        continue;
                    } // Skip comments

                    for pattern in &patterns {
                        if text.contains(pattern)
                            && !text.contains("compute_realized_pnl")
                            && !text.contains("compute_weighted_avg_cost")
                        {
                            violations.push(format!(
                                "{}:{}: found forbidden math '{}'",
                                target.display(),
                                line_no + 1,
                                pattern
                            ));
                        }
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "Found rogue PnL math outside trade_accounting:\n{:#?}",
            violations
        );
    }
}
