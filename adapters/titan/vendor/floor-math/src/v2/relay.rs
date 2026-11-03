// SPDX-License-Identifier: Apache-2.0
use super::book::{Book, Range, KIND_BONDING};
use super::split::SplitRow;
use super::sqrt::{
    flat_money_for_tokens, flat_tokens_for_money, liquidity_for_money, money_between, sqrt_after_tokens, tokens_between,
    Sqrt, FRAC,
};
use super::wide::{div_u256, mul_div, shl_div, Round};
use crate::u256::U256;
use crate::MathError;

pub use super::book::BIDS_MAX;
pub const CUT_NUDGES: u32 = 8;

fn ovf<T>(x: Option<T>) -> Result<T, MathError> {
    x.ok_or(MathError::Overflow)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seg {
    pub lo: Sqrt,
    pub hi: Sqrt,
    pub money: u128,
    pub f_in: u128,
}

fn slice(l: u128, a: Sqrt, hi: Sqrt, sqrt_f: Sqrt) -> Result<(u128, u128, u128), MathError> {
    let money = ovf(money_between(l, a, hi, Round::Down))?;
    let tok = ovf(tokens_between(l, a, hi, Round::Up))?;
    let f_in = ovf(flat_money_for_tokens(sqrt_f, tok, Round::Up))?.min(money);
    Ok((money, tok, f_in))
}

fn cut_root(l: u128, hi: Sqrt, sqrt_f: Sqrt, left: u128) -> Option<Sqrt> {
    let f2 = U256::sqr_u128(sqrt_f);
    let f_hi = div_u256(f2, hi, Round::Up)?.to_u128()?;
    let lt = shl_div(U256::from_u128(left), FRAC, l, Round::Down)?.to_u128()?;
    let beta = hi.checked_add(f_hi)?.checked_sub(lt)?;
    let two_f = f2.checked_add(f2)?;
    let disc = U256::sqr_u128(beta)
        .checked_sub(two_f.checked_add(two_f)?)
        .unwrap_or(U256::ZERO);
    let s = beta.checked_add(isqrt_near(disc))?;
    Some(s / 2 + (s & 1))
}

fn isqrt_u64(m: u64) -> u64 {
    if m < 2 {
        return m;
    }
    let mut x = 1u64 << (64 - m.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + m / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

fn isqrt_near(n: U256) -> u128 {
    if n.hi == 0 && n.lo < 2 {
        return n.lo;
    }
    if U256::sqr_u128(u128::MAX) <= n {
        return u128::MAX;
    }
    let bits = if n.hi != 0 { 256 - n.hi.leading_zeros() } else { 128 - n.lo.leading_zeros() };
    let sh = (bits.saturating_sub(64) + 1) & !1;
    let m = match sh {
        0 => n.lo as u64,
        1..=127 => ((n.lo >> sh) | (n.hi << (128 - sh))) as u64,
        _ => (n.hi >> (sh - 128)) as u64,
    };
    let r = isqrt_u64(m);
    if sh == 0 {
        return r as u128;
    }
    let r1 = r as u128 + 1;
    let mut x = if 128 - r1.leading_zeros() + sh / 2 > 128 { u128::MAX } else { r1 << (sh / 2) };
    loop {
        let q = match n.div_rem_u128(x).0.to_u128() {
            Some(q) if q < x => q,
            _ => return x,
        };
        let y = q + (x - q) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

fn absorb(sqrt_f: Sqrt, edge: Sqrt, left: u128) -> Result<Seg, MathError> {
    if edge <= sqrt_f {
        return Ok(Seg::default());
    }
    let money = ovf(mul_div(left, edge, edge - sqrt_f, Round::Down))?;
    let f_in = ovf(mul_div(money, sqrt_f, edge, Round::Up))?.min(money);
    Ok(Seg {
        lo: sqrt_f,
        hi: edge,
        money,
        f_in,
    })
}

pub fn mirror_into(
    ranges: &[Range],
    sqrt_f: Sqrt,
    top: Sqrt,
    premium: u128,
    tok_cap: u64,
    absorb_last: bool,
    out: &mut [Seg],
) -> Result<(u8, u64, u128), MathError> {
    mirror_memo(ranges, sqrt_f, top, premium, tok_cap, absorb_last, out, &mut (0, 0))
}

#[allow(clippy::too_many_arguments)]
fn mirror_memo(
    ranges: &[Range],
    sqrt_f: Sqrt,
    top: Sqrt,
    premium: u128,
    tok_cap: u64,
    absorb_last: bool,
    out: &mut [Seg],
    lm: &mut (usize, u128),
) -> Result<(u8, u64, u128), MathError> {
    let (mut left, mut tok_left, mut top, mut n) = (premium, tok_cap as u128, top, 0usize);
    if left == 0 || tok_left == 0 || out.is_empty() || top <= sqrt_f {
        return Ok((0, 0, left));
    }
    let mut cut_done = false;
    let below = ranges.partition_point(|r| r.sqrt_a < top);
    for (ri, r) in ranges[..below].iter().enumerate().rev() {
        if r.kind == KIND_BONDING || r.is_flat() || r.sqrt_a >= top {
            continue;
        }
        let hi = top.min(r.sqrt_b);
        let lo = r.sqrt_a.max(sqrt_f);
        if hi <= lo {
            continue;
        }
        let l = if lm.0 == ri + 1 { lm.1 } else { ovf(r.liquidity())? };
        *lm = (ri + 1, l);
        if l == 0 {
            continue;
        }
        if absorb_last && n + 1 == out.len() {
            break;
        }
        let (money, tok, f_in) = slice(l, lo, hi, sqrt_f)?;
        if money - f_in <= left && tok <= tok_left {
            out[n] = Seg {
                lo,
                hi,
                money,
                f_in,
            };
            n += 1;
            left -= money - f_in;
            tok_left -= tok;
            top = lo;
            if left == 0 || tok_left == 0 || n == out.len() {
                break;
            }
            continue;
        }
        let at = if tok > tok_left { sqrt_after_tokens(l, hi, tok_left).unwrap_or(lo) } else { lo };
        let ap = cut_root(l, hi, sqrt_f, left).unwrap_or(lo);
        let a0 = ap.max(at).max(lo).min(hi);
        for a in (a0..hi).take(CUT_NUDGES as usize + 1) {
            let (money, tok, f_in) = slice(l, a, hi, sqrt_f)?;
            if money - f_in <= left && tok <= tok_left {
                out[n] = Seg {
                    lo: a,
                    hi,
                    money,
                    f_in,
                };
                n += 1;
                left -= money - f_in;
                tok_left -= tok;
                top = a;
                break;
            }
        }
        cut_done = true;
        break;
    }
    if absorb_last && !cut_done && left > 0 && tok_left > 0 && n + 1 == out.len() && top > sqrt_f {
        let s = absorb(sqrt_f, top, left)?;
        let tok = match s.money {
            0 => 0,
            m => ovf(tokens_between(ovf(liquidity_for_money(s.lo, s.hi, m))?, s.lo, s.hi, Round::Up))?,
        };
        if s.money > 0 && tok <= tok_left {
            tok_left -= tok;
            left -= s.money - s.f_in;
            out[n] = s;
            n += 1;
        }
    }
    Ok((n as u8, (tok_cap as u128 - tok_left) as u64, left))
}

pub fn mirror(
    ranges: &[Range],
    sqrt_f: Sqrt,
    top: Sqrt,
    premium: u128,
    tok_cap: u64,
    out: &mut [Seg; BIDS_MAX],
) -> Result<(u8, u64, u128), MathError> {
    mirror_into(ranges, sqrt_f, top, premium, tok_cap, false, out)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Relay {
    pub n_c: u8,
    pub n_k: u8,
    pub laid_c: u128,
    pub laid_k: u128,
    pub at_f: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bid {
    pub sqrt_p: Sqrt,
    pub sqrt_f: Sqrt,
    pub floor: u128,
    pub floor_tokens: u64,
    pub n_bands: u8,
    pub n_c: u8,
    pub band_edge: [Sqrt; BIDS_MAX + 1],
    pub band_money: [u128; BIDS_MAX],
    pub band_tokens: [u64; BIDS_MAX],
    pub band_f: [u128; BIDS_MAX],
    pub band_f_claim: [u128; BIDS_MAX],
    pub band_c: [bool; BIDS_MAX],
    pub ck_at_f: u128,
    pub ck_floor_tokens: u64,
}

impl Bid {
    pub fn new(sqrt_p: Sqrt, sqrt_f: Sqrt, floor: u128) -> Bid {
        Bid {
            sqrt_p,
            sqrt_f,
            floor,
            floor_tokens: 0,
            n_bands: 0,
            n_c: 0,
            band_edge: [0; BIDS_MAX + 1],
            band_money: [0; BIDS_MAX],
            band_tokens: [0; BIDS_MAX],
            band_f: [0; BIDS_MAX],
            band_f_claim: [0; BIDS_MAX],
            band_c: [false; BIDS_MAX],
            ck_at_f: 0,
            ck_floor_tokens: 0,
        }
    }
    fn n(&self) -> usize {
        self.n_bands as usize
    }
    pub fn f_own(&self) -> u128 {
        self.floor - self.ck_at_f
            + (0..self.n())
                .map(|i| self.band_f[i].min(self.band_money[i]))
                .sum::<u128>()
    }
    pub fn f_claims(&self) -> u128 {
        self.band_f_claim[..self.n()].iter().sum()
    }
    pub fn ck_money(&self) -> u128 {
        (0..self.n())
            .map(|i| self.band_money[i].saturating_sub(self.band_f[i]))
            .sum::<u128>()
            + self.ck_at_f
    }
    pub fn ck_held(&self) -> u128 {
        self.band_tokens[..self.n()]
            .iter()
            .map(|t| *t as u128)
            .sum::<u128>()
            + self.ck_floor_tokens as u128
    }
    pub fn bid_money(&self) -> u128 {
        self.floor + self.band_money[..self.n()].iter().sum::<u128>()
    }
}

pub fn relay(b: &mut Book, row: &SplitRow, extra_ck: u128, ck_out: u128, circ: u64) -> Result<Relay, MathError> {
    let mut bid = Bid {
        sqrt_p: b.sqrt_p,
        sqrt_f: b.sqrt_f,
        floor: b.floor,
        floor_tokens: b.floor_tokens,
        n_bands: b.n_bands,
        n_c: b.n_c,
        band_edge: b.band_edge,
        band_money: b.band_money,
        band_tokens: b.band_tokens,
        band_f: b.band_f,
        band_f_claim: b.band_f_claim,
        band_c: core::array::from_fn(|i| i < b.n_c as usize),
        ck_at_f: b.ck_at_f,
        ck_floor_tokens: b.ck_floor_tokens,
    };
    let n_ranges = b.n_ranges as usize;
    let r = relay_bid(&mut bid, &b.ranges[..n_ranges], row.c_share_bps, extra_ck, ck_out, circ)?;
    b.floor = bid.floor;
    b.floor_tokens = bid.floor_tokens;
    b.n_bands = bid.n_bands;
    b.n_c = bid.band_c[..bid.n_bands as usize].iter().filter(|c| **c).count() as u8;
    b.band_edge = bid.band_edge;
    b.band_money = bid.band_money;
    b.band_tokens = bid.band_tokens;
    b.band_f = bid.band_f;
    b.band_f_claim = bid.band_f_claim;
    b.ck_at_f = bid.ck_at_f;
    b.ck_floor_tokens = bid.ck_floor_tokens;
    Ok(r)
}

pub fn relay_bid(
    bid: &mut Bid,
    ranges: &[Range],
    c_share_bps: u16,
    extra_ck: u128,
    ck_out: u128,
    circ: u64,
) -> Result<Relay, MathError> {
    let (p, f) = (bid.sqrt_p, bid.sqrt_f);
    let mut pool = ovf(bid.ck_at_f.checked_add(extra_ck))?;
    let mut floor = bid.floor - bid.ck_at_f;
    let mut edge = [0 as Sqrt; BIDS_MAX + 1];
    let mut segs = [Seg::default(); BIDS_MAX];
    let mut tok = [0u64; BIDS_MAX];
    let mut claim = [0u128; BIDS_MAX];
    let mut cflag = [false; BIDS_MAX];
    let mut n = 0usize;
    for i in 0..bid.n() {
        let (lo, hi) = (bid.band_edge[i + 1], bid.band_edge[i]);
        let fi = bid.band_f[i].min(bid.band_money[i]);
        pool = ovf(pool.checked_add(bid.band_money[i] - fi))?;
        floor = ovf(floor.checked_add(fi))?;
        let klo = lo.max(p);
        if hi > klo && (bid.band_tokens[i] > 0 || bid.band_f_claim[i] > 0) {
            if n > 0 && edge[n] != hi {
                segs[n] = Seg {
                    lo: hi,
                    hi: edge[n],
                    money: 0,
                    f_in: 0,
                };
                edge[n + 1] = hi;
                n += 1;
            }
            edge[n] = hi;
            segs[n] = Seg {
                lo: klo,
                hi,
                money: 0,
                f_in: 0,
            };
            tok[n] = bid.band_tokens[i];
            claim[n] = bid.band_f_claim[i];
            cflag[n] = bid.band_c[i];
            edge[n + 1] = klo;
            n += 1;
        } else if bid.band_tokens[i] > 0 {
            let t = bid.band_tokens[i];
            let mine = if f == 0 {
                t
            } else {
                ovf(flat_tokens_for_money(f, bid.band_f_claim[i], Round::Up))?.min(t as u128) as u64
            };
            bid.floor_tokens = ovf(bid.floor_tokens.checked_add(t))?;
            bid.ck_floor_tokens = ovf(bid.ck_floor_tokens.checked_add(t - mine))?;
        }
    }
    pool = ovf(pool.checked_sub(ck_out))?;
    let room = p > f && f > 0;
    if room && n > 0 && edge[n] > p {
        segs[n] = Seg {
            lo: p,
            hi: edge[n],
            money: 0,
            f_in: 0,
        };
        edge[n + 1] = p;
        n += 1;
    }
    let n_top = n;
    let mut r = Relay::default();
    let mut lk = pool;
    if room && n + 1 < BIDS_MAX {
        let cp = ovf(mul_div(pool, c_share_bps as u128, 10_000, Round::Down))?;
        if U256::mul_u128(floor, 1 << 16) >= U256::sqr_u128(f) {
            return Err(MathError::Overflow);
        }
        let mut backed = None;
        let mut cap = circ;
        let mut lm = (0usize, 0u128);
        let (nc, nkk, l2) = loop {
            let (nc, xc, lc) = mirror_memo(ranges, f, p, cp, cap, false, &mut segs[n..BIDS_MAX - 1], &mut lm)?;
            let nc = nc as usize;
            let ktop = if nc > 0 { segs[n + nc - 1].lo } else { p };
            let (nkk, _, l2) = mirror_memo(
                ranges,
                f,
                ktop,
                pool - cp + lc,
                cap - xc,
                true,
                &mut segs[n + nc..],
                &mut lm,
            )?;
            let nkk = nkk as usize;
            let fi: u128 = segs[n..n + nc + nkk].iter().map(|s| s.f_in).sum();
            if fi <= floor || cap == 0 {
                break (nc, nkk, l2);
            }
            segs[n..].fill(Seg::default());
            let backed = match backed {
                Some(x) => x,
                None => *backed.insert(ovf(flat_tokens_for_money(f, floor, Round::Down))?),
            };
            cap = if (cap as u128) > backed {
                backed as u64
            } else {
                let short = ovf(flat_tokens_for_money(f, fi - floor, Round::Up))?;
                (cap as u128).saturating_sub(short + (nc + nkk) as u128 + 1) as u64
            };
        };
        r.n_c = nc as u8;
        r.n_k = nkk as u8;
        if n == 0 && nc + nkk > 0 {
            edge[0] = p;
        }
        for (j, s) in segs[n..n + nc + nkk].iter().enumerate() {
            edge[n + j + 1] = s.lo;
            cflag[n + j] = j < nc;
            if j < nc {
                r.laid_c += s.money - s.f_in;
            } else {
                r.laid_k += s.money - s.f_in;
            }
        }
        n += nc + nkk;
        lk = l2;
    }
    let f_in: u128 = segs[n_top..n].iter().map(|s| s.f_in).sum();
    bid.floor = ovf(floor.checked_sub(f_in))? + lk;
    bid.ck_at_f = lk;
    bid.n_bands = n as u8;
    bid.n_c = cflag[..n].iter().filter(|c| **c).count() as u8;
    bid.band_edge = [0; BIDS_MAX + 1];
    if n > 0 {
        bid.band_edge[..=n].copy_from_slice(&edge[..=n]);
    }
    for i in 0..BIDS_MAX {
        let live = i < n;
        bid.band_money[i] = if live { segs[i].money } else { 0 };
        bid.band_f[i] = if live { segs[i].f_in } else { 0 };
        bid.band_tokens[i] = if live { tok[i] } else { 0 };
        bid.band_f_claim[i] = if live { claim[i] } else { 0 };
        bid.band_c[i] = live && cflag[i];
    }
    r.at_f = lk;
    Ok(r)
}

#[cfg(test)]
#[path = "../../tests/common/json.rs"]
mod json;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::book::{KIND_CLIMB, KIND_TAIL};
    use crate::v2::sqrt::{liquidity_for_money, ONE};

    const S0: Sqrt = ONE / 64;
    const TOK: u64 = 100_000_000_000_000;

    fn ranges() -> [Range; 6] {
        let mut r = [Range::default(); 6];
        r[0] = Range {
            sqrt_a: S0,
            sqrt_b: S0,
            tokens: TOK,
            kind: KIND_BONDING,
            ..Range::default()
        };
        for i in 0..5 {
            r[i + 1] = Range {
                sqrt_a: S0 << i,
                sqrt_b: S0 << (i + 1),
                tokens: TOK,
                kind: if i == 4 { KIND_TAIL } else { KIND_CLIMB },
                ..Range::default()
            };
        }
        r
    }

    fn rng(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn check(b: &Bid) {
        let n = b.n();
        for i in 0..n {
            assert!(
                b.band_edge[i + 1] < b.band_edge[i],
                "descending, non-empty: {i}"
            );
            assert!(b.band_f[i] <= b.band_money[i]);
            assert!(b.band_edge[i + 1] >= b.sqrt_f);
        }
        assert!(b.ck_at_f <= b.floor);
        assert!(b.ck_floor_tokens <= b.floor_tokens);
        assert!(b.n_c as usize <= n);
    }

    fn backed(b: &Bid, circ: u64) -> bool {
        b.f_own() + b.f_claims()
            >= flat_money_for_tokens(b.sqrt_f, circ as u128 + b.ck_held(), Round::Up).unwrap()
    }

    fn state(pool: u128) -> (Bid, [Range; 6], u64) {
        let circ = 3 * TOK;
        let f = S0 * 2;
        let f_own = flat_money_for_tokens(f, circ as u128, Round::Up).unwrap() * 11 / 10;
        let mut b = Bid::new(S0 * 24, f, f_own + pool);
        b.ck_at_f = pool;
        let r = ranges();
        relay_bid(&mut b, &r, 5_000, 0, 0, circ).unwrap();
        (b, r, circ)
    }

    fn sell(b: &mut Bid, mut t: u64) -> (u128, u128, u64) {
        let (mut paid, mut from_f, mut sold) = (0u128, 0u128, 0u64);
        for i in 0..b.n() {
            if t == 0 {
                break;
            }
            let (lo, hi) = (b.band_edge[i + 1], b.band_edge[i]);
            let top = hi.min(b.sqrt_p);
            if top <= lo || b.band_money[i] == 0 {
                continue;
            }
            let l = liquidity_for_money(lo, top, b.band_money[i]).unwrap();
            let cap = tokens_between(l, lo, top, Round::Down).unwrap() as u64;
            let (took, u1) = if t >= cap {
                (cap, lo)
            } else {
                (
                    t,
                    sqrt_after_tokens(l, top, t as u128).unwrap().clamp(lo, top),
                )
            };
            let q = if u1 == lo {
                b.band_money[i]
            } else {
                money_between(l, u1, top, Round::Down)
                    .unwrap()
                    .min(b.band_money[i])
            };
            let df = q
                .min(b.band_f[i])
                .min(flat_money_for_tokens(b.sqrt_f, took as u128, Round::Down).unwrap())
                .max(q.saturating_sub(b.band_money[i] - b.band_f[i]));
            b.band_money[i] -= q;
            b.band_tokens[i] += took;
            b.band_f[i] -= df;
            b.band_f_claim[i] += df;
            paid += q;
            from_f += df;
            sold += took;
            t -= took;
            b.sqrt_p = u1;
        }
        (paid, from_f, sold)
    }

    #[test]
    fn isqrt_near_is_wide_isqrt() {
        use crate::v2::wide::isqrt;
        let mut seed = 11u64;
        let check = |n: U256| assert_eq!(isqrt_near(n), isqrt(n), "{n:?}");
        for k in 0..256u32 {
            let p = if k < 128 { U256 { hi: 0, lo: 1u128 << k } } else { U256 { hi: 1u128 << (k - 128), lo: 0 } };
            check(p);
            if let Some(m) = p.checked_sub(U256::from_u128(1)) {
                check(m);
            }
            check(p.checked_add(U256::from_u128(1)).unwrap());
        }
        for x in [0u128, 1, 2, 3, u64::MAX as u128, u64::MAX as u128 + 1, u128::MAX - 1, u128::MAX] {
            let sq = U256::sqr_u128(x);
            check(sq);
            if let Some(m) = sq.checked_sub(U256::from_u128(1)) {
                check(m);
            }
            if let Some(p) = sq.checked_add(U256::from_u128(1)) {
                check(p);
            }
        }
        check(U256 { hi: u128::MAX, lo: u128::MAX });
        for _ in 0..200_000 {
            let sh = rng(&mut seed) % 256;
            let mut w = || (rng(&mut seed) as u128) << 64 | rng(&mut seed) as u128;
            let n = U256 { hi: w(), lo: w() };
            let n = if sh >= 128 { U256 { hi: 0, lo: n.lo >> (sh - 128) } } else { U256 { hi: n.hi >> sh, lo: n.lo } };
            check(n);
            let x = (rng(&mut seed) as u128) << 64 | rng(&mut seed) as u128;
            let x = x >> (rng(&mut seed) % 128);
            check(U256::sqr_u128(x));
            if x > 0 {
                check(U256::sqr_u128(x).checked_sub(U256::from_u128(1)).unwrap());
            }
        }
    }

    #[test]
    fn cut_root_never_over_lays() {
        let r = ranges()[3];
        let l = r.liquidity().unwrap();
        let f = S0;
        let (m, _, fi) = slice(l, r.sqrt_a, r.sqrt_b, f).unwrap();
        let mut seed = 5u64;
        for _ in 0..200 {
            let left = rng(&mut seed) as u128 % (m - fi);
            let a = cut_root(l, r.sqrt_b, f, left).unwrap();
            let (m2, _, f2) = slice(l, a, r.sqrt_b, f).unwrap();
            assert!(m2 - f2 <= left, "the closed form never over-lays");
            assert!(
                left - (m2 - f2) <= 2,
                "and lands within the roundings of the premium: {} {}",
                left,
                m2 - f2
            );
        }
    }

    #[test]
    fn mirror_skips_bonding_and_takes_whole_slices_top_down() {
        let r = ranges();
        let mut out = [Seg::default(); BIDS_MAX];
        let (n, tok, left) = mirror(&r, S0, S0 * 32, u128::MAX / 4, u64::MAX, &mut out).unwrap();
        assert_eq!(
            n, 5,
            "every issuance range under the top, never the Bonding"
        );
        assert_eq!(
            left,
            u128::MAX / 4 - out[..5].iter().map(|s| s.money - s.f_in).sum::<u128>()
        );
        assert!((5 * TOK..=5 * TOK + 5).contains(&tok));
        for (i, s) in out[..5].iter().enumerate() {
            assert_eq!((s.lo, s.hi), (S0 << (4 - i), S0 << (5 - i)));
        }
    }

    #[test]
    fn mirror_cut_fits_and_stops() {
        let r = ranges();
        let mut out = [Seg::default(); BIDS_MAX];
        let mut seed = 11u64;
        for _ in 0..100 {
            let prem = rng(&mut seed) as u128 % 2_000_000_000_000;
            let cap = if rng(&mut seed).is_multiple_of(4) {
                rng(&mut seed) % (2 * TOK)
            } else {
                u64::MAX
            };
            let (n, tok, left) = mirror(&r, S0 * 3 / 2, S0 * 24, prem, cap, &mut out).unwrap();
            let spent: u128 = out[..n as usize].iter().map(|s| s.money - s.f_in).sum();
            assert_eq!(spent + left, prem, "premium conserved");
            assert!(tok <= cap);
            for w in out[..n as usize].windows(2) {
                assert_eq!(w[0].lo, w[1].hi, "contiguous");
            }
        }
    }

    #[test]
    fn lay_is_c_on_top_then_k_premium_only() {
        let pool = 40_000_000_000;
        let (b, r, circ) = state(pool);
        check(&b);
        assert!(b.n_bands >= 2 && b.n_c >= 1);
        assert_eq!(b.band_edge[0], b.sqrt_p, "C starts at the price");
        assert_eq!(b.ck_money(), pool, "all of C + K's money");
        let c: u128 = (0..b.n_c as usize)
            .map(|i| b.band_money[i] - b.band_f[i])
            .sum();
        assert!(
            c <= pool / 2 && pool / 2 - c < 1_000,
            "C is the top half: {c}"
        );
        assert!(
            b.ck_at_f < 1_000,
            "nothing waits at f while the mirror has room: {}",
            b.ck_at_f
        );
        assert!(backed(&b, circ));
        for i in 0..b.n_bands as usize {
            let (lo, hi) = (b.band_edge[i + 1], b.band_edge[i]);
            let rr = r
                .iter()
                .find(|x| x.sqrt_a <= lo && hi <= x.sqrt_b && !x.is_flat())
                .unwrap();
            assert_eq!(
                b.band_money[i],
                money_between(rr.liquidity().unwrap(), lo, hi, Round::Down).unwrap()
            );
        }
    }

    #[test]
    fn matches_the_model_in_f64() {
        let pool = 40_000_000_000u128;
        let (b, r, _) = state(pool);
        let q = |x: Sqrt| x as f64 / ONE as f64;
        let fnn = q(b.sqrt_f) * q(b.sqrt_f);
        let (mut left, mut top, mut c_lo) = (pool as f64 / 2.0, q(b.sqrt_p), 0.0);
        for x in r.iter().rev().filter(|x| !x.is_flat()) {
            let (va, vb, l) = (q(x.sqrt_a), q(x.sqrt_b), x.liquidity().unwrap() as f64);
            if va >= top {
                continue;
            }
            let (hi, lo) = (top.min(vb), va.max(q(b.sqrt_f)));
            let p = l * (hi - lo) - fnn * l * (1.0 / lo - 1.0 / hi);
            if p <= left {
                left -= p;
                top = lo;
                c_lo = lo;
                continue;
            }
            let be = l * hi + fnn * l / hi - left;
            c_lo = (be + (be * be - 4.0 * l * l * fnn).sqrt()) / (2.0 * l);
            break;
        }
        let rust = q(b.band_edge[b.n_c as usize]);
        assert!(
            (rust / c_lo - 1.0).abs() < 1e-9,
            "C's lower edge: rust {rust} model {c_lo}"
        );
    }

    #[test]
    fn relay_conserves_f_and_ck_separately() {
        let (mut b, r, circ) = state(40_000_000_000);
        let (f0, ck0, v0) = (b.f_own(), b.ck_money(), b.bid_money());
        for extra in [0u128, 0, 123_456_789] {
            let (f1, ck1) = (b.f_own(), b.ck_money());
            relay_bid(&mut b, &r, 5_000, extra, 0, circ).unwrap();
            check(&b);
            assert_eq!(b.f_own(), f1);
            assert_eq!(b.ck_money(), ck1 + extra);
        }
        assert_eq!(
            (b.f_own(), b.ck_money(), b.bid_money()),
            (f0, ck0 + 123_456_789, v0 + 123_456_789)
        );
    }

    #[test]
    fn sells_then_relays_conserve_and_stay_backed() {
        let (mut b, r, mut circ) = state(40_000_000_000);
        let mut seed = 99u64;
        let mut ops = 0;
        for i in 0..80 {
            let (f0, cl0, ck0) = (b.f_own(), b.f_claims(), b.ck_money());
            let t = (rng(&mut seed) % (circ / 200)).max(1_000_000);
            let (paid, from_f, sold) = sell(&mut b, t);
            if sold == 0 {
                break;
            }
            assert_eq!(f0 - b.f_own(), from_f, "F pays its f per token, op {i}");
            assert_eq!(b.f_claims() - cl0, from_f, "and holds the claim, op {i}");
            assert_eq!(
                ck0 - b.ck_money(),
                paid - from_f,
                "C + K pays the premium, op {i}"
            );
            circ -= sold;
            let (f1, cl1, ck1, held1) = (b.f_own(), b.f_claims(), b.ck_money(), b.ck_held());
            relay_bid(&mut b, &r, 5_000, 0, 0, circ).unwrap();
            check(&b);
            assert_eq!(
                (b.f_own(), b.f_claims(), b.ck_money(), b.ck_held()),
                (f1, cl1, ck1, held1),
                "relay, op {i}"
            );
            assert!(backed(&b, circ), "backed, op {i}");
            assert!(b.n_bands == 0 || b.band_edge[0] >= b.sqrt_p);
            ops += 1;
        }
        assert!(ops >= 10, "the walk ran {ops} sells");
    }

    #[test]
    fn no_room_waits_at_f() {
        let r = ranges();
        let mut b = Bid::new(S0 * 4, S0 * 4, 1_000 + 77);
        b.ck_at_f = 77;
        let x = relay_bid(&mut b, &r, 5_000, 0, 0, TOK).unwrap();
        assert_eq!((b.n_bands, b.ck_at_f, b.floor, x.at_f), (0, 77, 1_077, 77));
    }

    #[test]
    fn tokens_cap_at_circulating() {
        let r = ranges();
        let circ = TOK / 1_000;
        let mut b = Bid::new(S0 * 24, S0 * 2, u64::MAX as u128 + 1_000_000_000_000_000);
        b.ck_at_f = 1_000_000_000_000_000;
        relay_bid(&mut b, &r, 5_000, 0, 0, circ).unwrap();
        let mut laid = 0u128;
        for i in 0..b.n_bands as usize {
            let (lo, hi) = (b.band_edge[i + 1], b.band_edge[i]);
            let rr = r
                .iter()
                .find(|x| !x.is_flat() && x.sqrt_a <= lo && hi <= x.sqrt_b)
                .unwrap();
            laid += tokens_between(rr.liquidity().unwrap(), lo, hi, Round::Up).unwrap();
        }
        assert!(laid <= circ as u128, "laid {laid} of {circ}");
        assert!(b.ck_at_f > 0, "the rest waits at f");
    }

    #[test]
    fn full_list_absorbs_the_rest_of_k() {
        let mut r = [Range::default(); 40];
        let mut a = S0;
        for x in r.iter_mut() {
            let b = a + a / 32;
            *x = Range {
                sqrt_a: a,
                sqrt_b: b,
                tokens: TOK / 100,
                kind: KIND_CLIMB,
                ..Range::default()
            };
            a = b;
        }
        let top = r[39].sqrt_b;
        let mut b = Bid::new(top, S0, u64::MAX as u128 * 4 + 10_000_000_000_000);
        b.ck_at_f = 10_000_000_000_000;
        let ck = b.ck_money();
        let f = b.f_own();
        let b0 = b;
        let x = relay_bid(&mut b, &r, 5_000, 0, 0, u64::MAX).unwrap();
        check(&b);
        let mut c = b0;
        let y = relay_bid(&mut c, &r, 5_000, 0, 0, 50 * TOK).unwrap();
        check(&c);
        assert!(y.at_f > 1, "the rest past the cap waits at f");
        assert_eq!((c.ck_money(), c.f_own()), (ck, f));
        assert_eq!(b.n_bands as usize, BIDS_MAX);
        assert_eq!(b.band_edge[BIDS_MAX], S0, "the last K piece reaches f");
        assert!(
            x.at_f <= 1,
            "the absorb piece takes the rest but its rounding: {}",
            x.at_f
        );
        assert_eq!((b.ck_money(), b.f_own()), (ck, f));
    }

    #[test]
    fn relay_unit_golden() {
        use super::json::{parse, J};
        let dir = std::env::var("FLOOR_SPLIT_GOLDEN")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/golden/split").into());
        let Ok(text) = std::fs::read_to_string(format!("{dir}/relay_unit.json")) else {
            eprintln!("relay_unit_golden: no {dir}/relay_unit.json, skipped");
            return;
        };
        let u = |j: &J| j.u128();
        let mut scen: Vec<(String, Vec<Range>)> = Vec::new();
        let cases = parse(&text);
        let cases = cases.at("cases").arr();
        assert!(!cases.is_empty());
        for (k, c) in cases.iter().enumerate() {
            let from = c.at("ranges_from").s().to_string();
            if !scen.iter().any(|(n, _)| *n == from) {
                let sj = parse(&std::fs::read_to_string(format!("{dir}/{from}")).unwrap());
                let rs = sj
                    .at("ranges")
                    .arr()
                    .iter()
                    .map(|r| Range {
                        sqrt_a: u(r.at("a")),
                        sqrt_b: u(r.at("b")),
                        tokens: u(r.at("tokens")) as u64,
                        kind: u(r.at("kind")) as u8,
                        ..Range::default()
                    })
                    .collect();
                scen.push((from.clone(), rs));
            }
            let ranges = &scen.iter().find(|(n, _)| *n == from).unwrap().1;
            let (bj, aj) = (c.at("before"), c.at("after"));
            let mut b = Bid::new(u(bj.at("sqrt_p")), u(bj.at("sqrt_f")), u(bj.at("floor")));
            b.floor_tokens = u(bj.at("floor_tokens")) as u64;
            b.ck_at_f = u(bj.at("ck_at_f"));
            b.ck_floor_tokens = u(bj.at("ck_floor_tokens")) as u64;
            let load = |b: &mut Bid, segs: &[J]| {
                assert!(segs.len() <= BIDS_MAX);
                b.n_bands = segs.len() as u8;
                for (i, s) in segs.iter().enumerate() {
                    b.band_edge[i] = u(s.at("hi"));
                    b.band_edge[i + 1] = u(s.at("lo"));
                    b.band_money[i] = u(s.at("money"));
                    b.band_tokens[i] = u(s.at("tokens")) as u64;
                    b.band_f[i] = u(s.at("f"));
                    b.band_f_claim[i] = u(s.at("claim"));
                    b.band_c[i] = s.at("c").bool();
                }
                b.n_c = b.band_c.iter().filter(|c| **c).count() as u8;
            };
            load(&mut b, bj.at("segs").arr());
            let cs = &c.at("consts");
            let r = relay_bid(
                &mut b,
                ranges,
                u(cs.at("c_share_bps")) as u16,
                u(c.at("extra_ck")),
                c.get("ck_out").map(u).unwrap_or(0),
                u(bj.at("circ")) as u64,
            )
            .unwrap_or_else(|e| panic!("case {k}: {e:?}"));
            let mut want = Bid::new(u(aj.at("sqrt_p")), u(aj.at("sqrt_f")), u(aj.at("floor")));
            want.floor_tokens = u(aj.at("floor_tokens")) as u64;
            want.ck_at_f = u(aj.at("ck_at_f"));
            want.ck_floor_tokens = u(aj.at("ck_floor_tokens")) as u64;
            load(&mut want, aj.at("segs").arr());
            assert_eq!(want.n_c as u128, u(aj.at("n_c")), "case {k}: n_c");
            assert_eq!(b, want, "case {k}: the bid");
            let rj = c.at("relay");
            let got = [r.n_c as u128, r.n_k as u128, r.laid_c, r.laid_k, r.at_f];
            let exp = ["n_c", "n_k", "laid_c", "laid_k", "at_f"].map(|x| u(rj.at(x)));
            assert_eq!(got, exp, "case {k}: relay");
            assert_eq!(
                (b.f_own(), b.f_claims(), b.ck_money(), b.ck_held(), b.bid_money()),
                (
                    u(aj.at("f_own")),
                    u(aj.at("f_claims")),
                    u(aj.at("ck_money")),
                    u(aj.at("ck_held")),
                    u(aj.at("bid_money"))
                ),
                "case {k}: reads"
            );
        }
        eprintln!("relay_unit_golden: {} cases bit for bit", cases.len());
    }
}
