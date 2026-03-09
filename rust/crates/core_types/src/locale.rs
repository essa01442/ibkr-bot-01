//! Locale helper — maps enum codes to translation keys.
//! Translation itself happens in the UI/display layer, not in core logic.
//! Per §1.6: logs store codes (enum u8), UI reads .ftl files.

use crate::RejectReason;

impl RejectReason {
    /// Returns the Fluent message key for this reason.
    pub fn fluent_key(self) -> &'static str {
        match self {
            Self::Blocklist           => "reject-blocklist",
            Self::CorporateActionBlock=> "reject-corporate-action-block",
            Self::PriceRange          => "reject-price-range",
            Self::Liquidity           => "reject-liquidity",
            Self::Regime              => "reject-regime",
            Self::DailyContext        => "reject-daily-context",
            Self::MtfVeto             => "reject-mtf-veto",
            Self::AntiChase           => "reject-anti-chase",
            Self::GuardSpread         => "reject-guard-spread",
            Self::GuardImbalance      => "reject-guard-imbalance",
            Self::GuardStale          => "reject-guard-stale",
            Self::GuardSlippage       => "reject-guard-slippage",
            Self::GuardL2Vacuum       => "reject-guard-l2-vacuum",
            Self::GuardFlicker        => "reject-guard-flicker",
            Self::TapeScoreLow        => "reject-tape-score-low",
            Self::NetNegative         => "reject-net-negative",
            Self::Exposure            => "reject-exposure",
            Self::TapeReversal        => "reject-tape-reversal",
            Self::MonitorOnly         => "reject-monitor-only",
            Self::MaxDailyLoss        => "reject-max-daily-loss",
            Self::PdtViolation        => "reject-pdt-violation",
            Self::Unknown             => "reject-unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_reasons_have_keys() {
        // Every variant must return a non-empty key
        let reasons = [
            RejectReason::Blocklist, RejectReason::CorporateActionBlock,
            RejectReason::PriceRange, RejectReason::Liquidity,
            RejectReason::Regime, RejectReason::DailyContext,
            RejectReason::MtfVeto, RejectReason::AntiChase,
            RejectReason::GuardSpread, RejectReason::GuardImbalance,
            RejectReason::GuardStale, RejectReason::GuardSlippage,
            RejectReason::GuardL2Vacuum, RejectReason::GuardFlicker,
            RejectReason::TapeScoreLow, RejectReason::NetNegative,
            RejectReason::Exposure, RejectReason::TapeReversal,
            RejectReason::MonitorOnly, RejectReason::MaxDailyLoss,
            RejectReason::PdtViolation,
        ];
        for reason in &reasons {
            let key = reason.fluent_key();
            assert!(!key.is_empty(), "Empty key for {:?}", reason);
            assert!(key.starts_with("reject-"), "Key must start with reject-: {}", key);
        }
    }
}
