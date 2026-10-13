//! THE QUOTE LINEAGE: the pure arithmetic `floor_hook::register_pool` and `floor_launch::create_launch` run when a
//! launch is quoted in a mint no governance roster names (allow-all quotes, automatic admission, the smart tip cap):
//!
//! - [`min_bid_for`] — a quote with no `QuotePolicy` takes `min_bid = 0.001` of ONE WHOLE TOKEN, rounded UP to at
//!   least one raw unit: `ceil(10^decimals / 1,000)`, never 0. The twin of the EVM's `FloorPolicy.minBidFor`
//!   (`_minBid()` rounded up). A raw amount of the QUOTE, like every `min_bid`.
//! - [`quote_amount_at_tick`] — the smart tip cap. A child launch quoted in a graduated FLOOR token `F` gets
//!   `max_bounty` (in raw `F`) = the PARENT pool's `max_bounty` (in raw parent quote) converted at the parent's price.
//!   On every FLOOR pool the quote is token0 and the base token1, so at tick `t` one raw quote unit is
//!   `1.0001^t` raw base units, i.e. `amount · sqrtP² / 2^128` with `sqrtP` the pool's Q64.64 square-root price. The
//!   raw-unit ratio already carries both mints' decimals, so no decimals count enters here.
//!
//! ⛔ Overflow-safe by construction: the product is taken in U256 in two `>> 64` steps (the second rounds down) and
//! SATURATES at `u64::MAX` — a cap is a ceiling, and a ceiling past the representable range is the range. Never a
//! revert: registration must never fail over the cap.

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::full_math::MulDiv;
use carpenter_clmm_math::tick_math::{get_sqrt_price_at_tick, MAX_TICK, MIN_TICK};

use crate::error::{FloorMathError, FloorMathResult};
use crate::modes::MAX_TRANCHE_PRICE_X64;

/// The parent's TWAP window for the smart tip cap — **1,800 s (30 minutes)**, the EVM twin's, NOT the cushion's 900 s:
/// the cap is written ONCE at a child's registration, so it reads the
/// longer, harder-to-move average. Pinned by `the_smart_cap_reads_the_evms_thirty_minute_twap`.
pub const SMART_CAP_TWAP_WINDOW_SECS: u32 = 1_800;

/// The largest decimals count whose `10^d` fits a `u128` with room (`10^38 < 2^127`). A mint above it cannot be a
/// quote here anyway (`MAX_QUOTE_DECIMALS` is 36); `min_bid_for` saturates rather than overflows past it.
const POW10_U128_MAX_EXP: u8 = 38;

/// `ceil(10^decimals / 1,000)`, at least 1 — the default `min_bid` of a quote listed by no governance roster
/// (R5 allow-all and R6 automatic admission). ⚠ Saturates at `u64::MAX` for `decimals ≥ 23` (`10^22` raw units is
/// 10^19 / 1,000 … the `u64` ceiling); `register_pool` bounds the decimals at 36 and the dust line is a ceiling for
/// such a mint whatever it reads.
pub fn min_bid_for(decimals: u8) -> u64 {
    if decimals > POW10_U128_MAX_EXP {
        return u64::MAX;
    }
    let unit = 10u128.pow(decimals as u32);
    let v = unit.div_ceil(1_000).max(1);
    u64::try_from(v).unwrap_or(u64::MAX)
}

/// `amount · sqrt_price_x64² / 2^128`, rounded DOWN, saturating at `u64::MAX` — how many raw units of a FLOOR pool's
/// BASE `amount` raw units of its QUOTE are worth at the price `sqrt_price_x64` (the quote is token0). The smart tip
/// cap's conversion.
pub fn quote_amount_at_sqrt(amount: u64, sqrt_price_x64: u128) -> u64 {
    if amount == 0 || sqrt_price_x64 == 0 {
        return 0;
    }
    let s = U256::from(sqrt_price_x64);
    // (amount · s) >> 64, then · s >> 64: each product < 2^64 · 2^128 · … fits U256 (amount < 2^64, s < 2^128).
    let a = (U256::from(amount) * s) >> 64;
    let v = (a * s) >> 64;
    if v > U256::from(u64::MAX) {
        u64::MAX
    } else {
        v.as_u64()
    }
}

/// [`quote_amount_at_sqrt`] at a TICK. A tick outside the AMM's range answers 0 — the conservative fallback.
pub fn quote_amount_at_tick(amount: u64, tick: i32) -> u64 {
    match get_sqrt_price_at_tick(tick) {
        Ok(s) => quote_amount_at_sqrt(amount, s),
        Err(_) => 0,
    }
}

/// THE CHAINED QUOTE: a child launch quoted in a FLOOR token prices its parent's token from the parent pool's own
/// average; admission is automatic on chain and the hops are unlimited. The twin of the EVM's
/// `FloorPolicy.FLOOR_QUOTE_PRICE_TWAP_WINDOW`: each parent hop is read
/// over this window. ⛔ Kept apart from [`SMART_CAP_TWAP_WINDOW_SECS`] on purpose (the EVM's two numbers): the tip cap
/// reads the settled TWAP alone (fallback 0), the sizing the CHEAPER of the strict TWAP and the spot (no fallback — a
/// parent whose ring does not reach the window refuses the child).
pub const CHAIN_PRICE_TWAP_WINDOW_SECS: u32 = 1_800;

/// One parent hop's tick in the quote-is-token0 frame (raw parent BASE per raw parent QUOTE = `1.0001^t`; a HIGHER tick is a
/// CHEAPER parent token): the higher of the TWAP and the live tick — the cheaper parent, the EVM's `twap > spot ? twap :
/// spot`. A pumped spot cannot make a parent dearer here, so it cannot sell the child's sale too cheap.
pub fn chain_hop_tick(twap_tick: i32, live_tick: i32) -> i32 {
    twap_tick.max(live_tick)
}

/// THE CHAINED TRANCHE PRICE. `root_price_x64` is [`crate::tranche_price_x64`] in the ROOT quote's raw units (the
/// first non-FLOOR quote up the chain, its stated USD and its decimals); `chain_ticks` is Σ [`chain_hop_tick`] over the
/// parent hops. One raw ROOT unit buys `1.0001^Σt` raw units of the child's immediate quote, so the child's price in its
/// immediate quote is `root_price_x64 · sqrtP(Σt)² / 2^128`, ONE floor rounding (U256 · U256 widened to U512 inside
/// `mul_div_floor`). `chain_ticks == 0` returns the root price unchanged (a plain quote prices exactly as before).
/// ⛔ Refused, never clamped: `Σt` outside the AMM's tick range, or a result outside `(0, 2^77)` — the EVM's
/// `BadConfig` on the same two conditions.
pub fn chained_price_x64(root_price_x64: u128, chain_ticks: i64) -> FloorMathResult<u128> {
    if chain_ticks == 0 {
        return Ok(root_price_x64);
    }
    let t = i32::try_from(chain_ticks).map_err(|_| FloorMathError::TranchePriceOutOfRange)?;
    if !(MIN_TICK..=MAX_TICK).contains(&t) {
        return Err(FloorMathError::TranchePriceOutOfRange);
    }
    let s = U256::from(get_sqrt_price_at_tick(t).map_err(|_| FloorMathError::TranchePriceOutOfRange)?);
    // root < 2^77 and s < 2^128, so root · s < 2^205 fits U256; the second factor widens inside `mul_div_floor`.
    let a = U256::from(root_price_x64) * s;
    let p = a.mul_div_floor(s, U256::one() << 128).ok_or(FloorMathError::TranchePriceOutOfRange)?;
    if p.is_zero() || p >= U256::from(MAX_TRANCHE_PRICE_X64) {
        return Err(FloorMathError::TranchePriceOutOfRange);
    }
    Ok(p.as_u128())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE CHAINED QUOTE: Σt = 0 is the root price bit for bit; a doubling hop doubles it; two hops sum; the hop takes
    /// the CHEAPER parent (the higher tick); out of range is refused by name, never clamped.
    #[test]
    fn the_chained_price_is_the_root_price_times_the_summed_hops() {
        let root = crate::tranche_price_x64(50_000, 15_000, 9).unwrap();
        assert_eq!(chained_price_x64(root, 0), Ok(root), "no hop: unchanged");
        let one = chained_price_x64(root, 6_932).unwrap();
        let ratio = one as f64 / root as f64;
        assert!((ratio - 2.0).abs() < 1e-3, "6,932 ticks ~ 2x, got {ratio}");
        let two = chained_price_x64(root, 6_932 + 23_027).unwrap();
        let r2 = two as f64 / root as f64;
        assert!((r2 - 20.0).abs() / 20.0 < 1e-3, "2x then 10x = 20x, got {r2}");
        let down = chained_price_x64(root, -6_932).unwrap();
        assert!(((down as f64 / root as f64) - 0.5).abs() < 1e-3);
        assert_eq!(chain_hop_tick(100, 50), 100, "the cheaper parent: the higher tick");
        assert_eq!(chain_hop_tick(-5, 7), 7);
        assert_eq!(chained_price_x64(root, MAX_TICK as i64 + 1), Err(FloorMathError::TranchePriceOutOfRange));
        assert_eq!(chained_price_x64(root, i64::MIN), Err(FloorMathError::TranchePriceOutOfRange));
        assert_eq!(chained_price_x64(root, 400_000), Err(FloorMathError::TranchePriceOutOfRange), "past 2^77");
        assert_eq!(chained_price_x64(1, -400_000), Err(FloorMathError::TranchePriceOutOfRange), "rounds to zero");
        assert_eq!(CHAIN_PRICE_TWAP_WINDOW_SECS, 1_800, "the EVM's FLOOR_QUOTE_PRICE_TWAP_WINDOW");
    }

    /// The EVM's `minBidFor`: 0.001 whole token, rounded UP, never 0.
    #[test]
    fn min_bid_is_a_thousandth_of_a_token_rounded_up_and_never_zero() {
        assert_eq!(min_bid_for(0), 1, "0 decimals: 1/1000 of one unit rounds UP to one raw unit");
        assert_eq!(min_bid_for(1), 1);
        assert_eq!(min_bid_for(2), 1);
        assert_eq!(min_bid_for(3), 1);
        assert_eq!(min_bid_for(4), 10);
        assert_eq!(min_bid_for(6), 1_000, "a 6-dp FLOOR token: 0.001 = 1,000 raw");
        assert_eq!(min_bid_for(8), 100_000);
        assert_eq!(min_bid_for(9), 1_000_000, "wSOL-shaped: 0.001 SOL = 1,000,000 lamports");
        assert_eq!(min_bid_for(18), 1_000_000_000_000_000);
        assert_eq!(min_bid_for(22), 10_000_000_000_000_000_000u128.min(u64::MAX as u128) as u64);
        assert_eq!(min_bid_for(36), u64::MAX, "saturates, never overflows");
        assert_eq!(min_bid_for(255), u64::MAX);
        for d in 0..=40u8 {
            assert!(min_bid_for(d) >= 1, "d = {d}");
        }
    }

    /// At tick 0 one raw quote is one raw base (to the rounding of the Q64.64 square root); a positive tick makes the
    /// base CHEAPER (more base per quote); the conversion is monotone in the tick and in the amount.
    #[test]
    fn the_cap_converts_at_the_raw_unit_ratio_and_is_monotone() {
        let at0 = quote_amount_at_tick(1_000_000, 0);
        assert!((999_999..=1_000_000).contains(&at0), "tick 0 ~ 1:1, got {at0}");
        // 1.0001^6932 ~ 2.0000: a doubling.
        let at_double = quote_amount_at_tick(1_000_000, 6_932);
        assert!((1_999_000..=2_000_100).contains(&at_double), "6,932 ticks ~ 2x, got {at_double}");
        let at_half = quote_amount_at_tick(1_000_000, -6_932);
        assert!((499_900..=500_100).contains(&at_half), "-6,932 ticks ~ 0.5x, got {at_half}");
        let mut prev = 0;
        for t in (-100_000..=100_000).step_by(997) {
            let v = quote_amount_at_tick(10_000_000_000, t);
            assert!(v >= prev, "monotone in the tick at {t}");
            prev = v;
        }
        assert!(quote_amount_at_tick(2, 1_000) >= quote_amount_at_tick(1, 1_000));
        assert_eq!(quote_amount_at_tick(0, 50_000), 0);
    }

    /// Two levels, different decimals: a 9-dp parent quote (wSOL-shaped) → a 6-dp FLOOR token `F` → a 6-dp grandchild
    /// token `G`. The cap is converted level by level at each parent's price and the decimals come in through the
    /// raw-unit price alone. Worked: 0.05 SOL cap; `F` trades at 0.0001 SOL per whole `F` (= 1e5 raw lamports per 1e6
    /// raw F → raw ratio 10 raw F per raw lamport → tick ln 10 / ln 1.0001 ≈ 23,027); `G` at 0.5 F per whole G (raw
    /// ratio 2 raw G per raw F → tick ≈ 6,932). Expected: 0.05 SOL = 500 whole F = 5e8 raw F; then 1,000 whole G = 1e9 raw G.
    #[test]
    fn the_cap_rides_two_levels_through_different_decimals() {
        let sol_cap = 50_000_000u64; // 0.05 SOL
        let f_cap = quote_amount_at_tick(sol_cap, 23_027);
        assert!((499_000_000..=500_100_000).contains(&f_cap), "500 F (6 dp), got {f_cap}");
        let g_cap = quote_amount_at_tick(f_cap, 6_932);
        assert!((998_000_000..=1_000_100_000).contains(&g_cap), "1,000 G (6 dp), got {g_cap}");
    }

    #[test]
    fn the_smart_cap_reads_the_evms_thirty_minute_twap() {
        assert_eq!(SMART_CAP_TWAP_WINDOW_SECS, 1_800);
    }

    /// Never a revert, never a wrap: the extremes saturate or answer 0.
    #[test]
    fn the_cap_saturates_at_the_extremes() {
        use carpenter_clmm_math::tick_math::{MAX_TICK, MIN_TICK};
        assert_eq!(quote_amount_at_tick(u64::MAX, MAX_TICK), u64::MAX);
        assert_eq!(quote_amount_at_sqrt(u64::MAX, u128::MAX), u64::MAX);
        assert_eq!(quote_amount_at_tick(1, MIN_TICK), 0);
        assert_eq!(quote_amount_at_tick(1_000, MAX_TICK + 1), 0, "an out-of-range tick is the conservative 0");
        assert_eq!(quote_amount_at_sqrt(1_000, 0), 0);
    }
}
