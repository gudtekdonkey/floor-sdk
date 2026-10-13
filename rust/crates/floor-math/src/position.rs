//! Position math in pool coordinates: `position_liquidity`, `position_amounts`, `bid_amounts`,
//! `pol_base`, `quote_cost` (the EVM's `FloorPositionMath`, the non-`reverse` branch only: every
//! FLOOR pool has the quote at token0).
//!
//! Rounding directions (all as the EVM's native branch):
//! - liquidity from amounts: DOWN (`FullMath.mulDiv`), so a deployed position never needs more than
//!   it was given;
//! - amounts from liquidity: DOWN (`getAmount{0,1}Delta(…, false)`), so the solver never
//!   over-excludes POL tokens and a live bid never overstates what it holds;
//! - `quote_cost`: DOWN.

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::fixed_point_64::Q64;
use carpenter_clmm_math::full_math::MulDiv;
use carpenter_clmm_math::liquidity_math::{get_delta_amount_0_unsigned, get_delta_amount_1_unsigned};
use carpenter_clmm_math::tick_math::get_sqrt_price_at_tick;

use crate::error::{FloorMathError, FloorMathResult};

fn bounds(lower: i32, upper: i32) -> FloorMathResult<(u128, u128)> {
    Ok((get_sqrt_price_at_tick(lower)?, get_sqrt_price_at_tick(upper)?))
}

/// `_liq0(a, b, amount) = mulDiv(amount, mulDiv(a, b, 2^64), b − a)` (`:29-31`, Q96 → Q64).
fn liq0(a: u128, b: u128, amount: u64) -> FloorMathResult<U256> {
    if b <= a {
        return Err(FloorMathError::InvalidRange);
    }
    let ab = U256::from(a)
        .mul_div_floor(U256::from(b), U256::from(Q64))
        .ok_or(FloorMathError::Overflow)?;
    U256::from(amount)
        .mul_div_floor(ab, U256::from(b - a))
        .ok_or(FloorMathError::Overflow)
}

/// `_liq1(a, b, amount) = mulDiv(amount, 2^64, b − a)` (`:33-35`).
fn liq1(a: u128, b: u128, amount: u64) -> FloorMathResult<U256> {
    if b <= a {
        return Err(FloorMathError::InvalidRange);
    }
    U256::from(amount)
        .mul_div_floor(U256::from(Q64), U256::from(b - a))
        .ok_or(FloorMathError::Overflow)
}

/// The liquidity a position over `[lower, upper]` gets from `quote` (token0) and `base` (token1) at
/// sqrt price `price` (the EVM's `FloorPositionMath`): all-quote at or below `sa`, all-base at or
/// above `sb`, otherwise the smaller of the two sides. A result above `u128::MAX` is 0.
pub fn position_liquidity(
    price: u128,
    lower: i32,
    upper: i32,
    quote: u64,
    base: u64,
) -> FloorMathResult<u128> {
    let (a, b) = bounds(lower, upper)?;
    position_liquidity_at_sqrt(price, a, b, quote, base)
}

/// [`position_liquidity`] with the edges' sqrt prices already read (floor first's probes). The ONE body both spell.
pub(crate) fn position_liquidity_at_sqrt(price: u128, a: u128, b: u128, quote: u64, base: u64) -> FloorMathResult<u128> {
    let l = if price <= a {
        liq0(a, b, quote)?
    } else if price >= b {
        liq1(a, b, base)?
    } else {
        let l0 = liq0(price, b, quote)?;
        let l1 = liq1(a, price, base)?;
        if l0 < l1 {
            l0
        } else {
            l1
        }
    };
    Ok(if l > U256::from(u128::MAX) { 0 } else { l.as_u128() })
}

/// `(quote, base)` held by liquidity `liquidity` over `[lower, upper]` at `price`, both rounded DOWN
/// (the EVM's `FloorPositionMath`). The price is clamped to `[sa, sb]`.
pub fn position_amounts(
    price: u128,
    lower: i32,
    upper: i32,
    liquidity: u128,
) -> FloorMathResult<(u64, u64)> {
    if liquidity == 0 {
        return Ok((0, 0));
    }
    let (a, b) = bounds(lower, upper)?;
    let mut p = price;
    if p < a {
        p = a;
    }
    if p > b {
        p = b;
    }
    let amount0 = get_delta_amount_0_unsigned(p, b, liquidity, false)?;
    let amount1 = get_delta_amount_1_unsigned(a, p, liquidity, false)?;
    Ok((amount0, amount1))
}

/// The bid's live composition (the EVM's `FloorPositionMath`): no liquidity → `(0, 0)` FIRST
/// (before the untouched test, whatever `deployed` says); untouched (price at or below the
/// lower edge) reports the recorded `deployed` quote exactly; otherwise `position_amounts` over
/// the band `[lower, lower + width]`.
///
/// ⛔ The `L == 0` guard returns `(0, 0)`, as the EVM does; `no_liquidity_is_empty_whatever_was_deployed` pins it.
///
/// The third argument is the bid's WIDTH. The bid is a band `[floor_tick − W, floor_tick]`, so the engine passes
/// `_bidWidth()` (`bid_width`, or one spacing on a pool with no bid yet — [`crate::band::bid_width_or_spacing`]).
pub fn bid_amounts(
    price: u128,
    lower: i32,
    width: i32,
    liquidity: u128,
    deployed: u64,
) -> FloorMathResult<(u64, u64)> {
    if liquidity == 0 {
        return Ok((0, 0));
    }
    let untouched = get_sqrt_price_at_tick(lower)?;
    if price <= untouched {
        return Ok((deployed, 0));
    }
    let upper = lower.checked_add(width).ok_or(FloorMathError::Overflow)?;
    position_amounts(price, lower, upper, liquidity)
}

/// Base tokens the POL position would hold at `tick`, rounded DOWN so the solver never over-excludes
/// supply (the EVM's `FloorPositionMath` / `FloorHookMath`). The tick's sqrt price is computed first, as the EVM
/// evaluates the argument before the zero-liquidity return.
///
/// ⛔ Computes the BASE leg only, not `position_amounts(…).1`: the quote leg it would discard is
/// `Err(MaxTokenOverflow)` above `u64::MAX` lamports, which at or below `lower` on a dear POL is the
/// position's whole value — a solve failure the EVM's uint256 `positionAmounts` does not have
/// (unreachable under the 2^77 tranche cap, and pinned anyway).
pub fn pol_base(tick: i32, lower: i32, upper: i32, liquidity: u128) -> FloorMathResult<u64> {
    let p = get_sqrt_price_at_tick(tick)?;
    if liquidity == 0 {
        return Ok(0);
    }
    let (a, b) = bounds(lower, upper)?;
    pol_base_at_sqrt(p, a, b, liquidity)
}

/// [`pol_base`] with the sqrt prices already read: `p` the tried tick's, `a` / `b` the position's edges'. The ONE body
/// both spell (the floor solve reads each edge once a solve, not once a step).
pub(crate) fn pol_base_at_sqrt(mut p: u128, a: u128, b: u128, liquidity: u128) -> FloorMathResult<u64> {
    if p < a {
        p = a;
    }
    if p > b {
        p = b;
    }
    Ok(get_delta_amount_1_unsigned(a, p, liquidity, false)?)
}

/// A position's base (token) leg at the sqrt price `p`, its edges' sqrt prices `a` / `b` already read: exactly
/// [`position_amounts`]`.1` (the same clamp and rounding), 0 with no liquidity. The hook's `standing_tokens` reads each
/// edge once a ratchet with it.
pub fn base_at_sqrt(p: u128, a: u128, b: u128, liquidity: u128) -> FloorMathResult<u64> {
    if liquidity == 0 {
        return Ok(0);
    }
    pol_base_at_sqrt(p, a, b, liquidity)
}

/// A position's edges' sqrt prices, `None` for no liquidity (which locks nothing, as [`pol_base`] answers 0).
pub(crate) fn edges(lower: i32, upper: i32, liquidity: u128) -> FloorMathResult<Option<(u128, u128, u128)>> {
    if liquidity == 0 {
        return Ok(None);
    }
    let (a, b) = bounds(lower, upper)?;
    Ok(Some((a, b, liquidity)))
}

/// Lamports to buy `base` units at `tick`: `floor(floor(base·2^64/sp)·2^64/sp)` in U256
/// (the EVM's `FloorPositionMath`, native branch; floored).
///
/// ⛔ Returns `u128`, saturating at `u128::MAX`, and is never narrowed to u64: at the dearest ticks the
/// bisection visits, 1e15 units cost ≈ 2^114 lamports.
pub fn quote_cost(base: u64, tick: i32) -> FloorMathResult<u128> {
    quote_cost_at_sqrt(base, get_sqrt_price_at_tick(tick)?)
}

/// [`quote_cost`] at a sqrt price already read (the floor solve's step, 4A). The ONE body both spell.
pub(crate) fn quote_cost_at_sqrt(base: u64, sqrt: u128) -> FloorMathResult<u128> {
    let sp = U256::from(sqrt);
    let q = U256::from(Q64);
    let x = U256::from(base).mul_div_floor(q, sp).ok_or(FloorMathError::Overflow)?;
    let y = x.mul_div_floor(q, sp).ok_or(FloorMathError::Overflow)?;
    Ok(if y > U256::from(u128::MAX) { u128::MAX } else { y.as_u128() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use carpenter_clmm_math::tick_math::{MAX_TICK, MIN_TICK};

    // :62-66 — POL is all quote below its range, all base above it, and ≤ what it was seeded with.
    #[test]
    fn pol_base_is_bounded_by_the_seed() {
        let (lower, upper) = (-6960, 0);
        let seed = 150_000_000_000_000u64;
        let p_top = get_sqrt_price_at_tick(upper).unwrap();
        let l = position_liquidity(p_top, lower, upper, 0, seed).unwrap();
        assert!(l > 0);
        assert_eq!(pol_base(lower - 60, lower, upper, l).unwrap(), 0); // below sa: all quote
        let at_top = pol_base(upper + 6000, lower, upper, l).unwrap(); // above sb: all base
        assert!(at_top <= seed && seed - at_top < 2, "seed {seed} vs {at_top}");
        let mut last = 0;
        for t in (lower..=upper).step_by(60) {
            let b = pol_base(t, lower, upper, l).unwrap();
            assert!(b >= last, "base must grow with tick");
            last = b;
        }
        assert_eq!(pol_base(0, lower, upper, 0).unwrap(), 0);
        assert!(pol_base(MAX_TICK + 1, lower, upper, 0).is_err()); // sqrt evaluated first
    }

    // `pol_base` computes the BASE leg only. Through `position_amounts` it would also compute the quote leg it
    // discards, which is `Err(MaxTokenOverflow)` above u64::MAX lamports — so on a dear POL every tick at or below
    // `seed_lower` would error where the EVM's uint256 `positionAmounts` proceeds.
    // ⛔ This is the LIVE PATH: the full-range POL is seeded from `ceil_to_spacing(MIN_TICK, 60)` = −443,580, and the
    // solve's `lo` is that same tick, so EVERY solve evaluates `pol_base` at `tick == seed_lower` — the "at or below
    // the range" case this test probes, where the discarded quote leg is the position's whole value and overflows
    // u64. The range below is an arbitrary DEAR range for the overflow probe; the fixture at the real seed is
    // `solve.rs::graduated`.
    #[test]
    fn pol_base_below_a_dear_range_is_zero_not_an_overflow() {
        let open = -240_000;
        let (lower, upper) = (-246_960, -240_000); // an arbitrary dear range, one doubling wide
        let p = get_sqrt_price_at_tick(open).unwrap();
        let l = position_liquidity(p, lower, upper, 0, 150_000_000_000_000).unwrap();
        assert_eq!(l, 83_019_744_082_906_835_908);
        // the discarded quote leg really is above u64::MAX there
        assert_eq!(
            position_amounts(get_sqrt_price_at_tick(lower - 60).unwrap(), lower, upper, l),
            Err(FloorMathError::Math(carpenter_clmm_math::MathError::MaxTokenOverflow))
        );
        assert_eq!(pol_base(lower - 60, lower, upper, l), Ok(0));
        assert_eq!(pol_base(lower, lower, upper, l), Ok(0));
        // inside the range (open − 3000 = −243,000) the quote leg still overflows; the base leg is fine
        let p_in = get_sqrt_price_at_tick(open - 3_000).unwrap();
        assert!(position_amounts(p_in, lower, upper, l).is_err());
        let mid = pol_base(open - 3_000, lower, upper, l).unwrap();
        assert!(mid > 0 && mid < 150_000_000_000_000, "{mid}");
        // above the range the two computations agree, where both are defined
        let top = pol_base(upper + 60, lower, upper, l).unwrap();
        assert_eq!(top, position_amounts(get_sqrt_price_at_tick(upper + 60).unwrap(), lower, upper, l).unwrap().1);
        assert!(150_000_000_000_000 - top < 2);

        // The live path: the full-range tail seeded as the EVM's `FloorLaunch` does, evaluated at the solve's own `lo` —
        // which IS `seed_lower`. Every solve does this.
        let (tail_lower, tail_upper) = (-443_580, 99_000);
        let p_open = get_sqrt_price_at_tick(99_039).unwrap();
        let l_tail = position_liquidity(p_open, tail_lower, tail_upper, 0, 150_000_000_000_000).unwrap();
        assert!(l_tail > 0);
        assert!(position_amounts(get_sqrt_price_at_tick(tail_lower).unwrap(), tail_lower, tail_upper, l_tail).is_err());
        assert_eq!(pol_base(tail_lower, tail_lower, tail_upper, l_tail), Ok(0));
    }

    // over a one-spacing band and over the 120-tick band (the engine passes `_bidWidth()`).
    #[test]
    fn bid_untouched_reports_the_recorded_deployment() {
        for width in [60, 120] {
            let (lower, s) = (1200, width);
            let p_low = get_sqrt_price_at_tick(lower).unwrap();
            let l = position_liquidity(p_low, lower, lower + s, 1_000_000_000, 0).unwrap();
            assert_eq!(bid_amounts(p_low, lower, s, l, 999_999_999).unwrap(), (999_999_999, 0));
            let p_high = get_sqrt_price_at_tick(lower + s).unwrap();
            let (q, b) = bid_amounts(p_high + 1, lower, s, l, 999_999_999).unwrap();
            assert_eq!(q, 0);
            assert!(b > 0);
            // half-way through the band: both sides; a wider band at the same quote is a thinner
            // position, so the same mid-price leaves MORE quote unfilled than on the narrow one
            let p_mid = get_sqrt_price_at_tick(lower + 30).unwrap();
            let (q, b) = bid_amounts(p_mid, lower, s, l, 1_000_000_000).unwrap();
            assert!(q > 0 && q < 1_000_000_000 && b > 0, "width {width}");
        }
        let (lower, s) = (1200, 60);
        let p_low = get_sqrt_price_at_tick(lower).unwrap();
        let l = position_liquidity(p_low, lower, lower + s, 1_000_000_000, 0).unwrap();
        assert_eq!(bid_amounts(p_low, lower, s, l, 999_999_999).unwrap(), (999_999_999, 0));
        assert_eq!(bid_amounts(p_low - 1, lower, s, l, 7).unwrap(), (7, 0));
        assert_eq!(bid_amounts(p_low, lower, s, 0, 7).unwrap(), (0, 0));
        // fully crossed: all base, no quote
        let p_high = get_sqrt_price_at_tick(lower + s).unwrap();
        let (q, b) = bid_amounts(p_high + 1, lower, s, l, 999_999_999).unwrap();
        assert_eq!(q, 0);
        assert!(b > 0);
        // mid-fill: both sides, quote below the deployment
        let p_mid = get_sqrt_price_at_tick(lower + 30).unwrap();
        let (q, b) = bid_amounts(p_mid, lower, s, l, 1_000_000_000).unwrap();
        assert!(q > 0 && q < 1_000_000_000 && b > 0);
    }

    // `liquidity == 0` is `(0, 0)` before anything else is looked at, as the EVM's `FloorPositionMath` returns — not
    // `(deployed, 0)`. The first case is an EVM fixture row verbatim; the mutation check is moving the guard below the
    // untouched test, which fails the first three cases.
    #[test]
    fn no_liquidity_is_empty_whatever_was_deployed() {
        let p = get_sqrt_price_at_tick(-46_015).unwrap();
        assert_eq!(bid_amounts(p, 160_260, 60, 0, 2_247_591_574).unwrap(), (0, 0));
        assert_eq!(bid_amounts(p, 160_260, 60, 0, u64::MAX).unwrap(), (0, 0));
        // exactly on the lower edge, and above it
        let p_low = get_sqrt_price_at_tick(160_260).unwrap();
        assert_eq!(bid_amounts(p_low, 160_260, 60, 0, 7).unwrap(), (0, 0));
        assert_eq!(bid_amounts(p_low + 1, 160_260, 60, 0, 7).unwrap(), (0, 0));
        // the guard is FIRST: a tick the sqrt table refuses is still (0, 0) with no liquidity,
        // where L > 0 reaches `get_sqrt_price_at_tick` and errs
        assert_eq!(bid_amounts(p, MAX_TICK + 1, 60, 0, 7).unwrap(), (0, 0));
        assert!(bid_amounts(p, MAX_TICK + 1, 60, 1, 7).is_err());
        // with liquidity the untouched branch still reports `deployed`
        assert_eq!(bid_amounts(p, 160_260, 60, 1, 2_247_591_574).unwrap(), (2_247_591_574, 0));
    }

    // :17-31 — the deposit round-trips DOWN: amounts from the computed liquidity never exceed it.
    #[test]
    fn liquidity_then_amounts_never_exceeds_the_deposit() {
        for (lower, upper) in [(-6960, 0), (60, 120), (-443_520, -443_460), (443_400, 443_520)] {
            let a = get_sqrt_price_at_tick(lower).unwrap();
            let b = get_sqrt_price_at_tick(upper).unwrap();
            let mid = a + (b - a) / 3;
            for (price, quote, base) in [(a, 5_000_000_000u64, 0u64), (b, 0, 7_000_000_000), (mid, 3_000_000_000, 4_000_000_000)] {
                let l = position_liquidity(price, lower, upper, quote, base).unwrap();
                if l == 0 {
                    continue;
                }
                let (q, bb) = position_amounts(price, lower, upper, l).unwrap();
                assert!(q <= quote && bb <= base, "[{lower},{upper}] {q}/{quote} {bb}/{base}");
            }
        }
    }

    #[test]
    fn empty_range_is_refused() {
        let p = get_sqrt_price_at_tick(0).unwrap();
        assert_eq!(position_liquidity(p, 60, 60, 1, 1), Err(FloorMathError::InvalidRange));
        assert_eq!(position_liquidity(p, 120, 60, 1, 1), Err(FloorMathError::InvalidRange));
    }

    // :26's "above u128 → 0" cannot fire with u64 amounts: the largest liquidity is u64::MAX over a
    // one-tick range, ≈ 2^110 at either tick bound. Pinned so a widened amount type revisits it.
    #[test]
    fn u64_amounts_cannot_reach_the_u128_liquidity_cap() {
        for (lo, hi) in [(MAX_TICK - 1, MAX_TICK), (MIN_TICK, MIN_TICK + 1)] {
            let a = get_sqrt_price_at_tick(lo).unwrap();
            let b = get_sqrt_price_at_tick(hi).unwrap();
            let l0 = position_liquidity(a, lo, hi, u64::MAX, 0).unwrap();
            let l1 = position_liquidity(b, lo, hi, 0, u64::MAX).unwrap();
            assert!(l0 > 0 && l0 < 1u128 << 112, "{l0}");
            assert!(l1 > 0 && l1 < 1u128 << 112, "{l1}");
        }
    }

    // quote_cost: floor, u128, saturating.
    #[test]
    fn quote_cost_is_floor_and_never_narrowed() {
        assert_eq!(quote_cost(1_000, 0).unwrap(), 1_000); // price 1
        // tick −6932 is 1.0001^−6932 = 0.49998 units per lamport -> 2.000035 lamports per unit
        // (6932 is ln2/ln1.0001 = 6931.8 rounded up), floored
        assert_eq!(quote_cost(1_000_000, -6932).unwrap(), 2_000_035);
        // dearest tick the bisection visits at spacing 60: whole supply ≈ 2^114 lamports, above u64
        let big = quote_cost(1_000_000_000_000_000, -443_580).unwrap();
        assert!(big > u64::MAX as u128 && big < u128::MAX);
        // MIN_TICK with u64::MAX units does not panic
        assert!(quote_cost(u64::MAX, MIN_TICK).unwrap() > u64::MAX as u128);
        // the two-step floor is ≤ the one-step exact value
        let sp = U256::from(get_sqrt_price_at_tick(-1234).unwrap());
        let exact = (U256::from(987_654_321u64) << 128usize) / (sp * sp);
        assert!(U256::from(quote_cost(987_654_321, -1234).unwrap()) <= exact);
    }
}
