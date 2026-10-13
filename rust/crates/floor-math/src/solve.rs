//! The floor solve: the highest floor (lowest tick) the vault can fully cover,
//! `cost(t) = quote_cost(sellable(t), t) ≤ q`, by bisection on the spacing grid.
//!
//! The EVM's `_solveFloorTickWith`, with Solana's tick bounds (at most 14 steps at spacing 60 against the EVM's 15).
//! ⛔ No lamport quantity inside the solve is narrowed to u64: `q` is u128 and every comparison is
//! `quote_cost(…) (u128) ≤ q`.
//!
//! The solve answers in DEAR-EDGE ticks. The caller publishes `band::publish(solved)` as `floor_tick`, and the
//! undefined-floor branch returns the CURRENT dear edge, not `floor_tick`, so that `publish(solve)` leaves an
//! undefined floor exactly where it is.

use carpenter_clmm_math::tick_math::{get_sqrt_price_at_tick, MAX_TICK, MIN_TICK};

use crate::band::dear_edge;
use crate::error::{FloorMathError, FloorMathResult};
use crate::position::{pol_base, quote_cost};
use crate::spacing::ceil_to_spacing;
use crate::state::FloorState;

/// Supply the vault does not hold: `supply − inventory_tokens − ladder_tokens_remaining −
/// wall_held_tokens`. Outside-LP tokens stay in it (⛔ never exclude them). Underflow is an `Err`, as the EVM
/// reverts.
///
/// ⛔ Held tokens are outside `S_circ` as well as listed ones: they are still the protocol's, idle in the pool's
/// claims. This is the ONE spelling on Solana (`floor_hook::mech::ratchet::circulating` calls it), as
/// `circulatingSupply()` is on the EVM.
pub fn circulating_supply(state: &FloorState, supply: u64) -> FloorMathResult<u64> {
    supply
        .checked_sub(state.inventory_tokens)
        .and_then(|c| c.checked_sub(state.ladder_tokens_remaining))
        .and_then(|c| c.checked_sub(state.wall_held_tokens))
        .ok_or(FloorMathError::SupplyUnderflow)
}

/// Tokens that could be sold into the bid with the market at `tick`, when `extra` more are known to be
/// off-market: `C − pol_base(tick) − extra`, floored at 0.
pub fn sellable_at(state: &FloorState, supply: u64, tick: i32, extra: u64) -> FloorMathResult<u64> {
    sellable_at_with(state, supply, tick, extra, &[])
}

/// A protocol position the solve must also count as locking what it WOULD hold at the tried tick, beside the
/// classic POL: `(lower, upper, liquidity)`. The staircase's CUSHION, a quote-only position under the price that buys
/// tokens as the price falls — at the floor it would hold them, so they are not circulating (`Floor = Q ÷ S_circ`:
/// circulating supply excludes every token a protocol position would hold at the floor).
pub type MorePosition = (i32, i32, u128);

/// [`sellable_at`] with `more` protocol positions locking their base at `tick` exactly as the POL does.
pub fn sellable_at_with(state: &FloorState, supply: u64, tick: i32, extra: u64, more: &[MorePosition]) -> FloorMathResult<u64> {
    let c = circulating_supply(state, supply)?;
    let mut locked = pol_base(tick, state.seed_lower, state.seed_upper, state.seed_liq.get())? as u128
        + extra as u128;
    for &(lower, upper, liq) in more {
        locked += pol_base(tick, lower, upper, liq)? as u128;
    }
    Ok(if locked >= c as u128 { 0 } else { c - locked as u64 })
}

fn cost_at(state: &FloorState, supply: u64, tick: i32, extra: u64, more: &[MorePosition]) -> FloorMathResult<u128> {
    quote_cost(sellable_at_with(state, supply, tick, extra, more)?, tick)
}

/// The solve's per-step work with everything that does not move between steps read ONCE (the solve is a large share of
/// a ratchet's compute, each step otherwise re-reading the classic POL's
/// and the cushion's edges and the tried tick's sqrt price three times). The SAME arithmetic as [`sellable_at_with`] and
/// [`cost_at`] — [`crate::position::pol_base_at_sqrt`] and [`crate::position::quote_cost_at_sqrt`] are the bodies
/// `pol_base` / `quote_cost` call — so every answer is identical (`tests::the_fast_step_is_the_step`).
#[derive(Clone)]
pub(crate) struct Steps {
    pub(crate) circulating: u64,
    pub(crate) extra: u64,
    /// The classic POL's and each `more` position's `(sqrt lower, sqrt upper, liquidity)`; a zero-liquidity one is left out.
    pub(crate) locked: [Option<(u128, u128, u128)>; 3],
}

impl Steps {
    pub(crate) fn of(state: &FloorState, supply: u64, extra: u64, more: &[MorePosition]) -> FloorMathResult<Option<Self>> {
        if more.len() > 2 {
            return Ok(None); // the general path: [`cost_at`]
        }
        let circulating = circulating_supply(state, supply)?; // first, as `sellable_at_with` reads it
        let mut locked = [None; 3];
        locked[0] = crate::position::edges(state.seed_lower, state.seed_upper, state.seed_liq.get())?;
        for (i, &(lower, upper, liq)) in more.iter().enumerate() {
            locked[i + 1] = crate::position::edges(lower, upper, liq)?;
        }
        Ok(Some(Steps { circulating, extra, locked }))
    }

    pub(crate) fn sellable(&self, p: u128) -> FloorMathResult<u64> {
        let mut locked = self.extra as u128;
        for &(a, b, liq) in self.locked.iter().flatten() {
            locked += crate::position::pol_base_at_sqrt(p, a, b, liq)? as u128;
        }
        let c = self.circulating;
        Ok(if locked >= c as u128 { 0 } else { c - locked as u64 })
    }

    fn sellable_at(&self, tick: i32) -> FloorMathResult<u64> {
        self.sellable(get_sqrt_price_at_tick(tick)?)
    }

    /// The grid tick the bracketed solve's answer should be, from the closed form (see
    /// `within_guided`), clamped into `[lo, hi]`; `None` when there is nothing to aim with. A GUESS: the probes decide.
    /// ⚠ COMPUTE: f64 is soft-float on the program target, so every position is converted once, each stretch is tested by
    /// its closed-form cost at its cheap end (no root), and the one square root is taken in the stretch that holds it.
    fn aim(&self, q: u128, lo: i32, hi: i32, s: i32) -> Option<i32> {
        if q == 0 {
            return None;
        }
        const Q64F: f64 = 18_446_744_073_709_551_616.0;
        let sp_lo = get_sqrt_price_at_tick(lo).ok()?;
        let sp_hi = get_sqrt_price_at_tick(hi).ok()?;
        let (p_lo, p_hi) = (sp_lo as f64 / Q64F, sp_hi as f64 / Q64F);
        let c0 = self.circulating as f64 - self.extra as f64;
        let qf = q as f64;
        // the positions once, and the stretches' edges inside (p_lo, p_hi), in order (at most two positions)
        let mut pos = [(0.0f64, 0.0f64, 0.0f64); 3];
        let mut np = 0;
        let mut cuts = [p_hi; 6];
        let mut n = 0;
        for &(a, b, l) in self.locked.iter().flatten() {
            let (af, bf) = (a as f64 / Q64F, b as f64 / Q64F);
            pos[np] = (af, bf, l as f64);
            np += 1;
            for e in [af, bf] {
                if e > p_lo && e < p_hi {
                    cuts[n] = e;
                    n += 1;
                }
            }
        }
        for i in 1..n {
            let mut j = i;
            while j > 0 && cuts[j - 1] > cuts[j] {
                cuts.swap(j - 1, j);
                j -= 1;
            }
        }
        let mut from = p_lo;
        for k in 0..=n {
            let to = if k < n { cuts[k] } else { p_hi };
            let mid = 0.5 * (from + to);
            // sellable = A − B·P over this stretch
            let (mut a_, mut b_) = (c0, 0.0f64);
            for &(af, bf, lf) in &pos[..np] {
                if mid >= bf {
                    a_ -= lf * (bf - af);
                } else if mid > af {
                    a_ += lf * af;
                    b_ += lf;
                }
            }
            // cost falls as P rises: the root is in this stretch iff the cost at its cheap end `to` is at most q
            let sell_to = a_ - b_ * to;
            if k < n && sell_to > qf * to * to {
                from = to;
                continue;
            }
            if k == n && sell_to > qf * to * to {
                return Some(hi); // unaffordable at the bracket's cheap end: the answer is `hi`
            }
            if a_ <= 0.0 {
                return None;
            }
            // q·P² + B·P − A = 0, its positive root in the stable form 2A / (B + √(B² + 4qA))
            let root = 2.0 * a_ / (b_ + sqrt_f64(b_ * b_ + 4.0 * qf * a_));
            let sp = root * Q64F;
            let t = if sp <= sp_lo as f64 {
                lo
            } else if sp >= sp_hi as f64 {
                hi
            } else {
                carpenter_clmm_math::tick_math::get_tick_at_sqrt_price(sp as u128).ok()? + 1
            };
            return ceil_to_spacing(t, s).ok().map(|g| g.clamp(lo, hi));
        }
        None
    }

    fn cost_at(&self, tick: i32) -> FloorMathResult<u128> {
        let p = get_sqrt_price_at_tick(tick)?;
        crate::position::quote_cost_at_sqrt(self.sellable(p)?, p)
    }

}

/// The EVM's `_solveFloorTickWith(q, extraExcluded)`, with `supply` the base mint's supply
/// (`args.supply1` in a callback, `Mint.supply` in a hook-initiated instruction).
///
/// The answer is a DEAR-EDGE tick; the caller publishes it with [`crate::band::publish`].
/// 1. `hi = ceil_to_spacing(MAX_TICK)` (cheapest), `lo = ceil_to_spacing(MIN_TICK)` (dearest).
/// 2. `sellable(lo) < min_sellable` → `dear_edge(floor_tick)` (the floor is undefined: leave it where it
///    is, as the dear edge it publishes from).
/// 3. `cost(hi) > q` → `hi` (nothing affordable; at Solana bounds only q = 0 against a supply near u64::MAX).
/// 4. While `hi − lo > spacing`: `mid = ceil_to_spacing(lo + (hi − lo)/2)`, nudged strictly inside
///    `(lo, hi)`; `cost(mid) ≤ q` → `hi = mid`, else `lo = mid`. Return `hi`.
pub fn solve_floor_tick_with(
    q: u128,
    extra: u64,
    state: &FloorState,
    supply: u64,
) -> FloorMathResult<i32> {
    solve_counting(q, extra, state, supply).map(|(t, _)| t)
}

/// [`solve_floor_tick_with`] counting `more` protocol positions' base at each tried tick (the staircase's cushion).
/// `more` empty is [`solve_floor_tick_with`] exactly.
pub fn solve_floor_tick_with_more(
    q: u128,
    extra: u64,
    state: &FloorState,
    supply: u64,
    more: &[MorePosition],
) -> FloorMathResult<i32> {
    solve_counting_with(q, extra, state, supply, more).map(|(t, _)| t)
}

/// [`solve_floor_tick_with`] plus the number of bisection steps taken (for tests and CU notes).
pub fn solve_counting(
    q: u128,
    extra: u64,
    state: &FloorState,
    supply: u64,
) -> FloorMathResult<(i32, u32)> {
    solve_counting_with(q, extra, state, supply, &[])
}

fn solve_counting_with(
    q: u128,
    extra: u64,
    state: &FloorState,
    supply: u64,
    more: &[MorePosition],
) -> FloorMathResult<(i32, u32)> {
    let s = state.tick_spacing;
    let mut hi = ceil_to_spacing(MAX_TICK, s)?; // :946
    let mut lo = ceil_to_spacing(MIN_TICK, s)?; // :947
    let Some(steps) = Steps::of(state, supply, extra, more)? else {
        return solve_counting_general(q, extra, state, supply, more, lo, hi);
    };
    if steps.sellable_at(lo)? < state.min_sellable {
        return Ok((dear_edge(state.floor_tick, s)?, 0)); // :951
    }
    if steps.cost_at(hi)? > q {
        return Ok((hi, 0)); // :952
    }
    let mut n = 0u32;
    while hi - lo > s {
        let mut mid = ceil_to_spacing(lo + (hi - lo) / 2, s)?;
        if mid >= hi {
            mid = hi - s;
        }
        if mid <= lo {
            mid = lo + s;
        }
        if steps.cost_at(mid)? <= q {
            hi = mid;
        } else {
            lo = mid;
        }
        n += 1;
    }
    Ok((hi, n))
}

/// THE SOLVE WITHIN A BRACKET: [`solve_floor_tick_with_more`] CLAMPED to `[lo, hi]`
/// (grid ticks, `lo < hi`), found by bisecting only inside them. `cost(t)` is monotone — falling as `t` rises: the
/// sellable supply shrinks as the POL and the cushion lock more and each token costs less — so the full solve's answer
/// `T` is the one grid tick where it crosses `q`, and `clamp(T, lo, hi)` is: `hi` when even `hi` is unaffordable, `lo`
/// when `lo` already is, else `T` itself, by the same bisection. The undefined floor (nothing sellable at the grid's
/// dearest tick) answers the dear edge, as the full solve does. A caller that adopts only answers inside `[lo, hi]` —
/// the ratchet's passes: the stand line below, the standing floor above — ends exactly where the full solve would have
/// left it (`tests::the_bracket_is_the_clamp`), in fewer steps (the ratchet's compute profile: the solve was 40 % of a
/// ratchet, ~15 steps over the whole grid).
pub fn solve_floor_tick_within(
    q: u128,
    state: &FloorState,
    supply: u64,
    more: &[MorePosition],
    lo: i32,
    hi: i32,
) -> FloorMathResult<i32> {
    let s = state.tick_spacing;
    let grid_lo = ceil_to_spacing(MIN_TICK, s)?;
    let grid_hi = ceil_to_spacing(MAX_TICK, s)?;
    // the clamp of the full solve only exists inside its own range: a bracket past it is the full solve
    let (lo, hi) = (lo.max(grid_lo), hi.min(grid_hi));
    if lo >= hi || lo % s != 0 || hi % s != 0 {
        return Err(FloorMathError::InvalidRange); // the ratchet's bracket is always grid ticks, the stand line under the floor
    }
    let Some(steps) = Steps::of(state, supply, 0, more)? else {
        // more than two positions (no caller): the full solve, clamped — but the undefined floor as it answers it
        if sellable_at_with(state, supply, grid_lo, 0, more)? < state.min_sellable {
            return dear_edge(state.floor_tick, s);
        }
        return solve_floor_tick_with_more(q, 0, state, supply, more).map(|t| t.clamp(lo, hi));
    };
    if steps.sellable_at(grid_lo)? < state.min_sellable {
        return dear_edge(state.floor_tick, s); // :951, unclamped as the full solve answers it
    }
    let probes = core::cell::Cell::new(0u32);
    within_guided(q, lo, hi, s, grid_lo, &steps, &probes)
}

/// [`solve_floor_tick_within`] returning how many cost evaluations it made too (the M1 replay; never a program path).
#[doc(hidden)]
pub fn solve_floor_tick_within_probes(
    q: u128,
    state: &FloorState,
    supply: u64,
    more: &[MorePosition],
    lo: i32,
    hi: i32,
) -> FloorMathResult<(i32, u32)> {
    let s = state.tick_spacing;
    let grid_lo = ceil_to_spacing(MIN_TICK, s)?;
    let grid_hi = ceil_to_spacing(MAX_TICK, s)?;
    let (lo, hi) = (lo.max(grid_lo), hi.min(grid_hi));
    if lo >= hi || lo % s != 0 || hi % s != 0 {
        return Err(FloorMathError::InvalidRange);
    }
    let steps = Steps::of(state, supply, 0, more)?.ok_or(FloorMathError::InvalidRange)?;
    if steps.sellable_at(grid_lo)? < state.min_sellable {
        return Ok((dear_edge(state.floor_tick, s)?, 0));
    }
    let probes = core::cell::Cell::new(0u32);
    let t = within_guided(q, lo, hi, s, grid_lo, &steps, &probes)?;
    Ok((t, probes.get()))
}

/// THE CLOSED-FORM SOLVE: the bracketed solve's search with its first two probes AIMED.
/// Between two edges of the protocol positions the sellable supply is a straight line in the sqrt price P (each
/// position's base is `L·(P − a)`, nothing, or all of it), so there cost(P) = (A − B·P) / P² and cost = q is a quadratic
/// with one positive root. [`Steps::aim`] solves it stretch by stretch and keeps the root that lies in its own stretch;
/// the probe at that grid tick and the one a spacing beside it settle the answer when the aim is right, and the plain
/// bisection takes over inside what they left when it is not. Every probe keeps cost(lo) > q ≥ cost(hi) — an unprobed
/// `hi` is the answer when nothing under it is affordable (`clamp(T, lo, hi)`), and an unprobed `lo` is probed before it
/// is answered — so the answer is the deployed bisection's to the tick.
fn within_guided(q: u128, lo: i32, hi: i32, s: i32, grid_lo: i32, steps: &Steps, probes: &core::cell::Cell<u32>) -> FloorMathResult<i32> {
    let aim = steps.aim(q, lo, hi, s);
    within_aimed(q, lo, hi, s, grid_lo, aim, |t| {
        probes.set(probes.get() + 1);
        steps.cost_at(t)
    })
}

/// The aimed search on any monotone `cost` (falling as the tick rises) and ANY `aim`: the deployed bisection's answer
/// whatever the aim (`tests::m1_any_aim_is_the_bisection`).
pub(crate) fn within_aimed(
    q: u128,
    mut lo: i32,
    mut hi: i32,
    s: i32,
    grid_lo: i32,
    aim: Option<i32>,
    mut cost: impl FnMut(i32) -> FloorMathResult<u128>,
) -> FloorMathResult<i32> {
    // `lo` is known never to be the answer: the full solve's own `lo`, or a probed unaffordable tick
    let mut lo_out = lo == grid_lo;
    if let Some(g) = aim.map(|g| ceil_to_spacing(g, s)).transpose()?.map(|g| g.clamp(lo, hi)) {
        // the aimed probe: at `hi` an unaffordable answer is `hi` itself; at `lo` an affordable one is `lo`
        let down = if g >= hi {
            if cost(hi)? > q {
                return Ok(hi); // T > hi
            }
            true
        } else if g <= lo {
            if lo_out {
                false
            } else if cost(lo)? <= q {
                return Ok(lo); // T <= lo
            } else {
                lo_out = true;
                false
            }
        } else if cost(g)? <= q {
            hi = g;
            true
        } else {
            lo = g;
            lo_out = true;
            false
        };
        // THE GALLOP: from the aimed tick toward the answer, one spacing, then two, four … until a probe crosses it
        let mut step = s;
        // ⚠ at most GALLOP probes: a far-off aim costs no more than that over the bisection that finishes the bracket
        for _ in 0..GALLOP {
            let t = if down { hi - step } else { lo + step };
            if t <= lo || t >= hi {
                break;
            }
            if cost(t)? <= q {
                hi = t;
                if !down {
                    break;
                }
            } else {
                lo = t;
                lo_out = true;
                if down {
                    break;
                }
            }
            step = step.saturating_mul(2);
        }
    }
    while hi - lo > s {
        let mut mid = ceil_to_spacing(lo + (hi - lo) / 2, s)?;
        if mid >= hi {
            mid = hi - s;
        }
        if mid <= lo {
            mid = lo + s;
        }
        if cost(mid)? <= q {
            hi = mid;
        } else {
            lo = mid;
            lo_out = true;
        }
    }
    if !lo_out && cost(lo)? <= q {
        return Ok(lo); // T <= lo
    }
    Ok(hi)
}

/// The aimed searches' gallop: at most this many probes from the aim before the bisection takes the rest.
pub(crate) const GALLOP: u32 = 3;

/// √x by Newton from the exponent's halving (no libm on the program target): a guess's precision, not an answer's.
pub(crate) fn sqrt_f64(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut r = f64::from_bits((x.to_bits() >> 1) + (0x3ff_u64 << 51)); // ≈ √x
    for _ in 0..5 {
        r = 0.5 * (r + x / r);
    }
    r
}

/// [`solve_counting_with`] the original way, every step through [`cost_at`]: for more than two `more` positions (no
/// caller passes any), and the reference `tests::the_fast_step_is_the_step` checks the fast path against.
fn solve_counting_general(
    q: u128,
    extra: u64,
    state: &FloorState,
    supply: u64,
    more: &[MorePosition],
    mut lo: i32,
    mut hi: i32,
) -> FloorMathResult<(i32, u32)> {
    let s = state.tick_spacing;
    if sellable_at_with(state, supply, lo, extra, more)? < state.min_sellable {
        return Ok((dear_edge(state.floor_tick, s)?, 0)); // :951
    }
    if cost_at(state, supply, hi, extra, more)? > q {
        return Ok((hi, 0)); // :952
    }
    let mut steps = 0u32;
    while hi - lo > s {
        // :954
        let mut mid = ceil_to_spacing(lo + (hi - lo) / 2, s)?; // :955
        if mid >= hi {
            mid = hi - s; // :956
        }
        if mid <= lo {
            mid = lo + s; // :957
        }
        if cost_at(state, supply, mid, extra, more)? <= q {
            hi = mid; // :958
        } else {
            lo = mid; // :959
        }
        steps += 1;
    }
    Ok((hi, steps)) // :961
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::band::publish;
    use crate::constants::{LADDER, MIN_SELLABLE, POL, SUPPLY, TICKS_PER_DOUBLING};
    use crate::modes::tranche_price_x64;
    use crate::position::position_liquidity;
    use crate::price::open_tick;
    use crate::spacing::floor_to_spacing;
    use carpenter_clmm_math::tick_math::get_sqrt_price_at_tick;

    fn state() -> FloorState {
        let mut s = FloorState::default();
        s.tick_spacing = 60;
        s.floor_tick = ceil_to_spacing(MAX_TICK, 60).unwrap();
        s.min_sellable = MIN_SELLABLE;
        s
    }

    /// A low-cap open at SOL = $100: `open_tick(tranche_price_x64(5_000, 10_000, 9))` = 99,039. A realistic
    /// open — the 2^77 tranche cap puts every reachable open above ≈ −90,120.
    ///
    /// ⛔ $5,000 is an example Custom cap: `tranche_price_x64` takes a raw (mcap, quote price, quote decimals), so every
    /// measured literal downstream of this open — 99,039, the seed edges, SELLABLE_ABOVE, LOCKED_ABOVE — is tied to it.
    /// Re-anchoring on another cap would move the open and force the measured constants to be re-measured.
    fn low_cap_open() -> i32 {
        let t = open_tick(tranche_price_x64(5_000, 100 * 100, 9).unwrap()).unwrap();
        assert_eq!(t, 99_039);
        t
    }

    /// A graduated-shaped pool, seeded as the EVM's `FloorLaunch` seeds it: the POL is the FULL-RANGE TAIL
    /// ("POL → starting price → infinity"), token-only over `[ceil_to_spacing(MIN_TICK, 60),
    /// floor_to_spacing(open, 60)]` = `[−443,580, open]`, at the open's sqrt price — which is at or above the range's
    /// UPPER edge, so the position is all base and `seed_liq > 0`. ⛔ `ceil_to_spacing`, not `floor_to_spacing`, for the
    /// lower edge: FloorMath's floor clamps `MIN_TICK` to `MIN_TICK + spacing` (−443,520); ceil gives the bottom of the
    /// usable grid.
    /// ⛔ Seeding at the LOWER edge makes it all quote, `L = 0`, and `pol_base ≡ 0` in every solve — the term
    /// `C − pol_base(t) − extra` is then never exercised. Ladder remaining, some inventory.
    /// ⚠ The full-range tail's cost, as arithmetic: for a token-only position `L = tokens / (√pb − √pa)`, and moving
    /// `pa` from one doubling below the open to MIN_TICK takes `√pa/√pb` from `2^(−1/2)` to ≈ 0, so `L` scales by
    /// `1 − 2^(−1/2)` ≈ 0.2929 = 1/(2+√2) — the tail is ≈ 3.414× THINNER near the open than a bounded one-doubling band.
    /// Asserted as a band in
    fn graduated(open: i32) -> FloorState {
        let mut s = state();
        s.seed_lower = ceil_to_spacing(MIN_TICK, 60).unwrap();
        s.seed_upper = floor_to_spacing(open, 60).unwrap();
        let p = get_sqrt_price_at_tick(open).unwrap();
        assert!(p >= get_sqrt_price_at_tick(s.seed_upper).unwrap());
        s.seed_liq.set(position_liquidity(p, s.seed_lower, s.seed_upper, 0, POL).unwrap());
        assert!(s.seed_liq.get() > 0, "the POL fixture must lock supply");
        s.ladder_tokens_remaining = LADDER;
        s.inventory_tokens = 5_000_000_000_000;
        s
    }

    // `sellable(t) = C − pol_base(t) − extra`: above the seed range the POL is all base and locks
    // ≈ POL of the supply; at its bottom edge (the grid's bottom, −443,580), none. At the $5,000 open the full-range
    // tail locks LOCKED_ABOVE of the POL seeded (the seed's rounding leaves the rest unlocked), so `above` =
    // SELLABLE_ABOVE against C (395e12 with the 30 / 10 / 60 split). `extra` locks on top.
    #[test]
    fn fixture_pol_locks_supply_above_its_range() {
        let s = graduated(low_cap_open());
        assert_eq!((s.seed_lower, s.seed_upper), (-443_580, 99_000));
        let c = circulating_supply(&s, SUPPLY).unwrap();
        assert_eq!(c, 395_000_000_000_000);
        let above = sellable_at(&s, SUPPLY, s.seed_upper + 60, 0).unwrap();
        let at_bottom = sellable_at(&s, SUPPLY, s.seed_lower, 0).unwrap();
        assert!(above < at_bottom, "{above} !< {at_bottom}");
        assert_eq!(at_bottom, c, "at the tail's bottom edge the POL is all quote: nothing locked");
        assert_eq!(above, SELLABLE_ABOVE);
        assert_eq!(c - above, LOCKED_ABOVE, "the tail locks POL minus the seed's rounding");
        assert!(POL - LOCKED_ABOVE < 1_000, "the rounding loss is dust: 114 units on a 100e12 seed");
        assert_eq!(sellable_at(&s, SUPPLY, s.seed_upper + 60, 1_000).unwrap(), above - 1_000);
        // and the lock is monotone in the tick across the range, as the bisection relies on
        let mut last = u64::MAX;
        for t in (s.seed_lower..=s.seed_upper + 60).step_by(6000) {
            let v = sellable_at(&s, SUPPLY, t, 0).unwrap();
            assert!(v <= last, "tick {t}: {v} > {last}");
            last = v;
        }
    }

    /// The two measured literals of the full-range fixture (`cargo test -p floor-math`): the tail's liquidity is
    /// spread over 542,640 ticks, so `position_liquidity`'s floor loses units of the POL; with the 30 / 10 / 60 split
    /// (a 100e12 POL and a 600e12 ladder, so C = 395e12) the tail loses 114 units.
    const SELLABLE_ABOVE: u64 = 295_000_000_000_114;
    const LOCKED_ABOVE: u64 = 99_999_999_999_886;

    /// The solve against the shape graduation actually seeds: monotone across the REACHABLE grid
    /// (there is no tick below `seed_lower` — it IS `lo`), the whole supply never sellable, and the
    /// bisection still inside the 14-step bound with a non-zero `seed_liq` at `lo`.
    #[test]
    fn the_full_range_pol_locks_at_every_reachable_tick_and_the_solve_still_bisects() {
        let open = low_cap_open();
        let s = graduated(open);
        let lo = ceil_to_spacing(MIN_TICK, 60).unwrap();
        assert_eq!((s.seed_lower, s.seed_upper), (lo, 99_000));
        assert_eq!(lo, -443_580, "the bottom of the usable grid at spacing 60");

        // ⛔ The regime: `lo` IS `seed_lower`, so there is no tick below the range and `pol_base`'s
        // `p < a` clamp cannot be reached. At `lo` itself the position is all quote, so the lock is
        // 0 and the whole supply is sellable — the same read the bounded fixture gets a spacing
        // BELOW its range, which is why the solve's step-2 guard behaves identically either way.
        let c = circulating_supply(&s, SUPPLY).unwrap();
        assert_eq!(pol_base(lo, s.seed_lower, s.seed_upper, s.seed_liq.get()).unwrap(), 0);
        assert_eq!(sellable_at(&s, SUPPLY, lo, 0).unwrap(), c);
        // …and the lock is material only near the UPPER edge — the `T/√m` profile the full range buys
        // the tail with. At the open the POL locks what the bounded band locks there, less the
        // thinner liquidity (asserted as the ratio below).
        let at_upper = sellable_at(&s, SUPPLY, s.seed_upper + 60, 0).unwrap();
        assert!(at_upper < c, "the POL locks above its range ({at_upper} !< {c})");

        // The full-range tail's cost, as arithmetic: 1 − 2^(−1/2) ≈ 0.2929 of the bounded band's
        // liquidity for the same POL tokens. Asserted as a band, not a literal, so a spacing or
        // open change does not re-record it.
        let bounded_liq = {
            // the bounded one-doubling band, rebuilt inline for comparison.
            let mut b = state();
            b.seed_lower = floor_to_spacing(open - TICKS_PER_DOUBLING, 60).unwrap();
            b.seed_upper = floor_to_spacing(open, 60).unwrap();
            let p = get_sqrt_price_at_tick(open).unwrap();
            position_liquidity(p, b.seed_lower, b.seed_upper, 0, POL).unwrap()
        };
        let full_liq = s.seed_liq.get();
        assert!(full_liq * 100 < bounded_liq * 30 && full_liq * 100 > bounded_liq * 29, "{full_liq} vs {bounded_liq}");

        // Monotone in the tick across the reachable grid, which is what the bisection assumes.
        // Stepped coarsely: the grid is 14,786 spacings wide.
        let mut last = u64::MAX;
        for t in (lo..=s.seed_upper + 60).step_by(60 * 97) {
            let v = sellable_at(&s, SUPPLY, t, 0).unwrap();
            assert!(v <= last, "tick {t}: {v} > {last}");
            last = v;
        }

        // The 14-step bound still holds with a live `pol_base` at every probe, and `cost_at` at `lo` still
        // exceeds u64 (the overflow arm, now with a non-zero `seed_liq` under it).
        assert!(cost_at(&s, SUPPLY, lo, 0, &[]).unwrap() > u64::MAX as u128);
        for q in [1u128, 1_000, 27_500_000_000, 10u128.pow(20)] {
            let (t, steps) = solve_counting(q, 0, &s, SUPPLY).unwrap();
            assert!(steps <= 14, "step bound: {steps} steps at q={q}");
            assert_eq!(t, brute(q, 0, &s, SUPPLY), "q={q}");
        }
    }

    // The solve from lo with supply 1e15 and q = 1 SOL does not overflow. At lo the
    // whole supply costs ≈ 2^114 lamports, which a u64 cost would have wrapped or panicked on.
    #[test]
    fn solve_from_lo_with_full_supply_and_one_sol_does_not_overflow() {
        let s = state();
        let lo = ceil_to_spacing(MIN_TICK, 60).unwrap();
        assert!(cost_at(&s, SUPPLY, lo, 0, &[]).unwrap() > u64::MAX as u128);
        let (t, steps) = solve_counting(1_000_000_000, 0, &s, SUPPLY).unwrap();
        assert!(steps <= 14, "at most 14 steps at spacing 60, took {steps}");
        assert!(quote_cost(SUPPLY, t).unwrap() <= 1_000_000_000);
        assert!(quote_cost(SUPPLY, t - 60).unwrap() > 1_000_000_000);
        for q in [0u128, 1, u64::MAX as u128, u128::MAX] {
            solve_floor_tick_with(q, 0, &s, SUPPLY).unwrap();
        }
    }

    /// Linear scan of the grid for the smallest tick with cost ≤ q (cost is monotone in tick).
    fn brute(q: u128, extra: u64, s: &FloorState, supply: u64) -> i32 {
        let hi = ceil_to_spacing(MAX_TICK, s.tick_spacing).unwrap();
        let lo = ceil_to_spacing(MIN_TICK, s.tick_spacing).unwrap();
        if sellable_at(s, supply, lo, extra).unwrap() < s.min_sellable {
            return s.floor_tick;
        }
        let mut t = hi;
        while t - s.tick_spacing > lo && cost_at(s, supply, t - s.tick_spacing, extra, &[]).unwrap() <= q {
            t -= s.tick_spacing;
        }
        t
    }

    // The solve is a bisection for the boundary; checked against a scan on a graduated pool at the
    // $5,000 open, where the POL locks ≈ 150e12 of the supply above its range and none below it, so the
    // scan crosses the range with `sellable` changing under it. The (q, extra) pairs straddle the raise:
    // the whole sellable supply at the open costs ≈ 27.25 SOL.
    #[test]
    fn bisection_matches_a_grid_scan() {
        let s = graduated(low_cap_open());
        for q in [1u128, 1_000, 50_000_000, 27_500_000_000, 1_000_000_000_000, 10u128.pow(20)] {
            for extra in [0u64, 1_000_000_000_000] {
                assert_eq!(
                    solve_floor_tick_with(q, extra, &s, SUPPLY).unwrap(),
                    brute(q, extra, &s, SUPPLY),
                    "q={q} extra={extra}"
                );
            }
        }
    }

    // nothing sellable even at the dearest price -> the CURRENT dear edge (floor_tick − 120 at
    // spacing 60), so that `publish(solve)` republishes floor_tick unchanged.
    #[test]
    fn undefined_floor_returns_the_dear_edge() {
        let mut s = state();
        s.floor_tick = -12_000;
        s.inventory_tokens = SUPPLY - (MIN_SELLABLE - 1);
        assert_eq!(solve_counting(u128::MAX, 0, &s, SUPPLY).unwrap(), (-12_120, 0));
        assert_eq!(publish(-12_120, 60).unwrap(), -12_000);
        let mut t = state();
        t.floor_tick = -6_000;
        assert_eq!(solve_counting(u128::MAX, SUPPLY - 5, &t, SUPPLY).unwrap(), (-6_120, 0));
        assert_eq!(publish(-6_120, 60).unwrap(), -6_000);
        // spacing 200: the band is 200 wide
        let mut u = state();
        u.tick_spacing = 200;
        u.floor_tick = -6_000;
        u.inventory_tokens = SUPPLY;
        assert_eq!(solve_counting(u128::MAX, 0, &u, SUPPLY).unwrap(), (-6_200, 0));
    }

    // what the ratchet publishes is `publish(solve)`, and publishing the dear edge of
    // a floor that is on the grid gives that floor back (`publish(dear_edge(f)) == f`). On a defined
    // floor the published tick is one band above the solve; on an undefined one it is `floor_tick`.
    #[test]
    fn publish_of_the_solve_round_trips() {
        let s = graduated(low_cap_open());
        for q in [1_000u128, 50_000_000, 27_500_000_000, 1_000_000_000_000] {
            let solved = solve_floor_tick_with(q, 0, &s, SUPPLY).unwrap();
            let published = publish(solved, 60).unwrap();
            assert_eq!(published, solved + 120, "q={q}: the published floor is one band above the solve");
            assert_eq!(dear_edge(published, 60).unwrap(), solved);
            assert_eq!(publish(dear_edge(published, 60).unwrap(), 60).unwrap(), published);
        }
        // undefined: publish(solve) == floor_tick, for every grid floor
        let mut u = state();
        u.inventory_tokens = SUPPLY;
        for f in [-12_000, 0, 6_000, 443_520] {
            u.floor_tick = f;
            let solved = solve_floor_tick_with(u128::MAX, 0, &u, SUPPLY).unwrap();
            assert_eq!(publish(solved, 60).unwrap(), f, "floor {f}");
        }
    }

    // cost at the cheapest tick above q -> hi. At Solana's bounds that needs an absurd supply:
    // at hi = 443,520 one lamport buys ≈ 1.9e19 base units, so the real SUPPLY (1e15) costs 0 there
    // and every q passes; only q = 0 against a supply near u64::MAX reaches the branch.
    #[test]
    fn nothing_affordable_returns_hi() {
        let s = state();
        let hi = ceil_to_spacing(MAX_TICK, 60).unwrap();
        assert_eq!(hi, 443_520);
        assert_eq!(cost_at(&s, SUPPLY, hi, 0, &[]).unwrap(), 0);
        assert_eq!(cost_at(&s, u64::MAX, hi, 0, &[]).unwrap(), 1);
        assert_eq!(solve_counting(0, 0, &s, u64::MAX).unwrap(), (hi, 0));
        let (t, steps) = solve_counting(0, 0, &s, SUPPLY).unwrap();
        assert!(steps > 0 && t < hi);
    }

    // More backing -> a floor no lower (a tick no higher): the solve is monotone in q.
    #[test]
    fn more_backing_never_lowers_the_floor() {
        let s = graduated(low_cap_open());
        let mut last = i32::MAX;
        for e in 0..40 {
            let q = 1u128 << e;
            let t = solve_floor_tick_with(q, 0, &s, SUPPLY).unwrap();
            assert!(t <= last, "q=2^{e}: {t} > {last}");
            last = t;
        }
    }

    // The step count is path-dependent, between floor and ceil of log2 of the grid count; at spacing 60 that is 13 to
    // 14, never the EVM's 15.
    #[test]
    fn step_counts_by_spacing() {
        for (spacing, min, max) in [(1, 19, 20), (10, 16, 17), (60, 13, 14), (200, 12, 13)] {
            let mut s = graduated(low_cap_open());
            s.tick_spacing = spacing;
            let mut seen = (u32::MAX, 0);
            for e in 0..64 {
                let (_, n) = solve_counting(1u128 << e, 0, &s, SUPPLY).unwrap();
                if n == 0 {
                    continue;
                }
                seen = (seen.0.min(n), seen.1.max(n));
            }
            assert!(seen.0 >= min && seen.1 <= max, "spacing {spacing}: {seen:?}");
        }
    }

    #[test]
    fn supply_underflow_is_an_error() {
        let mut s = state();
        s.inventory_tokens = 10;
        s.ladder_tokens_remaining = 10;
        assert_eq!(solve_floor_tick_with(1, 0, &s, 19), Err(FloorMathError::SupplyUnderflow));
        assert_eq!(circulating_supply(&s, 20).unwrap(), 0);
    }

    /// Held tokens (`wall_held_tokens` — on a staircase pool every staircase token still in the claims,
    /// `FloorState::is_stair`) are outside circulating supply, so moving tokens between standing and held never moves
    /// `C` (or the solve). Mutations this kills: held not subtracted; held subtracted twice; the subtraction unchecked
    /// (wraps past supply).
    #[test]
    fn held_tokens_are_outside_circulating_supply() {
        let mut s = FloorState::default();
        s.inventory_tokens = 10;
        s.ladder_tokens_remaining = 600;
        let all_listed = circulating_supply(&s, 1_000).unwrap();
        assert_eq!(all_listed, 390);
        s.ladder_tokens_remaining = 450;
        s.wall_held_tokens = 150;
        assert_eq!(circulating_supply(&s, 1_000).unwrap(), all_listed, "standing + held is the whole deposit");
        s.wall_held_tokens = 151;
        assert_eq!(circulating_supply(&s, 1_000).unwrap(), all_listed - 1, "C falls by held");
        s.wall_held_tokens = 541;
        assert_eq!(circulating_supply(&s, 1_000), Err(FloorMathError::SupplyUnderflow));
    }

    /// ⛔ The staircase's cushion — a quote-only position under the price — locks what
    /// it WOULD hold at the tried tick, as the POL does, so counting it can only RAISE the solved floor (fewer
    /// circulating tokens for the same vault), never lower it; `more` empty is the old solve bit for bit; and the
    /// solve is monotone in the vault (more backing never lowers the floor — the ratchet's "only up").
    #[test]
    fn the_cushion_locks_what_it_would_hold_and_only_raises_the_floor() {
        let open = low_cap_open();
        let s = graduated(open);
        let q: u128 = 20_000_000_000; // 20 SOL of backing
        let plain = solve_floor_tick_with(q, 0, &s, SUPPLY).unwrap();
        assert_eq!(solve_floor_tick_with_more(q, 0, &s, SUPPLY, &[]).unwrap(), plain, "no cushion: the old solve");
        // a cushion of 5 SOL from 0.9 × a price 4 × the open up to it
        let market = open - 2 * TICKS_PER_DOUBLING;
        let (lo, hi) = crate::stair::cushion_range(market, 60).unwrap().unwrap();
        let p = get_sqrt_price_at_tick(market).unwrap();
        let liq = position_liquidity(p, lo, hi, 5_000_000_000, 0).unwrap();
        assert!(liq > 0);
        // at a tick cheaper than its range the cushion is all base: it locks tokens
        let at = hi + 60;
        let with = sellable_at_with(&s, SUPPLY, at, 0, &[(lo, hi, liq)]).unwrap();
        let without = sellable_at(&s, SUPPLY, at, 0).unwrap();
        assert!(with < without, "{with} !< {without}");
        assert_eq!(without - with, pol_base(at, lo, hi, liq).unwrap());
        // and dearer than its range it is all quote: it locks nothing
        assert_eq!(sellable_at_with(&s, SUPPLY, lo - 60, 0, &[(lo, hi, liq)]).unwrap(), sellable_at(&s, SUPPLY, lo - 60, 0).unwrap());
        let more = solve_floor_tick_with_more(q, 0, &s, SUPPLY, &[(lo, hi, liq)]).unwrap();
        assert!(more <= plain, "the cushion never lowers the floor (a lower tick is a dearer floor): {more} vs {plain}");
        // monotone in the vault
        let mut last = i32::MAX;
        for q in [1_000_000_000u128, 5_000_000_000, 20_000_000_000, 80_000_000_000, 400_000_000_000] {
            let t = solve_floor_tick_with_more(q, 0, &s, SUPPLY, &[(lo, hi, liq)]).unwrap();
            assert!(t <= last, "q {q}: {t} > {last}");
            last = t;
        }
    }

    /// A BURN RAISES THE FLOOR — the EVM twins `FloorMath::test_burningSupplyRaisesFloor` and the burn half of
    /// `OFTSafety::test_bridgeDoesNotChangeSupportedFloorWhilePermanentBurnDoes`.
    /// The ratchet solves against the mint's LIVE supply (`instructions/ratchet.rs` reads `base_mint.supply`), so the same
    /// vault and positions over fewer tokens back a floor at least as dear (a LOWER tick on Solana's axis), strictly
    /// dearer at a 10 % burn, and monotone in the amount burned. Mutations this kills: circulating read off a constant
    /// supply; the burn added instead of subtracted.
    #[test]
    fn a_burn_raises_the_floor_and_never_lowers_it() {
        let s = graduated(low_cap_open());
        let q: u128 = 20_000_000_000; // 20 SOL of backing
        let base = solve_floor_tick_with(q, 0, &s, SUPPLY).unwrap();
        let tenth = solve_floor_tick_with(q, 0, &s, SUPPLY - SUPPLY / 10).unwrap();
        assert!(tenth < base, "a 10 % burn backs a strictly dearer floor: {tenth} vs {base}");
        let mut last = base;
        for pct in [1u64, 2, 5, 10, 20, 30] {
            let t = solve_floor_tick_with(q, 0, &s, SUPPLY - SUPPLY / 100 * pct).unwrap();
            assert!(t <= last, "{pct} % burned: {t} > {last}");
            last = t;
        }
    }

    /// ⛔ The published floor is never lowered: the staircase ratchet keeps the PUBLISHED floor backed —
    /// `stair::backing_need` is the exact quote at which the solve (the cushion counted) publishes `floor_tick` or
    /// dearer, and one lamport less publishes cheaper. And the cushion moved DEARER over the same quote locks fewer
    /// tokens at the floor, so the need rises: the climb check's monotony.
    #[test]
    fn the_backing_need_is_the_solves_own_line_at_the_published_floor() {
        let open = low_cap_open();
        let s0 = graduated(open);
        let market = open - 2 * TICKS_PER_DOUBLING;
        let p = get_sqrt_price_at_tick(market).unwrap();
        let cushion_at = |lower: i32| {
            let (lo, hi) = crate::stair::cushion_range_from(lower, 60).unwrap().unwrap();
            (lo, hi, position_liquidity(p, lo, hi, 5_000_000_000, 0).unwrap())
        };
        let (lo, _) = crate::stair::cushion_range(market, 60).unwrap().unwrap();
        for floor_tick in [open + 600, open + 3_000, open + 9_000].map(|t| ceil_to_spacing(t, 60).unwrap()) {
            let mut s = s0;
            s.floor_tick = floor_tick;
            for c in [cushion_at(lo), cushion_at(lo + 3_000)] {
                let need = crate::stair::backing_need(&s, SUPPLY, floor_tick, &[c]).unwrap();
                assert!(need > 0);
                let at = publish(solve_floor_tick_with_more(need, 0, &s, SUPPLY, &[c]).unwrap(), 60).unwrap();
                assert!(at <= floor_tick, "need {need} publishes {at}, the floor stands at {floor_tick}");
                let under = publish(solve_floor_tick_with_more(need - 1, 0, &s, SUPPLY, &[c]).unwrap(), 60).unwrap();
                assert!(under > floor_tick, "one lamport less publishes cheaper: {under} vs {floor_tick}");
            }
            // a dearer cushion (lower tick) with the same quote needs MORE backing
            let dear = crate::stair::backing_need(&s, SUPPLY, floor_tick, &[cushion_at(lo)]).unwrap();
            let cheap = crate::stair::backing_need(&s, SUPPLY, floor_tick, &[cushion_at(lo + 3_000)]).unwrap();
            assert!(dear > cheap, "{dear} !> {cheap}");
        }
    }

    /// THE FAST STEP IS THE STEP: the solve with every edge read once a solve answers
    /// exactly as the original, every step through `cost_at`, over a sweep of backings, `extra`, cushions (none, empty,
    /// below, straddling and above the open) and two of them — the same tick and the same step count.
    #[test]
    fn the_fast_step_is_the_step() {
        let s = graduated(low_cap_open());
        let open = low_cap_open();
        let cushion = |lo: i32, w: i32, quote: u64| -> MorePosition {
            let (lo, hi) = (ceil_to_spacing(lo, 60).unwrap(), ceil_to_spacing(lo + w, 60).unwrap());
            let p = get_sqrt_price_at_tick(lo - 60).unwrap();
            (lo, hi, position_liquidity(p, lo, hi, quote, 0).unwrap())
        };
        let sets: [&[MorePosition]; 6] = [
            &[],
            &[(open, open + 600, 0)],
            &[cushion(open + 600, 2_100, 5_000_000_000)],
            &[cushion(open - 1_200, 2_100, 5_000_000_000)],
            &[cushion(open + 9_000, 6_000, 50_000_000_000)],
            &[cushion(open + 600, 2_100, 5_000_000_000), cushion(open + 4_000, 1_200, 1_000_000_000)],
        ];
        let mut checked = 0;
        for more in sets {
            for q in [0u128, 1, 1_000, 50_000_000, 2_750_000_000, 27_500_000_000, 1_000_000_000_000, 10u128.pow(20)] {
                for extra in [0u64, 1_000_000_000_000, 300_000_000_000_000] {
                    let (lo, hi) = (ceil_to_spacing(MIN_TICK, 60).unwrap(), ceil_to_spacing(MAX_TICK, 60).unwrap());
                    let fast = solve_counting_with(q, extra, &s, SUPPLY, more);
                    let general = solve_counting_general(q, extra, &s, SUPPLY, more, lo, hi);
                    assert_eq!(fast, general, "q={q} extra={extra} more={more:?}");
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 6 * 8 * 3);
        // an underflowing supply is the same error on both paths
        let mut bad = s;
        bad.inventory_tokens = SUPPLY;
        let (lo, hi) = (ceil_to_spacing(MIN_TICK, 60).unwrap(), ceil_to_spacing(MAX_TICK, 60).unwrap());
        assert_eq!(solve_counting_with(1, 0, &bad, SUPPLY, &[]), solve_counting_general(1, 0, &bad, SUPPLY, &[], lo, hi));
    }

    /// FLOOR FIRST'S PROBE IS `backing_need`: [`crate::stair::BackingNeed::with_quote`] against
    /// `backing_need` with the cushion built as the hook's floor first builds it, over floors, ranges, prices and quotes.
    #[test]
    fn backing_need_at_is_backing_need() {
        let s = graduated(low_cap_open());
        let open = low_cap_open();
        let mut checked = 0;
        for floor_tick in [open + 1_200, open + 6_000, open + 30_000] {
            for range in [None, Some((open + 600, open + 2_700)), Some((open - 600, open + 1_500)), Some((open + 9_000, open + 9_060))] {
                let at = crate::stair::BackingNeed::new(&s, SUPPLY, floor_tick, range).unwrap();
                for price_tick in [open - 3_000, open, open + 1_000] {
                    let p = get_sqrt_price_at_tick(price_tick).unwrap();
                    for quote in [0u64, 1, 1_000_000, 5_000_000_000, 500_000_000_000] {
                        let more: Vec<MorePosition> = match range {
                            Some((lo, hi)) if quote > 0 => {
                                let l = position_liquidity(p, lo, hi, quote, 0).unwrap();
                                if l != 0 { vec![(lo, hi, l)] } else { vec![] }
                            }
                            _ => vec![],
                        };
                        let want = crate::stair::backing_need(&s, SUPPLY, floor_tick, &more).unwrap();
                        assert_eq!(at.with_quote(p, quote).unwrap(), want, "floor {floor_tick} range {range:?} price {price_tick} quote {quote}");
                        // the climb's form: the range per probe, its liquidity always computed (`backing_need` with it)
                        if let Some((lo, hi)) = range {
                            let l = position_liquidity(p, lo, hi, quote, 0).unwrap();
                            let want = crate::stair::backing_need(&s, SUPPLY, floor_tick, &[(lo, hi, l)]).unwrap();
                            let none = crate::stair::BackingNeed::new(&s, SUPPLY, floor_tick, None).unwrap();
                            assert_eq!(none.with_range(p, lo, hi, quote).unwrap(), want, "the climb's form");
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 3 * 4 * 3 * 5);
    }

    /// THE BRACKET IS THE CLAMP: [`solve_floor_tick_within`] equals the full solve clamped to the bracket
    /// over backings, cushions and brackets below, around, above and astride the answer — and the undefined floor answers
    /// as the full solve does.
    #[test]
    fn m1_any_aim_is_the_bisection() {
        // a monotone staircase of costs on the grid, every answer position, every aim (inside, at, and past both ends)
        let s = 60;
        let grid_lo = ceil_to_spacing(MIN_TICK, s).unwrap();
        let mut n = 0;
        for (lo, hi) in [(6_000, 6_600), (6_000, 12_000), (-60_000, 60_000), (grid_lo, grid_lo + 1_200)] {
            for knee in (lo - 120..=hi + 120).step_by(60) {
                for qd in [0u128, 1] {
                    // cost(t) = 1000 + (knee - t) for t < knee (unaffordable at q = 1000), ≤ q from the knee on
                    let q = 1_000u128 + qd;
                    let cost = |t: i32| -> FloorMathResult<u128> { Ok(if t < knee { 1_000 + (knee - t) as u128 + 1 } else { 1_000 - ((t - knee) as u128).min(999) }) };
                    let want = {
                        let (mut l, mut h) = (lo, hi);
                        if cost(h).unwrap() > q { h } else if l != grid_lo && cost(l).unwrap() <= q { l } else {
                            while h - l > s { let mut m = ceil_to_spacing(l + (h - l) / 2, s).unwrap(); if m >= h { m = h - s; } if m <= l { m = l + s; } if cost(m).unwrap() <= q { h = m } else { l = m } }
                            h
                        }
                    };
                    for aim in [None, Some(lo - 600), Some(hi + 600)].into_iter().chain((lo..=hi).step_by(60).map(Some)) {
                        assert_eq!(within_aimed(q, lo, hi, s, grid_lo, aim, cost).unwrap(), want, "lo {lo} hi {hi} knee {knee} aim {aim:?}");
                        n += 1;
                    }
                }
            }
        }
        assert!(n > 10_000);
    }

    #[test]
    fn m1_guided_probe_counts() {
        let s = graduated(low_cap_open());
        let open = low_cap_open();
        let c = {
            let (lo, hi) = (ceil_to_spacing(open + 600, 60).unwrap(), ceil_to_spacing(open + 2_700, 60).unwrap());
            (lo, hi, position_liquidity(get_sqrt_price_at_tick(lo - 60).unwrap(), lo, hi, 5_000_000_000, 0).unwrap())
        };
        let sets: [&[MorePosition]; 2] = [&[], &[c]];
        let (mut n, mut tot, mut bis, mut worst) = (0u32, 0u32, 0u32, 0u32);
        for more in sets {
            for q in [50_000_000u128, 2_750_000_000, 27_500_000_000, 1_000_000_000_000, 30_000_000_000_000] {
                let full = solve_floor_tick_with_more(q, 0, &s, SUPPLY, more).unwrap();
                for (dl, dh) in [(-20_000, 600), (-6_000, 6_000), (-600, 18_000), (-40_000, 60), (-120, 30_000)] {
                    let lo = ceil_to_spacing(full + dl, 60).unwrap();
                    let hi = ceil_to_spacing(full + dh, 60).unwrap();
                    let (got, p) = solve_floor_tick_within_probes(q, &s, SUPPLY, more, lo, hi).unwrap();
                    assert_eq!(got, full.clamp(lo, hi));
                    let b = 2 + (((hi - lo) / 60) as f64).log2().ceil() as u32;
                    n += 1; tot += p; bis += b; worst = worst.max(p);
                }
            }
        }
        println!("[m1-guided] {n} solves: {tot} probes guided vs ~{bis} bisection; worst {worst}");
    }

    #[test]
    fn the_bracket_is_the_clamp() {
        let s = graduated(low_cap_open());
        let open = low_cap_open();
        let c = {
            let (lo, hi) = (ceil_to_spacing(open + 600, 60).unwrap(), ceil_to_spacing(open + 2_700, 60).unwrap());
            (lo, hi, position_liquidity(get_sqrt_price_at_tick(lo - 60).unwrap(), lo, hi, 5_000_000_000, 0).unwrap())
        };
        let sets: [&[MorePosition]; 2] = [&[], &[c]];
        let mut checked = 0;
        for more in sets {
            for q in [0u128, 1_000, 50_000_000, 2_750_000_000, 27_500_000_000, 1_000_000_000_000, 10u128.pow(20)] {
                let full = solve_floor_tick_with_more(q, 0, &s, SUPPLY, more).unwrap();
                for (dl, dh) in [(-6_000, -600), (-600, 600), (-60, 60), (0, 60), (-60, 0), (600, 6_000), (-200_000, 200_000), (-1_200, 0), (0, 1_200)] {
                    let lo = ceil_to_spacing(full + dl, 60).unwrap();
                    let hi = ceil_to_spacing(full + dh, 60).unwrap();
                    if lo >= hi {
                        continue;
                    }
                    let got = solve_floor_tick_within(q, &s, SUPPLY, more, lo, hi).unwrap();
                    let gl = ceil_to_spacing(MIN_TICK, 60).unwrap();
                    let gh = ceil_to_spacing(MAX_TICK, 60).unwrap();
                    assert_eq!(got, full.clamp(lo.max(gl), hi.min(gh)), "q={q} bracket [{lo}, {hi}] full {full}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "{checked}");
        // the undefined floor: the dear edge, whatever the bracket
        let mut none = s;
        none.min_sellable = SUPPLY;
        let full = solve_floor_tick_with_more(1, 0, &none, SUPPLY, &[]).unwrap();
        let (lo, hi) = (ceil_to_spacing(open, 60).unwrap(), ceil_to_spacing(open + 6_000, 60).unwrap());
        assert_eq!(solve_floor_tick_within(1, &none, SUPPLY, &[], lo, hi).unwrap(), full);
        assert!(solve_floor_tick_within(1, &s, SUPPLY, &[], open, open + 6_000).is_err(), "off the grid");
        assert!(solve_floor_tick_within(1, &s, SUPPLY, &[], hi, lo).is_err(), "empty");
    }
}
