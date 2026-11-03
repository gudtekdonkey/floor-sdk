// SPDX-License-Identifier: Apache-2.0
use crate::u256::U256;

const K: [u128; 64] = [
    0xb504f333f9de6484597d89b3754abe9f,
    0x9837f0518db8a96f46ad23182e42f6f6,
    0x8b95c1e3ea8bd6e6fbe4628758a53c90,
    0x85aac367cc487b14c5c95b8c2154c1b2,
    0x82cd8698ac2ba1d73e2a475b46520bff,
    0x8164d1f3bc0307737be56527bd14def4,
    0x80b1ed4fd999ab6c25335719b6e6fd20,
    0x8058d7d2d5e5f6b094d589f608ee4aa2,
    0x802c6436d0e04f50ff8ce94a6797b3ce,
    0x8016302f174676283690dfe44d11d008,
    0x800b179c82028fd0945e54e2ae18f2f0,
    0x80058baf7fee3b5d1c718b38e549cb93,
    0x8002c5d00fdcfcb6b6566a58c048be1f,
    0x800162e61bed4a48e84c2e1a463473d9,
    0x8000b17292f702a3aa22beacca949013,
    0x800058b92abbae02030c5fa5256f41fe,
    0x80002c5c8dade4d71776c0f4dbea67d6,
    0x8000162e44eaf636526be456600bdbe4,
    0x80000b1721fa7c188307016c1cd4e8b6,
    0x8000058b90de7e4cecfc487503488bb1,
    0x800002c5c8678f36cbfce50a6de60b14,
    0x80000162e431db9f80b2347b5d62e516,
    0x800000b1721872d0c7b08cf1e0114152,
    0x80000058b90c1aa8a5c3736cb77e8dff,
    0x8000002c5c8605a4635f2efc2362d978,
    0x800000162e4300e635cf4a109e3939bd,
    0x8000000b17217ff81bef9c551590cf83,
    0x800000058b90bfdd4e39cd52c0cfa27c,
    0x80000002c5c85fe6f72d669e0e76e411,
    0x8000000162e42ff18f9ad35186d0df28,
    0x80000000b17217f84cce71aa0dcfffe7,
    0x8000000058b90bfc07a77ad56ed22aaa,
    0x800000002c5c85fdfc23cdead40da8d6,
    0x80000000162e42fefc25eb1571853a66,
    0x800000000b17217f7d97f692baacded5,
    0x80000000058b90bfbead3b8b5dd254d7,
    0x8000000002c5c85fdf4eedd62f084e67,
    0x800000000162e42fefa58aef378bf586,
    0x8000000000b17217f7d24a78a3c7ef02,
    0x800000000058b90bfbe9067c93e474a6,
    0x80000000002c5c85fdf47b8e5a72599f,
    0x8000000000162e42fefa3bdb315934a2,
    0x80000000000b17217f7d1d7299b49c46,
    0x8000000000058b90bfbe8e9a8d1c4ea0,
    0x800000000002c5c85fdf4745969ea76f,
    0x80000000000162e42fefa3a0df5373bf,
    0x800000000000b17217f7d1cff4aac1e1,
    0x80000000000058b90bfbe8e7db95a2f1,
    0x8000000000002c5c85fdf473e61ae1f8,
    0x800000000000162e42fefa39f121751c,
    0x8000000000000b17217f7d1cf815bb96,
    0x800000000000058b90bfbe8e7bec1e0d,
    0x80000000000002c5c85fdf473dee5f17,
    0x8000000000000162e42fefa39ef5438f,
    0x80000000000000b17217f7d1cf7a26c8,
    0x8000000000000058b90bfbe8e7bcf4a4,
    0x800000000000002c5c85fdf473de72a2,
    0x80000000000000162e42fefa39ef3765,
    0x800000000000000b17217f7d1cf79b37,
    0x80000000000000058b90bfbe8e7bcd7d,
    0x8000000000000002c5c85fdf473de6b6,
    0x800000000000000162e42fefa39ef359,
    0x8000000000000000b17217f7d1cf79ac,
    0x800000000000000058b90bfbe8e7bcd6,
];

const ONE_127: u128 = 1 << 127;

#[cfg(test)]
fn mul_q127(a: u128, b: u128) -> u128 {
    let p = U256::mul_u128(a, b);
    (p.hi << 1) | (p.lo >> 127)
}

static KD: [[u64; 4]; 64] = {
    let mut d = [[0u64; 4]; 64];
    let mut i = 0;
    while i < 64 {
        let v = K[i] - ONE_127;
        d[i] = [(v as u64) & 0xffff_ffff, (v as u64) >> 32, ((v >> 64) as u64) & 0xffff_ffff, (v >> 96) as u64];
        i += 1;
    }
    d
};

#[inline(always)]
fn mul_k(r: u128, i: usize) -> u128 {
    if i < 31 {
        mul_kn::<4>(r, &KD[i])
    } else if i < 63 {
        mul_kn::<3>(r, &KD[i])
    } else {
        mul_kn::<2>(r, &KD[i])
    }
}

#[inline(always)]
fn mul_kn<const N: usize>(r: u128, y: &[u64; 4]) -> u128 {
    const M: u64 = 0xffff_ffff;
    let x = [(r as u64) & M, (r as u64) >> 32, ((r >> 64) as u64) & M, (r >> 96) as u64];
    let mut c = [0u64; 9];
    let mut a = 0;
    while a < 4 {
        let mut b = 0;
        while b < N {
            let p = x[a] * (y[b] & M);
            c[a + b] += p & M;
            c[a + b + 1] += p >> 32;
            b += 1;
        }
        a += 1;
    }
    let mut k = 0;
    while k < 7 {
        c[k + 1] += c[k] >> 32;
        c[k] &= M;
        k += 1;
    }
    let hi = ((((c[7] << 32) | c[6]) as u128) << 64) | ((c[5] << 32) | c[4]) as u128;
    r + ((hi << 1) | (c[3] >> 31) as u128)
}

pub fn log2_x64(n: u128) -> Option<u128> {
    #[cfg(not(target_os = "solana"))]
    return log2_x64_host(n);
    #[cfg(target_os = "solana")]
    return log2_x64_sbf(n);
}

#[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
pub fn log2_x64_sbf(n: u128) -> Option<u128> {
    #[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
    OPS.with(|c| c.set(c.get() + 1));
    #[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
    ARGS.with(|a| a.borrow_mut().push((0, n)));
    if n == 0 {
        return None;
    }
    match log2_staged::<NEAR_TWO>(n) {
        Some(v) => Some(v),
        None => log2_exact(n),
    }
}

#[cfg(not(target_os = "solana"))]
#[cfg_attr(not(any(test, feature = "op-count")), allow(dead_code))]
pub fn log2_x64_host(n: u128) -> Option<u128> {
    #[cfg(any(test, feature = "op-count"))]
    OPS.with(|c| c.set(c.get() + 1));
    #[cfg(any(test, feature = "op-count"))]
    ARGS.with(|a| a.borrow_mut().push((0, n)));
    if n == 0 {
        return None;
    }
    match log2_staged_host(n) {
        Some(v) => Some(v),
        None => log2_exact(n),
    }
}

#[cfg(not(target_os = "solana"))]
#[inline(always)]
fn log2_staged_host(n: u128) -> Option<u128> {
    let msb = 127 - n.leading_zeros();
    let mut m: u128 = n << (127 - msb);
    let mut frac: u64 = 0;
    for _ in 0..EXACT_STEPS {
        let p = U256::sqr_u128(m);
        let one = (p.hi >> 127) as u64;
        frac = (frac << 1) | one;
        m = if one == 1 { p.hi } else { (p.hi << 1) | (p.lo >> 127) };
    }
    let mut x: u128 = m >> 32;
    const LOW48: u128 = (1u128 << 48) - 1;
    for _ in 0..MID_STEPS {
        let (a, b) = (x >> 48, x & LOW48);
        let sq = (a * a << 1) + ((a * b) >> 46);
        let one = (sq >> 96) as u64;
        frac = (frac << 1) | one;
        if one == 1 {
            x = sq >> 1;
        } else {
            if (sq >> 65) as u64 >= NEAR_TWO {
                return None;
            }
            x = sq;
        }
    }
    let mut y = (x >> 32) as u64;
    for _ in EXACT_STEPS + MID_STEPS..64 {
        let p = (y as u128) * (y as u128);
        let one = (p >> 127) as u64;
        frac = (frac << 1) | one;
        if one == 1 {
            y = (p >> 64) as u64;
        } else {
            if (p >> 96) as u64 >= NEAR_TWO {
                return None;
            }
            y = (p >> 63) as u64;
        }
    }
    Some(((msb as u128) << 64) | frac as u128)
}

pub const EXACT_STEPS: u32 = 24;
pub const MID_STEPS: u32 = 20;
const NEAR_TWO: u64 = 0x7fff_ffff;
const M32: u64 = 0xffff_ffff;

#[inline(always)]
fn log2_staged<const NEAR: u64>(n: u128) -> Option<u128> {
    let msb = 127 - n.leading_zeros();
    let mut m: u128 = n << (127 - msb);
    let mut frac: u64 = 0;
    for _ in 0..EXACT_STEPS {
        let p = U256::sqr_u128(m);
        let one = (p.hi >> 127) as u64;
        frac = (frac << 1) | one;
        m = if one == 1 { p.hi } else { (p.hi << 1) | (p.lo >> 127) };
    }
    let (mut hi, mut lo) = ((m >> 64) as u64, ((m >> 32) as u64) & M32);
    for _ in 0..MID_STEPS {
        let (x2, x1, x0) = (hi >> 32, hi & M32, lo);
        let (s1, s2, p02, p12) = (x1.wrapping_mul(x1), x2.wrapping_mul(x2), x0.wrapping_mul(x2), x1.wrapping_mul(x2));
        let c2 = (s1 & M32).wrapping_add((p02 & M32) << 1);
        let c3 = (s1 >> 32).wrapping_add((p02 >> 32) << 1).wrapping_add((p12 & M32) << 1).wrapping_add(c2 >> 32);
        let c4 = (s2 & M32).wrapping_add((p12 >> 32) << 1).wrapping_add(c3 >> 32);
        let c5 = (s2 >> 32).wrapping_add(c4 >> 32);
        let one = c5 >> 31;
        frac = (frac << 1) | one;
        if one == 1 {
            hi = (c5 << 32) | (c4 & M32);
            lo = c3 & M32;
        } else {
            if c5 >= NEAR {
                return None;
            }
            hi = (c5 << 33) | ((c4 & M32) << 1) | ((c3 >> 31) & 1);
            lo = ((c3 << 1) & M32) | ((c2 >> 31) & 1);
        }
    }
    let mut x = hi;
    for _ in EXACT_STEPS + MID_STEPS..64 {
        let (x1, x0) = (x >> 32, x & M32);
        let (s1, p01) = (x1.wrapping_mul(x1), x0.wrapping_mul(x1));
        let c1 = (p01 & M32) << 1;
        let c2 = (s1 & M32).wrapping_add((p01 >> 32) << 1).wrapping_add(c1 >> 32);
        let c3 = (s1 >> 32).wrapping_add(c2 >> 32);
        let one = c3 >> 31;
        frac = (frac << 1) | one;
        if one == 1 {
            x = (c3 << 32) | (c2 & M32);
        } else {
            if c3 >= NEAR {
                return None;
            }
            x = (c3 << 33) | ((c2 & M32) << 1) | ((c1 >> 31) & 1);
        }
    }
    Some(((msb as u128) << 64) | frac as u128)
}

#[cold]
#[inline(never)]
fn log2_exact(n: u128) -> Option<u128> {
    if n == 0 {
        return None;
    }
    let msb = 127 - n.leading_zeros();
    let mut m: u128 = n << (127 - msb);
    let mut frac: u64 = 0;
    for _ in 0..64 {
        let p = U256::sqr_u128(m);
        let one = (p.hi >> 127) as u64;
        frac = (frac << 1) | one;
        m = if one == 1 { p.hi } else { (p.hi << 1) | (p.lo >> 127) };
    }
    Some(((msb as u128) << 64) | frac as u128)
}

pub fn exp2_x64(y: u128) -> Option<u128> {
    #[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
    OPS.with(|c| c.set(c.get() + 1));
    #[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
    ARGS.with(|a| a.borrow_mut().push((1, y)));
    let int = y >> 64;
    if int >= 64 {
        return None;
    }
    let mut r = ONE_127;
    let mut f = y as u64;
    let mut i = 0;
    while i < 64 {
        if f == 0 {
            break;
        }
        if f >> 63 == 1 {
            r = mul_k(r, i);
        }
        f <<= 1;
        i += 1;
    }
    Some(r >> (63 - int as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log2_ref(n: u128) -> Option<u128> {
        if n == 0 {
            return None;
        }
        let msb = 127 - n.leading_zeros();
        let mut m: u128 = n << (127 - msb);
        let mut frac: u128 = 0;
        for i in 1..=64u32 {
            let p = U256::mul_u128_ref(m, m);
            if p.hi >> 127 == 1 {
                frac |= 1u128 << (64 - i);
                m = p.hi;
            } else {
                m = (p.hi << 1) | (p.lo >> 127);
            }
        }
        Some(((msb as u128) << 64) | frac)
    }

    fn exp2_ref(y: u128) -> Option<u128> {
        let int = y >> 64;
        if int >= 64 {
            return None;
        }
        let mut r = ONE_127;
        for i in 1..=64u32 {
            if (y >> (64 - i)) & 1 == 1 {
                let p = U256::mul_u128_ref(r, K[(i - 1) as usize]);
                r = (p.hi << 1) | (p.lo >> 127);
            }
        }
        Some(r >> (63 - int as u32))
    }

    #[test]
    fn log2_and_exp2_match_the_reference() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for v in [0u128, 1, 2, 3, u64::MAX as u128, 1 << 64, u128::MAX, 64u128 << 64, (64u128 << 64) - 1, (63u128 << 64) | u64::MAX as u128] {
            assert_eq!(log2_x64(v), log2_ref(v), "log2 {v:#x}");
            assert_eq!(exp2_x64(v), exp2_ref(v), "exp2 {v:#x}");
        }
        for k in 0..300_000u32 {
            let n = (((next() as u128) << 64) | next() as u128) >> (k % 128);
            assert_eq!(log2_x64(n), log2_ref(n), "log2 {n:#x}");
            let f = match k % 3 { 0 => next(), 1 => next() & next() & next(), _ => next() | next() };
            let y = (((k % 64) as u128) << 64) | f as u128;
            assert_eq!(exp2_x64(y), exp2_ref(y), "exp2 {y:#x}");
        }
    }

    #[test]
    fn log2_matches_the_exact_body() {
        let mut x = 0x5851_f42d_4c95_7f2du64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for v in [0u128, 1, 2, 3, u128::MAX, u128::MAX >> 1, (1 << 127) + 1, (1 << 72) - 1, 1 << 72, (1 << 72) + 1] {
            assert_eq!(log2_x64(v), log2_exact(v), "{v:#x}");
        }
        let mut fell = 0;
        for k in 0..200_000u32 {
            let n = (((next() as u128) << 64) | next() as u128) >> (k % 128);
            assert_eq!(log2_x64(n), log2_exact(n), "{n:#x}");
            match log2_staged::<0x7fff_0000>(n.max(1)) {
                Some(v) => assert_eq!(Some(v), log2_exact(n.max(1)), "{n:#x}"),
                None => fell += 1,
            }
        }
        assert!(fell > 0, "the wide band never fell back");
    }

    #[test]
    fn staged_log2_error_bound_holds() {
        let (mut e, mut worst) = (0f64, 0f64);
        for k in 0..64u32 {
            if k == EXACT_STEPS {
                e += 2f64.powi(-95);
            }
            if k == EXACT_STEPS + MID_STEPS {
                e += 2f64.powi(-63);
            }
            if k >= EXACT_STEPS {
                worst = worst.max(2.829 * e + e * e);
                e = 2.829 * e + e * e + if k < EXACT_STEPS + MID_STEPS { 6.0 * 2f64.powi(-95) } else { 3.0 * 2f64.powi(-63) };
            }
        }
        assert!(worst < 2f64.powi(-31), "{worst:e}");
        assert!(2.829f64 > 2.0 * 2f64.sqrt());
    }

    const ONE: f64 = 18446744073709551616.0;

    #[test]
    fn k_deltas_are_short() {
        for i in 0..64 {
            assert!(K[i] >= ONE_127);
            if i >= 31 {
                assert_eq!(KD[i][3], 0, "K[{i}]");
            }
            if i >= 63 {
                assert_eq!(KD[i][2], 0, "K[{i}]");
            }
        }
        let mut x = 0x2545_f491_4f6c_dd1du64;
        for k in 0..200_000u64 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let r = ONE_127 | (((x as u128) << 64) | (x.rotate_left(17) ^ k) as u128) >> 1;
            for r in [r, ONE_127, u128::MAX >> 1 | ONE_127] {
                let i = (k % 64) as usize;
                if U256::mul_u128(r, K[i]).hi >> 127 == 0 {
                    assert_eq!(mul_k(r, i), mul_q127(r, K[i]), "{r:#x} K[{i}]");
                }
            }
        }
    }

    #[test]
    fn log2_of_powers_of_two_is_exact() {
        for k in 0..128u32 {
            assert_eq!(log2_x64(1u128 << k), Some((k as u128) << 64));
        }
        assert_eq!(log2_x64(0), None);
    }

    #[test]
    fn exp2_of_integers_is_exact() {
        for k in 0..64u128 {
            assert_eq!(exp2_x64(k << 64), Some(1u128 << (64 + k)));
        }
        assert_eq!(exp2_x64(64u128 << 64), None);
    }

    #[test]
    fn match_f64_closely() {
        for n in [3u128, 5, 7, 10, 1000, 123_456_789, 29_250_000_000, u64::MAX as u128, u128::MAX] {
            let got = log2_x64(n).unwrap() as f64 / ONE;
            let want = (n as f64).log2();
            assert!((got - want).abs() < 1e-12, "log2({n}) {got} vs {want}");
        }
        for y in [0.5f64, 0.123456, 1.75, 9.999, 23.25, 40.1, 62.9] {
            let got = exp2_x64((y * ONE) as u128).unwrap() as f64;
            let want = 2f64.powf(y) * ONE;
            assert!((got / want - 1.0).abs() < 1e-12, "exp2({y}) {got} vs {want}");
        }
    }

    #[test]
    fn round_trip_never_exceeds() {
        for n in [3u128, 17, 1_000_003, 29_250_000_000, 1 << 40] {
            let back = exp2_x64(log2_x64(n).unwrap()).unwrap();
            let exact = n << 64;
            assert!(back <= exact, "n={n}");
            assert!(((exact - back) as f64) / (exact as f64) < 2f64.powi(-56), "n={n}");
        }
    }
}

#[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
thread_local! {
    pub static OPS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

#[cfg(any(test, all(feature = "op-count", not(target_os = "solana"))))]
thread_local! {
    pub static ARGS: core::cell::RefCell<Vec<(u8, u128)>> = const { core::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(feature = "op-count", not(target_os = "solana")))]
pub fn take_ops() -> (u64, Vec<(u8, u128)>) {
    (OPS.with(|c| c.replace(0)), ARGS.with(|a| core::mem::take(&mut *a.borrow_mut())))
}
