// SPDX-License-Identifier: Apache-2.0
use super::book::{Book, Range};
use super::fees::{cheaper, Lg, LgMemo, FEE_LEGS};
use super::sqrt::{flat_money_for_tokens, flat_tokens_for_money, Sqrt};
use super::wide::{div_u256, div_w, mul_w, shr_w, w_u128, Round, W3};
use crate::u256::U256;
use crate::MathError;

const PPM: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub sqrt: Sqrt,
    pub ppm: [u32; FEE_LEGS],
}

pub fn buy_point(b: &Book) -> Option<Point> {
    buy_point_with(b, &mut LgMemo::default())
}

pub fn sell_point(b: &Book) -> Option<Point> {
    sell_point_with(b, &mut LgMemo::default())
}

pub fn buy_point_with<M: Lg + ?Sized>(b: &Book, memo: &mut M) -> Option<Point> {
    let p = b.sqrt_p;
    let s = if sells_at(b, p) { p } else { next_up(b, p)? };
    Some(point(b, s, memo))
}

pub fn sell_point_with<M: Lg + ?Sized>(b: &Book, memo: &mut M) -> Option<Point> {
    let p = b.sqrt_p;
    let s = if buys_at(b, p) { p } else { next_down(b, p)? };
    Some(point(b, s, memo))
}

pub fn buy_rate_x64(b: &Book) -> Result<u128, MathError> {
    buy_rate_x64_with(b, &mut LgMemo::default())
}

pub fn buy_rate_x64_with<M: Lg + ?Sized>(b: &Book, memo: &mut M) -> Result<u128, MathError> {
    let pt = buy_point_with(b, memo).ok_or(MathError::BookEmpty)?;
    let n: W3 = [0, PPM << 80, 0];
    let q = div_w(n, PPM + sum(&pt.ppm), Round::Down).ok_or(MathError::Overflow)?;
    let q = div_u256(q, pt.sqrt, Round::Down).ok_or(MathError::Overflow)?;
    let q = div_u256(q, pt.sqrt, Round::Down).ok_or(MathError::Overflow)?;
    q.to_u128().ok_or(MathError::Overflow)
}

pub fn buy_price_x64(b: &Book) -> Result<u128, MathError> {
    buy_price_x64_with(b, &mut LgMemo::default())
}

pub fn buy_price_x64_with<M: Lg + ?Sized>(b: &Book, memo: &mut M) -> Result<u128, MathError> {
    let pt = buy_point_with(b, memo).ok_or(MathError::BookEmpty)?;
    price_x64(pt.sqrt, PPM + sum(&pt.ppm), Round::Up)
}

pub fn sell_rate_x64(b: &Book) -> Result<u128, MathError> {
    sell_rate_x64_with(b, &mut LgMemo::default())
}

pub fn sell_rate_x64_with<M: Lg + ?Sized>(b: &Book, memo: &mut M) -> Result<u128, MathError> {
    let pt = sell_point_with(b, memo).ok_or(MathError::BookEmpty)?;
    price_x64(pt.sqrt, PPM.saturating_sub(sum(&pt.ppm)), Round::Down)
}

fn price_x64(s: Sqrt, m: u128, r: Round) -> Result<u128, MathError> {
    let w = shr_w(mul_w(U256::sqr_u128(s), m), 80, r);
    let q = div_w(w, PPM, r).ok_or(MathError::Overflow)?;
    w_u128([q.lo, q.hi, 0]).ok_or(MathError::Overflow)
}

fn sum(ppm: &[u32; FEE_LEGS]) -> u128 {
    ppm.iter().map(|x| *x as u128).sum()
}

fn point<M: Lg + ?Sized>(b: &Book, s: Sqrt, memo: &mut M) -> Point {
    let pre = b.fees_at_memo(b.sqrt_p, memo);
    let ppm = if s == b.sqrt_p { pre } else { cheaper(&pre, &b.fees_at_memo(s, memo)) };
    Point { sqrt: s, ppm }
}

fn range_lo(b: &Book, r: &Range) -> Sqrt {
    r.sqrt_a.max(b.sqrt_l_max)
}

fn bands(b: &Book) -> impl Iterator<Item = (Sqrt, Sqrt, usize)> + '_ {
    (0..b.n_bands as usize).map(|j| (b.band_edge[j + 1], b.band_edge[j], j))
}

fn ranges(b: &Book) -> &[Range] {
    &b.ranges[..b.n_ranges as usize]
}

fn sells_at(b: &Book, p: Sqrt) -> bool {
    (b.sqrt_f != 0 && b.sqrt_f == p && b.floor_tokens > 0)
        || ranges(b).iter().any(|r| {
            if r.is_flat() {
                r.sqrt_a == p && r.held() > 0
            } else {
                let lo = range_lo(b, r);
                lo < r.sqrt_b && lo <= p && p < r.sqrt_b && r.held() > 0
            }
        })
        || bands(b).any(|(lo, hi, j)| lo < hi && lo <= p && p < hi && b.band_tokens[j] > 0)
}

pub(crate) fn range_sells_at(b: &Book, p: Sqrt) -> bool {
    ranges(b).iter().any(|r| {
        if r.is_flat() {
            r.sqrt_a == p && r.held() > 0
        } else {
            let lo = range_lo(b, r);
            lo < r.sqrt_b && lo <= p && p < r.sqrt_b && r.held() > 0
        }
    })
}

fn next_up(b: &Book, p: Sqrt) -> Option<Sqrt> {
    let r = ranges(b).iter().filter(|r| r.held() > 0).filter_map(|r| {
        if r.is_flat() {
            Some(r.sqrt_a)
        } else {
            let lo = range_lo(b, r);
            (lo < r.sqrt_b).then_some(lo)
        }
    });
    let k = bands(b).filter(|(lo, hi, j)| lo < hi && b.band_tokens[*j] > 0).map(|(lo, _, _)| lo);
    let f = (b.sqrt_f != 0 && b.floor_tokens > 0).then_some(b.sqrt_f);
    r.chain(k).chain(f).filter(|s| *s > p).min()
}

fn flat_can_buy(s: Sqrt, money: u128) -> bool {
    money > 0
        && match flat_tokens_for_money(s, money, Round::Down) {
            Some(t) if t >= 1 => flat_money_for_tokens(s, t, Round::Down).is_some_and(|q| q > 0),
            _ => false,
        }
}

fn floor_bids(b: &Book) -> bool {
    b.sqrt_f != 0 && flat_can_buy(b.sqrt_f, b.floor)
}

fn flat_range_bids(r: &Range) -> bool {
    r.is_flat() && flat_can_buy(r.sqrt_a, r.place)
}

fn buys_at(b: &Book, p: Sqrt) -> bool {
    ranges(b).iter().any(|r| r.sqrt_a == p && flat_range_bids(r))
        || (b.sqrt_f == p && floor_bids(b))
        || bands(b).any(|(lo, hi, j)| lo < hi && lo < p && p <= hi && b.band_money[j] > 0)
}

fn next_down(b: &Book, p: Sqrt) -> Option<Sqrt> {
    let r = ranges(b).iter().filter(|r| flat_range_bids(r)).map(|r| r.sqrt_a);
    let k = bands(b).filter(|(lo, hi, j)| lo < hi && b.band_money[*j] > 0).map(|(_, hi, _)| hi);
    let f = floor_bids(b).then_some(b.sqrt_f);
    r.chain(k).chain(f).filter(|s| *s < p).max()
}
