// SPDX-License-Identifier: Apache-2.0
use super::ask::{self, Kind};
#[cfg(not(target_os = "solana"))]
use super::book::Book;
use super::book::{KIND_BONDING, KIND_CLIMB, KIND_SALE_BAND, KIND_TAIL, KIND_UNUSED, RANGES_MAX};
#[cfg(not(target_os = "solana"))]
use super::fees::DEFAULT_FEE_PPM;
use super::rows::Row;
use super::sqrt::{flat_money_for_tokens, liquidity_for_tokens, money_between, Sqrt, FRAC, SQRT_TAIL_END};
use super::wide::{div_w_wide, isqrt, mul_shr, mul_w, shl_w, w_u256, Round};
use crate::MathError;
use crate::u256::U256;

pub const N0: u64 = 1_000_000_000_000_000;
pub const TOKEN_DECIMALS_MIN: u8 = 6;
pub const TOKEN_DECIMALS_MAX: u8 = 24;
pub const QUOTE_DECIMALS_MAX: u8 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuoteRange {
    pub sqrt_a: Sqrt,
    pub sqrt_b: Sqrt,
    pub tokens: u64,
    pub kind: u8,
}

pub fn sqrt_p0(launch_cap_usd_cents: u64, quote_usd_cents_x64: u128, quote_decimals: u8) -> Result<Sqrt, MathError> {
    if quote_usd_cents_x64 == 0 || quote_decimals > QUOTE_DECIMALS_MAX || launch_cap_usd_cents == 0 {
        return Err(MathError::QuoteRowCannotOpen);
    }
    let num = mul_w(U256::from_u128(launch_cap_usd_cents as u128), 10u128.pow(quote_decimals as u32));
    let num = shl_w(num, 64 + 2 * FRAC).ok_or(MathError::QuoteRowCannotOpen)?;
    let den = w_u256(mul_w(U256::from_u128(quote_usd_cents_x64), N0 as u128)).ok_or(MathError::QuoteRowCannotOpen)?;
    let s = isqrt(div_w_wide(num, den, Round::Down).ok_or(MathError::QuoteRowCannotOpen)?);
    if s == 0 || s > SQRT_TAIL_END {
        return Err(MathError::QuoteRowCannotOpen);
    }
    Ok(s)
}

pub fn ranges_at(row: &Row, quote_usd_cents_x64: u128, quote_decimals: u8, token_decimals: u8) -> Result<Vec<QuoteRange>, MathError> {
    if !(TOKEN_DECIMALS_MIN..=TOKEN_DECIMALS_MAX).contains(&token_decimals) {
        return Err(MathError::QuoteRowCannotOpen);
    }
    let cap_cents = u64::try_from(row.launch_cap.checked_mul(100).ok_or(MathError::QuoteRowCannotOpen)?).map_err(|_| MathError::QuoteRowCannotOpen)?;
    let p0 = sqrt_p0(cap_cents, quote_usd_cents_x64, quote_decimals)?;
    let a = ask::build(row).map_err(|_| MathError::QuoteRowCannotOpen)?;
    if a.segs.len() > RANGES_MAX || a.segs.is_empty() {
        return Err(MathError::QuoteRowCannotOpen);
    }
    let cap = row.launch_cap as f64;
    let abs = |x: f64| -> Result<Sqrt, MathError> {
        let rel = (x / cap).sqrt() * 18_446_744_073_709_551_616.0;
        if !(rel.is_finite() && rel >= 1.0 && rel < 3.4e38) {
            return Err(MathError::QuoteRowCannotOpen);
        }
        let s = mul_shr(p0, rel as u128, 64, Round::Down).ok_or(MathError::QuoteRowCannotOpen)?;
        if s == 0 || s >= SQRT_TAIL_END {
            return Err(MathError::QuoteRowCannotOpen);
        }
        Ok(s)
    };
    let mut out = Vec::with_capacity(a.segs.len());
    let mut lo = p0;
    let mut left = N0;
    for (i, g) in a.segs.iter().enumerate() {
        let kind = match g.kind {
            Kind::Bonding => KIND_BONDING,
            Kind::Climb => KIND_CLIMB,
            Kind::Sale => KIND_SALE_BAND,
            Kind::Unused => KIND_UNUSED,
            Kind::Tail => KIND_TAIL,
        };
        let last = i + 1 == a.segs.len();
        if last != (g.kind == Kind::Tail) {
            return Err(MathError::QuoteRowCannotOpen);
        }
        let flat = g.a == g.b;
        let hi = if last { SQRT_TAIL_END } else if flat { lo } else { abs(g.b)? };
        if hi < lo || (!flat && hi == lo) {
            return Err(MathError::QuoteRowCannotOpen);
        }
        let share = (g.tokens.max(0.0) * N0 as f64) as u64;
        let tokens = if last { left } else { share.min(left) };
        left -= tokens;
        if flat {
            let m = flat_money_for_tokens(lo, tokens as u128, Round::Down).ok_or(MathError::QuoteRowCannotOpen)?;
            if kind == KIND_BONDING && m == 0 {
                return Err(MathError::QuoteRowCannotOpen);
            }
        } else if tokens > 0 {
            let l = liquidity_for_tokens(lo, hi, tokens as u128).ok_or(MathError::QuoteRowCannotOpen)?;
            if !last {
                money_between(l, lo, hi, Round::Up).ok_or(MathError::QuoteRowCannotOpen)?;
            }
        }
        out.push(QuoteRange { sqrt_a: lo, sqrt_b: hi, tokens, kind });
        lo = hi;
    }
    Ok(out)
}

pub fn quote_opens(row: &Row, quote_usd_cents_x64: u128, quote_decimals: u8, token_decimals: u8) -> Result<(), MathError> {
    ranges_at(row, quote_usd_cents_x64, quote_decimals, token_decimals).map(|_| ())
}

#[cfg(not(target_os = "solana"))]
pub fn book_at(row: &Row, quote_usd_cents_x64: u128, quote_decimals: u8, token_decimals: u8) -> Result<Book, MathError> {
    let rs = ranges_at(row, quote_usd_cents_x64, quote_decimals, token_decimals)?;
    let mut b = Book::empty(rs[0].sqrt_a, DEFAULT_FEE_PPM);
    for r in &rs {
        b.push_range(r.sqrt_a, r.sqrt_b, r.tokens, r.kind, 0)?;
    }
    b.check()?;
    Ok(b)
}
