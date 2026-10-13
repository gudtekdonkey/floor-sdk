//! ⛔ THE CUSHION FOLLOWS NET BUYING — the EVM twin's "net-flow meter" (`FloorPolicy.STAIR_CUSHION_NETFLOW_BPS` /
//! `STAIR_CUSHION_NETFLOW_RELAX_BPS`, `SingletonFloorEngine._noteFlow`, `SingletonFloorStairs._flowCap` /
//! `_climbHold`). Behind a compile-time switch that ⛔ SHIPS ON: H = 130 bps of the market cap, K = 0, and the climb
//! hold's reach 954 ticks — parity with the EVM.
//!
//! Today the staircase's cushion follows the TIME-averaged price (`min(spot, 15-min TWAP)`), and time is free to a pumper:
//! he climbs a quiet pool in small steps, waits, and sells into the cushion that followed him up. With the switch ON the
//! cushion's top also follows where the recent NET BUYERS paid, measured by VOLUME:
//!
//! - **The meter** ([`note_flow_with`], written by `floor_hook`'s `after_swap` on every swap that reaches the hook — the hook's
//!   own nudge never does, as the EVM's `sender == this` returns early). A BUY adds its quote leg at its mean tick
//!   `(pre + post) / 2`, first scaling what the meter holds down at its mean so the weight never passes
//!   `H = mcap · bps / 10,000 + 1` (the meter remembers the last H of net buying; `mcap` is the base supply priced at the
//!   tick after the swap). A SELL takes its leg off at the DEARER of its STARTING tick and the meter's mean (emptying it at
//!   most): a sell, and so any round trip, can only pull the mean cheaper — never raise it. With K ≠ 0 either leg then pulls
//!   the mean toward its own mean tick by `leg / (K + leg)`. Nothing moves with time.
//! - **The cap** ([`flow_cap`], read by the ratchet's cushion target): a STANDING cushion's top climbs no dearer than the
//!   meter's mean rounded to the cheaper tick, and never moves down for it (a cushion already dearer than the mean keeps
//!   its top); an empty meter climbs nothing; no cushion standing: the target as today. `min(spot, TWAP)` still applies.
//! - **The climb hold** (`floor_hook::mech::netflow::climb_line`, folded into the ratchet's hold in `mech::ratchet::run`): while the cushion stands cheaper than the spot's range,
//!   no floor the ratchet publishes spends what the cushion's later climb to the spot's range will need.
//!
//! ⛔ Ticks: a DEARER price is a LOWER tick on every FLOOR pool (the quote is token0), so Solana's raw tick IS the
//! EVM's normalised one, and "the dearer of" is the LOWER, "rounded to the cheaper tick" is the ceiling. The EVM's
//! operations are kept in its order with its rounding: Solidity's signed division truncates toward zero, and so does
//! Rust's; every product that can pass `i128` is taken in 256 bits ([`mul_div_trunc`]).
//!
//! ⚠ TWO BOUNDS THE EVM DOES NOT HAVE, both unreachable on a real pool and stated for the auditor:
//! - `H` and `K` are clamped to `u64::MAX` raw quote units (≈ 1.8e10 SOL of market cap × the bps), so the weight is a u64.
//! - The meter is `(a: i128, b: u64)`; the EVM's is `int256[2]`. A step that would leave `a` outside `i128` (a mean past
//!   2^63 ticks at full weight — only a chain of near-total sells can push it there) EMPTIES the meter instead of
//!   reverting the trader's swap: an empty meter climbs nothing, as a mean that far cheaper than any price does, but the
//!   next buy then sets the line alone. The EVM's checked `int256` would revert the swap there instead (at 2^255).
//!
//! Cross-chain vectors: [`tests`] pins [`note_flow_with`] and [`flow_cap`] on hand-worked cases. The EVM has no PURE form of
//! either (`_noteFlow` reads and writes storage, `_flowCap` reads it), so ⚠ these vectors are WRITTEN HERE and must be
//! pinned on the EVM too (a harness exposing the two bodies over the same inputs).

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::full_math::MulDiv;

use crate::constants::BPS;

/// THE SWITCH: the meter's memory H, in bps of the market cap. ⛔ SHIPS ON AT 130 (parity with
/// `FloorPolicy.STAIR_CUSHION_NETFLOW_BPS = 130`). 0 would
/// be OFF: no swap reads or writes the meter, no ratchet caps or holds anything.
pub const STAIR_CUSHION_NETFLOW_BPS: u64 = 130;
/// THE CATCH-UP BY VOLUME, K, in bps of the market cap (with [`STAIR_CUSHION_NETFLOW_BPS`]; 0 = none). Parity with
/// `FloorPolicy.STAIR_CUSHION_NETFLOW_RELAX_BPS = 0`.
pub const STAIR_CUSHION_NETFLOW_RELAX_BPS: u64 = 0;

/// THE CLIMB HOLD'S REACH, in ticks (parity with `FloorPolicy.STAIR_CLIMB_HOLD_REACH_TICKS = 954`). With a standing
/// cushion the hold reserves money only for the cushion's next REACH ticks of climb from where it stands, not the whole
/// climb to the spot's range: the hold's target tick is raised to `cushion_lower − 1 − REACH` when that is cheaper than
/// the spot ([`climb_hold_target`]).
/// ⛔ SHIPS AT 954 (≈ 10 % of price), with the dial at 130. 0 would be the FULL hold (the
pub const STAIR_CLIMB_HOLD_REACH_TICKS: i32 = 954;

const _: () = assert!(STAIR_CLIMB_HOLD_REACH_TICKS >= 0);

const _: () = assert!(STAIR_CUSHION_NETFLOW_BPS <= BPS && (STAIR_CUSHION_NETFLOW_BPS != 0 || STAIR_CUSHION_NETFLOW_RELAX_BPS == 0));

/// Whether the net-flow meter is switched on.
pub const fn cushion_netflow_on() -> bool {
    STAIR_CUSHION_NETFLOW_BPS != 0
}

/// The meter: `a` = Σ quote × tick (raw quote units × ticks), `b` = Σ quote (raw quote units), so the mean tick is `a / b`.
/// Empty when `b == 0` (and then `a` is 0). The EVM's `cushionFlow[poolId] = [a, b]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetFlow {
    pub a: i128,
    pub b: u64,
}

impl NetFlow {
    pub fn is_empty(&self) -> bool {
        self.b == 0
    }

    /// The mean tick, truncated toward zero (the EVM's `a / b`), or `None` when empty.
    pub fn mean(&self) -> Option<i128> {
        (self.b != 0).then(|| self.a / self.b as i128)
    }
}

/// `trunc(x · m / d)` toward zero, for a signed `x` (Solidity's `x * m / d` on `int256` with `m`, `d` > 0), in 256 bits;
/// `None` when `d == 0` or the result leaves `i128`.
pub fn mul_div_trunc(x: i128, m: u128, d: u128) -> Option<i128> {
    if d == 0 {
        return None;
    }
    let mag = U256::from(x.unsigned_abs()).mul_div_floor(U256::from(m), U256::from(d))?;
    if mag > U256::from(i128::MAX as u128) {
        return None;
    }
    let mag = mag.as_u128() as i128;
    Some(if x < 0 { -mag } else { mag })
}

/// `mcap · bps / 10,000 + 1`, clamped to `u64::MAX` (the EVM's `mcap * int256(BPS) / 10_000 + 1`).
pub fn share_of_mcap(mcap: u128, bps: u64) -> u64 {
    let x = U256::from(mcap) * U256::from(bps) / U256::from(BPS) + U256::from(1u8);
    if x > U256::from(u64::MAX) {
        u64::MAX
    } else {
        x.as_u64()
    }
}

/// [`note_flow_with`] at the shipped switch ([`STAIR_CUSHION_NETFLOW_BPS`], [`STAIR_CUSHION_NETFLOW_RELAX_BPS`]); OFF it
/// returns the meter unchanged.
pub fn note_flow(f: NetFlow, buy: bool, v: u64, pre: i32, post: i32, mcap: u128) -> NetFlow {
    if !cushion_netflow_on() {
        return f;
    }
    note_flow_with(f, buy, v, pre, post, mcap, STAIR_CUSHION_NETFLOW_BPS, STAIR_CUSHION_NETFLOW_RELAX_BPS)
}

/// THE METER'S UPDATE for one swap (`SingletonFloorEngine._noteFlow`, its operations in its order): `buy` is the trader's
/// direction, `v` the swap's quote leg (raw, the absolute value), `pre` / `post` the pool's tick before and after the
/// swap's walk, `mcap` the base supply priced at `post` (`floor_math::quote_cost(supply, post)`), `h_bps` / `k_bps` the
/// switch's H and K. A zero leg changes nothing. Never fails: a step past `i128` empties the meter (module docs).
#[allow(clippy::too_many_arguments)]
pub fn note_flow_with(f: NetFlow, buy: bool, v: u64, pre: i32, post: i32, mcap: u128, h_bps: u64, k_bps: u64) -> NetFlow {
    if v == 0 {
        return f;
    }
    step(f, buy, v, pre, post, mcap, h_bps, k_bps).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn step(f: NetFlow, buy: bool, v: u64, pre: i32, post: i32, mcap: u128, h_bps: u64, k_bps: u64) -> Option<NetFlow> {
    let (mut a, mut b) = (f.a, f.b);
    // the swap's mean tick: Solidity's `(pre + int256(post)) / 2`, truncated toward zero
    let mid = (pre as i128 + post as i128) / 2;
    if buy {
        let h = share_of_mcap(mcap, h_bps);
        if STAIR_CUSHION_NETFLOW_HALF_RULE && b > 0 && 2 * v as u128 > h as u128 {
            // ⛔ THE HALF RULE: while the meter holds anything, one buy
            // takes at most half of it -- `w = h − ⌊h/2⌋` of weight at its own LAST-SLICE tick ([`slice_tick`]); the older
            // weight is scaled down at its mean to the other half, `⌊h/2⌋`
            let w = h - h / 2;
            let m = slice_tick(pre, post, w, v)?;
            let keep = h - w;
            if b > keep {
                a = mul_div_trunc(a, keep as u128, b as u128)?;
                b = keep;
            }
            a = a.checked_add((w as i128).checked_mul(m)?)?;
            b = b.checked_add(w)?;
        } else if v >= h {
            // ⛔ THE LAST SLICE: a buy of H or more into an EMPTY meter is remembered by its own last H
            // of quote, at that slice's tick ([`slice_tick`]) -- not its mean tick
            if STAIR_CUSHION_NETFLOW_HALF_RULE {
                let m = slice_tick(pre, post, h, v)?;
                (a, b) = ((h as i128).checked_mul(m)?, h);
            } else {
                (a, b) = ((v as i128).checked_mul(mid)?, v);
            }
        } else if b as u128 + v as u128 > h as u128 {
            // older weight scaled down at its mean to make room: `a * (h - v) / b`, `b = h - v`
            a = mul_div_trunc(a, (h - v) as u128, b as u128)?;
            b = h - v;
            a = a.checked_add((v as i128).checked_mul(mid)?)?;
            b = b.checked_add(v)?;
        } else {
            a = a.checked_add((v as i128).checked_mul(mid)?)?;
            b = b.checked_add(v)?;
        }
    } else if b > 0 {
        // a sell comes off at the DEARER (lower tick) of where it started and the mean: never raises the mean
        let mut l = a / b as i128;
        if (pre as i128) < l {
            l = pre as i128;
        }
        if v >= b {
            (a, b) = (0, 0);
        } else {
            a = a.checked_sub((v as i128).checked_mul(l)?)?;
            b -= v;
        }
    }
    if k_bps != 0 && b > 0 {
        // the catch-up by volume: `a += (mid * b - a) * v / (k + v)`
        let k = share_of_mcap(mcap, k_bps);
        let gap = mid.checked_mul(b as i128)?.checked_sub(a)?;
        a = a.checked_add(mul_div_trunc(gap, v as u128, k as u128 + v as u128)?)?;
    }
    Some(NetFlow { a, b })
}

/// ⛔ THE HALF RULE AND THE LAST SLICE (after a pump, a sell should not find the cushion at the pumped price): a buy is
/// remembered by the tick of its own LAST `w` of quote, and while the meter holds anything one buy takes at most half of
/// it. `true` ships; `false` is the meter as first built (the buy at its mean tick, a buy of H or more replacing the whole
/// meter), kept for measurement. Parity: `FloorPolicy.STAIR_CUSHION_NETFLOW_HALF_RULE`.
pub const STAIR_CUSHION_NETFLOW_HALF_RULE: bool = true;

/// THE LAST SLICE's tick: the mean tick of the last `w` of a buy's `v` quote, as if the buy paid its quote evenly over
/// its ticks from `pre` to `post`: `post + trunc((pre − post) · w / (2 v))` (Solidity's signed division, toward zero). Ticks
/// only -- no sqrt-price table -- so both chains compute it to the bit; against the constant-liquidity slice it is never
/// DEARER (quote per tick grows with the price), so the line it sets is the conservative one. `w >= v` gives the buy's mean
/// tick (truncated toward `post`). `None` when `v == 0` or a step leaves `i128`.
pub fn slice_tick(pre: i32, post: i32, w: u64, v: u64) -> Option<i128> {
    if v == 0 {
        return None;
    }
    let w = w.min(v);
    let d = pre as i128 - post as i128;
    let num = d.checked_mul(w as i128)?;
    Some(post as i128 + num / (2 * v as i128))
}

/// THE CAP on the cushion's target top (`SingletonFloorStairs._flowCap`): `top` is the target the TWAP rule gave (its
/// top tick, `max(spot, settled)`), `standing_lower` the standing cushion's dear edge (`StairState.cushion_lower`; `None`
/// when no cushion stands, which returns `top`). The cushion may climb no dearer than the meter's mean rounded to the
/// CHEAPER tick, and never moves down for it: `max(top, min(standing_lower − 1, ceil(mean)))` — an empty meter keeps the
/// standing top (climbs nothing).
pub fn flow_cap(top: i32, standing_lower: Option<i32>, f: NetFlow) -> i32 {
    let Some(lower) = standing_lower else { return top };
    let mut l = lower as i128 - 1;
    if f.b > 0 {
        let b = f.b as i128;
        let mut m = f.a / b;
        if f.a % b != 0 && f.a > 0 {
            m += 1;
        }
        if m < l {
            l = m;
        }
    }
    if l > top as i128 {
        l.min(i32::MAX as i128) as i32
    } else {
        top
    }
}

/// THE CLIMB HOLD's line, pure (`SingletonFloorStairs._climbLine`; `floor_hook::mech::netflow::climb_line` reads the
/// accounts and calls this). `sqrt_price_x64` / `tick` the market, `state` the pool's `FloorState` (spacing, floor, POL,
/// inventory, the staircase's standing tokens, `min_sellable`), `supply` the base mint's, `vault_q` what the passes solve
/// on (`vault_eth − reserve`), `cushion` the standing cushion `(lower, upper, liquidity)` after the half placed it, `width`
/// its depth in ticks. Every step is recorded in [`ClimbLine`] so the cross-chain vectors pin the intermediates as well as
/// the answer.
pub fn climb_line_parts(
    sqrt_price_x64: u128,
    tick: i32,
    state: &crate::state::FloorState,
    supply: u64,
    vault_q: u64,
    cushion: Option<(i32, i32, u128)>,
    width: i32,
) -> crate::FloorMathResult<ClimbLine> {
    climb_line_parts_reach(sqrt_price_x64, tick, state, supply, vault_q, cushion, width, STAIR_CLIMB_HOLD_REACH_TICKS)
}

/// THE HOLD'S TARGET TICK (`_climbLine`'s `to`, the REACH clamp): the spot `tick`, raised (made cheaper) to
/// `cushion_lower − 1 − reach` when that is higher and a cushion stands with liquidity. `reach` 0 = the full hold: `tick`.
/// The ONE spelling of the clamp; [`climb_line_parts`] reads it at [`STAIR_CLIMB_HOLD_REACH_TICKS`].
pub fn climb_hold_target(tick: i32, cushion: Option<(i32, i32, u128)>, reach: i32) -> i32 {
    match cushion {
        Some((lower, _, liq)) if reach != 0 && liq != 0 => {
            let r = (lower as i64 - 1 - reach as i64).max(i32::MIN as i64) as i32;
            r.max(tick)
        }
        _ => tick,
    }
}

/// [`climb_line_parts`] at an explicit `reach` (ticks; 0 = the full hold): the tests and the vectors pin the clamp at
/// values the shipped constant does not carry yet.
#[allow(clippy::too_many_arguments)]
pub fn climb_line_parts_reach(
    sqrt_price_x64: u128,
    tick: i32,
    state: &crate::state::FloorState,
    supply: u64,
    vault_q: u64,
    cushion: Option<(i32, i32, u128)>,
    width: i32,
    reach: i32,
) -> crate::FloorMathResult<ClimbLine> {
    climb_line_core(sqrt_price_x64, tick, state, supply, vault_q, cushion, width, reach, None)
}

/// ⛔ THE CLIMB HOLD'S BRACKET (the compute headroom): [`climb_line`] with its
/// solve clamped to `[lo, hi]` (grid ticks; anything else is the full solve). The ratchet's one reader adopts the line
/// only through `published = max(stand, line)` and the passes' `floor = min(cap, floor0)`: a line at or cheaper than the
/// stand line changes nothing, and one at or dearer than the standing floor acts as the standing floor. With
/// `hi = floor0` and `lo = floor_to_spacing(min(stand, floor0) − band − 2·spacing)` — the passes' own bracket — a solve
/// clamped there publishes a line that changes exactly what the full solve's would (`publish` is monotone; the clamp
/// of the full solve is `solve_floor_tick_within`'s contract). The same answer, ~a quarter of the compute.
#[allow(clippy::too_many_arguments)]
pub fn climb_line_within(
    sqrt_price_x64: u128,
    tick: i32,
    state: &crate::state::FloorState,
    supply: u64,
    vault_q: u64,
    cushion: Option<(i32, i32, u128)>,
    width: i32,
    lo: i32,
    hi: i32,
) -> crate::FloorMathResult<Option<i32>> {
    Ok(climb_line_core(sqrt_price_x64, tick, state, supply, vault_q, cushion, width, STAIR_CLIMB_HOLD_REACH_TICKS, Some((lo, hi)))?
        .line)
}

#[allow(clippy::too_many_arguments)]
fn climb_line_core(
    sqrt_price_x64: u128,
    tick: i32,
    state: &crate::state::FloorState,
    supply: u64,
    vault_q: u64,
    cushion: Option<(i32, i32, u128)>,
    width: i32,
    reach: i32,
    bracket: Option<(i32, i32)>,
) -> crate::FloorMathResult<ClimbLine> {
    let mut out = ClimbLine::default();
    let Some((lower, upper, liq)) = cushion else { return Ok(out) };
    let s = state.tick_spacing;
    // the hold's target: the spot, or the cushion's next `reach` ticks of climb (the EVM's `to`)
    let to = climb_hold_target(tick, cushion, reach);
    // the TARGET's range above the band (the EVM's `_cushionRange(to)`)
    let Some((lo, hi)) = crate::stair::cushion_range_above_w(to, to, s, state.floor_tick, width)? else { return Ok(out) };
    out.spot_range = Some((lo, hi));
    // it already stands at or dearer than that range: nothing to hold for
    if lower <= lo {
        return Ok(out);
    }
    // the cushion's whole quote, as it would stand over the spot's range
    let q = crate::position::position_amounts(sqrt_price_x64, lower, upper, liq)?.0;
    out.cushion_quote = q;
    if q == 0 {
        return Ok(out);
    }
    let l = crate::position::position_liquidity(sqrt_price_x64, lo, hi, q, 0)?;
    out.liquidity = l;
    if vault_q == 0 {
        // an empty vault holds the floor where it is
        out.line = Some(state.floor_tick);
        return Ok(out);
    }
    let more = [(lo, hi, l)];
    // ⛔ THE CLIMB HOLD'S BRACKET: the solve CLAMPED to the caller's bracket when it
    // gives one (`[bracket.0, bracket.1]`, grid ticks): the full grid solve was ~190k CU a ratchet (the profile), and
    // the ratchet reads the line only between its stand line and its standing floor (`climb_line_within`'s doc)
    let solved = match bracket {
        Some((blo, bhi)) if blo < bhi && blo % s == 0 && bhi % s == 0 => {
            crate::solve::solve_floor_tick_within(vault_q as u128, state, supply, &more, blo, bhi)?
        }
        _ => crate::solve::solve_floor_tick_with_more(vault_q as u128, 0, state, supply, &more)?,
    };
    out.solved = Some(solved);
    out.line = Some(crate::band::publish(solved, s)?);
    Ok(out)
}

/// The climb hold's line ([`climb_line_parts`]'s answer): `None` when nothing holds.
pub fn climb_line(
    sqrt_price_x64: u128,
    tick: i32,
    state: &crate::state::FloorState,
    supply: u64,
    vault_q: u64,
    cushion: Option<(i32, i32, u128)>,
    width: i32,
) -> crate::FloorMathResult<Option<i32>> {
    Ok(climb_line_parts(sqrt_price_x64, tick, state, supply, vault_q, cushion, width)?.line)
}

/// Every step of [`climb_line_parts`]: the spot's range above the band, the cushion's quote at the market, the
/// liquidity that quote makes over the spot's range, the solve (the dear edge) and the published line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClimbLine {
    pub spot_range: Option<(i32, i32)>,
    pub cushion_quote: u64,
    pub liquidity: u128,
    pub solved: Option<i32>,
    pub line: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: u64 = 130;
    /// mcap 1e12 raw at 130 bps: h = 1e12 · 130 / 10,000 + 1 = 13,000,000,001.
    const MCAP: u128 = 1_000_000_000_000;

    fn note(f: NetFlow, buy: bool, v: u64, pre: i32, post: i32) -> NetFlow {
        note_flow_with(f, buy, v, pre, post, MCAP, H, 0)
    }

    /// ⛔ THE SWITCH SHIPS ON AT 130 (the EVM's pin too): H 130, K 0, the switch reads on, and the shipped entry point
    /// writes the meter on a buy exactly as
    /// [`note_flow_with`] at 130 / 0 does.
    #[test]
    fn the_netflow_switch_ships_at_130() {
        assert_eq!(STAIR_CUSHION_NETFLOW_BPS, 130, "the net-flow cushion cap ships ON at 130");
        assert_eq!(STAIR_CUSHION_NETFLOW_RELAX_BPS, 0, "its volume catch-up ships at none");
        assert!(STAIR_CUSHION_NETFLOW_HALF_RULE, "the half rule and the last slice ship ON");
        assert!(cushion_netflow_on());
        let f = NetFlow { a: -7, b: 3 };
        let out = note_flow(f, true, 1_000, -10, -20, MCAP);
        assert_ne!(out, f, "a buy writes the meter");
        assert_eq!(out, note_flow_with(f, true, 1_000, -10, -20, MCAP, 130, 0));
    }

    /// CROSS-CHAIN VECTORS (written here; ⚠ to be pinned on the EVM by a harness over `_noteFlow`'s body): `(meter in,
    /// buy, v, pre, post, mcap, H, K) → meter out`, each worked by hand from the EVM's operations.
    #[test]
    fn vectors_note_flow() {
        let h = share_of_mcap(MCAP, H);
        assert_eq!(h, 13_000_000_001);
        let cases: &[(NetFlow, bool, u64, i32, i32, u128, u64, u64, NetFlow)] = &[
            // 1. a buy into an empty meter at its mean tick (-1000 + -1100) / 2 = -1050
            (NetFlow::default(), true, 1_000, -1_000, -1_100, MCAP, H, 0, NetFlow { a: -1_050_000, b: 1_000 }),
            // 2. the mean tick truncates toward zero: (-1001 + -1000) / 2 = -1000 (Solidity), not -1001
            (NetFlow::default(), true, 10, -1_001, -1_000, MCAP, H, 0, NetFlow { a: -10_000, b: 10 }),
            // 3. ... and on the positive side (7 + 8) / 2 = 7
            (NetFlow::default(), true, 10, 7, 8, MCAP, H, 0, NetFlow { a: 70, b: 10 }),
            // 4. a second buy adds: (-1050·1000 + -1200·3000) / 4000
            (NetFlow { a: -1_050_000, b: 1_000 }, true, 3_000, -1_150, -1_250, MCAP, H, 0, NetFlow { a: -4_650_000, b: 4_000 }),
            // 5. a sell that started CHEAPER than the mean comes off at the mean (-1162): the mean cannot rise
            (NetFlow { a: -4_650_000, b: 4_000 }, false, 1_000, -1_100, -1_050, MCAP, H, 0, NetFlow { a: -3_488_000, b: 3_000 }),
            // 6. a sell that started DEARER than the mean comes off at its start (-1300): the mean falls cheaper
            (NetFlow { a: -4_650_000, b: 4_000 }, false, 1_000, -1_300, -1_250, MCAP, H, 0, NetFlow { a: -3_350_000, b: 3_000 }),
            // 7. a sell of the whole weight or more empties it
            (NetFlow { a: -4_650_000, b: 4_000 }, false, 4_000, -1_300, -1_000, MCAP, H, 0, NetFlow::default()),
            // 8. a sell into an empty meter changes nothing
            (NetFlow::default(), false, 4_000, -1_300, -1_000, MCAP, H, 0, NetFlow::default()),
            // 9. ⛔ THE HALF RULE: a buy of H into a meter holding anything takes w = h − ⌊h/2⌋ = 6,500,000,001 at
            // its last slice's tick, -2100 + trunc(100·w / 2v) = -2075; the old 4,000 (under ⌊h/2⌋) is kept whole
            (
                NetFlow { a: -4_650_000, b: 4_000 },
                true,
                13_000_000_001,
                -2_000,
                -2_100,
                MCAP,
                H,
                0,
                NetFlow { a: -4_650_000 - 6_500_000_001i128 * 2_075, b: 4_000 + 6_500_000_001 },
            ),
            // 10. past H: the old weight scaled to h - v at its mean, a·(h−v)/b truncated toward zero
            (
                NetFlow { a: -12_000_000_000 * 1_000 - 7, b: 12_000_000_000 },
                true,
                2_000_000_000,
                -1_400,
                -1_600,
                MCAP,
                H,
                0,
                NetFlow { a: (-12_000_000_000i128 * 1_000 - 7) * 11_000_000_001 / 12_000_000_000 - 2_000_000_000 * 1_500, b: 13_000_000_001 },
            ),
            // 11. the catch-up K = 500 bps: k = 50,000,000,001; a += (mid·b − a)·v/(k + v)
            (
                NetFlow { a: -4_650_000, b: 4_000 },
                false,
                1_000,
                -1_100,
                -1_050,
                MCAP,
                H,
                500,
                NetFlow { a: -3_488_000 + (-1_075i128 * 3_000 - -3_488_000) * 1_000 / 50_000_001_001, b: 3_000 },
            ),
            // 12. a zero leg changes nothing
            (NetFlow { a: -4_650_000, b: 4_000 }, true, 0, -1, -2, MCAP, H, 500, NetFlow { a: -4_650_000, b: 4_000 }),
            // 13. ⛔ THE LAST SLICE: a buy of 4·H into an EMPTY meter is remembered by its last H, at
            // -3000 + trunc(1000·h / 2v) = -3000 + 125 = -2875 -- not its mean -2500
            (NetFlow::default(), true, 52_000_000_004, -2_000, -3_000, MCAP, H, 0, NetFlow { a: 13_000_000_001i128 * -2_875, b: 13_000_000_001 }),
            // 14. the half rule scales a FULL meter down to ⌊h/2⌋ at its mean (-1000), then adds w at the slice
            (
                NetFlow { a: -1_000 * 13_000_000_001i128, b: 13_000_000_001 },
                true,
                26_000_000_002,
                -2_000,
                -3_000,
                MCAP,
                H,
                0,
                NetFlow { a: -1_000i128 * 6_500_000_000 + 6_500_000_001i128 * (-3_000 + 1_000 * 6_500_000_001 / 52_000_000_004), b: 13_000_000_001 },
            ),
            // 15. a buy under H but over H/2 into a holding meter is the half rule too (slice -1100 + trunc(100·w / 2v) = -1054)
            (NetFlow { a: -4_650_000, b: 4_000 }, true, 7_000_000_000, -1_000, -1_100, MCAP, H, 0, NetFlow { a: -4_650_000 + 6_500_000_001i128 * -1_054, b: 4_000 + 6_500_000_001 }),
        ];
        for (i, (f, buy, v, pre, post, mcap, h, k, want)) in cases.iter().enumerate() {
            assert_eq!(note_flow_with(*f, *buy, *v, *pre, *post, *mcap, *h, *k), *want, "vector {}", i + 1);
        }
    }

    /// CROSS-CHAIN VECTORS for the cap (⚠ to be pinned on the EVM over `_flowCap`'s body): `(top, standing lower, meter) →
    /// top`.
    #[test]
    fn vectors_flow_cap() {
        let m = |a: i128, b: u64| NetFlow { a, b };
        let cases: &[(i32, Option<i32>, NetFlow, i32)] = &[
            // no cushion standing: the target, whatever the meter
            (-2_000, None, m(-5_000_000, 1_000), -2_000),
            // empty meter: the standing top (lower − 1) — climbs nothing — unless the target is cheaper
            (-2_000, Some(-1_500), NetFlow::default(), -1_501),
            (-1_000, Some(-1_500), NetFlow::default(), -1_000),
            // a mean between the target and the standing top: the cushion climbs to the mean, ceiled (cheaper)
            (-2_000, Some(-1_500), m(-1_750_500, 1_000), -1_750),
            (-2_000, Some(-1_500), m(-1_750_000, 1_000), -1_750),
            // a positive mean ceils upward: 1750.5 → 1751
            (1_000, Some(2_000), m(1_750_500, 1_000), 1_751),
            // a mean dearer than the target: the target (min(spot, TWAP) still applies)
            (-2_000, Some(-1_500), m(-2_500_000, 1_000), -2_000),
            // a mean cheaper than where it stands: it keeps its top (never moved down for the meter)
            (-2_000, Some(-1_500), m(-1_000_000, 1_000), -1_501),
            // ... but a target cheaper than both still moves it down
            (-500, Some(-1_500), m(-1_000_000, 1_000), -500),
        ];
        for (i, (top, lower, f, want)) in cases.iter().enumerate() {
            assert_eq!(flow_cap(*top, *lower, *f), *want, "vector {}", i + 1);
        }
    }

    /// The properties the EVM's `test_netflow_meterOnlyWithTheSwitch` checks ON: a buy's mean between its before / after
    /// ticks; a sell never raises the mean (in ticks: never lowers it) and never adds weight; a round trip only pulls it
    /// cheaper.
    #[test]
    fn a_buy_sits_between_its_prices_and_a_sell_never_raises_the_mean() {
        let f1 = note(NetFlow::default(), true, 1_000_000, -5_000, -5_200);
        let m1 = f1.mean().unwrap();
        assert!((-5_200..=-5_000).contains(&m1));
        for pre in [-6_000, -5_100, -4_000] {
            let f2 = note(f1, false, 300_000, pre, pre + 50);
            assert!(f2.mean().unwrap() >= m1, "a sell from {pre} raised the mean");
            assert!(f2.b <= f1.b);
        }
        // a round trip at the top: buy then sell the same quote
        let top = note(f1, true, 500_000, -5_200, -5_300);
        let back = note(top, false, 500_000, -5_300, -5_200);
        assert!(back.mean().unwrap() >= m1, "a round trip raised the mean");
    }

    /// The weight never passes H, and a buy of H or more remembers only itself.
    #[test]
    fn the_meter_remembers_only_the_last_h() {
        let h = share_of_mcap(MCAP, H);
        let mut f = NetFlow::default();
        for i in 0..200 {
            f = note(f, true, h / 30, -10 * i, -10 * i - 10);
            assert!(f.b <= h);
        }
        assert_eq!(f.b, h);
        // the mean sits near the last H's ticks (the last ~30 buys: about -1,700 .. -2,000), not the whole history's
        let mean = f.mean().unwrap();
        assert!((-2_000..=-1_700).contains(&mean), "{mean}");
    }

    /// Never fails: a sequence that would leave `i128` empties the meter instead of reverting the swap.
    #[test]
    fn an_overflowing_step_empties_the_meter() {
        let f = NetFlow { a: i128::MAX - 10, b: 2 };
        assert_eq!(note_flow_with(f, true, 1, 400_000, 400_000, MCAP, H, 0), NetFlow::default());
        assert_eq!(mul_div_trunc(-7, 3, 2), Some(-10));
        assert_eq!(mul_div_trunc(7, 3, 2), Some(10));
        assert_eq!(mul_div_trunc(7, 3, 0), None);
        assert_eq!(share_of_mcap(u128::MAX, 10_000), u64::MAX);
    }

    fn climb_state() -> crate::FloorState {
        let mut s: crate::FloorState = bytemuck::Zeroable::zeroed();
        s.tick_spacing = 60;
        s.floor_tick = 60_000;
        s.seed_lower = -443_580;
        s.seed_upper = 58_200;
        s.seed_liq = crate::PodU128::new(2_000_000_000_000_000);
        s.ladder_tokens_remaining = 690_000_000_000_000;
        s.min_sellable = 1_000;
        s
    }

    /// ⛔ THE REACH SHIPS AT 954 with the dial at 130 (the EVM's pin too): with a cushion standing at 43,200 the hold's
    /// target is 43,200 − 1 − 954.
    #[test]
    fn the_climb_hold_reach_ships_at_954() {
        assert_eq!(STAIR_CLIMB_HOLD_REACH_TICKS, 954);
        assert_eq!(climb_hold_target(40_000, Some((43_200, 44_280, 1)), STAIR_CLIMB_HOLD_REACH_TICKS), 43_200 - 1 - 954);
    }

    /// THE REACH CLAMP (`_climbLine`'s `to`): the target is `max(spot, lower − 1 − reach)`, only with a cushion standing
    /// with liquidity, and reach 0 is the spot.
    #[test]
    fn the_reach_clamps_the_hold_target() {
        let c = Some((43_200, 44_280, 3u128));
        assert_eq!(climb_hold_target(40_000, c, 0), 40_000, "0 = the full hold");
        assert_eq!(climb_hold_target(40_000, c, 954), 43_200 - 1 - 954);
        assert_eq!(climb_hold_target(40_000, c, 100), 43_099);
        assert_eq!(climb_hold_target(40_000, c, 5_000), 40_000, "a reach past the spot holds for the whole climb");
        assert_eq!(climb_hold_target(40_000, None, 954), 40_000, "no cushion standing");
        assert_eq!(climb_hold_target(40_000, Some((43_200, 44_280, 0)), 954), 40_000, "a cushion with no liquidity");
        assert_eq!(climb_hold_target(i32::MIN + 5, Some((i32::MIN + 2, 0, 1)), 954), i32::MIN + 5, "no underflow");
    }

    /// THE REACH ON THE LINE: the full hold reserves for the climb to the spot's range; a reach of 954 ticks only for
    /// the next 954 ticks from where the cushion stands: the same quote over a CHEAPER range (the line never above the full
    /// hold's here). A reach covering the whole climb is the full hold.
    #[test]
    fn the_reach_softens_the_climb_line() {
        let s = climb_state();
        let tick = 40_000;
        let sqrt = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(tick).unwrap();
        let c = Some((43_200, 44_280, 3_000_000_000_000_000u128));
        let full = climb_line_parts_reach(sqrt, tick, &s, 1_000_000_000_000_000, 50_000_000_000, c, 1_080, 0).unwrap();
        assert_eq!(full.spot_range, Some((40_020, 41_100)));
        let reach = climb_line_parts_reach(sqrt, tick, &s, 1_000_000_000_000_000, 50_000_000_000, c, 1_080, 954).unwrap();
        assert_eq!(reach, climb_line_parts(sqrt, tick, &s, 1_000_000_000_000_000, 50_000_000_000, c, 1_080).unwrap(), "the shipped reach is 954");
        // to = 43,200 − 1 − 954 = 42,245 → the range [42,300, 43,380], within the reach of the standing cushion
        assert_eq!(reach.spot_range, Some((42_300, 43_380)));
        assert_eq!(reach.cushion_quote, full.cushion_quote, "the same quote, placed nearer");
        let (fl, rl) = (full.line.unwrap(), reach.line.unwrap());
        assert!(rl <= fl, "the reach holds no more than the full hold: {rl} vs {fl}");
        assert_ne!(reach.liquidity, full.liquidity, "the same quote over a different range");
        let whole = climb_line_parts_reach(sqrt, tick, &s, 1_000_000_000_000_000, 50_000_000_000, c, 1_080, 10_000).unwrap();
        assert_eq!(whole, full, "a reach past the spot is the full hold");
    }

    /// ⛔ THE CLIMB HOLD'S BRACKET: on random states, the hold's line from the
    /// solve clamped to the ratchet's bracket ([`climb_line_within`]) changes what the ratchet adopts exactly as the
    /// full-grid line ([`climb_line`]) does — `min(max(stand, line), floor0)` equal — its solve is the full solve clamped
    /// into the bracket, and the two lines are equal whenever the full one lands strictly inside `(stand, floor0)`. The
    /// bracket is the hook's own (`floor_hook::mech::netflow::climb_line`): `hi` the standing floor, `lo` the passes'.
    #[test]
    fn the_bracketed_climb_line_moves_the_ratchet_as_the_full_one_does() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |n: u64| -> u64 {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) % n
        };
        let sp = 60;
        let band = crate::band::band_ticks(sp).unwrap();
        let (mut held, mut inside, mut both) = (0u32, 0u32, 0u32);
        for _ in 0..4_000 {
            let mut s = climb_state();
            s.floor_tick = (20_000 + next(80_000) as i32) / sp * sp;
            s.seed_upper = s.floor_tick - band - sp * (1 + next(200) as i32);
            s.seed_liq = crate::PodU128::new(1 + next(5_000_000_000_000_000) as u128);
            s.ladder_tokens_remaining = next(900_000_000_000_000);
            let supply = 1_000_000_000_000_000u64;
            let tick = s.floor_tick - band - 1 - next(40_000) as i32;
            let sqrt = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(tick).unwrap();
            let lower = (tick + 1 + next(20_000) as i32) / sp * sp + sp;
            let width = 1_080;
            let c = Some((lower, lower + width, 1 + next(5_000_000_000_000_000) as u128));
            let vq = (1u64 << next(50)) + next(1 << 20); // log-uniform: the vault from a dust to ~1e15 raw
            let stand = crate::spacing::ceil_to_spacing(tick + 1 + band, sp).unwrap();
            let hi = s.floor_tick;
            let lo = crate::spacing::floor_to_spacing(stand.min(hi) - band - 2 * sp, sp).unwrap();
            let Ok(full) = climb_line_core(sqrt, tick, &s, supply, vq, c, width, STAIR_CLIMB_HOLD_REACH_TICKS, None) else { continue };
            let within = climb_line_core(sqrt, tick, &s, supply, vq, c, width, STAIR_CLIMB_HOLD_REACH_TICKS, Some((lo, hi))).unwrap();
            assert_eq!(
                within.line,
                climb_line_within(sqrt, tick, &s, supply, vq, c, width, lo, hi).unwrap(),
                "the public entry is the core"
            );
            assert_eq!(full.line.is_some(), within.line.is_some(), "a line in one is a line in the other");
            let (Some(fl), Some(wl)) = (full.line, within.line) else { continue };
            both += 1;
            if let (Some(fs), Some(ws)) = (full.solved, within.solved) {
                assert_eq!(ws, fs.clamp(lo, hi), "the bracketed solve is the full solve clamped");
            }
            let adopt = |l: i32| l.max(stand).min(hi);
            assert_eq!(adopt(wl), adopt(fl), "the ratchet adopts the same: full {fl} vs bracketed {wl}, stand {stand}, floor {hi}");
            if fl > stand && fl < hi {
                inside += 1;
                assert_eq!(wl, fl, "a line strictly inside the bracket is the full one");
            }
            held += (fl > stand) as u32;
        }
        assert!(both > 1_000 && held > 80 && inside > 50, "the states reach the bracket: {both} lines, {held} binding, {inside} inside");
    }
}
