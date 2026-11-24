// SPDX-License-Identifier: Apache-2.0
use core::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U256 {
    pub hi: u128,
    pub lo: u128,
}

impl U256 {
    pub const ZERO: U256 = U256 { hi: 0, lo: 0 };

    pub const fn from_u128(v: u128) -> U256 {
        U256 { hi: 0, lo: v }
    }

    #[inline]
    pub fn mul_u128(a: u128, b: u128) -> U256 {
        #[cfg(not(target_os = "solana"))]
        return U256::mul_u128_host(a, b);
        #[cfg(target_os = "solana")]
        return U256::mul_u128_sbf(a, b);
    }

    #[inline(always)]
    pub fn mul_u128_host(a: u128, b: u128) -> U256 {
        const MASK: u128 = u64::MAX as u128;
        if (a | b) >> 64 == 0 {
            return U256 { hi: 0, lo: a * b };
        }
        let (a0, a1) = (a & MASK, a >> 64);
        let (b0, b1) = (b & MASK, b >> 64);
        let p00 = a0 * b0;
        let p01 = a0 * b1;
        let p10 = a1 * b0;
        let p11 = a1 * b1;
        let (mid, c1) = p01.overflowing_add(p10);
        let (mid, c2) = mid.overflowing_add(p00 >> 64);
        U256 { hi: p11 + (mid >> 64) + (((c1 as u128) + (c2 as u128)) << 64), lo: (p00 & MASK) | (mid << 64) }
    }

    #[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
    #[inline]
    pub fn mul_u128_sbf(a: u128, b: u128) -> U256 {
        if (a | b) >> 64 == 0 {
            let (h, l) = mul_64(a as u64, b as u64);
            return U256 { hi: 0, lo: ((h as u128) << 64) | l as u128 };
        }
        let x = limbs32(a);
        let y = limbs32(b);
        let mut c = [0u64; 9];
        let mut i = 0;
        while i < 4 {
            let mut j = 0;
            while j < 4 {
                let p = x[i] * y[j];
                c[i + j] += p & M32;
                c[i + j + 1] += p >> 32;
                j += 1;
            }
            i += 1;
        }
        from_columns([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
    }

    #[inline(always)]
    pub fn sqr_u128(a: u128) -> U256 {
        #[cfg(not(target_os = "solana"))]
        return U256::mul_u128_host(a, a);
        #[cfg(target_os = "solana")]
        return U256::sqr_u128_sbf(a);
    }

    #[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
    #[inline(always)]
    pub fn sqr_u128_sbf(a: u128) -> U256 {
        let [x0, x1, x2, x3] = limbs32(a);
        let (p00, p11, p22, p33) = (x0 * x0, x1 * x1, x2 * x2, x3 * x3);
        let d = |p: u64| ((p & M32) << 1, (p >> 32) << 1);
        let (l01, h01) = d(x0 * x1);
        let (l02, h02) = d(x0 * x2);
        let (l03, h03) = d(x0 * x3);
        let (l12, h12) = d(x1 * x2);
        let (l13, h13) = d(x1 * x3);
        let (l23, h23) = d(x2 * x3);
        let c0 = p00 & M32;
        let c1 = (p00 >> 32) + l01;
        let c2 = (p11 & M32) + h01 + l02 + (c1 >> 32);
        let c3 = (p11 >> 32) + h02 + l03 + l12 + (c2 >> 32);
        let c4 = (p22 & M32) + h03 + h12 + l13 + (c3 >> 32);
        let c5 = (p22 >> 32) + h13 + l23 + (c4 >> 32);
        let c6 = (p33 & M32) + h23 + (c5 >> 32);
        let c7 = (p33 >> 32) + (c6 >> 32);
        U256 {
            hi: ((((c7 << 32) | (c6 & M32)) as u128) << 64) | ((c5 << 32) | (c4 & M32)) as u128,
            lo: ((((c3 << 32) | (c2 & M32)) as u128) << 64) | ((c1 << 32) | c0) as u128,
        }
    }

    #[cfg(test)]
    pub fn mul_u128_mul64(a: u128, b: u128) -> U256 {
        let (a0, a1) = (a as u64, (a >> 64) as u64);
        let (b0, b1) = (b as u64, (b >> 64) as u64);
        let (h00, l00) = mul_64(a0, b0);
        let (h01, l01) = mul_64(a0, b1);
        let (h10, l10) = mul_64(a1, b0);
        let (h11, l11) = mul_64(a1, b1);
        let (m1, c1) = h00.overflowing_add(l01);
        let (m1, c2) = m1.overflowing_add(l10);
        let (m2, c3) = h01.overflowing_add(h10);
        let (m2, c4) = m2.overflowing_add(l11);
        let (m2, c5) = m2.overflowing_add(c1 as u64 + c2 as u64);
        let m3 = h11 + c3 as u64 + c4 as u64 + c5 as u64;
        U256 { hi: ((m3 as u128) << 64) | m2 as u128, lo: ((m1 as u128) << 64) | l00 as u128 }
    }

    #[cfg(test)]
    pub fn mul_u128_ref(a: u128, b: u128) -> U256 {
        const MASK: u128 = u64::MAX as u128;
        let (a0, a1) = (a & MASK, a >> 64);
        let (b0, b1) = (b & MASK, b >> 64);
        let p00 = a0 * b0;
        let p01 = a0 * b1;
        let p10 = a1 * b0;
        let p11 = a1 * b1;
        let (mid, c1) = p01.overflowing_add(p10);
        let (mid, c2) = mid.overflowing_add(p00 >> 64);
        let lo = (p00 & MASK) | (mid << 64);
        let carry = ((c1 as u128) + (c2 as u128)) << 64;
        let hi = p11 + (mid >> 64) + carry;
        U256 { hi, lo }
    }

    pub fn checked_mul_u64(self, m: u64) -> Option<U256> {
        let lo = U256::mul_u128(self.lo, m as u128);
        let hi = U256::mul_u128(self.hi, m as u128);
        if hi.hi != 0 {
            return None;
        }
        let (h, o) = lo.hi.overflowing_add(hi.lo);
        if o {
            return None;
        }
        Some(U256 { hi: h, lo: lo.lo })
    }

    pub fn checked_add(self, o: U256) -> Option<U256> {
        let (lo, c) = self.lo.overflowing_add(o.lo);
        let hi = self.hi.checked_add(o.hi)?.checked_add(c as u128)?;
        Some(U256 { hi, lo })
    }

    pub fn checked_sub(self, o: U256) -> Option<U256> {
        if self < o {
            return None;
        }
        let (lo, b) = self.lo.overflowing_sub(o.lo);
        let hi = self.hi - o.hi - (b as u128);
        Some(U256 { hi, lo })
    }

    pub fn checked_shl64(self) -> Option<U256> {
        if self.hi >> 64 != 0 {
            return None;
        }
        Some(U256 { hi: (self.hi << 64) | (self.lo >> 64), lo: self.lo << 64 })
    }

    pub fn is_zero(self) -> bool {
        self.hi == 0 && self.lo == 0
    }

    pub fn div_rem_u128(self, d: u128) -> (U256, u128) {
        assert!(d != 0, "division by zero");
        if self.hi == 0 && (self.lo | d) >> 64 == 0 {
            let (n, v) = (self.lo as u64, d as u64);
            return (U256::from_u128((n / v) as u128), (n % v) as u128);
        }
        if d == 1 << 64 {
            return (U256 { hi: self.hi >> 64, lo: (self.lo >> 64) | (self.hi << 64) }, self.lo & u64::MAX as u128);
        }
        if d & (d - 1) == 0 {
            let k = d.trailing_zeros();
            if k == 0 {
                return (self, 0);
            }
            return (U256 { hi: self.hi >> k, lo: (self.lo >> k) | (self.hi << (128 - k)) }, self.lo & (d - 1));
        }
        let u = [self.lo as u64, (self.lo >> 64) as u64, self.hi as u64, (self.hi >> 64) as u64];
        if d >> 64 == 0 {
            let s = (d as u64).leading_zeros();
            let v = (d as u64) << s;
            let w = |i: usize| -> u64 {
                let hi = if i == 4 { 0 } else { u[i] };
                if s == 0 {
                    hi
                } else {
                    let lo = if i == 0 { 0 } else { u[i - 1] };
                    (hi << s) | (lo >> (64 - s))
                }
            };
            let mut r = w(4);
            let mut q = [0u64; 4];
            let mut i = 4;
            while i > 0 {
                i -= 1;
                let n = w(i);
                if r == 0 && n < v {
                    r = n;
                    continue;
                }
                (q[i], r) = div_128_64(r, n, v);
            }
            return (U256 { hi: ((q[3] as u128) << 64) | q[2] as u128, lo: ((q[1] as u128) << 64) | q[0] as u128 }, (r >> s) as u128);
        }
        let s = ((d >> 64) as u64).leading_zeros();
        let v = d << s;
        let (v1, v0) = ((v >> 64) as u64, v as u64);
        let mut n = [0u64; 5];
        if s == 0 {
            n[..4].copy_from_slice(&u);
        } else {
            n[0] = u[0] << s;
            n[1] = (u[1] << s) | (u[0] >> (64 - s));
            n[2] = (u[2] << s) | (u[1] >> (64 - s));
            n[3] = (u[3] << s) | (u[2] >> (64 - s));
            n[4] = u[3] >> (64 - s);
        }
        let mut q = [0u64; 3];
        let mut j = 3;
        while j > 0 {
            j -= 1;
            let (n2, n1, n0) = (n[j + 2], n[j + 1], n[j]);
            if n2 == 0 && n1 < v1 {
                continue;
            }
            let (mut qh, mut rh, mut rh_wide) = if n2 >= v1 {
                let (r, o) = n1.overflowing_add(v1);
                (u64::MAX, r, o)
            } else {
                let (qq, rr) = div_128_64(n2, n1, v1);
                (qq, rr, false)
            };
            while !rh_wide {
                let (ph, pl) = mul_64(qh, v0);
                if ph > rh || (ph == rh && pl > n0) {
                    qh -= 1;
                    (rh, rh_wide) = rh.overflowing_add(v1);
                } else {
                    break;
                }
            }
            let (a_h, a_l) = mul_64(qh, v0);
            let (b_h, b_l) = mul_64(qh, v1);
            let (p1, c1) = a_h.overflowing_add(b_l);
            let p2 = b_h.wrapping_add(c1 as u64);
            let (r0, br0) = n0.overflowing_sub(a_l);
            let (r1, br1a) = n1.overflowing_sub(p1);
            let (r1, br1b) = r1.overflowing_sub(br0 as u64);
            let (r2, br2a) = n2.overflowing_sub(p2);
            let (r2, br2b) = r2.overflowing_sub((br1a | br1b) as u64);
            let (mut r0, mut r1, mut r2) = (r0, r1, r2);
            if br2a | br2b {
                qh -= 1;
                let (s0, c0) = r0.overflowing_add(v0);
                let (s1, c1a) = r1.overflowing_add(v1);
                let (s1, c1b) = s1.overflowing_add(c0 as u64);
                r0 = s0;
                r1 = s1;
                r2 = r2.wrapping_add((c1a | c1b) as u64);
            }
            n[j] = r0;
            n[j + 1] = r1;
            n[j + 2] = r2;
            q[j] = qh;
        }
        let r = (((n[1] as u128) << 64) | n[0] as u128) >> s;
        (U256 { hi: q[2] as u128, lo: ((q[1] as u128) << 64) | q[0] as u128 }, r)
    }

    #[cfg(test)]
    pub fn div_rem_knuth_u128div(self, d: u128) -> (U256, u128) {
        assert!(d != 0, "division by zero");
        if self.hi == 0 {
            return (U256::from_u128(self.lo / d), self.lo % d);
        }
        if d & (d - 1) == 0 {
            let k = d.trailing_zeros();
            if k == 0 {
                return (self, 0);
            }
            return (U256 { hi: self.hi >> k, lo: (self.lo >> k) | (self.hi << (128 - k)) }, self.lo & (d - 1));
        }
        const M: u128 = u64::MAX as u128;
        let u = [self.lo & M, self.lo >> 64, self.hi & M, self.hi >> 64];
        if d <= M {
            let mut q = [0u128; 4];
            let mut r = 0u128;
            for i in (0..4).rev() {
                let cur = (r << 64) | u[i];
                q[i] = cur / d;
                r = cur % d;
            }
            return (U256 { hi: (q[3] << 64) | q[2], lo: (q[1] << 64) | q[0] }, r);
        }
        let sh = d.leading_zeros();
        let v = d << sh;
        let (v1, v0) = (v >> 64, v & M);
        let n = if sh == 0 { [u[0], u[1], u[2], u[3], 0] } else {
            let w = |i: usize| -> u128 { if i == 0 { u[0] << sh & M } else { ((u[i] << sh) | (u[i - 1] >> (64 - sh))) & M } };
            [w(0), w(1), w(2), w(3), u[3] >> (64 - sh)]
        };
        let mut n = n;
        let mut q = [0u128; 3];
        for j in (0..3).rev() {
            let num = (n[j + 2] << 64) | n[j + 1];
            let mut qh = num / v1;
            let mut rh = num % v1;
            while qh > M || qh * v0 > ((rh << 64) | n[j]) {
                qh -= 1;
                rh += v1;
                if rh > M {
                    break;
                }
            }
            let w = U256 { hi: n[j + 2], lo: (n[j + 1] << 64) | n[j] };
            let mut p = U256::mul_u128(qh, v);
            while p > w {
                qh -= 1;
                p = p.checked_sub(U256::from_u128(v)).expect("qh ≥ 1 here");
            }
            let w = w.checked_sub(p).expect("p ≤ w");
            n[j + 2] = w.hi;
            n[j + 1] = w.lo >> 64;
            n[j] = w.lo & M;
            q[j] = qh;
        }
        let r = ((n[1] << 64) | n[0]) >> sh;
        (U256 { hi: q[2], lo: (q[1] << 64) | q[0] }, r)
    }

    #[cfg(test)]
    pub fn div_rem_slow(self, d: u128) -> (U256, u128) {
        let mut q = U256::ZERO;
        let mut r: u128 = 0;
        for i in (0..256).rev() {
            let bit = if i >= 128 { (self.hi >> (i - 128)) & 1 } else { (self.lo >> i) & 1 };
            let carry = r >> 127;
            r = (r << 1) | bit;
            if carry == 1 || r >= d {
                r = r.wrapping_sub(d);
                if i >= 128 {
                    q.hi |= 1 << (i - 128);
                } else {
                    q.lo |= 1 << i;
                }
            }
        }
        (q, r)
    }

    pub fn div_ceil_u128(self, d: u128) -> U256 {
        let (q, r) = self.div_rem_u128(d);
        if r == 0 {
            q
        } else {
            q.checked_add(U256::from_u128(1)).expect("ceil of a quotient below 2^256 fits")
        }
    }

    pub fn to_u128(self) -> Option<u128> {
        if self.hi == 0 {
            Some(self.lo)
        } else {
            None
        }
    }
}

const M32: u64 = u32::MAX as u64;

#[inline(always)]
fn limbs32(a: u128) -> [u64; 4] {
    [(a as u64) & M32, (a as u64) >> 32, ((a >> 64) as u64) & M32, (a >> 96) as u64]
}

#[inline(always)]
fn from_columns(mut c: [u64; 8]) -> U256 {
    let mut k = 0;
    while k < 7 {
        c[k + 1] += c[k] >> 32;
        c[k] &= M32;
        k += 1;
    }
    U256 { hi: ((((c[7] << 32) | c[6]) as u128) << 64) | ((c[5] << 32) | c[4]) as u128, lo: ((((c[3] << 32) | c[2]) as u128) << 64) | ((c[1] << 32) | c[0]) as u128 }
}

#[inline]
pub fn div_u128_by_u64(n: u128, d: u64) -> (u128, u64) {
    assert!(d != 0, "division by zero");
    let s = d.leading_zeros();
    let v = d << s;
    let (hi, lo) = ((n >> 64) as u64, n as u64);
    let (n2, n1, n0) = if s == 0 { (0, hi, lo) } else { (hi >> (64 - s), (hi << s) | (lo >> (64 - s)), lo << s) };
    let (q1, r1) = div_128_64(n2, n1, v);
    let (q0, r0) = div_128_64(r1, n0, v);
    ((((q1 as u128) << 64) | q0 as u128), r0 >> s)
}

#[inline(always)]
pub fn div_128_64(u1: u64, u0: u64, v: u64) -> (u64, u64) {
    #[cfg(not(target_os = "solana"))]
    {
        let n = ((u1 as u128) << 64) | u0 as u128;
        let q = (n / v as u128) as u64;
        return (q, u0.wrapping_sub(q.wrapping_mul(v)));
    }
    #[cfg(target_os = "solana")]
    return div_128_64_sbf(u1, u0, v);
}

#[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
#[inline(always)]
pub fn div_128_64_sbf(u1: u64, u0: u64, v: u64) -> (u64, u64) {
    const B: u64 = 1 << 32;
    let (vn1, vn0) = (v >> 32, v & M32);
    let (un1, un0) = (u0 >> 32, u0 & M32);
    let mut q1 = u1 / vn1;
    let mut rhat = u1 - q1.wrapping_mul(vn1);
    while q1 >= B || q1.wrapping_mul(vn0) > (rhat << 32) | un1 {
        q1 -= 1;
        rhat += vn1;
        if rhat >= B {
            break;
        }
    }
    let un21 = (u1 << 32).wrapping_add(un1).wrapping_sub(q1.wrapping_mul(v));
    let mut q0 = un21 / vn1;
    let mut rhat = un21 - q0.wrapping_mul(vn1);
    while q0 >= B || q0.wrapping_mul(vn0) > (rhat << 32) | un0 {
        q0 -= 1;
        rhat += vn1;
        if rhat >= B {
            break;
        }
    }
    let r = (un21 << 32).wrapping_add(un0).wrapping_sub(q0.wrapping_mul(v));
    ((q1 << 32) + q0, r)
}

#[inline(always)]
pub fn mul_64(a: u64, b: u64) -> (u64, u64) {
    #[cfg(not(target_os = "solana"))]
    {
        let p = a as u128 * b as u128;
        return ((p >> 64) as u64, p as u64);
    }
    #[cfg(target_os = "solana")]
    return mul_64_sbf(a, b);
}

#[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
#[inline(always)]
pub fn mul_64_sbf(a: u64, b: u64) -> (u64, u64) {
    const M: u64 = u32::MAX as u64;
    let (a0, a1) = (a & M, a >> 32);
    let (b0, b1) = (b & M, b >> 32);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 32) + (p01 & M) + (p10 & M);
    ((p11 + (p01 >> 32) + (p10 >> 32) + (mid >> 32)), (p00 & M) | (mid << 32))
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, o: &U256) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for U256 {
    fn cmp(&self, o: &U256) -> Ordering {
        self.hi.cmp(&o.hi).then(self.lo.cmp(&o.lo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_paths_equal_the_sbf_bodies() {
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let edges = [0u128, 1, 2, u64::MAX as u128, 1 << 64, (1 << 64) + 1, u128::MAX, u128::MAX - 1, 1 << 127];
        let mut pairs: Vec<(u128, u128)> = edges.iter().flat_map(|&a| edges.iter().map(move |&b| (a, b))).collect();
        for _ in 0..200_000 {
            let a = (((next() as u128) << 64) | next() as u128) >> (next() % 128);
            let b = (((next() as u128) << 64) | next() as u128) >> (next() % 128);
            pairs.push((a, b));
        }
        for &(a, b) in &pairs {
            assert_eq!(U256::mul_u128_host(a, b), U256::mul_u128_sbf(a, b), "mul {a} {b}");
            assert_eq!(U256::mul_u128(a, b), U256::mul_u128_ref(a, b), "mul ref {a} {b}");
            assert_eq!(U256::sqr_u128(a), U256::sqr_u128_sbf(a), "sqr {a}");
            let (a64, b64) = (a as u64, b as u64);
            assert_eq!(mul_64(a64, b64), mul_64_sbf(a64, b64), "mul_64 {a64} {b64}");
            let v = (b as u64) | (1 << 63);
            let u1 = ((a >> 64) as u64) % v;
            assert_eq!(div_128_64(u1, a as u64, v), div_128_64_sbf(u1, a as u64, v), "div {u1} {a} {v}");
        }
    }

    #[test]
    fn mul_matches_u128_when_small() {
        for (a, b) in [(0u128, 5u128), (7, 9), (u64::MAX as u128, u64::MAX as u128), (1 << 100, 1 << 20)] {
            let p = U256::mul_u128(a, b);
            assert_eq!(p.to_u128(), a.checked_mul(b));
        }
    }

    #[test]
    fn mul_matches_the_reference() {
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let edge = [0u128, 1, 2, u64::MAX as u128, 1 << 64, (1 << 64) + 1, u128::MAX, u128::MAX - 1, 1 << 127, (1 << 127) - 1];
        for &a in &edge {
            for &b in &edge {
                assert_eq!(U256::mul_u128(a, b), U256::mul_u128_ref(a, b), "{a:#x} × {b:#x}");
            }
            assert_eq!(U256::sqr_u128(a), U256::mul_u128_ref(a, a), "{a:#x}²");
        }
        for k in 0..400_000u32 {
            let w = |h: u64, l: u64, bits: u32| -> u128 { (((h as u128) << 64) | l as u128) >> (128 - bits) };
            let (ab, bb) = (1 + k % 128, 1 + (k / 128) % 128);
            let a = w(next(), next(), ab);
            let b = w(next(), next(), bb);
            assert_eq!(U256::mul_u128(a, b), U256::mul_u128_ref(a, b), "{a:#x} × {b:#x}");
            assert_eq!(U256::sqr_u128(a), U256::mul_u128_ref(a, a), "{a:#x}²");
            assert_eq!(U256::mul_u128(a, b), U256::mul_u128_mul64(a, b), "{a:#x} × {b:#x} (mul_64 body)");
            let (h, l) = mul_64(a as u64, b as u64);
            assert_eq!(((h as u128) << 64) | l as u128, (a as u64 as u128) * (b as u64 as u128));
        }
    }

    #[test]
    fn mul_max() {
        let p = U256::mul_u128(u128::MAX, u128::MAX);
        assert_eq!(p.lo, 1);
        assert_eq!(p.hi, u128::MAX - 1);
    }

    #[test]
    fn div_inverts_mul() {
        let cases = [
            (u128::MAX, u128::MAX),
            (u128::MAX, 3),
            (1u128 << 127, (1u128 << 127) + 12345),
            (0xdead_beef_u128 << 90, 0x1234_5678_9abc_u128),
        ];
        for (a, b) in cases {
            let p = U256::mul_u128(a, b);
            let (q, r) = p.div_rem_u128(b);
            assert_eq!((q.to_u128(), r), (Some(a), 0));
            let p1 = p.checked_add(U256::from_u128(1)).unwrap();
            let (q1, r1) = p1.div_rem_u128(b);
            if b > 1 {
                assert_eq!((q1.to_u128(), r1), (Some(a), 1));
            }
            assert_eq!(p1.div_ceil_u128(b).to_u128(), a.checked_add(1));
        }
    }

    #[test]
    fn div_matches_the_reference() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        for k in 0..200_000u32 {
            let w = |a: u64, b: u64, bits: u32| -> u128 { (((a as u128) << 64) | b as u128) >> (128 - bits.max(1)) };
            let hb = next() as u32 % 129;
            let hi = if hb == 0 { 0 } else { w(next(), next(), hb) };
            let lo = w(next(), next(), 128);
            let db = 1 + next() as u32 % 128;
            let mut d = w(next(), next(), db).max(1);
            if k % 7 == 0 {
                d = 1u128 << (next() % 128);
            }
            if k % 11 == 0 {
                d = u128::MAX - (next() as u128 & 0xff);
            }
            let a = U256 { hi, lo };
            assert_eq!(a.div_rem_u128(d), a.div_rem_slow(d), "{hi:#x} {lo:#x} / {d:#x}");
            assert_eq!(a.div_rem_u128(d), a.div_rem_knuth_u128div(d), "{hi:#x} {lo:#x} / {d:#x} (the u128 body)");
        }
        let edge = [U256 { hi: u128::MAX, lo: u128::MAX }, U256 { hi: 1, lo: 0 }, U256 { hi: u128::MAX, lo: 0 }];
        for a in edge {
            for d in [1u128, 2, 3, u64::MAX as u128, 1 << 64, (1 << 64) + 1, u128::MAX, u128::MAX - 1, 1 << 127, (1 << 127) + 1] {
                assert_eq!(a.div_rem_u128(d), a.div_rem_slow(d));
            }
        }
    }

    #[test]
    fn div_knuth_edges() {
        let mut x = 0x1234_5678_9abc_def1u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let digits = |n: &mut dyn FnMut() -> u64| -> u64 {
            match n() % 6 {
                0 => u64::MAX,
                1 => u64::MAX - (n() & 3),
                2 => 0,
                3 => 1 << 63,
                4 => (1 << 63) - 1,
                _ => n(),
            }
        };
        for _ in 0..400_000 {
            let d = (((digits(&mut next) | 1) as u128) << 64 | digits(&mut next) as u128) >> (next() % 64);
            let d = d.max((1u128 << 64) + 1);
            let qd = [digits(&mut next), digits(&mut next), next() % 4];
            let q = U256 { hi: qd[2] as u128, lo: ((qd[1] as u128) << 64) | qd[0] as u128 };
            let r = ((digits(&mut next) as u128) << 64 | digits(&mut next) as u128) % d;
            let lo = U256::mul_u128(q.lo, d);
            let hi = U256::mul_u128(q.hi, d);
            if hi.hi != 0 {
                continue;
            }
            let Some(n) = U256 { hi: lo.hi.checked_add(hi.lo).unwrap_or(u128::MAX), lo: lo.lo }.checked_add(U256::from_u128(r)) else { continue };
            if lo.hi.checked_add(hi.lo).is_none() {
                continue;
            }
            assert_eq!(n.div_rem_u128(d), (q, r), "{n:?} / {d:#x}");
            assert_eq!(n.div_rem_u128(d), n.div_rem_slow(d), "{n:?} / {d:#x}");
        }
        for _ in 0..1_000_000 {
            let n = ((digits(&mut next) as u128) << 64 | digits(&mut next) as u128) >> (next() % 128);
            let d = (digits(&mut next) >> (next() % 64)).max(1);
            let (q, r) = div_u128_by_u64(n, d);
            assert_eq!((q, r as u128), (n / d as u128, n % d as u128), "{n:#x} / {d:#x}");
            let v = digits(&mut next) | (1 << 63);
            let u1 = digits(&mut next) % v;
            let u0 = digits(&mut next);
            let n = ((u1 as u128) << 64) | u0 as u128;
            let (q, r) = div_128_64(u1, u0, v);
            assert_eq!(((q as u128), (r as u128)), (n / v as u128, n % v as u128), "{u1:#x} {u0:#x} / {v:#x}");
        }
    }

    #[test]
    fn sub_and_order() {
        let a = U256::mul_u128(u128::MAX, 2);
        let b = U256::from_u128(u128::MAX);
        assert!(a > b);
        assert_eq!(a.checked_sub(b), Some(b));
        assert_eq!(b.checked_sub(a), None);
    }
}
