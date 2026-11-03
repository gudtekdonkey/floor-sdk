// SPDX-License-Identifier: Apache-2.0
use crate::u256::U256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    Down,
    Up,
}

pub type W3 = [u128; 3];

pub fn w3(x: U256) -> W3 {
    [x.lo, x.hi, 0]
}

pub fn mul_w(x: U256, m: u128) -> W3 {
    let lo = U256::mul_u128(x.lo, m);
    let hi = U256::mul_u128(x.hi, m);
    let (w1, c) = lo.hi.overflowing_add(hi.lo);
    [lo.lo, w1, hi.hi + c as u128]
}

pub fn shl_w(w: W3, k: u32) -> Option<W3> {
    let mut v = w;
    let mut k = k;
    while k >= 128 {
        if v[2] != 0 {
            return None;
        }
        v = [0, v[0], v[1]];
        k -= 128;
    }
    if k == 0 {
        return Some(v);
    }
    if v[2] >> (128 - k) != 0 {
        return None;
    }
    Some([v[0] << k, (v[1] << k) | (v[0] >> (128 - k)), (v[2] << k) | (v[1] >> (128 - k))])
}

pub fn shr_w(w: W3, k: u32, r: Round) -> W3 {
    let mut v = w;
    let mut k = k;
    let mut rem = false;
    while k >= 128 {
        rem |= v[0] != 0;
        v = [v[1], v[2], 0];
        k -= 128;
    }
    if k > 0 {
        rem |= v[0] << (128 - k) != 0;
        v = [(v[0] >> k) | (v[1] << (128 - k)), (v[1] >> k) | (v[2] << (128 - k)), v[2] >> k];
    }
    if rem && r == Round::Up {
        let (a, c0) = v[0].overflowing_add(1);
        v[0] = a;
        if c0 {
            let (b, c1) = v[1].overflowing_add(1);
            v[1] = b;
            if c1 {
                v[2] += 1;
            }
        }
    }
    v
}

pub fn w_u256(w: W3) -> Option<U256> {
    if w[2] != 0 {
        return None;
    }
    Some(U256 { hi: w[1], lo: w[0] })
}

pub fn w_u128(w: W3) -> Option<u128> {
    if w[2] != 0 || w[1] != 0 {
        return None;
    }
    Some(w[0])
}

pub fn div_w(w: W3, d: u128, r: Round) -> Option<U256> {
    if d == 0 {
        return None;
    }
    if w[2] >= d {
        return None;
    }
    let r2 = w[2];
    let (q1, r1) = if r2 == 0 && w[1] < d { (U256::ZERO, w[1]) } else { U256 { hi: r2, lo: w[1] }.div_rem_u128(d) };
    let (q0, r0) = U256 { hi: r1, lo: w[0] }.div_rem_u128(d);
    let q = U256 { hi: q1.lo, lo: q0.lo };
    if r == Round::Up && r0 != 0 {
        return q.checked_add(U256::from_u128(1));
    }
    Some(q)
}

fn w_ge(a: &W3, b: &W3) -> bool {
    for i in (0..3).rev() {
        if a[i] != b[i] {
            return a[i] > b[i];
        }
    }
    true
}

fn w_sub(a: &W3, b: &W3) -> W3 {
    let (r0, c0) = a[0].overflowing_sub(b[0]);
    let (r1, c1a) = a[1].overflowing_sub(b[1]);
    let (r1, c1b) = r1.overflowing_sub(c0 as u128);
    let r2 = a[2].wrapping_sub(b[2]).wrapping_sub((c1a || c1b) as u128);
    [r0, r1, r2]
}

fn w_bits(w: &W3) -> u32 {
    for i in (0..3).rev() {
        if w[i] != 0 {
            return 128 * i as u32 + 128 - w[i].leading_zeros();
        }
    }
    0
}

pub fn div_w_wide(n: W3, d: U256, r: Round) -> Option<U256> {
    if d.is_zero() {
        return None;
    }
    if d.hi == 0 {
        return div_w(n, d.lo, r);
    }
    let dw = w3(d);
    let mut q = [0u128; 3];
    let mut rem = [0u128; 3];
    let mut bit = w_bits(&n);
    while bit > 0 {
        bit -= 1;
        rem = [rem[0] << 1, (rem[1] << 1) | (rem[0] >> 127), (rem[2] << 1) | (rem[1] >> 127)];
        rem[0] |= (n[(bit / 128) as usize] >> (bit % 128)) & 1;
        if w_ge(&rem, &dw) {
            rem = w_sub(&rem, &dw);
            q[(bit / 128) as usize] |= 1 << (bit % 128);
        }
    }
    let mut out = w_u256(q)?;
    if r == Round::Up && rem != [0, 0, 0] {
        out = out.checked_add(U256::from_u128(1))?;
    }
    Some(out)
}

pub fn div_u256(x: U256, d: u128, r: Round) -> Option<U256> {
    div_w(w3(x), d, r)
}

pub fn mul_div(a: u128, b: u128, d: u128, r: Round) -> Option<u128> {
    #[cfg(not(target_os = "solana"))]
    if d != 0 {
        if let Some(p) = a.checked_mul(b) {
            let q = p / d;
            return if r == Round::Up && q * d != p { q.checked_add(1) } else { Some(q) };
        }
    }
    mul_div_wide(a, b, d, r)
}

pub fn mul_div_wide(a: u128, b: u128, d: u128, r: Round) -> Option<u128> {
    div_w(w3(U256::mul_u128(a, b)), d, r)?.to_u128()
}

pub fn mul_shr(a: u128, b: u128, k: u32, r: Round) -> Option<u128> {
    w_u128(shr_w(w3(U256::mul_u128(a, b)), k, r))
}

pub fn shl_div(x: U256, k: u32, d: u128, r: Round) -> Option<U256> {
    div_w(shl_w(w3(x), k)?, d, r)
}

pub fn isqrt(n: U256) -> u128 {
    if n.hi == 0 && n.lo < 2 {
        return n.lo;
    }
    if U256::sqr_u128(u128::MAX) <= n {
        return u128::MAX;
    }
    let bits = if n.hi != 0 { 256 - n.hi.leading_zeros() } else { 128 - n.lo.leading_zeros() };
    let e = bits.div_ceil(2);
    let mut x: u128 = if e >= 128 { u128::MAX } else { 1u128 << e };
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

#[cfg(test)]
fn isqrt_bits(n: U256) -> u128 {
    let mut res: u128 = 0;
    let mut bit: u32 = 128;
    while bit > 0 {
        bit -= 1;
        let c = res | (1u128 << bit);
        if U256::sqr_u128(c) <= n {
            res = c;
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_host_path_is_the_wide_one() {
        let mut x = 0xD1B5_4A32_D192_ED03u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let edges = [0u128, 1, 2, 999_999, 1_000_000, u64::MAX as u128, 1 << 64, u128::MAX, u128::MAX - 1];
        let mut t: Vec<(u128, u128, u128)> = vec![];
        for &a in &edges {
            for &b in &edges {
                for &d in &edges {
                    t.push((a, b, d));
                }
            }
        }
        for _ in 0..300_000 {
            let mut w = || (((next() as u128) << 64) | next() as u128) >> (next() % 128);
            t.push((w(), w(), w()));
        }
        for &(a, b, d) in &t {
            for r in [Round::Down, Round::Up] {
                assert_eq!(mul_div(a, b, d, r), mul_div_wide(a, b, d, r), "{a} {b} {d} {r:?}");
            }
        }
    }

    #[test]
    fn shifts_round_trip_and_round() {
        let x = w3(U256 { hi: 3, lo: 5 });
        for k in [0u32, 1, 63, 64, 72, 127, 128, 129, 200] {
            let up = shl_w(x, k).unwrap();
            assert_eq!(shr_w(up, k, Round::Down), x);
            assert_eq!(shr_w(up, k, Round::Up), x);
        }
        assert_eq!(shr_w([5, 0, 0], 1, Round::Down), [2, 0, 0]);
        assert_eq!(shr_w([5, 0, 0], 1, Round::Up), [3, 0, 0]);
        assert_eq!(shr_w([1, 0, 0], 200, Round::Up), [1, 0, 0]);
        assert_eq!(shr_w([0, 1, 0], 129, Round::Down), [0, 0, 0]);
        assert!(shl_w([0, 0, 1u128 << 127], 1).is_none());
    }

    #[test]
    fn mul_div_rounds() {
        assert_eq!(mul_div(7, 3, 2, Round::Down), Some(10));
        assert_eq!(mul_div(7, 3, 2, Round::Up), Some(11));
        assert_eq!(mul_shr(3, 1 << 72, 72, Round::Down), Some(3));
        assert_eq!(mul_shr(3, (1 << 72) + 1, 72, Round::Up), Some(4));
        assert_eq!(isqrt(U256::from_u128(99)), 9);
        assert_eq!(isqrt(U256::sqr_u128(u128::MAX)), u128::MAX);
    }

    #[test]
    fn newton_isqrt_is_the_bit_loop() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut cases: Vec<U256> = vec![U256::ZERO, U256::from_u128(1), U256::from_u128(2), U256::from_u128(3), U256::from_u128(4)];
        for k in [1u128, 2, 3, 1 << 64, u64::MAX as u128, u128::MAX >> 1, u128::MAX - 1, u128::MAX] {
            let sq = U256::sqr_u128(k);
            cases.push(sq);
            cases.push(sq.checked_sub(U256::from_u128(1)).unwrap());
            if let Some(p) = sq.checked_add(U256::from_u128(1)) {
                cases.push(p);
            }
        }
        cases.push(U256 { hi: u128::MAX, lo: u128::MAX });
        for _ in 0..200_000 {
            let (a, b, c, d) = (next() as u128, next() as u128, next() as u128, next() as u128);
            let w = U256 { hi: (a << 64 | b) >> (next() % 128), lo: c << 64 | d };
            cases.push(w);
            cases.push(U256 { hi: 0, lo: w.lo >> (next() % 128) });
        }
        for n in cases {
            assert_eq!(isqrt(n), isqrt_bits(n), "{n:?}");
        }
    }

    #[test]
    fn wide_division_matches_narrow_and_rounds() {
        let mut s = 0x1234_5678_9abc_def1u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s as u128 | ((s.rotate_left(29) as u128) << 64)
        };
        for _ in 0..2_000 {
            let n = [next(), next(), next() >> 70];
            let dlo = next() | 1;
            for r in [Round::Down, Round::Up] {
                assert_eq!(div_w_wide(n, U256::from_u128(dlo), r), div_w(n, dlo, r));
            }
            let d = U256 { hi: next() >> 64 | 1, lo: next() };
            let qv = next() >> 1;
            let prod = mul_w(d, qv);
            assert_eq!(div_w_wide(prod, d, Round::Down), Some(U256::from_u128(qv)));
            assert_eq!(div_w_wide(prod, d, Round::Up), Some(U256::from_u128(qv)));
            let plus = [prod[0].wrapping_add(1), prod[1] + (prod[0] == u128::MAX) as u128, prod[2]];
            assert_eq!(div_w_wide(plus, d, Round::Down), Some(U256::from_u128(qv)));
            assert_eq!(div_w_wide(plus, d, Round::Up), Some(U256::from_u128(qv + 1)));
        }
    }
}
