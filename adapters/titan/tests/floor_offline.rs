// SPDX-License-Identifier: Apache-2.0
mod fixture;

use std::time::Instant;

use assert_no_alloc::assert_no_alloc;
use floor_amm_solana::quote::QuoteState;
use floor_amm_solana::quote_mint::QuoteMint;
use titan_integration_template::your_venue::FloorVenue;
use rand::{Rng, SeedableRng};
use solana_pubkey::Pubkey;
use titan_integration_template::trading_venue::{QuoteRequest, SwapType, TradingVenue};

#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn exact_in(input_mint: Pubkey, output_mint: Pubkey, amount: u64) -> QuoteRequest {
    QuoteRequest { input_mint, output_mint, amount, swap_type: SwapType::ExactIn }
}

fn rng() -> rand::rngs::StdRng {
    rand::rngs::StdRng::seed_from_u64(0x7174_616e_5345_4544)
}

fn log_uniform(rng: &mut rand::rngs::StdRng, lo: u64, hi: u64) -> u64 {
    let r: f64 = rng.random();
    (((lo as f64).ln() + r * ((hi as f64).ln() - (lo as f64).ln())).exp() as u64).clamp(lo, hi)
}

fn geometric_grid(lb: u64, ub: u64, n: usize) -> Vec<u64> {
    let mut p: Vec<u64> = (0..n).map(|i| (((lb as f64).ln() + i as f64 / (n - 1) as f64 * ((ub as f64).ln() - (lb as f64).ln())).exp() as u64).clamp(lb, ub)).collect();
    p.sort();
    p.dedup();
    p
}

async fn each(mut f: impl FnMut(&str, &FloorVenue, &floor_amm_solana::layout::LaunchV2Account, Pubkey, Pubkey, u64, u64)) {
    for s in fixture::states() {
        let (v, l) = fixture::venue(&s).await;
        assert!(v.initialized(), "{}: a live launch quoted in a plain mint is routable", s.name);
        assert_eq!(v.get_token_info().len(), 2);
        for (i, o) in v.directions_num() {
            let (lb, ub) = v.bounds(i, o).unwrap_or_else(|e| panic!("{}: bounds {i}->{o}: {e}", s.name));
            let im = v.get_token(i as usize).unwrap().pubkey;
            let om = v.get_token(o as usize).unwrap().pubkey;
            f(&format!("{} {}", s.name, if i == 0 { "sell" } else { "buy" }), &v, &l, im, om, lb, ub);
        }
    }
}

#[tokio::test]
async fn construction_and_bounds_allocate_nothing() {
    let mut bad = vec![];
    each(|name, v, _, im, om, _, _| {
        let (i, o) = if v.is_buy(&im, &om).unwrap() { (1, 0) } else { (0, 1) };
        let (lower, upper) = assert_no_alloc(|| v.bounds(i, o)).expect("bounds");
        assert!(lower < upper, "{name}: lower < upper");
        for (edge, amount) in [("lower", lower), ("upper", upper)] {
            let q = assert_no_alloc(|| v.quote(exact_in(im, om, amount))).expect("quote");
            if q.not_enough_liquidity || q.expected_output == 0 || q.price <= 0.0 {
                bad.push(format!("{name}: {edge} bound {amount}: nel {} out {} price {}", q.not_enough_liquidity, q.expected_output, q.price));
            }
        }
        println!("{name}: bounds [{lower}, {upper}]");
    })
    .await;
    assert!(bad.is_empty(), "bounds:\n{}", bad.join("\n"));
}

#[tokio::test]
async fn quotable_directions() {
    for s in fixture::states() {
        let (v, _) = fixture::venue(&s).await;
        let d = v.directions_num();
        assert!(d.contains(&(1, 0)), "{}: the buy is quotable", s.name);
        assert_eq!(d.contains(&(0, 1)), !s.name.ends_with("/open"), "{}: the sell is quotable once a buy funds the bid", s.name);
    }
}

#[tokio::test]
async fn zero_input_spot_price() {
    each(|name, v, _, im, om, _, _| {
        let q = v.quote(exact_in(im, om, 0)).expect("a zero-input quote does not error");
        assert_eq!(q.expected_output, 0, "{name}");
        assert!(q.price > 0.0, "{name}: spot price {}", q.price);
    })
    .await;
}

#[tokio::test]
async fn output_monotone() {
    let mut r = rng();
    each(|name, v, _, im, om, lb, ub| {
        let mut a: Vec<u64> = (0..50).map(|_| log_uniform(&mut r, lb, ub)).collect();
        a.sort();
        let mut prev = 0;
        for x in a {
            let out = v.quote(exact_in(im, om, x)).unwrap().expected_output;
            assert!(prev <= out, "{name}: output not monotone: {prev} > {out} at {x}");
            prev = out;
        }
    })
    .await;
}

#[tokio::test]
async fn price_monotone() {
    const REL_TOL: f64 = 1e-3;
    let mut r = rng();
    each(|name, v, _, im, om, lb, ub| {
        let mut a: Vec<u64> = (0..200).map(|_| log_uniform(&mut r, lb, ub)).collect();
        a.sort();
        let mut prev = f64::INFINITY;
        for x in a {
            let p = v.quote(exact_in(im, om, x)).unwrap().price;
            assert!(p > 0.0, "{name}: price {p} at {x}");
            assert!(p <= prev * (1.0 + REL_TOL), "{name}: price not non-increasing: {prev} -> {p} at {x}");
            prev = p;
        }
    })
    .await;
}

#[tokio::test]
async fn mean_value_theorem() {
    const REL_TOL: f64 = 1e-5;
    const OUT_QUANTUM: f64 = 2.0;
    let (mut pairs, mut worst, mut bad) = (0usize, 0f64, vec![]);
    each(|name, v, _, im, om, lb, ub| {
        let mut first = None;
        let mut n_bad = 0;
        for w in geometric_grid(lb, ub, 64).windows(2) {
            let (a, b) = (w[0], w[1]);
            let qa = v.quote(exact_in(im, om, a)).unwrap();
            let qb = v.quote(exact_in(im, om, b)).unwrap();
            if qb.expected_output <= qa.expected_output {
                continue;
            }
            pairs += 1;
            let chord = (qb.expected_output - qa.expected_output) as f64 / (b - a) as f64;
            let atol = OUT_QUANTUM / (b - a) as f64;
            worst = f64::max(worst, f64::max(chord / qa.price - 1.0, 1.0 - chord / qb.price));
            let ok = qb.price <= qa.price * (1.0 + REL_TOL) && chord <= qa.price * (1.0 + REL_TOL) + atol && chord >= qb.price * (1.0 - REL_TOL) - atol;
            if !ok {
                n_bad += 1;
                first.get_or_insert(format!("[{a}, {b}]: f'(a)={} chord={chord} f'(b)={}", qa.price, qb.price));
                if std::env::var("MVT_ALL").is_ok() { println!("BAD {name} [{a}, {b}] out {}..{} f'(a)={} chord={chord} f'(b)={}", qa.expected_output, qb.expected_output, qa.price, qb.price); }
            }
        }
        if let Some(f) = first {
            bad.push(format!("{name}: {n_bad} intervals, first {f}"));
        }
    })
    .await;
    println!("mean value: {pairs} chords, worst excess {worst:e} (positive = outside before slack)");
    assert!(bad.is_empty(), "mean value bracket broken:\n{}", bad.join("\n"));
}

#[tokio::test]
async fn sell_price_is_the_walk_end_point() {
    use floor_math::v2::book::Undo;
    use floor_math::v2::fees::LgMemo;
    use floor_math::v2::marginal;
    let mut r = rng();
    let mut n = 0usize;
    each(|name, v, _, im, om, lb, ub| {
        if v.is_buy(&im, &om).unwrap() {
            return;
        }
        let c = v.cached().unwrap();
        let mut a = geometric_grid(lb, ub, 64);
        a.extend((0..100).map(|_| log_uniform(&mut r, lb, ub)));
        for x in a {
            let q = v.quote(exact_in(im, om, x)).unwrap();
            let mut b = c.book;
            let mut u = Undo::EMPTY;
            let mut acc = b.fee_acc_with(LgMemo::default());
            let w = b.walk_down_acc(x, &mut acc, &mut u).expect("the walk");
            assert!(w.tokens > 0, "{name}: {x} fills");
            let want = c.rate_at(false, marginal::sell_point_with(&b, &mut LgMemo::default()));
            assert_eq!(q.price.to_bits(), want.to_bits(), "{name}: sell {x}: price {} vs the walk-end point {want}", q.price);
            n += 1;
        }
    })
    .await;
    println!("{n} sell prices are the walk-end point");
    assert!(n > 1000);
}

#[tokio::test]
async fn quoting_speed() {
    let mut r = rng();
    let (mut worst, mut slow) = (0f64, vec![]);
    each(|name, v, _, im, om, lb, ub| {
        let a: Vec<u64> = (0..10_000).map(|_| log_uniform(&mut r, lb, ub)).collect();
        let t = Instant::now();
        for x in a {
            let _ = std::hint::black_box(v.quote(exact_in(im, om, x)).unwrap());
        }
        let avg = t.elapsed().as_secs_f64() / 10_000.0;
        worst = worst.max(avg);
        println!("{name}: {:.0} ns per quote", avg * 1e9);
        if avg >= 0.000_001 {
            slow.push(format!("{name}: {:.0} ns", avg * 1e9));
        }
    })
    .await;
    if !cfg!(debug_assertions) {
        assert!(slow.is_empty(), "quoting too slow (template: under 1 µs average):\n{}", slow.join("\n"));
    }
    println!("quoting speed: worst {:.0} ns (debug_assertions {})", worst * 1e9, cfg!(debug_assertions));
}

#[tokio::test]
async fn amounts_equal_the_shared_quote() {
    let mut r = rng();
    let (mut n, mut drained, mut dusty, mut worst_dust) = (0, 0, 0, 0f64);
    each(|name, v, l, im, om, lb, ub| {
        let is_buy = v.is_buy(&im, &om).unwrap();
        let qs = QuoteState::new(l, QuoteMint::default());
        let mut a: Vec<u64> = (0..100).map(|_| log_uniform(&mut r, lb, ub.saturating_mul(4))).collect();
        a.extend([1, lb, ub, ub + 1]);
        for x in a {
            let t = v.quote(exact_in(im, om, x)).unwrap();
            match qs.quote(is_buy, x, 800) {
                Ok(s) => {
                    assert_eq!((t.amount, t.expected_output), (s.in_amount, s.out_amount), "{name}: at {x}");
                    let drains = !is_buy && t.price == 0.0;
                    let dust = is_buy && s.not_enough_liquidity && !t.not_enough_liquidity && t.price > 0.0;
                    if dust {
                        worst_dust = worst_dust.max((x - t.amount) as f64 / x as f64);
                    }
                    assert_eq!(t.not_enough_liquidity, (s.not_enough_liquidity && !dust) || drains, "{name}: at {x}");
                    drained += drains as usize;
                    dusty += dust as usize;
                }
                Err(_) => assert!(t.not_enough_liquidity, "{name}: the shared quote refuses {x}; the venue says not enough liquidity"),
            }
            n += 1;
        }
    })
    .await;
    println!("{n} quotes equal the shared quote ({drained} sells that take the last bid not routed, {dusty} buys refunding rounding, at most {worst_dust:e} of the input)");
    assert!(worst_dust < 1e-5, "a whole fill refunds under the template's REL_TOL (1e-5) of its input");
}
