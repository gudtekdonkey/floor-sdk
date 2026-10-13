//! The mechanism constants, in Solana base units (6-decimal tokens, lamports).
//!
//! Tick constants are copied from the EVM engine unchanged (a tick is a tick on both chains); the
//! quantity constants are the EVM per-token constants restated at 6 decimals and in lamports.
//!
//! The bid is a BAND (`BID_BAND_TICKS`), and the at-floor skim endpoints are per-pool values, not hook constants.

/// Distance over which the skim ramps; 16094 ticks ≈ 5×.
pub const SKIM_RAMP_TICKS: i32 = 16094;

/// The bid band's width in ticks BEFORE spacing rounding (the EVM's `SingletonFloorEngine`): the
/// bid is `[floor_tick − W, floor_tick]` with `W = ceil_to_spacing(BID_BAND_TICKS, s)` (`band::band_ticks`).
/// `floor_tick` is the band's CHEAP edge, the published floor; the backing solves to its DEAR edge.
/// ⛔ With the band's token capacity above the sellable float, no sell can carry the market through the cheap edge.
pub const BID_BAND_TICKS: i32 = 120;

/// ln 2 / ln 1.0001 (the EVM's `FloorMath`); also `MIN_ASK_SPREAD_TICKS` there.
pub const TICKS_PER_DOUBLING: i32 = 6932;

/// Flat LP fee override, hundredths of a bip.
pub const LP_FEE_PIPS: u32 = 2500;
/// The at-floor skim endpoints (the EVM's `FloorPolicy.DEFAULT_SKIM_*_AT_FLOOR_BPS`). The BUY endpoint (0) is written
/// at `register_pool`; the SELL one is NOT — a pool registers with [`skim_sell_at_floor_bps`]`(skim_sell_far_bps)` at
/// its multiple. `DEFAULT_SKIM_SELL_AT_FLOOR_BPS` (1000) is kept for the EVM pin (the EVM's per-launch family reads it).
pub const DEFAULT_SKIM_BUY_AT_FLOOR_BPS: u64 = 0;
pub const DEFAULT_SKIM_SELL_AT_FLOOR_BPS: u64 = 1000;
/// The dynamic fee scales only the floor skim: `FloorPolicy.SKIM_SELL_AT_FLOOR_MULTIPLE_BPS = 15_000` on the EVM
/// (1.5x the far rate, no cap). The same name and value as
/// `floor_types::constants::SKIM_SELL_AT_FLOOR_MULTIPLE_BPS` (pinned by `tests/floor_types_pin.rs`).
pub const SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u64 = 15_000;
/// The at-floor SELL endpoint a pool is registered with: `sell_far · 15_000 / 10_000`, rounded DOWN
/// (`SingletonFloorSetup._initialize`). NOT capped by `MAX_SKIM_FAR_BPS`; at that cap it is 1500, so it
/// always fits a `u16`. The creator and holder fees never ramp.
pub const fn skim_sell_at_floor_bps(sell_far: u16) -> u16 {
    (sell_far as u64 * SKIM_SELL_AT_FLOOR_MULTIPLE_BPS / 10_000) as u16
}
const _: () = assert!(skim_sell_at_floor_bps(crate::policy::MAX_SKIM_FAR_BPS) == 1500);
/// ⛔ THE AT-FLOOR SELL MULTIPLE IS PER LAUNCH — 1.5× on the three presets (Cook, Bluechip, Moonshot),
/// [`SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`]
/// (1.5×) on Custom and `$FLOOR`. The preset value is its own constant so a governance row can still carry up to
/// 5× ([`MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS`]). The same names and values as `floor_types::constants` (pinned by
/// `tests/floor_types_pin.rs`); the per-mode table is `modes::Preset::skim_sell_at_floor_multiple_bps`.
pub const PRESET_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u64 = 15_000;
pub const MIN_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u64 = 10_000;
pub const MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS: u64 = 50_000;
/// ⛔ The at-floor SELL skim's cap, 1,500 bps (no higher than 15 %).
/// Same name and value as `floor_types::constants::MAX_SKIM_SELL_AT_FLOOR_BPS` (pinned by `tests/floor_types_pin.rs`).
pub const MAX_SKIM_SELL_AT_FLOOR_BPS: u64 = 1_500;
const _: () = assert!(skim_sell_at_floor_bps(crate::policy::MAX_SKIM_FAR_BPS) as u64 == MAX_SKIM_SELL_AT_FLOOR_BPS);
/// The at-floor SELL endpoint at a given multiple: `sell_far · multiple_bps / 10_000`, rounded DOWN, saturating
/// at `u16::MAX` (the caller bounds both inputs; at the far cap and 5× it is 5,000).
pub const fn skim_sell_at_floor_bps_at(sell_far: u16, multiple_bps: u64) -> u16 {
    let v = (sell_far as u128 * multiple_bps as u128) / 10_000;
    if v > u16::MAX as u128 { u16::MAX } else { v as u16 }
}
const _: () = assert!(skim_sell_at_floor_bps_at(crate::policy::MAX_SKIM_FAR_BPS, MAX_SKIM_SELL_AT_FLOOR_MULTIPLE_BPS) == 5_000);

/// Token supply split: 1e9 tokens at 6 decimals; the classic split is 30 / 10 / 60 (sale / POL / staircase). Twins of
/// the EVM's `FloorFactoryStorage.SALE / POL / VAULT`; `tests/floor_types_pin.rs` holds them equal to
/// `floor_types::constants::{SALE,POL,LADDER}_TOKENS`.
pub const SUPPLY: u64 = 1_000_000_000_000_000;
pub const SALE: u64 = 300_000_000_000_000;
pub const POL: u64 = 100_000_000_000_000;
pub const LADDER: u64 = 600_000_000_000_000;
const _: () = assert!(SALE + POL + LADDER == SUPPLY);

/// Below this much sellable supply the floor is undefined: 1,000 tokens (`1_000e18` at 18 dp on the EVM).
pub const MIN_SELLABLE: u64 = 1_000_000_000;

/// Position salts.
pub const BID_SALT: u64 = 1;
pub const SEED_SALT: u64 = 3;
// Salts 2, 100 and 101 are retired; never re-use them.

/// Basis-point denominator.
pub const BPS: u64 = 10_000;

/// The share of a GROSS creator fee that funds the referrer ON A REFERRED POOL. 1000 bps = 10 %, the EVM's own
/// default.
///
/// ⛔ This is the value `register_pool` COPIES into `FloorPool.referral_share_bps`; `take_fees`
/// reads the pool's copy, never this constant, so an already-registered pool keeps the rate it was
/// created with whatever this becomes. The reason is structural: the Solana fee path is a swap CALLBACK whose
/// account list is fixed at `FLOOR_CALLBACK_ACCOUNTS`, so there is no other account to read a rate from there.
/// ⚠ THE COST: retuning needs a `floor_hook` upgrade and reaches only pools registered after it; nobody can raise
/// the referral take on a pool that already holds backing.
/// ⛔ Pinned equal to `floor_types::constants::REFERRAL_CREATOR_SHARE_BPS` by
/// `tests/floor_types_pin.rs`; change both or neither.
pub const REFERRAL_CREATOR_SHARE_BPS: u64 = 1_000;

/// The hard ceiling on a referral share — the EVM's own bound (`bps > 10_000` reverts there).
/// `take_fees` refuses past it (`FloorMathError::ReferralShareOutOfRange`) because the subtraction
/// `cfee - referral` would otherwise underflow.
pub const MAX_REFERRAL_SHARE_BPS: u64 = 10_000;

/// ⛔ **Quote tokens may have any number of decimals.** The admissible decimals of a FLOOR launch's QUOTE mint,
/// inclusive.
///
/// Two properties look like decimals bounds and are NOT; they are CAVEATS a lister must weigh:
/// - fee rounding. [`crate::fees`]'s `part` floors `leg · bps / 10_000`, so a leg under `10_000/bps` base units pays
///   ZERO fee, and at 2 decimals a 10 % at-floor sell skim is evaded by splitting into sub-$0.10 sells. TRUE, and NOT
///   ARITHMETIC: it is an economic property of a coarse quote, it scales with the quote's PRICE and not with its
///   decimals alone. ⚠ It is a real caveat at `list_quote` time.
/// - the u64 lifetime meters. [`crate::state::FloorState`]'s meters are u64 and `take_fees` bumps two with a CHECKED
///   add INSIDE the swap (`platform_paid_eth`, `referral_fees_paid`), so the first swap past `u64::MAX` reverts EVERY
///   fee-bearing swap on that pool for ever. TRUE, and NOT DECIMALS-SHAPED: the meter's ceiling in MONEY is
///   `1.8e19 · (the dollar value of one raw unit)`, so a 6-decimal quote worth $1M a whole unit ($1 a raw unit) hits it
///   far sooner than an 18-decimal quote worth $0.000001. The quantity that binds is the value of ONE RAW UNIT, which
///   no decimals count determines.
///
/// ⛔ **WHAT DOES BIND — AND IT IS THE TRANCHE-PRICE CAP, NOT AN INTEGER WIDTH.** `Mint.decimals`
/// is a `u8`, so the domain is `0..=255`, and
/// the ONE place a decimals count enters the mechanism is [`crate::modes::tranche_price_x64`]:
/// `p = floor(mcap · 100 · 10^d · 2^64 / (cents · SUPPLY))`, computed in U256 / U512 and required
/// to sit in `0 < p < 2^77` ([`crate::modes::MAX_TRANCHE_PRICE_X64`], the bound that keeps the
/// whole supply's cost inside a u64). `p` rises with `10^d`, falls with `cents` and rises with
/// `mcap`, so the CHEAPEST price any `d` can produce is at the smallest cap the program admits
/// (`$1` — `graduation_mcap_usd == 0` is refused and nothing else is) against the dearest quote a
/// u64 can state (`u64::MAX` cents a whole unit):
///
/// ```text
/// p_min(d) = floor(100 · 10^d · 2^64 / (u64::MAX · SUPPLY))
/// p_min(d) < 2^77   ⟺   100 · 10^d < 2^13 · u64::MAX · SUPPLY   (≈ 1.51e38)   ⟺   d ≤ 36
/// ```
///
/// At `d = 36` that price is `10^23 · 2^64/(2^64−1)` (5,421 over `10^23`), inside the cap, so
/// SOME launch prices; at `d = 37` it is ten times that, `> 2^77 ≈ 1.51e23`, so NO (cap, quote
/// price) the wire can carry prices a launch, and
/// admitting the mint would only move the refusal from the roster to every `create_launch` on
/// it. [`MAX_QUOTE_DECIMALS`] is that `36`, DERIVED by [`some_launch_prices_at`] and the two
/// `const` asserts below rather than chosen; `MIN_QUOTE_DECIMALS` is the u8's own floor, and a
/// 0-decimal quote prices at every preset cap against any ordinary price (asserted in
/// `modes.rs`).
///
/// ⚠ "One whole unit of the quote must fit a u64" is NOT a bound of the arithmetic: nothing in the mechanism ever
/// holds or moves a whole unit — every amount is raw units, the price is a RATIO of raw units, and
/// `tranche_price_x64` widens `10^d` into U256 before it multiplies. A `d = 20` mint priced at $70,000 a whole unit
/// against a $5,000 cap prices perfectly well (`p ≈ 7,142 < 8,192` raw units per token unit, a raise of `4.5e18` raw
/// units, inside u64). The rule is "bound only what the arithmetic actually bounds"; `36` is that bound.
///
/// ⚠ **THE EFFECTIVE CEILING FOR A REAL LAUNCH IS FAR LOWER, VALUE-SHAPED, AND REFUSED BY NAME
/// PER LAUNCH.** `10^d < 8192 · cents · 1e13 / mcap`: at a $30,000 cap against a $1.00 quote that is `d ≤ 14`; at
/// $60,000, `d ≤ 14`; against a $0.01 quote, `d ≤ 11`; against a $230 quote, `d ≤ 16`; against a $100,000 quote at a
/// $5,000 Custom cap, `d ≤ 20`. Every one of those is `TranchePriceOutOfRange` — by name, per (cap, price, d) triple,
/// which is the honest place for a bound that depends on all three. A decimals constant cannot express it and this
/// one does not try to.
///
/// ⚠ The window is enforced at TWO points on purpose — [`crate::modes::tranche_price_x64`] refuses
/// outside it, and `floor_hook::list_quote` refuses to put such a mint on the roster at all — so
/// a caller that reaches the math with a bad mint is stopped even if a future admission path
/// forgets. Pinned equal to `floor_types::constants::MIN/MAX_QUOTE_DECIMALS` by
/// `tests/floor_types_pin.rs`, and to the TS SDK's pair by its own `tickmath.test.ts`.
pub const MIN_QUOTE_DECIMALS: u8 = 0;
pub const MAX_QUOTE_DECIMALS: u8 = 36;

/// **THE DERIVATION OF [`MAX_QUOTE_DECIMALS`], as a function the compiler evaluates.** Does ANY
/// (cap, quote price) the wire can carry price a launch on a `d`-decimal quote? That is
/// `p_min(d) < 2^77`, i.e. `100 · 10^d < 2^13 · u64::MAX · SUPPLY`; a `10^d` or a `100 · 10^d`
/// that overflows a u128 is past the right-hand side (≈ 1.51e38) and answers `false` by
/// construction rather than by a wrapped comparison.
///
/// ⛔ The inputs are the mechanism's own: `SUPPLY` (the denominator's token count), `2^13` (the
/// cap `2^77` over the `2^64` fixed point) and `u64::MAX` (the widest `quote_usd_cents` the wire
/// carries). Change any of them and the window moves WITH it, at compile time, instead of a
/// literal going stale.
pub const fn some_launch_prices_at(decimals: u8) -> bool {
    const RHS: u128 = 8192 * (u64::MAX as u128) * (SUPPLY as u128);
    match 10u128.checked_pow(decimals as u32) {
        Some(unit) => match unit.checked_mul(100) {
            Some(lhs) => lhs < RHS,
            None => false,
        },
        None => false,
    }
}
// ⛔ THE WINDOW IS THE DERIVATION, checked by the compiler: `MAX_QUOTE_DECIMALS` is exactly the
//    last `d` at which some launch prices. Move the literal either way and one of these fails to
//    compile.
const _: () = assert!(some_launch_prices_at(MAX_QUOTE_DECIMALS));
const _: () = assert!(!some_launch_prices_at(MAX_QUOTE_DECIMALS + 1));
const _: () = assert!(MIN_QUOTE_DECIMALS == 0, "u8's own floor; 10^0 = 1 raw unit");
// …and the unit of the whole window fits the u128 `quote_unit` hands `tranche_price_x64`.
const _: () = assert!(quote_unit(MAX_QUOTE_DECIMALS).is_some());
const _: () = assert!(quote_unit(MAX_QUOTE_DECIMALS + 1).is_none());

/// `10^d`: ONE WHOLE UNIT of the quote, in that quote's own base units — `None` outside
/// [`MIN_QUOTE_DECIMALS`]`..=`[`MAX_QUOTE_DECIMALS`].
///
/// ⛔ `None`, never a silent `1` and never a silent saturation: a zero-decimals fallback would
/// make every tranche price 10^d× too cheap and would stay a finite positive number all the way
/// to a deployed pool. ⛔ A `u128`, and the window is what bounds it, not the other way round:
/// `10^36 < 2^128` (the window's top is asserted to fit above), and `tranche_price_x64` widens it
/// into U256 before any multiply.
pub const fn quote_unit(decimals: u8) -> Option<u128> {
    if decimals > MAX_QUOTE_DECIMALS {
        return None;
    }
    10u128.checked_pow(decimals as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any amount of decimals: the window is the WHOLE range at which some launch can price under the `2^77`
    /// tranche cap, and `quote_unit` is exactly 10^d inside it and `None` outside.
    ///
    /// ⛔ THE DERIVATION, RESTATED AT RUNTIME WITH NO SHARED CODE: `p_min(d)` is computed here in
    /// U256 from the formula itself (the $1 cap against `u64::MAX` cents), and the window's top is
    /// asserted to be the last `d` at which it sits under `2^77` — so `some_launch_prices_at`, the
    /// const derivation, is checked against an independent oracle rather than against itself.
    ///
    /// Mutations this kills: a saturating or wrapping `pow` (d=37 would then carry a unit — a
    /// wrong, finite, positive one); a `_ => Some(1)` fallback (every price 10^d too cheap,
    /// silently); either end of the window moved off the derivation (a `35` or a `37` fails the
    /// oracle by name); `some_launch_prices_at` comparing with `<=` (37 would pass); the `100`
    /// dropped from its left-hand side (the top would read 38).
    #[test]
    fn the_quote_decimals_window_is_where_some_launch_still_prices_under_the_cap() {
        use carpenter_clmm_math::big_num::U256;
        use carpenter_clmm_math::fixed_point_64::Q64;
        use carpenter_clmm_math::full_math::MulDiv;
        use crate::modes::MAX_TRANCHE_PRICE_X64;

        assert_eq!((MIN_QUOTE_DECIMALS, MAX_QUOTE_DECIMALS), (0, 36));

        // the oracle: the cheapest price the formula can produce at `d`, from the formula
        let p_min = |d: u8| -> U256 {
            let unit = (0..d).fold(U256::from(1u8), |a, _| a * U256::from(10u8));
            (U256::from(100u8) * unit)
                .mul_div_floor(U256::from(Q64), U256::from(u64::MAX) * U256::from(SUPPLY))
                .expect("fits U512")
        };
        let cap = U256::from(MAX_TRANCHE_PRICE_X64);
        assert!(p_min(36) < cap, "d=36: {} prices", p_min(36));
        assert!(p_min(37) >= cap, "d=37: {} is past the cap for EVERY input", p_min(37));
        // the derivation's own figure: 10^23 · 2^64 / (2^64 − 1), which is 10^23 plus the 5,421
        // that the `u64::MAX` (not 2^64) denominator adds — about two thirds of the cap, so the
        // window's top is not a near miss at either end: 36 clears by a third, 37 overshoots ×6.6
        let ten23 = U256::from(10u128.pow(23));
        assert_eq!(p_min(36), ten23 + U256::from(5_421u32), "10^23 · 2^64 / (2^64 − 1)");
        assert!(p_min(36) * U256::from(2u8) > cap, "36 is not a near miss: p_min is over half the cap");
        assert!(p_min(37) > cap * U256::from(6u8), "37 is not a near miss: p_min is over six times the cap");

        for d in 0..=255u8 {
            let some = some_launch_prices_at(d);
            assert_eq!(some, d <= 36, "some_launch_prices_at({d})");
            if d <= 40 {
                // past ~40 the oracle's own U256 unit overflows; the const fn answers `false`
                // there by construction and the window is far below it anyway
                assert_eq!(some, p_min(d) < cap, "d={d}: the const derivation vs the oracle");
            }
            let got = quote_unit(d);
            if (MIN_QUOTE_DECIMALS..=MAX_QUOTE_DECIMALS).contains(&d) {
                assert_eq!(got, Some(10u128.pow(d as u32)), "10^{d}");
            } else {
                assert_eq!(got, None, "{d} is outside the window and must carry no unit");
            }
        }
        // ⛔ The two low extremes, named rather than left to the loop: a 0-decimal quote and a 2-decimal one must
        //    WORK, not merely "not revert".
        assert_eq!(quote_unit(0), Some(1), "a 0-decimal quote's whole unit IS one raw unit");
        assert_eq!(quote_unit(2), Some(100));
        // ⚠ 20 is inside the window: a `u64` whole unit was never a bound of the arithmetic
        assert_eq!(quote_unit(20), Some(10u128.pow(20)), "a whole unit past u64 is still a unit");
    }

    // The band width and the EVM's at-floor skim defaults.
    #[test]
    fn band_multiple_and_skim_defaults_are_the_engine_values() {
        assert_eq!(BID_BAND_TICKS, 120);
        assert_eq!((DEFAULT_SKIM_BUY_AT_FLOOR_BPS, DEFAULT_SKIM_SELL_AT_FLOOR_BPS), (0, 1000));
        assert!(DEFAULT_SKIM_SELL_AT_FLOOR_BPS <= crate::policy::MAX_SKIM_FAR_BPS as u64);
    }

    // The at-floor sell multiple. Mutations: the multiple capped at MAX_SKIM_FAR_BPS, or rounded up.
    #[test]
    fn at_floor_sell_is_one_and_a_half_times_far_uncapped() {
        assert_eq!(SKIM_SELL_AT_FLOOR_MULTIPLE_BPS, 15_000);
        assert_eq!(skim_sell_at_floor_bps(0), 0);
        assert_eq!(skim_sell_at_floor_bps(150), 225);
        assert_eq!(skim_sell_at_floor_bps(200), 300);
        assert_eq!(skim_sell_at_floor_bps(crate::policy::MAX_SKIM_FAR_BPS), 1500);
        assert_eq!(skim_sell_at_floor_bps(1), 1);
        assert_eq!(skim_sell_at_floor_bps(333), 499);
    }
}
