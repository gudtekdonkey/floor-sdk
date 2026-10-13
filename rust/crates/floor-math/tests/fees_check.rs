//! The 21 cases of the launchpad's fee checks (a float model of the take), restated in the contract's own integers.
//!
//! One test per fee check, in its order, named for the same mutation. The float model treats the
//! ramp as a float in LOG PRICE; the hook computes it in integer TICKS (the EVM's `FloorHookMath`).
//! Where the two meet exactly — at the bid (distance 0) and at or past the ramp (distance ≥ 16094) —
//! the integer figure equals the float figure to its printed precision. Between (the 2× case)
//! the float is an approximation of the contract, and each such case asserts the integer figure
//! exactly AND says how far the float sits from it (under 1 bp of the ETH leg).
//!
//! The all-in take of an exact-in quote leg is `1 − (1 − hook_bps/10^4)·(1 − 2500/10^6)`, with
//! `hook_bps` = skim + creator + holder + TEAM: the hook's parts are summed, the pool's LP
//! fee applies to what is left. In integers it is `(10^10 − (10^4 − h)·(10^6 − 2500))` parts per
//! 10^10.
//!
//! TEAM is the 0.25 % platform fee (`policy::TEAM_POOL_FEE_BPS_DEFAULT`, the pool DEFAULT; a pool keeps the setting
//! it was created with), a FIFTH part on top of the leg. The figures below are therefore FIVE-PART (Meme 1.496875 /
//! 11.471875 / 3.491875, …); the four-part figures (team 0) are what a zero-rate pool pays.
//!
//! Every case here is the 0 / 1000 at-floor curve, so `skim_bps` below is floor-math's 7-arg function at those
//! endpoints, and `pool()` puts the band's DEAR edge at tick 0 (`floor_tick = 120`) so every
//! `take_fees(…, tick)` distance still reads as "ticks above the floor".

#![allow(clippy::field_reassign_with_default)]

use floor_math::constants::{
    DEFAULT_SKIM_BUY_AT_FLOOR_BPS, DEFAULT_SKIM_SELL_AT_FLOOR_BPS, LP_FEE_PIPS, SKIM_RAMP_TICKS, TICKS_PER_DOUBLING,
};
use floor_math::{band_ticks, take_fees, FloorState};

/// The 7-arg ramp at the 0 / 1000 at-floor endpoints: the curve the float model models. A new pool's sell endpoint
/// is its far rate × its multiple (`constants::skim_sell_at_floor_bps`), so the rows below that sit AT the floor on a
/// sell describe the 0 / 1000 curve; f16 carries the multiple.
fn skim_bps(distance: i32, ramp: i32, buy: bool, buy_far: u16, sell_far: u16) -> u64 {
    floor_math::skim_bps(distance, ramp, buy, buy_far, sell_far, DEFAULT_SKIM_BUY_AT_FLOOR_BPS as u16, DEFAULT_SKIM_SELL_AT_FLOOR_BPS as u16)
}

/// The platform fee a pool pays on top: the pool DEFAULT — a pool created under a different setting pays its own
/// `team_fee_bps`, and these figures are the default pool's.
const TEAM: u64 = floor_math::policy::TEAM_POOL_FEE_BPS_DEFAULT as u64;

/// Total take in parts per 10^10 for hook bps `h` (skim + creator + holder + team) and the LP fee.
fn take_1e10(h: u64) -> u64 {
    10_000_000_000 - (10_000 - h) * (1_000_000 - LP_FEE_PIPS as u64)
}

fn pct(h: u64) -> f64 {
    take_1e10(h) as f64 / 1e8
}

fn near(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() < eps, "expected {b}, got {a} (Δ {})", (a - b).abs());
}

/// The hook bps for a side at `distance` ticks above the floor: skim + creator + holder + the platform
/// fee (TEAM).
fn hook(buy: bool, distance: i32, far: u16, creator: u64, holder: u64) -> u64 {
    skim_bps(distance, SKIM_RAMP_TICKS, buy, far, far) + creator + holder + TEAM
}

const EPS: f64 = 5e-4; // the float model's own tolerance

/// A pool state with the given far rate and flat fees, the band's DEAR edge at 0 (`floor_tick` = one
/// band above it), the default at-floor endpoints, and the platform fee a post-upgrade pool pays.
fn pool(far: u16, creator: u16, holder: u16) -> FloorState {
    let mut s = FloorState::default();
    s.tick_spacing = 60;
    s.floor_tick = band_ticks(60).unwrap();
    s.skim_buy_at_floor_bps = DEFAULT_SKIM_BUY_AT_FLOOR_BPS as u16;
    s.skim_sell_at_floor_bps = DEFAULT_SKIM_SELL_AT_FLOOR_BPS as u16;
    s.skim_buy_far_bps = far;
    s.skim_sell_far_bps = far;
    s.creator_buy_bps = creator;
    s.creator_sell_bps = creator;
    s.holder_buy_bps = holder;
    s.holder_sell_bps = holder;
    s.platform_bps = 250;
    s.team_fee_bps = TEAM as u16;
    s
}

// 1 — "RAMP_5X is the contract's 16094 ticks, not a round 5 — catches Math.log(5)"
#[test]
fn f01_ramp_is_16094_ticks_not_a_round_5x() {
    assert_eq!(SKIM_RAMP_TICKS, 16094);
    let r = (SKIM_RAMP_TICKS as f64 * 1.0001f64.ln()).exp();
    near(r, 4.999408, 1e-5);
    // a round 5x would be 16095.4 ticks
    assert_ne!(SKIM_RAMP_TICKS, (5f64.ln() / 1.0001f64.ln()).round() as i32);
}

// 2 — "constants match the hook — catches an LP fee or at-floor sell rate edited by hand"
#[test]
fn f02_constants_match_the_hook() {
    assert_eq!(LP_FEE_PIPS, 2500); // -> 0.25 %
    assert_eq!(DEFAULT_SKIM_SELL_AT_FLOOR_BPS, 1000); // -> 10 %
}

// 3 — "f is 0 at the bid — catches an inverted ramp": closeness is 10,000 at distance 0 (and under).
#[test]
fn f03_at_the_bid_closeness_is_full() {
    assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, true, 200, 200), 0);
    assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, false, 200, 200), 1000);
    assert_eq!(skim_bps(-60, SKIM_RAMP_TICKS, false, 200, 200), 1000);
}

// 4 — "f is 1 at exactly RAMP_5X and stays clamped above — catches a missing clamp"
#[test]
fn f04_far_rate_at_the_ramp_and_clamped_past_it() {
    for d in [SKIM_RAMP_TICKS, SKIM_RAMP_TICKS + 1, 100_000, i32::MAX] {
        assert_eq!(skim_bps(d, SKIM_RAMP_TICKS, true, 200, 200), 200);
        assert_eq!(skim_bps(d, SKIM_RAMP_TICKS, false, 200, 200), 200);
    }
    // Integer truncation makes the last ticks of the ramp a plateau: at 16,093 closeness is 1 and
    // 200·1/10^4 floors to 0, so the buy is already at the far rate; 100 ticks in, it is not.
    assert_eq!(skim_bps(SKIM_RAMP_TICKS - 1, SKIM_RAMP_TICKS, true, 200, 200), 200);
    assert_eq!(skim_bps(SKIM_RAMP_TICKS - 100, SKIM_RAMP_TICKS, true, 200, 200), 199);
}

// 5 — "f is linear in LOG price, not in price — catches a linear ramp": the hook's ramp is linear in
// TICKS, and a tick is log price. Halfway up is 8047 ticks = sqrt(4.9994)x = 2.236x, not 3x.
#[test]
fn f05_linear_in_ticks_which_is_log_price() {
    // closeness at 8047 = 10000 − 80_470_000/16094 = 10000 − 5000 = 5000
    assert_eq!(skim_bps(8047, SKIM_RAMP_TICKS, true, 200, 200), 100);
    assert_eq!(skim_bps(8047, SKIM_RAMP_TICKS, false, 200, 200), 600);
    let three_x = (3f64.ln() / 1.0001f64.ln()).round() as i32; // 10987 ticks
    assert!(skim_bps(three_x, SKIM_RAMP_TICKS, true, 200, 200) > 100, "3x is past halfway");
}

// 6 — "f is null when there is no pool — catches a far-rate fallback on a bonding launch". The hook
// has no null: a bonding sale never reaches it. The SALE pays its own rate (default 1 %), but never
// through the hook: still no null here. The integer analogue of the defect is quoting the far
// rate at the bid, which the ramp never does for either side while far < 10 %.
#[test]
fn f06_never_the_far_rate_at_the_bid() {
    for far in [100u16, 200, 500, 999] {
        assert_ne!(skim_bps(0, SKIM_RAMP_TICKS, true, far, far), far as u64);
        assert_ne!(skim_bps(0, SKIM_RAMP_TICKS, false, far, far), far as u64);
    }
}

// 7 — "a buy pays NO skim at the bid and the far rate at the top — catches a flat far rate"
#[test]
fn f07_buy_zero_at_bid_far_at_top() {
    assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, true, 200, 200), 0);
    assert_eq!(skim_bps(SKIM_RAMP_TICKS, SKIM_RAMP_TICKS, true, 200, 200), 200);
}

// 8 — "a sell pays 10% at the bid and the far rate at the top — catches an unramped sell"
#[test]
fn f08_sell_ten_at_bid_far_at_top() {
    assert_eq!(skim_bps(0, SKIM_RAMP_TICKS, false, 200, 200), 1000);
    assert_eq!(skim_bps(SKIM_RAMP_TICKS, SKIM_RAMP_TICKS, false, 200, 200), 200);
}

// 9 — "the two sides move OPPOSITE ways — catches a shared ramp direction"
#[test]
fn f09_sides_move_opposite_ways() {
    let quarter = SKIM_RAMP_TICKS / 4;
    let three_q = SKIM_RAMP_TICKS * 3 / 4;
    assert!(skim_bps(quarter, SKIM_RAMP_TICKS, true, 200, 200) < skim_bps(three_q, SKIM_RAMP_TICKS, true, 200, 200));
    assert!(skim_bps(quarter, SKIM_RAMP_TICKS, false, 200, 200) > skim_bps(three_q, SKIM_RAMP_TICKS, false, 200, 200));
}

// 10 — "a 10% far sell rate is flat, not ramped — catches the :546 early return" (FloorHookMath.sol:20)
#[test]
fn f10_ten_percent_far_sell_is_flat() {
    for d in [0, 8047, SKIM_RAMP_TICKS] {
        assert_eq!(skim_bps(d, SKIM_RAMP_TICKS, false, 1000, 1000), 1000);
    }
}

// 11 — "LP composes multiplicatively — catches `0.25 + skim + creator + holder`" (+ team)
#[test]
fn f11_lp_fee_composes_multiplicatively() {
    let h = hook(true, SKIM_RAMP_TICKS, 200, 100, 0);
    assert_eq!(take_1e10(h), 349_187_500); // 3.491875 % (3.2425 % without the platform fee)
    near(pct(h), 3.491875, EPS);
    assert!(pct(h) < 0.25 + 2.0 + 1.0 + 0.25);
}

// 12 — "Meme at the bid: 1.247% to buy, 11.222% to sell" — 1.497% / 11.472% with the platform fee.
#[test]
fn f12_meme_at_the_bid() {
    assert_eq!(take_1e10(hook(true, 0, 200, 100, 0)), 149_687_500);
    assert_eq!(take_1e10(hook(false, 0, 200, 100, 0)), 1_147_187_500);
    near(pct(hook(true, 0, 200, 100, 0)), 1.496875, EPS);
    near(pct(hook(false, 0, 200, 100, 0)), 11.471875, EPS);
}

// 13 — "Meme at or above 5×: both sides converge on 3.242%" — 3.492% with the platform fee.
#[test]
fn f13_meme_far_converges() {
    for d in [SKIM_RAMP_TICKS, 50_000] {
        near(pct(hook(true, d, 200, 100, 0)), 3.491875, EPS);
        near(pct(hook(false, d, 200, 100, 0)), 3.491875, EPS);
    }
}

// 14 — "Meme at 2× the bid: 2.11% buy / 7.79% sell — the asymmetry the ticket shows" — 2.36% / 8.03%
// with the platform fee. 2x is 6932 ticks. closeness = 10000 −
// floor(69_320_000/16094) = 5693; buy skim = 200 − floor(200·5693/10^4) = 87; sell = 200 +
// floor(800·5693/10^4) = 655. The contract's all-in figures are 2.3647 % / 8.0305 % with the platform fee
// (2.115325 % / 7.781125 % without); the float model (f = 0.4307082) plus the team term prints 2.35614 /
// 8.03483 (2.10676 / 7.78545 without), under 1 bp of the leg away — the integer truncation and the
// 0.2-tick rounding of ln2/ln1.0001.
#[test]
fn f14_meme_at_two_x() {
    assert_eq!(skim_bps(TICKS_PER_DOUBLING, SKIM_RAMP_TICKS, true, 200, 200), 87);
    assert_eq!(skim_bps(TICKS_PER_DOUBLING, SKIM_RAMP_TICKS, false, 200, 200), 655);
    assert_eq!(take_1e10(87 + 100 + TEAM), 236_470_000);
    assert_eq!(take_1e10(655 + 100 + TEAM), 803_050_000);
    near(pct(212), 2.35614, 0.01);
    near(pct(780), 8.03483, 0.01);
    assert!(pct(780) - pct(212) > 5.6, "the asymmetry is the mechanism");
}

// 15 — "Bluechip and Degen match the contract's table — catches a hardcoded Meme rate"
// ⛔ The case name mirrors the launchpad's fee check by name, so it is kept. This file imports no `Mode` at all:
// `hook()` takes raw bps, so the 100/100 pair below is a RATE PAIR, not a preset read, and it stays a legal Custom
// launch (the skim cap is 1000). ⛔ Re-anchoring the pair onto Meme would make it 3.242, which f14 already asserts,
// collapsing the exact mutation this case's own title says it kills. Every figure below carries the 0.25 % platform
// fee — Bluechip 0.998125 / 5.985625 / 10.973125, the 100/100 pair 2.494375 / 11.471875 (the four-part 0.74875 /
// 5.73625 / 10.72375 / 2.245 / 11.2225 at team 0).
#[test]
fn f15_bluechip_and_degen() {
    near(pct(hook(true, 0, 500, 50, 0)), 0.998125, EPS);
    near(pct(hook(true, SKIM_RAMP_TICKS, 500, 50, 0)), 5.985625, EPS);
    near(pct(hook(false, 0, 500, 50, 0)), 10.973125, EPS);
    near(pct(hook(true, SKIM_RAMP_TICKS, 100, 100, 0)), 2.494375, EPS);
    near(pct(hook(false, 0, 100, 100, 0)), 11.471875, EPS);
}

// 16 — "the worst legal custom launch" at the caps (skim 1000, creator 500, holder 1000 — the same on both chains;
// `policy.rs` asserts `MAX_HOLDER_BPS == MAX_HOLDER_BPS_EVM`), with the at-floor SELL endpoint 1.5x far, uncapped.
// The worst legal launch is 25.1875 % far from the floor (skim 10 + creator 5 + holder 10, plus the LP fee) and
// 30.175 % on a sell AT the floor (skim 15). With the pool default's 0.25 % platform fee on top (this file's figures
// are five-part): 25.436875 % far, 30.424375 % on a sell at the floor.
#[test]
fn f16_worst_legal_evm_launch() {
    use floor_math::constants::skim_sell_at_floor_bps;
    use floor_math::policy::{MAX_CREATOR_BPS, MAX_HOLDER_BPS_EVM, MAX_SKIM_FAR_BPS};
    assert_eq!((MAX_CREATOR_BPS, MAX_HOLDER_BPS_EVM, MAX_SKIM_FAR_BPS), (500, 1000, 1000));
    let h = hook(false, SKIM_RAMP_TICKS, MAX_SKIM_FAR_BPS, MAX_CREATOR_BPS as u64, MAX_HOLDER_BPS_EVM as u64);
    // five-part, the pool-default platform fee on top: 25.436875 % (25.1875 % without it)
    assert_eq!(h, 2525);
    assert_eq!(take_1e10(h), 2_543_687_500);
    near(pct(h), 25.436875, EPS);
    // at the floor, with the endpoints `register_pool` writes: 30.424375 % (30.175 % without the platform fee)
    let at = floor_math::skim_bps(0, SKIM_RAMP_TICKS, false, MAX_SKIM_FAR_BPS, MAX_SKIM_FAR_BPS, 0, skim_sell_at_floor_bps(MAX_SKIM_FAR_BPS))
        + MAX_CREATOR_BPS as u64
        + MAX_HOLDER_BPS_EVM as u64
        + TEAM;
    assert_eq!(at, 3025);
    assert_eq!(take_1e10(at), 3_042_437_500);
    near(pct(at), 30.424375, EPS);
}

// 17 — "the platform cut never appears in a quote — catches 2.5% leaking into the take". take_fees's
// total (the hook delta the trader pays) is skim + creator + team (the 0.25 % platform fee); the cut (the "`$FLOOR`
// cut") is a split of the skim.
#[test]
fn f17_platform_cut_is_a_split_not_a_take() {
    let mut s = pool(200, 100, 0);
    let t = take_fees(&mut s, 10_000_000_000, true, -SKIM_RAMP_TICKS, None).unwrap(); // far: distance 16094
    assert_eq!(t.total, 325_000_000); // 3.25 % of the leg (skim + creator + team), the hook's part of 3.491875 %
    assert_eq!(t.kept + t.cut, t.skim_fee);
    assert_eq!(t.cut, 5_000_000);
    near(pct(t.total * 10_000 / 10_000_000_000), 3.491875, EPS);
}

// 18 — "holder tax is counted — catches a dropped term when a launch has one"
#[test]
fn f18_holder_tax_is_counted() {
    let off = pct(hook(true, SKIM_RAMP_TICKS, 200, 100, 0));
    let on = pct(hook(true, SKIM_RAMP_TICKS, 200, 100, 200));
    assert!(on > off + 1.9);
    let mut s = pool(200, 100, 200);
    let t = take_fees(&mut s, 1_000_000, true, -SKIM_RAMP_TICKS, None).unwrap();
    assert_eq!((t.holder_fee, t.team_fee, t.total), (20_000, 2_500, 52_500));
}

// 19 — "a null ramp yields a null quote — catches a fee quoted with no pool". The integer analogue: a
// zero leg books and returns nothing.
#[test]
fn f19_zero_leg_is_zero_take() {
    let mut s = pool(200, 100, 0);
    let before = s;
    assert_eq!(take_fees(&mut s, 0, false, 0, None).unwrap().total, 0);
    assert_eq!(s, before);
}

// 20 — "frac nets an amount consistently with pct": 10 SOL gross out at the bid on Meme nets 8.8528125
// with the platform fee (8.87775 without).
#[test]
fn f20_ten_sol_sell_at_the_bid_nets_8_8528125() {
    let mut s = pool(200, 100, 0);
    let gross = 10_000_000_000u64;
    let t = take_fees(&mut s, gross, false, 0, None).unwrap();
    let after_hook = gross - t.total;
    let lp = after_hook * LP_FEE_PIPS as u64 / 1_000_000;
    assert_eq!(after_hook - lp, 8_852_812_500);
}

// 21 — "skim is reported separately and is the part that moves — catches a frozen component"
#[test]
fn f21_skim_rate_reported_separately() {
    let mut s = pool(200, 100, 0);
    assert_eq!(take_fees(&mut s, 1_000_000, false, 0, None).unwrap().skim_bps, 1000);
    assert_eq!(take_fees(&mut s, 1_000_000, false, -SKIM_RAMP_TICKS, None).unwrap().skim_bps, 200);
}
