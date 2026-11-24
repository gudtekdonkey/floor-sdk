// SPDX-License-Identifier: Apache-2.0
mod common;

use common::v2::*;
use common::QUOTE_MINT;
use floor_math::v2::fees::{fee_legs, fee_total, net_from_gross};
use jupiter_amm_interface::{Amm, Quote};
use rust_decimal::Decimal;
use serde_json::Value;

fn pct(fee: u128, of: u128) -> Decimal {
    Decimal::from_i128_with_scale(fee as i128, 0) / Decimal::from_i128_with_scale(of as i128, 0)
}

fn want(out: &Value, is_buy: bool) -> (u64, u64, u128, Decimal) {
    let fees: [u128; 5] = arr(&out["fees"]);
    let fee = fee_total(&fees);
    if is_buy {
        let spent = n::<u128>(&out["gross"]) - n::<u128>(&out["refund"]);
        (spent as u64, n(&out["tokens"]), fee, pct(fee, spent))
    } else {
        let gross: u128 = n(&out["gross"]);
        (n::<u64>(&out["tokens_in"]) - n::<u64>(&out["unfilled"]), n(&out["out"]), fee, pct(fee, gross))
    }
}

fn got(q: &Quote) -> (u64, u64, u128, Decimal) {
    assert_eq!(q.fee_mint, QUOTE_MINT, "the fee is in the quote mint");
    (q.in_amount, q.out_amount, q.fee_amount as u128, q.fee_pct)
}

#[test]
fn every_fee_leg_and_net_equals_floor_maths_print() {
    let g = load("oracle/fees");
    let (mut legs, mut nets) = (0, 0);
    for c in g["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        assert_eq!(c["check"], "exact", "{id}");
        let ppm: [u32; 5] = arr(&c["args"]["fee_ppm"]);
        match c["fn"].as_str().unwrap() {
            "fee_legs" => {
                legs += 1;
                assert_eq!(fee_legs(n(&c["args"]["amount"]), &ppm), arr::<u128, 5>(&c["out"]["fees"]), "{id}");
            }
            "book_from_gross" => {
                nets += 1;
                let gross: u128 = n(&c["args"]["gross"]);
                let net = net_from_gross(gross, &ppm);
                assert_eq!(net, n::<u128>(&c["out"]["net"]), "{id}: net");
                assert_eq!(fee_legs(net, &ppm), arr::<u128, 5>(&c["out"]["fees"]), "{id}: the legs on the net");
                assert!(net + fee_total(&fee_legs(net, &ppm)) <= gross, "{id}: net + fees within the gross");
            }
            x => panic!("{id}: fn {x}"),
        }
    }
    assert_eq!((legs, nets), (2_000, 2_000), "fees.json's cases changed: re-read them");
}

#[test]
fn every_number_jupiter_reads_equals_the_v2_book_print() {
    let g = load("book");
    let (mut quoted, mut refused, mut wide) = (0, 0, 0);
    for c in g["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let is_buy = c["fn"] == "buy";
        let b_in = book(&c["state_in"]);
        if !fits_u64(&b_in) {
            assert_refused_as_wide(id, &b_in);
            wide += 1;
            continue;
        }
        let a = amm(&launch(&b_in));
        let amount: u64 = n(&c["args"][if is_buy { "gross" } else { "tokens" }]);
        let q = a.quote(&qp(amount, is_buy));
        match c["out"].get("err").and_then(Value::as_str) {
            Some("SlippageExceeded") | None => {
                let q = q.unwrap_or_else(|e| panic!("{id}: {e}"));
                if c["out"].get("err").is_none() {
                    assert_eq!(got(&q), want(&c["out"], is_buy), "{id}");
                    quoted += 1;
                } else {
                    refused += 1;
                }
            }
            Some(e) => {
                assert!(q.unwrap_err().to_string().contains(e), "{id}: want {e}");
                refused += 1;
            }
        }
    }
    println!("book.json: {wide} states wider than u64 filtered (not Solana states; set_book refuses each)");
    assert_eq!((quoted, refused, wide), (49, 3, 6), "book.json's trade cases changed: re-read them");
}

#[test]
fn every_number_jupiter_reads_on_launch_books_is_the_receipts() {
    let g = load("oracle/launch_book");
    let (mut buys, mut sells) = (0, 0);
    for c in g["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let is_buy = c["fn"] == "buy";
        let a = amm(&launch(&book(&c["state_in"])));
        let Ok(amount) = u64::try_from(n::<u128>(&c["args"][if is_buy { "gross" } else { "tokens" }])) else { continue };
        let Ok(q) = a.quote(&qp(amount, is_buy)) else { continue };
        if is_buy {
            let r = a.simulate_buy(amount).unwrap();
            let (spent, fee) = (r.gross - r.refund, fee_total(&r.fees));
            assert_eq!(got(&q), (spent as u64, r.tokens, fee, pct(fee, spent)), "{id}");
            assert_eq!(fee + r.net, spent, "{id}: spent = net + the five legs");
            buys += 1;
        } else {
            let r = a.simulate_sell(amount).unwrap();
            let fee = fee_total(&r.fees);
            assert_eq!(got(&q), (r.tokens_in - r.unfilled, r.out as u64, fee, pct(fee, r.gross)), "{id}");
            assert_eq!(r.out + fee, r.gross, "{id}: gross = out + the five legs");
            sells += 1;
        }
    }
    assert!(buys >= 400 && sells >= 150, "launch_book quotes: {buys} buys, {sells} sells");
}
