//! The one error type of `floor-math`. Program crates map it to their own Anchor codes.

use core::fmt;

use carpenter_clmm_math::MathError;

/// Every failure `floor-math` can report. Nothing in this crate panics on an input its public API
/// accepts: where the EVM reverts, this returns `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloorMathError {
    /// A tick / sqrt-price / amount failure from `carpenter-clmm-math`.
    Math(MathError),
    /// `sqrt_price_from_ratio` with zero tokens, or a zero price (the EVM's `ZeroDenominator`).
    ZeroDenominator,
    /// A checked add / sub / narrowing overflowed (the EVM's 0.8 checked arithmetic reverts here).
    Overflow,
    /// `supply < inventory_tokens + ladder_tokens_remaining` (the EVM's subtraction underflows and reverts).
    SupplyUnderflow,
    /// A tick spacing of zero or less (the EVM divides by it and reverts).
    InvalidTickSpacing,
    /// A position range whose upper sqrt price is not above its lower (`b - a` underflow / zero divisor).
    InvalidRange,
    /// `tranche_price_x64` outside `(0, 2^77)`.
    TranchePriceOutOfRange,
    /// A mode byte that names no `Mode`.
    UnknownMode,
    /// A supply split outside the rule: each zone >= `MIN_ZONE_BPS`, the three summing to supply.
    BadSplit,
    /// `FloorState.referral_share_bps > 10_000` on a pool that funds referrals — the EVM's `ReferralFundingFailed`
    /// (`if (bps > 10_000) revert`) and `FloorReferralDistributor.InvalidShare`.
    /// ⛔ NOT cosmetic and not unreachable-by-luck: without it `referral = cfee·bps/10_000` exceeds
    /// the gross creator fee and `cfee − referral` underflows, so a pool with a corrupt rate would
    /// wrap the creator's accrual instead of refusing the swap. `register_pool` already bounds the
    /// field at `MAX_REFERRAL_SHARE_BPS`; this is the second gate, at the point of use.
    ReferralShareOutOfRange,
    /// A quote mint whose `decimals` is outside `MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS` reached
    /// [`crate::modes::tranche_price_x64`].
    /// ⛔ The window is 0..=36: the one bound is the tranche-price cap itself — past 36 no (cap, quote price) the wire
    /// can carry prices a launch (`constants::some_launch_prices_at`).
    /// ⛔ It is a REFUSAL, never a clamp and never a default: a wrong decimals count does not
    /// produce a wrong-looking number, it produces a perfectly ordinary price that is 10^k out.
    QuoteDecimalsOutOfRange,
}

pub type FloorMathResult<T> = core::result::Result<T, FloorMathError>;

impl From<MathError> for FloorMathError {
    fn from(e: MathError) -> Self {
        FloorMathError::Math(e)
    }
}

impl fmt::Display for FloorMathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloorMathError::Math(e) => write!(f, "clmm math: {e}"),
            FloorMathError::ZeroDenominator => f.write_str("zero denominator"),
            FloorMathError::Overflow => f.write_str("arithmetic overflow"),
            FloorMathError::SupplyUnderflow => {
                f.write_str("supply is below inventory plus ladder tokens")
            }
            FloorMathError::InvalidTickSpacing => f.write_str("tick spacing must be at least 1"),
            FloorMathError::InvalidRange => f.write_str("position range is empty or inverted"),
            FloorMathError::TranchePriceOutOfRange => {
                f.write_str("tranche price outside (0, 2^77)")
            }
            FloorMathError::UnknownMode => f.write_str("unknown launch mode"),
            FloorMathError::BadSplit => f.write_str("supply split outside the rule"),
            FloorMathError::ReferralShareOutOfRange => {
                f.write_str("referral share outside 0..=10000")
            }
            FloorMathError::QuoteDecimalsOutOfRange => {
                // ⚠ No literal: a message carrying a number is a second place for it to go stale.
                f.write_str("quote mint decimals outside the admissible window")
            }
        }
    }
}

impl std::error::Error for FloorMathError {}
