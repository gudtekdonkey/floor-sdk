// SPDX-License-Identifier: Apache-2.0
use super::ask::{build, Ask, Kind};
use super::rows::{Row, StackRow};
use super::V2Error;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Violation {
    Structure,
    SupplyShort { excess: f64 },
    FloorBelowRatio { segment: usize },
    EmptyStretch,
    TargetUnreachable { stack: u8 },
    TargetShort { stack: u8, short: f64 },
    RangeCover { segment: usize, cover: f64 },
    OrderClamp { segment: usize },
}

pub fn r1_cover(phi: f64, s_a: f64, x: f64, a: f64, b: f64) -> f64 {
    let u = (b / a).sqrt();
    if phi * u >= 1.0 {
        return 0.0;
    }
    let need = phi * s_a * (u * u - 1.0) / (u * (1.0 - phi * u));
    if need <= 0.0 {
        f64::INFINITY
    } else {
        x / need
    }
}

fn ask_rules(row: &Row, a: &Ask, v: &mut Vec<Violation>) {
    let phi = row.phi_bps as f64 / 1e4;
    let clamp = (1.0 - row.w_bps as f64 / 1e4) * (1.0 - row.wc_bps() as f64 / 1e4);
    let (mut s, mut big_f, mut f) = (0.0f64, 0.0f64, 0.0f64);
    for (i, g) in a.segs.iter().enumerate() {
        let ranged = matches!(g.kind, Kind::Climb | Kind::Sale) && g.b > g.a * (1.0 + 1e-12);
        if ranged && g.a < a.tail_start * (1.0 - 1e-12) {
            let c = r1_cover(phi, s, g.tokens, g.a, g.b);
            if c < 1.0 - 1e-9 {
                v.push(Violation::RangeCover { segment: i, cover: c });
            }
        }
        if g.kind == Kind::Tail {
            break;
        }
        s += g.tokens;
        big_f += g.to_floor;
        let f_new = if s > 0.0 { f.max(big_f / s) } else { f };
        if g.kind != Kind::Bonding && f_new > f * (1.0 + 1e-12) && f_new > clamp * g.b * (1.0 + 1e-9) {
            v.push(Violation::OrderClamp { segment: i });
        }
        f = f_new;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    TailBps(u16),
    ScaleMilestones(f64),
    TopLevel(u128),
}

#[derive(Clone, Debug)]
pub struct Report {
    pub ok: bool,
    pub violations: Vec<Violation>,
    pub sold: f64,
    pub allowed: f64,
    pub room_top_level: Option<u128>,
    pub fix: Option<Fix>,
}

fn violations_of(row: &Row) -> (Vec<Violation>, Option<Ask>) {
    violations_with(row, build(row))
}

fn violations_with(row: &Row, built: Result<Ask, V2Error>) -> (Vec<Violation>, Option<Ask>) {
    match built {
        Err(V2Error::TargetUnreachable(i)) => (vec![Violation::TargetUnreachable { stack: i }], None),
        Err(_) => (vec![Violation::Structure], None),
        Ok(a) => {
            let mut v = vec![];
            if a.supply_excess > 1e-9 {
                v.push(Violation::SupplyShort { excess: a.supply_excess });
            }
            if let Some(g) = a.floor_break {
                v.push(Violation::FloorBelowRatio { segment: g });
            }
            if !a.contiguous {
                v.push(Violation::EmptyStretch);
            }
            for (i, s) in a.stacks.iter().enumerate() {
                if !s.skipped && s.short > 1e-6 * (s.milestone + s.short).max(1.0) {
                    v.push(Violation::TargetShort { stack: i as u8, short: s.short });
                }
            }
            ask_rules(row, &a, &mut v);
            (v, Some(a))
        }
    }
}

pub fn check_keeping_targets(row: &Row) -> Report {
    let (violations, ask) = violations_with(row, super::ask::build_keeping_targets(row));
    let allowed = 1.0 - row.tail_bps as f64 / 1e4;
    let sold = ask.as_ref().map_or(0.0, |a| a.supply_after_stacks);
    Report { ok: violations.is_empty(), violations, sold, allowed, room_top_level: None, fix: None }
}

pub fn passes(row: &Row) -> bool {
    violations_of(row).0.is_empty()
}

fn with_top_level(row: &Row, level: u128) -> Row {
    let mut r = row.clone();
    if let Some(t) = r.stacks.last_mut() {
        t.level = level;
    }
    r
}

fn scaled(row: &Row, k: f64) -> Row {
    let mut r = row.clone();
    for s in &mut r.stacks {
        s.milestone = (s.milestone as f64 * k).floor() as u128;
    }
    r
}

fn room(row: &Row) -> Option<u128> {
    let lvl = row.stacks.last()?.level;
    let (mut lo, mut hi) = (lvl as f64, lvl as f64 * 1e6);
    if passes(&with_top_level(row, hi as u128)) {
        return Some(hi as u128);
    }
    for _ in 0..80 {
        let mid = (lo * hi).sqrt();
        if passes(&with_top_level(row, mid as u128)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo as u128)
}

pub fn check(row: &Row) -> Report {
    let (violations, ask) = violations_of(row);
    let allowed = 1.0 - row.tail_bps as f64 / 1e4;
    let sold = ask.as_ref().map_or(0.0, |a| a.supply_after_stacks);
    let ok = violations.is_empty();
    let mut rep = Report { ok, violations, sold, allowed, room_top_level: None, fix: None };
    if ok {
        rep.room_top_level = room(row);
        return rep;
    }
    if rep.violations.iter().any(|v| matches!(v, Violation::SupplyShort { .. })) {
        let mut best: Option<(f64, Fix)> = None;
        let mut offer = |rel: f64, f: Fix| {
            if rel.is_finite() && best.map_or(true, |(r, _)| rel < r) {
                best = Some((rel, f));
            }
        };
        let tail_new = (10_000.0 - (sold * 1e4).ceil()).max(0.0);
        if tail_new >= 1.0 {
            let mut t = row.clone();
            t.tail_bps = tail_new as u16;
            if passes(&t) {
                offer(1.0 - tail_new / row.tail_bps.max(1) as f64, Fix::TailBps(tail_new as u16));
            }
        }
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        if passes(&scaled(row, 0.0)) {
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if passes(&scaled(row, mid)) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            offer(1.0 - lo, Fix::ScaleMilestones(lo));
        }
        if row.stacks.len() > 1 {
            let prev = row.stacks[row.stacks.len() - 2].level as f64;
            let top = row.stacks.last().unwrap().level as f64;
            let (mut lo, mut hi) = (prev * 1.0001, top);
            if passes(&with_top_level(row, lo as u128)) {
                for _ in 0..80 {
                    let mid = (lo * hi).sqrt();
                    if passes(&with_top_level(row, mid as u128)) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                offer(1.0 - lo / top, Fix::TopLevel(lo as u128));
            }
        }
        rep.fix = best.map(|(_, f)| f);
    }
    rep
}

pub fn fit_stacks(row: &Row) -> Row {
    let mut best: Option<(f64, Row)> = None;
    for n in (1..=row.stacks.len()).rev() {
        let mut r = row.clone();
        r.stacks.truncate(n);
        if let (true, Ok(a)) = (passes(&r), build(&r)) {
            let t = a.milestones_total();
            if best.as_ref().map_or(true, |(b, _)| t > *b) {
                best = Some((t, r));
            }
        }
    }
    best.map(|(_, r)| r).unwrap_or_else(|| {
        let mut r = row.clone();
        r.stacks.truncate(1);
        r
    })
}

#[allow(dead_code)]
fn _unused(_: StackRow) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::rows;

    #[test]
    fn the_spec_rows_pass_and_report_room() {
        for r in [rows::moonshot(), rows::wagmi(), rows::floor_genesis(), rows::wagmi_genesis(1_000_000)] {
            let rep = check(&r);
            assert!(rep.ok, "{}: {:?}", r.name, rep);
            assert!(rep.room_top_level.unwrap() >= r.stacks.last().unwrap().level, "{}", r.name);
        }
    }

    #[test]
    fn wagmi_theta_options_are_the_check_fix() {
        for (theta, k) in rows::WAGMI_THETA_OPTIONS {
            assert_eq!(k, 1_000_000, "θ {theta}");
            let w = rows::wagmi_at(theta).unwrap();
            assert_eq!(w.theta_bps, theta);
            for (s, m) in w.stacks.iter().zip(rows::WAGMI_SPEC_MILESTONES) {
                assert_eq!(s.milestone, m * k / 1_000_000, "θ {theta} {}", s.level);
            }
            let rep = check(&w);
            assert!(rep.ok && rep.sold <= rep.allowed && rep.fix.is_none(), "θ {theta}: {rep:?}");
        }
        assert!(rows::wagmi_at(300).is_none());
        assert_eq!(rows::wagmi(), rows::wagmi_at(rows::WAGMI_THETA_BPS).unwrap());
        assert_eq!(rows::moonshot().theta_bps, 1_000);
    }

    #[test]
    fn infeasible_rows_are_rejected() {
        let mut r = rows::moonshot();
        r.stacks[6].milestone *= 3;
        assert!(!check(&r).ok);
        let mut r = rows::moonshot();
        r.theta_bps = 10_000;
        assert!(check(&r).violations.contains(&Violation::TargetUnreachable { stack: 0 }));
        let mut r = rows::wagmi();
        r.stacks[0].milestone = 10u128.pow(12);
        assert!(!check(&r).ok);
        let mut r = rows::wagmi();
        r.depth_x100 = 50;
        assert_eq!(check(&r).violations, vec![Violation::Structure]);
    }

    #[test]
    fn moonshot_and_wagmi_match_the_spec_within_the_stated_gaps() {
        for (theta, sold, unused) in [(250u16, 0.899965, 1_588.28), (100, 0.895884, 184_722.19), (50, 0.894551, 244_539.80), (0, 0.893231, 303_759.23)] {
            let w = build(&rows::wagmi_at(theta).unwrap()).unwrap();
            assert!((w.supply_after_stacks - sold).abs() < 0.0001, "θ {theta}: {}", w.supply_after_stacks);
            assert!((w.milestones_stacks / 11.55e6 - 1.0).abs() < 1e-9, "θ {theta}: {}", w.milestones_stacks);
            assert!((w.unused_milestone / unused - 1.0).abs() < 0.001, "θ {theta}: {}", w.unused_milestone);
        }
        let w = build(&rows::wagmi()).unwrap();
        assert!(w.segs.len() <= 48, "ranges {}", w.segs.len());
        let m = build(&rows::moonshot()).unwrap();
        assert!((m.milestones_stacks / 3.972e6 - 1.0).abs() < 0.001, "{}", m.milestones_stacks);
        assert!((m.ck_at_tail / 441.5e3 - 1.0).abs() < 0.01, "{}", m.ck_at_tail);
        assert!((m.supply_after_stacks - 0.97992).abs() < 0.001, "{}", m.supply_after_stacks);
    }

    #[test]
    fn wagmi_genesis_follows_the_overflow_table() {
        let want = [
            (1e3, 38.584e6, 38.5888e6),
            (1e4, 38.513e6, 38.5185e6),
            (1e5, 37.810e6, 37.8145e6),
            (5e5, 34.681e6, 34.6862e6),
            (1e6, 30.771e6, 30.7756e6),
            (1_711_410.0, 25.207e6, 25.2117e6),
            (5e6, 25.207e6, 25.2117e6),
            (3e7, 25.207e6, 25.2117e6),
        ];
        for (ov, target, ms) in want {
            let r = rows::wagmi_genesis(ov as u128);
            assert_eq!(r.launch_cap, rows::WAGMI_GENESIS_CAP_USD, "the fixed cap");
            assert_eq!(r.bonding_raise, (ov as u128).min(rows::WAGMI_GENESIS_KEEP_USD), "the kept raise");
            assert_eq!(r.stacks[0].milestone as f64, target, "target {ov}");
            let a = build(&r).unwrap();
            assert!((a.milestones_total() / ms - 1.0).abs() < 1e-4, "Milestones {ov}: {}", a.milestones_total());
            assert!(a.unused_share > rows::WAGMI_GENESIS_UNUSED_MAX && a.unused_share < 2e-4, "unused {ov}: {}", a.unused_share);
            assert!((a.tail_start / 179.512863e6 - 1.0).abs() < 1e-6, "the band ends where the Tail starts: {}", a.tail_start);
            assert!(check(&r).ok, "{ov}");
        }
    }

    #[test]
    fn floor_genesis_phi_sweep_matches_the_spec() {
        let want = [
            (1_000u16, 330.75e6, 1e9),
            (1_500, 209.47e6, 1e9),
            (2_000, 23.99e6, 100e6),
            (2_500, 12.28e6, 100e6),
            (3_000, 4.046e6, 50e6),
            (3_500, 2.035e6, 10e6),
            (4_000, 1.265e6, 10e6),
            (4_500, 0.8275e6, 5e6),
            (5_000, 0.4770e6, 5e6),
        ];
        for (phi, team, tail) in want {
            let r = fit_stacks(&rows::floor_genesis_phi_study(phi));
            let a = build(&r).unwrap();
            assert!((a.tail_start / tail - 1.0).abs() < 1e-9, "φ {phi}: tail {}", a.tail_start);
            assert!((a.milestones_total() / team - 1.0).abs() < 0.001, "φ {phi}: team {} want {team}", a.milestones_total());
            assert!(check(&r).ok, "φ {phi}");
        }
    }

    #[test]
    fn r1_rejects_a_depth_1_climb_and_a_wide_band_in_one_range() {
        for mut r in [rows::moonshot(), rows::wagmi(), rows::floor_genesis()] {
            r.depth_x100 = 100;
            assert!(check(&r).violations.iter().any(|v| matches!(v, Violation::RangeCover { .. })), "{}", r.name);
        }
        let a = build(&rows::moonshot()).unwrap();
        let (s_a, x): (f64, f64) = {
            let i = a.segs.iter().position(|g| g.kind == Kind::Sale && g.a == 9e6).unwrap();
            let s_a = a.segs[..i].iter().map(|g| g.tokens).sum();
            let x = a.segs.iter().filter(|g| g.kind == Kind::Sale && g.a >= 9e6 && g.b <= 7.5e7).map(|g| g.tokens).sum();
            (s_a, x)
        };
        let one = r1_cover(0.1, s_a, x, 9e6, 7.5e7);
        assert!((one - 1.0848).abs() < 0.001, "{one}");
        let pieces = a.segs.iter().filter(|g| g.kind == Kind::Sale && g.a >= 9e6 && g.b <= 7.5e7).count();
        assert_eq!(pieces, 4);
        assert!(check(&rows::moonshot()).ok);
    }

    #[test]
    fn the_order_clamp_rejects_a_floor_above_the_cushion() {
        let mut r = rows::wagmi();
        r.w_bps = 2_500;
        assert!(check(&r).violations.iter().any(|v| matches!(v, Violation::OrderClamp { .. })), "{:?}", check(&r));
        for r in [rows::moonshot(), rows::wagmi(), rows::floor_genesis(), rows::wagmi_genesis(1_000_000)] {
            assert!(!check(&r).violations.iter().any(|v| matches!(v, Violation::OrderClamp { .. })));
        }
    }
}
