//! Backing ratio → Q64.64 sqrt price and ticks: `sqrt_price_from_ratio`, `ask_tick`, `open_tick` (the EVM's
//! `FloorMath` / `FloorLaunch`).
//!
//! Orientation: currency0 is wSOL, currency1 the FLOOR mint, pool price = base units per
//! lamport. A HIGHER tick is a CHEAPER token; "the floor goes up" means `floor_tick` goes DOWN.

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::full_math::MulDiv;
use carpenter_clmm_math::tick_math::{get_tick_at_sqrt_price, MAX_SQRT_PRICE_X64, MIN_SQRT_PRICE_X64};

use crate::error::{FloorMathError, FloorMathResult};
use crate::spacing::floor_to_spacing;

/// 2^96, the EVM's `FixedPoint96.Q96`: the scale `sqrtPriceX96FromRatio` roots at. Only `sqrt_price_from_ratio` uses
/// it; everything else here is Q64.
const Q96: u128 = 1u128 << 96;

/// sqrt(`tok / eth`) in Q64.64, the pool sqrt price implied by `eth` lamports per `tok` base units
/// (the EVM's `sqrtPriceX96FromRatio`).
///
/// `tok == 0` → `ZeroDenominator`; `eth == 0` → `MAX_SQRT_PRICE_X64 − 1` (a free floor is the cheapest
/// token). Otherwise **the EVM's own root, re-based**: `isqrt(floor(tok·2^96/eth)) << 16`, clamped to
/// `[MIN, MAX − 1]`. The EVM computes `Math.sqrt(mulDiv(tok, 2^96, eth)) << 48` in X96; its X96 value has its low 48
/// bits zero, so `>> 32` is exact and lands
/// on this Q64 value bit for bit. Every step rounds DOWN, as the EVM's `mulDiv` / `Math.sqrt` do.
///
/// ⛔ The EVM's own root, re-based, rather than `isqrt(floor(tok·2^64/eth)) << 32`, which drops 32 bits of Q64 sqrt
/// where the EVM drops 16 — below tick ≈ −200,000 that moves `ratio_tick` by tens to thousands of ticks. Full
/// precision (`isqrt(tok·2^128/eth)`) is not used either: it diverges from the EVM in the other direction where the
/// EVM's own root is coarse, and this is a parity build.
pub fn sqrt_price_from_ratio(eth: u128, tok: u128) -> FloorMathResult<u128> {
    if tok == 0 {
        return Err(FloorMathError::ZeroDenominator);
    }
    if eth == 0 {
        return Ok(MAX_SQRT_PRICE_X64 - 1);
    }
    // tok < 2^128, so tok·2^96 < 2^224: the U256 mul-div cannot fail, and the root is < 2^112, so
    // `<< 16` stays below 2^128.
    let ratio_x96 = U256::from(tok)
        .mul_div_floor(U256::from(Q96), U256::from(eth))
        .ok_or(FloorMathError::Overflow)?;
    let sqrt = ratio_x96.integer_sqrt() << 16usize;
    if sqrt < U256::from(MIN_SQRT_PRICE_X64) {
        return Ok(MIN_SQRT_PRICE_X64);
    }
    if sqrt >= U256::from(MAX_SQRT_PRICE_X64) {
        return Ok(MAX_SQRT_PRICE_X64 - 1);
    }
    Ok(sqrt.as_u128())
}

/// The ask wall's upper tick: `multiple_bps` over the inventory's cost basis, but never closer to the
/// floor than `min_spread` ticks (the EVM's `FloorMath`).
///
/// `ceiling = floor_to_spacing(floor_tick − min_spread)`; empty inventory or zero cost → `ceiling`;
/// otherwise `by_basis = floor_to_spacing(tick_at(sqrt_price_from_ratio(cost·mult/10_000, tokens)))`
/// and the result is `min(by_basis, ceiling)` (lower tick = higher ask).
pub fn ask_tick(
    cost: u64,
    tokens: u64,
    multiple_bps: u64,
    floor_tick: i32,
    min_spread: i32,
    spacing: i32,
) -> FloorMathResult<i32> {
    let raw_ceiling = floor_tick.checked_sub(min_spread).ok_or(FloorMathError::Overflow)?;
    let ceiling = floor_to_spacing(raw_ceiling, spacing)?;
    if tokens == 0 || cost == 0 {
        return Ok(ceiling);
    }
    // cost·mult < 2^128, so the floor division fits u128.
    let target = (cost as u128 * multiple_bps as u128) / 10_000;
    let sqrt = sqrt_price_from_ratio(target, tokens as u128)?;
    let by_basis = floor_to_spacing(get_tick_at_sqrt_price(sqrt)?, spacing)?;
    Ok(by_basis.min(ceiling))
}

/// The pool's opening tick for a sale priced at `price_x64` lamports per base unit × 2^64 (the twin of the EVM's
/// `FloorLaunch` open).
///
/// Pool price is `2^64 / price_x64` base units per lamport, so its Q64 sqrt is
/// `isqrt(2^192 / price_x64)`, both steps floored, and the tick is `tick_at_sqrt_price` of it (floored).
/// The pool is then initialized at `get_sqrt_price_at_tick(open_tick)`.
/// ⚠ The EVM goes through `sqrtPriceX96FromRatio(p1, 1e18)`, `Math.sqrt(floor(tok·2^96/eth)) << 48`,
/// which drops 48 low bits AFTER the root; this keeps full Q64 resolution. The EVM sqrt is therefore
/// ≤ this one, so a boundary price can land one tick LOWER on EVM than here, never higher
/// (`evm_open_tick_is_never_higher`).
pub fn open_tick(price_x64: u128) -> FloorMathResult<i32> {
    if price_x64 == 0 {
        return Err(FloorMathError::ZeroDenominator);
    }
    let sqrt = ((U256::one() << 192usize) / U256::from(price_x64)).integer_sqrt();
    if sqrt >= U256::from(MAX_SQRT_PRICE_X64) {
        return Err(carpenter_clmm_math::MathError::SqrtPriceOutOfRange.into());
    }
    Ok(get_tick_at_sqrt_price(sqrt.as_u128())?)
}


#[cfg(test)]
mod tests {
    use super::*;

    use crate::constants::TICKS_PER_DOUBLING;
    use carpenter_clmm_math::fixed_point_64::Q64;
    use carpenter_clmm_math::tick_math::{get_sqrt_price_at_tick, MAX_TICK};

    // the EVM's zero-amount branches.
    #[test]
    fn ratio_edges() {
        assert_eq!(sqrt_price_from_ratio(1, 0), Err(FloorMathError::ZeroDenominator));
        assert_eq!(sqrt_price_from_ratio(0, 1).unwrap(), MAX_SQRT_PRICE_X64 - 1);
        assert_eq!(sqrt_price_from_ratio(0, 0), Err(FloorMathError::ZeroDenominator));
        // price 1: sqrt(1)·2^64
        assert_eq!(sqrt_price_from_ratio(7, 7).unwrap(), Q64);
        // 4 base units per lamport: sqrt = 2
        assert_eq!(sqrt_price_from_ratio(1, 4).unwrap(), 2 * Q64);
        // clamps
        assert_eq!(sqrt_price_from_ratio(u128::MAX, 1).unwrap(), MIN_SQRT_PRICE_X64);
        assert_eq!(sqrt_price_from_ratio(1, u128::MAX).unwrap(), MAX_SQRT_PRICE_X64 - 1);
    }

    // isqrt(floor(tok·2^96/eth)) << 16 is floor(sqrt(tok/eth)·2^48)·2^16: never above the true root,
    // and within 2^16 Q64 units of it (the EVM's own granularity).
    #[test]
    fn ratio_rounds_down_within_2_pow_16() {
        let cases: [(u128, u128); 8] = [
            (1_000_000_000, 1_000_000_000_000_000),
            (3, 10),
            (123_456_789, 987_654_321),
            (50_000_000_000, 550_000_000_000_000),
            (999_999_937, 1),
            (1, 999_999_937),
            (1_275_002_924_882_050_770, 2_196), // fixtures/evm/ratio_tick.json row 69: tick −339,968
            (426_441_616_129, 376),             // row 2: tick −208,502
        ];
        for (eth, tok) in cases {
            let s = sqrt_price_from_ratio(eth, tok).unwrap();
            // s² · eth ≤ tok · 2^128, and (s + 2^16)² · eth > tok · 2^128 (both in U256)
            let lhs = U256::from(s) * U256::from(s) * U256::from(eth);
            assert!(lhs <= U256::from(tok) * (U256::one() << 128usize), "{eth}/{tok}");
            let up = U256::from(s) + (U256::one() << 16usize);
            let lhs_up = up * up * U256::from(eth);
            assert!(lhs_up > U256::from(tok) * (U256::one() << 128usize), "{eth}/{tok}");
        }
    }

    // The Q64 sqrt IS the EVM's X96 sqrt `>> 32`, bit for bit, on every ratio inside the Solana clamp. The EVM form is
    // transcribed here independently — `Math.sqrt(mulDiv(tok, Q96, eth)) << 48` in uint256 — and only the clamp is
    // re-based to the Solana tick range. The two low-price rows are real fixture rows that an
    // `isqrt(tok·2^64/eth) << 32` form decodes to −339,996 / −208,503 against the EVM's −339,968 / −208,502; the
    // mutation check is reverting `price.rs` to that form, which fails every case below whose root has a non-zero low
    // 16 bits.
    #[test]
    fn matches_the_evm_x96_root_bit_for_bit() {
        let cases: [(u128, u128, Option<i32>); 9] = [
            (1_275_002_924_882_050_770, 2_196, Some(-339_968)), // fixtures/evm/ratio_tick.json row 69
            (426_441_616_129, 376, Some(-208_502)),             // row 2
            (1_249_717_930_458, 1_000_000_000_000_000, Some(66_851)), // row 0
            (56, 4, Some(-26_392)),                             // row 1
            (1_007_062_920_101_364_418, 11_156_033_366_948, Some(-114_112)), // row 4
            (3, 10, None),
            (123_456_789, 987_654_321, None),
            (u64::MAX as u128, 1, None), // inside the MIN clamp
            (1, u64::MAX as u128, None), // inside the MAX clamp
        ];
        for (eth, tok, evm_tick) in cases {
            let ratio_x96 = (U256::from(tok) * (U256::one() << 96usize)) / U256::from(eth);
            let evm_x96 = ratio_x96.integer_sqrt() << 48usize;
            assert!(evm_x96 & ((U256::one() << 48usize) - U256::one()) == U256::zero());
            let mut evm_q64 = (evm_x96 >> 32usize).as_u128();
            evm_q64 = evm_q64.clamp(MIN_SQRT_PRICE_X64, MAX_SQRT_PRICE_X64 - 1);
            let ours = sqrt_price_from_ratio(eth, tok).unwrap();
            assert_eq!(ours, evm_q64, "{eth}/{tok}");
            if let Some(t) = evm_tick {
                assert_eq!(get_tick_at_sqrt_price(ours).unwrap(), t, "{eth}/{tok}");
            }
        }
    }

    // The EVM's ask tick.
    #[test]
    fn ask_tick_takes_the_higher_ask_of_basis_and_floor() {
        let s = 60;
        // no inventory or no cost -> the floor ceiling
        assert_eq!(ask_tick(0, 5, 20_000, 0, TICKS_PER_DOUBLING, s).unwrap(), -6960);
        assert_eq!(ask_tick(5, 0, 20_000, 0, TICKS_PER_DOUBLING, s).unwrap(), -6960);
        // cost basis far above the floor (cost 1 lamport per unit, floor at price 1): by_basis wins
        // (2x basis = 2 lamports/unit -> pool price 1/2 -> tick ≈ −6932 -> floored −6960, ceiling
        // floor_to_spacing(100000 − 6932) = 93060); min picks −6960.
        assert_eq!(ask_tick(1_000_000, 1_000_000, 20_000, 100_000, TICKS_PER_DOUBLING, s).unwrap(), -6960);
        // a stale cheap lot (basis would ask BELOW the bid): the floor ceiling wins
        let at = ask_tick(1, 1_000_000_000, 20_000, -50_000, TICKS_PER_DOUBLING, s).unwrap();
        assert_eq!(at, floor_to_spacing(-50_000 - 6932, s).unwrap());
        // overflow of floor_tick − spread is an Err, not a wrap
        assert_eq!(ask_tick(1, 1, 1, i32::MIN, 1, s), Err(FloorMathError::Overflow));
    }

    // The EVM form, `isqrt(floor(2^160/price_x64)) << 48` in X96, brought to Q64 by a floor `>> 32`, never lands above
    // `open_tick` (the direction an EVM `openTick` mismatch takes).
    #[test]
    fn evm_open_tick_is_never_higher() {
        for price_x64 in [
            922_337_203_685_477u128,
            (1u128 << 64) / 3,
            (1u128 << 64) / 7 + 1,
            1u128 << 64,
            (1u128 << 64) + 12_345,
            (1u128 << 77) - 1,
            (1u128 << 77) / 3,
            1u128 << 20,
            (1u128 << 20) + 1,
            999_999_999_999,
        ] {
            let ratio_x96 = (U256::one() << 160usize) / U256::from(price_x64);
            let evm_x96 = ratio_x96.integer_sqrt() << 48usize;
            let evm_tick = get_tick_at_sqrt_price((evm_x96 >> 32usize).as_u128()).unwrap();
            let ours = open_tick(price_x64).unwrap();
            assert!(evm_tick <= ours, "price {price_x64}: evm {evm_tick} > ours {ours}");
            assert!(ours - evm_tick <= 1, "price {price_x64}: evm {evm_tick}, ours {ours}");
        }
    }

    // open_tick: the tick brackets the exact pool sqrt price.
    #[test]
    fn open_tick_brackets_the_sale_price() {
        for price_x64 in [
            922_337_203_685_477u128,          // a $5k cap at SOL $100
            (1u128 << 64) / 3,
            1u128 << 64,                      // 1 lamport per base unit
            (1u128 << 77) - 1,                // the tranche price cap
            1u128 << 20,
        ] {
            let t = open_tick(price_x64).unwrap();
            let exact = ((U256::one() << 192usize) / U256::from(price_x64)).integer_sqrt().as_u128();
            assert!(get_sqrt_price_at_tick(t).unwrap() <= exact);
            if t < MAX_TICK {
                assert!(get_sqrt_price_at_tick(t + 1).unwrap() > exact);
            }
        }
        assert_eq!(open_tick(0), Err(FloorMathError::ZeroDenominator));
        // price_x64 = 1 -> sqrt = 2^96 ≥ MAX_SQRT_PRICE
        assert!(open_tick(1).is_err());
    }

}
