// SPDX-License-Identifier: Apache-2.0
use super::rows::Row;
use super::V2Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bonding,
    Climb,
    Sale,
    Unused,
    Tail,
}

#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub kind: Kind,
    pub stack: u8,
    pub tokens: f64,
    pub a: f64,
    pub b: f64,
    pub money: f64,
    pub to_floor: f64,
    pub to_ck: f64,
    pub to_ms: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct StackReport {
    pub level: f64,
    pub top: f64,
    pub supply_after: f64,
    pub milestone: f64,
    pub milestone_running: f64,
    pub money_in: f64,
    pub to_floor: f64,
    pub skipped: bool,
    pub short: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Ask {
    pub segs: Vec<Seg>,
    pub stacks: Vec<StackReport>,
    pub bonding_tokens: f64,
    pub supply_after_stacks: f64,
    pub supply_excess: f64,
    pub milestones_stacks: f64,
    pub unused_share: f64,
    pub unused_milestone: f64,
    pub unused_floor: f64,
    pub ck_at_tail: f64,
    pub floor_at_tail: f64,
    pub f_end: f64,
    pub tail_start: f64,
    pub tail_tokens: f64,
    pub floor_break: Option<usize>,
    pub contiguous: bool,
}

impl Ask {
    pub fn milestones_total(&self) -> f64 {
        self.milestones_stacks + self.unused_milestone
    }
}

pub const SALE_STEP: f64 = 2.0;

fn pieces(a: f64, b: f64, step: f64) -> usize {
    if b <= a {
        1
    } else {
        ((b / a).ln() / step.ln() - 1e-9).ceil().max(1.0) as usize
    }
}

pub fn tail_cap(t0: f64, q: f64) -> f64 {
    t0 / ((1.0 - q) * (1.0 - q))
}

pub fn range_tokens(money: f64, a: f64, b: f64) -> f64 {
    money / (a * b).sqrt()
}

pub fn ck_target_at(ck_bps: u16, p: f64, s: f64) -> f64 {
    ck_bps as f64 / 1e4 * p * s
}

#[derive(Clone, Copy)]
struct St {
    s: f64,
    f_money: f64,
    f: f64,
    p: f64,
    ck: f64,
    m: f64,
}

pub fn build(row: &Row) -> Result<Ask, V2Error> {
    build_with(row, false)
}

pub fn build_keeping_targets(row: &Row) -> Result<Ask, V2Error> {
    build_with(row, true)
}

fn build_with(row: &Row, target_first: bool) -> Result<Ask, V2Error> {
    row.validate()?;
    let tf = if target_first { row.ck_bps } else { 0 };
    let phi = row.phi_bps as f64 / 1e4;
    let theta = row.theta_bps as f64 / 1e4;
    let es = 1.0 / phi - 1.0;
    let eps = es / row.depth();
    let cap = row.launch_cap as f64;
    let raise = row.bonding_raise as f64;
    let mut st = St { s: raise / cap, f_money: raise, f: cap, p: cap, ck: 0.0, m: 0.0 };
    let mut ask = Ask { contiguous: true, ..Default::default() };
    ask.bonding_tokens = st.s;
    ask.segs.push(Seg { kind: Kind::Bonding, stack: 255, tokens: st.s, a: cap, b: cap, money: raise, to_floor: raise, to_ck: 0.0, to_ms: 0.0 });

    let climb = |st: &mut St, ask: &mut Ask, lvl: f64, stack: u8, kind: Kind| {
        if lvl <= st.p {
            return;
        }
        let (p0, k) = (st.p, pieces(st.p, lvl, row.climb_step_x100 as f64 / 100.0));
        for j in 1..=k {
            let to = if j == k { lvl } else { p0 * (lvl / p0).powf(j as f64 / k as f64) };
            let se = st.s * (to / st.p).powf(1.0 / eps);
            let q = (se - st.s) * (st.p * to).sqrt();
            let ft = st.f.max(phi * to);
            let df = q.min((ft * se - st.f_money).max(0.0));
            push(st, ask, kind, stack, se - st.s, st.p, to, q, df, q - df, 0.0, phi);
        }
    };

    let n = row.stacks.len();
    let mut running = 0.0;
    for (i, sr) in row.stacks.iter().enumerate() {
        let lvl = sr.level as f64;
        if lvl < st.p {
            ask.stacks.push(StackReport { level: lvl, top: lvl, supply_after: st.s, milestone: 0.0, milestone_running: running, money_in: 0.0, to_floor: 0.0, skipped: true, short: sr.milestone as f64 });
            continue;
        }
        let money0 = ask.segs.iter().map(|g| g.money).sum::<f64>();
        let floor0 = st.f_money;
        climb(&mut st, &mut ask, lvl, i as u8, Kind::Climb);
        let spread = sr.spread_bps.unwrap_or(row.spread_bps) as f64 / 1e4;
        let top = if i + 1 < n {
            let next = row.stacks[i + 1].level as f64;
            lvl * (next / lvl).powf(spread)
        } else {
            lvl * 2f64.powf(spread)
        };
        let (a, b) = (st.p, top);
        let tgt = sr.milestone as f64;
        let k = pieces(a, b, SALE_STEP);
        let qbar = (1..=k).map(|j| a * (b / a).powf((2 * j - 1) as f64 / (2 * k) as f64)).sum::<f64>() / k as f64;
        let ft = st.f.max(phi * b);
        let den = qbar - ft;
        let need = ft * st.s - st.f_money;
        let xt = if tf > 0 {
            0.0
        } else if tgt > 0.0 {
            if theta >= 1.0 || den <= 0.0 {
                return Err(V2Error::TargetUnreachable(i as u8));
            }
            (tgt / (1.0 - theta) + need) / den
        } else if need <= 0.0 {
            0.0
        } else if den > 0.0 {
            need / den
        } else {
            return Err(V2Error::TargetUnreachable(i as u8));
        };
        let xmin = st.s * ((b / a).powf(1.0 / es) - 1.0);
        let mut x = xt.max(xmin).max(0.0);
        let band = |st: &mut St, ask: &mut Ask, x: f64| -> f64 {
            let mut m = 0.0;
            for j in 1..=k {
                let (pa, pb) = (st.p, if j == k { b } else { a * (b / a).powf(j as f64 / k as f64) });
                let xj = x / k as f64;
                let qj = xj * (pa * pb).sqrt();
                let ftj = st.f.max(phi * pb);
                let dfj = qj.min((ftj * (st.s + xj) - st.f_money).max(0.0));
                let rj = qj - dfj;
                let mj = if tf > 0 {
                    let ck = rj.min((ck_target_at(tf, pb, st.s + xj) - st.ck).max(0.0));
                    (tgt - m).max(0.0).min(rj - ck)
                } else {
                    (tgt - m).max(0.0).min((1.0 - theta) * rj)
                };
                m += mj;
                push(st, ask, Kind::Sale, i as u8, xj, pa, pb, qj, dfj, rj - mj, mj, phi);
            }
            m
        };
        if tf > 0 && tgt > 0.0 {
            let reach = |x: f64| band(&mut st.clone(), &mut Ask::default(), x) >= tgt * (1.0 - 1e-12);
            if !reach(x) {
                if den <= 0.0 {
                    return Err(V2Error::TargetUnreachable(i as u8));
                }
                let mut hi = x.max(tgt / den);
                let mut n_up = 0;
                while !reach(hi) {
                    hi *= 2.0;
                    n_up += 1;
                    if n_up > 200 {
                        return Err(V2Error::TargetUnreachable(i as u8));
                    }
                }
                let mut lo = x;
                for _ in 0..200 {
                    let mid = 0.5 * (lo + hi);
                    if reach(mid) {
                        hi = mid
                    } else {
                        lo = mid
                    }
                }
                x = hi;
            }
        }
        let m = band(&mut st, &mut ask, x);
        running += m;
        let money1 = ask.segs.iter().map(|g| g.money).sum::<f64>();
        ask.stacks.push(StackReport { level: lvl, top, supply_after: st.s, milestone: m, milestone_running: running, money_in: money1 - money0, to_floor: st.f_money - floor0, skipped: false, short: (tgt - m).max(0.0) });
    }
    ask.milestones_stacks = running;
    ask.supply_after_stacks = st.s;
    let tail = row.tail_bps as f64 / 1e4;
    let u = 1.0 - tail - st.s;
    ask.supply_excess = (-u).max(0.0);
    let t0 = st.p;
    ask.tail_start = t0;
    let u = u.max(0.0);
    ask.tail_tokens = tail;
    if u > 0.0 {
        let q = u * t0;
        let ft = st.f.max(phi * t0);
        let df = q.min((ft * (st.s + u) - st.f_money).max(0.0));
        let ck = if tf > 0 { (q - df).min((ck_target_at(tf, t0, st.s + u) - st.ck).max(0.0)) } else { 0.0 };
        ask.unused_share = u;
        ask.unused_floor = df;
        ask.unused_milestone = q - df - ck;
        push(&mut st, &mut ask, Kind::Unused, 255, u, t0, t0, q, df, ck, q - df - ck, phi);
        st.m -= 0.0;
    }
    ask.ck_at_tail = st.ck;
    ask.floor_at_tail = st.f_money;
    ask.f_end = st.f;
    ask.segs.push(Seg { kind: Kind::Tail, stack: 255, tokens: ask.tail_tokens, a: t0, b: f64::INFINITY, money: 0.0, to_floor: 0.0, to_ck: 0.0, to_ms: 0.0 });
    let mut prev_b = cap;
    for g in &ask.segs[1..] {
        if (g.a - prev_b).abs() > 1e-9 * prev_b.max(1.0) {
            ask.contiguous = false;
        }
        prev_b = g.b;
    }
    Ok(ask)
}

#[allow(clippy::too_many_arguments)]
fn push(st: &mut St, ask: &mut Ask, kind: Kind, stack: u8, x: f64, a: f64, b: f64, q: f64, df: f64, to_ck: f64, to_ms: f64, phi: f64) {
    st.s += x;
    st.f_money += df;
    st.f = st.f.max(st.f_money / st.s);
    st.ck += to_ck;
    st.m += to_ms;
    st.p = b;
    if ask.floor_break.is_none() && st.f_money / st.s < phi * b * (1.0 - 1e-9) && st.f < phi * b * (1.0 - 1e-9) {
        ask.floor_break = Some(ask.segs.len());
    }
    ask.segs.push(Seg { kind, stack, tokens: x, a, b, money: q, to_floor: df, to_ck, to_ms });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::rows;

    fn near(got: f64, want: f64, tol: f64, what: &str) {
        let rel = if want == 0.0 { got.abs() } else { ((got - want) / want).abs() };
        assert!(rel <= tol, "{what}: got {got}, want {want} (rel {rel})");
    }

    #[test]
    fn every_piece_is_priced_as_the_cp_book() {
        for d in [rows::wagmi(), rows::moonshot(), rows::floor_genesis(), rows::wagmi_genesis(1_000_000)] {
            for g in build(&d).unwrap().segs.iter().filter(|g| matches!(g.kind, Kind::Climb | Kind::Sale) && g.b > g.a) {
                near(g.money, g.tokens * (g.a * g.b).sqrt(), 1e-12, d.name);
            }
        }
    }

    #[test]
    fn floor_genesis_matches_the_spec_stack_table() {
        let a = build(&rows::floor_genesis_phi_study(1_000)).unwrap();
        let want_supply = [0.076, 0.102, 0.122, 0.157, 0.180, 0.244];
        let want_run = [10e3, 60e3, 160e3, 660e3, 1.66e6, 11.7e6];
        let want_paid = [51e3, 97e3, 170e3, 1.16e6, 2.02e6, 32.6e6];
        let want_floor = [41e3, 21e3, 40e3, 661e3, 1.02e6, 22.6e6];
        let mut cum_floor = 0.0;
        for (i, s) in a.stacks.iter().enumerate() {
            near(s.supply_after, want_supply[i], 0.006, "supply after");
            near(s.milestone_running, want_run[i], 0.004, "running milestone");
            near(s.money_in, want_paid[i], 0.012, "money paid");
            cum_floor += s.to_floor;
            let _ = (cum_floor, want_floor[i]);
        }
        near(a.unused_share, 0.356, 0.005, "unused");
        near(a.unused_milestone, 320.6e6, 0.005, "unused milestone");
        near(a.milestones_total(), 332.2e6, 0.005, "team total");
        near(a.tail_start, 1e9, 1e-9, "tail start");
    }
}
