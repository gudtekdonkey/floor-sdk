// SPDX-License-Identifier: Apache-2.0
use super::book::{Range, KIND_TAIL, MAX_WALK_STEPS};
use super::sqrt::{flat_money_for_tokens, sqrt_of_ratio, sqrt_scale_down_bps, Sqrt};
use super::wide::{mul_div, Round};
use crate::MathError;

pub const MS_TEAM: u8 = 1;
pub const MS_HOLDER: u8 = 2;
pub const LEG_TEAM: usize = 0;
pub const LEG_HOLDER: usize = 1;
pub const LEG_PLATFORM: usize = 2;
pub const LEG_FLOOR: usize = 3;
pub const THETA_NONE: u16 = 10_000;
pub const PIECES_MAX: usize = MAX_WALK_STEPS as usize * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitRow {
    pub phi_bps: u16,
    pub ck_bps: u16,
    pub wc_bps: u16,
    pub c_share_bps: u16,
    pub ms_class: u8,
    pub platform_ms_bps: u16,
    pub floor_ms_bps: u16,
}

impl SplitRow {
    pub fn new(phi_bps: u16, ck_bps: u16, ms_class: u8) -> SplitRow {
        SplitRow { phi_bps, ck_bps, wc_bps: 300, c_share_bps: 5_000, ms_class, platform_ms_bps: 1_000, floor_ms_bps: 1_000 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Piece {
    pub sqrt_lo: Sqrt,
    pub sqrt_hi: Sqrt,
    pub money: u128,
    pub tokens: u64,
    pub range: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplitIn {
    pub sqrt_f: Sqrt,
    pub sqrt_p1: Sqrt,
    pub f_total: u128,
    pub f_claims: u128,
    pub ck0: u128,
    pub held: u64,
    pub s_new: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Split {
    pub d_f: u128,
    pub ck_add: u128,
    pub ck_take: u128,
    pub ms: u128,
    pub booked: [u128; 4],
    pub sqrt_f_new: Sqrt,
}

fn ov<T>(o: Option<T>) -> Result<T, MathError> {
    o.ok_or(MathError::Overflow)
}

pub fn f_cap(m: u128, psi_bps: u16, sqrt_f: Sqrt, sqrt_lo: Sqrt) -> Result<u128, MathError> {
    if sqrt_f >= sqrt_lo {
        return Ok(m);
    }
    let by_psi = ov(mul_div(m, psi_bps as u128, 10_000, Round::Up))?;
    let t = ov(mul_div(m, sqrt_f, sqrt_lo, Round::Up))?;
    let by_f = ov(mul_div(t, sqrt_f, sqrt_lo, Round::Up))?;
    Ok(by_psi.max(by_f).min(m))
}

pub fn f_need(row: &SplitRow, sqrt_f: Sqrt, sqrt_hi: Sqrt, n: u128) -> Result<u128, MathError> {
    let at_f = flat_money_for_tokens(sqrt_f, n, Round::Up).unwrap_or(u128::MAX);
    let at_phi = flat_money_for_tokens(sqrt_hi, n, Round::Up)
        .and_then(|v| mul_div(v, row.phi_bps as u128, 10_000, Round::Up))
        .unwrap_or(u128::MAX);
    Ok(at_f.max(at_phi))
}

pub fn ck_target(row: &SplitRow, sqrt_p1: Sqrt, s_new: u64) -> Result<u128, MathError> {
    let Some(v) = flat_money_for_tokens(sqrt_p1, s_new as u128, Round::Up) else {
        return Ok(u128::MAX);
    };
    Ok(mul_div(v, row.ck_bps as u128, 10_000, Round::Up).unwrap_or(u128::MAX))
}

pub fn floor_after(row: &SplitRow, sqrt_f: Sqrt, f_back: u128, tokens: u128, sqrt_p1: Sqrt) -> Result<Sqrt, MathError> {
    if tokens == 0 {
        return Ok(sqrt_f);
    }
    let r = ov(sqrt_of_ratio(f_back, tokens))?;
    let cap = ov(sqrt_scale_down_bps(sqrt_p1, row.wc_bps))?;
    Ok(sqrt_f.max(r.min(cap)))
}

pub fn split(row: &SplitRow, ranges: &[Range], inp: &SplitIn, pieces: &[Piece]) -> Result<Split, MathError> {
    split_checked(row, ranges, inp, pieces).map(|(s, _)| s)
}

pub fn split_checked(row: &SplitRow, ranges: &[Range], inp: &SplitIn, pieces: &[Piece]) -> Result<(Split, bool), MathError> {
    let n_all = pieces.iter().try_fold(0u64, |a, p| a.checked_add(p.tokens)).ok_or(MathError::Overflow)?;
    let mut sr = ov(inp.s_new.checked_sub(n_all))? as u128;
    let mut fr = inp.f_total;
    let (mut ck_add, mut ms, mut order_ok) = (0u128, 0u128, true);
    let mut top_need = 0u128;
    for p in pieces {
        let r = ov(ranges.get(p.range as usize))?;
        let m = p.money;
        sr += p.tokens as u128;
        let cap = f_cap(m, r.psi_bps, inp.sqrt_f, p.sqrt_lo)?;
        let need_all = f_need(row, inp.sqrt_f, p.sqrt_hi, sr + inp.held as u128)?;
        if r.kind != KIND_TAIL {
            top_need = need_all;
        }
        let need = need_all.saturating_sub(ov(fr.checked_add(inp.f_claims))?);
        let d_f = cap.min(need);
        fr = ov(fr.checked_add(d_f))?;
        let rem = m - d_f;
        if r.theta_bps >= THETA_NONE {
            ck_add = ov(ck_add.checked_add(rem))?;
            continue;
        }
        let gap = ck_target(row, p.sqrt_hi, sr as u64)?.saturating_sub(inp.ck0.saturating_add(ck_add));
        let mut ck = rem.min(gap);
        let mut mm = rem - ck;
        let mut capped = false;
        if r.ms_cap_ppm > 0 {
            let capm = ov(mul_div(m, r.ms_cap_ppm as u128, 1_000_000, Round::Down))?;
            if mm > capm {
                ck += mm - capm;
                mm = capm;
                capped = true;
            }
        }
        let gap_after = gap.saturating_sub(ck);
        if mm > 0 && !capped && (gap_after != 0 || need != d_f) {
            order_ok = false;
        }
        ck_add = ov(ck_add.checked_add(ck))?;
        ms = ov(ms.checked_add(mm))?;
    }
    let mut ck_take = 0u128;
    let short = top_need.saturating_sub(ov(fr.checked_add(inp.f_claims))?);
    if short > 0 && (inp.ck0 > 0 || ck_add > 0) {
        let a = short.min(ck_add);
        let m2 = (short - a).min(ms);
        ck_take = (short - a - m2).min(inp.ck0);
        ck_add -= a;
        ms -= m2;
        fr = ov(fr.checked_add(a + m2 + ck_take))?;
    }
    let back = ov(fr.checked_add(inp.f_claims))?;
    let sqrt_f_new = floor_after(row, inp.sqrt_f, back, inp.s_new as u128 + inp.held as u128, inp.sqrt_p1)?;
    let mut booked = [0u128; 4];
    booked[LEG_PLATFORM] = ov(mul_div(ms, row.platform_ms_bps as u128, 10_000, Round::Down))?;
    booked[LEG_FLOOR] = ov(mul_div(ms, row.floor_ms_bps as u128, 10_000, Round::Down))?;
    let class = ov(ms.checked_sub(booked[LEG_PLATFORM] + booked[LEG_FLOOR]))?;
    booked[if row.ms_class == MS_TEAM { LEG_TEAM } else { LEG_HOLDER }] = class;
    Ok((Split { d_f: fr - inp.f_total, ck_add, ck_take, ms, booked, sqrt_f_new }, order_ok))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::sqrt::{liquidity_for_tokens, money_between, tokens_between, ONE};

    const E6: u64 = 1_000_000;

    fn sq(p: f64) -> Sqrt {
        (p.sqrt() * ONE as f64) as Sqrt
    }
    fn fs(s: Sqrt) -> f64 {
        s as f64 / ONE as f64
    }
    fn rng(psi: u16, theta: u16, cap: u32) -> Range {
        Range { psi_bps: psi, theta_bps: theta, ms_cap_ppm: cap, kind: 2, ..Range::default() }
    }
    fn ladder(lo: f64, step: f64, n: usize, tok: u64, r: Range) -> (Vec<Range>, Vec<Piece>) {
        let mut rs = Vec::new();
        let mut ps = Vec::new();
        for i in 0..n {
            let (a, b) = (sq(lo * step.powi(i as i32)), sq(lo * step.powi(i as i32 + 1)));
            let l = liquidity_for_tokens(a, b, tok as u128).unwrap();
            rs.push(Range { sqrt_a: a, sqrt_b: b, tokens: tok, ..r });
            ps.push(Piece {
                sqrt_lo: a,
                sqrt_hi: b,
                money: money_between(l, a, b, Round::Up).unwrap(),
                tokens: tokens_between(l, a, b, Round::Down).unwrap() as u64,
                range: i as u8,
            });
        }
        (rs, ps)
    }
    fn inp(f: f64, f_total: u128, ck0: u128, held: u64, circ0: u64, ps: &[Piece]) -> SplitIn {
        let n: u64 = ps.iter().map(|p| p.tokens).sum();
        SplitIn { sqrt_f: sq(f), sqrt_p1: ps.last().map(|p| p.sqrt_hi).unwrap_or(sq(1.0)), f_total, f_claims: 0, ck0, held, s_new: circ0 + n }
    }

    fn model(row: &SplitRow, rs: &[Range], st: &SplitIn, ps: &[Piece]) -> (f64, f64, f64, f64) {
        let f = fs(st.sqrt_f).powi(2);
        let phi = row.phi_bps as f64 / 1e4;
        let p1 = fs(st.sqrt_p1).powi(2);
        let (s_new, xh, fc) = (st.s_new as f64, st.held as f64, st.f_claims as f64);
        let mut sr = s_new - ps.iter().map(|p| p.tokens as f64).sum::<f64>();
        let mut fr = st.f_total as f64;
        let (mut ck_add, mut ms, mut top) = (0.0, 0.0, 0.0f64);
        for p in ps {
            let r = &rs[p.range as usize];
            let (lo, hi, m) = (fs(p.sqrt_lo), fs(p.sqrt_hi), p.money as f64);
            sr += p.tokens as f64;
            let psi = (r.psi_bps as f64 / 1e4).max(f / (lo * lo)).min(1.0);
            let need_all = f.max(phi * hi * hi) * (sr + xh);
            if r.kind != KIND_TAIL {
                top = need_all;
            }
            let need = need_all - (fr + fc);
            let d_f = (psi * m).min(need.max(0.0));
            fr += d_f;
            let rem = m - d_f;
            if r.theta_bps >= THETA_NONE {
                ck_add += rem;
                continue;
            }
            let gap = (row.ck_bps as f64 / 1e4 * hi * hi * sr - (st.ck0 as f64 + ck_add)).max(0.0);
            let mut ck = rem.min(gap);
            let mut mm = rem - ck;
            let cp = r.ms_cap_ppm as f64 / 1e6;
            if cp > 0.0 && mm > cp * m {
                ck += mm - cp * m;
                mm = cp * m;
            }
            ck_add += ck;
            ms += mm;
        }
        let short = top - (fr + fc);
        if short > 0.0 && st.ck0 as f64 + ck_add > 0.0 {
            let a = short.min(ck_add);
            let m2 = (short - a).min(ms);
            let take = (short - a - m2).min(st.ck0 as f64);
            ck_add -= a;
            ms -= m2;
            fr += a + m2 + take;
        }
        let fnew = f.max(((fr + fc) / (s_new + xh)).min(p1 * (1.0 - row.wc_bps as f64 / 1e4)));
        (fr - st.f_total as f64, ck_add, ms, fnew)
    }

    fn close(a: f64, b: f64, tol: f64, units: f64) -> bool {
        (a - b).abs() <= tol * a.abs().max(b.abs()) + units
    }

    fn check(row: &SplitRow, rs: &[Range], st: &SplitIn, ps: &[Piece]) -> Split {
        let (s, order_ok) = split_checked(row, rs, st, ps).unwrap();
        let money: u128 = ps.iter().map(|p| p.money).sum();
        assert_eq!(s.d_f + s.ck_add + s.ms, money + s.ck_take, "conservation");
        assert!(s.ck_take <= st.ck0, "the top-up takes at most C + K's pot");
        assert_eq!(s.booked.iter().sum::<u128>(), s.ms);
        assert!(s.sqrt_f_new >= st.sqrt_f, "f fell");
        if s.sqrt_f_new > st.sqrt_f {
            let need = flat_money_for_tokens(s.sqrt_f_new, st.s_new as u128 + st.held as u128, Round::Up).unwrap();
            assert!(st.f_total + s.d_f + st.f_claims + 1 >= need, "unbacked");
        }
        let _ = order_ok;
        let (df, dck, ms, fnew) = model(row, rs, st, ps);
        let u = 4.0 * ps.len() as f64;
        assert!(close(s.d_f as f64, df, 1e-9, u), "d_f {} vs {}", s.d_f, df);
        assert!(close(s.ck_add as f64, dck, 1e-9, u), "ck_add {} vs {}", s.ck_add, dck);
        assert!(close(s.ms as f64, ms, 1e-9, u), "ms {} vs {}", s.ms, ms);
        assert!(close(fs(s.sqrt_f_new).powi(2), fnew, 1e-9, 0.0), "f {} vs {}", fs(s.sqrt_f_new).powi(2), fnew);
        s
    }

    fn funded(phi: u16, ck_bps: u16) -> (SplitRow, u64, u128, u128) {
        let row = SplitRow::new(phi, ck_bps, MS_TEAM);
        let circ0 = 100_000_000 * E6;
        let ftot = f_need(&row, 0, sq(1.0), circ0 as u128).unwrap();
        (row, circ0, ftot, ck_target(&row, sq(1.0), circ0).unwrap())
    }

    #[test]
    fn targets_past_u128_saturate() {
        let row = SplitRow::new(5_000, 700, MS_TEAM);
        let top = u128::MAX >> 1;
        assert_eq!(ck_target(&row, top, u64::MAX).unwrap(), u128::MAX);
        assert_eq!(f_need(&row, top, top, u64::MAX as u128).unwrap(), u128::MAX);
        assert_eq!(f_need(&row, 0, top, u64::MAX as u128).unwrap(), u128::MAX);
    }

    #[test]
    fn empty_buy_splits_nothing() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let st = SplitIn { sqrt_f: sq(0.5), sqrt_p1: sq(1.0), f_total: ftot, f_claims: 0, ck0: ck, held: 0, s_new: circ0 };
        let s = split(&row, &[], &st, &[]).unwrap();
        assert_eq!((s.d_f, s.ck_add, s.ms), (0, 0, 0));
        assert!(s.sqrt_f_new >= st.sqrt_f);
    }

    #[test]
    fn funded_sale_pays_f_then_ck_then_milestones() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 6, 10_000_000 * E6, rng(9_000, 0, 0));
        let st = inp(0.49, ftot, ck * 11 / 10, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert!(s.d_f > 0 && s.ck_add > 0 && s.ms > 0, "{s:?}");
        assert_eq!(st.ck0 + s.ck_add, ck_target(&row, st.sqrt_p1, st.s_new).unwrap(), "C + K exactly at target");
        assert_eq!(s.booked[LEG_HOLDER], 0);
        assert_eq!(s.booked[LEG_PLATFORM], s.ms / 10);
        assert_eq!(s.booked[LEG_FLOOR], s.ms / 10);
    }

    #[test]
    fn short_ck_takes_everything_before_milestones() {
        let (row, circ0, ftot, _) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.01, 3, 50_000 * E6, rng(5_000, 0, 0));
        let st = inp(0.49, ftot, 0, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert_eq!(s.ms, 0);
        assert!(s.ck_add < ck_target(&row, st.sqrt_p1, st.s_new).unwrap());
    }

    fn top_need(row: &SplitRow, rs: &[Range], st: &SplitIn, ps: &[Piece]) -> u128 {
        let p = ps.iter().rev().find(|p| rs[p.range as usize].kind != KIND_TAIL).unwrap();
        let n: u64 = ps.iter().take_while(|q| q.sqrt_lo <= p.sqrt_lo).map(|q| q.tokens).sum();
        let sr = st.s_new - ps.iter().map(|q| q.tokens).sum::<u64>() + n;
        f_need(row, st.sqrt_f, p.sqrt_hi, sr as u128 + st.held as u128).unwrap()
    }

    #[test]
    fn short_f_takes_its_cap_then_the_top_up() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 4, 200_000 * E6, rng(5_000, 0, 0));
        let st = inp(0.49, ftot / 2, ck * 40, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        let money: u128 = ps.iter().map(|p| p.money).sum();
        assert_eq!((s.ck_add, s.ms), (0, 0), "the buy's C + K part and Milestones went to F first");
        assert_eq!(s.d_f, money + s.ck_take);
        assert!(s.ck_take > 0 && s.ck_take < st.ck0);
        assert_eq!(st.f_total + s.d_f, top_need(&row, &rs, &st, &ps), "F exactly at its need");
    }

    #[test]
    fn top_up_takes_ck_part_then_milestones_then_the_pot() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 6, 10_000_000 * E6, rng(5_000, 0, 0));
        let st = inp(0.49, ftot, 0, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert!(s.ck_add > 0 && s.ms > 0 && s.ck_take == 0, "{s:?}");
        assert_eq!(st.f_total + s.d_f, top_need(&row, &rs, &st, &ps));
        let st = inp(0.49, ftot, ck, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert!(s.ck_add == 0 && s.ms > 0 && s.ck_take == 0, "{s:?}");
        assert_eq!(st.f_total + s.d_f, top_need(&row, &rs, &st, &ps));
        let (rs, ps) = ladder(1.0, 1.02, 6, 1_000_000 * E6, rng(5_000, 0, 0));
        let st = inp(0.49, ftot, ck, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert!(s.ck_add == 0 && s.ms == 0 && s.ck_take > 0, "{s:?}");
        assert_eq!(st.f_total + s.d_f, top_need(&row, &rs, &st, &ps));
    }

    #[test]
    fn top_up_none_when_ck_holds_none() {
        let (row, circ0, ftot, _) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 4, 200_000 * E6, rng(5_000, 0, 0));
        let st = inp(0.49, ftot / 2, 0, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        let cap: u128 = ps.iter().map(|p| (p.money * 5_000).div_ceil(10_000)).sum();
        assert_eq!(s.ck_take, 0);
        assert!(s.d_f > cap && s.ck_add == 0);
        let st = SplitIn { ck0: 0, ..st };
        let none = check(&row, &rs, &st, &[]);
        assert_eq!((none.d_f, none.ck_take), (0, 0));
    }

    #[test]
    fn top_up_skips_the_tail() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let tail = Range { kind: KIND_TAIL, ..rng(5_000, THETA_NONE, 0) };
        let (rs, ps) = ladder(1.0, 1.02, 4, 200_000 * E6, tail);
        let st = inp(0.49, ftot, ck * 40, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert_eq!(s.ck_take, 0, "the Tail is exempt");
        assert!(s.ck_add > 0);
    }

    #[test]
    fn climb_gives_all_its_rest_to_ck() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.2, 3, 50_000_000 * E6, rng(9_000, THETA_NONE, 0));
        let st = inp(0.49, ftot, ck * 4, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert_eq!(s.ms, 0);
        assert!(s.ck_add > 0);
    }

    #[test]
    fn q59_cap_sends_the_excess_to_ck() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 5, 10_000_000 * E6, rng(9_000, 0, 50_000));
        let st = inp(0.49, ftot, ck * 4, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        let cap: u128 = ps.iter().map(|p| p.money * 50_000 / 1_000_000).sum();
        assert!(s.ms > 0 && s.ms <= cap);
        assert!(s.ck_add > 0, "the cap's excess joins C + K although C + K is past its target");
    }

    #[test]
    fn the_row_class_takes_the_rest() {
        let (mut row, circ0, ftot, ck) = funded(4_000, 500);
        row.ms_class = MS_HOLDER;
        let (rs, ps) = ladder(1.0, 1.02, 4, 10_000_000 * E6, rng(9_000, 0, 0));
        let st = inp(0.39, ftot, ck * 4, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert!(s.booked[LEG_HOLDER] > 0 && s.booked[LEG_TEAM] == 0);
        assert_eq!(s.booked[LEG_HOLDER], s.ms - s.booked[LEG_PLATFORM] - s.booked[LEG_FLOOR]);
    }

    #[test]
    fn held_tokens_and_claims_count_in_f_need() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 4, 3_000_000 * E6, rng(10_000, 0, 0));
        let mut st = inp(0.49, ftot, ck, 0, circ0, &ps);
        let base = check(&row, &rs, &st, &ps);
        st.held = 500_000 * E6;
        let h = check(&row, &rs, &st, &ps);
        assert!(h.d_f > base.d_f, "F reserves for the tokens C + K holds");
        st.f_claims = flat_money_for_tokens(sq(0.49), st.held as u128, Round::Up).unwrap();
        let c = check(&row, &rs, &st, &ps);
        assert!(c.d_f < h.d_f, "F's claims count as its backing");
    }

    #[test]
    fn floor_never_falls_and_is_clamped_under_the_price() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let (rs, ps) = ladder(1.0, 1.02, 2, 100_000 * E6, rng(10_000, 0, 0));
        let st = inp(0.49, ftot * 4, ck, 0, circ0, &ps);
        let s = check(&row, &rs, &st, &ps);
        assert_eq!(s.sqrt_f_new, sqrt_scale_down_bps(st.sqrt_p1, row.wc_bps).unwrap());
        let st2 = inp(0.9, ftot / 4, ck, 0, circ0, &ps);
        assert_eq!(split(&row, &rs, &st2, &ps).unwrap().sqrt_f_new, st2.sqrt_f);
    }

    #[test]
    fn f_cap_lifts_to_f_over_lo_squared() {
        let m = 1_000_000 * E6 as u128;
        assert_eq!(f_cap(m, 1_000, sq(1.0), sq(1.0)).unwrap(), m);
        let b = f_cap(m, 1_000, sq(0.25), sq(1.0)).unwrap();
        assert!(b >= m / 4 && b <= m / 4 + 2, "{b}");
        assert_eq!(f_cap(m, 5_000, sq(0.25), sq(1.0)).unwrap(), m / 2);
        assert_eq!(f_cap(m + 1, 5_000, 0, sq(1.0)).unwrap(), m / 2 + 1, "⌈ψ·m⌉ rounds toward F");
    }

    #[test]
    fn flat_piece_and_bad_input() {
        let (row, circ0, ftot, ck) = funded(5_000, 700);
        let v = sq(1.0);
        let tok = 1_000_000 * E6;
        let rs = [Range { sqrt_a: v, sqrt_b: v, tokens: tok, ..rng(5_000, 0, 0) }];
        let p = Piece { sqrt_lo: v, sqrt_hi: v, money: flat_money_for_tokens(v, tok as u128, Round::Up).unwrap(), tokens: tok, range: 0 };
        let st = inp(0.49, ftot, ck, 0, circ0, &[p]);
        check(&row, &rs, &st, &[p]);
        assert_eq!(split(&row, &rs, &st, &[Piece { range: 1, ..p }]), Err(MathError::Overflow), "a piece naming no range");
        assert_eq!(split(&row, &rs, &SplitIn { s_new: 1, ..st }, &[p]), Err(MathError::Overflow), "pieces over s_new");
    }

    #[test]
    fn random_states_match_the_model() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = || {
            x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            (z ^ (z >> 31)) as f64 / u64::MAX as f64
        };
        for _ in 0..300 {
            let phi = if rnd() < 0.5 { 4_000u16 } else { 5_000 };
            let ckb = if rnd() < 0.5 { 500u16 } else { 700 };
            let (row, circ0, ftot, ck) = funded(phi, ckb);
            let n = 1 + (rnd() * 6.0) as usize;
            let theta = if rnd() < 0.2 { THETA_NONE } else { 0 };
            let cap = if rnd() < 0.3 { (rnd() * 200_000.0) as u32 } else { 0 };
            let (rs, ps) = ladder(1.0, 1.005 + rnd() * 0.1, n, (1.0 + rnd() * 500_000.0) as u64 * E6, rng(phi, theta, cap));
            let mut st = inp(0.3 + rnd() * 0.2, (ftot as f64 * (0.5 + rnd())) as u128, (ck as f64 * rnd() * 2.0) as u128, 0, circ0, &ps);
            if rnd() < 0.5 {
                st.held = (rnd() * 3e6) as u64 * E6;
            }
            if rnd() < 0.3 {
                st.f_claims = flat_money_for_tokens(st.sqrt_f, st.held as u128, Round::Up).unwrap();
            }
            let s = check(&row, &rs, &st, &ps);
            if s.ms > 0 && cap == 0 && theta == 0 {
                let sr0 = st.s_new - ps.iter().skip(1).map(|p| p.tokens).sum::<u64>();
                assert!(st.ck0 + s.ck_add >= ck_target(&row, ps[0].sqrt_hi, sr0).unwrap(), "{s:?}");
            }
        }
    }

    #[derive(Debug, Clone)]
    enum J {
        N(String),
        S(String),
        B(bool),
        Null,
        A(Vec<J>),
        O(Vec<(String, J)>),
    }
    impl J {
        fn get(&self, k: &str) -> &J {
            match self {
                J::O(v) => &v.iter().find(|(a, _)| a == k).unwrap_or_else(|| panic!("no key {k}")).1,
                _ => panic!("not an object at {k}"),
            }
        }
        fn u(&self) -> u128 {
            match self {
                J::N(s) => s.parse().unwrap_or_else(|_| panic!("not a u128: {s}")),
                _ => panic!("not a number"),
            }
        }
        fn b(&self) -> bool {
            matches!(self, J::B(true))
        }
        fn a(&self) -> &[J] {
            match self {
                J::A(v) => v,
                _ => panic!("not an array"),
            }
        }
    }
    fn parse(s: &[u8], i: &mut usize) -> J {
        while s[*i].is_ascii_whitespace() {
            *i += 1;
        }
        match s[*i] {
            b'{' => {
                *i += 1;
                let mut v = Vec::new();
                loop {
                    while s[*i].is_ascii_whitespace() || s[*i] == b',' {
                        *i += 1;
                    }
                    if s[*i] == b'}' {
                        *i += 1;
                        return J::O(v);
                    }
                    let J::S(k) = parse(s, i) else { panic!("key") };
                    while s[*i] != b':' {
                        *i += 1;
                    }
                    *i += 1;
                    v.push((k, parse(s, i)));
                }
            }
            b'[' => {
                *i += 1;
                let mut v = Vec::new();
                loop {
                    while s[*i].is_ascii_whitespace() || s[*i] == b',' {
                        *i += 1;
                    }
                    if s[*i] == b']' {
                        *i += 1;
                        return J::A(v);
                    }
                    v.push(parse(s, i));
                }
            }
            b'"' => {
                let st = *i + 1;
                *i = st;
                while s[*i] != b'"' {
                    *i += if s[*i] == b'\\' { 2 } else { 1 };
                }
                *i += 1;
                J::S(String::from_utf8_lossy(&s[st..*i - 1]).into_owned())
            }
            b't' => {
                *i += 4;
                J::B(true)
            }
            b'f' => {
                *i += 5;
                J::B(false)
            }
            b'n' => {
                *i += 4;
                J::Null
            }
            _ => {
                let st = *i;
                while *i < s.len() && (s[*i].is_ascii_digit() || b"-+.eE".contains(&s[*i])) {
                    *i += 1;
                }
                J::N(String::from_utf8_lossy(&s[st..*i]).into_owned())
            }
        }
    }

    #[test]
    fn r1_split_goldens_bit_exact() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/golden/split");
        if !std::path::Path::new(dir).exists() {
            eprintln!("golden/split/ absent (R1's goldens not merged here): skipped");
            return;
        }
        let raw = std::fs::read(format!("{dir}/split_unit.json")).expect("golden/split/split_unit.json");
        let doc = parse(&raw, &mut 0);
        let cases = doc.get("cases").a();
        assert!(!cases.is_empty());
        for (n, c) in cases.iter().enumerate() {
            let k = c.get("consts");
            let row = SplitRow {
                phi_bps: k.get("phi_bps").u() as u16,
                ck_bps: k.get("ck_bps").u() as u16,
                wc_bps: k.get("wc_bps").u() as u16,
                c_share_bps: k.get("c_share_bps").u() as u16,
                ms_class: k.get("ms_class").u() as u8,
                platform_ms_bps: k.get("platform_ms_bps").u() as u16,
                floor_ms_bps: k.get("floor_ms_bps").u() as u16,
            };
            let rs: Vec<Range> = c
                .get("ranges")
                .a()
                .iter()
                .map(|r| Range {
                    sqrt_a: r.get("a").u(),
                    sqrt_b: r.get("b").u(),
                    tokens: r.get("tokens").u() as u64,
                    psi_bps: r.get("psi_bps").u() as u16,
                    theta_bps: r.get("theta_bps").u() as u16,
                    ms_cap_ppm: r.get("ms_cap_ppm").u() as u32,
                    kind: r.get("kind").u() as u8,
                    ..Range::default()
                })
                .collect();
            let i = c.get("inp");
            let st = SplitIn {
                sqrt_f: i.get("sqrt_f").u(),
                sqrt_p1: i.get("sqrt_p1").u(),
                f_total: i.get("f_total").u(),
                f_claims: i.get("f_claims").u(),
                ck0: i.get("ck0").u(),
                held: i.get("held").u() as u64,
                s_new: i.get("s_new").u() as u64,
            };
            let ps: Vec<Piece> = c
                .get("pieces")
                .a()
                .iter()
                .map(|p| Piece {
                    sqrt_lo: p.get("sqrt_lo").u(),
                    sqrt_hi: p.get("sqrt_hi").u(),
                    money: p.get("money").u(),
                    tokens: p.get("tokens").u() as u64,
                    range: p.get("range").u() as u8,
                })
                .collect();
            let o = c.get("out");
            let (s, ok) = split_checked(&row, &rs, &st, &ps).unwrap_or_else(|e| panic!("case {n}: {e:?}"));
            let booked: Vec<u128> = o.get("booked").a().iter().map(J::u).collect();
            assert_eq!(
                (s.d_f, s.ck_add, s.ck_take, s.ms, s.booked.to_vec(), s.sqrt_f_new, ok),
                (
                    o.get("d_f").u(),
                    o.get("ck_add").u(),
                    o.get("ck_take").u(),
                    o.get("ms").u(),
                    booked,
                    o.get("sqrt_f_new").u(),
                    o.get("order_ok").b()
                ),
                "case {n}"
            );
        }
        eprintln!("split goldens: {} cases bit-exact", cases.len());
    }
}
