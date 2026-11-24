// SPDX-License-Identifier: Apache-2.0
use crate::u256::U256;
use crate::{MathError, BPS};

pub const GENESIS_MIGRATED_DISCOUNT_BPS: u16 = 7_500;

pub fn fixed_raise_claim(c: u128, raised: u128, out: u128, refund: u128) -> Result<(u128, u128), MathError> {
    if raised == 0 {
        return Err(MathError::Zero);
    }
    if c > raised {
        return Err(MathError::Overflow);
    }
    let t = U256::mul_u128(out, c).div_rem_u128(raised).0.to_u128().ok_or(MathError::Overflow)?;
    let q = U256::mul_u128(refund, c).div_rem_u128(raised).0.to_u128().ok_or(MathError::Overflow)?;
    Ok((t, q))
}

pub fn genesis_weight(migrated: u128, public: u128) -> Option<u128> {
    migrated.checked_mul(BPS as u128)?.checked_add(public.checked_mul(BPS as u128 - GENESIS_MIGRATED_DISCOUNT_BPS as u128)?)
}

pub fn genesis_weight_listed(migrated: u128, public: u128, listed: u128, listed_discount: bool) -> Option<u128> {
    let (m, p) = if listed_discount { (migrated.checked_add(listed)?, public) } else { (migrated, public.checked_add(listed)?) };
    genesis_weight(m, p)
}

pub fn genesis_claim(c: u128, w: u128, raised: u128, weight_total: u128, out: u128, refund: u128) -> Result<(u128, u128), MathError> {
    if raised == 0 || weight_total == 0 {
        return Err(MathError::Zero);
    }
    let lo = c.checked_mul(BPS as u128 - GENESIS_MIGRATED_DISCOUNT_BPS as u128).ok_or(MathError::Overflow)?;
    let hi = c.checked_mul(BPS as u128).ok_or(MathError::Overflow)?;
    if c > raised || w > weight_total || w < lo || w > hi {
        return Err(MathError::Overflow);
    }
    let t = U256::mul_u128(out, w).div_rem_u128(weight_total).0.to_u128().ok_or(MathError::Overflow)?;
    let q = U256::mul_u128(refund, c).div_rem_u128(raised).0.to_u128().ok_or(MathError::Overflow)?;
    Ok((t, q))
}

pub const GENESIS_CONVERT_DISCOUNT_MAX_BPS: u16 = 100;

pub fn genesis_convert_quote(lamports: u128, sol_cents_x64: u128, sol_decimals: u8, quote_cents_x64: u128, quote_decimals: u8, discount_bps: u16) -> Result<u128, MathError> {
    if sol_cents_x64 == 0 || quote_cents_x64 == 0 {
        return Err(MathError::Zero);
    }
    if discount_bps > GENESIS_CONVERT_DISCOUNT_MAX_BPS {
        return Err(MathError::Overflow);
    }
    let qd = 10u64.checked_pow(quote_decimals as u32).ok_or(MathError::Overflow)?;
    let sd = 10u128.checked_pow(sol_decimals as u32).ok_or(MathError::Overflow)?;
    let num = U256::mul_u128(lamports, sol_cents_x64).checked_mul_u64(qd).and_then(|n| n.checked_mul_u64(BPS - discount_bps as u64)).ok_or(MathError::Overflow)?;
    let den = sd.checked_mul(quote_cents_x64).and_then(|d| d.checked_mul(BPS as u128)).ok_or(MathError::Overflow)?;
    num.div_ceil_u128(den).to_u128().ok_or(MathError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL: u128 = 1_000_000_000;
    const USDC: u128 = 1_000_000;
    const SOL_X64: u128 = 18_500u128 << 64;
    const USDC_X64: u128 = 100u128 << 64;

    #[test]
    fn inconsistent_inputs_are_refused() {
        assert_eq!(genesis_claim(1, 10_000, 0, 10_000, 1, 1), Err(MathError::Zero), "nothing raised");
        assert_eq!(genesis_claim(1, 10_000, 1, 0, 1, 1), Err(MathError::Zero), "no weight");
        assert_eq!(genesis_claim(2, 20_000, 1, 20_000, 1, 1), Err(MathError::Overflow), "c > raised");
        assert_eq!(genesis_claim(1, 10_000, 2, 9_999, 1, 1), Err(MathError::Overflow), "w > weight_total");
        assert_eq!(genesis_claim(1, 10_001, 2, 20_000, 1, 1), Err(MathError::Overflow), "w over a migrated lamport's");
        assert_eq!(genesis_claim(1, 2_499, 2, 20_000, 1, 1), Err(MathError::Overflow), "w under a Phase 3 lamport's");
        assert_eq!(genesis_claim(0, 0, 2, 20_000, 7, 7), Ok((0, 0)), "a wallet with nothing live gets nothing");
        assert_eq!(genesis_weight_listed(1, 1, 1, true), genesis_weight(2, 1));
        assert_eq!(genesis_weight_listed(1, 1, 1, false), genesis_weight(1, 2));
    }

    #[test]
    fn proportional_weights_are_fixed_raise_claim_bit_for_bit() {
        let (out, refund) = (5_000_000_000_000u128, 123_456_789u128);
        let cs = [1u128, 7, 1_000, 33_333, 999_999_937];
        let raised: u128 = cs.iter().sum();
        for migrated in [false, true] {
            let wt = if migrated { genesis_weight(raised, 0) } else { genesis_weight(0, raised) }.unwrap();
            for &c in &cs {
                let wc = if migrated { genesis_weight(c, 0) } else { genesis_weight(0, c) }.unwrap();
                assert_eq!(genesis_claim(c, wc, raised, wt, out, refund).unwrap(), fixed_raise_claim(c, raised, out, refund).unwrap());
            }
        }
    }

    #[test]
    fn the_conversion_is_the_registry_price_less_the_discount_rounded_up() {
        assert_eq!(genesis_convert_quote(SOL, SOL_X64, 9, USDC_X64, 6, 100).unwrap(), 183_150_000);
        assert_eq!(genesis_convert_quote(SOL, SOL_X64, 9, USDC_X64, 6, 0).unwrap(), 185 * USDC);
        assert_eq!(genesis_convert_quote(1, SOL_X64, 9, USDC_X64, 6, 100).unwrap(), 1);
        assert_eq!(genesis_convert_quote(0, SOL_X64, 9, USDC_X64, 6, 100).unwrap(), 0);
        assert!(genesis_convert_quote(SOL, SOL_X64, 9, 99u128 << 64, 6, 100).unwrap() > 183_150_000);
        assert_eq!(genesis_convert_quote(SOL, SOL_X64, 9, USDC_X64, 6, GENESIS_CONVERT_DISCOUNT_MAX_BPS + 1).err(), Some(MathError::Overflow), "the discount's max");
        assert_eq!(genesis_convert_quote(SOL, 0, 9, USDC_X64, 6, 100).err(), Some(MathError::Zero));
        assert!(genesis_convert_quote(u64::MAX as u128, 100_000_000u128 << 64, 9, USDC_X64, 6, 100).is_ok());
        let one = genesis_convert_quote(130 * SOL, SOL_X64, 9, USDC_X64, 6, 100).unwrap();
        let parts: u128 = (0..13).map(|_| genesis_convert_quote(10 * SOL, SOL_X64, 9, USDC_X64, 6, 100).unwrap()).sum();
        assert!(parts >= one);
    }
}
