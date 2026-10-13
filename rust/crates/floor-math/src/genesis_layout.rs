//! ⛔ THE STEP LAYOUT F. `$FLOOR`'s 897.5M staircase steps, split per step between the PROTOCOL and the TEAM: the
//! protocol has 197M to sell and sells in every step, including some in step 3; the team sells 45M at the top of step
//! 3 and the rest from step 4 on.
//!
//! With `N` steps (indices `0..N`, "step 1" is index 0):
//!
//! | group | tokens | where | shape |
//! |---|---|---|---|
//! | protocol, low | 60 % of 197M = 118.2M | steps 1–3 (indices `0..=2`) | each 0.95 × the one below |
//! | protocol, high | the other 40 % = 78.8M | steps 4..N (indices `3..N`) | each 0.95 × the one below |
//! | team, step 3 | 45M ([`GENESIS_TEAM_STEP3_TOKENS`]) | step 3 (index 2) | — |
//! | team, the rest | 655.5M | steps 4..N (indices `3..N`) | each 0.95 × the one below |
//!
//! So the protocol has tokens in EVERY step, the team none in steps 1–2. Each group's weights are
//! [`crate::stair::step_tokens`]' (`w_0 = 1e18`, `w_k = floor(w_{k−1} · 9,500 / 10,000)`, floored shares), but the
//! rounding DUST goes to the TOP step of its group — never step 0. Every group sums EXACTLY.
//!
//! ⛔ THE TEAM IS FIXED, THE PROTOCOL IS THE REST: graduation's deposit is 897.5M + 10M only when the 42.5M sale sold
//! out. The sale's UNSOLD tokens (phase 1 under 80 SOL, phase 2 under 90 SOL) join the deposit, and the hook gives the
//! tail exactly its 10M on this pool (`stair_tail_split_supply`), so EVERY unsold token lands on the steps — and is the
//! PROTOCOL's (`protocol = steps_total − 700.5M`), spread 60 / 40 like the 197M with the protocol's shape. The team's
//! 700.5M never moves. A deposit under 700.5M is refused ([`FloorMathError::BadSplit`]).
//!
//! ⛔ N ≥ 4 ([`GENESIS_MIN_STAIR_STEPS`]): the layout needs step 4 (the high groups' home). A typical raise gives
//! N = 12–15 under a $2.56B top, so the floor binds only on a raise of thousands of SOL or a top set too low, where
//! `end_genesis_sale` takes 4 steps instead of fewer ([`genesis_layout_steps`]).

use crate::error::{FloorMathError, FloorMathResult};
use crate::stair::{MAX_STAIR_STEPS, STEP_WEIGHT_ONE};

const N: usize = MAX_STAIR_STEPS as usize;
const BPS: u128 = 10_000;

/// The protocol's steps: 197M (19.7 %).
pub const GENESIS_PROTOCOL_STAIR_TOKENS: u64 = 197_000_000_000_000;
/// The team's steps: **700.5M** (70.05 %) — 45M in step 3 and 655.5M over steps 4..N.
pub const GENESIS_TEAM_STAIR_TOKENS: u64 = 700_500_000_000_000;
/// The team's tokens in step 3 (index 2): 45M ("Team should only sell 45m at top of step 3").
pub const GENESIS_TEAM_STEP3_TOKENS: u64 = 45_000_000_000_000;
/// The protocol's share in steps 1–3, bps (60 %); the other 40 % is spread over steps 4..N.
pub const GENESIS_PROTOCOL_LOW_BPS: u16 = 6_000;
/// Steps 1–3: the protocol's low group (indices `0..3`).
pub const GENESIS_LOW_STEPS: u8 = 3;
/// The least step count layout F admits: step 4 must exist.
pub const GENESIS_MIN_STAIR_STEPS: u8 = 4;
/// The 0.95 shape of every group (`$FLOOR`'s growth, `stair::GENESIS_STAIR_GROWTH_BPS`).
pub const GENESIS_LAYOUT_GROWTH_BPS: u16 = crate::stair::GENESIS_STAIR_GROWTH_BPS;

const _: () = assert!(GENESIS_PROTOCOL_STAIR_TOKENS + GENESIS_TEAM_STAIR_TOKENS == 897_500_000_000_000);
const _: () = assert!(GENESIS_TEAM_STEP3_TOKENS < GENESIS_TEAM_STAIR_TOKENS);

/// The per-step owners of `$FLOOR`'s steps. Entries past the step count are 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenesisLayout {
    pub protocol: [u64; N],
    pub team: [u64; N],
}

impl GenesisLayout {
    /// Each step's whole amount (`protocol + team`): the staircase's planned `step_tokens` on `$FLOOR`'s pool.
    pub fn planned(&self) -> [u64; N] {
        let mut out = [0u64; N];
        for i in 0..N {
            out[i] = self.protocol[i] + self.team[i]; // Σ ≤ steps_total ≤ u64::MAX
        }
        out
    }
}

/// `total` over indices `from..to` (non-empty), each `0.95 ×` the one below, floored; the dust to index `to − 1`.
fn front_loaded(total: u64, out: &mut [u64; N], from: usize, to: usize) {
    let n = to - from;
    let mut w = [0u128; N];
    w[0] = STEP_WEIGHT_ONE;
    for k in 1..n {
        w[k] = w[k - 1] * GENESIS_LAYOUT_GROWTH_BPS as u128 / BPS;
    }
    let sum: u128 = w[..n].iter().sum(); // ≤ 16e18
    let mut given = 0u64;
    for k in 0..n {
        // total < 2^64 and w ≤ 1e18 < 2^60: the product fits u128
        let x = (total as u128 * w[k] / sum) as u64;
        out[from + k] = x;
        given += x;
    }
    out[to - 1] += total - given; // the dust (< n units) to the TOP step of the group
}

/// LAYOUT F of `steps_total` (what graduation handed the steps) over `steps` steps. `steps` must be
/// [`GENESIS_MIN_STAIR_STEPS`]`..=16` and `steps_total ≥` [`GENESIS_TEAM_STAIR_TOKENS`] ([`FloorMathError::BadSplit`]).
/// `Σ protocol = steps_total − 700.5M` and `Σ team = 700.5M`, exactly.
pub fn genesis_layout(steps_total: u64, steps: u8) -> FloorMathResult<GenesisLayout> {
    if !(GENESIS_MIN_STAIR_STEPS..=MAX_STAIR_STEPS).contains(&steps) || steps_total < GENESIS_TEAM_STAIR_TOKENS {
        return Err(FloorMathError::BadSplit);
    }
    let n = steps as usize;
    let low = GENESIS_LOW_STEPS as usize;
    let protocol_total = steps_total - GENESIS_TEAM_STAIR_TOKENS;
    let protocol_low = (protocol_total as u128 * GENESIS_PROTOCOL_LOW_BPS as u128 / BPS) as u64;
    let mut protocol = [0u64; N];
    front_loaded(protocol_low, &mut protocol, 0, low);
    front_loaded(protocol_total - protocol_low, &mut protocol, low, n);
    let mut team = [0u64; N];
    team[low - 1] = GENESIS_TEAM_STEP3_TOKENS;
    front_loaded(GENESIS_TEAM_STAIR_TOKENS - GENESIS_TEAM_STEP3_TOKENS, &mut team, low, n);
    Ok(GenesisLayout { protocol, team })
}

/// The planned per-step tokens of a staircase: LAYOUT F on `$FLOOR`'s team pool (`layout_f`: `stair_kind` GENESIS and
/// `stair_owner` TEAM), else [`crate::stair::step_tokens`] — the ONE spelling `floor_hook` reads at graduation and at
/// every reserve split, so the two can never disagree.
pub fn planned_steps(layout_f: bool, total: u64, steps: u8, growth_bps: u16) -> FloorMathResult<[u64; N]> {
    if layout_f {
        Ok(genesis_layout(total, steps)?.planned())
    } else {
        crate::stair::step_tokens(total, steps, growth_bps)
    }
}

/// `end_genesis_sale`'s count under layout F: [`crate::genesis::genesis_stair_steps`]' count, raised to at least
/// [`GENESIS_MIN_STAIR_STEPS`] (every count up to 16 fits above every open the genesis clamp allows —
/// `sgr_m7_the_step_count_is_the_nearest_doubling_to_the_top`).
pub fn genesis_layout_steps(top_mcap: u64, open_tick: i32, spacing: i32) -> FloorMathResult<u8> {
    Ok(crate::genesis::genesis_stair_steps(top_mcap, open_tick, spacing)?.max(GENESIS_MIN_STAIR_STEPS))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: u64 = 1_000_000_000_000; // 1M tokens in base units (6 decimals)
    const STEPS: u64 = 897_500 * M / 1_000;

    fn sums(l: &GenesisLayout) -> (u64, u64) {
        (l.protocol.iter().sum(), l.team.iter().sum())
    }

    /// Rounded to 0.1M — the worked table.
    fn tenths(xs: &[u64]) -> Vec<u64> {
        xs.iter().map(|&x| (x + M / 20) / (M / 10)).collect()
    }

    /// The worked layout at N = 12, in millions (the team's 700.5M) — and pinned to the base unit.
    #[test]
    fn lf_1_the_worked_layout_at_twelve_steps() {
        let l = genesis_layout(STEPS, 12).unwrap();
        assert_eq!(tenths(&l.protocol[..12]), [414, 394, 374, 107, 101, 96, 91, 87, 82, 78, 74, 71]);
        assert_eq!(tenths(&l.team[..12]), [0, 0, 450, 886, 842, 800, 760, 722, 686, 652, 619, 588]);
        assert_eq!(
            l.protocol[..12],
            [
                41_437_335_670_464,
                39_365_468_886_941,
                37_397_195_442_595,
                10_655_831_535_158,
                10_123_039_958_400,
                9_616_887_960_480,
                9_136_043_562_456,
                8_679_241_384_333,
                8_245_279_315_117,
                7_833_015_349_361,
                7_441_364_581_893,
                7_069_296_352_802,
            ]
        );
        assert_eq!(
            l.team[..12],
            [
                0,
                0,
                45_000_000_000_000,
                88_640_832_123_051,
                84_208_790_516_898,
                79_998_350_991_053,
                75_998_433_441_501,
                72_198_511_769_425,
                68_588_586_180_954,
                65_159_156_871_906,
                61_901_199_028_311,
                58_806_139_076_901,
            ]
        );
        assert_eq!(sums(&l), (GENESIS_PROTOCOL_STAIR_TOKENS, GENESIS_TEAM_STAIR_TOKENS));
        assert_eq!(l.planned().iter().sum::<u64>(), STEPS);
    }

    /// N = 4, 13, 15, 16 — every group sums exactly, the dust at the TOP of its group, the protocol in every step and
    /// the team in none of steps 1–2, each group 0.95 × the step below (to the unit, the top step + its dust).
    #[test]
    fn lf_2_every_count_sums_exactly_and_keeps_its_shape() {
        for n in [4u8, 12, 13, 15, 16] {
            let l = genesis_layout(STEPS, n).unwrap();
            let n = n as usize;
            assert_eq!(sums(&l), (GENESIS_PROTOCOL_STAIR_TOKENS, GENESIS_TEAM_STAIR_TOKENS), "N = {n}");
            assert_eq!(l.protocol[..3].iter().sum::<u64>(), 118_200 * M / 1_000, "60 % of 197M in steps 1–3");
            assert_eq!(l.protocol[3..n].iter().sum::<u64>(), 78_800 * M / 1_000, "40 % over steps 4..N");
            assert_eq!(l.team[2], GENESIS_TEAM_STEP3_TOKENS);
            assert_eq!(l.team[3..n].iter().sum::<u64>(), 655_500 * M / 1_000);
            assert_eq!((l.team[0], l.team[1]), (0, 0));
            assert!(l.protocol[..n].iter().all(|&x| x > 0), "the protocol sells every step");
            assert!(l.protocol[n..].iter().chain(&l.team[n..]).all(|&x| x == 0), "nothing past N");
            for (group, from, to) in [(&l.protocol, 0, 3), (&l.protocol, 3, n), (&l.team, 3, n)] {
                for k in from + 1..to - 1 {
                    let r = group[k] as f64 / group[k - 1] as f64;
                    assert!((r - 0.95).abs() < 1e-9, "N = {n}, step {k}: {r}");
                }
                // the dust is < the group's step count, and it sits on the top step only
                let top = group[to - 1] as f64 / group[to - 2] as f64;
                assert!((top - 0.95).abs() < 1e-9 || to - from == 1, "N = {n}: top {top}");
            }
        }
        // N = 4: the high groups are one step each
        let l = genesis_layout(STEPS, 4).unwrap();
        assert_eq!((l.protocol[3], l.team[3]), (78_800 * M / 1_000, 655_500 * M / 1_000));
        // N = 13, 15, 16: the high groups, base unit for base unit (steps 1–3 are the same at every N)
        let pins: [(u8, &[u64], &[u64]); 3] = [
            (13, &[9_818_995_031_642, 9_328_045_280_060, 8_861_643_016_057, 8_418_560_865_254, 7_997_632_821_991, 7_597_751_180_892, 7_217_863_621_847, 6_856_970_440_755, 6_514_121_918_717, 6_188_415_822_785],
                 &[81_679_584_305_096, 77_595_605_089_841, 73_715_824_835_349, 70_030_033_593_582, 66_528_531_913_903, 63_202_105_318_208, 60_042_000_052_297, 57_039_900_049_682, 54_187_905_047_198, 51_478_509_794_844]),
            (15, &[8_571_927_489_856, 8_143_331_115_363, 7_736_164_559_595, 7_349_356_331_615, 6_981_888_515_035, 6_632_794_089_283, 6_301_154_384_819, 5_986_096_665_578, 5_686_791_832_299, 5_402_452_240_684, 5_132_329_628_650, 4_875_713_147_223],
                 &[71_305_818_142_146, 67_740_527_235_038, 64_353_500_873_286, 61_135_825_829_622, 58_079_034_538_141, 55_175_082_811_234, 52_416_328_670_672, 49_795_512_237_138, 47_305_736_625_282, 44_940_449_794_017, 42_693_427_304_317, 40_558_755_939_107]),
            (16, &[8_096_035_972_354, 7_691_234_173_736, 7_306_672_465_049, 6_941_338_841_797, 6_594_271_899_707, 6_264_558_304_721, 5_951_330_389_485, 5_653_763_870_011, 5_371_075_676_510, 5_102_521_892_685, 4_847_395_798_051, 4_605_026_008_148, 4_374_774_707_746],
                 &[67_347_101_267_488, 63_979_746_204_113, 60_780_758_893_908, 57_741_720_949_212, 54_854_634_901_752, 52_111_903_156_664, 49_506_307_998_831, 47_030_992_598_889, 44_679_442_968_945, 42_445_470_820_497, 40_323_197_279_473, 38_307_037_415_499, 36_391_685_544_729]),
        ];
        for (n, p, t) in pins {
            let l = genesis_layout(STEPS, n).unwrap();
            assert_eq!(&l.protocol[3..n as usize], p, "N = {n}: the protocol's high group");
            assert_eq!(&l.team[3..n as usize], t, "N = {n}: the team's high group");
            assert_eq!(l.protocol[..3], [41_437_335_670_464, 39_365_468_886_941, 37_397_195_442_595]);
        }
    }

    /// The refusals and the zero raise's bigger deposit (the excess is the protocol's, the team's 700.5M never moves).
    #[test]
    fn lf_3_refusals_and_the_protocol_takes_any_excess() {
        assert!(genesis_layout(STEPS, 3).is_err() && genesis_layout(STEPS, 17).is_err() && genesis_layout(STEPS, 0).is_err());
        assert!(genesis_layout(GENESIS_TEAM_STAIR_TOKENS - 1, 12).is_err());
        let l = genesis_layout(GENESIS_TEAM_STAIR_TOKENS, 12).unwrap();
        assert_eq!(sums(&l), (0, GENESIS_TEAM_STAIR_TOKENS), "a deposit of exactly the team's: the protocol holds nothing");
        let more = STEPS + 42_500 * M / 1_000; // a zero raise: the whole 42.5M sale unsold, on the steps
        let l = genesis_layout(more, 13).unwrap();
        assert_eq!(sums(&l), (more - GENESIS_TEAM_STAIR_TOKENS, GENESIS_TEAM_STAIR_TOKENS));
        // the planned spelling: layout F only when asked, the 0.95 staircase otherwise
        assert_eq!(planned_steps(true, STEPS, 12, 9_500).unwrap(), genesis_layout(STEPS, 12).unwrap().planned());
        assert_eq!(planned_steps(false, STEPS, 12, 9_500).unwrap(), crate::stair::step_tokens(STEPS, 12, 9_500).unwrap());
    }

    /// The count is raised to 4 and never otherwise moved. The open is fixed, so the top alone decides — 12 under the
    /// suite's $2.56B; a top at or near the open takes 4, not 1.
    #[test]
    fn lf_4_the_step_count_is_at_least_four() {
        let t = crate::genesis::genesis_open_tick().unwrap().0;
        let at = |top: u64| (crate::genesis::genesis_stair_steps(top, t, 60).unwrap(), genesis_layout_steps(top, t, 60).unwrap());
        assert_eq!(at(17_066_666_666_666_667), (12, 12), "$2.56B at $150");
        assert_eq!(at(1), (1, 4), "a top under the open: 1 → 4");
        assert_eq!(at(crate::genesis::GENESIS_OPEN_MCAP_LAMPORTS * 8), (3, 4), "three doublings: 3 → 4");
        assert_eq!(at(crate::genesis::GENESIS_OPEN_MCAP_LAMPORTS * 16), (4, 4));
    }
}
