// SPDX-License-Identifier: Apache-2.0
pub const FEE_LEGS: usize = 5;
pub const FEE_DEFAULT_PPM: u32 = 100;
pub const DEFAULT_FEE_PPM: [u32; FEE_LEGS] = [FEE_DEFAULT_PPM; FEE_LEGS];
pub const LEG_BACKING: usize = 0;
pub const LEG_TEAM: usize = 1;
pub const LEG_PLATFORM: usize = 2;
pub const LEG_FLOOR: usize = 3;
pub const LEG_HOLDER: usize = 4;
const PPM: u128 = 1_000_000;

pub fn fee_leg(amount: u128, ppm: u32) -> u128 {
    super::wide::mul_div(amount, ppm as u128, PPM, super::wide::Round::Up).unwrap_or(amount)
}

pub fn ppm_at(end: &[u32; FEE_LEGS], start: &[u32; FEE_LEGS], sqrt_a: u128, sqrt_b: u128, sqrt_p: u128) -> [u32; FEE_LEGS] {
    ppm_at_cached(end, start, sqrt_a, sqrt_b, sqrt_p, &mut LgMemo::default())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LgMemo {
    pub knots: Option<(u128, u128)>,
    pub last: Option<(u128, u128)>,
    #[cfg(not(target_os = "solana"))]
    pub rates: Option<RatesKey>,
}

#[cfg(not(target_os = "solana"))]
pub type RatesKey = (u128, u128, u128, [u32; FEE_LEGS], [u32; FEE_LEGS], [u32; FEE_LEGS]);

pub trait Lg {
    fn knots(&mut self, sqrt_a: u128, sqrt_b: u128) -> (u128, u128);
    fn lg(&mut self, sqrt_p: u128) -> u128;
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn lg_kept(&mut self, _sqrt_p: u128) -> Option<u128> {
        None
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn rates_kept(&mut self, _k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS])) -> Option<[u32; FEE_LEGS]> {
        None
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn keep_rates(&mut self, _k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS]), _f: [u32; FEE_LEGS]) {}
}

#[inline(always)]
fn lg_raw(x: u128) -> u128 {
    crate::fixed::log2_x64(x).unwrap_or(0)
}

impl Lg for LgMemo {
    #[inline(always)]
    fn knots(&mut self, sqrt_a: u128, sqrt_b: u128) -> (u128, u128) {
        *self.knots.get_or_insert_with(|| (lg_raw(sqrt_a), lg_raw(sqrt_b)))
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn lg_kept(&mut self, sqrt_p: u128) -> Option<u128> {
        match self.last {
            Some((p, l)) if p == sqrt_p => Some(l),
            _ => None,
        }
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn rates_kept(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS])) -> Option<[u32; FEE_LEGS]> {
        match &self.rates {
            Some((p, a, b, e, s, f)) if (*p, *a, *b) == (k.0, k.1, k.2) && e == k.3 && s == k.4 => Some(*f),
            _ => None,
        }
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn keep_rates(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS]), f: [u32; FEE_LEGS]) {
        self.rates = Some((k.0, k.1, k.2, *k.3, *k.4, f));
    }
    #[inline(always)]
    fn lg(&mut self, sqrt_p: u128) -> u128 {
        match self.last {
            Some((p, l)) if p == sqrt_p => l,
            _ => {
                let l = lg_raw(sqrt_p);
                self.last = Some((sqrt_p, l));
                l
            }
        }
    }
}

impl<T: Lg + ?Sized> Lg for &mut T {
    #[inline(always)]
    fn knots(&mut self, sqrt_a: u128, sqrt_b: u128) -> (u128, u128) {
        (**self).knots(sqrt_a, sqrt_b)
    }
    #[inline(always)]
    fn lg(&mut self, sqrt_p: u128) -> u128 {
        (**self).lg(sqrt_p)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn lg_kept(&mut self, sqrt_p: u128) -> Option<u128> {
        (**self).lg_kept(sqrt_p)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn rates_kept(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS])) -> Option<[u32; FEE_LEGS]> {
        (**self).rates_kept(k)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline(always)]
    fn keep_rates(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS]), f: [u32; FEE_LEGS]) {
        (**self).keep_rates(k, f)
    }
}

pub const LG_CACHE_SLOTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LgCache {
    knots: Option<(u128, u128, u128, u128)>,
    slots: [(u128, u128); LG_CACHE_SLOTS],
}

impl Default for LgCache {
    fn default() -> LgCache {
        LgCache::new()
    }
}

impl LgCache {
    pub const fn new() -> LgCache {
        LgCache { knots: None, slots: [(0, 0); LG_CACHE_SLOTS] }
    }

    pub fn clear(&mut self) {
        *self = LgCache::new();
    }

    #[inline(always)]
    fn slot(p: u128) -> usize {
        let h = (p as u64 ^ (p >> 64) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (h >> (64 - LG_CACHE_SLOTS.trailing_zeros())) as usize
    }
}

impl Lg for LgCache {
    #[inline]
    fn knots(&mut self, sqrt_a: u128, sqrt_b: u128) -> (u128, u128) {
        match self.knots {
            Some((a, b, la, lb)) if a == sqrt_a && b == sqrt_b => (la, lb),
            _ => {
                let (la, lb) = (lg_raw(sqrt_a), lg_raw(sqrt_b));
                self.knots = Some((sqrt_a, sqrt_b, la, lb));
                (la, lb)
            }
        }
    }
    #[inline]
    fn lg(&mut self, sqrt_p: u128) -> u128 {
        let i = LgCache::slot(sqrt_p);
        let (p, l) = self.slots[i];
        if p == sqrt_p && p != 0 {
            return l;
        }
        let l = lg_raw(sqrt_p);
        self.slots[i] = (sqrt_p, l);
        l
    }
    #[cfg(not(target_os = "solana"))]
    #[inline]
    fn lg_kept(&mut self, sqrt_p: u128) -> Option<u128> {
        let (p, l) = self.slots[LgCache::slot(sqrt_p)];
        (p == sqrt_p && p != 0).then_some(l)
    }
}

impl LgCache {
    pub fn peek(&self) -> LgPeek<'_> {
        LgPeek { cache: self, memo: LgMemo::default() }
    }

    pub fn warm(&mut self, sqrt_p: u128) {
        if sqrt_p != 0 {
            let _ = self.lg(sqrt_p);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LgPeek<'a> {
    cache: &'a LgCache,
    memo: LgMemo,
}

impl Lg for LgPeek<'_> {
    #[inline]
    fn knots(&mut self, sqrt_a: u128, sqrt_b: u128) -> (u128, u128) {
        match self.cache.knots {
            Some((a, b, la, lb)) if a == sqrt_a && b == sqrt_b => (la, lb),
            _ => self.memo.knots(sqrt_a, sqrt_b),
        }
    }
    #[inline]
    fn lg(&mut self, sqrt_p: u128) -> u128 {
        let (p, l) = self.cache.slots[LgCache::slot(sqrt_p)];
        if p == sqrt_p && p != 0 {
            return l;
        }
        self.memo.lg(sqrt_p)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline]
    fn lg_kept(&mut self, sqrt_p: u128) -> Option<u128> {
        let (p, l) = self.cache.slots[LgCache::slot(sqrt_p)];
        if p == sqrt_p && p != 0 {
            return Some(l);
        }
        self.memo.lg_kept(sqrt_p)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline]
    fn rates_kept(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS])) -> Option<[u32; FEE_LEGS]> {
        self.memo.rates_kept(k)
    }
    #[cfg(not(target_os = "solana"))]
    #[inline]
    fn keep_rates(&mut self, k: (u128, u128, u128, &[u32; FEE_LEGS], &[u32; FEE_LEGS]), f: [u32; FEE_LEGS]) {
        self.memo.keep_rates(k, f)
    }
}

pub fn ppm_at_cached<M: Lg + ?Sized>(end: &[u32; FEE_LEGS], start: &[u32; FEE_LEGS], sqrt_a: u128, sqrt_b: u128, sqrt_p: u128, memo: &mut M) -> [u32; FEE_LEGS] {
    if sqrt_b == 0 || sqrt_p >= sqrt_b {
        return *end;
    }
    if sqrt_p <= sqrt_a {
        return *start;
    }
    let (la, lb) = memo.knots(sqrt_a, sqrt_b);
    #[cfg(not(target_os = "solana"))]
    if memo.lg_kept(sqrt_p).is_none() {
        let k = (sqrt_p, sqrt_a, sqrt_b, end, start);
        if let Some(f) = memo.rates_kept(k) {
            return f;
        }
        if let Some(f) = rates_f64(end, start, la, lb, sqrt_p) {
            memo.keep_rates(k, f);
            return f;
        }
    }
    let lp = memo.lg(sqrt_p);
    let one = 1u128 << 64;
    let w = if lb <= la { 0 } else { super::wide::mul_div(lb.saturating_sub(lp), one, lb - la, super::wide::Round::Down).unwrap_or(0).min(one) };
    let mut f = [0u32; FEE_LEGS];
    for i in 0..FEE_LEGS {
        let x = start[i] as u128 * w + end[i] as u128 * (one - w);
        f[i] = x.div_ceil(one) as u32;
    }
    f
}

#[cfg(not(target_os = "solana"))]
#[inline]
fn lg_bracket(n: u128) -> Option<(u128, u128)> {
    if n == 0 {
        return None;
    }
    let msb = 127 - n.leading_zeros();
    let m = if msb >= 52 { (n >> (msb - 52)) as u64 } else { (n << (52 - msb)) as u64 };
    let fr = (m as f64 / (1u64 << 52) as f64).log2();
    if !(0.0..1.0).contains(&fr) {
        return None;
    }
    let est = ((msb as u128) << 64) + (fr * 18_446_744_073_709_551_616.0) as u128;
    Some((est.saturating_sub(LG_BRACKET), est + LG_BRACKET))
}

#[cfg(not(target_os = "solana"))]
const LG_BRACKET: u128 = 1 << 32;

#[cfg(not(target_os = "solana"))]
#[inline]
fn rates_f64(end: &[u32; FEE_LEGS], start: &[u32; FEE_LEGS], la: u128, lb: u128, sqrt_p: u128) -> Option<[u32; FEE_LEGS]> {
    if lb <= la {
        return Some(*end);
    }
    let (lo, hi) = lg_bracket(sqrt_p)?;
    let est = lo / 2 + hi / 2 + (lo & hi & 1);
    let span = (lb - la) as f64;
    let w = (lb.saturating_sub(est) as f64 / span).clamp(0.0, 1.0);
    let err = (2 * LG_BRACKET + 1) as f64 / span + 1.0 / 281_474_976_710_656.0;
    let mut f = [0u32; FEE_LEGS];
    for i in 0..FEE_LEGS {
        let (e, s) = (end[i] as f64, start[i] as f64);
        if end[i] == start[i] {
            f[i] = end[i];
            continue;
        }
        let x = e + (s - e) * w;
        let d = (s - e).abs() * err + 1.0 / 1_048_576.0;
        let (a, b) = ((x - d).ceil(), (x + d).ceil());
        if a != b || a < 0.0 {
            return None;
        }
        f[i] = a as u32;
    }
    Some(f)
}

pub const PREGRAD_PPM: [u32; FEE_LEGS] = [0, 0, 4_000, 1_000, 0];

#[allow(clippy::too_many_arguments)]
pub fn rates_at<M: Lg + ?Sized>(graduated: bool, own: bool, end: &[u32; FEE_LEGS], start: &[u32; FEE_LEGS], sqrt_a: u128, sqrt_b: u128, sqrt_p: u128, memo: &mut M) -> [u32; FEE_LEGS] {
    if !graduated && !own {
        return PREGRAD_PPM;
    }
    ppm_at_cached(end, start, sqrt_a, sqrt_b, sqrt_p, memo)
}

pub fn cheaper(a: &[u32; FEE_LEGS], b: &[u32; FEE_LEGS]) -> [u32; FEE_LEGS] {
    let mut f = *a;
    for i in 0..FEE_LEGS {
        f[i] = a[i].min(b[i]);
    }
    f
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeeAcc<M = LgMemo> {
    pub pre: [u32; FEE_LEGS],
    pub whole: [u128; FEE_LEGS],
    pub rem: [u32; FEE_LEGS],
    pub memo: M,
}

impl FeeAcc {
    pub fn new(pre: [u32; FEE_LEGS]) -> FeeAcc {
        FeeAcc { pre, whole: [0; FEE_LEGS], rem: [0; FEE_LEGS], memo: LgMemo::default() }
    }
}

impl<M> FeeAcc<M> {
    pub fn with_memo(pre: [u32; FEE_LEGS], memo: M) -> FeeAcc<M> {
        FeeAcc { pre, whole: [0; FEE_LEGS], rem: [0; FEE_LEGS], memo }
    }

    pub fn add(&mut self, q: u128, at: &[u32; FEE_LEGS]) -> Option<()> {
        let (qh, ql) = (q / PPM, q % PPM);
        for i in 0..FEE_LEGS {
            let r = self.pre[i].min(at[i]) as u128;
            let lo = ql * r;
            let rem = self.rem[i] as u128 + lo % PPM;
            self.whole[i] = self.whole[i].checked_add(qh.checked_mul(r)?)?.checked_add(lo / PPM + rem / PPM)?;
            self.rem[i] = (rem % PPM) as u32;
        }
        Some(())
    }

    pub fn legs(&self) -> [u128; FEE_LEGS] {
        let mut f = [0u128; FEE_LEGS];
        for i in 0..FEE_LEGS {
            f[i] = self.whole[i].saturating_add((self.rem[i] > 0) as u128);
        }
        f
    }
}

pub fn fee_legs(amount: u128, ppm: &[u32; FEE_LEGS]) -> [u128; FEE_LEGS] {
    let mut f = [0u128; FEE_LEGS];
    for i in 0..FEE_LEGS {
        f[i] = fee_leg(amount, ppm[i]);
    }
    f
}

pub fn fee_total(f: &[u128; FEE_LEGS]) -> u128 {
    f.iter().fold(0u128, |t, x| t.saturating_add(*x))
}

pub fn gross_for_net(net: u128, ppm: &[u32; FEE_LEGS]) -> Option<u128> {
    #[cfg(not(target_os = "solana"))]
    return gross_for_net_host(net, ppm);
    #[cfg(target_os = "solana")]
    return gross_for_net_sbf(net, ppm);
}

#[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
pub fn gross_for_net_sbf(net: u128, ppm: &[u32; FEE_LEGS]) -> Option<u128> {
    net.checked_add(fee_total(&fee_legs(net, ppm)))
}

#[cfg_attr(not(any(test, not(target_os = "solana"))), allow(dead_code))]
pub fn gross_for_net_host(net: u128, ppm: &[u32; FEE_LEGS]) -> Option<u128> {
    if net >> 96 != 0 {
        return gross_for_net_sbf(net, ppm);
    }
    let mut t = net;
    for p in ppm {
        t += (net * *p as u128 + (PPM - 1)) / PPM;
    }
    Some(t)
}

pub fn net_from_gross(gross: u128, ppm: &[u32; FEE_LEGS]) -> u128 {
    #[cfg(not(target_os = "solana"))]
    return net_from_gross_host(gross, ppm);
    #[cfg(target_os = "solana")]
    return net_from_gross_sbf(gross, ppm);
}

#[cfg_attr(not(any(test, target_os = "solana")), allow(dead_code))]
pub fn net_from_gross_sbf(gross: u128, ppm: &[u32; FEE_LEGS]) -> u128 {
    let sum: u128 = ppm.iter().map(|p| *p as u128).sum();
    let mut net = super::wide::mul_div(gross, PPM, PPM + sum, super::wide::Round::Down).unwrap_or(0);
    while net < gross && gross_for_net_sbf(net + 1, ppm).is_some_and(|g| g <= gross) {
        net += 1;
    }
    while net > 0 && gross_for_net_sbf(net, ppm).is_none_or(|g| g > gross) {
        net -= 1;
    }
    net
}

#[cfg_attr(not(any(test, not(target_os = "solana"))), allow(dead_code))]
pub fn net_from_gross_host(gross: u128, ppm: &[u32; FEE_LEGS]) -> u128 {
    if gross >> 96 != 0 {
        return net_from_gross_sbf(gross, ppm);
    }
    let sum = ppm.iter().fold(0u128, |t, p| t + *p as u128);
    let mut net = gross * PPM / (PPM + sum);
    let mut g = gross_for_net_host(net, ppm).unwrap_or(u128::MAX);
    if g <= gross {
        loop {
            let up = gross_for_net_host(net + 1, ppm).unwrap_or(u128::MAX);
            if up > gross {
                return net;
            }
            net += 1;
        }
    }
    while net > 0 && g > gross {
        net -= 1;
        g = gross_for_net_host(net, ppm).unwrap_or(u128::MAX);
    }
    net
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a12_rates_at_is_flat_before_graduation_unless_own() {
        let (start, end) = ([2_000, 9_000, 2_000, 500, 0], [1_000, 2_000, 500, 100, 0]);
        let (a, b) = (47_223_664_828_696_452_136u128, 1_299_743_739_601_393_633_171u128);
        for p in [a / 2, a, (a + b) / 2, b, b * 2] {
            let curve = ppm_at(&end, &start, a, b, p);
            assert_eq!(rates_at(false, false, &end, &start, a, b, p, &mut LgMemo::default()), PREGRAD_PPM, "a preset before graduation");
            assert_eq!(rates_at(false, true, &end, &start, a, b, p, &mut LgMemo::default()), curve, "a genesis book keeps its curve");
            assert_eq!(rates_at(true, false, &end, &start, a, b, p, &mut LgMemo::default()), curve, "graduated: the curve");
            assert_eq!(rates_at(true, true, &end, &start, a, b, p, &mut LgMemo::default()), curve);
        }
        assert_eq!(PREGRAD_PPM.iter().sum::<u32>(), 5_000);
        assert_eq!(fee_legs(1_000_001, &PREGRAD_PPM), [0, 0, 4_001, 1_001, 0]);
    }

    #[test]
    fn ppm_at_cached_is_ppm_at_bit_for_bit() {
        let (start, end) = ([2_000, 9_000, 2_000, 500, 0], [1_000, 2_000, 500, 100, 0]);
        let (a, b) = (47_223_664_828_696_452_136u128, 1_299_743_739_601_393_633_171u128);
        let mut memo = LgMemo::default();
        for k in 0..=64u128 {
            let p = a / 2 + (b * 2 - a / 2) * k / 64;
            assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut memo), ppm_at(&end, &start, a, b, p), "at {p}");
            assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut memo), ppm_at(&end, &start, a, b, p), "again at {p}");
            assert_eq!(ppm_at_cached(&end, &start, a, 0, p, &mut LgMemo::default()), end);
        }
        assert!(memo.knots.is_some(), "a price inside the curve filled the memo's knots");
    }

    #[test]
    fn fee_acc_is_one_rounding_of_the_steps() {
        let pre = [100, 10_000, 500, 2_000, 0];
        for q in [0u128, 1, 999_999, 1_000_000, 1_234_567_891, u128::MAX / 20_000] {
            let mut a = FeeAcc::new(pre);
            a.add(q, &pre).unwrap();
            assert_eq!(a.legs(), fee_legs(q, &pre));
        }
        let mut a = FeeAcc::new(pre);
        let steps = [3u128, 999_999, 7, 1_000_001, 123_456_789];
        steps.iter().for_each(|q| a.add(*q, &[u32::MAX; FEE_LEGS]).unwrap());
        assert_eq!(a.legs(), fee_legs(steps.iter().sum(), &pre));
        let mut a = FeeAcc::new(pre);
        a.add(1_000_001, &[200, 5_000, 600, 1_000, 7]).unwrap();
        a.add(500_000, &[50, 20_000, 500, 3_000, 7]).unwrap();
        let want = |r0: u128, r1: u128| (1_000_001 * r0 + 500_000 * r1).div_ceil(1_000_000);
        assert_eq!(a.legs(), [want(100, 50), want(5_000, 10_000), want(500, 500), want(1_000, 2_000), 0]);
    }

    #[test]
    fn legs_round_up_and_net_is_largest() {
        let p = DEFAULT_FEE_PPM;
        assert_eq!(fee_legs(1, &p), [1; 5]);
        assert_eq!(fee_legs(10_000, &p), [1; 5]);
        assert_eq!(fee_legs(10_001, &p), [2; 5]);
        assert_eq!(fee_legs(0, &p), [0; 5]);
        assert_eq!(net_from_gross(5, &p), 0);
        assert_eq!(net_from_gross(6, &p), 1);
        let mut s = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..20_000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let g = ((s as u128) << 64 | s.rotate_left(17) as u128) >> (s % 127);
            let q = [s as u32 % 5_001, 100, 0, 7, 5_000];
            for ppm in [p, q] {
                let n = net_from_gross(g, &ppm);
                assert!(gross_for_net(n, &ppm).unwrap() <= g);
                assert!(gross_for_net(n + 1, &ppm).is_none_or(|x| x > g));
            }
        }
    }

    #[test]
    fn lg_cache_is_bit_for_bit() {
        let (end, start) = ([100, 200, 300, 400, 500], [10_000, 0, 5_000, 1, 9_999]);
        let (a, b) = (1u128 << 70, 1u128 << 80);
        let mut cache = LgCache::new();
        let mut x = 0x1234_5678_9abc_def0u128;
        for i in 0..20_000u32 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let p = match i % 4 {
                0 => a + (x % (b - a)),
                1 => a + ((x % 97) << 70),
                2 => x % (b << 1),
                _ => b - (i as u128 % 7),
            };
            let want = ppm_at(&end, &start, a, b, p);
            assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut cache), want, "LgCache at {p}");
            assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut cache.peek()), want, "LgPeek at {p}");
            assert_eq!(ppm_at_cached(&end, &start, a + 1, b, p, &mut cache), ppm_at(&end, &start, a + 1, b, p));
        }
        cache.clear();
        assert_eq!(cache, LgCache::new());
    }

    #[test]
    fn lg_bracket_holds_the_exact_log2() {
        let mut ns: Vec<u128> = vec![1, 2, 3, u128::MAX, u128::MAX - 1];
        for k in 0..128u32 {
            let t = 1u128 << k;
            ns.extend([t, t.wrapping_sub(1), t + 1, t | (t >> 1), t.wrapping_add(t - 1)]);
        }
        let mut x = 0x0bad_5eed_u128;
        for _ in 0..200_000 {
            x = x.wrapping_mul(0x2545_F491_4F6C_DD1D_9E37_79B9_7F4A_7C15).wrapping_add(1442695040888963407);
            ns.push(x >> (x % 128) as u32);
        }
        for n in ns.into_iter().filter(|n| *n > 0) {
            let l = crate::fixed::log2_x64(n).unwrap();
            let (lo, hi) = lg_bracket(n).expect("a bracket");
            assert!(lo <= l && l <= hi, "log2_x64({n}) = {l} outside [{lo}, {hi}]");
        }
    }

    #[test]
    fn bracketed_rates_are_bit_for_bit() {
        fn exact(end: &[u32; FEE_LEGS], start: &[u32; FEE_LEGS], a: u128, b: u128, p: u128) -> [u32; FEE_LEGS] {
            if b == 0 || p >= b {
                return *end;
            }
            if p <= a {
                return *start;
            }
            let lg = |x: u128| crate::fixed::log2_x64(x).unwrap_or(0);
            let (la, lb, lp) = (lg(a), lg(b), lg(p));
            let one = 1u128 << 64;
            let w = if lb <= la { 0 } else { crate::v2::wide::mul_div(lb.saturating_sub(lp), one, lb - la, crate::v2::wide::Round::Down).unwrap_or(0).min(one) };
            core::array::from_fn(|i| (start[i] as u128 * w + end[i] as u128 * (one - w)).div_ceil(one) as u32)
        }
        let legs: [([u32; FEE_LEGS], [u32; FEE_LEGS]); 4] = [
            ([100, 200, 300, 400, 500], [10_000, 0, 5_000, 1, 9_999]),
            ([0; FEE_LEGS], [1_000_000; FEE_LEGS]),
            ([1_000_000, 0, 1, 999_999, 3], [0, 1_000_000, 2, 1, 4]),
            ([2_500, 2_500, 4_000, 1_000, 2_000], [5_000, 5_000, 4_000, 1_000, 4_000]),
        ];
        let (mut n, mut x) = (0usize, 0x51ed_2701_u128);
        for (end, start) in legs {
            for (a, b) in [(1u128, 2u128), (1 << 70, 1 << 80), (1 << 72, (1 << 72) + 1_000), (3 << 60, 7 << 90), (1 << 100, 1 << 120), (5, 1 << 127)] {
                let mut cache = LgCache::new();
                let mut shared = LgMemo::default();
                for i in 0..4_000u32 {
                    x = x.wrapping_mul(0x2545_F491_4F6C_DD1D_9E37_79B9_7F4A_7C15).wrapping_add(1442695040888963407);
                    let p = match i % 5 {
                        0 => a + (x % (b - a)),
                        1 => a + (x % 64),
                        2 => b.saturating_sub(x % 64),
                        3 => x % (b << 1).max(4),
                        _ => a + ((b - a) >> (x % 64) as u32),
                    };
                    let want = exact(&end, &start, a, b, p);
                    assert_eq!(ppm_at(&end, &start, a, b, p), want, "LgMemo at {p} on [{a}, {b}]");
                    assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut cache.peek()), want, "LgPeek at {p}");
                    assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut cache), want, "LgCache at {p}");
                    for _ in 0..2 {
                        assert_eq!(ppm_at_cached(&end, &start, a, b, p, &mut shared), want, "a shared LgMemo at {p} on [{a}, {b}]");
                    }
                    let (e2, s2) = (start, end);
                    assert_eq!(ppm_at_cached(&e2, &s2, a, b, p, &mut shared), exact(&e2, &s2, a, b, p), "swapped legs at {p}");
                    n += 1;
                }
            }
        }
        assert!(n > 90_000);
    }
}
