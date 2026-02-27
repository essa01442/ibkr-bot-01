/// Configuration for Position Sizing.
#[derive(Debug, Clone)]
pub struct SizingConfig {
    /// Maximum risk per trade in USD (RiskPerTrade).
    pub risk_per_trade_usd: f64,

    /// Minimum stop distance in price units (cents) to avoid noise (MinStopDistance).
    pub min_stop_distance_cents: f64,

    /// Maximum position size as a percentage of total account NAV (Max position %).
    pub max_position_pct_nav: f64,

    /// Maximum liquidity participation rate (Liquidity cap).
    /// Caps size to `DailyVolume * liquidity_cap_pct`.
    pub liquidity_cap_pct: f64,

    /// Hard budget cap in USD for any single position (Budget cap).
    pub budget_cap_usd: f64,
}

impl Default for SizingConfig {
    fn default() -> Self {
        Self {
            risk_per_trade_usd: 50.0,
            min_stop_distance_cents: 0.05,
            max_position_pct_nav: 0.10, // 10%
            liquidity_cap_pct: 0.01,    // 1%
            budget_cap_usd: 5000.0,
        }
    }
}

pub struct PositionSizer {
    pub config: SizingConfig,
}

impl PositionSizer {
    pub fn new(config: SizingConfig) -> Self {
        Self { config }
    }

    /// Calculates the position size (number of shares) based on risk constraints.
    ///
    /// # Arguments
    /// * `account_balance` - Total account equity/NAV.
    /// * `entry_price` - Planned entry price.
    /// * `stop_price` - Planned stop loss price.
    /// * `daily_volume` - Average Daily Volume (shares) or available liquidity metric.
    /// * `available_cash` - Buying power currently available.
    ///
    /// # Returns
    /// Number of shares to trade (u32).
    pub fn calculate_size(
        &self,
        account_balance: f64,
        entry_price: f64,
        stop_price: f64,
        daily_volume: u64,
        available_cash: f64,
    ) -> u32 {
        if entry_price <= 0.0 || stop_price <= 0.0 || entry_price <= stop_price {
            return 0;
        }

        // 1. Calculate Valid Stop Distance (MinStopDistance)
        let raw_stop_dist = entry_price - stop_price;
        let stop_dist = raw_stop_dist.max(self.config.min_stop_distance_cents);

        // 2. Risk-Based Size (RiskPerTrade)
        // RiskAmount = Shares * StopDist
        // Shares = RiskAmount / StopDist
        let risk_shares = self.config.risk_per_trade_usd / stop_dist;

        // 3. Budget-Based Size (Budget cap & Max Position %)
        // Cap 1: Hard budget cap
        let budget_cap_shares = self.config.budget_cap_usd / entry_price;

        // Cap 2: % of NAV (Available Cash is proxy for NAV/Buying Power here)
        // Ensure we don't exceed max_position_pct_nav of TOTAL account balance
        // And also don't exceed available cash
        let nav_cap_usd = account_balance * self.config.max_position_pct_nav;
        let nav_cap_shares = nav_cap_usd / entry_price;

        // Also capped by absolute available cash
        let cash_cap_shares = available_cash / entry_price;

        let budget_shares = budget_cap_shares.min(nav_cap_shares).min(cash_cap_shares);

        // 4. Liquidity-Based Size (Liquidity cap)
        // Cap to % of daily volume
        let liquidity_shares = (daily_volume as f64) * self.config.liquidity_cap_pct;

        // Final Size = min(Risk, Budget, Liquidity)
        let final_shares = risk_shares.min(budget_shares).min(liquidity_shares);

        final_shares.floor() as u32
    }
}

/// Full fee and slippage model per §18.1 and §18.2.
#[derive(Debug, Clone)]
pub struct PricingModel {
    pub commission_per_share: f64,
    pub sec_fee_rate: f64,
    pub taf_rate: f64,
    pub slippage_alpha: f64,
    pub slippage_beta: f64,
    pub min_net_profit_usd: f64,
}

impl PricingModel {
    /// Total brokerage fees for a round-trip (buy + sell) trade.
    /// commission × 2 sides, SEC fee on sell side only, TAF on sell side.
    pub fn total_fees(&self, shares: u32, _entry_price: f64, exit_price: f64) -> f64 {
    pub fn total_fees(&self, shares: u32, entry_price: f64, exit_price: f64) -> f64 {
        let shares_f = shares as f64;
        let commission = self.commission_per_share * shares_f * 2.0; // buy + sell
        let sec_fee = self.sec_fee_rate * (exit_price * shares_f);
        let taf = self.taf_rate * shares_f;
        commission + sec_fee + taf
    }

    /// Expected slippage per §18.2.
    /// ImpactSlippage = beta × (shares / avg_depth_top3)
    /// SlippagePerShare = max(spread/2, alpha × vol_1m × price, ImpactSlippage)
    pub fn expected_slippage(
        &self,
        shares: u32,
        price: f64,
        spread_cents: f64,
        vol_1m: f64,
        avg_depth_top3: f64,
    ) -> f64 {
        let shares_f = shares as f64;
        let half_spread = spread_cents / 200.0; // convert cents to dollar, halved
        let volatility_slip = self.slippage_alpha * vol_1m * price;
        let impact_slip = if avg_depth_top3 > 0.0 {
            self.slippage_beta * (shares_f / avg_depth_top3)
        } else {
            self.slippage_beta * 0.01 // fallback: 1% impact if no depth data
        };
        let slip_per_share = half_spread.max(volatility_slip).max(impact_slip);
        slip_per_share * shares_f
    }

    /// Gross profit estimate per §18 — min(50.0, 10% price move × shares).
    pub fn gross(&self, entry_price: f64, shares: u32) -> f64 {
        let target_10pct = entry_price * 0.10 * (shares as f64);
        target_10pct.min(50.0_f64)
    }

    /// Full ExpectedNet = Gross − TotalFees − ExpectedSlippage per §18.3.
    /// Returns the net profit estimate. Negative = reject.
    pub fn expected_net(
        &self,
        shares: u32,
        entry_price: f64,
        spread_cents: f64,
        vol_1m: f64,
        avg_depth_top3: f64,
    ) -> f64 {
        if shares == 0 {
            return -1.0;
        }
        // Exit price estimate = entry + 10% (optimistic, for fee calculation)
        let exit_price_est = entry_price * 1.10;
        let gross = self.gross(entry_price, shares);
        let fees = self.total_fees(shares, entry_price, exit_price_est);
        let slippage = self.expected_slippage(shares, entry_price, spread_cents, vol_1m, avg_depth_top3);
        gross - fees - slippage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_constrained_size() {
        let config = SizingConfig {
            risk_per_trade_usd: 100.0,
            min_stop_distance_cents: 0.05,
            max_position_pct_nav: 1.0, // High cap
            liquidity_cap_pct: 1.0,    // High cap
            budget_cap_usd: 1_000_000.0,
        };
        let sizer = PositionSizer::new(config);

        // Entry 10.00, Stop 9.90 -> Dist 0.10. Risk 100. Shares = 1000.
        // Account 100k, Cash 100k
        let shares = sizer.calculate_size(100_000.0, 10.00, 9.90, 1_000_000, 100_000.0);
        assert_eq!(shares, 1000);
    }

    #[test]
    fn test_min_stop_distance() {
        let config = SizingConfig {
            risk_per_trade_usd: 100.0,
            min_stop_distance_cents: 0.10, // Enforce min 10 cents
            max_position_pct_nav: 1.0,
            liquidity_cap_pct: 1.0,
            budget_cap_usd: 1_000_000.0,
        };
        let sizer = PositionSizer::new(config);

        // Entry 10.00, Stop 9.95 -> Raw Dist 0.05. Used Dist 0.10.
        // Shares = 100 / 0.10 = 1000 (instead of 2000)
        let shares = sizer.calculate_size(100_000.0, 10.00, 9.95, 1_000_000, 100_000.0);
        assert_eq!(shares, 1000);
    }

    #[test]
    fn test_budget_cap() {
        let config = SizingConfig {
            risk_per_trade_usd: 1000.0,
            min_stop_distance_cents: 0.01,
            max_position_pct_nav: 1.0,
            liquidity_cap_pct: 1.0,
            budget_cap_usd: 5000.0, // Cap at $5k
        };
        let sizer = PositionSizer::new(config);

        // Entry 10.00. Budget Cap -> 500 shares.
        // Risk would allow: 1000 / 0.10 = 10000 shares.
        let shares = sizer.calculate_size(100_000.0, 10.00, 9.90, 1_000_000, 100_000.0);
        assert_eq!(shares, 500);
    }

    #[test]
    fn test_max_position_pct() {
        let config = SizingConfig {
            risk_per_trade_usd: 1000.0,
            min_stop_distance_cents: 0.01,
            max_position_pct_nav: 0.10, // 10% of NAV
            liquidity_cap_pct: 1.0,
            budget_cap_usd: 1_000_000.0,
        };
        let sizer = PositionSizer::new(config);

        // Balance 50,000. Max Pos = 5,000.
        // Entry 10.00. Shares = 500.
        let shares = sizer.calculate_size(50_000.0, 10.00, 9.90, 1_000_000, 50_000.0);
        assert_eq!(shares, 500);
    }

    #[test]
    fn test_liquidity_cap() {
        let config = SizingConfig {
            risk_per_trade_usd: 10000.0,
            min_stop_distance_cents: 0.01,
            max_position_pct_nav: 1.0,
            liquidity_cap_pct: 0.01, // 1% of Vol
            budget_cap_usd: 1_000_000.0,
        };
        let sizer = PositionSizer::new(config);

        // Volume 50,000. Cap = 500 shares.
        let shares = sizer.calculate_size(100_000.0, 10.00, 9.90, 50_000, 100_000.0);
        assert_eq!(shares, 500);
    }

    #[test]
    fn test_pricing_model_fees() {
        let model = PricingModel {
            commission_per_share: 0.005,
            sec_fee_rate: 0.0000278,
            taf_rate: 0.000166,
            slippage_alpha: 0.5,
            slippage_beta: 0.3,
            min_net_profit_usd: 0.10,
        };
        // 100 shares at $5.00, exit at $5.50
        let fees = model.total_fees(100, 5.00, 5.50);
        // Commission: 0.005 * 100 * 2 = $1.00
        // SEC: 0.0000278 * 5.50 * 100 = $0.01529
        // TAF: 0.000166 * 100 = $0.0166
        assert!(fees > 1.0 && fees < 1.1, "fees = {fees}");
    }

    #[test]
    fn test_pricing_model_expected_net_positive() {
        let model = PricingModel {
            commission_per_share: 0.005,
            sec_fee_rate: 0.0000278,
            taf_rate: 0.000166,
            slippage_alpha: 0.5,
            slippage_beta: 0.3,
            min_net_profit_usd: 0.10,
        };
        // 200 shares at $2.00, tight spread, low vol, good depth
        let net = model.expected_net(200, 2.00, 2.0, 0.001, 10000.0);
        // Gross = min(50, 2.00*0.10*200) = min(50, 40) = 40
        // Should have positive net after fees
        assert!(net > 0.0, "expected_net = {net}");
    }

    #[test]
    fn test_pricing_model_expected_net_negative_high_fees() {
        let model = PricingModel {
            commission_per_share: 0.005,
            sec_fee_rate: 0.0000278,
            taf_rate: 0.000166,
            slippage_alpha: 0.5,
            slippage_beta: 0.3,
            min_net_profit_usd: 0.10,
        };
        // 1 share at $0.30 — fees will overwhelm gross
        let net = model.expected_net(1, 0.30, 10.0, 0.05, 100.0);
        assert!(net < 0.0, "should be net negative for tiny position: {net}");
    }
}
