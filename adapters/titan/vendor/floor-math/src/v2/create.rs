// SPDX-License-Identifier: Apache-2.0
use super::book::{Book, Undo, KIND_TAIL};
use super::preset::{off, range_at, RangeRow, RowV2, ROW_GLOBALS, ROW_LEN, SHARE_PPB_TOTAL};
#[cfg(not(target_os = "solana"))]
use super::preset::spec_ok;
#[cfg(not(target_os = "solana"))]
use super::quote::{TOKEN_DECIMALS_MAX, TOKEN_DECIMALS_MIN};
use super::split::SplitRow;
use super::supply::Supply;
use super::quote;
use super::sqrt::{flat_money_for_tokens, liquidity_for_tokens, money_between, Sqrt, SQRT_TAIL_END};
use super::trade::{buy_with, BookBuy};
use super::wide::{mul_div, mul_shr, Round};
use crate::u256::U256;
use crate::MathError;

pub const N0: u64 = 1_000_000_000_000_000;
pub const TOKENS_PER_PPB: u64 = N0 / SHARE_PPB_TOTAL as u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub now: i64,
    pub slot: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreateCtx {
    pub quote_decimals: u8,
    pub quote_usd_cents_x64: u128,
    pub token_decimals: u8,
    pub clock: Clock,
}

#[cfg(not(target_os = "solana"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Launch {
    pub book: Book,
    pub split: SplitRow,
    pub supply: Supply,
    pub sqrt_p0: Sqrt,
    pub raise: u128,
    pub team_buy_max: u128,
    pub created_ts: i64,
    pub created_slot: u64,
}

pub fn usd_cents_to_units(cents: u64, ctx: &CreateCtx) -> Result<u128, MathError> {
    if ctx.quote_usd_cents_x64 == 0 {
        return Err(MathError::Zero);
    }
    if ctx.quote_decimals > 30 {
        return Err(MathError::Overflow);
    }
    let pow = 10u128.pow(ctx.quote_decimals as u32);
    let n = U256::mul_u128(cents as u128, pow).checked_shl64().ok_or(MathError::Overflow)?;
    n.div_rem_u128(ctx.quote_usd_cents_x64).0.to_u128().ok_or(MathError::Overflow)
}

pub fn sqrt_p0_for(launch_cap_usd_cents: u64, ctx: &CreateCtx) -> Result<Sqrt, MathError> {
    quote::sqrt_p0(launch_cap_usd_cents, ctx.quote_usd_cents_x64, ctx.quote_decimals)
}

pub fn team_buy_bounds(row: &RowV2, ctx: &CreateCtx) -> Result<(u128, u128), MathError> {
    let cents = row.raise_usd_cents as u128 * row.team_buy_share_bps as u128 / 10_000;
    Ok((0, usd_cents_to_units(cents as u64, ctx)?))
}

fn edges(sqrt_p0: Sqrt, g: &RangeRow) -> Result<(Sqrt, Sqrt), MathError> {
    let no = MathError::QuoteRowCannotOpen;
    let at = |rel| match mul_shr(sqrt_p0, rel, 64, Round::Down) {
        Some(s) if s > 0 && s < SQRT_TAIL_END => Ok(s),
        _ => Err(no),
    };
    let a = at(g.sqrt_a_rel)?;
    let b = if g.kind == KIND_TAIL {
        SQRT_TAIL_END
    } else if g.sqrt_b_rel == g.sqrt_a_rel {
        a
    } else {
        at(g.sqrt_b_rel)?
    };
    if g.sqrt_b_rel != g.sqrt_a_rel && b <= a {
        return Err(no);
    }
    Ok((a, b))
}

fn reset(book: &mut Book, sqrt_p0: Sqrt, fee: &RowFees) {
    let Book {
        sqrt_p, sqrt_f, sqrt_l_max, floor, floor_tokens, n_bands, band_edge, band_money, band_tokens, n_c, band_f, band_f_claim, ck_at_f,
        ck_floor_tokens,
        fee_ppm, fee_start_ppm, sqrt_fee_a, sqrt_fee_b, graduated, own_fees, n_ranges, ranges,
    } = book;
    (*sqrt_p, *sqrt_f, *sqrt_l_max, *floor, *floor_tokens, *n_bands) = (sqrt_p0, 0, 0, 0, 0, 0);
    (*n_c, *ck_at_f, *ck_floor_tokens) = (0, 0, 0);
    band_f.iter_mut().for_each(|x| *x = 0);
    band_f_claim.iter_mut().for_each(|x| *x = 0);
    band_edge.iter_mut().for_each(|x| *x = 0);
    band_money.iter_mut().for_each(|x| *x = 0);
    band_tokens.iter_mut().for_each(|x| *x = 0);
    (*fee_ppm, *graduated, *own_fees, *n_ranges) = (fee.end, false, false, 0);
    *fee_start_ppm = fee.start;
    let knot = |rel: u64| mul_div(sqrt_p0, rel as u128, 1 << 32, Round::Down).unwrap_or(0);
    (*sqrt_fee_a, *sqrt_fee_b) = (knot(fee.a_rel), knot(fee.b_rel));
    ranges.iter_mut().for_each(|r| *r = Default::default());
}

struct RowFees {
    end: [u32; 5],
    start: [u32; 5],
    a_rel: u64,
    b_rel: u64,
}

fn build_into(book: &mut Book, fee: &RowFees, n: usize, sqrt_p0: Sqrt, range: impl Fn(usize) -> RangeRow) -> Result<u128, MathError> {
    reset(book, sqrt_p0, fee);
    let mut left = N0;
    for i in 0..n {
        let g = range(i);
        let (a, b) = edges(sqrt_p0, &g)?;
        let mut t = (g.share_ppb as u64).checked_mul(TOKENS_PER_PPB).ok_or(MathError::Overflow)?.min(left);
        if g.kind == KIND_TAIL {
            t = left;
        }
        if a != b && t > 0 {
            let l = liquidity_for_tokens(a, b, t as u128).ok_or(MathError::QuoteRowCannotOpen)?;
            if g.kind != KIND_TAIL {
                money_between(l, a, b, Round::Up).ok_or(MathError::QuoteRowCannotOpen)?;
            }
        }
        book.push_range(a, b, t, g.kind, g.class)?;
        let r = &mut book.ranges[i];
        r.theta_bps = g.theta_bps;
        r.psi_bps = g.psi_bps;
        r.ms_cap_ppm = g.ms_cap_ppm;
        left -= t;
    }
    if n == 0 || book.ranges[n - 1].kind != KIND_TAIL || left != 0 {
        return Err(MathError::RowTail);
    }
    book.check()?;
    let r0 = &book.ranges[0];
    if r0.sqrt_a == r0.sqrt_b && flat_money_for_tokens(r0.sqrt_a, r0.tokens as u128, Round::Down) == Some(0) {
        return Err(MathError::QuoteRowCannotOpen);
    }
    flat_money_for_tokens(r0.sqrt_a, r0.tokens as u128, Round::Up).ok_or(MathError::QuoteRowCannotOpen)
}

pub fn create_book(book: &mut Book, row: &RowV2, sqrt_p0: Sqrt) -> Result<u128, MathError> {
    let fee = RowFees { end: row.fee_ppm, start: row.fee_start_ppm, a_rel: row.fee_a_rel, b_rel: row.fee_b_rel };
    build_into(book, &fee, row.n_ranges as usize, sqrt_p0, |i| row.ranges[i])
}

pub fn create_book_from_bytes(book: &mut Book, b: &[u8; ROW_LEN], sqrt_p0: Sqrt) -> Result<u128, MathError> {
    let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let u64_at = |o: usize| u64::from_le_bytes(core::array::from_fn(|i| b[o + i]));
    let mut fee = RowFees { end: [0; 5], start: [0; 5], a_rel: u64_at(off::FEE_A), b_rel: u64_at(off::FEE_B) };
    for i in 0..5 {
        fee.end[i] = u32_at(off::FEE_PPM + 4 * i);
        fee.start[i] = u32_at(off::FEE_START + 4 * i);
    }
    let n = b[off::N_RANGES] as usize;
    if n * super::preset::RANGE_ROW + ROW_GLOBALS > ROW_LEN {
        return Err(MathError::RowTooManyRanges);
    }
    build_into(book, &fee, n, sqrt_p0, |i| range_at(b, i))
}

pub fn team_buy_into(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, max: u128, u: &mut Undo) -> Result<BookBuy, MathError> {
    let take = gross.min(max);
    if take == 0 {
        return Ok(BookBuy { gross, refund: gross, sqrt_before: book.sqrt_p, sqrt_after: book.sqrt_p, ..BookBuy::default() });
    }
    let mut r = buy_with(book, row, sup, take, 0, u)?;
    r.refund += gross - take;
    r.gross = gross;
    Ok(r)
}

#[cfg(not(target_os = "solana"))]
pub fn create(row: &RowV2, ctx: &CreateCtx) -> Result<Launch, MathError> {
    spec_ok(row)?;
    quote_opens(row, ctx)?;
    let sqrt_p0 = sqrt_p0_for(row.launch_cap_usd_cents, ctx)?;
    let mut book = Book::empty(sqrt_p0, row.fee_ppm);
    let raise = create_book(&mut book, row, sqrt_p0)?;
    let (_, team_buy_max) = team_buy_bounds(row, ctx)?;
    Ok(Launch {
        book,
        split: row.split_row(),
        supply: Supply { live: N0, dead: 0 },
        sqrt_p0,
        raise,
        team_buy_max,
        created_ts: ctx.clock.now,
        created_slot: ctx.clock.slot,
    })
}

#[cfg(not(target_os = "solana"))]
pub fn quote_opens(row: &RowV2, ctx: &CreateCtx) -> Result<(), MathError> {
    if !(TOKEN_DECIMALS_MIN..=TOKEN_DECIMALS_MAX).contains(&ctx.token_decimals) {
        return Err(MathError::QuoteRowCannotOpen);
    }
    let sqrt_p0 = sqrt_p0_for(row.launch_cap_usd_cents, ctx)?;
    let mut book = Book::empty(sqrt_p0, row.fee_ppm);
    create_book(&mut book, row, sqrt_p0).map(|_| ())
}

#[cfg(not(target_os = "solana"))]
pub fn create_team_buy(l: &mut Launch, gross: u128, clock: Clock) -> Result<BookBuy, MathError> {
    if clock.now != l.created_ts || l.book.sqrt_p != l.sqrt_p0 || l.book.ranges[0].ask_left != l.book.ranges[0].tokens {
        return Err(MathError::TeamBuyOverShare);
    }
    let mut u = Undo::EMPTY;
    team_buy_into(&mut l.book, &l.split, l.supply, gross, l.team_buy_max, &mut u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::preset::from_design;
    use crate::v2::rows;

    const USDC: CreateCtx = CreateCtx { quote_decimals: 6, quote_usd_cents_x64: 100 << 64, token_decimals: 6, clock: Clock { now: 1_700_000_000, slot: 1 } };

    fn cap_of(l: &Launch) -> f64 {
        let p = (l.book.sqrt_p as f64 / 2f64.powi(72)).powi(2);
        p * N0 as f64 / 1e6
    }

    #[test]
    fn reset_is_book_empty() {
        use crate::v2::trade;
        let row = from_design(&rows::moonshot()).unwrap();
        let mut l = create(&row, &USDC).unwrap();
        let r = trade::buy(&mut l.book, &l.split, l.supply, l.raise * 3, 0);
        assert!(r.is_ok() && l.book != Book::empty(1, [7; 5]));
        reset(&mut l.book, 1, &RowFees { end: [7; 5], start: [0; 5], a_rel: 0, b_rel: 0 });
        assert_eq!(l.book, Book::empty(1, [7; 5]));
    }

    #[test]
    fn theta_is_row_data_end_to_end() {
        let mut d = rows::moonshot();
        d.theta_bps = 1_500;
        d.stacks.pop();
        let row = from_design(&d).unwrap();
        let back = crate::v2::preset::row_from_bytes(&crate::v2::preset::row_to_bytes(&row)).unwrap();
        let l = create(&back, &USDC).unwrap();
        for (g, r) in back.ranges().iter().zip(l.book.ranges.iter()) {
            assert_eq!(r.theta_bps, g.theta_bps);
        }
        let mut base = from_design(&rows::moonshot()).unwrap();
        assert!(back.ranges().iter().zip(base.ranges.iter_mut()).any(|(a, b)| a.theta_bps != b.theta_bps), "θ moved");
    }

    #[test]
    fn create_agrees_with_quote_on_the_a9_11_grid() {
        for d in [rows::wagmi(), rows::moonshot(), rows::floor_genesis(), rows::wagmi_genesis(1_000_000)] {
            let row = from_design(&d).unwrap();
            for qd in [6u8, 8, 9, 18] {
                for k in -4i32..=5 {
                    let q = (10f64.powi(k) * 100.0 * 18_446_744_073_709_551_616.0) as u128;
                    for td in [6u8, 18] {
                        let ctx = CreateCtx { quote_decimals: qd, quote_usd_cents_x64: q, token_decimals: td, clock: Clock::default() };
                        let want = quote::ranges_at(&d, q, qd, td);
                        let got = quote_opens(&row, &ctx);
                        assert_eq!(got.is_ok(), want.is_ok(), "{} qd {qd} 1e{k} td {td}: {got:?} vs {:?}", d.name, want.as_ref().err());
                        let Ok(want) = want else { continue };
                        let l = create(&row, &ctx).unwrap();
                        assert_eq!(l.sqrt_p0, quote::sqrt_p0(row.launch_cap_usd_cents, q, qd).unwrap());
                        let want: Vec<_> = want.into_iter().filter(|w| !(w.sqrt_a == w.sqrt_b && w.tokens == 0)).collect();
                        assert_eq!(want.len(), l.book.n_ranges as usize, "{}", d.name);
                        for (w, r) in want.iter().zip(l.book.ranges.iter()) {
                            assert_eq!((w.sqrt_a, w.sqrt_b, w.kind), (r.sqrt_a, r.sqrt_b, r.kind), "{} qd {qd} 1e{k}", d.name);
                            assert!(w.tokens.abs_diff(r.tokens) <= 2 * TOKENS_PER_PPB * 48, "{}: tokens {} vs {}", d.name, w.tokens, r.tokens);
                        }
                    }
                }
            }
        }
        let bad = CreateCtx { token_decimals: 5, ..USDC };
        assert_eq!(quote_opens(&from_design(&rows::wagmi()).unwrap(), &bad), Err(MathError::QuoteRowCannotOpen));
    }

    #[test]
    fn create_opens_the_whole_market_at_the_launch_cap() {
        for d in [rows::wagmi(), rows::moonshot(), rows::floor_genesis(), rows::wagmi_genesis(1_000_000)] {
            let row = from_design(&d).unwrap();
            let l = create(&row, &USDC).unwrap();
            let b = &l.book;
            assert_eq!(b.n_ranges, row.n_ranges);
            let total: u64 = b.ranges[..b.n_ranges as usize].iter().map(|r| r.tokens).sum();
            assert_eq!(total, N0, "{}: Σ tokens = N0", d.name);
            assert_eq!(b.for_sale(), N0);
            assert!((cap_of(&l) / d.launch_cap as f64 - 1.0).abs() < 1e-9, "{}: p0 at the cap", d.name);
            assert_eq!(b.ranges[0].sqrt_a, l.sqrt_p0);
            let raise = d.bonding_raise as f64 * 1e6;
            assert!((l.raise as f64 / raise - 1.0).abs() < 1e-6, "{}: raise {} vs {raise}", d.name, l.raise);
        }
    }

    #[test]
    fn team_buy_bounds_and_the_buy() {
        let row = from_design(&rows::moonshot()).unwrap();
        let (lo, hi) = team_buy_bounds(&row, &USDC).unwrap();
        assert_eq!((lo, hi), (0, 2_000_000_000));
        let mut l = create(&row, &USDC).unwrap();
        let r = create_team_buy(&mut l, hi + 5, USDC.clock).unwrap();
        assert_eq!(r.gross, hi + 5);
        assert_eq!(r.net + r.fees.iter().sum::<u128>() + r.refund, hi + 5);
        assert!(r.refund >= 5);
        assert!(r.tokens > 0 && r.tokens <= l.book.ranges[0].tokens);
        assert_eq!(create_team_buy(&mut l, 1, USDC.clock), Err(MathError::TeamBuyOverShare));
        let mut l = create(&row, &USDC).unwrap();
        let before = l.book;
        let z = create_team_buy(&mut l, 0, USDC.clock).unwrap();
        assert_eq!((z.tokens, z.net, z.refund), (0, 0, 0));
        assert_eq!(l.book, before);
    }

    #[test]
    fn create_from_bytes_equals_create_from_the_row() {
        let row = from_design(&rows::wagmi()).unwrap();
        let bytes = crate::v2::preset::row_to_bytes(&row);
        let l = create(&row, &USDC).unwrap();
        let mut b = Book::empty(0, [0; 5]);
        let raise = create_book_from_bytes(&mut b, &bytes, l.sqrt_p0).unwrap();
        assert_eq!((b, raise), (l.book, l.raise));
    }

    #[test]
    fn an_eighteen_decimal_quote_opens_too() {
        let ctx = CreateCtx { quote_decimals: 18, quote_usd_cents_x64: (1u128 << 64) / 100, token_decimals: 18, clock: Clock::default() };
        let row = from_design(&rows::wagmi()).unwrap();
        let l = create(&row, &ctx).unwrap();
        assert_eq!(l.book.ranges[..l.book.n_ranges as usize].iter().map(|r| r.tokens).sum::<u64>(), N0);
    }

    fn pregrad_legs(money: u128) -> [u128; 5] {
        [0, 0, (money * 4_000).div_ceil(1_000_000), (money * 1_000).div_ceil(1_000_000), 0]
    }

    #[test]
    fn a12_before_graduation_a_buy_and_a_sell_pay_platform_and_floor_only() {
        use crate::v2::fees::{fee_total, PREGRAD_PPM};
        use crate::v2::trade;
        assert_eq!(PREGRAD_PPM, [0, 0, 4_000, 1_000, 0]);
        for design in [rows::wagmi(), rows::moonshot()] {
            let row = from_design(&design).unwrap();
            let mut l = create(&row, &USDC).unwrap();
            assert!(!l.book.graduated && !l.book.own_fees, "{}", design.name);
            assert_eq!(l.book.fees_now(), PREGRAD_PPM, "{}: the open's rates", design.name);
            let b = trade::buy(&mut l.book, &l.split, l.supply, l.raise / 4, 0).unwrap();
            assert!(!l.book.graduated && b.tokens > 0);
            assert_eq!(b.fees, pregrad_legs(b.net), "{}: the buy", design.name);
            assert_eq!(b.net + fee_total(&b.fees) + b.refund, b.gross);
            let s = trade::sell(&mut l.book, &l.split, l.supply, b.tokens / 2, 0).unwrap();
            assert!(!l.book.graduated && s.unfilled == 0);
            assert_eq!(s.fees, pregrad_legs(s.gross), "{}: the sell", design.name);
            assert_eq!(s.out, s.gross - fee_total(&s.fees));
        }
    }

    #[test]
    fn a12_the_first_trade_after_graduation_pays_the_curve() {
        use crate::v2::fees::{ppm_at, PREGRAD_PPM};
        use crate::v2::trade;
        for design in [rows::wagmi(), rows::moonshot()] {
            let row = from_design(&design).unwrap();
            let mut l = create(&row, &USDC).unwrap();
            let g = trade::buy(&mut l.book, &l.split, l.supply, l.raise * 2, 0).unwrap();
            assert!(g.graduated_now && l.book.graduated, "{}", design.name);
            let bk = &l.book;
            let curve = ppm_at(&bk.fee_ppm, &bk.fee_start_ppm, bk.sqrt_fee_a, bk.sqrt_fee_b, bk.sqrt_p);
            assert_eq!(bk.fees_now(), curve, "{}", design.name);
            assert_ne!(curve, PREGRAD_PPM);
            assert!(curve[0] > 0, "{}: Backing from graduation", design.name);
            let b = trade::buy(&mut l.book, &l.split, l.supply, l.raise / 10, 0).unwrap();
            assert!(b.fees[0] > 0, "{}: the Backing Fee is charged", design.name);
            if row.fee_start_ppm[1] > 0 {
                assert!(b.fees[1] > 0, "{}: the Team Fee is charged", design.name);
            }
            for i in 0..5 {
                assert!(b.fees[i] <= (b.net * curve[i] as u128).div_ceil(1_000_000), "{}: leg {i} at most its start rate", design.name);
            }
            let s = trade::sell(&mut l.book, &l.split, l.supply, b.tokens / 2, 0).unwrap();
            assert!(s.fees[0] > 0, "{}: a sell after graduation pays Backing", design.name);
        }
    }

    #[test]
    fn a12_the_genesis_rows_keep_their_own_fees_before_graduation() {
        use crate::v2::fees::{fee_legs, ppm_at};
        use crate::v2::trade;
        for (design, start) in [(rows::floor_genesis(), [3_000, 0, 2_000, 0, 0]), (rows::wagmi_genesis(10_000), [6_000, 0, 2_000, 500, 0])] {
            let row = from_design(&design).unwrap();
            assert_eq!(row.fee_start_ppm, start, "{}: the row's start rates as built", design.name);
            let mut l = create(&row, &USDC).unwrap();
            l.book.own_fees = true;
            let bk = &l.book;
            let curve = ppm_at(&bk.fee_ppm, &bk.fee_start_ppm, bk.sqrt_fee_a, bk.sqrt_fee_b, bk.sqrt_p);
            assert_eq!((bk.fees_now(), curve), (start, start), "{}: before graduation, the row's own rates", design.name);
            let b = trade::buy(&mut l.book, &l.split, l.supply, l.raise / 4, 0).unwrap();
            assert!(!l.book.graduated);
            assert_eq!(b.fees, fee_legs(b.net, &start), "{}", design.name);
        }
    }

    #[test]
    fn the_market_trades_after_create() {
        use crate::v2::trade;
        let row = from_design(&rows::moonshot()).unwrap();
        let mut l = create(&row, &USDC).unwrap();
        let rb = trade::buy(&mut l.book, &l.split, l.supply, l.raise * 2, 0).unwrap();
        assert!(rb.tokens > l.book.ranges[0].tokens && l.book.graduated);
        let s = trade::sell(&mut l.book, &l.split, l.supply, rb.tokens / 2, 0).unwrap();
        assert!(s.out > 0);
    }
}

