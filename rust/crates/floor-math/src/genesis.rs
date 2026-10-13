//! `$FLOOR`'s SOLANA GENESIS — the timed raise's arithmetic, the twin of the EVM's `FloorProtocolSetup`
//! (`_initializePool`, the opening tick; `entitlement`; `graduate`'s floor / POL split).
//!
//! ⛔ **THE TWO-PHASE RAISE**: the raise sells at a FIXED price (4 SOL per 1M tokens; the open is that price, 4,000 SOL
//! of market cap) in two phases — phase 1 20M (≤ 20 SOL a wallet, 80 SOL fills it, 5 minutes) then phase 2 22.5M (≤ 5 SOL
//! a wallet, 90 SOL fills it, open until the authority closes it), every buyer at the same price. It departs from the
//! EVM's `raised ÷ A` open and pro-rata entitlement; `genesis_split` (the event's floor-share figures) and the buyers'
//! vest are unchanged.
//!
//! Pure functions, no Solana dependency, so `floor_launch`, the indexer and the SDK's quoter all
//! compute the same numbers. `floor-types`' `GENESIS_*` constants are pinned to these by
//! `tests/floor_types_pin.rs`.

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::tick_math::get_tick_at_sqrt_price;

use crate::constants::{BPS, SUPPLY};
use crate::error::{FloorMathError, FloorMathResult};
use crate::price::sqrt_price_from_ratio;

/// `A`: the most the raise can sell, **42.5M** whole tokens in 6-decimal base units (4.25 % of supply): phase 1's
/// [`GENESIS_CORE_TOKENS`] 20M + phase 2's [`GENESIS_FF_TOKENS`] 22.5M, both at ONE fixed price. Only what was SOLD is
/// held for the claims ([`genesis_sold`]); the rest joins the protocol's steps at graduation.
pub const GENESIS_SALE: u64 = GENESIS_CORE_TOKENS + GENESIS_FF_TOKENS;
/// Phase 1 — Core: 20M tokens (2 %), at most 20 SOL a wallet ("1. 20 sol"; "It will be 4 people in the Phase 1 -> 20 sol
/// each"; "Nono, Phase 1 still gets 20m": 4 × 20 SOL buy the whole 20M).
pub const GENESIS_CORE_TOKENS: u64 = 20_000_000_000_000;
/// Phase 2: **22.5M** tokens (2.25 %), at most 5 SOL a wallet (18 × 5 SOL), at the SAME fixed price as phase 1 —
/// first come, the contribution that passes it trimmed.
pub const GENESIS_FF_TOKENS: u64 = 22_500_000_000_000;
/// One million whole tokens in base units (6 decimals): the unit the fixed price is quoted per.
pub const GENESIS_PRICE_UNIT_TOKENS: u64 = 1_000_000_000_000;
/// ⛔ THE FIXED SALE PRICE: **4 SOL per 1M whole tokens** (4,000 lamports a whole token, 250 base units a lamport) for
/// BOTH phases — every buyer gets `SOL ÷ price` tokens and nobody overpays; a 4,000-SOL opening market cap, ≈ $600k at
/// SOL $150. A CONSTANT in SOL, not a setting: the wallet caps and the phase sizes are in SOL too, and a SOL price
/// keeps them true together; the dollar figure moves with SOL.
pub const GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION: u64 = 4_000_000_000;
/// What Core's 20M cost at the fixed price: 80 SOL (4 wallets × 20) — the phase's ceiling (a contribution past it is TRIMMED).
pub const GENESIS_CORE_CAP_LAMPORTS: u64 = 80_000_000_000;
/// What Phase 2's 22.5M cost at the fixed price: 90 SOL (18 wallets × 5 = 90) — the phase's ceiling (TRIMMED as Core's).
pub const GENESIS_FF_CAP_LAMPORTS: u64 = 90_000_000_000;
/// The open's market cap in lamports: `SUPPLY · price` = 4,000 SOL (`4e12`).
pub const GENESIS_OPEN_MCAP_LAMPORTS: u64 = 4_000_000_000_000;
const _: () = assert!(GENESIS_CORE_CAP_LAMPORTS as u128 * GENESIS_PRICE_UNIT_TOKENS as u128 == GENESIS_CORE_TOKENS as u128 * GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128);
const _: () = assert!(GENESIS_FF_CAP_LAMPORTS as u128 * GENESIS_PRICE_UNIT_TOKENS as u128 == GENESIS_FF_TOKENS as u128 * GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128);
const _: () = assert!(GENESIS_OPEN_MCAP_LAMPORTS as u128 * GENESIS_PRICE_UNIT_TOKENS as u128 == SUPPLY as u128 * GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128);

/// The EVM clamp `[-120_000, 600_000]` shifted by the decimals offset, and capped at the top of this
/// AMM's grid (`floor_types::constants::GENESIS_OPEN_TICK_MIN` / `_MAX` carry the argument). The open is the FIXED
/// price, well inside it; the clamp is kept as the bound a pool can open at.
pub const GENESIS_OPEN_TICK_MIN: i32 = -189_081;
pub const GENESIS_OPEN_TICK_MAX: i32 = 443_515;

/// ⛔ The open is the FIXED price — `tick(sqrt_price_from_ratio(4 SOL, 1M tokens))`, 250 base units a lamport
/// (≈ 55,217) — whatever was raised. Returns `(tick, clamped)`; the clamp never binds.
/// ⚠ Pool price is base units per lamport: a dearer token is a LOWER tick.
pub fn genesis_open_tick() -> FloorMathResult<(i32, bool)> {
    let raw = get_tick_at_sqrt_price(sqrt_price_from_ratio(
        GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128,
        GENESIS_PRICE_UNIT_TOKENS as u128,
    )?)?;
    let tick = raw.clamp(GENESIS_OPEN_TICK_MIN, GENESIS_OPEN_TICK_MAX);
    Ok((tick, tick != raw))
}

/// Tokens `lamports` buy at the fixed price, floored (`lamports · 1M / 4 SOL` = `lamports · 250`).
pub fn genesis_tokens_at_price(lamports: u64) -> FloorMathResult<u64> {
    u64::try_from(lamports as u128 * GENESIS_PRICE_UNIT_TOKENS as u128 / GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION as u128)
        .map_err(|_| FloorMathError::Overflow)
}

/// ⛔ THE TWO-PHASE RAISE: a wallet's whole allocation — its lamports in each phase at the ONE fixed price
/// ([`genesis_tokens_at_price`], each part floored). ⛔ THE ONE SPELLING `claim_genesis` pays by, and the indexer and the SDK
/// serve. (The price is a whole 250 base units a lamport, so the floors are exact.)
pub fn genesis_allocation(core: u64, ff: u64) -> FloorMathResult<u64> {
    genesis_tokens_at_price(core)?.checked_add(genesis_tokens_at_price(ff)?).ok_or(FloorMathError::Overflow)
}

/// What the raise SOLD, `(core, ff)` — each phase's lamports at the fixed price, at most its tokens (Core ≤ 20M, Phase 2 ≤
/// 22.5M: `contribute` trims at 80 / 90 SOL). Every allocation is a floor of its share, so Σ allocations ≤ the sum and the
/// difference is the ALLOCATION DUST (`flush_allocation_dust`; none at a whole price). The rest of the 42.5M, `GENESIS_SALE −
/// core − ff`, is UNSOLD and joins the protocol's steps at graduation.
pub fn genesis_sold(raised_core: u64, raised_ff: u64) -> FloorMathResult<(u64, u64)> {
    let core = genesis_tokens_at_price(raised_core)?.min(GENESIS_CORE_TOKENS);
    let ff = genesis_tokens_at_price(raised_ff)?.min(GENESIS_FF_TOKENS);
    Ok((core, ff))
}

/// ⛔ THE RAISE BUYERS' LOCK (locked for 30 days, a 14-day cliff): of an `allocation` ([`genesis_allocation`]), what
/// has vested `elapsed` seconds after the SALE END — 0 before the 14-day cliff, then
/// `allocation · min(elapsed, 30 d) / 30 d`, FLOORED (u128), so the cliff instant pays 14/30 at once and day 30 pays
/// the rest. A negative `elapsed` is 0. Never more than `allocation`.
/// `floor_types::constants::GENESIS_BUYER_{CLIFF,VEST}_SECS` are pinned to these by `tests/floor_types_pin.rs`.
pub const GENESIS_BUYER_VEST_SECS: u64 = 30 * 86_400;
pub const GENESIS_BUYER_CLIFF_SECS: u64 = 14 * 86_400;
pub fn genesis_buyer_vested(allocation: u64, elapsed: i64) -> u64 {
    if elapsed < GENESIS_BUYER_CLIFF_SECS as i64 {
        return 0;
    }
    let e = (elapsed as u64).min(GENESIS_BUYER_VEST_SECS);
    (allocation as u128 * e as u128 / GENESIS_BUYER_VEST_SECS as u128) as u64
}

/// `graduate`'s split (`:163-164`): `initialFloorEth = mulDiv(raised, floorShareBps, 10_000)` and
/// `offeredPolEth = raised − initialFloorEth`. `floor_share_bps > 10_000` is refused (`BadConfig` on
/// the EVM) rather than underflowing.
pub fn genesis_split(raised: u64, floor_share_bps: u16) -> FloorMathResult<(u64, u64)> {
    if floor_share_bps as u64 > BPS {
        return Err(FloorMathError::Overflow);
    }
    let initial = u64::try_from(raised as u128 * floor_share_bps as u128 / BPS as u128).map_err(|_| FloorMathError::Overflow)?;
    Ok((initial, raised - initial))
}

/// ⛔ `$FLOOR`'s STEP COUNT: each step is one doubling of the price from the open, and there are as many as reach about
/// the top the deployer set (≈ $2.6B). A program holds no USD: the TOP is a market cap IN QUOTE UNITS
/// (`GenesisSale.stair_top_mcap`, set by the genesis authority before the open — e.g. $2.56B at the SOL price of the
/// day), and `end_genesis_sale` fixes the count from it:
///
/// `steps = clamp(round(log2(top_mcap ÷ open_mcap)), 1, 16)`, rounded to the NEAREST doubling (`r ≥ 2^(k + ½)` rounds up;
/// `r` is rational and `2^(k + ½)` is not, so there is no tie), at least one step, at most `MAX_STAIR_STEPS` (16, the
/// `StairState`'s width), then reduced while the staircase does not fit above `open_tick` (`stair::stair_fits`; never
/// binds inside the genesis open clamp). A top at or under the open takes one.
///
/// The open is the FIXED price, so `open_mcap` is [`GENESIS_OPEN_MCAP_LAMPORTS`] (4,000 SOL) whatever was raised.
/// Worked: a top of `1.7067e16` lamports (≈ $2.56B at $150) is `r` = 4,266.7, `log2 r` = 12.06 → **12 steps**.
pub fn genesis_stair_steps(top_mcap: u64, open_tick: i32, spacing: i32) -> FloorMathResult<u8> {
    let max = crate::stair::MAX_STAIR_STEPS as u32;
    let open = GENESIS_OPEN_MCAP_LAMPORTS as u128;
    let top = top_mcap as u128;
    let mut n: u32 = if top <= open {
        0
    } else {
        // k = floor(log2(top / open)): open · 2^k ≤ top < open · 2^(k+1). open ≥ 1 and top < 2^64, so no shift passes 2^65.
        let mut k = 0u32;
        while (open << (k + 1)) <= top {
            k += 1;
        }
        let base = open << k; // ≤ top < 2^64
        // round to nearest: top / base ≥ √2 ⇔ top² ≥ 2 · base²
        let up = U256::from(top) * U256::from(top) >= U256::from(base) * U256::from(base) * U256::from(2u8);
        k + up as u32
    };
    n = n.clamp(1, max);
    let open_edge = crate::spacing::floor_to_spacing(open_tick, spacing)?;
    while n > 1 && !crate::stair::stair_fits(open_edge, n as u8, spacing)? {
        n -= 1;
    }
    Ok(n as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carpenter_clmm_math::tick_math::{get_sqrt_price_at_tick, MAX_SQRT_PRICE_X64, MAX_TICK};

    /// The open is the FIXED price — 4 SOL per 1M tokens, 250 base units a lamport, ln(250)/ln(1.0001) ≈ 55,216 —
    /// unclamped, a tick a pool can be initialised at.
    #[test]
    fn sgr_m1_the_open_is_the_fixed_price() {
        let (t, clamped) = genesis_open_tick().unwrap();
        assert!(!clamped && (55_210..=55_222).contains(&t), "{t}");
        assert_eq!(GENESIS_OPEN_TICK_MAX, MAX_TICK - 120 - 1, "`create_launch`'s own bound on an open");
        assert!(get_sqrt_price_at_tick(t).unwrap() < MAX_SQRT_PRICE_X64);
        assert!(t > GENESIS_OPEN_TICK_MIN && t < GENESIS_OPEN_TICK_MAX);
        // the fixed price is the old ratio open of a 40-SOL raise of 10M tokens: the same 250 base units a lamport
        let ratio = get_tick_at_sqrt_price(sqrt_price_from_ratio(40_000_000_000, 10_000_000_000_000).unwrap()).unwrap();
        assert_eq!(t, ratio);
    }

    /// The constants are one price: 4 SOL per 1M; phase 1's 20M cost 80 SOL (4 wallets × 20), phase 2's 22.5M 90 SOL
    /// (18 × 5); the open's market cap 4,000 SOL (≈ $600k at $150); 42.5M at most.
    #[test]
    fn sgr_m2_one_fixed_price() {
        const SOL: u64 = 1_000_000_000;
        assert_eq!(GENESIS_SALE_PRICE_LAMPORTS_PER_MILLION, 4 * SOL);
        assert_eq!((GENESIS_CORE_TOKENS, GENESIS_FF_TOKENS, GENESIS_SALE), (20_000_000_000_000, 22_500_000_000_000, 42_500_000_000_000));
        assert_eq!((GENESIS_CORE_CAP_LAMPORTS, GENESIS_FF_CAP_LAMPORTS), (80 * SOL, 90 * SOL));
        assert_eq!(GENESIS_OPEN_MCAP_LAMPORTS, 4_000 * SOL);
        assert_eq!(GENESIS_OPEN_MCAP_LAMPORTS as u128 * 150 / SOL as u128, 600_000, "$600k at $150");
        assert_eq!(genesis_tokens_at_price(SOL).unwrap(), 250_000_000_000, "1 SOL buys 250,000 tokens");
        assert_eq!(genesis_tokens_at_price(GENESIS_CORE_CAP_LAMPORTS).unwrap(), GENESIS_CORE_TOKENS);
        assert_eq!(genesis_tokens_at_price(GENESIS_FF_CAP_LAMPORTS).unwrap(), GENESIS_FF_TOKENS);
        assert_eq!(genesis_tokens_at_price(1).unwrap(), 250);
        assert_eq!(genesis_tokens_at_price(5 * SOL).unwrap(), 1_250_000_000_000, "5 SOL buys 1.25M in Phase 2");
    }

    /// ⛔ THE TWO-PHASE RAISE: every buyer pays the same price in both phases — a wallet's allocation is its lamports
    /// × 250, its two phases' parts added; nothing depends on what others put in. Mutations: a phase priced apart, a phase's
    /// part dropped.
    #[test]
    fn sgr_m3_both_phases_buy_at_the_one_price() {
        const SOL: u64 = 1_000_000_000;
        const M: u64 = 1_000_000_000_000;
        assert_eq!(genesis_allocation(20 * SOL, 5 * SOL).unwrap(), 5 * M + 1_250_000_000_000, "a Core wallet in Phase 2 too");
        assert_eq!(genesis_allocation(0, 5 * SOL).unwrap(), 1_250_000_000_000);
        assert_eq!(genesis_allocation(7, 3).unwrap(), 2_500);
        assert_eq!(genesis_allocation(0, 0).unwrap(), 0);
        assert!(genesis_allocation(u64::MAX, 1).is_err(), "an overflow is refused, never wrapped");
    }

    /// What is sold and what is unsold — four cases. Full 80 + 90: all 42.5M. 80 + 45: 20M + 11.25M, 11.25M unsold.
    /// 80 + 0: 20M, 22.5M unsold. 40 + 20: 10M + 5M, 27.5M unsold.
    #[test]
    fn sgr_m4_sold_and_unsold() {
        const SOL: u64 = 1_000_000_000;
        const M: u64 = 1_000_000_000_000;
        for ((c, f), (sc, sf)) in [((80, 90), (20 * M, 22_500_000_000_000)), ((80, 45), (20 * M, 11_250_000_000_000)), ((80, 0), (20 * M, 0)), ((40, 20), (10 * M, 5 * M))] {
            assert_eq!(genesis_sold(c * SOL, f * SOL).unwrap(), (sc, sf), "{c} + {f}");
        }
        assert_eq!(GENESIS_SALE - 10 * M - 5 * M, 27_500_000_000_000, "40 + 20 leaves 27.5M to the protocol's steps");
        assert_eq!(genesis_sold(0, 0).unwrap(), (0, 0));
        // each phase's sold never passes its tokens, even if handed more than the cap (the program trims at 80 / 90 SOL)
        assert_eq!(genesis_sold(500 * SOL, 500 * SOL).unwrap(), (20 * M, 22_500_000_000_000));
    }

    /// Σ allocations = what was sold at the whole price (no dust), whatever the wallets' split.
    #[test]
    fn sgr_m5_allocations_never_pass_what_was_sold() {
        const SOL: u64 = 1_000_000_000;
        let core = [20 * SOL, 20 * SOL, 7_333_333_333, 1];
        let ff = [5 * SOL, 3_333_333_333, 4_999_999_999, 1, 7];
        let (rc, rf) = (core.iter().sum::<u64>(), ff.iter().sum::<u64>());
        let (sc, sf) = genesis_sold(rc, rf).unwrap();
        let got: u64 = core.iter().chain(&ff).map(|c| genesis_tokens_at_price(*c).unwrap()).sum();
        assert_eq!(got, sc + sf, "250 base units a lamport: no dust");
    }

    /// The floor share split.
    #[test]
    fn sgr_m6_split() {
        assert_eq!(genesis_split(3_000, 10_000).unwrap(), (3_000, 0));
        assert_eq!(genesis_split(3_000, 0).unwrap(), (0, 3_000));
        assert_eq!(genesis_split(3, 5_000).unwrap(), (1, 2));
        assert!(genesis_split(3, 10_001).is_err());
    }

    /// `$FLOOR` on the staircase: the step count is `clamp(round(log2(top ÷ open_mcap)), 1, 16)` — the open's market cap
    /// FIXED at 4,000 SOL. Mutations this kills: floor instead of round, no clamp at either end, a top under the open
    /// giving 0.
    #[test]
    fn sgr_m7_the_step_count_is_the_nearest_doubling_to_the_top() {
        let s = 60;
        let (t, _) = genesis_open_tick().unwrap();
        let steps = |top: u64| genesis_stair_steps(top, t, s).unwrap();
        let open = GENESIS_OPEN_MCAP_LAMPORTS;
        // $2.56B at $150 = 17,066,666.67 SOL; r = 4,266.7 → log2 12.06 → 12 steps
        assert_eq!(steps(17_066_666_666_666_667), 12, "the suite's top");
        // exact doublings, and the half-doubling boundary (√2 ≈ 1.41421356)
        assert_eq!(steps(open << 9), 9);
        assert_eq!(steps((open << 9) + (open << 9) / 100_000 * 41_421), 9, "just under 2^9.5 rounds down");
        assert_eq!(steps((open << 9) + (open << 9) / 100_000 * 41_422), 10, "just over 2^9.5 rounds up");
        assert_eq!(steps(u64::MAX), 16, "u64::MAX lamports is 2^22.1 of the open: clamped at 16");
        // a top at or under the open: one step, never zero
        assert_eq!(steps(1), 1);
        assert_eq!(steps(open), 1);
        assert_eq!(steps(open * 2), 1, "one doubling");
        // every count fits above every open the clamp allows (the fit check never binds on the genesis path)
        for t in [GENESIS_OPEN_TICK_MIN, 0, GENESIS_OPEN_TICK_MAX] {
            let e = crate::spacing::floor_to_spacing(t, s).unwrap();
            assert!(crate::stair::stair_fits(e, 16, s).unwrap(), "{t}");
        }
    }

    /// The count does not depend on the raise; the top decides — $2.56B 12 (log2 12.06), $1B 11 (log2 10.70), $5B 13
    /// (log2 13.02), $500M 10 (log2 9.70) at $150.
    #[test]
    fn sgr_m8_the_top_alone_sets_the_count() {
        let (t, _) = genesis_open_tick().unwrap();
        let at = |usd: u128| genesis_stair_steps((usd * 1_000_000_000 / 150) as u64, t, 60).unwrap();
        assert_eq!(at(2_560_000_000), 12);
        assert_eq!(at(1_000_000_000), 11);
        assert_eq!(at(5_000_000_000), 13);
        assert_eq!(at(500_000_000), 10);
    }

    /// The raise buyers' lock: nothing before day 14, exactly 14/30 at the cliff, linear to day 30, all from then;
    /// floored; never above the allocation. The worked example: 1M bought → 466,666.666… at day 14 (466,666,666,666
    /// base units), 733,333,333,333 at day 22, all at day 30.
    #[test]
    fn sgr_m9_the_buyers_vest_over_thirty_days_from_a_fourteen_day_cliff() {
        const D: i64 = 86_400;
        let a = 1_000_000_000_000u64; // 1M tokens
        assert_eq!(genesis_buyer_vested(a, -1), 0);
        assert_eq!(genesis_buyer_vested(a, 0), 0);
        assert_eq!(genesis_buyer_vested(a, 14 * D - 1), 0, "a second before the cliff");
        assert_eq!(genesis_buyer_vested(a, 14 * D), 466_666_666_666, "14/30 at the cliff");
        assert_eq!(genesis_buyer_vested(a, 22 * D), 733_333_333_333);
        assert_eq!(genesis_buyer_vested(a, 30 * D - 1), a - 385_803, "a second before the end: floored");
        assert_eq!(genesis_buyer_vested(a, 30 * D), a);
        assert_eq!(genesis_buyer_vested(a, 365 * D), a, "never more than the allocation");
        assert_eq!(genesis_buyer_vested(u64::MAX, 20 * D), (u64::MAX as u128 * 20 / 30) as u64, "u128: no overflow");
        let mut last = 0;
        for t in (14 * D..=31 * D).step_by(3_607) {
            let v = genesis_buyer_vested(a, t);
            assert!(v >= last && v <= a);
            last = v;
        }
    }
}
