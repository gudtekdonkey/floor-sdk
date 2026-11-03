// SPDX-License-Identifier: Apache-2.0
mod common;

use common::v2::*;
use floor_amm_jupiter::v2::LaunchV2Account;
use floor_math::v2::trade;
use jupiter_amm_interface::Amm;

fn dec_units(s: &str, scale: u32) -> (u128, bool) {
    assert!(!s.contains(['e', 'E']), "{s}: exponent");
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let mut u: u128 = int.parse::<u128>().unwrap() * 10u128.pow(scale);
    let fb = frac.as_bytes();
    for k in 0..scale as usize {
        let d = fb.get(k).map_or(0, |c| (c - b'0') as u128);
        u += d * 10u128.pow(scale - 1 - k as u32);
    }
    let exact = fb.iter().skip(scale as usize).all(|c| *c == b'0');
    (u, exact)
}

#[test]
fn every_launch_book_trade_replays_through_the_account_bytes_and_the_amm() {
    let g = load("oracle/launch_book");
    let (mut buys, mut sells, mut refused, mut wide, mut short) = (0, 0, 0, 0, Vec::new());
    let mut bad: Vec<String> = Vec::new();
    for c in g["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let b_in = oracle_book(&c["state_in"]);
        let o = &c["out"];
        let is_buy = c["fn"] == "buy";
        let Ok(amount) = u64::try_from(n::<u128>(&c["args"][if is_buy { "gross" } else { "tokens" }])) else {
            wide += 1;
            continue;
        };
        let l = launch_circ(&b_in, if is_buy { 0 } else { amount });
        assert_eq!(LaunchV2Account::decode(&l.encode()).unwrap().book(), b_in, "{id}: state_in through the bytes");
        let a = amm(&l);
        let q = a.quote(&qp(amount, is_buy));
        if is_buy {
            match sol_quote_buy(&b_in, &l.split_row(), l.supply(), amount as u128) {
                Err(e) => {
                    let e = format!("{e:?}");
                    assert!(q.unwrap_err().to_string().contains(&e), "{id}: the adapter refuses as the math ({e})");
                    if !(n::<u128>(&o["tokens_out_min"]) == 0 && (e == "NothingToDo" || e == "BookEmpty")) {
                        bad.push(format!("{id} refused {e} {o}"));
                    }
                    refused += 1;
                }
                Ok(r) => {
                    buys += 1;
                    assert_eq!(a.simulate_buy(amount).unwrap(), r, "{id}: the adapter's receipt is the math's");
                    let q = q.unwrap();
                    assert_eq!((q.in_amount as u128, q.out_amount), (r.gross - r.refund, r.tokens), "{id}");
                    let (t, net) = (r.tokens as u128, r.net);
                    assert!(t <= n::<u128>(&o["tokens_out_max"]), "{id}: UNSAFE tokens {t} {o}");
                    assert!(net <= n::<u128>(&o["book_in_max"]), "{id}: UNSAFE net {net} {o}");
                    if t < n::<u128>(&o["tokens_out_min"]) || net < n::<u128>(&o["book_in_min"]) {
                        short.push(format!("{id} buy: tokens {t} net {net} {o}"));
                    }
                }
            }
        } else {
            match trade::quote_sell(&b_in, &l.split_row(), l.supply(), amount) {
                Err(e) => {
                    let e = format!("{e:?}");
                    assert!(q.unwrap_err().to_string().contains(&e), "{id}: the adapter refuses as the math ({e})");
                    let max: u128 = n(&o["book_out_max"]);
                    if !(n::<u128>(&o["book_out_min"]) <= 5 * max.div_ceil(10_000) && (e == "NothingToDo" || e == "BookEmpty")) {
                        bad.push(format!("{id} refused {e} {o}"));
                    }
                    refused += 1;
                }
                Ok(r) => {
                    sells += 1;
                    assert_eq!(a.simulate_sell(amount).unwrap(), r, "{id}: the adapter's receipt is the math's");
                    let q = q.unwrap();
                    assert_eq!((q.in_amount, q.out_amount as u128), (r.tokens_in - r.unfilled, r.out), "{id}");
                    let (gross, tin) = (r.gross, (r.tokens_in - r.unfilled) as u128);
                    assert!(gross <= n::<u128>(&o["book_out_max"]), "{id}: UNSAFE gross {gross} {o}");
                    assert!(tin <= n::<u128>(&o["tokens_in_max"]), "{id}: filled {tin} past the request {o}");
                    if gross < n::<u128>(&o["book_out_min"]) || tin < n::<u128>(&o["tokens_in_min"]) {
                        short.push(format!("{id} sell: gross {gross} filled {tin} {o}"));
                    }
                }
            }
        }
    }
    assert_eq!((buys + sells + refused, wide), (956, 64), "launch_book.json's cases changed: re-read them");
    assert_eq!((buys, sells, refused), (411, 317, 228), "launch_book.json's cases changed: re-read them");
    for x in &bad {
        eprintln!("REFUSED past the engine's bound: {x}");
    }
    for x in &short {
        eprintln!("SHORT (math, check.py 'FAIL safe'): {x}");
    }
    eprintln!("launch_book: {buys} buys, {sells} sells, {refused} refused, {wide} past u64, {} short of the engine's minimum", short.len());
    let ids: Vec<&str> = short.iter().map(|x| x.split(' ').next().unwrap()).collect();
    assert!(bad.is_empty(), "{} refusals past the oracle's bounds", bad.len());
    assert_eq!(ids, [] as [&str; 0], "short fills changed");
}
