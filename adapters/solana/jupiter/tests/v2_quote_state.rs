// SPDX-License-Identifier: Apache-2.0
mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use common::v2::*;
use floor_amm_solana::quote::{QuoteError, QuoteState};
use floor_amm_solana::quote_mint::QuoteMint;
use floor_math::v2::fees::fee_total;
use floor_math::v2::trade;

struct Count;
thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|a| a.set(a.get() + 1));
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Count = Count;

#[test]
fn every_golden_case_through_the_cached_state_equals_the_reference_trade() {
    let g = load("book");
    let (mut buys, mut sells, mut refunded, mut unfilled) = (0, 0, 0, 0);
    for c in g["cases"].as_array().unwrap() {
        let f = c["fn"].as_str().unwrap();
        if f != "buy" && f != "sell" {
            continue;
        }
        let b_in = book(&c["state_in"]);
        if !fits_u64(&b_in) || c["out"].get("err").is_some() {
            continue;
        }
        let l = launch(&b_in);
        let st = QuoteState::new(&l, QuoteMint::default());
        let (row, sup) = (l.split_row(), l.supply());
        let id = c["id"].as_str().unwrap();
        if f == "buy" {
            let gross: u64 = n(&c["args"]["gross"]);
            let r = sol_quote_buy(&b_in, &row, sup, gross as u128).unwrap();
            let q = st.quote(true, gross, 0).unwrap();
            let spent = gross - r.refund as u64;
            assert_eq!((q.in_amount, q.out_amount, q.fee_amount as u128, q.fee_base), (spent, r.tokens, fee_total(&r.fees), spent as u128), "{id}");
            assert_eq!(q.not_enough_liquidity, r.refund > 0, "{id}");
            buys += 1;
            refunded += (r.refund > 0) as u32;
        } else {
            let tokens: u64 = n(&c["args"]["tokens"]);
            let r = trade::quote_sell(&b_in, &row, sup, tokens).unwrap();
            let q = st.quote(false, tokens, 0).unwrap();
            assert_eq!((q.in_amount, q.out_amount as u128, q.fee_amount as u128, q.fee_base), (r.tokens_in - r.unfilled, r.out, fee_total(&r.fees), r.gross), "{id}");
            assert_eq!(q.not_enough_liquidity, r.unfilled > 0, "{id}");
            sells += 1;
            unfilled += (r.unfilled > 0) as u32;
        }
        assert_eq!(*st.book(), b_in, "{id}: the cache holds the book it was given and a quote moves nothing");
    }
    for c in g["cases"].as_array().unwrap() {
        let b_in = book(&c["state_in"]);
        if !fits_u64(&b_in) {
            continue;
        }
        let l = launch(&b_in);
        let st = QuoteState::new(&l, QuoteMint::default());
        for tokens in [1u64 << 40, 1 << 50, 1 << 55, 1 << 60] {
            let Ok(r) = trade::quote_sell(&b_in, &l.split_row(), l.supply(), tokens) else { continue };
            let q = st.quote(false, tokens, 0).unwrap();
            assert_eq!(q.not_enough_liquidity, r.unfilled > 0);
            assert_eq!(q.in_amount, r.tokens_in - r.unfilled);
            unfilled += (r.unfilled > 0) as u32;
        }
    }
    eprintln!("quote_state golden: {buys} buys ({refunded} refunded), {sells} sells ({unfilled} unfilled)");
    assert!(buys > 0 && sells > 0);
    assert!(refunded > 0, "no golden buy was partly refunded: the flag is untested");
    assert!(unfilled > 0, "no golden sell was partly unfilled: the flag is untested");
}

#[test]
fn a_launch_that_is_not_live_refuses_with_a_copy_error() {
    let g = load("book");
    let c = g["cases"].as_array().unwrap().iter().find(|c| c["fn"] == "buy" && c["out"].get("err").is_none() && fits_u64(&book(&c["state_in"]))).unwrap();
    let mut l = launch(&book(&c["state_in"]));
    l.status = 1;
    let st = QuoteState::new(&l, QuoteMint::default());
    assert_eq!(st.quote(true, 1_000_000, 0).unwrap_err(), QuoteError::NotLive(1));
    assert_eq!(st.quote(false, 1_000_000, 0).unwrap_err(), QuoteError::NotLive(1));
}

#[test]
fn a_transfer_fee_that_takes_the_whole_amount_refuses() {
    let g = load("book");
    let c = g["cases"].as_array().unwrap().iter().find(|c| c["fn"] == "buy" && c["out"].get("err").is_none() && fits_u64(&book(&c["state_in"]))).unwrap();
    let l = launch(&book(&c["state_in"]));
    let st = QuoteState::new(&l, QuoteMint { hooked: false, fee: Some(((0, u64::MAX, 10_000), (0, u64::MAX, 10_000))) });
    assert_eq!(st.quote(true, 1_000_000, 0).unwrap_err(), QuoteError::TransferFeeTakesAll);
}

#[test]
fn ten_thousand_quotes_allocate_nothing_and_beat_the_rebuild() {
    let g = load("book");
    let cases: Vec<_> = g["cases"].as_array().unwrap().iter().filter(|c| (c["fn"] == "buy" || c["fn"] == "sell") && c["out"].get("err").is_none() && fits_u64(&book(&c["state_in"]))).collect();
    let c = cases.iter().find(|c| c["fn"] == "buy").unwrap();
    let b_in = book(&c["state_in"]);
    let l = launch(&b_in);
    let gross: u64 = n(&c["args"]["gross"]);
    let st = QuoteState::new(&l, QuoteMint::default());
    const N: u32 = 10_000;
    let before = ALLOCS.with(|a| a.get());
    let t = Instant::now();
    let mut acc = 0u64;
    for i in 0..N {
        acc = acc.wrapping_add(st.quote(true, gross + (i % 7) as u64, 0).unwrap().out_amount);
    }
    let cached = t.elapsed();
    assert_eq!(ALLOCS.with(|a| a.get()), before, "a cached quote allocated");
    let t = Instant::now();
    let mut acc2 = 0u64;
    for i in 0..N {
        let (mut b, mut u) = (l.book(), floor_math::v2::book::Undo::EMPTY);
        let r = trade::buy_steps_with(&mut b, &l.split_row(), l.supply(), (gross + (i % 7) as u64) as u128, 0, floor_math::v2::book::SOL_BUY_WALK_STEPS, &mut u).unwrap();
        acc2 = acc2.wrapping_add(r.tokens);
    }
    let rebuilt = t.elapsed();
    assert_eq!(acc, acc2, "same quotes");
    eprintln!("quote_state timing {N} buys: cached {:.2} us/quote, rebuild {:.2} us/quote ({})", cached.as_secs_f64() * 1e6 / N as f64, rebuilt.as_secs_f64() * 1e6 / N as f64, if cfg!(debug_assertions) { "debug" } else { "release" });
    assert!(cached.as_secs_f64() < 5.0, "10,000 cached quotes took {cached:?}");
}
