// SPDX-License-Identifier: Apache-2.0
use super::fees::{FeeAcc, Lg, FEE_LEGS};
use super::split::Piece;
use super::sqrt::{
    flat_money_for_tokens, flat_tokens_for_money, liquidity_for_money, liquidity_for_tokens, money_between,
    sqrt_after_money, sqrt_after_tokens, tokens_between, Sqrt, SQRT_TAIL_END,
};
use super::wide::{mul_div, Round};
use crate::MathError;

pub const RANGES_MAX: usize = 48;
pub const BIDS_MAX: usize = 24;
pub const MAX_WALK_STEPS: u8 = 24;
pub const SOL_BUY_WALK_STEPS: u8 = 16;

pub const KIND_BONDING: u8 = 0;
pub const KIND_CLIMB: u8 = 1;
pub const KIND_SALE_BAND: u8 = 2;
pub const KIND_RETIRED_3: u8 = 3;
pub const KIND_UNUSED: u8 = 4;
pub const KIND_TAIL: u8 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
    pub sqrt_a: Sqrt,
    pub sqrt_b: Sqrt,
    pub tokens: u64,
    pub ask_left: u64,
    pub back: u64,
    pub place: u128,
    pub theta_bps: u16,
    pub psi_bps: u16,
    pub kind: u8,
    pub class: u8,
    pub ms_cap_ppm: u32,
}

impl Range {
    pub fn is_flat(&self) -> bool {
        self.sqrt_a == self.sqrt_b
    }
    pub fn liquidity(&self) -> Option<u128> {
        liquidity_for_tokens(self.sqrt_a, self.sqrt_b, self.tokens as u128)
    }
    pub fn held(&self) -> u64 {
        self.ask_left + self.back
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Book {
    pub sqrt_p: Sqrt,
    pub sqrt_f: Sqrt,
    pub sqrt_l_max: Sqrt,
    pub floor: u128,
    pub floor_tokens: u64,
    pub n_bands: u8,
    pub n_c: u8,
    pub band_edge: [Sqrt; BIDS_MAX + 1],
    pub band_money: [u128; BIDS_MAX],
    pub band_tokens: [u64; BIDS_MAX],
    pub band_f: [u128; BIDS_MAX],
    pub band_f_claim: [u128; BIDS_MAX],
    pub ck_at_f: u128,
    pub ck_floor_tokens: u64,
    pub fee_ppm: [u32; FEE_LEGS],
    pub fee_start_ppm: [u32; FEE_LEGS],
    pub sqrt_fee_a: Sqrt,
    pub sqrt_fee_b: Sqrt,
    pub graduated: bool,
    pub own_fees: bool,
    pub n_ranges: u8,
    pub ranges: [Range; RANGES_MAX],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Range(usize),
    Band(usize),
    Floor,
}

pub const PIECES_MAX: usize = super::split::PIECES_MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkUp {
    pub money: u128,
    pub tokens: u64,
    pub from_resale: u64,
    pub from_first: u64,
    pub steps: u8,
    pub budget_hit: bool,
    pub graduated_now: bool,
    pub bonding_money: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkDown {
    pub tokens: u64,
    pub money: u128,
    pub from_f: u128,
    pub from_k: u128,
    pub from_c: u128,
    pub steps: u8,
    pub budget_hit: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Snap {
    i: u8,
    ask_left: u64,
    back: u64,
    place: u128,
}

const UNDO_MAX: usize = MAX_WALK_STEPS as usize + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Undo {
    sqrt_p: Sqrt,
    sqrt_f: Sqrt,
    sqrt_l_max: Sqrt,
    floor: u128,
    floor_tokens: u64,
    n_bands: u8,
    n_c: u8,
    band_edge: [Sqrt; BIDS_MAX + 1],
    band_money: [u128; BIDS_MAX],
    band_tokens: [u64; BIDS_MAX],
    band_f: [u128; BIDS_MAX],
    band_f_claim: [u128; BIDS_MAX],
    ck_at_f: u128,
    ck_floor_tokens: u64,
    graduated: bool,
    touched: u64,
    n: u8,
    snaps: [Snap; UNDO_MAX],
    pub n_bought: u8,
    pub bought: [Piece; PIECES_MAX],
}

impl Undo {
    pub const EMPTY: Undo = Undo {
        sqrt_p: 0,
        sqrt_f: 0,
        sqrt_l_max: 0,
        floor: 0,
        floor_tokens: 0,
        n_bands: 0,
        n_c: 0,
        band_edge: [0; BIDS_MAX + 1],
        band_money: [0; BIDS_MAX],
        band_tokens: [0; BIDS_MAX],
        band_f: [0; BIDS_MAX],
        band_f_claim: [0; BIDS_MAX],
        ck_at_f: 0,
        ck_floor_tokens: 0,
        graduated: false,
        touched: 0,
        n: 0,
        snaps: [Snap { i: 0, ask_left: 0, back: 0, place: 0 }; UNDO_MAX],
        n_bought: 0,
        bought: [Piece { sqrt_lo: 0, sqrt_hi: 0, money: 0, tokens: 0, range: 0 }; PIECES_MAX],
    };

    pub fn start(&mut self, b: &Book) {
        self.sqrt_p = b.sqrt_p;
        self.sqrt_f = b.sqrt_f;
        self.sqrt_l_max = b.sqrt_l_max;
        self.floor = b.floor;
        self.floor_tokens = b.floor_tokens;
        self.n_bands = b.n_bands;
        self.n_c = b.n_c;
        self.band_edge = b.band_edge;
        self.band_money = b.band_money;
        self.band_tokens = b.band_tokens;
        self.band_f = b.band_f;
        self.band_f_claim = b.band_f_claim;
        self.ck_at_f = b.ck_at_f;
        self.ck_floor_tokens = b.ck_floor_tokens;
        self.graduated = b.graduated;
        self.touched = 0;
        self.n = 0;
        self.n_bought = 0;
    }

    pub fn bought(&self) -> &[Piece] {
        &self.bought[..self.n_bought as usize]
    }

    fn buy_first(&mut self, i: usize, lo: Sqrt, hi: Sqrt, money: u128, tokens: u64) -> Result<(), MathError> {
        let n = self.n_bought as usize;
        if n > 0 && self.bought[n - 1].range as usize == i {
            let p = &mut self.bought[n - 1];
            p.sqrt_hi = hi;
            p.money = add(p.money, money)?;
            p.tokens = p.tokens.checked_add(tokens).ok_or(MathError::Overflow)?;
            return Ok(());
        }
        if n == PIECES_MAX {
            return Err(MathError::StepBudget);
        }
        self.bought[n] = Piece { sqrt_lo: lo, sqrt_hi: hi, money, tokens, range: i as u8 };
        self.n_bought += 1;
        Ok(())
    }

    fn touch(&mut self, b: &Book, i: usize) -> Result<(), MathError> {
        if self.touched & (1u64 << i) != 0 {
            return Ok(());
        }
        if self.n as usize == UNDO_MAX {
            return Err(MathError::StepBudget);
        }
        let r = &b.ranges[i];
        self.snaps[self.n as usize] = Snap { i: i as u8, ask_left: r.ask_left, back: r.back, place: r.place };
        self.n += 1;
        self.touched |= 1u64 << i;
        Ok(())
    }

    pub fn rollback(&self, b: &mut Book) {
        b.sqrt_p = self.sqrt_p;
        b.sqrt_f = self.sqrt_f;
        b.sqrt_l_max = self.sqrt_l_max;
        b.floor = self.floor;
        b.floor_tokens = self.floor_tokens;
        b.n_bands = self.n_bands;
        b.n_c = self.n_c;
        b.band_edge = self.band_edge;
        b.band_money = self.band_money;
        b.band_tokens = self.band_tokens;
        b.band_f = self.band_f;
        b.band_f_claim = self.band_f_claim;
        b.ck_at_f = self.ck_at_f;
        b.ck_floor_tokens = self.ck_floor_tokens;
        b.graduated = self.graduated;
        for s in &self.snaps[..self.n as usize] {
            let r = &mut b.ranges[s.i as usize];
            r.ask_left = s.ask_left;
            r.back = s.back;
            r.place = s.place;
        }
    }
}

fn ov<T>(x: Option<T>) -> Result<T, MathError> {
    x.ok_or(MathError::Overflow)
}

fn add(a: u128, b: u128) -> Result<u128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

fn cp_up(l: u128, p: Sqrt, hi: Sqrt, rem: u128, held: u64, all_at_top: bool) -> Result<(u64, u128, Sqrt), MathError> {
    if held == 0 || p >= hi || l == 0 {
        return Ok((0, 0, p));
    }
    let (p2, q) = if let Some(q_full) = money_between(l, p, hi, Round::Up).filter(|q| rem >= *q) {
        (hi, q_full)
    } else {
        let p2 = ov(sqrt_after_money(l, p, rem))?.min(hi);
        (p2, ov(money_between(l, p, p2, Round::Up))?)
    };
    if p2 <= p {
        return Ok((0, 0, p));
    }
    let mut dt = ov(tokens_between(l, p, p2, Round::Down))?.min(held as u128) as u64;
    if all_at_top && p2 == hi {
        dt = held;
    }
    if dt == 0 {
        return Ok((0, 0, p));
    }
    Ok((dt, q, p2))
}

fn cp_down(l: u128, lo: Sqrt, p: Sqrt, rem: u64, held: u128) -> Result<(u64, u128, Sqrt), MathError> {
    if held == 0 || p <= lo || l == 0 {
        return Ok((0, 0, p));
    }
    let x_full = ov(tokens_between(l, lo, p, Round::Up))?;
    let (p2, dx) = if rem as u128 >= x_full {
        (lo, x_full as u64)
    } else {
        let p2 = ov(sqrt_after_tokens(l, p, rem as u128))?.max(lo);
        if p2 >= p {
            return Ok((0, 0, p));
        }
        (p2, ov(tokens_between(l, p2, p, Round::Up))?.min(rem as u128) as u64)
    };
    let q = ov(money_between(l, p2, p, Round::Down))?.min(held);
    if q == 0 {
        return Ok((0, 0, p));
    }
    Ok((dx, q, p2))
}

fn flat_can_buy(s: Sqrt, money: u128) -> bool {
    money > 0
        && match flat_tokens_for_money(s, money, Round::Down) {
            Some(t) if t >= 1 => flat_money_for_tokens(s, t, Round::Down).is_some_and(|q| q > 0),
            _ => false,
        }
}

impl Book {
    pub fn fees_now(&self) -> [u32; FEE_LEGS] {
        self.fees_at(self.sqrt_p)
    }

    pub fn fee_acc(&self) -> FeeAcc {
        let mut acc = FeeAcc::new([0; FEE_LEGS]);
        acc.pre = self.fees_at_memo(self.sqrt_p, &mut acc.memo);
        acc
    }

    pub fn fee_acc_with<M: Lg>(&self, mut memo: M) -> FeeAcc<M> {
        let pre = self.fees_at_memo(self.sqrt_p, &mut memo);
        FeeAcc::with_memo(pre, memo)
    }

    pub fn warm_lg(&self, cache: &mut super::fees::LgCache) {
        for r in &self.ranges[..(self.n_ranges as usize).min(self.ranges.len())] {
            cache.warm(r.sqrt_a);
            cache.warm(r.sqrt_b);
        }
        for e in &self.band_edge[..(self.n_bands as usize + 1).min(self.band_edge.len())] {
            cache.warm(*e);
        }
        cache.warm(self.sqrt_f);
        cache.warm(self.sqrt_l_max);
        cache.warm(self.sqrt_p);
        if self.sqrt_fee_b != 0 {
            let _ = cache.knots(self.sqrt_fee_a, self.sqrt_fee_b);
        }
    }

    fn charge<M: Lg>(&self, acc: &mut FeeAcc<M>, q: u128) -> Result<(), MathError> {
        if q > 0 {
            let at = super::fees::rates_at(self.graduated, self.own_fees, &self.fee_ppm, &self.fee_start_ppm, self.sqrt_fee_a, self.sqrt_fee_b, self.sqrt_p, &mut acc.memo);
            ov(acc.add(q, &at))?;
        }
        Ok(())
    }

    pub fn fees_at(&self, sqrt_p: Sqrt) -> [u32; FEE_LEGS] {
        self.fees_at_memo(sqrt_p, &mut super::fees::LgMemo::default())
    }

    pub fn fees_at_memo<M: Lg + ?Sized>(&self, sqrt_p: Sqrt, memo: &mut M) -> [u32; FEE_LEGS] {
        super::fees::rates_at(self.graduated, self.own_fees, &self.fee_ppm, &self.fee_start_ppm, self.sqrt_fee_a, self.sqrt_fee_b, sqrt_p, memo)
    }

    pub fn empty(sqrt_p: Sqrt, fee_ppm: [u32; FEE_LEGS]) -> Book {
        Book {
            sqrt_p,
            sqrt_f: 0,
            sqrt_l_max: 0,
            floor: 0,
            floor_tokens: 0,
            n_bands: 0,
            n_c: 0,
            band_edge: [0; BIDS_MAX + 1],
            band_money: [0; BIDS_MAX],
            band_tokens: [0; BIDS_MAX],
            band_f: [0; BIDS_MAX],
            band_f_claim: [0; BIDS_MAX],
            ck_at_f: 0,
            ck_floor_tokens: 0,
            fee_ppm,
            fee_start_ppm: [0; FEE_LEGS],
            sqrt_fee_a: 0,
            sqrt_fee_b: 0,
            graduated: false,
            own_fees: false,
            n_ranges: 0,
            ranges: [Range::default(); RANGES_MAX],
        }
    }

    pub fn push_range(&mut self, sqrt_a: Sqrt, sqrt_b: Sqrt, tokens: u64, kind: u8, class: u8) -> Result<(), MathError> {
        let n = self.n_ranges as usize;
        if n == RANGES_MAX {
            return Err(MathError::RowTooManyRanges);
        }
        if sqrt_a == 0 || sqrt_a > sqrt_b || sqrt_b > SQRT_TAIL_END || (n > 0 && self.ranges[n - 1].sqrt_b != sqrt_a) {
            return Err(MathError::SqrtRange);
        }
        self.ranges[n] = Range { sqrt_a, sqrt_b, tokens, ask_left: tokens, kind, class, ..Range::default() };
        self.n_ranges += 1;
        Ok(())
    }

    fn range_lo(&self, r: &Range) -> Sqrt {
        r.sqrt_a.max(self.sqrt_l_max)
    }

    fn cp_edges(&self, pc: Part) -> (Sqrt, Sqrt) {
        match pc {
            Part::Range(i) => (self.range_lo(&self.ranges[i]), self.ranges[i].sqrt_b),
            Part::Band(i) => (self.band_edge[i + 1], self.band_edge[i]),
            Part::Floor => (self.sqrt_f, self.sqrt_f),
        }
    }

    fn tokens_of(&self, pc: Part) -> u64 {
        match pc {
            Part::Range(i) => self.ranges[i].held(),
            Part::Band(i) => self.band_tokens[i],
            Part::Floor => self.floor_tokens,
        }
    }

    fn money_of(&self, pc: Part) -> u128 {
        match pc {
            Part::Range(i) => self.ranges[i].place,
            Part::Band(i) => self.band_money[i],
            Part::Floor => self.floor,
        }
    }

    fn is_flat(&self, pc: Part) -> bool {
        match pc {
            Part::Range(i) => self.ranges[i].is_flat(),
            Part::Floor => true,
            _ => false,
        }
    }

    fn flat_s(&self, pc: Part) -> Sqrt {
        match pc {
            Part::Range(i) => self.ranges[i].sqrt_a,
            _ => self.sqrt_f,
        }
    }

    fn flat_bids(&self, i: usize) -> bool {
        let r = &self.ranges[i];
        r.is_flat() && flat_can_buy(r.sqrt_a, r.place)
    }

    fn floor_bids(&self) -> bool {
        self.sqrt_f != 0 && flat_can_buy(self.sqrt_f, self.floor)
    }

    fn pieces(&self) -> impl Iterator<Item = Part> + '_ {
        let nb = self.n_bands as usize;
        (0..self.n_ranges as usize)
            .map(Part::Range)
            .chain((0..nb).map(Part::Band))
            .chain((self.sqrt_f != 0).then_some(Part::Floor))
    }

    pub fn money_held(&self) -> u128 {
        self.bid_money() + self.ranges[..self.n_ranges as usize].iter().map(|r| r.place).sum::<u128>()
    }

    pub fn f_own(&self) -> u128 {
        self.floor - self.ck_at_f + self.band_f[..self.n_bands as usize].iter().sum::<u128>()
    }

    pub fn f_claims(&self) -> u128 {
        self.band_f_claim[..self.n_bands as usize].iter().sum()
    }

    pub fn ck_money(&self) -> u128 {
        let nb = self.n_bands as usize;
        self.band_money[..nb].iter().zip(&self.band_f[..nb]).map(|(m, f)| m - f).sum::<u128>() + self.ck_at_f
    }

    pub fn ck_held(&self) -> u64 {
        self.band_tokens[..self.n_bands as usize].iter().sum::<u64>() + self.ck_floor_tokens
    }

    pub fn bid_money(&self) -> u128 {
        self.floor + self.band_money[..self.n_bands as usize].iter().sum::<u128>()
    }

    pub fn vault(&self) -> u128 {
        self.f_own() + self.ranges[..self.n_ranges as usize].iter().map(|r| r.place).sum::<u128>()
    }

    pub fn sqrt_t0(&self) -> Sqrt {
        self.ranges[..self.n_ranges as usize].iter().rev().find(|r| r.kind == KIND_TAIL).map(|r| r.sqrt_a).unwrap_or(SQRT_TAIL_END)
    }

    pub fn tokens_held(&self) -> u128 {
        let mut t = self.floor_tokens as u128;
        for x in &self.band_tokens[..self.n_bands as usize] {
            t += *x as u128;
        }
        for r in &self.ranges[..self.n_ranges as usize] {
            t += r.held() as u128;
        }
        t
    }

    pub fn for_sale(&self) -> u64 {
        self.ranges[..self.n_ranges as usize].iter().map(|r| r.ask_left).sum()
    }

    pub fn bought_back(&self) -> u64 {
        let b: u64 = self.ranges[..self.n_ranges as usize].iter().map(|r| r.back).sum();
        b + self.floor_tokens + self.band_tokens[..self.n_bands as usize].iter().sum::<u64>()
    }

    pub fn check(&self) -> Result<(), MathError> {
        let n = self.n_ranges as usize;
        if n == 0 || n > RANGES_MAX || self.n_bands as usize > BIDS_MAX || self.n_c > self.n_bands {
            return Err(MathError::BookEmpty);
        }
        if self.sqrt_p == 0 || self.sqrt_p > SQRT_TAIL_END {
            return Err(MathError::SqrtRange);
        }
        for i in 0..n {
            let r = &self.ranges[i];
            if r.sqrt_a == 0 || r.sqrt_a > r.sqrt_b || (i > 0 && self.ranges[i - 1].sqrt_b != r.sqrt_a) {
                return Err(MathError::SqrtRange);
            }
            if r.ask_left.checked_add(r.back).is_none() {
                return Err(MathError::Overflow);
            }
            if r.kind != KIND_BONDING && (r.place != 0 || r.back != 0) {
                return Err(MathError::Overflow);
            }
            if r.kind == KIND_BONDING && i != 0 {
                return Err(MathError::SqrtRange);
            }
        }
        if self.ranges[n - 1].sqrt_b != SQRT_TAIL_END || self.ranges[n - 1].is_flat() {
            return Err(MathError::SqrtRange);
        }
        let nb = self.n_bands as usize;
        if nb > 0 {
            if self.band_edge[0] > self.sqrt_l_max || self.band_edge[nb] < self.sqrt_f {
                return Err(MathError::SqrtRange);
            }
            for i in 0..nb {
                if self.band_edge[i + 1] > self.band_edge[i] {
                    return Err(MathError::SqrtRange);
                }
                if self.band_f[i] > self.band_money[i] {
                    return Err(MathError::Overflow);
                }
            }
        }
        if self.ck_at_f > self.floor || self.ck_floor_tokens > self.floor_tokens {
            return Err(MathError::Overflow);
        }
        Ok(())
    }

    fn flat_up(&self) -> Option<Part> {
        let p = self.sqrt_p;
        if self.sqrt_f != 0 && self.sqrt_f == p && self.floor_tokens > 0 {
            return Some(Part::Floor);
        }
        (0..self.n_ranges as usize)
            .find(|i| {
                let r = &self.ranges[*i];
                r.is_flat() && r.sqrt_a == p && r.held() > 0
            })
            .map(Part::Range)
    }

    fn flat_down(&self) -> Option<Part> {
        let p = self.sqrt_p;
        if let Some(i) = (0..self.n_ranges as usize).rev().find(|i| self.ranges[*i].sqrt_a == p && self.flat_bids(*i)) {
            return Some(Part::Range(i));
        }
        if self.sqrt_f == p && self.floor_bids() {
            return Some(Part::Floor);
        }
        None
    }

    fn cp_piece_up(&self) -> Option<Part> {
        let p = self.sqrt_p;
        self.pieces().find(|pc| {
            let (lo, hi) = self.cp_edges(*pc);
            !self.is_flat(*pc) && lo < hi && lo <= p && p < hi && self.tokens_of(*pc) > 0
        })
    }

    fn band_down(&self) -> Option<usize> {
        let p = self.sqrt_p;
        (0..self.n_bands as usize).find(|j| {
            let (lo, hi) = (self.band_edge[*j + 1], self.band_edge[*j]);
            lo < hi && lo < p && p <= hi && self.band_money[*j] > 0
        })
    }

    fn next_up(&self) -> Option<Sqrt> {
        let p = self.sqrt_p;
        self.pieces()
            .filter(|pc| self.tokens_of(*pc) > 0)
            .filter_map(|pc| {
                if self.is_flat(pc) {
                    Some(self.flat_s(pc))
                } else {
                    let (lo, hi) = self.cp_edges(pc);
                    (lo < hi).then_some(lo)
                }
            })
            .filter(|s| *s > p)
            .min()
    }

    fn next_down(&self) -> Option<Sqrt> {
        let p = self.sqrt_p;
        self.pieces()
            .filter_map(|pc| match pc {
                Part::Range(i) if self.ranges[i].is_flat() => self.flat_bids(i).then_some(self.ranges[i].sqrt_a),
                Part::Floor => self.floor_bids().then_some(self.sqrt_f),
                Part::Band(j) => {
                    let (lo, hi) = (self.band_edge[j + 1], self.band_edge[j]);
                    (lo < hi && self.band_money[j] > 0).then_some(hi)
                }
                Part::Range(_) => None
            })
            .filter(|s| *s < p)
            .max()
    }

    fn l_up(&self, pc: Part, hi: Sqrt) -> Result<u128, MathError> {
        match pc {
            Part::Range(i) => ov(self.ranges[i].liquidity()),
            _ => ov(liquidity_for_tokens(self.sqrt_p, hi, self.tokens_of(pc) as u128)),
        }
    }

    fn l_down(&self, pc: Part, lo: Sqrt) -> Result<u128, MathError> {
        match pc {
            Part::Range(i) => ov(self.ranges[i].liquidity()),
            _ => ov(liquidity_for_money(lo, self.sqrt_p, self.money_of(pc))),
        }
    }

    fn graduate(&mut self) -> Result<u128, MathError> {
        let b = &mut self.ranges[0];
        let raised = b.place;
        self.floor = add(self.floor, raised)?;
        b.place = 0;
        self.sqrt_f = b.sqrt_a;
        self.sqrt_l_max = self.sqrt_l_max.max(b.sqrt_a);
        self.graduated = true;
        Ok(raised)
    }

    #[inline(never)]
    pub fn walk_up(&mut self, budget: u128, u: &mut Undo) -> Result<WalkUp, MathError> {
        let mut acc = self.fee_acc();
        self.walk_up_from(budget, WalkUp::default(), &mut acc, MAX_WALK_STEPS, u)
    }

    #[inline(never)]
    pub fn walk_up_from<M: Lg>(&mut self, budget: u128, w0: WalkUp, acc: &mut FeeAcc<M>, max_steps: u8, u: &mut Undo) -> Result<WalkUp, MathError> {
        let mut rem = budget;
        let mut w = WalkUp { budget_hit: false, ..w0 };
        while rem > 0 {
            if w.steps >= max_steps.min(MAX_WALK_STEPS) {
                w.budget_hit = true;
                break;
            }
            w.steps += 1;
            if let Some(pc) = self.flat_up() {
                let s = self.flat_s(pc);
                let can = ov(flat_tokens_for_money(s, rem, Round::Down))?;
                let take = can.min(self.tokens_of(pc) as u128) as u64;
                if take == 0 {
                    break;
                }
                let cost = ov(flat_money_for_tokens(s, take as u128, Round::Up))?;
                rem -= cost;
                w.money += cost;
                self.charge(acc, cost)?;
                w.tokens += take;
                match pc {
                    Part::Floor => {
                        self.floor_rebuy(take, cost)?;
                        w.from_resale += take;
                    }
                    Part::Range(i) => {
                        u.touch(self, i)?;
                        let bonding = self.ranges[i].kind == KIND_BONDING;
                        let r = &mut self.ranges[i];
                        let tb = take.min(r.back);
                        r.back -= tb;
                        r.ask_left -= take - tb;
                        w.from_resale += tb;
                        w.from_first += take - tb;
                        if bonding {
                            r.place = add(r.place, cost)?;
                            if r.held() == 0 && !self.graduated {
                                w.bonding_money = self.graduate()?;
                                w.graduated_now = true;
                            }
                        } else {
                            self.sqrt_l_max = self.sqrt_l_max.max(s);
                            u.buy_first(i, s, s, cost, take)?;
                        }
                    }
                    Part::Band(_) => unreachable!(),
                }
                if self.tokens_of(pc) > 0 {
                    break;
                }
                continue;
            }
            if let Some(pc) = self.cp_piece_up() {
                let (_, hi) = self.cp_edges(pc);
                let l = self.l_up(pc, hi)?;
                let p = self.sqrt_p;
                let moved = match pc {
                    Part::Range(i) => {
                        u.touch(self, i)?;
                        self.range_up(i, l, hi, &mut rem, &mut w, acc, u)?
                    }
                    Part::Band(j) => {
                        let (dt, q, p2) = cp_up(l, p, hi, rem, self.band_tokens[j], true)?;
                        if dt > 0 {
                            self.band_rebuy(j, dt, q)?;
                            self.sqrt_p = p2;
                            rem -= q;
                            w.money += q;
                            w.tokens += dt;
                            w.from_resale += dt;
                            self.charge(acc, q)?;
                        }
                        dt > 0
                    }
                    Part::Floor => unreachable!(),
                };
                if self.sqrt_p == hi {
                    continue;
                }
                if !moved && rem > 0 && ov(tokens_between(l, self.sqrt_p, hi, Round::Down))? == 0 {
                    if let Part::Range(i) = pc {
                        let q = ov(money_between(l, self.sqrt_p, hi, Round::Up))?;
                        if q > rem {
                            break;
                        }
                        u.buy_first(i, self.sqrt_p, hi, q, 0)?;
                        rem -= q;
                        w.money += q;
                        self.charge(acc, q)?;
                        self.sqrt_l_max = self.sqrt_l_max.max(hi);
                    }
                    self.sqrt_p = hi;
                    continue;
                }
                if !moved || rem == 0 || self.tokens_of(pc) > 0 {
                    break;
                }
                continue;
            }
            match self.next_up() {
                Some(s) => self.sqrt_p = s,
                None => break,
            }
        }
        Ok(w)
    }

    fn floor_rebuy(&mut self, take: u64, cost: u128) -> Result<(), MathError> {
        let (x0, xe) = (self.floor_tokens as u128, self.ck_floor_tokens as u128);
        if xe > 0 {
            let back = ov(mul_div(cost, xe, x0, Round::Down))?;
            let gone = ov(mul_div(take as u128, xe, x0, Round::Up))?.min(xe);
            self.ck_at_f = add(self.ck_at_f, back)?;
            self.ck_floor_tokens -= gone as u64;
        }
        self.floor_tokens -= take;
        self.floor = add(self.floor, cost)?;
        Ok(())
    }

    fn band_rebuy(&mut self, j: usize, dt: u64, q: u128) -> Result<(), MathError> {
        let claim = self.band_f_claim[j];
        let r = ov(mul_div(claim, dt as u128, self.band_tokens[j] as u128, Round::Up))?.min(claim).min(q);
        self.band_tokens[j] -= dt;
        self.band_money[j] = add(self.band_money[j], q)?;
        self.band_f[j] += r;
        self.band_f_claim[j] -= r;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn range_up<M: Lg>(&mut self, i: usize, l: u128, hi: Sqrt, rem: &mut u128, w: &mut WalkUp, acc: &mut FeeAcc<M>, u: &mut Undo) -> Result<bool, MathError> {
        let p = self.sqrt_p;
        let (dt, q, p2) = cp_up(l, p, hi, *rem, self.ranges[i].ask_left, false)?;
        if dt == 0 {
            return Ok(false);
        }
        self.ranges[i].ask_left -= dt;
        self.sqrt_p = p2;
        self.sqrt_l_max = self.sqrt_l_max.max(p2);
        u.buy_first(i, p, p2, q, dt)?;
        *rem -= q;
        w.money += q;
        w.tokens += dt;
        w.from_first += dt;
        self.charge(acc, q)?;
        Ok(true)
    }

    #[inline(never)]
    pub fn walk_down(&mut self, tokens: u64, u: &mut Undo) -> Result<WalkDown, MathError> {
        let mut acc = self.fee_acc();
        self.walk_down_acc(tokens, &mut acc, u)
    }

    #[inline(never)]
    pub fn walk_down_acc<M: Lg>(&mut self, tokens: u64, acc: &mut FeeAcc<M>, u: &mut Undo) -> Result<WalkDown, MathError> {
        let mut rem = tokens;
        let mut w = WalkDown::default();
        while rem > 0 {
            if w.steps == MAX_WALK_STEPS {
                w.budget_hit = true;
                break;
            }
            w.steps += 1;
            if let Some(pc) = self.flat_down() {
                let s = self.flat_s(pc);
                let money = self.money_of(pc);
                let can = ov(flat_tokens_for_money(s, money, Round::Down))?;
                let take = can.min(rem as u128) as u64;
                let q = if take == 0 { 0 } else { ov(flat_money_for_tokens(s, take as u128, Round::Down))?.min(money) };
                if q == 0 {
                    break;
                }
                rem -= take;
                w.tokens += take;
                w.money += q;
                self.charge(acc, q)?;
                match pc {
                    Part::Floor => {
                        let ce = q.min(self.ck_at_f);
                        if ce > 0 {
                            let xe = ov(flat_tokens_for_money(s, ce, Round::Down))?.min(take as u128) as u64;
                            self.ck_at_f -= ce;
                            self.ck_floor_tokens += xe;
                        }
                        self.floor -= q;
                        self.floor_tokens += take;
                        w.from_f += q;
                    }
                    Part::Range(i) => {
                        u.touch(self, i)?;
                        let r = &mut self.ranges[i];
                        r.place -= q;
                        r.back += take;
                    }
                    Part::Band(_) => unreachable!(),
                }
                continue;
            }
            if let Some(j) = self.band_down() {
                let (lo, _) = self.cp_edges(Part::Band(j));
                let l = self.l_down(Part::Band(j), lo)?;
                let (dx, q, p2) = cp_down(l, lo, self.sqrt_p, rem, self.band_money[j])?;
                if dx == 0 {
                    if ov(money_between(l, lo, self.sqrt_p, Round::Down))? == 0 {
                        match self.next_down() {
                            Some(s) => {
                                self.sqrt_p = s;
                                continue;
                            }
                            None => break,
                        }
                    }
                    break;
                }
                let d_f = ov(flat_money_for_tokens(self.sqrt_f, dx as u128, Round::Down))?.min(self.band_f[j]).min(q);
                let q = q.min(self.band_money[j] - self.band_f[j] + d_f);
                self.band_f[j] -= d_f;
                self.band_f_claim[j] = add(self.band_f_claim[j], d_f)?;
                self.band_money[j] -= q;
                self.band_tokens[j] += dx;
                if j < self.n_c as usize {
                    w.from_c += q;
                } else {
                    w.from_k += q;
                }
                self.sqrt_p = p2;
                rem -= dx;
                w.tokens += dx;
                w.money += q;
                self.charge(acc, q)?;
                if p2 == lo && self.band_money[j] > 0 {
                    self.floor = add(self.floor, self.band_money[j])?;
                    self.ck_at_f = add(self.ck_at_f, self.band_money[j] - self.band_f[j])?;
                    self.band_money[j] = 0;
                    self.band_f[j] = 0;
                }
                if p2 == lo || self.band_money[j] == 0 {
                    if p2 != lo {
                        match self.next_down() {
                            Some(s) => self.sqrt_p = s,
                            None => break,
                        }
                    }
                    continue;
                }
                break;
            }
            match self.next_down() {
                Some(s) => self.sqrt_p = s,
                None => break,
            }
        }
        Ok(w)
    }
}

#[cfg(not(target_os = "solana"))]
#[path = "book_cp.rs"]
mod cp;
#[cfg(not(target_os = "solana"))]
pub use cp::WalkCheckpoints;
