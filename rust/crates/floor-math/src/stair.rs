//! THE STAIRCASE AND THE FED, FOLLOWING POL. The pure half: the supply split, the steps' tokens and tick edges, the
//! two-number split of a ratchet's collection, and the cushion's and the POL re-list's ranges. The EVM twin computes
//! the same: every constant and rounding rule here is written so the two chains compute the same thing from the same
//! inputs.
//!
//! The split is decided by two numbers: how far the price is from the floor, and the liquidity holdings relative to
//! the market cap. The classic POL always stays and always gets liquidity.
//!
//! THE SHAPE (1B tokens): 20 % sale · 10 % CLASSIC POL · 69 % STAIRCASE · 1 % OPEN TAIL.
//! - Step i (i = 0..steps) is ONE token-only position over one doubling of the price: token prices
//!   `[open · 2^i, open · 2^(i+1)]`, placed once and NEVER moved. Its tokens are `stair · g^i / Σ g^j`
//!   ([`step_tokens`], `g` = growth in bps, 10,000 = equal steps).
//! - The open tail is token-only from the staircase's top (`open · 2^steps`) to the top of the tick range.
//! - At every ratchet the quote the steps and the tail took is COLLECTED and split by [`stair_split`]: the POL
//!   gets `collected · shortC / (shortF + shortC)` (rounded DOWN), the floor vault the rest (as a normal vault
//!   inflow, so the 2.5 % `$FLOOR` cut applies to it and only to it).
//! - The POL's share and the cushion's previous quote are placed as ONE quote-only CUSHION from 0.9 × the price
//!   up to the price ([`cushion_range`]), re-placed under the new price at every ratchet and never swept. Tokens
//!   the cushion bought from sellers are re-listed token-only from the price to 2 × the price ([`relist_range`]);
//!   what that position takes joins the next ratchet's collection.
//!
//! ⛔ TICK ORIENTATION (every FLOOR pool): the quote is token0, the base token1, and the pool's price is
//! base per quote. A DEARER token is a LOWER tick. So "one doubling of the token price above x" is
//! `x − ticks(2)`, a token-only (base-only) position sits at ticks AT OR BELOW the current tick, and a quote-only
//! one strictly ABOVE it.

use carpenter_clmm_math::big_num::U256;
use carpenter_clmm_math::full_math::MulDiv;
use carpenter_clmm_math::tick_math::{get_tick_at_sqrt_price, MAX_TICK, MIN_TICK};

use crate::constants::{BPS, SUPPLY};
use crate::error::{FloorMathError, FloorMathResult};
use crate::price::sqrt_price_from_ratio;
use crate::spacing::{ceil_to_spacing, floor_to_spacing};

// ---- the staircase's numbers -----------------------------------------------------------------------

/// The sale's share of supply on a staircase preset (20 %).
pub const STAIR_SALE_BPS: u16 = 2_000;
/// The CLASSIC POL's share (10 %) — today's POL, unchanged: token-only from the open to the top of the range,
/// never withdrawn.
pub const STAIR_POL_BPS: u16 = 1_000;
/// The staircase's share (69 %), so the steps reach about a billion dollars with less sell impact near the top.
pub const STAIR_STEPS_BPS: u16 = 6_900;
/// The open tail's share (1 %).
pub const STAIR_TAIL_BPS: u16 = 100;
const _: () = assert!(STAIR_SALE_BPS as u64 + STAIR_POL_BPS as u64 + STAIR_STEPS_BPS as u64 + STAIR_TAIL_BPS as u64 == BPS);

/// Bounds on a staircase's step count. 16 is the storage's width (`StairState.step_*`).
pub const MIN_STAIR_STEPS: u8 = 1;
pub const MAX_STAIR_STEPS: u8 = 16;
/// Bounds on the growth factor between consecutive steps, bps (10,000 = equal steps; < 10,000 = early steps
/// heavier). The presets use 10,000 (Moonshot) and 10,400 (Cook, Bluechip); the window is generous and refuses only
/// the absurd.
pub const MIN_STAIR_GROWTH_BPS: u16 = 5_000;
pub const MAX_STAIR_GROWTH_BPS: u16 = 20_000;

/// THE TWO NUMBERS' TARGETS: the floor ÷ the price, and the POL's quote ÷ the market cap.
pub const FLOOR_TO_PRICE_BPS: u64 = 2_000;
/// The POL target: 15 % (1,500 bps), with `CUSHION_TWAP_WINDOW_SECS` 900 (the EVM twin's `STAIR_POL_TO_MCAP_BPS` is the
/// same): a cushion just under 3 % of the mcap would let a 3 % sell fall through it. Floor ÷ price is 20 %. Presets
/// only: `$FLOOR` keeps its own 20 % (`GENESIS_POL_TO_MCAP_BPS`).
pub const POL_TO_MCAP_BPS: u64 = 1_500;
/// The cushion's cheap edge as a share of the price (0.9 ×).
pub const CUSHION_BOTTOM_BPS: u64 = 9_000;
/// The POL re-list's dear edge as a multiple of the price (2 ×).
pub const POL_RELIST_MULTIPLE: u128 = 2;

// ---- the per-pool parameters ----------------------------------------------------------------------

/// ⛔ `$FLOOR` (the Solana genesis) runs the SAME staircase as the presets — one machine, never a fork — with four
/// numbers of its own: its steps' and tail's shares of supply (89.75 % / 1 %, the presets' 69 % / 1 %), its POL target
/// (POL money ÷ market cap 20 %, the presets' 15 %) and its cushion's depth (the POL's bids from 0.7 × the price, the
/// presets' 0.9 ×). Floor ÷ price stays 20 % on both.
///
/// So these are PER POOL: a pool records which set it runs (`FloorPool.stair_kind`, [`STAIR_KIND_PRESET`] /
/// [`STAIR_KIND_GENESIS`]) and graduation copies the set's numbers into its `StairState` (`cushion_bottom_bps`,
/// `pol_to_mcap_bps`, `cushion_ticks`), which is what every ratchet reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StairParams {
    /// The steps' share of supply, bps (what graduation deposits for the steps and the tail is split in the ratio
    /// `steps_bps : tail_bps`, [`stair_tail_split_for`]).
    pub steps_bps: u16,
    /// The open tail's share of supply, bps.
    pub tail_bps: u16,
    /// The floor ÷ price target, bps.
    pub floor_to_price_bps: u64,
    /// The POL's quote ÷ market cap target, bps.
    pub pol_to_mcap_bps: u64,
    /// The cushion's cheap edge as a share of the price, bps (the POL's bids sit from here up to the price).
    pub cushion_bottom_bps: u64,
    /// Ticks from the price down to `cushion_bottom_bps` of it: `ceil(ln(10,000 / bottom) / ln 1.0001)`
    /// ([`cushion_ticks_for`], tabled here because the formula costs thousands of CU on SBF).
    pub cushion_ticks: i32,
}

/// `FloorPool.stair_kind`: the presets' parameter set (0 — so every pool written before reads it).
pub const STAIR_KIND_PRESET: u8 = 0;
/// `FloorPool.stair_kind`: `$FLOOR`'s (the Solana genesis) parameter set.
pub const STAIR_KIND_GENESIS: u8 = 1;

/// The presets' set (Moonshot, Cook, Bluechip): 69 / 1, targets 20 % / 15 %, the cushion from 0.9 × the price.
pub const PRESET_STAIR: StairParams = StairParams {
    steps_bps: STAIR_STEPS_BPS,
    tail_bps: STAIR_TAIL_BPS,
    floor_to_price_bps: FLOOR_TO_PRICE_BPS,
    pol_to_mcap_bps: POL_TO_MCAP_BPS,
    cushion_bottom_bps: CUSHION_BOTTOM_BPS,
    cushion_ticks: CUSHION_TICKS,
};

/// `$FLOOR`'s steps' share: **89.75 %** (197M protocol + 700.5M team).
/// ⚠ `$FLOOR`'s pool splits its deposit by SUPPLY (`stair_tail_split_supply`: the tail exactly 10M, every unsold sale
/// token on the steps), so this ratio is the plan, not the arithmetic.
pub const GENESIS_STAIR_STEPS_BPS: u16 = 8_975;
/// `$FLOOR`'s open tail (1 %, 10M).
pub const GENESIS_STAIR_TAIL_BPS: u16 = 100;
/// `$FLOOR`'s POL target: the POL's quote ÷ market cap 20 % (the presets keep 15 %).
pub const GENESIS_POL_TO_MCAP_BPS: u64 = 2_000;
/// `$FLOOR`'s cushion: the POL's bids from 0.7 × the price up to it (the presets keep 0.9 ×).
pub const GENESIS_CUSHION_BOTTOM_BPS: u64 = 7_000;
/// `ceil(ln(10,000 / 7,000) / ln 1.0001)` = 3,567 (3,566.93): [`cushion_ticks_for`]`(7,000)`, pinned equal by a test.
pub const GENESIS_CUSHION_TICKS: i32 = 3_567;
/// `$FLOOR`'s steps are FRONT-loaded: step i holds `0.95^i` parts (each step 0.95 × the tokens of the one below).
pub const GENESIS_STAIR_GROWTH_BPS: u16 = 9_500;

/// `$FLOOR`'s set: 89.75 / 1, targets 20 % / 20 %, the cushion from 0.7 × the price.
pub const GENESIS_STAIR: StairParams = StairParams {
    steps_bps: GENESIS_STAIR_STEPS_BPS,
    tail_bps: GENESIS_STAIR_TAIL_BPS,
    floor_to_price_bps: FLOOR_TO_PRICE_BPS,
    pol_to_mcap_bps: GENESIS_POL_TO_MCAP_BPS,
    cushion_bottom_bps: GENESIS_CUSHION_BOTTOM_BPS,
    cushion_ticks: GENESIS_CUSHION_TICKS,
};

/// The parameter set a pool's `stair_kind` names; `None` for a byte no program writes.
pub fn stair_params(kind: u8) -> Option<StairParams> {
    match kind {
        STAIR_KIND_PRESET => Some(PRESET_STAIR),
        STAIR_KIND_GENESIS => Some(GENESIS_STAIR),
        _ => None,
    }
}

/// The set a `StairState` carries (graduation wrote it from `stair_params(kind)`, never 0); the stored numbers are
/// read as they are.
pub fn stored_stair_params(cushion_bottom_bps: u16, pol_to_mcap_bps: u16, cushion_ticks: i32) -> StairParams {
    StairParams {
        cushion_bottom_bps: cushion_bottom_bps as u64,
        pol_to_mcap_bps: pol_to_mcap_bps as u64,
        cushion_ticks,
        ..PRESET_STAIR
    }
}

/// The fixed-point scale of the two shortfalls (1e18 = "fully short"). Every ratio is floored at this scale.
pub const SPLIT_SCALE: u128 = 1_000_000_000_000_000_000;
/// The first step's weight in [`step_tokens`] (1e18); each next weight is the previous × growth / 10,000, floored.
pub const STEP_WEIGHT_ONE: u128 = 1_000_000_000_000_000_000;

// ---- the supply split -------------------------------------------------------------------------

/// `(steps_total, tail)` out of what graduation deposited for the staircase and the tail together (`credited`, 70 %
/// of supply on a staircase preset): the tail is `credited · 100 / 7,000` (floored), the steps the rest. At the
/// exact 700e12 this is 690e12 / 10e12, the supply-based split to the unit.
pub fn stair_tail_split(credited: u64) -> (u64, u64) {
    stair_tail_split_for(credited, &PRESET_STAIR)
}

/// [`stair_tail_split`] for a parameter set: the tail is `credited · tail_bps / (steps_bps + tail_bps)` (floored), the
/// steps the rest. `$FLOOR` (45 / 1): at the exact 460e12 this is 450e12 / 10e12.
pub fn stair_tail_split_for(credited: u64, p: &StairParams) -> (u64, u64) {
    let whole = p.steps_bps as u128 + p.tail_bps as u128;
    let tail = if whole == 0 { 0 } else { (credited as u128 * p.tail_bps as u128 / whole) as u64 }; // ≤ credited
    (credited - tail, tail)
}

// ---- Custom on the staircase ------------------------------------------------------------------------------------------

/// ⛔ CUSTOM ON THE STAIRCASE: a Custom launch's step count is derived from its cap, its steps are equal, and its at-floor
/// sell multiple is the launcher's own (1–5×); there is no wall. The EVM twin is `FloorPolicy.CUSTOM_STAIR_TOP_MCAP_USD`
/// / `FloorModes.customStairSteps`. A Custom launch's staircase climbs from its graduation cap to this market cap, in
/// whole USD: $1B.
pub const CUSTOM_STAIR_TOP_MCAP_USD: u64 = 1_000_000_000;
/// A Custom launch's steps are EQUAL (the EVM's `FloorPolicy.CUSTOM_STAIR_GROWTH_BPS`).
pub const CUSTOM_STAIR_GROWTH_BPS: u16 = 10_000;

/// THE CUSTOM STEP COUNT (the EVM's `FloorModes.customStairSteps`, 1:1): the
/// number of doublings from a Custom launch's graduation cap (`cap_usd`, WHOLE USD — `CreateLaunchParams.
/// graduation_mcap_usd`'s own unit) to [`CUSTOM_STAIR_TOP_MCAP_USD`], `round(log2(top / cap))` clamped to
/// `1..=`[`MAX_STAIR_STEPS`], ROUNDED EXACTLY IN LOG SPACE: n = floor(log2(top / cap)) (capped at 16), then n + 1 when
/// `top² ≥ cap² · 2^(2n + 1)` (the ratio at or past 2^(n + 0.5)). A cap at or above the top gets 1 step; n = 0 reads as
/// 1. `cap_usd` 0 (refused by `resolve_config` before this is asked) reads as the most steps, as on the EVM.
/// $25k → 15, $50k → 14, $100k → 13, $200k → 12, $250k → 12, $1M → 10, $10M → 7, $1k → 16, $600M → 1, $800M → 1,
/// $690,533 → 11, $690,534 → 10.
pub fn custom_stair_steps(cap_usd: u64) -> u8 {
    let top = CUSTOM_STAIR_TOP_MCAP_USD as u128;
    let max = MAX_STAIR_STEPS as u32;
    let cap = cap_usd as u128;
    if cap == 0 {
        return MAX_STAIR_STEPS;
    }
    if cap >= top {
        return 1;
    }
    let mut n = 0u32;
    // cap < 1e9, so cap << 17 < 2^47: no shift overflows
    while n < max && (cap << (n + 1)) <= top {
        n += 1;
    }
    // round up when top / cap ≥ 2^(n + 0.5) ⇔ top² ≥ cap² · 2^(2n + 1); cap² < 1e18 < 2^60 and 2n + 1 ≤ 33, so < 2^93
    if n < max && top * top >= (cap * cap) << (2 * n + 1) {
        n += 1;
    }
    if n == 0 {
        1
    } else {
        n as u8
    }
}

/// A Custom launch's staircase `(steps, growth_bps)`: [`custom_stair_steps`] of its cap, equal steps. It runs the
/// PRESETS' machine ([`PRESET_STAIR`]: the POL target, the cushion, `STAIR_KIND_PRESET`), never `$FLOOR`'s.
pub fn custom_stair(cap_usd: u64) -> (u8, u16) {
    (custom_stair_steps(cap_usd), CUSTOM_STAIR_GROWTH_BPS)
}

/// ⛔ THE TAIL IS 1 % OF SUPPLY (the EVM's `FloorStairs.split`): of what graduation deposited for the steps
/// and the tail together (`credited`), the open tail takes `SUPPLY / 10,000 · tail_bps` (capped at `credited`) and the
/// steps take the rest — so a Custom launch whose reserve is not the presets' 70 % (its own split, each zone ≥ 10 %;
/// 30 / 10 / 60 by default) keeps a 10M tail and puts the rest of its reserve on the steps. ⛔ `floor_hook` uses it for a
/// CUSTOM pool of the presets' kind ONLY: every preset and `$FLOOR` keep [`stair_tail_split_for`], so their placement is
/// bit-identical whatever was credited (the two agree at exactly 700e12 / 460e12 — pinned by
/// `the_tail_is_one_percent_of_supply_and_the_presets_are_unchanged` — but not at a deposit off those, e.g. `$FLOOR`'s
/// zero raise, which deposits more).
pub fn stair_tail_split_supply(credited: u64, p: &StairParams) -> (u64, u64) {
    let tail = (SUPPLY / BPS * p.tail_bps as u64).min(credited);
    (credited - tail, tail)
}

/// Is `(steps, growth_bps)` a staircase the programs admit?
pub fn stair_shape_ok(steps: u8, growth_bps: u16) -> bool {
    (MIN_STAIR_STEPS..=MAX_STAIR_STEPS).contains(&steps) && (MIN_STAIR_GROWTH_BPS..=MAX_STAIR_GROWTH_BPS).contains(&growth_bps)
}

/// THE STEPS' TOKENS: step i gets `floor(total · w_i / Σ w)`, `w_0 = 1e18`, `w_i = floor(w_{i−1} · growth / 10,000)`;
/// the rounding residue (fewer units than there are steps) goes to step 0, the first to sell. The result sums to
/// `total` EXACTLY. Entries past `steps` are 0.
pub fn step_tokens(total: u64, steps: u8, growth_bps: u16) -> FloorMathResult<[u64; MAX_STAIR_STEPS as usize]> {
    if !stair_shape_ok(steps, growth_bps) {
        return Err(FloorMathError::BadSplit);
    }
    let n = steps as usize;
    let mut w = [0u128; MAX_STAIR_STEPS as usize];
    w[0] = STEP_WEIGHT_ONE;
    for i in 1..n {
        w[i] = w[i - 1] * growth_bps as u128 / BPS as u128; // ≤ 1e18 · 2^16: no overflow
    }
    let sum: u128 = w[..n].iter().sum();
    let mut out = [0u64; MAX_STAIR_STEPS as usize];
    let mut given = 0u64;
    for i in 0..n {
        // ⚠ COMPUTE: the product in u128 whenever it fits (every preset: total < 2^50, w ≤ 2^60), the U256
        // mul-div only when it does not — the same floor either way.
        out[i] = match (total as u128).checked_mul(w[i]) {
            Some(p) => (p / sum) as u64,
            None => U256::from(total).mul_div_floor(U256::from(w[i]), U256::from(sum)).ok_or(FloorMathError::Overflow)?.as_u64(),
        }; // ≤ total
        given += out[i];
    }
    out[0] += total - given; // the residue: < n units
    Ok(out)
}

// ---- the steps' ticks ---------------------------------------------------------------------------

/// Ticks for a doubling of the token price, `i` times over: `ceil(i · ln 2 / ln 1.0001)` — the
/// [`crate::wall_ticks`] form for the multiple `2^i` (so `doubling_ticks(1)` = 6,932 = `TICKS_PER_DOUBLING`).
/// Computed per `i`, never `i · doubling_ticks(1)`, so steps do not drift.
/// ⚠ COMPUTE (measured on the graduating buy under LiteSVM): each computation is a U256 integer square root and a
/// `get_tick_at_sqrt_price` — thousands of CU on SBF — and graduation's placement asks for ~25 of them. So every `i` a
/// staircase can use (`0..=MAX_STAIR_STEPS`) is read from [`DOUBLING_TICKS`], the SAME values the formula gives
/// (`the_doubling_table_is_the_formula` pins the two equal), and only `i > 16` is computed.
pub fn doubling_ticks(i: u8) -> FloorMathResult<i32> {
    if (i as usize) < DOUBLING_TICKS.len() {
        return Ok(DOUBLING_TICKS[i as usize]);
    }
    doubling_ticks_computed(i)
}

/// `doubling_ticks(i)` for `i = 0..=16`: `ceil(i · ln 2 / ln 1.0001)`, as [`doubling_ticks_computed`] gives them.
pub const DOUBLING_TICKS: [i32; MAX_STAIR_STEPS as usize + 1] =
    [0, 6_932, 13_864, 20_796, 27_728, 34_660, 41_591, 48_523, 55_455, 62_387, 69_319, 76_251, 83_182, 90_114, 97_046, 103_978, 110_910];

/// The formula itself: `get_tick_at_sqrt_price(sqrt_price_from_ratio(1, 2^i)) + 1`.
pub fn doubling_ticks_computed(i: u8) -> FloorMathResult<i32> {
    if i == 0 {
        return Ok(0);
    }
    if i > 64 {
        return Err(FloorMathError::Overflow);
    }
    let t = get_tick_at_sqrt_price(sqrt_price_from_ratio(1, 1u128 << i)?)?;
    t.checked_add(1).ok_or(FloorMathError::Overflow)
}

/// The staircase's cheap edge `i` doublings above `open_edge`, on the grid: `open_edge − ceil_to_spacing(doubling_ticks(i))`.
/// Step i is `[stair_edge(i + 1), stair_edge(i)]` (lower, upper); consecutive steps share an edge, so there is no
/// gap and no overlap.
pub fn stair_edge(open_edge: i32, i: u8, spacing: i32) -> FloorMathResult<i32> {
    let off = ceil_to_spacing(doubling_ticks(i)?, spacing)?;
    (open_edge as i64 - off as i64).try_into().map_err(|_| FloorMathError::Overflow)
}

/// The usable grid's dearest tick (the top of the token-price range), the tail's lower edge.
pub fn grid_bottom(spacing: i32) -> FloorMathResult<i32> {
    ceil_to_spacing(MIN_TICK, spacing)
}

/// Does a staircase of `steps` from `open_edge` fit, leaving at least one spacing for the open tail? `resolve_config`
/// refuses a launch that does not (a quote and a cap whose open tick sits too near the top of the range).
pub fn stair_fits(open_edge: i32, steps: u8, spacing: i32) -> FloorMathResult<bool> {
    let top = stair_edge(open_edge, steps, spacing)?;
    Ok(top as i64 - spacing as i64 >= grid_bottom(spacing)? as i64)
}

/// Step i's range `(lower, upper)` in ticks (upper = its CHEAP edge). Refused past `steps` or off the grid.
pub fn step_range(open_edge: i32, i: u8, steps: u8, spacing: i32) -> FloorMathResult<(i32, i32)> {
    if i >= steps {
        return Err(FloorMathError::InvalidRange);
    }
    let upper = stair_edge(open_edge, i, spacing)?;
    let lower = stair_edge(open_edge, i + 1, spacing)?;
    if lower >= upper || lower < grid_bottom(spacing)? {
        return Err(FloorMathError::InvalidRange);
    }
    Ok((lower, upper))
}

/// The open tail's range `(lower, upper)`: from the top of the range to the staircase's top.
pub fn tail_range(open_edge: i32, steps: u8, spacing: i32) -> FloorMathResult<(i32, i32)> {
    let upper = stair_edge(open_edge, steps, spacing)?;
    let lower = grid_bottom(spacing)?;
    if lower >= upper {
        return Err(FloorMathError::InvalidRange);
    }
    Ok((lower, upper))
}

/// What is left of a token-only range `(lower, upper)` once the market is at `current`: `[lower, min(upper,
/// floor_to_spacing(current))]`, or `None` when nothing dearer than the market is left (the range is sold through).
/// A partly bought step is re-opened over exactly this ("from max(its lower edge, the price) to its upper edge", in
/// token prices).
pub fn unsold_range(lower: i32, upper: i32, current: i32, spacing: i32) -> FloorMathResult<Option<(i32, i32)>> {
    // ⛔ `floor_to_spacing` clamps UP at the top of the range (a result under MIN_TICK + spacing): a line cheaper
    // than the market would not be token-only, so that reads as sold through.
    let at = floor_to_spacing(current, spacing)?.min(upper);
    Ok(if at > lower && at <= current { Some((lower, at)) } else { None })
}

// ---- the cushion and the POL re-list ------------------------------------------------------------

/// Ticks from the price down to 0.9 × the price: `ceil(ln(10,000 / 9,000) / ln 1.0001)` = 1,054 — the constant
/// [`CUSHION_TICKS`] (every ratchet asks; the formula, [`cushion_ticks_computed`], is pinned equal to it).
pub fn cushion_ticks() -> FloorMathResult<i32> {
    Ok(CUSHION_TICKS)
}

/// `cushion_ticks()`'s value.
pub const CUSHION_TICKS: i32 = 1_054;

/// The formula: `get_tick_at_sqrt_price(sqrt_price_from_ratio(9,000, 10,000)) + 1`.
pub fn cushion_ticks_computed() -> FloorMathResult<i32> {
    cushion_ticks_for(CUSHION_BOTTOM_BPS)
}

/// Ticks from the price down to `bottom_bps` of it: `get_tick_at_sqrt_price(sqrt_price_from_ratio(bottom, 10,000)) + 1`
/// = `ceil(ln(10,000 / bottom) / ln 1.0001)` — 1,054 at 9,000 (the presets), 3,567 at 7,000 (`$FLOOR`). Refused outside
/// `1..10,000`.
pub fn cushion_ticks_for(bottom_bps: u64) -> FloorMathResult<i32> {
    if bottom_bps == 0 || bottom_bps >= BPS {
        return Err(FloorMathError::InvalidRange);
    }
    let t = get_tick_at_sqrt_price(sqrt_price_from_ratio(bottom_bps as u128, BPS as u128)?)?;
    t.checked_add(1).ok_or(FloorMathError::Overflow)
}

/// THE CUSHION'S RANGE, quote-only, under the market at `current`: `lower = ceil_to_spacing(current + 1)` (the first
/// grid line strictly cheaper than the price, so the position is all quote) and `upper = lower +
/// ceil_to_spacing(1,054)` (≈ 0.9 × the price). `None` when no grid line is left above the market. The presets' width;
/// [`cushion_range_w`] takes a pool's own.
pub fn cushion_range(current: i32, spacing: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_w(current, spacing, CUSHION_TICKS)
}

/// [`cushion_range`] `width` ticks deep (a pool's `cushion_ticks`: 1,054 on the presets, 3,567 on `$FLOOR`).
pub fn cushion_range_w(current: i32, spacing: i32, width: i32) -> FloorMathResult<Option<(i32, i32)>> {
    let top = floor_to_spacing(MAX_TICK, spacing)?;
    let lower = ceil_to_spacing(current.checked_add(1).ok_or(FloorMathError::Overflow)?, spacing)?;
    if lower <= current || lower >= top {
        return Ok(None);
    }
    let upper = (lower as i64 + ceil_to_spacing(width, spacing)? as i64).min(top as i64) as i32;
    Ok(if upper > lower { Some((lower, upper)) } else { None })
}

/// ⛔ THE CUSHION'S TOP (the fix for the pump + ratchet + dump hole): the cushion
/// sits under `min(spot, TWAP)` — in ticks the CHEAPER, i.e. the HIGHER, of the two — never the spot alone, so a price
/// pumped inside one transaction cannot drag it up to be dumped into. `twap_tick` is the pool's settled tick
/// (`floor_types::twap::TwapOracle::settled_tick` over `CUSHION_TWAP_WINDOW_SECS`).
pub fn cushion_top_tick(spot_tick: i32, twap_tick: i32) -> i32 {
    spot_tick.max(twap_tick)
}

/// [`cushion_range`] under [`cushion_top_tick`]: `[0.9 × top, top]` on the grid, always strictly cheaper than the spot
/// (so all quote).
pub fn cushion_range_at(spot_tick: i32, twap_tick: i32, spacing: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_at_w(spot_tick, twap_tick, spacing, CUSHION_TICKS)
}

/// [`cushion_range_at`] `width` ticks deep.
pub fn cushion_range_at_w(spot_tick: i32, twap_tick: i32, spacing: i32, width: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_w(cushion_top_tick(spot_tick, twap_tick), spacing, width)
}

/// ⛔ The cushion's range never starts inside the bid band; it always sits above the floor: a cushion range CLIPPED to
/// sit above the floor's bid band — its cheap edge (its UPPER tick, 0.9 × the
/// top) never cheaper than the band's DEAR edge `dear_edge` (`floor_tick − band_ticks`, on the grid). A range that would
/// start inside the band starts at the dear edge, its top kept; a range whose top is itself at or under the dear edge
/// (`lower ≥ dear_edge`) is `None` — no cushion is placed or moved this ratchet.
pub fn cushion_clip(range: Option<(i32, i32)>, dear_edge: i32) -> Option<(i32, i32)> {
    match range {
        Some((lo, hi)) if lo < dear_edge => Some((lo, hi.min(dear_edge))),
        _ => None,
    }
}

/// [`cushion_range_at`] clipped above the bid band of the published floor `floor_tick` ([`cushion_clip`]): THE
/// CUSHION'S RANGE on a staircase pool.
pub fn cushion_range_above(spot_tick: i32, twap_tick: i32, spacing: i32, floor_tick: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_above_w(spot_tick, twap_tick, spacing, floor_tick, CUSHION_TICKS)
}

/// [`cushion_range_above`] `width` ticks deep (a pool's own `cushion_ticks`).
pub fn cushion_range_above_w(
    spot_tick: i32,
    twap_tick: i32,
    spacing: i32,
    floor_tick: i32,
    width: i32,
) -> FloorMathResult<Option<(i32, i32)>> {
    Ok(cushion_clip(cushion_range_at_w(spot_tick, twap_tick, spacing, width)?, crate::band::dear_edge(floor_tick, spacing)?))
}

/// ⛔ THE CUSHION'S HOME: the top a cushion takes when `min(spot, TWAP)` has no range above the bid band — the CHEAPEST
/// full-depth range just above the band, `[dear_edge − ceil_to_spacing(width), dear_edge]`, never the spot's. Where the
/// spot is cheaper still (the price within one cushion width of the band) the spot's own range, which the band clips.
/// In ticks the HIGHER of the two. A price pumped earlier in the same transaction or second can no longer set the
/// cushion's top: the home is read off the published floor alone. `cushion_range_above_w(spot, home, …)` is its range;
/// the floor still rises in the same ratchet, and no cushion quote sits idle while the spot has a range.
pub fn cushion_home_top(spot_tick: i32, floor_tick: i32, spacing: i32, width: i32) -> FloorMathResult<i32> {
    let dear = crate::band::dear_edge(floor_tick, spacing)?;
    let home = (dear as i64 - ceil_to_spacing(width, spacing)? as i64 - 1).max(i32::MIN as i64) as i32;
    Ok(spot_tick.max(home))
}

/// THE POL RE-LIST'S RANGE, token-only, over the market at `current`: `upper = floor_to_spacing(current)` (the price,
/// on the grid, so the position is all base) and `lower = upper − ceil_to_spacing(doubling_ticks(1))` (2 × the price).
/// `None` when the top of the range leaves no room.
pub fn relist_range(current: i32, spacing: i32) -> FloorMathResult<Option<(i32, i32)>> {
    let upper = floor_to_spacing(current, spacing)?;
    if upper > current {
        return Ok(None); // the clamp at the top of the range (see `unsold_range`)
    }
    let lower = (upper as i64 - ceil_to_spacing(doubling_ticks(1)?, spacing)? as i64).max(grid_bottom(spacing)? as i64) as i32;
    Ok(if upper > lower { Some((lower, upper)) } else { None })
}

// ---- the two-number split -----------------------------------------------------------------------

/// The split of one ratchet's collection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StairSplit {
    /// Quote that joins the cushion (the POL's share). Rounded DOWN, so the split never over-allocates.
    pub to_pol: u64,
    /// Quote that goes to the floor vault (as a vault inflow: the cut applies).
    pub to_floor: u64,
    /// `max(0, 1 − (floor ÷ price) ÷ 20 %)` at [`SPLIT_SCALE`].
    pub short_floor: u128,
    /// `max(0, 1 − (POL quote ÷ market cap) ÷ 5 %)` at [`SPLIT_SCALE`].
    pub short_pol: u128,
}

/// `floor ÷ price` at [`SPLIT_SCALE`], from the two sqrt prices (Q64.64, base per quote): the token price is `1 / P`,
/// so the ratio is `P_market / P_floor = (√P_market / √P_floor)^2`. Floored; capped at the scale. A zero floor
/// sqrt price (no floor) reads as 0.
pub fn floor_to_price(sqrt_price_x64: u128, floor_sqrt_x64: u128) -> FloorMathResult<u128> {
    if floor_sqrt_x64 == 0 {
        return Ok(0);
    }
    let p2 = U256::from(sqrt_price_x64) * U256::from(sqrt_price_x64);
    let f2 = U256::from(floor_sqrt_x64) * U256::from(floor_sqrt_x64);
    let r = p2.mul_div_floor(U256::from(SPLIT_SCALE), f2).ok_or(FloorMathError::Overflow)?;
    Ok(if r > U256::from(SPLIT_SCALE) { SPLIT_SCALE } else { r.as_u128() })
}

/// `quote ÷ market cap` at [`SPLIT_SCALE`], the market cap being `price × supply = supply / P = supply · 2^128 /
/// √P²` in raw quote units. Floored; capped at the scale. `supply == 0` reads as fully covered.
pub fn quote_to_mcap(quote: u64, sqrt_price_x64: u128, supply: u64) -> FloorMathResult<u128> {
    if supply == 0 {
        return Ok(SPLIT_SCALE);
    }
    let p2 = U256::from(sqrt_price_x64) * U256::from(sqrt_price_x64);
    let num = U256::from(quote) * U256::from(SPLIT_SCALE);
    let den = U256::from(supply) << 128usize;
    let r = p2.mul_div_floor(num, den).ok_or(FloorMathError::Overflow)?;
    Ok(if r > U256::from(SPLIT_SCALE) { SPLIT_SCALE } else { r.as_u128() })
}

/// `max(0, 1 − have / target)` at [`SPLIT_SCALE`], `have` at the scale and `target_bps` in bps: `S − have·10,000 /
/// target_bps`, floored at 0. (Floored division, so a shortfall is never overstated by more than one unit.)
pub fn shortfall(have: u128, target_bps: u64) -> u128 {
    if target_bps == 0 {
        return 0;
    }
    let reached = have.saturating_mul(BPS as u128) / target_bps as u128;
    SPLIT_SCALE.saturating_sub(reached)
}

/// THE TWO-NUMBER SPLIT: how far the price is from the floor, and the liquidity holdings relative to the market cap:
/// - `shortF = max(0, 1 − (floor ÷ price) ÷ 0.20)`, the floor being the published floor BEFORE this ratchet's inflow;
/// - `shortC = max(0, 1 − (polQuote ÷ marketCap) ÷ target)`, `polQuote` = the classic POL's quote + the cushion's;
/// - `toPol = floor(collected · shortC / (shortF + shortC))`, 0 when both are met; the floor gets the rest.
pub fn stair_split(
    collected: u64,
    sqrt_price_x64: u128,
    floor_sqrt_x64: u128,
    pol_quote: u64,
    supply: u64,
) -> FloorMathResult<StairSplit> {
    stair_split_for(collected, sqrt_price_x64, floor_sqrt_x64, pol_quote, supply, &PRESET_STAIR)
}

/// [`stair_split`] against a parameter set's two targets (`floor_to_price_bps`, `pol_to_mcap_bps`): the presets
/// 2,000 / 1,500, `$FLOOR` 2,000 / 2,000.
pub fn stair_split_for(
    collected: u64,
    sqrt_price_x64: u128,
    floor_sqrt_x64: u128,
    pol_quote: u64,
    supply: u64,
    p: &StairParams,
) -> FloorMathResult<StairSplit> {
    let short_floor = shortfall(floor_to_price(sqrt_price_x64, floor_sqrt_x64)?, p.floor_to_price_bps);
    let short_pol = shortfall(quote_to_mcap(pol_quote, sqrt_price_x64, supply)?, p.pol_to_mcap_bps);
    let both = short_floor + short_pol; // ≤ 2e18
    let to_pol = if both == 0 { 0 } else { (collected as u128 * short_pol / both) as u64 }; // ≤ collected
    Ok(StairSplit { to_pol, to_floor: collected - to_pol, short_floor, short_pol })
}

// ---- the reserve: graduation inside the buy ---------------------------------------------------------

/// ⛔ GRADUATION HAPPENS INSIDE THE BUY. The
/// graduating buy cannot open 9–11 steps and the tail inside Solana's compute and trace budgets, so it opens STEP 0 at
/// its own range, ONE RESERVE holding every later step's tokens over their whole range, and the TAIL — three
/// positions, nothing held back and no vacuum above the price. The ratchet then splits the next step off the reserve
/// when the market enters it.
///
/// The reserve's range from step `from` (its first step) to the last: `[stair_edge(steps), stair_edge(from)]`, the
/// concatenation of the planned steps `from..steps` (no gap, no overlap).
pub fn reserve_range(open_edge: i32, from: u8, steps: u8, spacing: i32) -> FloorMathResult<(i32, i32)> {
    if from >= steps {
        return Err(FloorMathError::InvalidRange);
    }
    let (lower, _) = step_range(open_edge, steps - 1, steps, spacing)?;
    let (_, upper) = step_range(open_edge, from, steps, spacing)?;
    Ok((lower, upper))
}

/// Does the staircase keep a RESERVE once steps `[0, placed)` stand on their own? Only when at least TWO steps are
/// left in it: a one-step reserve IS that step, so the last step always becomes its own position (when only the last
/// step remains, it becomes that step, no reserve).
pub fn keeps_reserve(placed: u8, steps: u8) -> bool {
    steps > placed && steps - placed >= 2
}

/// THE SPLIT, pure: the reserve's tokens `left` (what its close returned) hand step `j` (the reserve's first step)
/// `min(planned[j], left − Σ planned[j+1..steps])` — its planned amount less what already sold out of the reserve,
/// never more than planned — and keep the rest for the later steps (`(to_step, rest)`, summing to `left` exactly).
/// A single liquidity over the whole range is THICKER LOW (tokens per doubling fall by 2^(−1/2) a step), so buyers
/// who entered the reserve took more than step `j`'s share from its cheap end: the deficit falls on step `j`, the
/// later steps keep their planned tokens whenever the reserve still holds them, and any excess stays in the reserve.
pub fn reserve_split(left: u64, planned: &[u64; MAX_STAIR_STEPS as usize], j: u8, steps: u8) -> FloorMathResult<(u64, u64)> {
    if j >= steps || steps > MAX_STAIR_STEPS {
        return Err(FloorMathError::InvalidRange);
    }
    let later: u128 = planned[j as usize + 1..steps as usize].iter().map(|&x| x as u128).sum();
    let spare = (left as u128).saturating_sub(later);
    let to_step = spare.min(planned[j as usize] as u128) as u64; // ≤ left
    Ok((to_step, left - to_step))
}

// ---- the vault's bought-back tokens go to the staircase ---------------------------------------------

/// THE 2 × COST LINE of the vault's bought-back tokens, in ticks: the tick of the token price `2 · cost / tokens`
/// (`get_tick_at_sqrt_price` floors it, so every tick AT OR BELOW it is a token price at or above 2 × the cost). A
/// token-only position whose cheap edge (`upper`) is at or below this tick sells nothing under 2 × what the vault
/// paid. `None` when the tokens cost nothing (fees, dust): any step will do. `tokens == 0` has no line either.
pub fn vault_relist_cap_tick(tokens: u64, cost: u64) -> FloorMathResult<Option<i32>> {
    if tokens == 0 || cost == 0 {
        return Ok(None);
    }
    let two_cost = (cost as u128).checked_mul(2).ok_or(FloorMathError::Overflow)?;
    let sqrt = sqrt_price_from_ratio(two_cost, tokens as u128)?; // √(tokens / 2·cost): base per quote at 2 × cost
    Ok(Some(get_tick_at_sqrt_price(sqrt)?))
}

/// Where the vault's bought-back tokens go on a staircase pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultListTarget {
    /// Step i's own position (the step just above the price, or the first own step that starts at ≥ 2 × cost).
    Step(u8),
    /// The reserve (every step from `placed` on).
    Reserve,
    /// The open tail.
    Tail,
    /// Nothing qualifies (every standing position starts under 2 × cost): the tokens stay HELD, off the market and
    /// outside circulating supply, and the next ratchet tries again.
    Hold,
}

/// THE TARGET: tokens the vault buys at the floor are ADDED TO THE STAIRCASE STEP JUST ABOVE THE PRICE, at NO LESS THAN
/// 2× what the vault paid for them; if 2× cost is above that step's range, they go to the first step (or the reserve /
/// tail) whose range starts at or above 2× cost. The
/// candidates, cheapest first: each own step `first_open..placed` still standing (`own[i] = Some(upper)`), then the
/// reserve (`Some(upper)` when it stands), then the tail. The first whose cheap edge is at or dearer than the 2 × cost
/// line (`upper ≤ cap`) takes them; `cap == None` takes the first candidate.
/// ⛔ A position is added to WHOLE, over its own range, so a step inside the reserve is reached only through the
/// reserve (its cheap edge is the reserve's first step's): when that edge is under 2 × cost the tokens go on to the
/// tail, never into a range that would sell them for less.
pub fn vault_list_target(
    cap: Option<i32>,
    first_open: u8,
    placed: u8,
    own_upper: &[Option<i32>; MAX_STAIR_STEPS as usize],
    reserve_upper: Option<i32>,
    tail_upper: Option<i32>,
) -> VaultListTarget {
    let ok = |upper: i32| cap.map_or(true, |c| upper <= c);
    for i in first_open..placed.min(MAX_STAIR_STEPS) {
        if let Some(u) = own_upper[i as usize] {
            if ok(u) {
                return VaultListTarget::Step(i);
            }
        }
    }
    if let Some(u) = reserve_upper {
        if ok(u) {
            return VaultListTarget::Reserve;
        }
    }
    if let Some(u) = tail_upper {
        if ok(u) {
            return VaultListTarget::Tail;
        }
    }
    VaultListTarget::Hold
}

/// The staircase presets' whole-supply check: 20 / 10 / 69 / 1 of [`SUPPLY`].
pub const STAIR_SUPPLY_TOKENS: (u64, u64, u64, u64) = (
    SUPPLY / BPS * STAIR_SALE_BPS as u64,
    SUPPLY / BPS * STAIR_POL_BPS as u64,
    SUPPLY / BPS * STAIR_STEPS_BPS as u64,
    SUPPLY / BPS * STAIR_TAIL_BPS as u64,
);

// ---- the published floor stays backed --------------------------------------------------------------

/// ⛔ THE PUBLISHED FLOOR IS NEVER LOWERED — the cushion stays counted in the solve as it is — and instead (1) FLOOR
/// FIRST ON COLLECTED MONEY and (2) THE CUSHION CLIMBS ONLY AS FAR AS THE FLOOR STAYS BACKED.
///
/// What the vault must hold for the PUBLISHED floor `floor_tick` to be backed: the solve's own cost at its dear edge
/// (`dear_edge(floor_tick)`, the tick `publish` maps back onto `floor_tick`), the protocol positions `more` (the cushion)
/// counted exactly as the solve counts them. `vault ≥ need` ⇔ the solve lands at or dearer than the dear edge ⇔ the
/// floor it would publish is at or dearer than `floor_tick`.
pub fn backing_need(
    state: &crate::state::FloorState,
    supply: u64,
    floor_tick: i32,
    more: &[crate::solve::MorePosition],
) -> FloorMathResult<u128> {
    let t = crate::band::dear_edge(floor_tick, state.tick_spacing)?;
    crate::position::quote_cost(crate::solve::sellable_at_with(state, supply, t, 0, more)?, t)
}

/// [`backing_need`] for ONE published floor and ONE cushion range, every sqrt price read once (floor first's probes would
/// otherwise each re-read the dear edge's, the classic POL's and the cushion's, up to 341k CU a
/// ratchet). [`BackingNeed::with_quote`] is `backing_need` with the cushion holding `quote` over the range — its liquidity
/// `position_liquidity(price, lower, upper, quote, 0)`, left out when zero, as the hook's floor first builds it. The SAME
/// arithmetic (`tests::backing_need_at_is_backing_need`).
pub struct BackingNeed {
    t: u128,
    steps: crate::solve::Steps,
    cushion: Option<(u128, u128)>,
    /// what everything but the cushion locks at the dear edge (`extra` and the classic POL's base there), read once: a
    /// probe adds only the cushion's — the same sum `Steps::sellable` makes
    fixed: u128,
}

impl BackingNeed {
    /// `range`: the cushion's `(lower, upper)`, or `None` for no cushion.
    pub fn new(
        state: &crate::state::FloorState,
        supply: u64,
        floor_tick: i32,
        range: Option<(i32, i32)>,
    ) -> FloorMathResult<Self> {
        let t = crate::band::dear_edge(floor_tick, state.tick_spacing)?;
        let steps = crate::solve::Steps::of(state, supply, 0, &[])?.ok_or(FloorMathError::Overflow)?;
        let cushion = match range {
            Some((lo, hi)) => Some((
                carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(lo)?,
                carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(hi)?,
            )),
            None => None,
        };
        let t = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(t)?;
        let mut fixed = steps.extra as u128;
        if let Some((a, b, liq)) = steps.locked[0] {
            fixed += crate::position::pol_base_at_sqrt(t, a, b, liq)? as u128;
        }
        Ok(BackingNeed { t, steps, cushion, fixed })
    }

    /// The need with `cushion` (`(sqrt lower, sqrt upper, liquidity)`, or none) locking its base at the dear edge.
    fn need(&self, cushion: Option<(u128, u128, u128)>) -> FloorMathResult<u128> {
        let mut locked = self.fixed;
        if let Some((a, b, liq)) = cushion {
            locked += crate::position::pol_base_at_sqrt(self.t, a, b, liq)? as u128;
        }
        let c = self.steps.circulating;
        let sellable = if locked >= c as u128 { 0 } else { c - locked as u64 };
        crate::position::quote_cost_at_sqrt(sellable, self.t)
    }

    /// The need with the cushion holding `quote` over the range at `price`.
    pub fn with_quote(&self, price: u128, quote: u64) -> FloorMathResult<u128> {
        let mut c = None;
        if let Some((a, b)) = self.cushion {
            if quote > 0 {
                let liq = crate::position::position_liquidity_at_sqrt(price, a, b, quote, 0)?;
                if liq != 0 {
                    c = Some((a, b, liq));
                }
            }
        }
        self.need(c)
    }

    /// The need with the cushion holding `quote` over `[lower, upper]` at `price` — the cushion's climb, whose probes
    /// move the range (its edges read per probe; the dear edge's and the classic POL's once).
    pub fn with_range(&self, price: u128, lower: i32, upper: i32, quote: u64) -> FloorMathResult<u128> {
        let a = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(lower)?;
        let b = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(upper)?;
        let liq = crate::position::position_liquidity_at_sqrt(price, a, b, quote, 0)?;
        self.need(if liq != 0 { Some((a, b, liq)) } else { None })
    }
}

/// What the vault can count toward [`backing_need`] at this ratchet, read LOW on purpose: `vault_eth` less the most the
/// ratchet's bounty can take (`unpaid_skim + pending` at `bounty_bps`, capped at `max_bounty`; `pending` is quote about
/// to join `unpaid_skim`) and less the whole `min_bid` (the nudge's reserve is `min(vault, min_bid)`). Never more than
/// the solve will read.
pub fn backing_have(state: &crate::state::FloorState, pending: u64) -> u128 {
    let unpaid = state.unpaid_skim as u128 + pending as u128;
    let bounty = (unpaid * state.bounty_bps as u128 / BPS as u128).min(state.max_bounty as u128);
    (state.vault_eth as u128).saturating_sub(bounty).saturating_sub(state.min_bid as u128)
}

/// FLOOR FIRST (rule 1): the gross quote, out of `collected`, the vault takes BEFORE the two numbers split the rest —
/// the least `x` whose kept part (`x` less the `$FLOOR` cut at `platform_bps`, rounded down as `fees::intake` rounds it)
/// covers `need − have`; 0 when `have` already covers it; at most `collected` (a pump the first ratchet cannot back is
/// accepted: a buy and dump before the ratchet is outside what the floor covers).
pub fn floor_first(collected: u64, need: u128, have: u128, platform_bps: u16) -> u64 {
    if need <= have || collected == 0 {
        return 0;
    }
    let short = need - have;
    let keep = BPS as u128 - (platform_bps as u128).min(BPS as u128 - 1);
    // ceil(short · 10,000 / keep): x − floor(x · bps / 10,000) ≥ x · keep / 10,000 ≥ short
    let x = (short * BPS as u128).div_ceil(keep);
    if x >= collected as u128 { collected } else { x as u64 }
}

/// FLOOR FIRST, COUNTING THE CUSHION AS IT WILL STAND: the LEAST `k` in `[0, collected]` the vault keeps
/// before the split such that `surplus(k) ≥ 0` — `surplus(k)` being what the vault will hold less what the solve at the
/// published floor needs, with the cushion counted as it WILL stand: its kept quote PLUS the split's share of
/// `collected − k`, over its new range. ([`floor_first`] counts only the kept quote, so it over-takes: the surplus raises
/// `floor_tick` instead of standing in the cushion.) A lamport kept moves from the cushion's share to the vault, so the
/// test is monotone in `k` — but NOT linear: the bounty's cap (`min(unpaid · bps, max_bounty)`) bends it.
/// ⛔ THE BRACKET (an end-to-end secant can fall a few
/// lamports short four times on the bend, and fell back to the WHOLE collection — no cushion at all, and a 3 % sell then
/// dropped 55 %): `lo` = the largest `k` found short, `hi` = the smallest found backed (the whole collection to start),
/// refined by Illinois-secant probes, at most [`FLOOR_FIRST_PROBES`]; always `hi`, so the answer is backed and never the
/// whole collection unless that is the only backed value found. `surplus(0) ≥ 0` → 0 (the split alone backs it). A vault
/// lamport that backs no more than a cushion lamport (`surplus(collected) ≤ surplus(0)`) → 0: keeping more cannot help.
/// `surplus(collected) < 0` → `collected` (a pump the first ratchet cannot back).
pub fn floor_first_least(collected: u64, mut surplus: impl FnMut(u64) -> FloorMathResult<i128>) -> FloorMathResult<u64> {
    if collected == 0 {
        return Ok(0);
    }
    let f0 = surplus(0)?;
    if f0 >= 0 {
        return Ok(0);
    }
    let fc = surplus(collected)?;
    if fc <= f0 {
        return Ok(0);
    }
    if fc < 0 {
        return Ok(collected);
    }
    // f(lo) < 0 ≤ f(hi); Illinois: the end kept twice in a row has its value halved, so neither end stalls
    let (mut lo, mut flo, mut hi, mut fhi) = (0u64, f0, collected, fc);
    let mut kept_side = 0i8;
    let mut bisect = false;
    for _ in 0..FLOOR_FIRST_PROBES {
        if hi - lo <= 1 {
            break;
        }
        let width = (hi - lo) as i128;
        let span = fhi - flo; // ≥ 1: flo ≤ −1 and fhi ≥ 0 always (the halvings below keep them so)
        // a secant probe, or — after one that did not at least halve the bracket (a flat stretch) — the midpoint
        let step = if bisect { width / 2 } else { (-flo).checked_mul(width).map(|n| (n + span - 1) / span).unwrap_or(width / 2) };
        let k = lo + (step.clamp(1, width - 1) as u64);
        let f = surplus(k)?;
        if f >= 0 {
            hi = k;
            fhi = f;
            if kept_side == -1 {
                flo = (flo / 2).min(-1);
            }
            kept_side = -1; // lo kept
        } else {
            lo = k;
            flo = f;
            if kept_side == 1 {
                fhi /= 2;
            }
            kept_side = 1; // hi kept
        }
        bisect = !bisect && ((hi - lo) as i128) * 2 > width;
    }
    Ok(hi)
}

/// The most probes [`floor_first_least`] spends between its two ends (compute: a solve-cost question each).
pub const FLOOR_FIRST_PROBES: u32 = 10;

/// THE CUSHION CLIMBS ONLY AS FAR AS THE FLOOR STAYS BACKED (rule 2). The cushion stands with its dear edge (its lower tick) at
/// `current_lower` and its target (under `min(spot, TWAP)`) starts at `target_lower`; both on the grid. A target at or
/// cheaper than where it stands (`target_lower ≥ current_lower`) is returned as is: a cheaper cushion would buy MORE
/// tokens at the floor, so it never un-backs it. A DEARER target is taken when `backed(target_lower)`; else the dearest
/// grid line strictly between the two that is `backed`, by bisection (`backed` is monotone: the same quote over a
/// cheaper range locks more tokens at the floor); else `current_lower` — the cushion stays and catches up at a later
/// ratchet. `backed(lower)` is asked about the range [`cushion_range`] draws from that lower edge.
pub fn cushion_climb(
    target_lower: i32,
    current_lower: i32,
    spacing: i32,
    mut backed: impl FnMut(i32) -> FloorMathResult<bool>,
) -> FloorMathResult<i32> {
    if target_lower >= current_lower || backed(target_lower)? {
        return Ok(target_lower);
    }
    // backed(lo) is false; hi is the fallback (where it stands)
    let (mut lo, mut hi) = (target_lower, current_lower);
    while hi - lo > spacing {
        let mid = floor_to_spacing(lo + (hi - lo) / 2, spacing)?;
        let mid = if mid <= lo { lo + spacing } else { mid };
        if backed(mid)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

/// ⛔ THE OPEN'S FIXPOINT — team steps: a POL bid never sits under the floor, including the open cushion. `$FLOOR`
/// opens with half its raise in the cushion; the rule "a bid's bottom ≥ the floor's band top" makes the cushion's cheap
/// edge follow the floor, and the floor in turn rises as the cushion is pushed up to the price — iterating the two
/// reaches their fixpoint (≈ 96 % of the price at the open). This is that fixpoint in one search: the DEAREST grid floor `f` in
/// `[dearest, cheapest]` whose band still leaves the cushion a range above it (`range(f)`) and that `backed(f, range)`
/// accepts — the published floor's backing with the cushion standing over `range(f)`. `backed` is monotone in `f` (true
/// from some line on, cheaper being easier). `None` when even `cheapest` is not backed (the caller keeps the plain path).
pub fn open_fixpoint_floor(
    dearest: i32,
    cheapest: i32,
    spacing: i32,
    mut range: impl FnMut(i32) -> FloorMathResult<Option<(i32, i32)>>,
    mut backed: impl FnMut(i32, (i32, i32)) -> FloorMathResult<bool>,
) -> FloorMathResult<Option<i32>> {
    let mut ok = |f: i32| -> FloorMathResult<bool> {
        match range(f)? {
            Some(r) => backed(f, r),
            None => Ok(false),
        }
    };
    if dearest > cheapest || !ok(cheapest)? {
        return Ok(None);
    }
    if ok(dearest)? {
        return Ok(Some(dearest));
    }
    // `lo` not backed, `hi` backed
    let (mut lo, mut hi) = (dearest, cheapest);
    while hi - lo > spacing {
        let mid = floor_to_spacing(lo + (hi - lo) / 2, spacing)?;
        let mid = if mid <= lo { lo + spacing } else { mid };
        if ok(mid)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(Some(hi))
}

/// THE AIMED CLIMB: [`cushion_climb`] with its first probe AIMED and a gallop from there —
/// the same answer (the dearest grid line in `[target, current]` that `backed` accepts, `current` when none is; `backed`
/// monotone: true from some line on), in a few probes where the bisection takes up to ten. `aim` is a guess only
/// ([`BackingNeed::climb_aim`]); every answer is settled by `backed` itself.
pub fn cushion_climb_aimed(
    target_lower: i32,
    current_lower: i32,
    spacing: i32,
    aim: Option<i32>,
    mut backed: impl FnMut(i32) -> FloorMathResult<bool>,
) -> FloorMathResult<i32> {
    if target_lower >= current_lower {
        return Ok(target_lower);
    }
    let Some(g) = aim else {
        return cushion_climb(target_lower, current_lower, spacing, backed);
    };
    // `lo` not backed (once `lo_out`), `hi` backed (or `current`, the fallback, never probed)
    let (mut lo, mut hi) = (target_lower, current_lower);
    let mut lo_out = false;
    let g = floor_to_spacing(g, spacing)?.clamp(target_lower, current_lower);
    let down = if g <= lo {
        if backed(lo)? {
            return Ok(lo);
        }
        lo_out = true;
        false
    } else if g >= hi {
        true
    } else if backed(g)? {
        hi = g;
        true
    } else {
        lo = g;
        lo_out = true;
        false
    };
    let mut step = spacing;
    // ⚠ at most GALLOP probes: a far-off aim costs no more than that over the bisection that finishes the bracket
    for _ in 0..crate::solve::GALLOP {
        let t = if down { hi - step } else { lo + step };
        if t <= lo || t >= hi {
            break;
        }
        if backed(t)? {
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
    if !lo_out {
        // the target itself is still unread: it is the answer when backed
        if backed(lo)? {
            return Ok(lo);
        }
    }
    while hi - lo > spacing {
        let mid = floor_to_spacing(lo + (hi - lo) / 2, spacing)?;
        let mid = if mid <= lo { lo + spacing } else { mid };
        if backed(mid)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

impl BackingNeed {
    /// The climb's AIM — the cushion line `l` from which its range `[l, min(l + width, dear)]`,
    /// holding `quote` all in quote under the market, locks enough at the floor's dear edge for `have` to back it. At the
    /// dear edge the cushion holds `quote·Pa·Pb` tokens (P the edges' sqrt prices), so `l` solves `quote·Pa·Pb = K` with
    /// `K = circulating − the rest locked − have·T²` (T the dear edge's): a square root unclipped (`Pb = Pa·1.0001^(w/2)`),
    /// a division clipped (`Pb = T`). A GUESS in f64: [`cushion_climb_aimed`]'s probes decide.
    pub fn climb_aim(&self, have: u128, quote: u64, dear: i32, width: i32, spacing: i32) -> Option<i32> {
        const Q64F: f64 = 18_446_744_073_709_551_616.0;
        let tf = self.t as f64 / Q64F;
        let k = self.steps.circulating as f64 - self.fixed as f64 - have as f64 * tf * tf;
        if k <= 0.0 {
            return Some(MIN_TICK); // backed without the cushion: the target
        }
        if quote == 0 {
            return None;
        }
        let qf = quote as f64;
        let wv = ceil_to_spacing(width, spacing).ok()?;
        let r = carpenter_clmm_math::tick_math::get_sqrt_price_at_tick(wv).ok()? as f64 / Q64F; // 1.0001^(w/2)
        let tick_of = |p: f64| -> Option<i32> {
            let sp = p * Q64F;
            if !(sp >= carpenter_clmm_math::tick_math::MIN_SQRT_PRICE_X64 as f64) {
                return Some(MIN_TICK);
            }
            if sp >= carpenter_clmm_math::tick_math::MAX_SQRT_PRICE_X64 as f64 {
                return Some(MAX_TICK);
            }
            get_tick_at_sqrt_price(sp as u128).ok().map(|t| t + 1)
        };
        let l = tick_of(crate::solve::sqrt_f64(k / (qf * r)))?;
        if l.saturating_add(wv) <= dear {
            return Some(l);
        }
        tick_of(k / (qf * tf))
    }
}

/// ⛔ NO PROTOCOL LISTING WITHIN ~3% ABOVE THE FLOOR (the EVM twin's
/// `FloorPolicy.STAIR_CLEAR_TICKS` / `_clearOfAsks`): the least ticks between the bid band's DEAR edge and the staircase's
/// cheapest ask after a ratchet. 254 = ceil(ln(10,000 / 9,750) / ln 1.0001): an ask that far above the dear edge sells for
/// at least the dear-edge price over (1 − the 2.5 % cut), so the vault keeps at least the dear-edge price per token it
/// lets into circulation.
pub const STAIR_CLEAR_TICKS: i32 = 254;

/// The dearest floor tick whose bid band (`band_ticks` wide) stands [`STAIR_CLEAR_TICKS`] clear of an ask whose cheap
/// edge is `ask`: `ceil_to_spacing(ask + STAIR_CLEAR_TICKS + band_ticks)`.
pub fn clear_of_ask(ask: i32, band_ticks: i32, spacing: i32) -> FloorMathResult<i32> {
    let t = (ask as i64 + STAIR_CLEAR_TICKS as i64 + band_ticks as i64).min(i32::MAX as i64) as i32;
    ceil_to_spacing(t, spacing)
}

/// ⛔ THE CUSHION'S COST BASIS: what the cushion bought from sellers never re-lists cheaper than 1.03 × what it paid
/// (the EVM twin's `STAIR_RELIST_COST_BPS` 10,300). A higher markup parks the cushion's tokens above a flat market and
/// never sells them back; 1.03 × is the highest markup measured that still trades in quiet flow. In ticks: 296 =
/// ceil(ln 1.03 / ln 1.0001)
/// (1.0001^296 ≈ 1.03004), so a line this many ticks DEARER than the cost line sells at no less than 1.03 × the cost.
pub const RELIST_COST_MARKUP_TICKS: i32 = 296;

/// The ORIGINAL cost line of `tokens` the cushion paid `cost` lamports for, in ticks: the tick of the token price
/// `cost / tokens` (`get_tick_at_sqrt_price` floors it, so the line's own price is at or above the cost). `None` when
/// either is 0 (nothing bought, or bought for nothing).
pub fn cushion_cost_tick(tokens: u64, cost: u128) -> FloorMathResult<Option<i32>> {
    if tokens == 0 || cost == 0 {
        return Ok(None);
    }
    let sqrt = sqrt_price_from_ratio(cost, tokens as u128)?; // √(tokens / cost): base per quote at the cost
    Ok(Some(get_tick_at_sqrt_price(sqrt)?))
}

/// The re-list's cheapest grid line under a cost line (`cushion_cost_tick`): 1.03 × the cost
/// (`RELIST_COST_MARKUP_TICKS` dearer), floored to the spacing (dearer still).
pub fn relist_cost_line(cost_tick: i32, spacing: i32) -> FloorMathResult<i32> {
    floor_to_spacing(cost_tick.saturating_sub(RELIST_COST_MARKUP_TICKS), spacing)
}

/// The cushion's range from its LOWER tick (its DEAR edge, the one next to the price) on the grid: `[lower, lower + ceil(1,054)]`,
/// the shape [`cushion_range`] draws (`cushion_range(lower − 1)`).
pub fn cushion_range_from(lower: i32, spacing: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_from_w(lower, spacing, CUSHION_TICKS)
}

/// [`cushion_range_from`] `width` ticks deep.
pub fn cushion_range_from_w(lower: i32, spacing: i32, width: i32) -> FloorMathResult<Option<(i32, i32)>> {
    cushion_range_w(lower.checked_sub(1).ok_or(FloorMathError::Overflow)?, spacing, width)
}

#[cfg(test)]
mod tests {
    /// THE OPEN'S FIXPOINT: the dearest backed grid floor, found by bisection; `None` when even the cheapest
    /// fails; the range rule refuses a floor with no room for the cushion.
    #[test]
    fn the_open_fixpoint_is_the_dearest_backed_floor() {
        let range = |f: i32| -> FloorMathResult<Option<(i32, i32)>> { Ok(if f > 1_000 { Some((0, f - 120)) } else { None }) };
        for line in [1_060, 1_120, 2_000, 3_000, 4_980, 5_000] {
            let mut probes = 0;
            let got = open_fixpoint_floor(1_000, 5_000, 60, range, |f, _| {
                probes += 1;
                Ok(f >= line)
            })
            .unwrap();
            let want = floor_to_spacing(line + 59, 60).unwrap().max(1_020);
            assert_eq!(got, Some(want.min(5_000)), "line {line}");
            assert!(probes <= 9, "{probes} probes");
        }
        assert_eq!(open_fixpoint_floor(1_000, 5_000, 60, range, |_, _| Ok(false)).unwrap(), None);
        assert_eq!(open_fixpoint_floor(1_000, 5_000, 60, range, |_, _| Ok(true)).unwrap(), Some(1_020));
    }

    /// THE PAYEE LEAVES THE SOLVE'S RESERVE — the EVM twin's `TeamSteps::test_teamPay_leavesTheSolvesReserve`.
    /// `backing_have` never counts the `min_bid` reserve (nor the bounty a ratchet may
    /// take), so when the floor needs the whole vault FLOOR FIRST keeps at least the reserve out of the collection and the
    /// payee is paid only from what is left: the reserve is never paid out. Mutations this kills: `min_bid` dropped from
    /// `backing_have`; the payee paid before floor first; `keep` rounded down below the reserve.
    #[test]
    fn the_payee_never_takes_the_solves_reserve() {
        let mut st = crate::state::FloorState::default();
        st.vault_eth = 5_000_000_000;
        st.min_bid = 50_000_000;
        st.bounty_bps = 100;
        st.max_bounty = 50_000_000;
        assert_eq!(backing_have(&st, 0), (st.vault_eth - st.min_bid) as u128, "no skim: the vault less the reserve");
        st.unpaid_skim = 1_000_000_000; // 1 % = 0.01 SOL of possible bounty
        assert_eq!(backing_have(&st, 0), (st.vault_eth - st.min_bid - 10_000_000) as u128, "…and less the bounty");
        st.unpaid_skim = 0;
        // the floor needs the whole vault: the shortfall is the reserve, grossed up for the cut
        let need = st.vault_eth as u128;
        for collected in [10_000_000u64, 51_282_052, 200_000_000, 1_000_000_000] {
            let keep = floor_first(collected, need, backing_have(&st, 0), crate::policy::PLATFORM_BPS);
            let kept_net = keep - (keep as u128 * crate::policy::PLATFORM_BPS as u128 / BPS as u128) as u64;
            assert!(kept_net >= st.min_bid.min(collected - collected / 40), "{collected}: keep {keep} backs the reserve");
            let (paid, forfeited) = crate::owners::payee_after_floor_first(collected, collected, keep);
            assert!(paid <= collected - keep, "{collected}: the payee is paid only out of what floor first left");
            assert_eq!(paid + forfeited, collected);
        }
    }

    /// ⛔ THE CUSHION'S COST BASIS: the cost line prices the tokens at or above what the cushion paid, and the re-list's
    /// line sells them at no less than 1.03 × that.
    #[test]
    fn h1_the_cost_line_and_the_relists_line_at_one_point_zero_three_x() {
        assert_eq!(cushion_cost_tick(0, 5).unwrap(), None);
        assert_eq!(cushion_cost_tick(5, 0).unwrap(), None);
        for (tokens, cost) in [(1_000_000_000_000u64, 3_000_000u128), (7_777_777_777, 123_456_789), (10u64.pow(15), 10u128.pow(12))] {
            let c = cushion_cost_tick(tokens, cost).unwrap().unwrap();
            // the line's own token price (lamports per base unit) is at or above the cost per token
            let paid = crate::position::quote_cost(tokens, c).unwrap();
            assert!(paid + 2 >= cost, "the cost line never prices them under what was paid: {paid} < {cost}");
            let line = relist_cost_line(c, 60).unwrap();
            assert_eq!(line % 60, 0);
            let at_line = crate::position::quote_cost(tokens, line).unwrap();
            assert!(at_line * 100 >= cost * 103, "the re-list's line sells at >= 1.03 × the cost: {at_line} vs {cost}");
        }
    }

    use super::*;
    use crate::constants::TICKS_PER_DOUBLING;
    use crate::modes::{preset, tranche_price_x64, Mode};
    use crate::price::open_tick;
    use carpenter_clmm_math::tick_math::get_sqrt_price_at_tick;

    const S: i32 = 60;

    #[test]
    fn m1_any_climb_aim_is_the_climb() {
        // every threshold (none backed, all backed, each line), every aim: the aimed climb is cushion_climb
        let s = 60;
        let (target, current) = (12_000, 18_000);
        let mut n = 0;
        for knee in (target - 120..=current + 120).step_by(60) {
            let backed = |l: i32| -> FloorMathResult<bool> { Ok(l >= knee) };
            let want = cushion_climb(target, current, s, backed).unwrap();
            for aim in [None, Some(target - 600), Some(current + 600), Some(MIN_TICK)].into_iter().chain((target..=current).step_by(60).map(Some)) {
                assert_eq!(cushion_climb_aimed(target, current, s, aim, backed).unwrap(), want, "knee {knee} aim {aim:?}");
                n += 1;
            }
        }
        assert!(n > 5_000);
    }

    #[test]
    fn the_supply_is_twenty_ten_sixty_nine_one() {
        let (sale, pol, steps, tail) = STAIR_SUPPLY_TOKENS;
        assert_eq!(sale + pol + steps + tail, SUPPLY);
        assert_eq!((sale, pol, steps, tail), (200_000_000_000_000, 100_000_000_000_000, 690_000_000_000_000, 10_000_000_000_000));
        // graduation deposits the staircase and the tail together (the 70 % `split_tokens` calls the wall)
        assert_eq!(stair_tail_split(steps + tail), (steps, tail), "the exact supply split, to the unit");
        let (s, t) = stair_tail_split(699_999_999_999_997);
        assert_eq!(s + t, 699_999_999_999_997, "conserved at any deposit");
    }

    #[test]
    fn step_tokens_sum_exactly_per_preset() {
        let total = STAIR_SUPPLY_TOKENS.2;
        for (m, steps, growth) in [(Mode::Moonshot, 15u8, 10_000u16), (Mode::Cook, 15, 10_400), (Mode::Bluechip, 14, 10_400)] {
            let p = preset(m);
            assert_eq!((p.stair_steps, p.stair_growth_bps), (steps, growth), "{m:?}'s row");
            let t = step_tokens(total, steps, growth).unwrap();
            assert_eq!(t.iter().sum::<u64>(), total, "{m:?}: the steps hold the staircase exactly");
            assert!(t[steps as usize..].iter().all(|&x| x == 0), "{m:?}: nothing past the last step");
            if growth == BPS as u16 {
                let each = total / steps as u64;
                for (i, &x) in t[..steps as usize].iter().enumerate() {
                    assert!(x.abs_diff(each) < steps as u64, "{m:?} step {i}: {x} vs {each}");
                }
            } else {
                let g = growth as f64 / BPS as f64;
                for i in 1..steps as usize {
                    assert_eq!(t[i] > t[i - 1], g > 1.0, "{m:?}: heavier up (Cook) or down (Bluechip)");
                    let ratio = t[i] as f64 / t[i - 1] as f64;
                    assert!((ratio - g).abs() < 1e-9, "{m:?} step {i}: ratio {ratio}");
                }
            }
        }
        // hand values: thirteen equal steps of 690e12 (Moonshot's weighting)
        let eq = step_tokens(total, 13, 10_000).unwrap();
        assert_eq!(eq[1], 53_076_923_076_923);
        assert_eq!(eq[0], 53_076_923_076_923 + 1, "the residue on step 0");
        // Cook's first step: 690e12 · 1 / Σ 1.04^j (j < 13) = 690e12 / 16.6268…; its last 1.04^12 ≈ 1.601 × that
        let cook = step_tokens(total, 13, 10_400).unwrap();
        let sum: f64 = (0..13).map(|j| 1.04f64.powi(j)).sum();
        assert!((cook[0] as f64 - total as f64 / sum).abs() < 20.0);
        assert!((cook[12] as f64 / cook[0] as f64 - 1.04f64.powi(12)).abs() < 1e-9);
        // Bluechip's first step: 690e12 · 1 / Σ 0.988^j (j < 11)
        let blue = step_tokens(total, 11, 9_880).unwrap();
        let sum: f64 = (0..11).map(|j| 0.988f64.powi(j)).sum();
        assert!((blue[0] as f64 - total as f64 / sum).abs() < 20.0);
        assert!(step_tokens(total, 0, 10_000).is_err() && step_tokens(total, 17, 10_000).is_err());
        assert!(step_tokens(total, 8, 4_999).is_err() && step_tokens(total, 8, 20_001).is_err());
        assert_eq!(step_tokens(0, 8, 9_500).unwrap(), [0; 16]);
        assert_eq!(step_tokens(u64::MAX, 16, 20_000).unwrap().iter().map(|&x| x as u128).sum::<u128>(), u64::MAX as u128);
    }

    /// The compute shortcut changes no value: the table IS the formula, entry for entry, and so is the cushion's constant.
    #[test]
    fn the_doubling_table_is_the_formula() {
        for i in 0..=MAX_STAIR_STEPS {
            assert_eq!(DOUBLING_TICKS[i as usize], doubling_ticks_computed(i).unwrap(), "2^{i}");
            assert_eq!(doubling_ticks(i).unwrap(), doubling_ticks_computed(i).unwrap());
        }
        assert_eq!(doubling_ticks(17).unwrap(), doubling_ticks_computed(17).unwrap());
        assert_eq!(CUSHION_TICKS, cushion_ticks_computed().unwrap());
    }

    #[test]
    fn doubling_ticks_are_the_ceiled_log() {
        assert_eq!(doubling_ticks(0).unwrap(), 0);
        assert_eq!(doubling_ticks(1).unwrap(), TICKS_PER_DOUBLING);
        for i in 1u8..=16 {
            let want = (i as f64 * 2f64.ln() / 1.0001f64.ln()).ceil() as i32;
            assert_eq!(doubling_ticks(i).unwrap(), want, "2^{i}");
        }
    }

    #[test]
    fn the_steps_double_the_price_and_tile_without_gaps() {
        // a Cook launch at its cap against SOL at $200 (9-decimal quote, 6-decimal base); every preset's step count
        use crate::modes as m;
        let open = open_tick(tranche_price_x64(m::COOK_GRADUATION_MCAP_USD, 200 * 100, 9).unwrap()).unwrap();
        let open_edge = floor_to_spacing(open, S).unwrap();
        assert_eq!((m::MOONSHOT_STAIR_STEPS, m::COOK_STAIR_STEPS, m::BLUECHIP_STAIR_STEPS), (15, 15, 14));
        for (steps, name) in [(m::MOONSHOT_STAIR_STEPS, "Moonshot"), (m::COOK_STAIR_STEPS, "Cook"), (m::BLUECHIP_STAIR_STEPS, "Bluechip")] {
            assert!(stair_fits(open_edge, steps, S).unwrap(), "{name} fits");
            let mut prev_lower = open_edge;
            for i in 0..steps {
                let (lo, hi) = step_range(open_edge, i, steps, S).unwrap();
                assert_eq!(hi, prev_lower, "{name} step {i} starts where the last ended");
                assert!(lo < hi && lo % S == 0 && hi % S == 0);
                // one doubling of the token price, give or take the grid
                let ratio = 1.0001f64.powi(hi - lo);
                assert!((ratio - 2.0).abs() < 0.02, "{name} step {i}: ×{ratio}");
                // and the cheap edge sits at open · 2^i, within one spacing
                let exact = open_edge as f64 - i as f64 * 2f64.ln() / 1.0001f64.ln();
                assert!((hi as f64 - exact).abs() <= S as f64, "{name} step {i}: {hi} vs {exact}");
                prev_lower = lo;
            }
            let (tl, tu) = tail_range(open_edge, steps, S).unwrap();
            assert_eq!(tu, prev_lower, "{name}: the tail starts at the staircase's top");
            assert_eq!(tl, grid_bottom(S).unwrap());
            assert!(step_range(open_edge, steps, steps, S).is_err());
        }
        // a staircase that cannot fit is refused by name, never clamped
        let near_top = grid_bottom(S).unwrap() + ceil_to_spacing(doubling_ticks(11).unwrap(), S).unwrap();
        assert!(!stair_fits(near_top, 11, S).unwrap());
        assert!(stair_fits(near_top + S, 11, S).unwrap());
    }

    #[test]
    fn a_partly_bought_step_keeps_only_what_is_dearer_than_the_market() {
        let (lo, hi) = (60_000, 66_960);
        assert_eq!(unsold_range(lo, hi, 70_000, S).unwrap(), Some((lo, hi)), "not entered: whole");
        assert_eq!(unsold_range(lo, hi, 63_001, S).unwrap(), Some((lo, 63_000)), "entered: the grid line at or dearer than the market");
        assert_eq!(unsold_range(lo, hi, 62_999, S).unwrap(), Some((lo, 62_940)));
        assert_eq!(unsold_range(lo, hi, 62_940, S).unwrap(), Some((lo, 62_940)));
        assert_eq!(unsold_range(lo, hi, 60_059, S).unwrap(), None, "sold through: nothing left");
        assert_eq!(unsold_range(lo, hi, 50_000, S).unwrap(), None);
    }

    #[test]
    fn the_cushion_sits_from_nine_tenths_of_the_price_to_the_price() {
        assert_eq!(cushion_ticks().unwrap(), 1_054);
        for cur in [-200_000, -61, -60, 0, 1, 59, 60, 82_946, 99_039] {
            let (lo, hi) = cushion_range(cur, S).unwrap().unwrap();
            assert!(lo > cur && lo - cur <= S, "{cur}: the first grid line cheaper than the price");
            assert_eq!(hi - lo, 1_080, "ceil_to_spacing(1,054)");
            // all quote at the market: a quote-only position
            let p = get_sqrt_price_at_tick(cur).unwrap();
            let (q, b) = crate::position::position_amounts(p, lo, hi, 1_000_000_000_000).unwrap();
            assert!(q > 0 && b == 0, "{cur}: all quote");
            // its cheap edge is at most 0.9 × the price
            assert!(1.0001f64.powi(-(hi - cur)) <= 0.9 + 1e-9);
        }
        let top = floor_to_spacing(MAX_TICK, S).unwrap();
        assert_eq!(cushion_range(top, S).unwrap(), None, "no room above the top of the grid");
    }

    /// THE CUSHION FOLLOWS THE PRICE: re-placed under each new price, its range moves with the market — up a climb and
    /// back down a fall — always strictly cheaper than the market (all quote), never left behind below 0.9 × it.
    #[test]
    fn the_cushion_is_re_placed_under_each_new_price() {
        let mut last: Option<(i32, i32)> = None;
        for cur in (40_000..=90_000).rev().step_by(3_337) {
            let (lo, hi) = cushion_range(cur, S).unwrap().unwrap();
            assert!(lo > cur, "{cur}: under the price");
            assert!(1.0001f64.powi(-(hi - cur)) <= 0.9 + 1e-9 && 1.0001f64.powi(-(hi - cur)) > 0.88, "{cur}: to about 0.9 x");
            if let Some((plo, phi)) = last {
                assert!(lo < plo && hi < phi, "a dearer market moves the cushion dearer");
            }
            last = Some((lo, hi));
        }
        // and after a fall it sits under the lower price, never above the market
        let (lo, _) = cushion_range(95_000, S).unwrap().unwrap();
        assert!(lo > 95_000);
    }

    /// ⛔ THE REGRESSION: pump through the steps, ratchet, dump into the cushion — all in one transaction. Under the spot
    /// the ratchet would have raised the cushion to the
    /// pumped price; under `min(spot, TWAP)` (the TWAP unmoved by a same-transaction pump) it stays exactly where it
    /// was, so the dump meets bids at the settled price, never the pumped one. A genuine climb (the TWAP following)
    /// still moves it up, and a fall (spot under the TWAP) puts it under the spot.
    #[test]
    fn a_pump_ratchet_dump_in_one_transaction_does_not_move_the_cushion_up() {
        let settled = 80_000; // the TWAP, and the price before the pump
        let before = cushion_range_at(settled, settled, S).unwrap().unwrap();
        for pumped in [79_000, 73_068, 60_000, 40_000] {
            // the pump moved the spot dearer (lower tick); the TWAP, written before the pump, did not move
            let naive = cushion_range(pumped, S).unwrap().unwrap();
            assert!(naive.0 < before.0, "the spot-only rule would have raised it (the hole)");
            let fixed = cushion_range_at(pumped, settled, S).unwrap().unwrap();
            assert_eq!(fixed, before, "pumped to {pumped}: the cushion does not move up");
            // and it is still all quote at the pumped spot
            assert!(fixed.0 > pumped);
        }
        // the TWAP catching up (a real climb) raises it
        let climbed = cushion_range_at(70_000, 70_060, S).unwrap().unwrap();
        assert!(climbed.0 < before.0 && climbed.0 > 70_060);
        // a fall: the spot is cheaper than the TWAP, the cushion sits under the spot
        let fell = cushion_range_at(85_000, settled, S).unwrap().unwrap();
        assert_eq!(fell, cushion_range(85_000, S).unwrap().unwrap());
        assert_eq!(cushion_top_tick(85_000, settled), 85_000);
    }

    #[test]
    fn the_relist_sits_from_the_price_to_twice_the_price() {
        for cur in [-100_000, -1, 0, 59, 82_946] {
            let (lo, hi) = relist_range(cur, S).unwrap().unwrap();
            assert!(hi <= cur && cur - hi < S);
            assert_eq!(hi - lo, 6_960);
            let p = get_sqrt_price_at_tick(cur).unwrap();
            let (q, b) = crate::position::position_amounts(p, lo, hi, 1_000_000_000_000).unwrap();
            assert!(q == 0 && b > 0, "{cur}: all base");
        }
        let bottom = grid_bottom(S).unwrap();
        assert_eq!(relist_range(bottom + 10, S).unwrap(), None);
        assert_eq!(relist_range(bottom + 100, S).unwrap(), Some((bottom, bottom + 60)), "clamped at the top of the range");
    }

    fn sqrt_at_price_multiple(base: i32, m: f64) -> u128 {
        // a token price m × the one at `base` is the tick base − log(m)
        get_sqrt_price_at_tick(base - (m.ln() / 1.0001f64.ln()).round() as i32).unwrap()
    }

    #[test]
    fn the_split_follows_the_two_shortfalls() {
        let floor_tick = 90_000;
        let f = get_sqrt_price_at_tick(floor_tick).unwrap();
        let supply = SUPPLY;
        // the price at 10 × the floor: floor ÷ price = 10 %, half the target → shortF = 0.5
        let p = sqrt_at_price_multiple(floor_tick, 10.0);
        let r = floor_to_price(p, f).unwrap();
        assert!((r as f64 / SPLIT_SCALE as f64 - 0.1).abs() < 1e-4, "{r}");
        let sf = shortfall(r, FLOOR_TO_PRICE_BPS);
        assert!((sf as f64 / SPLIT_SCALE as f64 - 0.5).abs() < 1e-3);
        // market cap at that price, in raw quote: supply / P
        let mcap = {
            let p2 = U256::from(p) * U256::from(p);
            ((U256::from(supply) << 128usize) / p2).as_u128() as f64
        };
        // POL quote at 7.5 % of the market cap (half the 15 % target) → shortC = 0.5 → an even split
        let pol_q = (mcap * 0.075) as u64;
        let sp = stair_split(1_000_000, p, f, pol_q, supply).unwrap();
        assert!((sp.short_pol as f64 / SPLIT_SCALE as f64 - 0.5).abs() < 1e-3);
        assert!((sp.to_pol as i64 - 500_000).abs() < 1_000, "{sp:?}");
        assert_eq!(sp.to_pol + sp.to_floor, 1_000_000, "never over-allocates, never loses a unit");
        // POL met (≥ 15 %), floor short → all to the floor
        let sp = stair_split(1_000_000, p, f, (mcap * 0.151) as u64, supply).unwrap();
        assert_eq!((sp.short_pol, sp.to_pol, sp.to_floor), (0, 0, 1_000_000));
        // 12 % is SHORT against the 15 % target: 1 − 12/15 of the way
        let sp = stair_split(1_000_000, p, f, (mcap * 0.12) as u64, supply).unwrap();
        assert!((sp.short_pol as f64 / SPLIT_SCALE as f64 - 0.2).abs() < 1e-3, "{sp:?}");
        // both met (floor ÷ price ≥ 20 %, POL ≥ 15 %) → all to the floor
        let near = sqrt_at_price_multiple(floor_tick, 4.0);
        let sp = stair_split(777, near, f, u64::MAX / 2, supply).unwrap();
        assert_eq!((sp.short_floor, sp.short_pol, sp.to_pol, sp.to_floor), (0, 0, 0, 777), "both met: all to the floor");
        // floor met, POL empty → all to the POL
        let sp = stair_split(777, near, f, 0, supply).unwrap();
        assert_eq!((sp.short_floor, sp.short_pol, sp.to_pol, sp.to_floor), (0, SPLIT_SCALE, 777, 0));
        // no floor yet (sqrt 0) → the floor is fully short
        let sp = stair_split(1_000, p, 0, 0, supply).unwrap();
        assert_eq!((sp.short_floor, sp.short_pol), (SPLIT_SCALE, SPLIT_SCALE));
        assert_eq!(sp.to_pol, 500, "both fully short: half each, rounded down to the POL");
        // rounding: toPol is floored, never above collected · shortC / (shortF + shortC)
        for c in [0u64, 1, 2, 3, 999, u64::MAX] {
            let sp = stair_split(c, p, f, pol_q, supply).unwrap();
            assert_eq!(sp.to_pol + sp.to_floor, c);
            let exact = c as u128 * sp.short_pol / (sp.short_floor + sp.short_pol);
            assert_eq!(sp.to_pol as u128, exact);
        }
    }

    #[test]
    fn the_ratios_are_bounded_and_monotone() {
        let f = get_sqrt_price_at_tick(90_000).unwrap();
        assert_eq!(floor_to_price(f, f).unwrap(), SPLIT_SCALE, "price on the floor: 100 %");
        assert_eq!(floor_to_price(get_sqrt_price_at_tick(95_000).unwrap(), f).unwrap(), SPLIT_SCALE, "under the floor: capped");
        let mut last = SPLIT_SCALE;
        for t in (0..90_000).rev().step_by(7_000) {
            let r = floor_to_price(get_sqrt_price_at_tick(t).unwrap(), f).unwrap();
            assert!(r <= last);
            last = r;
        }
        assert_eq!(quote_to_mcap(0, f, SUPPLY).unwrap(), 0);
        assert_eq!(quote_to_mcap(u64::MAX, f, 1).unwrap(), SPLIT_SCALE);
        assert_eq!(shortfall(0, 500), SPLIT_SCALE);
        assert_eq!(shortfall(SPLIT_SCALE / 20, 500), 0, "exactly at the 5 % target: met");
        assert_eq!(shortfall(SPLIT_SCALE / 40, 500), SPLIT_SCALE / 2);
    }

    /// TOKEN CONSERVATION across the staircase's lifecycle, on the position math the programs use: place every step
    /// and the tail at the open, walk the market up in uneven jumps, and at each "ratchet" close each entered step
    /// (collecting its quote), re-open what is left of it with `unsold_range`, and account every unit. What the
    /// positions hold + what re-opening left over (held dust) + what buyers took = the staircase exactly; and the
    /// quote collected is what the buyers paid those positions (to rounding: the AMM rounds in the pool's favour).
    #[test]
    fn the_staircase_conserves_every_token_across_ratchets() {
        use crate::position::{position_amounts, position_liquidity};
        let open = open_tick(tranche_price_x64(50_000, 200 * 100, 9).unwrap()).unwrap();
        let open_edge = floor_to_spacing(open, S).unwrap();
        let steps = 11u8;
        let (stair_total, tail_total) = stair_tail_split(STAIR_SUPPLY_TOKENS.2 + STAIR_SUPPLY_TOKENS.3);
        let want = step_tokens(stair_total, steps, 10_000).unwrap();
        let p0 = get_sqrt_price_at_tick(open).unwrap();
        // (lower, upper, liq) per step, placed once at the open
        let mut pos: Vec<(i32, i32, u128)> = Vec::new();
        let mut placed = 0u64;
        for i in 0..steps {
            let (lo, hi) = step_range(open_edge, i, steps, S).unwrap();
            let liq = position_liquidity(p0, lo, hi, 0, want[i as usize]).unwrap();
            let (q, b) = position_amounts(p0, lo, hi, liq).unwrap();
            assert_eq!(q, 0, "step {i} is token-only at the open");
            assert!(b <= want[i as usize] && want[i as usize] - b < 100, "step {i}: placed {b} of {} (liquidity rounds down: dust, held)", want[i as usize]);
            placed += b;
            pos.push((lo, hi, liq));
        }
        let (tl, tu) = tail_range(open_edge, steps, S).unwrap();
        let tail_liq = position_liquidity(p0, tl, tu, 0, tail_total).unwrap();
        let held0 = stair_total - placed; // the placement's rounding: held, never lost
        let mut held = held0;
        let mut collected_total = 0u64;
        let mut first_open = 0usize;
        // the market climbs through the staircase in uneven jumps (ticks), a ratchet after each
        for jump in [3_000, 9_100, 400, 20_000, 7_777, 13_000, 25_000] {
            let cur = pos[first_open.min(pos.len() - 1)].1.min(open) - jump;
            let p = get_sqrt_price_at_tick(cur).unwrap();
            // what buyers took from every step since the last ratchet: tokens before − tokens now
            let mut i = first_open;
            while i < pos.len() {
                let (lo, hi, liq) = pos[i];
                if cur >= hi {
                    break; // not entered: untouched, as the program's loop stops here
                }
                let (q, b) = position_amounts(p, lo, hi, liq).unwrap();
                collected_total += q;
                match unsold_range(lo, hi, cur, S).unwrap() {
                    Some((nlo, nhi)) => {
                        let nliq = position_liquidity(p, nlo, nhi, 0, b).unwrap();
                        let (q2, b2) = position_amounts(p, nlo, nhi, nliq).unwrap();
                        assert_eq!(q2, 0, "a re-opened step is token-only");
                        assert!(b2 <= b);
                        held += b - b2;
                        pos[i] = (nlo, nhi, nliq);
                        break;
                    }
                    None => {
                        held += b; // sold through: its dust is held
                        pos[i] = (lo, hi, 0);
                        first_open = i + 1;
                        i += 1;
                    }
                }
            }
            // conservation at every ratchet
            let standing: u64 = pos.iter().map(|&(lo, hi, liq)| position_amounts(p, lo, hi, liq).unwrap().1).sum();
            let sold: u64 = stair_total - held - standing;
            // the buyers paid at least the collected quote's worth at the steps' prices: sold ≥ 0 and collected > 0
            // once anything sold
            assert!(standing + held <= stair_total);
            if sold > 0 {
                assert!(collected_total > 0, "sold {sold} tokens for no quote");
            }
            assert!(held - held0 < 1_000, "re-opening loses only dust: {}", held - held0);
        }
        assert!(first_open >= 1, "the climb sold through at least one step");
        // the tail was never touched below the staircase's top
        assert_eq!(position_amounts(get_sqrt_price_at_tick(tu + 60).unwrap(), tl, tu, tail_liq).unwrap().0, 0);
    }

    /// ⛔ GRADUATION INSIDE THE BUY: the reserve is the concatenation of the later steps — no gap, no overlap with step 0
    /// or the tail.
    #[test]
    fn the_reserve_spans_every_later_step_exactly() {
        let open = open_tick(tranche_price_x64(200_000, 200 * 100, 9).unwrap()).unwrap();
        let open_edge = floor_to_spacing(open, S).unwrap();
        for n in [2u8, 8, 9, 11] {
            let (lo, hi) = reserve_range(open_edge, 1, n, S).unwrap();
            assert_eq!(hi, step_range(open_edge, 0, n, S).unwrap().0, "{n}: starts where step 0 ends");
            assert_eq!(lo, tail_range(open_edge, n, S).unwrap().1, "{n}: ends where the tail starts");
            for from in 1..n {
                let (l, h) = reserve_range(open_edge, from, n, S).unwrap();
                assert_eq!((l, h), (step_range(open_edge, n - 1, n, S).unwrap().0, step_range(open_edge, from, n, S).unwrap().1));
            }
            assert!(reserve_range(open_edge, n, n, S).is_err());
        }
        assert!(keeps_reserve(1, 3) && !keeps_reserve(1, 2) && !keeps_reserve(2, 2) && keeps_reserve(0, 2));
    }

    /// A single liquidity over the reserve is THICKER LOW: the tokens per doubling fall by 2^(−1/2) a step (base per
    /// doubling ∝ 1/√P_lower − 1/√P_upper), so the reserve holds MORE of its tokens in its cheapest step than the equal
    /// planned split — more depth near the price, never a vacuum.
    #[test]
    fn a_single_liquidity_over_the_reserve_is_thicker_low() {
        use crate::position::{position_amounts, position_liquidity};
        let open = open_tick(tranche_price_x64(200_000, 200 * 100, 9).unwrap()).unwrap();
        let open_edge = floor_to_spacing(open, S).unwrap();
        let n = 9u8;
        let (lo, hi) = reserve_range(open_edge, 1, n, S).unwrap();
        let p0 = get_sqrt_price_at_tick(open).unwrap();
        let liq = position_liquidity(p0, lo, hi, 0, 1_000_000_000_000).unwrap();
        let mut last = u64::MAX;
        for j in 1..n {
            let (l, h) = step_range(open_edge, j, n, S).unwrap();
            let b = position_amounts(p0, l, h, liq).unwrap().1;
            assert!(b < last, "step {j}: {b} < {last}");
            if last != u64::MAX {
                let r = b as f64 / last as f64;
                assert!((r - 0.5f64.sqrt()).abs() < 0.01, "step {j}: ratio {r}");
            }
            last = b;
        }
    }

    /// THE SPLIT conserves tokens and keeps the later steps whole while the reserve can: step j gets its planned share
    /// less what sold out of the reserve, never more than planned; the rest stays.
    #[test]
    fn the_split_conserves_tokens_and_keeps_the_later_steps_planned() {
        let total = STAIR_SUPPLY_TOKENS.2;
        for (n, g) in [(9u8, 10_000u16), (8, 9_500), (11, 10_000), (2, 10_000), (13, 10_400), (11, 9_880), (14, 10_000)] {
            let planned = step_tokens(total, n, g).unwrap();
            let later: u64 = planned[2..n as usize].iter().sum();
            let whole = planned[1] + later;
            // nothing sold: step 1 gets its plan exactly, the rest is the later steps' exactly
            assert_eq!(reserve_split(whole, &planned, 1, n).unwrap(), (planned[1], later), "{n}");
            // some sold out of the reserve: the deficit falls on step 1
            let sold = planned[1] / 3;
            assert_eq!(reserve_split(whole - sold, &planned, 1, n).unwrap(), (planned[1] - sold, later));
            // more than step 1's share sold (a single L is thicker low): step 1 gets nothing, the reserve keeps the rest
            if n > 2 {
                let (a, b) = reserve_split(later - 5, &planned, 1, n).unwrap();
                assert_eq!((a, b), (0, later - 5));
            }
            // an excess (the vault's listed tokens rode in the reserve) stays in the reserve, step 1 capped at its plan
            let (a, b) = reserve_split(whole + 777, &planned, 1, n).unwrap();
            assert_eq!((a, b), (planned[1], later + 777));
            for left in [0u64, 1, whole / 2, whole, u64::MAX] {
                let (a, b) = reserve_split(left, &planned, 1, n).unwrap();
                assert_eq!(a + b, left, "conserved");
                assert!(a <= planned[1]);
            }
            // the last step has no later steps: it takes up to its plan, the excess stays
            assert_eq!(reserve_split(planned[n as usize - 1] + 9, &planned, n - 1, n).unwrap(), (planned[n as usize - 1], 9));
            assert!(reserve_split(1, &planned, n, n).is_err());
        }
    }

    /// THE VAULT'S BOUGHT-BACK TOKENS go to the staircase:
    /// the 2 × cost line, and the target rule.
    #[test]
    fn the_vaults_tokens_list_at_no_less_than_twice_their_cost() {
        // 1,000 tokens that cost 500 quote: 0.5 a token, listed from 1.0 a token — tick 0 (base per quote 1)
        let cap = vault_relist_cap_tick(1_000, 500).unwrap().unwrap();
        assert!(cap <= 0 && cap > -2, "{cap}");
        // every tick at or below the line is a token price ≥ 2 × cost
        for t in [cap, cap - 1, cap - 6_932] {
            assert!(1.0001f64.powi(-t) >= 1.0 - 1e-9, "{t}");
        }
        assert!(1.0001f64.powi(-(cap + 1)) < 1.0, "one tick cheaper is under the line");
        // a dearer cost is a dearer (lower) line
        assert!(vault_relist_cap_tick(1_000, 5_000).unwrap().unwrap() < cap);
        assert_eq!(vault_relist_cap_tick(1_000, 0).unwrap(), None, "free tokens: no line");
        assert_eq!(vault_relist_cap_tick(0, 5).unwrap(), None);

        let mut own = [None; MAX_STAIR_STEPS as usize];
        own[2] = Some(80_000); // the step just above the price
        own[3] = Some(73_080);
        let (reserve, tail) = (Some(66_120), Some(10_000));
        use VaultListTarget::*;
        // 2 × cost cheaper than the step above the price: that step
        assert_eq!(vault_list_target(Some(85_000), 2, 4, &own, reserve, tail), Step(2));
        assert_eq!(vault_list_target(Some(80_000), 2, 4, &own, reserve, tail), Step(2), "its cheap edge exactly at 2 × cost");
        assert_eq!(vault_list_target(None, 2, 4, &own, reserve, tail), Step(2), "free tokens: the step above the price");
        // 2 × cost dearer than that step's start: the first position that starts at or above it
        assert_eq!(vault_list_target(Some(79_999), 2, 4, &own, reserve, tail), Step(3));
        assert_eq!(vault_list_target(Some(70_000), 2, 4, &own, reserve, tail), Reserve);
        assert_eq!(vault_list_target(Some(60_000), 2, 4, &own, reserve, tail), Tail, "inside the reserve past its first step: the tail");
        assert_eq!(vault_list_target(Some(9_999), 2, 4, &own, reserve, tail), Hold);
        // a sold-through own step is skipped
        own[2] = None;
        assert_eq!(vault_list_target(Some(85_000), 2, 4, &own, reserve, tail), Step(3));
        assert_eq!(vault_list_target(Some(85_000), 4, 4, &own, None, tail), Tail);
        assert_eq!(vault_list_target(Some(85_000), 4, 4, &own, None, None), Hold);
    }

    /// THE WHOLE LIFE, on the position math the programs use: graduation places step 0, the reserve and the tail at the
    /// open; the market climbs in uneven jumps; each "ratchet" collects the step it entered or splits the next step off
    /// the reserve (the hook's rule, one operation a ratchet) — and every token is accounted for at every step:
    /// standing + collected-as-sold + held = what graduation deposited.
    #[test]
    fn graduation_in_the_buy_then_splits_conserve_every_token() {
        use crate::position::{position_amounts, position_liquidity};
        let open = open_tick(tranche_price_x64(200_000, 200 * 100, 9).unwrap()).unwrap();
        let open_edge = floor_to_spacing(open, S).unwrap();
        let n = 9u8;
        let (stair_total, _) = stair_tail_split(STAIR_SUPPLY_TOKENS.2 + STAIR_SUPPLY_TOKENS.3);
        let planned = step_tokens(stair_total, n, 10_000).unwrap();
        let p0 = get_sqrt_price_at_tick(open).unwrap();
        let place = |p: u128, lo: i32, hi: i32, tokens: u64| -> (u128, u64) {
            let liq = position_liquidity(p, lo, hi, 0, tokens).unwrap();
            let (q, b) = position_amounts(p, lo, hi, liq).unwrap();
            assert_eq!(q, 0, "token-only");
            (liq, b)
        };
        // graduation: three positions
        let (s0lo, s0hi) = step_range(open_edge, 0, n, S).unwrap();
        let (l0, b0) = place(p0, s0lo, s0hi, planned[0]);
        let (rlo, rhi) = reserve_range(open_edge, 1, n, S).unwrap();
        let rtok: u64 = planned[1..n as usize].iter().sum();
        let (lr, br) = place(p0, rlo, rhi, rtok);
        let mut held = (planned[0] - b0) + (rtok - br);
        // (own step index, lower, upper, liq) + the reserve
        let mut own: Vec<(u8, i32, i32, u128)> = vec![(0, s0lo, s0hi, l0)];
        let mut reserve = Some((rlo, rhi, lr));
        let mut placed = 1u8;
        let mut sold = 0u64; // tokens buyers took (the positions' quote was collected for them)
        let mut tick = open;
        for jump in [4_000, 5_000, 900, 12_000, 3_000, 9_000, 20_000] {
            tick -= jump;
            let p = get_sqrt_price_at_tick(tick).unwrap();
            // ONE operation: the own step the market entered, else a split of the reserve
            if let Some(k) = own.iter().position(|o| o.3 != 0 && tick < o.2) {
                let (i, lo, hi, liq) = own[k];
                let before = position_amounts(p0.min(get_sqrt_price_at_tick(hi).unwrap()), lo, hi, liq).unwrap().1;
                let left = position_amounts(p, lo, hi, liq).unwrap().1;
                sold += before - left;
                match unsold_range(lo, hi, tick, S).unwrap() {
                    Some((nl, nu)) => {
                        let (nliq, used) = place(p, nl, nu, left);
                        held += left - used;
                        own[k] = (i, nl, nu, nliq);
                    }
                    None => {
                        held += left;
                        own[k].3 = 0;
                    }
                }
            } else if let Some((lo, hi, liq)) = reserve.filter(|r| tick < r.1) {
                let before = position_amounts(get_sqrt_price_at_tick(hi).unwrap(), lo, hi, liq).unwrap().1;
                let left = position_amounts(p, lo, hi, liq).unwrap().1;
                sold += before - left;
                let j = placed;
                let (to_step, rest) = reserve_split(left, &planned, j, n).unwrap();
                let (sl, su) = step_range(open_edge, j, n, S).unwrap();
                let mut rest = rest;
                match unsold_range(sl, su, tick, S).unwrap() {
                    Some((nl, nu)) if to_step > 0 => {
                        let (nliq, used) = place(p, nl, nu, to_step);
                        rest += to_step - used;
                        own.push((j, nl, nu, nliq));
                    }
                    _ => rest += to_step,
                }
                placed = j + 1;
                let (nlo, nhi) = (step_range(open_edge, n - 1, n, S).unwrap().0, step_range(open_edge, placed, n, S).unwrap().1);
                match unsold_range(nlo, nhi, tick, S).unwrap() {
                    Some((nl, nu)) if rest > 0 && placed < n => {
                        let (nliq, used) = place(p, nl, nu, rest);
                        held += rest - used;
                        reserve = Some((nl, nu, nliq));
                    }
                    _ => {
                        held += rest;
                        reserve = None;
                    }
                }
            }
            // conservation: every position's tokens as placed (what the market has bought out of a position no ratchet
            // collected yet is still in it until the close) + sold + held == the staircase, to the AMM's rounding
            let full = |lo: i32, hi: i32, liq: u128| position_amounts(get_sqrt_price_at_tick(hi).unwrap(), lo, hi, liq).unwrap().1;
            let mut standing: u64 = own.iter().filter(|o| o.3 != 0).map(|o| full(o.1, o.2, o.3)).sum();
            if let Some((lo, hi, liq)) = reserve {
                standing += full(lo, hi, liq);
            }
            let _ = p;
            assert!(standing + sold + held <= stair_total, "never more than was deposited");
            assert!(stair_total - (standing + sold + held) < 1_000, "conserved to rounding: {}", stair_total - (standing + sold + held));
        }
        assert!(placed >= 2, "the climb split at least one step off the reserve: placed {placed}");
    }

    /// ⛔ FLOOR FIRST: the least gross quote whose kept part (the `$FLOOR` cut
    /// off, rounded as `fees::intake` rounds it) covers the shortfall; nothing when the vault already covers it; never more
    /// than was collected.
    #[test]
    fn floor_first_takes_the_least_that_backs_the_floor() {
        assert_eq!(floor_first(1_000, 500, 500, 250), 0, "backed already");
        assert_eq!(floor_first(1_000, 500, 900, 250), 0);
        assert_eq!(floor_first(0, 900, 500, 250), 0, "nothing collected");
        assert_eq!(floor_first(1_000, 1_000_000, 0, 250), 1_000, "capped at the collection (the pump window)");
        for (short, bps) in [(1u128, 250u16), (39, 250), (40, 250), (1_234_567, 250), (987_654_321_987, 250), (77, 0), (10_000, 9_999)] {
            let x = floor_first(u64::MAX, short, 0, bps);
            let kept = |x: u64| x - crate::fees::part(x, bps as u64).unwrap();
            assert!(kept(x) as u128 >= short, "short {short} bps {bps}: x {x} keeps {}", kept(x));
            // never more than the cut's own rounding over the exact line short · 10,000 / (10,000 − bps)
            let keep = (BPS - bps as u64) as u128;
            assert!((x as u128) * keep <= short * BPS as u128 + keep, "short {short} bps {bps}: {x} overshoots");
        }
    }

    /// ⛔ THE CUSHION CLIMBS ONLY AS FAR AS THE FLOOR STAYS BACKED: the target when it
    /// is backed or cheaper; else the dearest backed grid line between it and where the cushion stands; else where it
    /// stands. Never cheaper than where it stands when the target is dearer; never a line the predicate refuses.
    #[test]
    fn the_cushion_climbs_to_the_dearest_backed_line() {
        let cur = 60_000;
        // the target cheaper than (or where) it stands: taken without asking
        let mut asked = 0;
        assert_eq!(cushion_climb(cur + 600, cur, S, |_| { asked += 1; Ok(false) }).unwrap(), cur + 600);
        assert_eq!(cushion_climb(cur, cur, S, |_| { asked += 1; Ok(false) }).unwrap(), cur);
        assert_eq!(asked, 0);
        // backed at the target: one question
        let mut asked = 0;
        assert_eq!(cushion_climb(cur - 6_000, cur, S, |_| { asked += 1; Ok(true) }).unwrap(), cur - 6_000);
        assert_eq!(asked, 1);
        // nothing dearer is backed: it stays
        assert_eq!(cushion_climb(cur - 6_000, cur, S, |_| Ok(false)).unwrap(), cur);
        // a line K: backed ⇔ lower ≥ K — the answer is the least grid line ≥ K, found in O(log n) questions
        for k in [cur - 5_999, cur - 3_000, cur - 2_970, cur - 60, cur - 1] {
            let mut asked = 0;
            let got = cushion_climb(cur - 6_000, cur, S, |l| {
                asked += 1;
                assert_eq!(l.rem_euclid(S), 0, "every question on the grid");
                Ok(l >= k)
            })
            .unwrap();
            assert_eq!(got, ceil_to_spacing(k, S).unwrap(), "k {k}");
            assert!(asked <= 9, "k {k}: {asked} questions");
        }
        assert_eq!(cushion_range_from(cur, S).unwrap(), cushion_range(cur - 1, S).unwrap());
        assert_eq!(cushion_range_from(cur, S).unwrap().unwrap().0, cur);
    }

    /// Floor first, counting the cushion as it will stand: the least `k` whose surplus is
    /// non-negative, in a handful of questions, on a linear surplus and on one with rounding noise; and the three edges.
    #[test]
    fn floor_first_least_finds_the_root_in_a_few_questions() {
        assert_eq!(floor_first_least(0, |_| Ok(-5)).unwrap(), 0);
        assert_eq!(floor_first_least(1_000, |_| Ok(3)).unwrap(), 0, "the split alone backs it");
        assert_eq!(floor_first_least(1_000, |k| Ok(-100 - k as i128)).unwrap(), 0, "keeping more cannot help");
        assert_eq!(floor_first_least(1_000, |k| Ok(-1_000 + k as i128 / 2)).unwrap(), 1_000, "the pump window");
        for (c, root, num, den) in [(1_000u64, 377u64, 1i128, 1i128), (10_000_000_000, 6_123_456_789, 3, 7), (5_000, 1, 9, 10), (5_000, 4_999, 1, 3)] {
            for noise in [0i128, 1, 3] {
                let mut asked = 0;
                let f = |k: u64| -> i128 { (k as i128 - root as i128) * num / den - if (k % 2) == 1 { noise } else { 0 } };
                let k = floor_first_least(c, |k| {
                    asked += 1;
                    Ok(f(k))
                })
                .unwrap();
                assert!(f(k) >= 0, "c {c} root {root} noise {noise}: k {k} is not backed");
                assert!(k <= root + (noise * den / num.max(1)) as u64 + 8, "c {c} root {root} noise {noise}: k {k} over-takes");
                assert!(asked as u32 <= 2 + FLOOR_FIRST_PROBES, "{asked} questions");
            }
        }
    }

    /// ⛔ The cushion's range never starts inside the bid band; it always sits above the floor: clipped at the band's dear
    /// edge, the top kept; nothing when the top is at or under it.
    #[test]
    fn the_cushion_never_starts_inside_the_bid_band() {
        let floor = 60_000;
        let dear = crate::band::dear_edge(floor, S).unwrap();
        assert_eq!(dear, floor - 120);
        // far above the floor: untouched
        let far = cushion_range(40_000, S).unwrap();
        assert_eq!(cushion_range_above(40_000, 40_000, S, floor).unwrap(), far);
        // 0.9 × the top would start inside the band: it starts at the dear edge, the top kept
        let near = cushion_range(dear - 600, S).unwrap().unwrap();
        assert!(near.1 > dear, "the fixture straddles the band");
        assert_eq!(cushion_range_above(dear - 600, dear - 600, S, floor).unwrap(), Some((near.0, dear)));
        // the TWAP's top (the cheaper) decides
        assert_eq!(cushion_range_above(40_000, dear - 600, S, floor).unwrap(), Some((near.0, dear)));
        // the top at or under the dear edge: no cushion this ratchet
        assert_eq!(cushion_range_above(dear - 1, dear - 1, S, floor).unwrap(), None);
        assert_eq!(cushion_range_above(floor + 5_000, floor + 5_000, S, floor).unwrap(), None);
        assert_eq!(cushion_clip(Some((dear - 60, dear + 900)), dear), Some((dear - 60, dear)), "one spacing wide at the least");
        assert_eq!(cushion_clip(None, dear), None);
    }

    /// ⛔ A surplus BENT by the bounty's cap — steeper, then flatter, past the cap —
    /// and a smoothly concave one: the answer is backed, within a few lamports of the true least (brute force), and never
    /// the whole collection unless that is the only backed value. An end-to-end secant form returned the whole collection here.
    #[test]
    fn floor_first_least_holds_on_a_bent_surplus() {
        // the measured shape: short 2.3 SOL with nothing kept, backed at ≈ 3.39 SOL of 140.17 collected
        let c: u64 = 140_166_068_720;
        let bent = |k: u64| -> i128 {
            let k = k as i128;
            let have = 92_384_416_155i128 + k * 975 / 1_000 - (k * 100 / 1_000).min(50_000_000);
            let need = 94_696_710_012i128 - k * 27 / 100;
            have - need
        };
        let concave = |k: u64| -> i128 {
            let x = k as f64 / c as f64;
            (((1.0 - (1.0 - x).powi(3)) * 3.0e9) as i128) - 2_300_000_000
        };
        let least = |f: &dyn Fn(u64) -> i128| -> u64 {
            let (mut lo, mut hi) = (0u64, c);
            while hi - lo > 1 {
                let m = lo + (hi - lo) / 2;
                if f(m) >= 0 { hi = m } else { lo = m }
            }
            hi
        };
        // the tolerance is the surplus's own resolution: the bent one moves a unit a lamport or two, the concave one (an
        // f64 curve cast to units) a unit per ≈ 30 lamports near its root
        // a flat stretch (−1 until the root, 0 after): the secant is blind there, the midpoint probes carry it
        let flat = |k: u64| -> i128 { if k >= 3_393_803_600 { 0 } else { -1 } };
        for (name, f, tol) in [("bent", &bent as &dyn Fn(u64) -> i128, 4u64), ("concave", &concave, 64), ("flat", &flat, c)] {
            let truth = least(f);
            let mut asked = 0u32;
            let k = floor_first_least(c, |k| {
                asked += 1;
                Ok(f(k))
            })
            .unwrap();
            assert!(f(k) >= 0, "{name}: {k} is not backed");
            assert!(k != c, "{name}: never the whole collection when less backs it (truth {truth})");
            assert!(k >= truth && k - truth <= tol, "{name}: {k} vs the least {truth}");
            assert!(asked <= 2 + FLOOR_FIRST_PROBES, "{name}: {asked} questions");
        }
    }

    /// ⛔ `$FLOOR` on the staircase: 42.5M sale /
    /// 10M POL / 897.5M steps (197M protocol + 700.5M team) / 10M tail / 40M vested, the steps FRONT-loaded (growth 9,500), the
    /// targets 20 % / 20 %, the cushion from 0.7 × the price — and the presets' set unchanged (69 / 1, 20 % / 15 %, 0.9 ×).
    /// Mutations this kills: `$FLOOR`'s numbers left at the presets' (or the presets' moved), the tail split off the
    /// presets' 69 / 1, the cushion width not the formula's, a kind byte other than 0 reading as the presets'.
    #[test]
    fn floor_runs_the_presets_staircase_with_its_own_four_numbers() {
        // the supply: 42.5M / 10M / 897.5M / 10M / 40M = 1B
        let (sale, pol, steps, tail, vest) = (42_500_000_000_000u64, 10_000_000_000_000u64, 897_500_000_000_000u64, 10_000_000_000_000u64, 40_000_000_000_000u64);
        assert_eq!(sale + pol + steps + tail + vest, SUPPLY);
        assert_eq!(SUPPLY / BPS * GENESIS_STAIR_STEPS_BPS as u64, steps);
        assert_eq!(SUPPLY / BPS * GENESIS_STAIR_TAIL_BPS as u64, tail);
        assert_eq!(stair_tail_split_for(steps + tail, &GENESIS_STAIR), (steps, tail), "897.5M / 10M to the unit");
        let (a, b) = stair_tail_split_for(989_999_999_999_997, &GENESIS_STAIR);
        assert_eq!(a + b, 989_999_999_999_997, "conserved at any deposit");
        assert_eq!(stair_tail_split(STAIR_SUPPLY_TOKENS.2 + STAIR_SUPPLY_TOKENS.3), (STAIR_SUPPLY_TOKENS.2, STAIR_SUPPLY_TOKENS.3), "presets unchanged");
        // the front-loaded weights: each step 0.95 × the one below, summing to the 897.5M exactly, at every count
        for n in 1..=MAX_STAIR_STEPS {
            let t = step_tokens(steps, n, GENESIS_STAIR_GROWTH_BPS).unwrap();
            assert_eq!(t.iter().sum::<u64>(), steps, "{n} steps hold the 897.5M exactly");
            for i in 1..n as usize {
                assert!(t[i] < t[i - 1], "front-loaded: step {i} lighter than {}", i - 1);
                assert!((t[i] as f64 / t[i - 1] as f64 - 0.95).abs() < 1e-9, "{n} steps, step {i}");
            }
        }
        // nine steps: step 0 = 897.5M / Σ 0.95^j ≈ 121.4M
        let nine = step_tokens(steps, 9, GENESIS_STAIR_GROWTH_BPS).unwrap();
        let sum: f64 = (0..9).map(|j| 0.95f64.powi(j)).sum();
        assert!((nine[0] as f64 - steps as f64 / sum).abs() < 20.0, "{}", nine[0]);
        assert_eq!((nine[0] + 50_000_000_000) / 100_000_000_000, 1_214, "121.4M");
        // the per-pool sets: the kind byte, the stored numbers (0 = the presets'), the cushion's width
        assert_eq!(stair_params(STAIR_KIND_PRESET), Some(PRESET_STAIR));
        assert_eq!(stair_params(STAIR_KIND_GENESIS), Some(GENESIS_STAIR));
        assert_eq!(stair_params(2), None);
        assert_eq!((PRESET_STAIR.pol_to_mcap_bps, PRESET_STAIR.cushion_bottom_bps, PRESET_STAIR.floor_to_price_bps), (1_500, 9_000, 2_000));
        assert_eq!((GENESIS_STAIR.pol_to_mcap_bps, GENESIS_STAIR.cushion_bottom_bps, GENESIS_STAIR.floor_to_price_bps), (2_000, 7_000, 2_000));
        assert_eq!(GENESIS_CUSHION_TICKS, cushion_ticks_for(GENESIS_CUSHION_BOTTOM_BPS).unwrap(), "0.7 ×: the formula's own value");
        assert_eq!(CUSHION_TICKS, cushion_ticks_for(CUSHION_BOTTOM_BPS).unwrap());
        assert_eq!(stored_stair_params(CUSHION_BOTTOM_BPS as u16, POL_TO_MCAP_BPS as u16, CUSHION_TICKS), PRESET_STAIR);
        let g = stored_stair_params(7_000, 2_000, GENESIS_CUSHION_TICKS);
        assert_eq!((g.pol_to_mcap_bps, g.cushion_bottom_bps, g.cushion_ticks, g.floor_to_price_bps), (2_000, 7_000, 3_567, 2_000));
        assert!(cushion_ticks_for(0).is_err() && cushion_ticks_for(10_000).is_err());
    }

    /// `$FLOOR`'s POL bids sit from 0.7 × the price up to it; a preset's from 0.9 ×. Mutations: the width ignored.
    #[test]
    fn the_cushion_depth_is_per_pool_seventy_or_ninety_percent() {
        let current = 100_000;
        for (width, share) in [(CUSHION_TICKS, 0.9), (GENESIS_CUSHION_TICKS, 0.7)] {
            let (lo, hi) = cushion_range_w(current, S, width).unwrap().unwrap();
            assert_eq!(lo, ceil_to_spacing(current + 1, S).unwrap());
            assert_eq!(hi - lo, ceil_to_spacing(width, S).unwrap());
            // the cushion's cheap edge in token price: 1.0001^−(hi − lo) of its dear edge ≈ share (the grid rounds it deeper)
            let r = 1.0001f64.powi(-(hi - lo));
            assert!(r <= share && r > share - 0.01, "{width}: {r}");
            assert_eq!(cushion_range_from_w(lo, S, width).unwrap(), Some((lo, hi)));
            assert_eq!(cushion_range_at_w(current, current + 600, S, width).unwrap(), cushion_range_w(current + 600, S, width).unwrap());
        }
        assert_eq!(cushion_range(current, S).unwrap(), cushion_range_w(current, S, CUSHION_TICKS).unwrap(), "the presets' default");
        // the clip above the bid band holds at either depth
        let floor_tick = 100_000 + 2_400;
        let dear = crate::band::dear_edge(floor_tick, S).unwrap();
        let (_, hi) = cushion_range_above_w(current, current, S, floor_tick, GENESIS_CUSHION_TICKS).unwrap().unwrap();
        assert!(hi <= dear, "never inside the band");
    }

    /// The cushion's home: with `min(spot, TWAP)` in the band, the cushion's home is the cheapest FULL-DEPTH
    /// range ending at the band's dear edge — never the spot's when the spot is dearer than that; the spot's own range
    /// (clipped) when the spot is cheaper. At either depth. Mutations: the spot returned; `− 1` dropped (one spacing
    /// short at spacing 1); the width not ceiled to the grid.
    #[test]
    fn the_cushion_home_is_the_cheapest_full_depth_range_above_the_band() {
        for spacing in [1, 10, S, 200] {
            for width in [CUSHION_TICKS, GENESIS_CUSHION_TICKS] {
                let w = ceil_to_spacing(width, spacing).unwrap();
                let floor_tick = ceil_to_spacing(200_000, spacing).unwrap();
                let dear = crate::band::dear_edge(floor_tick, spacing).unwrap();
                // a spot far dearer than the band (a pump): the home is the full depth ending at the dear edge
                for spot in [dear - 50 * w, dear - 3 * w, dear - w - 2 * spacing] {
                    let home = cushion_home_top(spot, floor_tick, spacing, width).unwrap();
                    assert_eq!(home, dear - w - 1, "spacing {spacing} width {width} spot {spot}");
                    let range = cushion_range_above_w(spot, home, spacing, floor_tick, width).unwrap();
                    assert_eq!(range, Some((dear - w, dear)), "the full depth, ending at the dear edge");
                    assert!(range.unwrap().0 > spot, "strictly cheaper than the spot: all quote");
                }
                // a spot within one cushion width of the band: the spot itself, the band clipping the range
                for spot in [dear - w + spacing, dear - 2 * spacing, dear - spacing - 1] {
                    let home = cushion_home_top(spot, floor_tick, spacing, width).unwrap();
                    assert_eq!(home, spot, "spacing {spacing} width {width} spot {spot}");
                    let (lo, hi) = cushion_range_above_w(spot, home, spacing, floor_tick, width).unwrap().unwrap();
                    assert_eq!((lo, hi), (ceil_to_spacing(spot + 1, spacing).unwrap(), dear));
                }
                // a spot with no grid line left above the band, or in it: no range at all (parked), as before
                for spot in [dear - spacing, dear] {
                    let home = cushion_home_top(spot, floor_tick, spacing, width).unwrap();
                    assert_eq!(home, spot);
                    assert_eq!(cushion_range_above_w(spot, home, spacing, floor_tick, width).unwrap(), None);
                }
            }
        }
    }

    /// The two-number split against `$FLOOR`'s targets (20 % / 20 %) and a preset's (20 % / 15 %): the same pool at the
    /// same price and POL money splits differently. Mutations: the POL target global.
    #[test]
    fn the_split_reads_each_pools_own_pol_target() {
        let floor_tick = 90_000;
        let f = get_sqrt_price_at_tick(floor_tick).unwrap();
        let p = sqrt_at_price_multiple(floor_tick, 4.0); // floor ÷ price 25 %: the floor met on both
        let mcap = {
            let p2 = U256::from(p) * U256::from(p);
            ((U256::from(SUPPLY) << 128usize) / p2).as_u128() as f64
        };
        let pol_q = (mcap * 0.10) as u64; // 10 % of the market cap
        let preset = stair_split_for(1_000_000, p, f, pol_q, SUPPLY, &PRESET_STAIR).unwrap();
        assert_eq!(preset, stair_split(1_000_000, p, f, pol_q, SUPPLY).unwrap());
        assert_eq!(preset.short_floor, 0);
        assert!((preset.short_pol as f64 / SPLIT_SCALE as f64 - 1.0 / 3.0).abs() < 1e-3, "a preset's is a third short at 10 % of 15 %");
        assert_eq!((preset.to_pol, preset.to_floor), (1_000_000, 0), "floor met, POL short: all to the POL");
        let met = stair_split_for(1_000_000, p, f, (mcap * 0.16) as u64, SUPPLY, &PRESET_STAIR).unwrap();
        assert_eq!((met.short_pol, met.to_pol), (0, 0), "a preset's POL is met at 16 % (> 15 %): all to the floor");
        let floor = stair_split_for(1_000_000, p, f, pol_q, SUPPLY, &GENESIS_STAIR).unwrap();
        assert_eq!(floor.short_floor, 0);
        assert!((floor.short_pol as f64 / SPLIT_SCALE as f64 - 0.5).abs() < 1e-3, "$FLOOR's is half short at 10 % of 20 %");
        assert_eq!((floor.to_pol, floor.to_floor), (1_000_000, 0), "floor met, POL short: all to the POL");
        // both short: in proportion
        let p10 = sqrt_at_price_multiple(floor_tick, 10.0); // floor ÷ price 10 %: half short
        let g = stair_split_for(1_000_000, p10, f, 0, SUPPLY, &GENESIS_STAIR).unwrap();
        assert!((g.to_pol as i64 - 666_666).abs() < 1_000, "{g:?}: 1 / (0.5 + 1)");
    }

    /// ⛔ CUSTOM ON THE STAIRCASE, the count derived from the cap: the EVM's `customStairSteps` reference values, the
    /// half-doubling boundary exactly ($1B / 2^10.5 = $690,533.9…), the clamps.
    /// Mutations this kills: rounding down (floor of log2: $25k → 15 still, but $200k → 12 would read 12 and $50k 15 → the
    /// pins move); the round-up comparison as `>` (no value here sits on it exactly — the boundary pair pins the side); the
    /// clamp at 16 dropped ($1k → 19 / 20); a cap at or over the top answering 0; the cap read in cents (every value / 100).
    #[test]
    fn custom_steps_are_derived_from_the_cap_exactly_as_the_evm() {
        for (cap, want) in [
            (25_000u64, 15u8),
            (50_000, 14),
            (100_000, 13),
            (200_000, 12),
            (250_000, 12),
            (1_000_000, 10),
            (10_000_000, 7),
            (1_000, 16),
            (600_000_000, 1),
            (800_000_000, 1),
            (690_533, 11),
            (690_534, 10),
            (1_000_000_000, 1),
            (5_000_000_000, 1),
            (1, 16),
            (0, 16),
        ] {
            assert_eq!(custom_stair_steps(cap), want, "${cap}");
        }
        // every cap gives 1..=16 and the count never rises with the cap
        let mut last = MAX_STAIR_STEPS;
        let mut cap = 1u64;
        while cap < 4_000_000_000 {
            let n = custom_stair_steps(cap);
            assert!((MIN_STAIR_STEPS..=MAX_STAIR_STEPS).contains(&n), "${cap}: {n}");
            assert!(n <= last, "${cap}: {n} after {last}");
            last = n;
            cap = cap * 11 / 10 + 1;
        }
        // the f64 rule agrees away from the rounding boundary
        for cap in [21_000u64, 33_333, 77_000, 123_456, 3_000_000, 42_000_000, 300_000_000] {
            let r = (CUSTOM_STAIR_TOP_MCAP_USD as f64 / cap as f64).log2().round().clamp(1.0, 16.0) as u8;
            assert_eq!(custom_stair_steps(cap), r, "${cap}");
        }
        assert_eq!(custom_stair(200_000), (12, 10_000), "equal steps");
        assert!(stair_shape_ok(custom_stair(25_000).0, CUSTOM_STAIR_GROWTH_BPS));
    }

    /// ⛔ THE TAIL IS 1 % OF SUPPLY (the EVM's `FloorStairs.split`): a Custom reserve at any split keeps a 10M tail
    /// and puts the rest on the steps; the presets' 700e12 and `$FLOOR`'s 460e12 split exactly as the ratio always did.
    /// Mutations: the tail by the ratio (a 60 % reserve's tail 8.57M); the cap at `credited` dropped (a 1-unit deposit
    /// underflows).
    #[test]
    fn the_tail_is_one_percent_of_supply_and_the_presets_are_unchanged() {
        let (_, _, steps, tail) = STAIR_SUPPLY_TOKENS;
        assert_eq!(stair_tail_split_supply(steps + tail, &PRESET_STAIR), stair_tail_split_for(steps + tail, &PRESET_STAIR));
        let g = SUPPLY / BPS * (GENESIS_STAIR_STEPS_BPS + GENESIS_STAIR_TAIL_BPS) as u64;
        assert_eq!(stair_tail_split_supply(g, &GENESIS_STAIR), stair_tail_split_for(g, &GENESIS_STAIR));
        // Custom's default 30 / 10 / 60: 590M on the steps, 10M tail
        let custom = SUPPLY / BPS * 6_000;
        assert_eq!(stair_tail_split_supply(custom, &PRESET_STAIR), (590_000_000_000_000, 10_000_000_000_000));
        // the 10 % floor of every zone: an 80 / 10 / 10 reserve of 100M keeps the 10M tail and 90M of steps
        assert_eq!(stair_tail_split_supply(SUPPLY / 10, &PRESET_STAIR), (90_000_000_000_000, 10_000_000_000_000));
        assert_eq!(stair_tail_split_supply(1, &PRESET_STAIR), (0, 1), "never more than the deposit");
        assert_ne!(stair_tail_split_for(custom, &PRESET_STAIR).1, 10_000_000_000_000, "the ratio would have differed");
    }
}
