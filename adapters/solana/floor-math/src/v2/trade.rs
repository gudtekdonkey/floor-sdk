// SPDX-License-Identifier: Apache-2.0
use super::book::{Book, Undo};
use super::book::{WalkUp, MAX_WALK_STEPS};
use super::marginal::Point;
use super::fees::{cheaper, fee_total, net_from_gross, FeeAcc, Lg, FEE_LEGS, LEG_BACKING};
use super::relay::{relay, Relay};
use super::split::{split, Split, SplitIn, SplitRow};
use super::sqrt::Sqrt;
use super::supply::{s_circ, Supply};
use crate::MathError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookBuy {
    pub gross: u128,
    pub net: u128,
    pub refund: u128,
    pub tokens: u64,
    pub fees: [u128; FEE_LEGS],
    pub from_resale: u64,
    pub from_first: u64,
    pub steps: u8,
    pub budget_hit: bool,
    pub graduated_now: bool,
    pub bonding_money: u128,
    pub sqrt_before: Sqrt,
    pub sqrt_after: Sqrt,
    pub split: Split,
    pub relay: Relay,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookSell {
    pub tokens_in: u64,
    pub unfilled: u64,
    pub gross: u128,
    pub fees: [u128; FEE_LEGS],
    pub out: u128,
    pub steps: u8,
    pub sqrt_before: Sqrt,
    pub sqrt_after: Sqrt,
    pub from_f: u128,
    pub from_k: u128,
    pub from_c: u128,
    pub relay: Relay,
}

#[inline(never)]
pub fn buy_with(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min_tokens: u64, u: &mut Undo) -> Result<BookBuy, MathError> {
    buy_steps_with(book, row, sup, gross, min_tokens, MAX_WALK_STEPS, u)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub fn buy_steps_with(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min_tokens: u64, walk_steps: u8, u: &mut Undo) -> Result<BookBuy, MathError> {
    let acc = book.fee_acc();
    buy_acc(book, row, sup, gross, min_tokens, walk_steps, u, acc)
}

#[allow(clippy::too_many_arguments)]
pub fn buy_steps_with_memo<M: Lg + ?Sized>(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min_tokens: u64, walk_steps: u8, u: &mut Undo, memo: &mut M) -> Result<BookBuy, MathError> {
    let acc = book.fee_acc_with(memo);
    buy_acc(book, row, sup, gross, min_tokens, walk_steps, u, acc)
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn buy_acc<M: Lg>(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min_tokens: u64, walk_steps: u8, u: &mut Undo, acc: FeeAcc<M>) -> Result<BookBuy, MathError> {
    let ppm = acc.pre;
    let budget = net_from_gross(gross, &ppm);
    if budget == 0 {
        return Err(MathError::NothingToDo);
    }
    let before = book.sqrt_p;
    u.start(book);
    let r = buy_inner(book, row, sup, gross, min_tokens, budget, acc, before, walk_steps, u);
    if r.is_err() {
        u.rollback(book);
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn buy_inner<M: Lg>(b: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min: u64, budget: u128, mut acc: FeeAcc<M>, before: Sqrt, walk_steps: u8, u: &mut Undo) -> Result<BookBuy, MathError> {
    let ppm = acc.pre;
    let mut walked = b.walk_up_from(budget, WalkUp::default(), &mut acc, walk_steps, u)?;
    let c = cheaper(&ppm, &b.fees_at_memo(b.sqrt_p, &mut acc.memo));
    if c != ppm && walked.tokens > 0 {
        let spent = walked.money.checked_add(fee_total(&acc.legs())).ok_or(MathError::Overflow)?;
        let budget2 = net_from_gross(gross.checked_sub(spent).ok_or(MathError::Overflow)?, &c);
        if budget2 > 0 {
            walked = b.walk_up_from(budget2, walked, &mut acc, walk_steps, u)?;
        }
    }
    if walked.tokens == 0 {
        return Err(if b.tokens_held() == 0 { MathError::BookEmpty } else { MathError::NothingToDo });
    }
    if walked.tokens < min {
        return Err(MathError::SlippageExceeded);
    }
    let fees = acc.legs();
    let paid = walked.money.checked_add(fee_total(&fees)).ok_or(MathError::Overflow)?;
    let refund = gross.checked_sub(paid).ok_or(MathError::Overflow)?;
    b.floor = add(b.floor, fees[LEG_BACKING])?;
    let (mut sp, mut rl) = (Split::default(), Relay::default());
    if b.graduated {
        let s_new = s_circ(b, sup)?;
        let inp = SplitIn {
            sqrt_f: b.sqrt_f,
            sqrt_p1: b.sqrt_p,
            f_total: b.f_own(),
            f_claims: b.f_claims(),
            ck0: b.ck_money(),
            held: b.ck_held(),
            s_new,
        };
        if !u.bought().is_empty() {
            sp = split(row, &b.ranges[..b.n_ranges as usize], &inp, u.bought())?;
            b.floor = add(b.floor, sp.d_f)?;
            b.sqrt_f = sp.sqrt_f_new;
        }
        rl = relay(b, row, sp.ck_add, sp.ck_take, s_new)?;
    }
    Ok(BookBuy {
        gross,
        net: walked.money,
        refund,
        tokens: walked.tokens,
        fees,
        from_resale: walked.from_resale,
        from_first: walked.from_first,
        steps: walked.steps,
        budget_hit: walked.budget_hit,
        graduated_now: walked.graduated_now,
        bonding_money: walked.bonding_money,
        sqrt_before: before,
        sqrt_after: b.sqrt_p,
        split: sp,
        relay: rl,
    })
}

fn add(a: u128, b: u128) -> Result<u128, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

#[inline(never)]
pub fn sell_with(book: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, min_out: u128, u: &mut Undo) -> Result<BookSell, MathError> {
    sell_memo(book, row, sup, tokens, min_out, u, super::fees::LgMemo::default())
}

pub fn sell_with_memo<M: Lg + ?Sized>(book: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, min_out: u128, u: &mut Undo, memo: &mut M) -> Result<BookSell, MathError> {
    sell_memo(book, row, sup, tokens, min_out, u, memo)
}

#[inline(always)]
fn sell_memo<M: Lg>(book: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, min_out: u128, u: &mut Undo, memo: M) -> Result<BookSell, MathError> {
    if tokens == 0 {
        return Err(MathError::NothingToDo);
    }
    let before = book.sqrt_p;
    u.start(book);
    let r = sell_inner(book, row, sup, tokens, min_out, before, u, memo);
    if r.is_err() {
        u.rollback(book);
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn sell_inner<M: Lg>(b: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, min: u128, before: Sqrt, u: &mut Undo, memo: M) -> Result<BookSell, MathError> {
    let mut acc = b.fee_acc_with(memo);
    let walked = b.walk_down_acc(tokens, &mut acc, u)?;
    let fees = acc.legs();
    let total = fee_total(&fees);
    if walked.tokens == 0 || walked.money <= total {
        return Err(if walked.money == 0 && b.money_held() == 0 {
            MathError::BookEmpty
        } else {
            MathError::NothingToDo
        });
    }
    let out = walked.money - total;
    if out < min {
        return Err(MathError::SlippageExceeded);
    }
    b.floor = add(b.floor, fees[LEG_BACKING])?;
    let rl = if b.graduated { relay(b, row, 0, 0, s_circ(b, sup)?)? } else { Relay::default() };
    Ok(BookSell {
        tokens_in: tokens,
        unfilled: tokens - walked.tokens,
        gross: walked.money,
        fees,
        out,
        steps: walked.steps,
        sqrt_before: before,
        sqrt_after: b.sqrt_p,
        from_f: walked.from_f,
        from_k: walked.from_k,
        from_c: walked.from_c,
        relay: rl,
    })
}

#[cfg(not(target_os = "solana"))]
pub fn buy(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, min_tokens: u64) -> Result<BookBuy, MathError> {
    let mut u = Undo::EMPTY;
    buy_with(book, row, sup, gross, min_tokens, &mut u)
}

#[cfg(not(target_os = "solana"))]
pub fn sell(book: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, min_out: u128) -> Result<BookSell, MathError> {
    let mut u = Undo::EMPTY;
    sell_with(book, row, sup, tokens, min_out, &mut u)
}

pub fn quote_buy_mut(book: &mut Book, row: &SplitRow, sup: Supply, gross: u128, u: &mut Undo) -> Result<BookBuy, MathError> {
    let r = buy_with(book, row, sup, gross, 0, u);
    if r.is_ok() {
        u.rollback(book);
    }
    r
}

pub fn quote_sell_mut(book: &mut Book, row: &SplitRow, sup: Supply, tokens: u64, u: &mut Undo) -> Result<BookSell, MathError> {
    let r = sell_with(book, row, sup, tokens, 0, u);
    if r.is_ok() {
        u.rollback(book);
    }
    r
}

#[cfg(not(target_os = "solana"))]
pub fn quote_buy(book: &Book, row: &SplitRow, sup: Supply, gross: u128) -> Result<BookBuy, MathError> {
    let mut c = *book;
    buy(&mut c, row, sup, gross, 0)
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell(book: &Book, row: &SplitRow, sup: Supply, tokens: u64) -> Result<BookSell, MathError> {
    let mut c = *book;
    sell(&mut c, row, sup, tokens, 0)
}

#[cfg(not(target_os = "solana"))]
pub fn quote_buy_with_memo<M: Lg + ?Sized>(book: &Book, row: &SplitRow, sup: Supply, gross: u128, memo: &mut M) -> Result<BookBuy, MathError> {
    let mut c = *book;
    let mut u = Undo::EMPTY;
    buy_steps_with_memo(&mut c, row, sup, gross, 0, MAX_WALK_STEPS, &mut u, memo)
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell_with_memo<M: Lg + ?Sized>(book: &Book, row: &SplitRow, sup: Supply, tokens: u64, memo: &mut M) -> Result<BookSell, MathError> {
    let mut c = *book;
    let mut u = Undo::EMPTY;
    sell_with_memo(&mut c, row, sup, tokens, 0, &mut u, memo)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuyQuote {
    pub gross: u128,
    pub net: u128,
    pub refund: u128,
    pub tokens: u64,
    pub fees: [u128; FEE_LEGS],
    pub from_resale: u64,
    pub from_first: u64,
    pub steps: u8,
    pub budget_hit: bool,
    pub graduated_now: bool,
    pub bonding_money: u128,
    pub sqrt_before: Sqrt,
    pub sqrt_after: Sqrt,
    pub next: Option<Point>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SellQuote {
    pub tokens_in: u64,
    pub unfilled: u64,
    pub gross: u128,
    pub fees: [u128; FEE_LEGS],
    pub out: u128,
    pub steps: u8,
    pub sqrt_before: Sqrt,
    pub sqrt_after: Sqrt,
    pub from_f: u128,
    pub from_k: u128,
    pub from_c: u128,
    pub next: Option<Point>,
}

#[cfg(not(target_os = "solana"))]
std::thread_local! {
    static QUOTE_UNDO: core::cell::RefCell<Undo> = const { core::cell::RefCell::new(Undo::EMPTY) };
}

#[cfg(not(target_os = "solana"))]
fn with_quote_undo<R>(f: impl FnOnce(&mut Undo) -> R) -> R {
    QUOTE_UNDO.with(|c| match c.try_borrow_mut() {
        Ok(mut u) => f(&mut u),
        Err(_) => {
            let mut fresh = Undo::EMPTY;
            f(&mut fresh)
        }
    })
}

#[cfg(not(target_os = "solana"))]
pub fn quote_buy_lite<M: Lg + ?Sized>(book: &Book, row: &SplitRow, sup: Supply, gross: u128, walk_steps: u8, point: bool, memo: &mut M) -> Result<BuyQuote, MathError> {
    with_quote_undo(|u| quote_buy_lite_in(book, None, row, sup, gross, walk_steps, point, memo, u))
}

#[cfg(not(target_os = "solana"))]
pub fn quote_buy_cp<M: Lg + ?Sized>(cps: &super::book::WalkCheckpoints, row: &SplitRow, sup: Supply, gross: u128, walk_steps: u8, point: bool, memo: &mut M) -> Result<BuyQuote, MathError> {
    with_quote_undo(|u| quote_buy_lite_in(cps.book(), Some(cps), row, sup, gross, walk_steps, point, memo, u))
}

#[cfg(not(target_os = "solana"))]
#[allow(clippy::too_many_arguments)]
fn quote_buy_lite_in<M: Lg + ?Sized>(book: &Book, cps: Option<&super::book::WalkCheckpoints>, row: &SplitRow, sup: Supply, gross: u128, walk_steps: u8, point: bool, memo: &mut M, u: &mut Undo) -> Result<BuyQuote, MathError> {
    let mut b = *book;
    let mut acc = match cps {
        Some(cps) => FeeAcc::with_memo(cps.pre(), memo),
        None => b.fee_acc_with(memo),
    };
    let ppm = acc.pre;
    let budget = net_from_gross(gross, &ppm);
    if budget == 0 {
        return Err(MathError::NothingToDo);
    }
    let before = b.sqrt_p;
    u.start(&b);
    let mut walked = match cps {
        Some(cps) => cps.walk_up(&mut b, budget, &mut acc, walk_steps, u)?,
        None => b.walk_up_from(budget, WalkUp::default(), &mut acc, walk_steps, u)?,
    };
    let c = cheaper(&ppm, &b.fees_at_memo(b.sqrt_p, &mut acc.memo));
    if c != ppm && walked.tokens > 0 {
        let spent = walked.money.checked_add(fee_total(&acc.legs())).ok_or(MathError::Overflow)?;
        let budget2 = net_from_gross(gross.checked_sub(spent).ok_or(MathError::Overflow)?, &c);
        if budget2 > 0 {
            walked = b.walk_up_from(budget2, walked, &mut acc, walk_steps, u)?;
        }
    }
    if walked.tokens == 0 {
        return Err(if b.tokens_held() == 0 { MathError::BookEmpty } else { MathError::NothingToDo });
    }
    let fees = acc.legs();
    let paid = walked.money.checked_add(fee_total(&fees)).ok_or(MathError::Overflow)?;
    let refund = gross.checked_sub(paid).ok_or(MathError::Overflow)?;
    b.floor = add(b.floor, fees[LEG_BACKING])?;
    if b.graduated {
        let s_new = s_circ(&b, sup)?;
        if point && !super::marginal::range_sells_at(&b, b.sqrt_p) {
            let inp = SplitIn {
                sqrt_f: b.sqrt_f,
                sqrt_p1: b.sqrt_p,
                f_total: b.f_own(),
                f_claims: b.f_claims(),
                ck0: b.ck_money(),
                held: b.ck_held(),
                s_new,
            };
            let mut sp = Split::default();
            if !u.bought().is_empty() {
                sp = split(row, &b.ranges[..b.n_ranges as usize], &inp, u.bought())?;
                b.floor = add(b.floor, sp.d_f)?;
                b.sqrt_f = sp.sqrt_f_new;
            }
            relay(&mut b, row, sp.ck_add, sp.ck_take, s_new)?;
        }
    }
    let next = if point { super::marginal::buy_point_with(&b, &mut acc.memo) } else { None };
    Ok(BuyQuote {
        gross,
        net: walked.money,
        refund,
        tokens: walked.tokens,
        fees,
        from_resale: walked.from_resale,
        from_first: walked.from_first,
        steps: walked.steps,
        budget_hit: walked.budget_hit,
        graduated_now: walked.graduated_now,
        bonding_money: walked.bonding_money,
        sqrt_before: before,
        sqrt_after: b.sqrt_p,
        next,
    })
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell_lite<M: Lg + ?Sized>(book: &Book, row: &SplitRow, sup: Supply, tokens: u64, point: bool, memo: &mut M) -> Result<SellQuote, MathError> {
    if tokens == 0 {
        return Err(MathError::NothingToDo);
    }
    let next = if point { SellNext::Relaid } else { SellNext::None };
    with_quote_undo(|u| quote_sell_lite_in(book, None, row, sup, tokens, next, memo, u))
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell_cp<M: Lg + ?Sized>(cps: &super::book::WalkCheckpoints, row: &SplitRow, sup: Supply, tokens: u64, point: bool, memo: &mut M) -> Result<SellQuote, MathError> {
    if tokens == 0 {
        return Err(MathError::NothingToDo);
    }
    let next = if point { SellNext::Relaid } else { SellNext::None };
    with_quote_undo(|u| quote_sell_lite_in(cps.book(), Some(cps), row, sup, tokens, next, memo, u))
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell_lite_end<M: Lg + ?Sized>(book: &Book, row: &SplitRow, sup: Supply, tokens: u64, memo: &mut M) -> Result<SellQuote, MathError> {
    if tokens == 0 {
        return Err(MathError::NothingToDo);
    }
    with_quote_undo(|u| quote_sell_lite_in(book, None, row, sup, tokens, SellNext::WalkEnd, memo, u))
}

#[cfg(not(target_os = "solana"))]
pub fn quote_sell_cp_end<M: Lg + ?Sized>(cps: &super::book::WalkCheckpoints, row: &SplitRow, sup: Supply, tokens: u64, memo: &mut M) -> Result<SellQuote, MathError> {
    if tokens == 0 {
        return Err(MathError::NothingToDo);
    }
    with_quote_undo(|u| quote_sell_lite_in(cps.book(), Some(cps), row, sup, tokens, SellNext::WalkEnd, memo, u))
}

#[cfg(not(target_os = "solana"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SellNext {
    None,
    Relaid,
    WalkEnd,
}

#[cfg(not(target_os = "solana"))]
#[allow(clippy::too_many_arguments)]
fn quote_sell_lite_in<M: Lg + ?Sized>(book: &Book, cps: Option<&super::book::WalkCheckpoints>, row: &SplitRow, sup: Supply, tokens: u64, want: SellNext, memo: &mut M, u: &mut Undo) -> Result<SellQuote, MathError> {
    let mut b = *book;
    let before = b.sqrt_p;
    u.start(&b);
    let mut acc = match cps {
        Some(cps) => FeeAcc::with_memo(cps.pre(), memo),
        None => b.fee_acc_with(memo),
    };
    let walked = match cps {
        Some(cps) => cps.walk_down(&mut b, tokens, &mut acc, u)?,
        None => b.walk_down_acc(tokens, &mut acc, u)?,
    };
    let fees = acc.legs();
    let total = fee_total(&fees);
    if walked.tokens == 0 || walked.money <= total {
        return Err(if walked.money == 0 && b.money_held() == 0 { MathError::BookEmpty } else { MathError::NothingToDo });
    }
    let out = walked.money - total;
    let end = if want == SellNext::WalkEnd { super::marginal::sell_point_with(&b, &mut acc.memo) } else { None };
    b.floor = add(b.floor, fees[LEG_BACKING])?;
    if b.graduated {
        let circ = s_circ(&b, sup)?;
        if want == SellNext::Relaid {
            relay(&mut b, row, 0, 0, circ)?;
        }
    }
    let next = match want {
        SellNext::Relaid => super::marginal::sell_point_with(&b, &mut acc.memo),
        SellNext::WalkEnd => end,
        SellNext::None => None,
    };
    Ok(SellQuote {
        tokens_in: tokens,
        unfilled: tokens - walked.tokens,
        gross: walked.money,
        fees,
        out,
        steps: walked.steps,
        sqrt_before: before,
        sqrt_after: b.sqrt_p,
        from_f: walked.from_f,
        from_k: walked.from_k,
        from_c: walked.from_c,
        next,
    })
}

#[cfg(not(target_os = "solana"))]
pub fn buy_min_tokens(b: &Book, row: &SplitRow, sup: Supply, gross: u128, slippage_bps: u16) -> Result<u64, MathError> {
    if slippage_bps > 10_000 {
        return Err(MathError::VaultShareOutOfRange);
    }
    if slippage_bps == 10_000 {
        return Ok(0);
    }
    let less = gross.checked_mul(10_000 - slippage_bps as u128).ok_or(MathError::Overflow)? / 10_000;
    let mut b = *b;
    Ok(buy(&mut b, row, sup, less, 0)?.tokens)
}
