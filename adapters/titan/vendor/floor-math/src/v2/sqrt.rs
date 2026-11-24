// SPDX-License-Identifier: Apache-2.0
use super::wide::{div_u256, div_w_wide, isqrt, mul_shr, mul_w, shl_div, shl_w, shr_w, w3, w_u128, w_u256, Round};
use crate::u256::U256;

pub type Sqrt = u128;
pub const FRAC: u32 = 72;
pub const ONE: Sqrt = 1 << FRAC;
pub const SQRT_TAIL_END: Sqrt = 1 << 127;

pub fn money_between(l: u128, a: Sqrt, b: Sqrt, r: Round) -> Option<u128> {
    mul_shr(l, b.checked_sub(a)?, FRAC, r)
}

pub fn tokens_between(l: u128, a: Sqrt, b: Sqrt, r: Round) -> Option<u128> {
    if a == 0 {
        return None;
    }
    let d = b.checked_sub(a)?;
    let t = shl_div(U256::mul_u128(l, d), FRAC, b, r)?;
    div_u256(t, a, r)?.to_u128()
}

pub fn sqrt_after_money(l: u128, p: Sqrt, money: u128) -> Option<Sqrt> {
    let dp = shl_div(U256::from_u128(money), FRAC, l, Round::Down)?.to_u128()?;
    p.checked_add(dp)
}

pub fn sqrt_after_tokens(l: u128, p: Sqrt, tokens: u128) -> Option<Sqrt> {
    let num = shl_w(w3(U256::mul_u128(l, p)), FRAC)?;
    let den = w_u256(shl_w(w3(U256::from_u128(l)), FRAC)?)?.checked_add(U256::mul_u128(tokens, p))?;
    div_w_wide(num, den, Round::Up)?.to_u128()
}

pub fn sqrt_after_buying_tokens(l: u128, p: Sqrt, tokens: u128) -> Option<Sqrt> {
    let num = shl_w(w3(U256::mul_u128(l, p)), FRAC)?;
    let den = w_u256(shl_w(w3(U256::from_u128(l)), FRAC)?)?.checked_sub(U256::mul_u128(tokens, p))?;
    if den.is_zero() {
        return None;
    }
    div_w_wide(num, den, Round::Up)?.to_u128()
}

pub fn liquidity_for_tokens(a: Sqrt, b: Sqrt, tokens: u128) -> Option<u128> {
    let d = b.checked_sub(a).filter(|d| *d != 0)?;
    let w = shr_w(mul_w(U256::mul_u128(tokens, a), b), FRAC, Round::Down);
    super::wide::div_w(w, d, Round::Down)?.to_u128()
}

pub fn liquidity_for_money(a: Sqrt, b: Sqrt, money: u128) -> Option<u128> {
    let d = b.checked_sub(a).filter(|d| *d != 0)?;
    shl_div(U256::from_u128(money), FRAC, d, Round::Down)?.to_u128()
}

pub fn flat_tokens_for_money(s: Sqrt, money: u128, r: Round) -> Option<u128> {
    if s == 0 {
        return None;
    }
    let t = super::wide::div_w(shl_w(w3(U256::from_u128(money)), 2 * FRAC)?, s, r)?;
    div_u256(t, s, r)?.to_u128()
}

pub fn flat_money_for_tokens(s: Sqrt, tokens: u128, r: Round) -> Option<u128> {
    w_u128(shr_w(mul_w(U256::mul_u128(tokens, s), s), 2 * FRAC, r))
}

pub fn sqrt_of_ratio(money: u128, tokens: u128) -> Option<Sqrt> {
    if tokens == 0 {
        return None;
    }
    let n = super::wide::div_w(shl_w(w3(U256::from_u128(money)), 2 * FRAC)?, tokens, Round::Down)?;
    Some(isqrt(n))
}

pub fn sqrt_scale_down_bps(s: Sqrt, bps: u16) -> Option<Sqrt> {
    if bps > 10_000 {
        return None;
    }
    let n = super::wide::div_w(shl_w(w3(U256::from_u128(10_000 - bps as u128)), 2 * FRAC)?, 10_000, Round::Down)?;
    mul_shr(s, isqrt(n), FRAC, Round::Down)
}

pub fn price_x64(s: Sqrt) -> u128 {
    w_u128(shr_w(w3(U256::sqr_u128(s)), 2 * FRAC - 64, Round::Down)).unwrap_or(u128::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_math_matches_closed_forms() {
        let l = 1000u128;
        assert_eq!(money_between(l, ONE, 2 * ONE, Round::Down), Some(1000));
        assert_eq!(tokens_between(l, ONE, 2 * ONE, Round::Down), Some(500));
        assert_eq!(liquidity_for_tokens(ONE, 2 * ONE, 500), Some(1000));
        assert_eq!(liquidity_for_money(ONE, 2 * ONE, 1000), Some(1000));
        assert_eq!(sqrt_after_money(l, ONE, 1000), Some(2 * ONE));
        assert_eq!(sqrt_after_tokens(l, 2 * ONE, 500), Some(ONE));
        assert_eq!(sqrt_after_buying_tokens(l, ONE, 500), Some(2 * ONE));
        assert_eq!(flat_tokens_for_money(2 * ONE, 400, Round::Down), Some(100));
        assert_eq!(flat_money_for_tokens(2 * ONE, 100, Round::Up), Some(400));
        assert_eq!(sqrt_of_ratio(400, 100), Some(2 * ONE));
        assert_eq!(price_x64(2 * ONE), 4u128 << 64);
        assert_eq!(sqrt_scale_down_bps(ONE, 0), Some(ONE));
        let s = sqrt_scale_down_bps(ONE, 2_500).unwrap();
        let exact = isqrt(super::super::wide::div_w(shl_w(w3(U256::from_u128(3)), 2 * FRAC).unwrap(), 4, Round::Down).unwrap());
        assert!(s <= exact && s + 2 >= exact);
    }

    #[test]
    fn extremes_fit() {
        let l = liquidity_for_tokens(SQRT_TAIL_END >> 8, SQRT_TAIL_END, 1_000_000_000_000_000).unwrap();
        assert!(tokens_between(l, SQRT_TAIL_END >> 8, SQRT_TAIL_END, Round::Down).unwrap() <= 1_000_000_000_000_000);
        assert!(flat_money_for_tokens(SQRT_TAIL_END, u64::MAX as u128, Round::Up).is_none());
    }
}
