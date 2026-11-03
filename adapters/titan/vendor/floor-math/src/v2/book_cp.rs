// SPDX-License-Identifier: Apache-2.0
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Scal {
    sqrt_p: Sqrt,
    sqrt_f: Sqrt,
    sqrt_l_max: Sqrt,
    floor: u128,
    floor_tokens: u64,
    ck_at_f: u128,
    ck_floor_tokens: u64,
    graduated: bool,
}

impl Scal {
    fn of(b: &Book) -> Scal {
        Scal {
            sqrt_p: b.sqrt_p,
            sqrt_f: b.sqrt_f,
            sqrt_l_max: b.sqrt_l_max,
            floor: b.floor,
            floor_tokens: b.floor_tokens,
            ck_at_f: b.ck_at_f,
            ck_floor_tokens: b.ck_floor_tokens,
            graduated: b.graduated,
        }
    }
    #[inline(always)]
    fn put(&self, b: &mut Book) {
        b.sqrt_p = self.sqrt_p;
        b.sqrt_f = self.sqrt_f;
        b.sqrt_l_max = self.sqrt_l_max;
        b.floor = self.floor;
        b.floor_tokens = self.floor_tokens;
        b.ck_at_f = self.ck_at_f;
        b.ck_floor_tokens = self.ck_floor_tokens;
        b.graduated = self.graduated;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RangeD {
    i: u8,
    ask_left: u64,
    back: u64,
    place: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BandD {
    j: u8,
    money: u128,
    tokens: u64,
    f: u128,
    f_claim: u128,
}

#[derive(Clone, Copy, Debug)]
struct Cp<W> {
    v: u128,
    h: u128,
    spent: u128,
    w: W,
    whole: [u128; FEE_LEGS],
    rem: [u32; FEE_LEGS],
    scal: Scal,
    ranges: (u16, u16),
    bands: (u16, u16),
    touched: u64,
    n: u8,
    n_bought: u8,
    last: Piece,
}

#[derive(Clone, Debug)]
struct Trail<W> {
    cps: Vec<Cp<W>>,
    ranges: Vec<RangeD>,
    bands: Vec<BandD>,
    undo: Box<Undo>,
}

impl<W: Copy> Trail<W> {
    fn new() -> Trail<W> {
        Trail { cps: Vec::new(), ranges: Vec::new(), bands: Vec::new(), undo: Box::new(Undo::EMPTY) }
    }

    #[allow(clippy::too_many_arguments)]
    fn record<M>(&mut self, base: &Book, b: &Book, v: u128, h: u128, spent: u128, w: W, acc: &FeeAcc<M>) -> bool {
        let (r0, b0) = (self.ranges.len(), self.bands.len());
        for i in 0..RANGES_MAX {
            let (x, y) = (&base.ranges[i], &b.ranges[i]);
            if (x.ask_left, x.back, x.place) != (y.ask_left, y.back, y.place) {
                self.ranges.push(RangeD { i: i as u8, ask_left: y.ask_left, back: y.back, place: y.place });
            }
        }
        for j in 0..BIDS_MAX {
            if (base.band_money[j], base.band_tokens[j], base.band_f[j], base.band_f_claim[j]) != (b.band_money[j], b.band_tokens[j], b.band_f[j], b.band_f_claim[j]) {
                self.bands.push(BandD { j: j as u8, money: b.band_money[j], tokens: b.band_tokens[j], f: b.band_f[j], f_claim: b.band_f_claim[j] });
            }
        }
        let u = &self.undo;
        let nb = u.n_bought as usize;
        let cp = Cp {
            v,
            h,
            spent,
            w,
            whole: acc.whole,
            rem: acc.rem,
            scal: Scal::of(b),
            ranges: (r0 as u16, self.ranges.len() as u16),
            bands: (b0 as u16, self.bands.len() as u16),
            touched: u.touched,
            n: u.n,
            n_bought: u.n_bought,
            last: if nb > 0 { u.bought[nb - 1] } else { Piece::default() },
        };
        let mut t = *base;
        self.put_book(&cp, &mut t);
        if t != *b {
            self.ranges.truncate(r0);
            self.bands.truncate(b0);
            return false;
        }
        self.cps.push(cp);
        true
    }

    #[inline(always)]
    fn pick(&self, budget: u128, max_steps: u8) -> Option<&Cp<W>> {
        let n = self.cps.len().min(max_steps.min(MAX_WALK_STEPS) as usize);
        let k = self.cps[..n].partition_point(|c| c.v <= budget && budget <= c.h);
        k.checked_sub(1).map(|k| &self.cps[k])
    }

    #[inline(always)]
    fn put_book(&self, c: &Cp<W>, b: &mut Book) {
        c.scal.put(b);
        for d in &self.ranges[c.ranges.0 as usize..c.ranges.1 as usize] {
            let r = &mut b.ranges[d.i as usize];
            r.ask_left = d.ask_left;
            r.back = d.back;
            r.place = d.place;
        }
        for d in &self.bands[c.bands.0 as usize..c.bands.1 as usize] {
            let j = d.j as usize;
            b.band_money[j] = d.money;
            b.band_tokens[j] = d.tokens;
            b.band_f[j] = d.f;
            b.band_f_claim[j] = d.f_claim;
        }
    }

    #[inline(always)]
    fn restore<M>(&self, c: &Cp<W>, b: &mut Book, acc: &mut FeeAcc<M>, u: &mut Undo) {
        self.put_book(c, b);
        acc.whole = c.whole;
        acc.rem = c.rem;
        let (n, nb) = (c.n as usize, c.n_bought as usize);
        u.touched = c.touched;
        u.n = c.n;
        u.snaps[..n].copy_from_slice(&self.undo.snaps[..n]);
        u.n_bought = c.n_bought;
        u.bought[..nb].copy_from_slice(&self.undo.bought[..nb]);
        if nb > 0 {
            u.bought[nb - 1] = c.last;
        }
    }
}

fn least(hint: u128, ok: impl Fn(u128) -> bool) -> Option<u128> {
    let hint = hint.max(1);
    let (mut lo, mut hi) = if ok(hint) {
        if hint == 1 || !ok(hint - 1) {
            return Some(hint);
        }
        (0u128, hint - 1)
    } else {
        let mut x = hint;
        loop {
            let y = x.saturating_mul(2);
            if ok(y) {
                break (x, y);
            }
            if y == u128::MAX {
                return None;
            }
            x = y;
        }
    };
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(hi)
}

fn greatest(from: u128, ok: impl Fn(u128) -> bool) -> u128 {
    if ok(u128::MAX) {
        return u128::MAX;
    }
    let (mut lo, mut hi) = (from, u128::MAX);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

impl Book {
    fn need_up(&self) -> Option<(u128, u128)> {
        if let Some(pc) = self.flat_up() {
            let s = self.flat_s(pc);
            let held = self.tokens_of(pc) as u128;
            let can = |r: u128| flat_tokens_for_money(s, r, Round::Down);
            let hint = flat_money_for_tokens(s, held, Round::Up)?;
            let need = least(hint, |r| can(r).is_none_or(|t| t >= held))?;
            can(need)?;
            let cap = greatest(need, |r| can(r).is_some());
            return Some((need, cap));
        }
        if let Some(pc) = self.cp_piece_up() {
            let (_, hi) = self.cp_edges(pc);
            let l = self.l_up(pc, hi).ok()?;
            return Some((money_between(l, self.sqrt_p, hi, Round::Up)?.max(1), u128::MAX));
        }
        self.next_up().map(|_| (1, u128::MAX))
    }

    fn need_down(&self) -> Option<(u128, u128)> {
        if let Some(pc) = self.flat_down() {
            let can = flat_tokens_for_money(self.flat_s(pc), self.money_of(pc), Round::Down)?;
            return Some((can.max(1), u128::MAX));
        }
        if let Some(j) = self.band_down() {
            let (lo, _) = self.cp_edges(Part::Band(j));
            let l = self.l_down(Part::Band(j), lo).ok()?;
            if self.band_money[j] == 0 || self.sqrt_p <= lo || l == 0 {
                return Some((1, u128::MAX));
            }
            return Some((tokens_between(l, lo, self.sqrt_p, Round::Up)?.max(1), u128::MAX));
        }
        self.next_down().map(|_| (1, u128::MAX))
    }

    pub fn walk_down_from<M: Lg>(&mut self, tokens: u64, w0: WalkDown, acc: &mut FeeAcc<M>, max_steps: u8, u: &mut Undo) -> Result<WalkDown, MathError> {
        let mut rem = tokens;
        let mut w = WalkDown { budget_hit: false, ..w0 };
        while rem > 0 {
            if w.steps >= max_steps.min(MAX_WALK_STEPS) {
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

#[derive(Clone, Debug)]
pub struct WalkCheckpoints {
    book: Box<Book>,
    pre: [u32; FEE_LEGS],
    up: Trail<WalkUp>,
    down: Trail<WalkDown>,
}

impl WalkCheckpoints {
    pub fn new<M: Lg>(book: &Book, mut memo: M) -> WalkCheckpoints {
        let pre = book.fees_at_memo(book.sqrt_p, &mut memo);
        let up = Self::build_up(book, &mut memo);
        let down = Self::build_down(book, &mut memo);
        WalkCheckpoints { book: Box::new(*book), pre, up, down }
    }

    #[inline(always)]
    pub fn pre(&self) -> [u32; FEE_LEGS] {
        self.pre
    }

    pub fn book(&self) -> &Book {
        &self.book
    }

    pub fn marks(&self) -> (Vec<(u128, u128, u128)>, Vec<(u128, u128, u128)>) {
        (self.up.cps.iter().map(|c| (c.v, c.h, c.spent)).collect(), self.down.cps.iter().map(|c| (c.v, c.h, c.spent)).collect())
    }

    fn build_up<M: Lg>(base: &Book, memo: M) -> Trail<WalkUp> {
        let mut t = Trail::new();
        let mut b = *base;
        let mut acc = b.fee_acc_with(memo);
        t.undo.start(&b);
        let (mut w, mut v, mut h) = (WalkUp::default(), 0u128, u128::MAX);
        while w.steps < MAX_WALK_STEPS {
            let Some((need, cap)) = b.need_up() else { break };
            let Some(vk) = w.money.checked_add(need) else { break };
            let hk = w.money.saturating_add(cap);
            let (vk, hk) = (vk.max(v), hk.min(h));
            let Some(budget) = need.checked_add(1).filter(|x| *x <= cap) else { break };
            let Ok(w2) = b.walk_up_from(budget, w, &mut acc, w.steps + 1, &mut t.undo) else { break };
            if !w2.budget_hit || w2.steps != w.steps + 1 || vk > hk {
                break;
            }
            (w, v, h) = (w2, vk, hk);
            if !t.record(base, &b, v, h, w.money, w, &acc) {
                break;
            }
        }
        t
    }

    fn build_down<M: Lg>(base: &Book, memo: M) -> Trail<WalkDown> {
        let mut t = Trail::new();
        let mut b = *base;
        let mut acc = b.fee_acc_with(memo);
        t.undo.start(&b);
        let (mut w, mut v) = (WalkDown::default(), 0u128);
        while w.steps < MAX_WALK_STEPS {
            let Some((need, _)) = b.need_down() else { break };
            let spent = w.tokens as u128;
            let Some(vk) = spent.checked_add(need).map(|x| x.max(v)) else { break };
            let Some(budget) = need.checked_add(1).and_then(|x| u64::try_from(x).ok()) else { break };
            let Ok(w2) = b.walk_down_from(budget, w, &mut acc, w.steps + 1, &mut t.undo) else { break };
            if !w2.budget_hit || w2.steps != w.steps + 1 || vk > u64::MAX as u128 {
                break;
            }
            (w, v) = (w2, vk);
            if !t.record(base, &b, v, u128::MAX, w.tokens as u128, w, &acc) {
                break;
            }
        }
        t
    }

    #[inline]
    pub fn walk_up<M: Lg>(&self, b: &mut Book, budget: u128, acc: &mut FeeAcc<M>, max_steps: u8, u: &mut Undo) -> Result<WalkUp, MathError> {
        match self.up.pick(budget, max_steps) {
            None => b.walk_up_from(budget, WalkUp::default(), acc, max_steps, u),
            Some(c) => {
                self.up.restore(c, b, acc, u);
                b.walk_up_from(budget - c.spent, c.w, acc, max_steps, u)
            }
        }
    }

    #[inline]
    pub fn walk_down<M: Lg>(&self, b: &mut Book, tokens: u64, acc: &mut FeeAcc<M>, u: &mut Undo) -> Result<WalkDown, MathError> {
        match self.down.pick(tokens as u128, MAX_WALK_STEPS) {
            None => b.walk_down_acc(tokens, acc, u),
            Some(c) => {
                self.down.restore(c, b, acc, u);
                b.walk_down_from(tokens - c.spent as u64, c.w, acc, MAX_WALK_STEPS, u)
            }
        }
    }
}
