//! `take_fees` and `intake`: the skim, creator fee and platform cut booked against a `FloorState`. The caller (the
//! hook's fee module) emits the events from the returned figures.
//!
//! ⛔ Both functions are all-or-nothing: they compute every new field on a copy and write the state
//! only when nothing overflowed, so an `Err` leaves `FloorState` untouched (the EVM reverts the frame).
//!
//! The skim distance is measured to the band's DEAR edge and the at-floor endpoints come off the state.

use crate::band::dear_edge;
use crate::constants::{BPS, SKIM_RAMP_TICKS};
use crate::error::{FloorMathError, FloorMathResult};
use crate::ramp::skim_bps;
use crate::state::FloorState;

fn add(a: u64, b: u64) -> FloorMathResult<u64> {
    a.checked_add(b).ok_or(FloorMathError::Overflow)
}

/// `leg · bps / 10_000`, floored, in u128 so it cannot overflow before the narrowing.
///
/// ⚠ `pub` for an off-chain indexer, which DERIVES each swap's referral share as
/// `part(gross creator fee, referral_share_bps)` and must round exactly as [`take_fees`] does below, which only this
/// function can promise.
pub fn part(leg: u64, bps: u64) -> FloorMathResult<u64> {
    u64::try_from(leg as u128 * bps as u128 / BPS as u128).map_err(|_| FloorMathError::Overflow)
}

/// The platform cut on one quote inflow (the EVM's proceeds path, accrued here instead of pushed). Returns what the
/// vault keeps.
///
/// `cut = x·platform_bps/10_000` (floored, so dust favours the vault); a zero amount, a zero
/// `platform_bps` (the EVM's `platformVault == address(0)`) or a zero cut returns `x` and books
/// nothing; otherwise `platform_owed += cut`, `platform_paid_eth += cut`, and `x − cut` is returned.
/// The caller emits `PlatformCut(x − returned)`.
/// ⚠ Reads `state.platform_bps` (250 on every Solana pool), identical to a literal 250 under `register_pool`'s
/// validation.
pub fn intake(state: &mut FloorState, x: u64) -> FloorMathResult<u64> {
    if x == 0 || state.platform_bps == 0 {
        return Ok(x);
    }
    let cut = part(x, state.platform_bps as u64)?;
    if cut == 0 {
        return Ok(x);
    }
    let owed = add(state.platform_owed, cut)?;
    let paid = add(state.platform_paid_eth, cut)?;
    state.platform_owed = owed;
    state.platform_paid_eth = paid;
    Ok(x - cut)
}

/// ⛔ THE GAP BUY SKIM: a buy pays 1.5 × the ramp's skim when the price stands far enough above the current cushion,
/// the buy side's total skim capped at 15 %. The EVM twin (`FloorPolicy.STAIR_GAP_SKIM*`,
/// `SingletonFloorEngine._gapSkim`) uses the SAME constants and the same integer operations in the same order.
///
/// The gap is how far the spot stands above the cushion's top as it stands (placed by the last ratchet under `min(spot,
/// 15-min TWAP)`), in ticks: `gap = cushion_top − tick` — ⛔ a dearer price is a LOWER tick on every FLOOR pool (the quote is
/// token0), so the cushion's top in PRICE is its LOWER tick, `StairState.cushion_lower` (the EVM's normalised
/// `cushionLower − current`). A BUY with `gap ≥` [`STAIR_GAP_SKIM_TICKS`] pays [`STAIR_GAP_SKIM_MULT_BPS`] (1.5 ×) of the
/// RAMP's figure (so the at-floor buy ramp to 0 is kept); mode 2 rises linearly in ticks from 1 × at
/// [`STAIR_GAP_SKIM_FROM_TICKS`] (G/2) to 1.5 × at G. Capped at [`STAIR_GAP_SKIM_MAX_BPS`] (the far buy rate is ≤ 1,000, so
/// the cap never binds on today's bounds). Sells, creator, holder and team fees unchanged; the extra skim is booked exactly
/// as the skim (`intake` → vault / `unpaid_skim`, the platform cut). Ticks are the ratio `g = (spot − top) / spot` as
/// `ceil(−ln(1 − g) / ln 1.0001)`: g 5 % → 513, 10 % → 1,054, 16.5 % → 1,804, 20 % → 2,232, 33 % → 4,005.
///
/// `STAIR_GAP_SKIM`: 0 = OFF (no pool's skim changes and the ratchet writes no cache); 1 = the cliff at G (it stays on
/// until the cushion is back to normal, no time fade); **2 = the linear ramp from G/2: 1 × at 20 % rising to 1.5 × at
/// 40 %, 1.5 × past it (the EVM twin ships the same)**.
pub const STAIR_GAP_SKIM: u8 = 2;
/// G, as ticks (40 %).
pub const STAIR_GAP_SKIM_TICKS: i32 = 5_109;
/// Mode 2's 1 × end (the ramp starts here; must stay under G), as ticks (20 %).
pub const STAIR_GAP_SKIM_FROM_TICKS: i32 = 2_232;
/// The boost at G, in bps of the ramp's figure (1.5 ×).
pub const STAIR_GAP_SKIM_MULT_BPS: u64 = 15_000;
/// The buy skim's ceiling under the boost (at most 15 %).
pub const STAIR_GAP_SKIM_MAX_BPS: u64 = 1_500;

const _: () = assert!(STAIR_GAP_SKIM <= 2 && 0 < STAIR_GAP_SKIM_FROM_TICKS && STAIR_GAP_SKIM_FROM_TICKS < STAIR_GAP_SKIM_TICKS);

/// Whether the gap buy skim is switched on (and the ratchet keeps the cushion-top cache).
pub const fn cushion_gap_skim_on() -> bool {
    STAIR_GAP_SKIM != 0
}

/// The gap buy skim on one leg's ramped skim `bps` at pre-swap `tick`, against the cushion top the last ratchet cached
/// (`None`: no cushion standing). A sell, `None`, or the switch OFF return `bps` unchanged.
pub fn gap_buy_skim_bps(bps: u64, is_buy: bool, tick: i32, cushion_top: Option<i32>) -> u64 {
    if !cushion_gap_skim_on() || !is_buy {
        return bps;
    }
    gap_skim_with(bps, tick, cushion_top, STAIR_GAP_SKIM, STAIR_GAP_SKIM_FROM_TICKS, STAIR_GAP_SKIM_TICKS)
}

/// The EVM's `_gapSkim(bps)` with the mode and thresholds passed in (the tests' grid). The EVM's operations, in its
/// order.
pub fn gap_skim_with(bps: u64, tick: i32, cushion_top: Option<i32>, mode: u8, from: i32, to: i32) -> u64 {
    let Some(top) = cushion_top else { return bps };
    let gap = top as i64 - tick as i64; // ⛔ dearer = lower tick (the quote is token0)
    let (from, to) = (from as i64, to as i64);
    let m = if gap >= to {
        STAIR_GAP_SKIM_MULT_BPS
    } else if mode == 2 && gap > from {
        BPS + (STAIR_GAP_SKIM_MULT_BPS - BPS) * (gap - from) as u64 / (to - from) as u64
    } else {
        return bps;
    };
    let bps = bps * m / BPS;
    bps.min(STAIR_GAP_SKIM_MAX_BPS)
}

/// What one `take_fees` call booked, for the caller's events and hook delta.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeeTake {
    /// `skim_fee + creator_fee + holder_fee + team_fee`: the hook delta on
    /// the quote leg.
    pub total: u64,
    /// The ramped skim rate this leg paid (`Skimmed.skimBps`).
    pub skim_bps: u64,
    /// `leg · skim_bps / 10_000` (`Skimmed.ethAmount`).
    pub skim_fee: u64,
    /// The part of the skim the vault kept, after `intake`.
    pub kept: u64,
    /// The platform's cut of the skim (`skim_fee − kept`).
    pub cut: u64,
    /// `leg · creator_bps(side) / 10_000`. ⛔ **GROSS** — before the referral split, exactly as
    /// `skim_fee` is gross before the platform cut. A hook record's fee breakdown carries this figure
    /// and is measured `skim + creator + holder` against the hook delta, so netting it here would
    /// under-report every referred pool's swap by the referral and fail the record check.
    pub creator_fee: u64,
    /// The part of `creator_fee` split off to the pool's `referrer` wallet (accrued in `referral_owed`, claimed by
    /// `floor_hook::claim_referral_fees`; the EVM's `_accrueCreator`). 0 on a pool that was not referred, and 0
    /// whenever the floored share rounds to nothing.
    /// ⛔ It is CARVED OUT OF `creator_fee`, never added beside it — the same relationship `cut` has
    /// to `skim_fee`. The creator accrues `creator_fee − referral_fee`; the trader pays neither more
    /// nor less than before, so `total` is untouched.
    pub referral_fee: u64,
    /// `leg · holder_bps(side) / 10_000`, up to 10 % of the leg (`policy::MAX_HOLDER_BPS` = 1000), booked into
    /// `holder_owed`, which leaves only through `floor_hook::pay_holder`, called by `floor_holders::claim`, whose tally
    /// (`crate::holders`) folds `holder_owed + holder_paid_eth` lazily.
    pub holder_fee: u64,
    /// floor(leg·team_fee_bps/1e4), booked to `team_owed`. ⛔ ON TOP (in `total`), unlike `cut` (inside the skim) and
    /// `referral_fee` (inside the creator fee). 0 at a zero rate.
    pub team_fee: u64,
}

/// Skim + creator fee + holder tax + team fee on one quote leg; the platform cut stays inside the skim.
///
/// `bps = skim_bps(dear_edge − tick, 16094, is_buy, buy_far, sell_far, buy_at_floor, sell_at_floor)`
/// with `dear_edge = floor_tick − band_ticks(spacing)` (a sell already inside the band is AT the floor)
/// and the four rates read off `state`; each part is `leg·rate/10_000`, floored (dust stays with the
/// trader), computed independently and summed — they never compound. A zero total books nothing.
/// Otherwise the skim goes through `intake` into `vault_eth` and `unpaid_skim`, the creator fee
/// accrues to `creator_fees_owed`, and the holder fee to `holder_owed`.
///
/// `is_buy` is `zero_for_one != quote_is_token1`, and it also selects the creator / holder side. The
/// LP fee is not here: the pool charges it.
///
/// A pool whose `referrer` is non-zero splits `referral_share_bps` off its GROSS creator fee into
/// `referral_owed` / `referral_fees_paid` before the remainder reaches `creator_fees_owed` (the EVM's
/// `_accrueCreator`). The trader pays the same either way: `total` and `FeeTake.creator_fee` are both
/// GROSS and unchanged, and the split only decides who the already-taken lamports are owed TO. ⚠ That
/// also means the quote a trader is shown never moves when a launch is referred.
/// The holder fee is computed from `holder_*_bps` (`register_pool` caps it at `MAX_HOLDER_BPS` per side)
/// and booked where I-3 counts it (`holder_owed`); it leaves through `floor_hook::pay_holder`.
/// ⛔ **THIS FUNCTION IS THE ONE `+=` WRITER of `holder_owed` and of `referral_owed`**, and the swap
/// path's one writer of `team_owed` (`floor_hook::seed_floor` is the other).
///
/// `cushion_top`: the gap buy skim's input ([`gap_buy_skim_bps`]) — the cushion's dear edge as the last ratchet
/// placed it (its LOWER tick, the price-side top), read off the pool account's TWAP tail
/// (`floor_types::twap::TwapOracle::cushion_top_of_account`); `None` leaves the skim exactly as before. ⛔ Every
/// caller passes it — the chain's two callbacks and the quoter — so the quote the trader is shown is the skim the
/// chain takes.
pub fn take_fees(
    state: &mut FloorState,
    leg: u64,
    is_buy: bool,
    tick: i32,
    cushion_top: Option<i32>,
) -> FloorMathResult<FeeTake> {
    let edge = dear_edge(state.floor_tick, state.tick_spacing)?;
    let distance = edge.checked_sub(tick).ok_or(FloorMathError::Overflow)?;
    let bps = skim_bps(
        distance,
        SKIM_RAMP_TICKS,
        is_buy,
        state.skim_buy_far_bps,
        state.skim_sell_far_bps,
        state.skim_buy_at_floor_bps,
        state.skim_sell_at_floor_bps,
    );
    let bps = gap_buy_skim_bps(bps, is_buy, tick, cushion_top);
    let (creator_bps, holder_bps) = if is_buy {
        (state.creator_buy_bps, state.holder_buy_bps)
    } else {
        (state.creator_sell_bps, state.holder_sell_bps)
    };
    let fee = part(leg, bps)?;
    let cfee = part(leg, creator_bps as u64)?;
    let hfee = part(leg, holder_bps as u64)?;
    // the platform fee, a FIFTH part on the same leg, floored alone.
    let tfee = part(leg, state.team_fee_bps as u64)?;
    let total = add(add(add(fee, cfee)?, hfee)?, tfee)?;
    // ⛔ EVERY FIELD NAMED, never `..FeeTake::default()`: a struct-update tail silently zeroes a field
    // the day one is added, and a record carrying the fee breakdown would then ship zeros.
    let mut take = FeeTake {
        total,
        skim_bps: bps,
        skim_fee: fee,
        kept: 0,
        cut: 0,
        creator_fee: cfee,
        referral_fee: 0,
        holder_fee: hfee,
        team_fee: tfee,
    };
    if total == 0 {
        return Ok(take);
    }
    let mut next = *state;
    if fee > 0 {
        let kept = intake(&mut next, fee)?;
        next.vault_eth = add(next.vault_eth, kept)?;
        next.unpaid_skim = add(next.unpaid_skim, kept)?;
        take.kept = kept;
        take.cut = fee - kept;
    }
    // The CONDITIONAL creator-fee referral split, mirroring the EVM's `_accrueCreator` line for line.
    // ⛔ **THE GATE IS THE ABSENT DESTINATION, NOT A FLAG.** There is no "was this launch referred?"
    // test in the fee path on either chain: `register_pool` wrote the zero key on an unreferred
    // pool, and a zero key splits nothing. Do not add a boolean here — this must stay the EVM's twin
    // so the two cannot drift.
    // ⛔ **ROUNDING: the referral floors and the CREATOR KEEPS THE REMAINDER.** `part` floors, and
    // the creator accrues `cfee − referral`, so the dust stays with the creator and the two halves
    // always re-sum to the gross — `creator_fees_owed + referral_owed` gains exactly `cfee`, which
    // is why I-3's quote sum is unchanged by the split.
    // ⛔ The destination is the pool's own `referrer` WALLET (@640); the retired distributor bytes @600
    // are never read.
    let mut creator_net = cfee;
    if cfee > 0 && next.referrer != [0u8; 32] {
        let share = next.referral_share_bps as u64;
        // `if (bps > 10_000) revert ReferralFundingFailed()` on the EVM. Load-bearing — past 10_000
        // the referral would exceed the gross fee and `cfee − referral` would underflow.
        if share > BPS {
            return Err(FloorMathError::ReferralShareOutOfRange);
        }
        let referral = part(cfee, share)?;
        // a zero referral books nothing and emits nothing; the creator keeps it all.
        if referral != 0 {
            next.referral_owed = add(next.referral_owed, referral)?;
            next.referral_fees_paid = add(next.referral_fees_paid, referral)?;
            creator_net = cfee - referral;
            take.referral_fee = referral;
        }
    }
    next.creator_fees_owed = add(next.creator_fees_owed, creator_net)?;
    next.holder_owed = add(next.holder_owed, hfee)?;
    next.team_owed = add(next.team_owed, tfee)?; // ⛔ the swap path's one writer
    *state = next;
    Ok(take)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::band::band_ticks;
    use crate::constants::{DEFAULT_SKIM_BUY_AT_FLOOR_BPS, DEFAULT_SKIM_SELL_AT_FLOOR_BPS};
    use crate::modes::{preset, Mode};
    use crate::policy::PLATFORM_BPS;

    /// A pool with fixed literal rates (2 % skim, 1 % creator) and the 0 / 1000 at-floor endpoints, whose band's DEAR
    /// edge is at `edge` (so `floor_tick = edge + 120`): every distance below reads as "ticks above the dear edge",
    /// exactly as the EVM's `_skimBps` measures it. ⚠ Literal rates, NOT a preset: these tests pin `take_fees`'
    /// arithmetic. Preset pools are `preset_pool` below.
    fn meme(edge: i32) -> FloorState {
        let mut s = FloorState::default();
        s.tick_spacing = 60;
        s.floor_tick = edge + band_ticks(60).unwrap();
        s.skim_buy_at_floor_bps = DEFAULT_SKIM_BUY_AT_FLOOR_BPS as u16;
        s.skim_sell_at_floor_bps = DEFAULT_SKIM_SELL_AT_FLOOR_BPS as u16;
        s.skim_buy_far_bps = 200;
        s.skim_sell_far_bps = 200;
        s.creator_buy_bps = 100;
        s.creator_sell_bps = 100;
        s.platform_bps = PLATFORM_BPS;
        s
    }

    /// A preset pool as `register_pool` writes it: the preset's far rates and creator rate, the at-floor endpoints 0 /
    /// `skim_sell_far × the preset's at-floor multiple` (1.5×).
    fn preset_pool(m: Mode, edge: i32) -> FloorState {
        let p = preset(m);
        let mut s = meme(edge);
        (s.skim_buy_far_bps, s.skim_sell_far_bps) = (p.skim_buy_far_bps, p.skim_sell_far_bps);
        (s.creator_buy_bps, s.creator_sell_bps) = (p.creator_bps, p.creator_bps);
        (s.holder_buy_bps, s.holder_sell_bps) = (p.holder_bps, p.holder_bps);
        s.skim_buy_at_floor_bps = 0;
        s.skim_sell_at_floor_bps =
            crate::constants::skim_sell_at_floor_bps_at(p.skim_sell_far_bps, p.skim_sell_at_floor_multiple_bps as u64);
        s
    }

    /// floor(leg·team_fee_bps/1e4), a FIFTH part ON TOP, booked to team_owed.
    /// Mutations: left out of `total`; booked into vault_eth / intake; team_fees_paid bumped here.
    #[test]
    fn the_team_fee_is_a_fifth_part_on_top_and_books_team_owed() {
        let mut s = meme(1200);
        s.team_fee_bps = crate::policy::TEAM_POOL_FEE_BPS_DEFAULT; // the pool default, 25
        let t = take_fees(&mut s, 10_000_000_000, false, 1200, None).unwrap();
        assert_eq!((t.team_fee, t.skim_fee, t.creator_fee, t.holder_fee, t.cut), (25_000_000, 1_000_000_000, 100_000_000, 0, 25_000_000));
        assert_eq!(t.total, 1_125_000_000);
        assert_eq!((s.team_owed, s.team_fees_paid), (25_000_000, 0));
        assert_eq!((s.vault_eth, s.unpaid_skim, s.platform_owed), (975_000_000, 975_000_000, 25_000_000));
    }

    /// `take_fees` charges the POOL's OWN rate, whatever it is — the 0.5 % cap, a lowered 0.10 %, or 0 — and nothing
    /// else in the split moves with it. A pool keeps the rate it was created with, so two pools created either side
    /// of a change charge two rates.
    /// Mutations: a constant read instead of `team_fee_bps`; the rate leaking into skim / cut / vault.
    #[test]
    fn the_team_fee_is_the_pools_own_rate_the_cap_a_lower_one_or_zero() {
        use crate::policy::TEAM_POOL_FEE_BPS_MAX;
        for (bps, fee) in [(TEAM_POOL_FEE_BPS_MAX, 50_000_000u64), (10, 10_000_000), (0, 0)] {
            let mut s = meme(1200);
            s.team_fee_bps = bps;
            let t = take_fees(&mut s, 10_000_000_000, false, 1200, None).unwrap();
            assert_eq!((t.team_fee, s.team_owed), (fee, fee), "{bps} bps");
            assert_eq!((t.skim_fee, t.creator_fee, t.cut), (1_000_000_000, 100_000_000, 25_000_000), "{bps} bps: the rest of the split is unchanged");
            assert_eq!(t.total, 1_100_000_000 + fee, "{bps} bps: on top");
            assert_eq!((s.vault_eth, s.platform_owed), (975_000_000, 25_000_000), "{bps} bps: never backing, never the cut");
        }
    }

    /// 399·25/1e4 floors to 0 and books nothing (the zero-total no-op); 400 -> 1.
    #[test]
    fn a_tiny_leg_pays_no_team_fee_and_books_nothing() {
        let mut s = meme(0);
        (s.team_fee_bps, s.skim_buy_far_bps, s.skim_buy_at_floor_bps, s.creator_buy_bps) = (25, 0, 0, 0);
        let before = s;
        let t = take_fees(&mut s, 399, true, -100_000, None).unwrap();
        assert_eq!((t.team_fee, t.total), (0, 0));
        assert_eq!(s, before);
        let t = take_fees(&mut s, 400, true, -100_000, None).unwrap();
        assert_eq!((t.team_fee, t.total, s.team_owed), (1, 1, 1));
    }

    #[test]
    fn a_pool_with_no_team_rate_pays_no_team_fee() {
        let mut s = meme(1200);
        let t = take_fees(&mut s, 10_000_000_000, false, 1200, None).unwrap();
        assert_eq!((t.team_fee, s.team_owed, t.total), (0, 0, 1_100_000_000));
    }

    #[test]
    fn team_overflow_is_atomic() {
        let mut s = meme(0);
        (s.team_fee_bps, s.team_owed) = (25, u64::MAX);
        let before = s;
        assert_eq!(take_fees(&mut s, 10_000_000, false, 0, None), Err(FloorMathError::Overflow));
        assert_eq!(s, before);
    }

    // Only the SKIM ramps on the presets — a sell on any preset pays 2 % far and 1.5× = 3 % at the floor, a buy 2 % far
    // and 0 at the floor; the creator fee never ramps. The EVM twin:
    // `FloorHookMath.skimBps` over the same presets.
    #[test]
    fn todays_presets_ramp_only_the_skim() {
        let leg = 1_000_000u64;
        let far = -20_000; // past the ramp (`distance = dear_edge − tick`)
        for (m, far_bps, at_bps, creator) in
            [(Mode::Cook, 200, 300, 75u64), (Mode::Bluechip, 200, 300, 100), (Mode::Moonshot, 200, 300, 50)]
        {
            let mut s = preset_pool(m, 0);
            let sell_far = take_fees(&mut s, leg, false, far, None).unwrap();
            let sell_at = take_fees(&mut s, leg, false, 0, None).unwrap();
            let buy_far = take_fees(&mut s, leg, true, far, None).unwrap();
            let buy_at = take_fees(&mut s, leg, true, 0, None).unwrap();
            assert_eq!((sell_far.skim_bps, sell_at.skim_bps), (far_bps, at_bps), "{m:?} sell far / at the floor");
            assert_eq!((buy_far.skim_bps, buy_at.skim_bps), (far_bps, 0), "{m:?} buy far / at the floor");
            for t in [sell_far, sell_at, buy_far, buy_at] {
                assert_eq!(t.creator_fee, leg * creator / 10_000, "{m:?}: the creator fee never ramps");
                assert_eq!(t.holder_fee, 0, "{m:?}: no preset carries a holder rate");
            }
        }
    }

    // The EVM's proceeds path (the platform cut).
    #[test]
    fn intake_cuts_2_5_percent_and_floors_dust_to_the_vault() {
        let mut s = meme(0);
        assert_eq!(intake(&mut s, 1_000_000).unwrap(), 975_000);
        assert_eq!((s.platform_owed, s.platform_paid_eth), (25_000, 25_000));
        // 39 · 250 / 10_000 = 0.975 -> 0: no cut, nothing booked
        assert_eq!(intake(&mut s, 39).unwrap(), 39);
        assert_eq!(s.platform_owed, 25_000);
        assert_eq!(intake(&mut s, 40).unwrap(), 39);
        assert_eq!(s.platform_owed, 25_001);
        assert_eq!(intake(&mut s, 0).unwrap(), 0);
        // platform_bps 0 is the EVM's platformVault == 0 branch
        s.platform_bps = 0;
        assert_eq!(intake(&mut s, 1_000_000).unwrap(), 1_000_000);
        assert_eq!(s.platform_owed, 25_001);
    }

    #[test]
    fn intake_overflow_leaves_state_untouched() {
        let mut s = meme(0);
        s.platform_owed = u64::MAX;
        let before = s;
        assert_eq!(intake(&mut s, 1_000_000), Err(FloorMathError::Overflow));
        assert_eq!(s, before);
    }

    // At the floor: a sell pays 10 % skim + 1 % creator; a buy 0 % skim + 1 % creator.
    #[test]
    fn take_fees_books_skim_cut_and_creator() {
        let mut s = meme(1200);
        assert_eq!(s.floor_tick, 1320);
        let t = take_fees(&mut s, 10_000_000_000, false, 1200, None).unwrap();
        assert_eq!(t.skim_bps, 1000);
        assert_eq!(t.skim_fee, 1_000_000_000);
        assert_eq!(t.cut, 25_000_000);
        assert_eq!(t.kept, 975_000_000);
        assert_eq!(t.creator_fee, 100_000_000);
        assert_eq!(t.total, 1_100_000_000);
        assert_eq!((s.vault_eth, s.unpaid_skim), (975_000_000, 975_000_000));
        assert_eq!((s.platform_owed, s.creator_fees_owed, s.holder_owed), (25_000_000, 100_000_000, 0));

        let mut b = meme(1200);
        let t = take_fees(&mut b, 10_000_000_000, true, 1300, None).unwrap(); // tick above floor... distance −100
        assert_eq!((t.skim_bps, t.skim_fee, t.creator_fee, t.total), (0, 0, 100_000_000, 100_000_000));
        assert_eq!((b.vault_eth, b.unpaid_skim, b.platform_owed), (0, 0, 0));
        assert_eq!(b.creator_fees_owed, 100_000_000);
    }

    // A zero total books nothing, but still reports the rate.
    #[test]
    fn zero_total_books_nothing() {
        let mut s = meme(0);
        let before = s;
        let t = take_fees(&mut s, 9, false, 0, None).unwrap(); // 9·1000/1e4 = 0, 9·100/1e4 = 0
        assert_eq!((t.total, t.skim_bps), (0, 1000));
        assert_eq!(s, before);
    }

    #[test]
    fn creator_and_holder_sides_follow_is_buy() {
        let mut s = meme(0);
        s.creator_buy_bps = 10;
        s.creator_sell_bps = 90;
        s.holder_buy_bps = 20;
        s.holder_sell_bps = 30;
        let buy = take_fees(&mut s, 1_000_000, true, -100_000, None).unwrap(); // far: distance 100000
        assert_eq!((buy.creator_fee, buy.holder_fee), (1_000, 2_000));
        let sell = take_fees(&mut s, 1_000_000, false, -100_000, None).unwrap();
        assert_eq!((sell.creator_fee, sell.holder_fee), (9_000, 3_000));
        assert_eq!(s.holder_owed, 5_000);
    }

    #[test]
    fn take_fees_overflow_is_atomic() {
        let mut s = meme(0);
        s.creator_fees_owed = u64::MAX;
        let before = s;
        assert_eq!(take_fees(&mut s, 10_000_000, false, 0, None), Err(FloorMathError::Overflow));
        assert_eq!(s, before);
        let mut t = meme(0);
        t.floor_tick = i32::MIN + 120; // dear edge = i32::MIN; the subtraction of tick 1 overflows
        assert_eq!(take_fees(&mut t, 1, true, 1, None), Err(FloorMathError::Overflow));
        // and the dear edge ITSELF is checked (`band::dear_edge` is checked arithmetic, so this state can neither panic
        // in debug nor wrap in release before `checked_sub` runs)
        let mut v = meme(0);
        v.floor_tick = i32::MIN;
        let before = v;
        assert_eq!(take_fees(&mut v, 1, true, 0, None), Err(FloorMathError::Overflow));
        assert_eq!(v, before);
        // a u64::MAX leg does not panic
        let mut u = meme(0);
        assert!(take_fees(&mut u, u64::MAX, false, 0, None).is_ok());
    }

    // The distance is to the band's DEAR edge, not to the published floor_tick. A sell
    // anywhere inside the band [floor_tick − 120, floor_tick] pays the at-floor rate; one spacing above
    // the dear edge it is already on the ramp.
    #[test]
    fn distance_is_measured_to_the_dear_edge() {
        let mut s = meme(0); // floor_tick 120, dear edge 0
        let inside = take_fees(&mut s, 1_000_000, false, 60, None).unwrap();
        assert_eq!(inside.skim_bps, 1000, "inside the band is AT the floor");
        let at_cheap = take_fees(&mut s, 1_000_000, false, 120, None).unwrap();
        assert_eq!(at_cheap.skim_bps, 1000);
        let at_dear = take_fees(&mut s, 1_000_000, false, 0, None).unwrap();
        assert_eq!(at_dear.skim_bps, 1000);
        let above = take_fees(&mut s, 1_000_000, false, -60, None).unwrap();
        // closeness = 10000 − 60·10000/16094 = 10000 − 37 = 9963; 200 + 800·9963/10000 = 997
        assert_eq!(above.skim_bps, 997);
        // the same tick measured against floor_tick (the cheap edge) would have read 180 ticks up: 991
        assert_eq!(skim_bps(180, SKIM_RAMP_TICKS, false, 200, 200, 0, 1000), 991);
    }

    // The endpoints come off the state. A pool registered with a 400 bp sell-at-floor pays
    // 400 at the edge, and one whose far rate meets its endpoint is flat.
    #[test]
    fn at_floor_endpoints_are_read_off_the_state() {
        let mut s = meme(0);
        s.skim_sell_at_floor_bps = 400;
        s.skim_buy_at_floor_bps = 50;
        assert_eq!(take_fees(&mut s, 1_000_000, false, 0, None).unwrap().skim_bps, 400);
        assert_eq!(take_fees(&mut s, 1_000_000, true, 0, None).unwrap().skim_bps, 50);
        let mut flat = meme(0);
        flat.skim_sell_far_bps = 500;
        flat.skim_sell_at_floor_bps = 400;
        assert_eq!(take_fees(&mut flat, 1_000_000, false, 0, None).unwrap().skim_bps, 500);
        // a zeroed state (no endpoints written) is a sell curve flat at the far rate: register_pool
        // MUST write the endpoints (0 and `skim_sell_at_floor_bps(far)`)
        let mut zero = meme(0);
        zero.skim_sell_at_floor_bps = 0;
        assert_eq!(take_fees(&mut zero, 1_000_000, false, 0, None).unwrap().skim_bps, 200);
    }

    // =====================================================================================
    // The conditional creator-fee referral split (the EVM's `_accrueCreator`).
    // =====================================================================================

    /// A Meme pool that WAS referred: a non-zero destination and the EVM's default 10 % share.
    /// The destination is the whole gate, so setting it is what "referred" means here.
    fn referred(edge: i32) -> FloorState {
        let mut s = meme(edge);
        // "referred" is a referrer WALLET @640, not the retired distributor bytes @600
        s.referrer = [7u8; 32];
        s.referral_share_bps = crate::constants::REFERRAL_CREATOR_SHARE_BPS as u16;
        s
    }

    /// The referral is floored and the CREATOR KEEPS THE REMAINDER (the EVM's `_accrueCreator`).
    ///
    /// Mutations this kills: rounding the referral UP (or the creator down); netting
    /// `FeeTake.creator_fee` instead of leaving it gross; forgetting either meter; splitting off
    /// something other than the gross creator fee (the skim, or the whole leg).
    #[test]
    fn a_referred_pool_splits_the_floored_share_and_the_creator_keeps_the_remainder() {
        let mut s = referred(0);
        s.creator_buy_bps = 100; // 1 %
        // leg 10_500 at 1 % = 105 gross; 105 * 1000 / 10_000 = 10.5 -> 10, creator keeps 95.
        let t = take_fees(&mut s, 10_500, true, -100_000, None).unwrap();
        assert_eq!(t.creator_fee, 105, "creator_fee stays GROSS -- the record is measured against the delta");
        assert_eq!(t.referral_fee, 10, "the referral FLOORS");
        assert_eq!(s.creator_fees_owed, 95, "the creator keeps the remainder, dust included");
        assert_eq!(s.referral_owed, 10);
        assert_eq!(s.referral_fees_paid, 10);
        // the two halves re-sum to the gross: nothing is created or lost by the split
        assert_eq!(s.creator_fees_owed + s.referral_owed, t.creator_fee);
    }

    /// Property (b) and the gate: an UNREFERRED pool is bit-identical to a pool that has never
    /// heard of referrals -- the zero destination makes the fee path skip the split entirely.
    ///
    /// Mutations this kills: the polarity inverted (splitting when the destination is ZERO);
    /// gating on the share instead of the destination; splitting unconditionally.
    #[test]
    fn an_unreferred_pool_splits_nothing_even_with_a_share_set() {
        let mut s = meme(0);
        s.creator_buy_bps = 100;
        // a rate with NO destination must still split nothing: the destination is the gate
        s.referral_share_bps = crate::constants::REFERRAL_CREATOR_SHARE_BPS as u16;
        assert_eq!(s.referrer, [0u8; 32]);
        let t = take_fees(&mut s, 10_500, true, -100_000, None).unwrap();
        assert_eq!(t.referral_fee, 0);
        assert_eq!(s.referral_owed, 0);
        assert_eq!(s.referral_fees_paid, 0);
        assert_eq!(s.creator_fees_owed, 105, "the creator keeps 100 %");
    }

    /// The gate is the REFERRER WALLET @640. A pool with only the retired distributor bytes @600 (`pad7`) non-zero and a
    /// zero referrer splits NOTHING: its creator keeps 100 %.
    ///
    /// Mutation this kills: the gate left on `referral_distributor` (or ORed with it), which would
    /// keep booking a legacy pool's referral share against a distributor nothing pays out any more.
    #[test]
    fn a_legacy_distributor_pool_splits_nothing_after_the_upgrade() {
        let mut s = meme(0);
        s.creator_buy_bps = 100;
        s.trader_ref_owed = 0xD1D1_D1D1_D1D1_D1D1;
        s.pad7 = [0xD1u8; 16];
        s.referral_share_bps = crate::constants::REFERRAL_CREATOR_SHARE_BPS as u16;
        assert_eq!(s.referrer, [0u8; 32]);
        let t = take_fees(&mut s, 10_500, true, -100_000, None).unwrap();
        assert_eq!(t.referral_fee, 0);
        assert_eq!((s.referral_owed, s.referral_fees_paid), (0, 0));
        assert_eq!(s.creator_fees_owed, 105, "the creator keeps 100 % going forward");
        // …and the same pool with a referrer WALLET splits, whatever @600 holds
        let mut r = s;
        r.creator_fees_owed = 0;
        r.referrer = [0x5Eu8; 32];
        let t = take_fees(&mut r, 10_500, true, -100_000, None).unwrap();
        assert_eq!(t.referral_fee, 10);
        assert_eq!(r.referral_owed, 10);
    }

    /// THE TRADER PAYS THE SAME EITHER WAY. The split decides who the already-taken lamports are
    /// owed to, never how many are taken -- so `total` (the hook delta) and the record's fee split
    /// are identical on a referred and an unreferred pool.
    ///
    /// Mutation this kills: taking the referral as an EXTRA fee on top of the creator fee (adding
    /// it into `total`), which would silently raise every referred pool's price.
    #[test]
    fn the_split_never_changes_what_the_trader_pays() {
        let leg = 10_000_000_000u64;
        let (mut plain, mut refd) = (meme(1200), referred(1200));
        let a = take_fees(&mut plain, leg, false, 1200, None).unwrap();
        let b = take_fees(&mut refd, leg, false, 1200, None).unwrap();
        assert_eq!(a.total, b.total, "the hook delta is unchanged");
        assert_eq!(a.creator_fee, b.creator_fee, "the record's creator figure is GROSS on both");
        assert_eq!((a.skim_fee, a.cut, a.kept), (b.skim_fee, b.cut, b.kept));
        assert_eq!(a.referral_fee, 0);
        assert_eq!(b.referral_fee, 10_000_000); // 10 % of the 100_000_000 gross creator fee
        // and the QUOTE LIABILITY the pool carries is the same number on both, just split in two
        // through the ONE spelling, so this case also fails if a liability is dropped from it
        assert_eq!(
            plain.quote_liabilities(),
            refd.quote_liabilities(),
            "I-3's quote sum is invariant under the split"
        );
    }

    /// `if (bps > 10_000) revert` on the EVM: a share past 10_000 is refused, and -- as everywhere in this module -- the refusal
    /// leaves the state untouched.
    ///
    /// Mutation this kills: dropping the bound, which makes `referral > cfee` and underflows
    /// `cfee - referral` (wrapping the creator's accrual in release).
    #[test]
    fn a_referral_share_past_ten_thousand_is_refused_atomically() {
        let mut s = referred(0);
        s.creator_buy_bps = 100;
        s.referral_share_bps = 10_001;
        let before = s;
        assert_eq!(
            take_fees(&mut s, 10_500, true, -100_000, None),
            Err(FloorMathError::ReferralShareOutOfRange)
        );
        assert_eq!(s, before);
        // exactly 10_000 is legal: the whole creator fee funds referrals and the creator accrues 0
        let mut all = referred(0);
        all.creator_buy_bps = 100;
        all.referral_share_bps = 10_000;
        let t = take_fees(&mut all, 10_500, true, -100_000, None).unwrap();
        assert_eq!((t.creator_fee, t.referral_fee), (105, 105));
        assert_eq!((all.creator_fees_owed, all.referral_owed), (0, 105));
    }

    /// `if (referral != 0)` on the EVM: a share that rounds to nothing books nothing and leaves the
    /// whole fee with the creator -- no zero-value meter bump, so the indexer sees no funding event.
    ///
    /// Mutation this kills: dropping the `referral != 0` guard, which would write a 0 into both
    /// meters and (in the program) emit a `ReferralFunded` for zero lamports.
    #[test]
    fn a_share_that_rounds_to_zero_books_nothing() {
        let mut s = referred(0);
        s.creator_buy_bps = 100;
        // gross creator fee 9; 9 * 1000 / 10_000 = 0.9 -> 0
        let t = take_fees(&mut s, 900, true, -100_000, None).unwrap();
        assert_eq!((t.creator_fee, t.referral_fee), (9, 0));
        assert_eq!((s.creator_fees_owed, s.referral_owed, s.referral_fees_paid), (9, 0, 0));
        // and a zero SHARE on a real destination is the same no-op
        let mut z = referred(0);
        z.creator_buy_bps = 100;
        z.referral_share_bps = 0;
        let t = take_fees(&mut z, 10_500, true, -100_000, None).unwrap();
        assert_eq!((t.referral_fee, z.referral_owed, z.creator_fees_owed), (0, 0, 105));
    }

    /// The module's all-or-nothing contract, extended to the two new meters.
    ///
    /// Mutation this kills: booking the referral straight onto `*state` instead of the `next` copy,
    /// which would leave a half-applied fee behind when a later add overflows.
    #[test]
    fn referral_overflow_is_atomic() {
        let mut s = referred(0);
        s.creator_buy_bps = 100;
        s.referral_owed = u64::MAX;
        let before = s;
        assert_eq!(take_fees(&mut s, 10_500, true, -100_000, None), Err(FloorMathError::Overflow));
        assert_eq!(s, before, "an overflowing referral leaves the skim booking behind too");

        let mut p = referred(0);
        p.creator_buy_bps = 100;
        p.referral_fees_paid = u64::MAX;
        let before = p;
        assert_eq!(take_fees(&mut p, 10_500, true, -100_000, None), Err(FloorMathError::Overflow));
        assert_eq!(p, before);
    }

    /// ⛔ THE FLOOR'S FREE BUYS WIN (a buy far above the cushion pays more, unless the floor's taxless buys apply):
    /// figure, so a buy at or inside the bid's dear edge (the ramp at its at-floor 0) pays 0 with the switch ON and OFF,
    /// however far the price stands above the cushion; and partway down the ramp it never pays more than 1.5 x the ramp.
    #[test]
    fn the_floors_free_buys_win_over_the_gap_multiplier() {
        let ramp = |distance| crate::ramp::skim_bps(distance, SKIM_RAMP_TICKS, true, 100, 100, 0, 1_500);
        for distance in [-500, -120, 0] {
            let off = ramp(distance);
            assert_eq!(off, 0, "today's ramp: a buy at the bid is free");
            for (mode, from, to) in [(1, 152, 305), (1, 1_054, 2_232), (2, 1_054, 2_232), (2, 1_804, 4_005)] {
                assert_eq!(gap_skim_with(off, -50_000, Some(0), mode, from, to), off, "ON = OFF at the bid, gap 50,000 t");
            }
        }
        for distance in [1, 4_000, 8_000, SKIM_RAMP_TICKS - 1] {
            let off = ramp(distance);
            let on = gap_skim_with(off, -50_000, Some(0), 1, 1_054, 2_232);
            assert!(on >= off && on <= off * 3 / 2, "partway down the ramp: at most 1.5 x the ramp ({off} -> {on})");
        }
    }

    /// THE GAP BUY SKIM: the cliff at G and the linear ramp from G/2, buys only, the ramp's
    /// figure scaled (the at-floor 0 stays 0), capped at 15 % — the EVM twin's `_gapSkim`, case for case.
    #[test]
    fn the_gap_buy_skim_boosts_only_buys_past_the_gap() {
        // `g(bps, gap, mode, from, to)`: the price `gap` ticks dearer than a cushion top at 0
        let g = |bps, gap: i32, mode, from, to| gap_skim_with(bps, -gap, Some(0), mode, from, to);
        // the cliff at 10 % (1,054)
        assert_eq!(g(75, 1_053, 1, 513, 1_054), 75);
        assert_eq!(g(75, 1_054, 1, 513, 1_054), 112, "75 x 1.5 floored");
        assert_eq!(g(75, 1_000, 1, 513, 1_054), 75, "mode 1 has no ramp");
        assert_eq!(g(100, 50_000, 1, 1_804, 4_005), 150);
        assert_eq!(g(0, 50_000, 1, 513, 1_054), 0, "the at-floor ramp to 0 stays");
        assert_eq!(gap_skim_with(75, -50_000, None, 1, 513, 1_054), 75, "no cushion standing");
        assert_eq!(g(75, -10, 1, 513, 1_054), 75, "the price under the cushion top");
        // the cap
        assert_eq!(g(1_000, 50_000, 1, 513, 1_054), 1_500);
        assert_eq!(g(1_200, 50_000, 1, 513, 1_054), 1_500);
        // linear from G/2 (513) to G (1,054)
        assert_eq!(g(100, 513, 2, 513, 1_054), 100);
        assert_eq!(g(100, 1_054, 2, 513, 1_054), 150);
        assert_eq!(g(100, 783, 2, 513, 1_054), 124, "270/541 of the way: m = 12,495");
        let mut last = 100;
        for d in 513..=1_100 {
            let b = g(100, d, 2, 513, 1_054);
            assert!(b >= last && b <= 150, "monotone in the gap");
            last = b;
        }
    }

    /// ⛔ THE SHIPPED SETTING, pinned on its own: G 40 % (5,109 t), × 1.5, reached by the LINEAR RAMP from G/2 = 20 %
    /// (2,232 t) (mode 2). Through `take_fees` the boost reaches the skim only, and only while ON.
    #[test]
    fn the_gap_buy_skim_ships_on_at_40_percent() {
        assert_eq!(
            (STAIR_GAP_SKIM, STAIR_GAP_SKIM_TICKS, STAIR_GAP_SKIM_FROM_TICKS, STAIR_GAP_SKIM_MULT_BPS, STAIR_GAP_SKIM_MAX_BPS),
            (2, 5_109, 2_232, 15_000, 1_500)
        );
        assert!(cushion_gap_skim_on());
        // The ramp: at or under G/2 the ramp's figure, then 1 x rising linearly in ticks to 1.5 x at G (floored), sells never.
        for d in [-5_000, 0, 2_000, 2_232] {
            assert_eq!(gap_buy_skim_bps(75, true, -d, Some(0)), 75, "at or under G/2: unchanged at gap {d}");
        }
        // m = 10,000 + 5,000 x (gap - 2,232) / 2,877, then 100 x m / 10,000 floored
        for (d, want) in [(2_233, 100), (2_808, 110), (3_670, 124), (4_000, 130), (5_108, 149)] {
            assert_eq!(gap_buy_skim_bps(100, true, -d, Some(0)), want, "inside the ramp: gap {d}");
            assert_eq!(gap_buy_skim_bps(100, false, -d, Some(0)), 100, "a sell is never boosted");
        }
        let mut last = 100;
        for d in 2_232..=5_200 {
            let b = gap_buy_skim_bps(100, true, -d, Some(0));
            assert!(b >= last && b <= 150, "monotone in the gap, at most 1.5 x");
            last = b;
        }
        for d in [5_109, 10_000] {
            assert_eq!(gap_buy_skim_bps(75, true, -d, Some(0)), 112, "past G: 1.5 x at gap {d}");
            assert_eq!(gap_buy_skim_bps(75, false, -d, Some(0)), 75, "a sell is never boosted");
        }
        assert_eq!(gap_buy_skim_bps(75, true, -10_000, None), 75, "no cushion: no boost");
        assert_eq!(gap_buy_skim_bps(0, true, -10_000, Some(0)), 0, "the floor's free buy stays free");
        let mut s = FloorState::default();
        s.tick_spacing = 60;
        s.floor_tick = 200_000; // the floor's price far under the market (a cheaper price is a higher tick)
        s.skim_buy_far_bps = 100;
        s.skim_sell_far_bps = 100;
        s.creator_buy_bps = 50;
        let t = take_fees(&mut s, 1_000_000, true, 0, Some(20_000)).unwrap();
        assert_eq!((t.skim_bps, t.creator_fee), (150, 5_000), "ON: a 20,000-tick gap pays 1.5 x the skim, the creator fee unchanged");
        let mut s2 = s;
        let t2 = take_fees(&mut s2, 1_000_000, true, 0, Some(1_000)).unwrap();
        assert_eq!(t2.skim_bps, 100, "ON: under G/2 the skim is unchanged");
        let mut s3 = s;
        let t3 = take_fees(&mut s3, 1_000_000, true, 0, Some(4_000)).unwrap();
        assert_eq!((t3.skim_bps, t3.creator_fee), (130, 5_000), "ON: a 4,000-tick gap pays the ramp's 1.30 x, the creator fee unchanged");
    }


}
