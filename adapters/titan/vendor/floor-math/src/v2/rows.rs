// SPDX-License-Identifier: Apache-2.0
use super::V2Error;

pub const MAX_STACKS: usize = 9;
pub const MAX_BANDS: usize = 4;
pub const BPS: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MilestoneClass {
    Team = 0,
    Holder = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackRow {
    pub level: u128,
    pub milestone: u128,
    pub spread_bps: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u8,
    pub name: &'static str,
    pub phi_bps: u16,
    pub w_bps: u16,
    pub bands: Vec<u16>,
    pub theta_bps: u16,
    pub depth_x100: u16,
    pub climb_step_x100: u16,
    pub spread_bps: u16,
    pub tail_bps: u16,
    pub psi_t_bps: u16,
    pub bonding_raise: u128,
    pub launch_cap: u128,
    pub stacks: Vec<StackRow>,
    pub class: MilestoneClass,
    pub platform_ms_bps: u16,
    pub floor_ms_bps: u16,
    pub team_buy_bps: u16,
    pub team_slot_max_bps: u16,
    pub ck_bps: u16,
    pub fee_curve: Option<FeeCurveRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeCurveRow {
    pub start_ppm: [u32; 5],
    pub end_ppm: [u32; 5],
    pub from_usd: u128,
    pub to_usd: u128,
}

pub const FEE_FROM_GRAD: u128 = 0;
pub const FEE_TO_TAIL: u128 = 0;

pub fn fee_curve(start_ppm: [u32; 5], end_ppm: [u32; 5]) -> Option<FeeCurveRow> {
    Some(FeeCurveRow { start_ppm, end_ppm, from_usd: FEE_FROM_GRAD, to_usd: FEE_TO_TAIL })
}

const WAGMI_FEES: ([u32; 5], [u32; 5]) = ([10_000, 0, 2_000, 500, 1_000], [2_000, 0, 500, 100, 1_000]);
const HODL_FEES: ([u32; 5], [u32; 5]) = ([2_000, 0, 2_000, 500, 9_000], [1_000, 0, 500, 100, 2_000]);
const WAGMI_GENESIS_FEES: ([u32; 5], [u32; 5]) = ([6_000, 0, 2_000, 500, 0], [6_000, 0, 500, 100, 0]);
const MOONSHOT_FEES: ([u32; 5], [u32; 5]) = ([2_000, 9_000, 2_000, 500, 0], [1_000, 2_000, 500, 100, 0]);
const FLOOR_GENESIS_FEES: ([u32; 5], [u32; 5]) = ([3_000, 0, 2_000, 0, 0], [2_000, 0, 500, 0, 0]);

impl Row {
    pub fn wc_bps(&self) -> u32 {
        self.bands.iter().map(|b| *b as u32).sum()
    }

    pub fn depth(&self) -> f64 {
        self.depth_x100 as f64 / 100.0
    }

    pub fn team_buy_bounds(&self) -> (u128, u128) {
        (0, self.bonding_raise * self.team_buy_bps as u128 / BPS)
    }

    pub fn validate(&self) -> Result<(), V2Error> {
        let bad = self.id == 0
            || self.id == 2
            || self.phi_bps == 0
            || self.phi_bps >= 10_000
            || self.w_bps >= 10_000
            || self.theta_bps > 10_000
            || self.tail_bps >= 10_000
            || self.psi_t_bps == 0
            || self.psi_t_bps > 10_000
            || self.depth_x100 < 100
            || self.climb_step_x100 <= 100
            || self.spread_bps > 10_000
            || self.bands.is_empty()
            || self.bands.len() > MAX_BANDS
            || self.wc_bps() >= 10_000
            || self.stacks.is_empty()
            || self.stacks.len() > MAX_STACKS
            || self.bonding_raise == 0
            || self.launch_cap <= self.bonding_raise
            || self.platform_ms_bps as u32 + self.floor_ms_bps as u32 > 10_000
            || self.team_buy_bps > 10_000
            || self.team_slot_max_bps > 9_500
            || self.stacks.iter().any(|s| s.level == 0 || s.spread_bps.map_or(false, |x| x > 10_000))
            || self.stacks.windows(2).any(|w| w[1].level <= w[0].level);
        if bad {
            Err(V2Error::RowInvalid)
        } else {
            Ok(())
        }
    }
}

pub const DEPTH_X100: u16 = 101;
pub const STEP_PHI10_X100: u16 = 150;
pub const STEP_PHI50_X100: u16 = 120;

pub fn r1_depth_x100(phi_bps: u16, step_x100: u16) -> Option<u16> {
    let phi = phi_bps as f64 / 1e4;
    let r = step_x100 as f64 / 100.0;
    let u = r.sqrt();
    if phi * u >= 1.0 {
        return None;
    }
    let need = phi * (r - 1.0) / (u * (1.0 - phi * u));
    (100..=400u16).find(|d| r.powf(*d as f64 / 100.0 / (1.0 / phi - 1.0)) - 1.0 >= need)
}

const fn st(level: u128, milestone: u128) -> StackRow {
    StackRow { level, milestone, spread_bps: None }
}

fn base(id: u8, name: &'static str) -> Row {
    Row {
        id,
        name,
        phi_bps: 1_000,
        w_bps: 2_500,
        bands: vec![300],
        theta_bps: 0,
        depth_x100: DEPTH_X100,
        climb_step_x100: STEP_PHI10_X100,
        spread_bps: 0,
        tail_bps: 4_000,
        psi_t_bps: 8_000,
        bonding_raise: 0,
        launch_cap: 0,
        stacks: vec![],
        class: MilestoneClass::Team,
        platform_ms_bps: 0,
        floor_ms_bps: 0,
        team_buy_bps: 10_000,
        team_slot_max_bps: 0,
        ck_bps: 0,
        fee_curve: None,
    }
}

pub const CK_TARGET_CIRC_BPS: u16 = 500;
pub const CK_TARGET_DEFAULT: u16 = CK_TARGET_CIRC_BPS;
pub const MOONSHOT_CK_TARGET: u16 = 700;

pub fn with_ck_target(row: &Row, ck_bps: u16) -> Row {
    Row { ck_bps, ..row.clone() }
}

pub fn moonshot() -> Row {
    Row {
        theta_bps: 1_000,
        spread_bps: 10_000,
        bonding_raise: 2_000,
        launch_cap: 100_000,
        tail_bps: 200,
        stacks: vec![
            st(250_000, 12_000),
            st(450_000, 20_000),
            st(500_000, 60_000),
            st(1_000_000, 150_000),
            st(2_500_000, 250_000),
            st(5_000_000, 700_000),
            st(9_000_000, 1_500_000),
            StackRow { level: 75_000_000, milestone: 1_280_000, spread_bps: Some(144) },
        ],
        platform_ms_bps: 1_000,
        floor_ms_bps: 1_000,
        team_slot_max_bps: 6_000,
        ck_bps: MOONSHOT_CK_TARGET,
        fee_curve: fee_curve(MOONSHOT_FEES.0, MOONSHOT_FEES.1),
        ..base(3, "moonshot-10")
    }
}

pub const WAGMI_SPEC_MILESTONES: [u128; 1] = [11_550_000];
pub const WAGMI_END_USD: u128 = 177_730_000;
pub const WAGMI_THETA_OPTIONS: [(u16, u128); 4] = [(250, 1_000_000), (100, 1_000_000), (50, 1_000_000), (0, 1_000_000)];
pub const WAGMI_THETA_BPS: u16 = 250;
pub const WAGMI_MS_SCALE_PPM: u128 = match wagmi_ms_scale_ppm(WAGMI_THETA_BPS) {
    Some(k) => k,
    None => panic!("WAGMI_THETA_BPS is not a listed option"),
};

pub const fn wagmi_ms_scale_ppm(theta_bps: u16) -> Option<u128> {
    let mut i = 0;
    while i < WAGMI_THETA_OPTIONS.len() {
        if WAGMI_THETA_OPTIONS[i].0 == theta_bps {
            return Some(WAGMI_THETA_OPTIONS[i].1);
        }
        i += 1;
    }
    None
}

pub fn wagmi_at(theta_bps: u16) -> Option<Row> {
    let k = wagmi_ms_scale_ppm(theta_bps)?;
    let mut r = wagmi_base();
    r.theta_bps = theta_bps;
    for (s, m) in r.stacks.iter_mut().zip(WAGMI_SPEC_MILESTONES) {
        s.milestone = m * k / 1_000_000;
    }
    Some(r)
}

pub fn wagmi() -> Row {
    wagmi_at(WAGMI_THETA_BPS).expect("WAGMI_THETA_BPS is a listed option")
}

pub const STEP_WAGMI_X100: u16 = 109;

fn wagmi_base() -> Row {
    Row {
        phi_bps: 7_500,
        w_bps: 2_000,
        climb_step_x100: STEP_WAGMI_X100,
        tail_bps: 1_000,
        bonding_raise: 1_000,
        launch_cap: 10_000_000,
        stacks: vec![st(WAGMI_END_USD, WAGMI_SPEC_MILESTONES[0])],
        spread_bps: 144,
        class: MilestoneClass::Holder,
        platform_ms_bps: 5_000,
        floor_ms_bps: 5_000,
        ck_bps: CK_TARGET_DEFAULT,
        fee_curve: fee_curve(WAGMI_FEES.0, WAGMI_FEES.1),
        ..base(1, "wagmi-75")
    }
}

pub const WAGMI_SPEC_MILESTONES_BEFORE_FINAL: [u128; 6] = [18_000, 30_000, 60_000, 120_000, 300_000, 110_000_000];
pub fn wagmi_spec_before_final() -> Row {
    let levels = [20_000_000, 40_000_000, 60_000_000, 80_000_000, 100_000_000, 1_000_000_000];
    Row {
        phi_bps: 5_000,
        climb_step_x100: STEP_PHI50_X100,
        bonding_raise: 10_000,
        launch_cap: 10_000_000,
        stacks: levels.iter().zip(WAGMI_SPEC_MILESTONES_BEFORE_FINAL).map(|(l, m)| st(*l, m)).collect(),
        class: MilestoneClass::Holder,
        ck_bps: CK_TARGET_DEFAULT,
        ..base(1, "wagmi-75")
    }
}
pub fn wagmi_a139_before_final() -> Row {
    let mut r = wagmi_spec_before_final();
    r.theta_bps = 250;
    for s in &mut r.stacks {
        s.milestone = s.milestone * 992_700 / 1_000_000;
    }
    r
}

pub const FLOOR_GENESIS_PHI_BPS: u16 = 5_000;

pub fn floor_genesis() -> Row {
    let phi_bps = FLOOR_GENESIS_PHI_BPS;
    let mut r = Row {
        phi_bps,
        climb_step_x100: STEP_PHI50_X100,
        depth_x100: r1_depth_x100(phi_bps, STEP_PHI50_X100).unwrap_or(400).max(DEPTH_X100),
        bonding_raise: 2_000,
        launch_cap: 5_000_000,
        team_buy_bps: 0,
        team_slot_max_bps: 0,
        ..moonshot()
    };
    r.id = 4;
    r.name = "floor-genesis";
    r.ck_bps = CK_TARGET_DEFAULT;
    r.fee_curve = fee_curve(FLOOR_GENESIS_FEES.0, FLOOR_GENESIS_FEES.1);
    let band = |level, milestone| StackRow { level, milestone, spread_bps: Some(1_000) };
    r.stacks = vec![
        band(20_000_000, 125_000),
        band(90_000_000, 1_125_000),
        band(500_000_000, 102_400_000),
    ];
    r
}

pub fn floor_genesis_phi_study(phi_bps: u16) -> Row {
    Row {
        phi_bps,
        climb_step_x100: if phi_bps <= 1_000 { STEP_PHI10_X100 } else { STEP_PHI50_X100 },
        depth_x100: r1_depth_x100(phi_bps, if phi_bps <= 1_000 { STEP_PHI10_X100 } else { STEP_PHI50_X100 })
            .unwrap_or(400)
            .max(DEPTH_X100),
        bonding_raise: 20_000,
        launch_cap: 800_000,
        stacks: vec![
            st(1_000_000, 10_000),
            st(5_000_000, 50_000),
            st(10_000_000, 100_000),
            st(50_000_000, 500_000),
            st(100_000_000, 1_000_000),
            st(1_000_000_000, 10_000_000),
        ],
        team_buy_bps: 0,
        team_slot_max_bps: 0,
        ck_bps: CK_TARGET_DEFAULT,
        ..base(4, "floor-genesis")
    }
}

pub fn hodl() -> Row {
    let mut r = Row {
        phi_bps: 3_000,
        climb_step_x100: STEP_PHI50_X100,
        depth_x100: 125,
        launch_cap: 2_000_000,
        bonding_raise: 12_600,
        class: MilestoneClass::Holder,
        team_slot_max_bps: 0,
        ..moonshot()
    };
    r.id = 6;
    r.name = "hodl";
    r.ck_bps = CK_TARGET_DEFAULT;
    r.fee_curve = fee_curve(HODL_FEES.0, HODL_FEES.1);
    let band = |level, milestone| StackRow { level, milestone, spread_bps: Some(144) };
    r.stacks = vec![
        band(50_000_000, 1_000_000),
        band(100_000_000, 2_000_000),
        band(250_000_000, 5_000_000),
        band(500_000_000, 198_961_000),
    ];
    r
}

pub const WAGMI_GENESIS_CAP_USD: u128 = 75_530_000;
pub const WAGMI_GENESIS_KEEP_USD: u128 = 1_711_410;
pub const WAGMI_GENESIS_UNUSED_MAX: f64 = 1e-4;

pub fn wagmi_genesis_at(raise: u128, target: u128) -> Row {
    let mut r = Row {
        launch_cap: WAGMI_GENESIS_CAP_USD,
        bonding_raise: raise,
        team_buy_bps: 0,
        team_slot_max_bps: 0,
        ..wagmi()
    };
    r.id = 5;
    r.name = "wagmi-genesis";
    r.fee_curve = fee_curve(WAGMI_GENESIS_FEES.0, WAGMI_GENESIS_FEES.1);
    r.stacks[0].milestone = target;
    r
}

pub fn wagmi_genesis_target(raise: u128) -> u128 {
    let unused = |t: u128| crate::v2::ask::build(&wagmi_genesis_at(raise, t)).ok().map(|a| (a.unused_share, a.supply_excess, a.stacks[0].skipped));
    match unused(0) {
        Some((_, excess, skipped)) if excess <= 0.0 && !skipped => {}
        _ => return 0,
    }
    let (mut lo, mut hi) = (0u128, 1_000_000_000u128);
    while hi - lo > 1 {
        let m = (lo + hi) / 2;
        match unused(m) {
            Some((u, _, _)) if u > WAGMI_GENESIS_UNUSED_MAX => lo = m,
            _ => hi = m,
        }
    }
    lo / 1_000 * 1_000
}

pub fn wagmi_genesis(overflow: u128) -> Row {
    let raise = overflow.min(WAGMI_GENESIS_KEEP_USD);
    wagmi_genesis_at(raise, wagmi_genesis_target(raise))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::ask::build;
    use crate::v2::feasibility::check;

    #[test]
    fn moonshot_is_the_built_row_and_feasible() {
        let r = moonshot();
        assert_eq!((r.id, r.name, r.theta_bps, r.spread_bps), (3, "moonshot-10", 1_000, 10_000));
        assert_eq!((r.launch_cap, r.bonding_raise, r.tail_bps), (100_000, 2_000, 200));
        assert_eq!((r.platform_ms_bps, r.floor_ms_bps, r.team_slot_max_bps), (1_000, 1_000, 6_000));
        let want = [
            (250_000, 12_000),
            (450_000, 20_000),
            (500_000, 60_000),
            (1_000_000, 150_000),
            (2_500_000, 250_000),
            (5_000_000, 700_000),
            (9_000_000, 1_500_000),
            (75_000_000, 1_280_000),
        ];
        assert_eq!(r.stacks.iter().map(|s| (s.level, s.milestone)).collect::<Vec<_>>(), want);
        assert!(r.stacks[..7].iter().all(|s| s.spread_bps.is_none()) && r.stacks[7].spread_bps == Some(144));
        r.validate().unwrap();
        let rep = check(&r);
        assert!(rep.ok, "{rep:?}");
        assert!((rep.sold - 0.979926).abs() < 1e-6 && (rep.allowed - 0.98).abs() < 1e-9, "{rep:?}");
        assert_eq!(rep.room_top_level, Some(79_158_720));
        let a = build(&r).unwrap();
        assert!((a.tail_start / 75.75235e6 - 1.0).abs() < 1e-6, "{}", a.tail_start);
        assert!((a.milestones_stacks / 3.972e6 - 1.0).abs() < 1e-9, "{}", a.milestones_stacks);
        assert!((a.milestones_total() / 3.9770373e6 - 1.0).abs() < 1e-6, "{}", a.milestones_total());
    }

    #[test]
    fn fees_ramp_from_graduation_to_the_tail() {
        let want: [(Row, [u32; 5], [u32; 5]); 5] = [
            (wagmi(), [10_000, 0, 2_000, 500, 1_000], [2_000, 0, 500, 100, 1_000]),
            (hodl(), [2_000, 0, 2_000, 500, 9_000], [1_000, 0, 500, 100, 2_000]),
            (wagmi_genesis(1_000_000), [6_000, 0, 2_000, 500, 0], [6_000, 0, 500, 100, 0]),
            (moonshot(), [2_000, 9_000, 2_000, 500, 0], [1_000, 2_000, 500, 100, 0]),
            (floor_genesis(), [3_000, 0, 2_000, 0, 0], [2_000, 0, 500, 0, 0]),
        ];
        for (r, start, end) in want {
            let v = crate::v2::preset::from_design(&r).unwrap();
            assert_eq!((v.fee_start_ppm, v.fee_ppm), (start, end), "{}", r.name);
            let usd = |rel: u64| (rel as f64 / crate::v2::preset::FEE_REL_ONE as f64).powi(2) * r.launch_cap as f64;
            assert!((usd(v.fee_a_rel) / r.launch_cap as f64 - 1.0).abs() < 1e-6, "{}: {}", r.name, usd(v.fee_a_rel));
            assert!((usd(v.fee_b_rel) / build(&r).unwrap().tail_start - 1.0).abs() < 1e-6, "{}: {}", r.name, usd(v.fee_b_rel));
            let ok = if matches!(r.id, 4 | 5) { crate::v2::preset::spec_ok_genesis(&v) } else { crate::v2::preset::spec_ok(&v) };
            assert!(ok.is_ok(), "{}", r.name);
        }
    }

    #[test]
    fn hodl_is_the_built_row_and_feasible() {
        let r = hodl();
        let m = moonshot();
        assert_eq!((r.id, r.name, r.phi_bps, r.climb_step_x100, r.depth_x100, r.class), (6, "hodl", 3_000, 120, 125, MilestoneClass::Holder));
        assert_eq!((r.launch_cap, r.bonding_raise, r.team_buy_bps, r.team_slot_max_bps), (2_000_000, 12_600, m.team_buy_bps, 0));
        assert_eq!((r.theta_bps, r.spread_bps, r.tail_bps, r.platform_ms_bps, r.floor_ms_bps), (m.theta_bps, m.spread_bps, 200, 1_000, 1_000));
        assert_eq!(r.ck_bps, CK_TARGET_DEFAULT);
        assert_eq!(r.fee_curve, fee_curve(HODL_FEES.0, HODL_FEES.1));
        let want = [(50_000_000, 1_000_000), (100_000_000, 2_000_000), (250_000_000, 5_000_000), (500_000_000, 198_961_000)];
        assert_eq!(r.stacks.iter().map(|s| (s.level, s.milestone)).collect::<Vec<_>>(), want);
        assert!(r.stacks.iter().all(|s| s.spread_bps == Some(144)));
        r.validate().unwrap();
        let rep = check(&r);
        assert!(rep.ok, "{rep:?}");
        let mut more = r.clone();
        more.stacks[3].milestone += 1_000;
        assert!(!check(&more).ok);
        assert!((rep.sold - 0.979998).abs() < 1e-5, "{rep:?}");
        let a = build(&r).unwrap();
        assert_eq!(a.segs.len(), 38);
        assert!((a.milestones_stacks / 206.961e6 - 1.0).abs() < 1e-9, "{}", a.milestones_stacks);
        assert!((a.milestones_total() / 206.961778e6 - 1.0).abs() < 1e-5, "{}", a.milestones_total());
        assert!(a.unused_share < 1e-4, "{}", a.unused_share);
        assert!((a.tail_start / 505.015649e6 - 1.0).abs() < 1e-8, "{}", a.tail_start);
        assert!((a.floor_at_tail / 148.474601e6 - 1.0).abs() < 1e-5, "{}", a.floor_at_tail);
        for (s, want) in a.stacks.iter().zip([1e6, 3e6, 8e6, 206.961e6]) {
            assert!((s.milestone_running / want - 1.0).abs() < 1e-12, "{}: {}", s.level, s.milestone_running);
        }
        let v = crate::v2::preset::from_design(&r).unwrap();
        assert_eq!(crate::v2::preset::spec_ok(&v), Ok(()));
    }

    #[test]
    fn floor_genesis_is_the_built_row_and_feasible() {
        let r = floor_genesis();
        let m = moonshot();
        assert_eq!((r.id, r.name, r.phi_bps, r.climb_step_x100), (4, "floor-genesis", 5_000, 120));
        assert_eq!(r.depth_x100, r1_depth_x100(5_000, 120).unwrap().max(DEPTH_X100));
        assert_eq!((r.launch_cap, r.bonding_raise, r.team_buy_bps, r.team_slot_max_bps), (5_000_000, 2_000, 0, 0));
        assert_eq!((r.theta_bps, r.spread_bps, r.tail_bps, r.class), (m.theta_bps, m.spread_bps, m.tail_bps, m.class));
        assert_eq!((r.platform_ms_bps, r.floor_ms_bps, r.ck_bps, m.ck_bps), (1_000, 1_000, CK_TARGET_DEFAULT, MOONSHOT_CK_TARGET));
        let want = [(20_000_000, 125_000), (90_000_000, 1_125_000), (500_000_000, 102_400_000)];
        assert_eq!(r.stacks.iter().map(|s| (s.level, s.milestone)).collect::<Vec<_>>(), want);
        assert!(r.stacks.iter().all(|s| s.spread_bps == Some(1_000)));
        r.validate().unwrap();
        let rep = check(&r);
        assert!(rep.ok, "{rep:?}");
        let mut more = r.clone();
        more.stacks[2].milestone += 1_100_000;
        assert!(!check(&more).ok);
        assert!((rep.allowed - 0.98).abs() < 1e-12 && (rep.sold - 0.975240).abs() < 1e-5, "{rep:?}");
        assert_eq!(rep.room_top_level, Some(529_186_122));
        let a = build(&r).unwrap();
        assert_eq!(a.segs.len(), 31);
        assert!((a.milestones_stacks / 103.65e6 - 1.0).abs() < 1e-9, "{}", a.milestones_stacks);
        assert!((a.milestones_total() / 104.925461e6 - 1.0).abs() < 1e-6, "{}", a.milestones_total());
        assert!((a.unused_share - 0.004760).abs() < 1e-6, "{}", a.unused_share);
        assert!((a.tail_start / 535.886731e6 - 1.0).abs() < 1e-8, "{}", a.tail_start);
        assert!((a.floor_at_tail / 262.584498e6 - 1.0).abs() < 1e-6, "{}", a.floor_at_tail);
        for (s, want) in a.stacks.iter().zip([125e3, 1.25e6, 103.65e6]) {
            assert!((s.milestone_running / want - 1.0).abs() < 1e-12, "{}: {}", s.level, s.milestone_running);
        }
        let (mut f, mut x) = (0.0, 0.0);
        for g in a.segs.iter().filter(|g| g.kind != crate::v2::ask::Kind::Tail) {
            f += g.to_floor;
            x += g.tokens;
            assert!(f / (x * g.b) >= 0.5 - 1e-9, "floor ÷ price at {}: {}", g.b, f / (x * g.b));
        }
        let v = crate::v2::preset::from_design(&r).unwrap();
        assert_eq!(crate::v2::preset::spec_ok(&v), Ok(()));
    }

    #[test]
    fn wagmi_is_the_built_row_and_feasible() {
        let r = wagmi();
        assert_eq!((r.id, r.name, r.theta_bps, r.spread_bps), (1, "wagmi-75", WAGMI_THETA_BPS, 144));
        assert_eq!((r.phi_bps, r.w_bps, r.climb_step_x100, r.depth_x100), (7_500, 2_000, 109, DEPTH_X100));
        assert_eq!((r.launch_cap, r.bonding_raise, r.tail_bps), (10_000_000, 1_000, 1_000));
        assert_eq!((r.class, r.platform_ms_bps, r.floor_ms_bps), (MilestoneClass::Holder, 5_000, 5_000));
        assert_eq!(r.stacks, vec![st(177_730_000, 11_550_000)]);
        assert_eq!(r.fee_curve, fee_curve(WAGMI_FEES.0, WAGMI_FEES.1));
        assert_eq!(r.ck_bps, CK_TARGET_DEFAULT);
        r.validate().unwrap();
        let rep = check(&r);
        assert!(rep.ok, "{rep:?}");
        assert!((rep.sold - 0.899965).abs() < 0.0001 && (rep.allowed - 0.9).abs() < 1e-9, "{rep:?}");
        assert_eq!(rep.room_top_level, Some(177_733_831));
        let mut up = r.clone();
        up.stacks[0].level += 10_000;
        assert!(!check(&up).ok, "the $177.73M level is the highest $10K step that fits");
        let a = build(&r).unwrap();
        assert_eq!(a.segs.len(), 38);
        assert!((a.tail_start / 179.512863e6 - 1.0).abs() < 1e-8, "{}", a.tail_start);
        assert!((a.milestones_stacks / 11.55e6 - 1.0).abs() < 1e-9, "{}", a.milestones_stacks);
        assert!((a.milestones_total() / 11.5516e6 - 1.0).abs() < 0.001, "{}", a.milestones_total());
        assert!(a.unused_share < 1e-4, "{}", a.unused_share);
        let v = crate::v2::preset::from_design(&r).unwrap();
        assert_eq!(crate::v2::preset::spec_ok(&v), Ok(()));
    }
}
