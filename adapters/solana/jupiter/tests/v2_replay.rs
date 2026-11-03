// SPDX-License-Identifier: Apache-2.0
mod common;

use common::v2::*;
use floor_amm_jupiter::v2::LaunchV2Account;
use floor_math::v2::fees::fee_total;
use floor_math::v2::trade;
use jupiter_amm_interface::Amm;

#[test]
fn every_v2_book_trade_replays_through_the_account_bytes_and_the_amm() {
    let g = load("book");
    let (mut buys, mut sells, mut errs, mut wide) = (0, 0, 0, 0);
    for c in g["cases"].as_array().unwrap() {
        let f = c["fn"].as_str().unwrap();
        if f != "buy" && f != "sell" {
            continue;
        }
        let id = c["id"].as_str().unwrap();
        let b_in = book(&c["state_in"]);
        if !fits_u64(&b_in) {
            assert_refused_as_wide(id, &b_in);
            wide += 1;
            continue;
        }
        let l = launch(&b_in);
        let decoded = LaunchV2Account::decode(&l.encode()).unwrap();
        assert_eq!(decoded, l, "{id}");
        assert_eq!(decoded.book(), b_in, "{id}: state_in through the bytes");
        let a = amm(&l);
        let out = &c["out"];
        let is_buy = f == "buy";
        let amount: u64 = n(&c["args"][if is_buy { "gross" } else { "tokens" }]);
        let min_tokens: u64 = opt(&c["args"]["min_tokens"]);
        let min_out: u128 = opt(&c["args"]["min_out"]);
        let (row, sup) = (l.split_row(), l.supply());
        let q = a.quote(&qp(amount, is_buy));
        if let Some(e) = out.get("err") {
            errs += 1;
            let e = e.as_str().unwrap();
            if e == "SlippageExceeded" {
                assert!(q.is_ok(), "{id}");
            } else {
                assert!(q.unwrap_err().to_string().contains(e), "{id}: want {e}");
            }
            let mut w = b_in;
            let r = if is_buy { sol_buy(&mut w, &row, sup, amount as u128, min_tokens).err() } else { trade::sell(&mut w, &row, sup, amount, min_out).err() };
            assert_eq!(format!("{:?}", r.unwrap()), e, "{id}");
            assert_eq!(w, b_in, "{id}: a refused trade moves nothing");
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{id}: {e}"));
        let mut w = decoded.book();
        if is_buy {
            buys += 1;
            let r = a.simulate_buy(amount).unwrap();
            assert_eq!(r, sol_quote_buy(&b_in, &row, sup, amount as u128).unwrap(), "{id}: the adapter's receipt is the math's");
            for (k, v) in [
                ("gross", r.gross),
                ("net", r.net),
                ("refund", r.refund),
                ("tokens", r.tokens as u128),
                ("from_resale", r.from_resale as u128),
                ("from_first", r.from_first as u128),
                ("steps", r.steps as u128),
                ("sqrt_before", r.sqrt_before),
                ("sqrt_after", r.sqrt_after),
            ] {
                assert_eq!(v, n::<u128>(&out[k]), "{id}: {k}");
            }
            assert_eq!(r.fees, arr::<u128, 5>(&out["fees"]), "{id}: fees");
            assert_eq!(r.budget_hit, out["budget_hit"].as_bool().unwrap(), "{id}");
            assert_eq!(r.graduated_now, out["graduated_now"].as_bool().unwrap(), "{id}");
            let all = sol_quote_buy(&b_in, &row, sup, u64::MAX as u128);
            match (a.quote(&qp(u64::MAX, true)), all) {
                (Ok(qa), Ok(ra)) => {
                    assert_eq!(qa.in_amount as u128, ra.gross - ra.refund, "{id}: max buy");
                    assert_eq!(qa.out_amount, ra.tokens, "{id}: max buy");
                }
                (Err(e), Err(m)) => assert!(e.to_string().contains(&format!("{m:?}")), "{id}: max buy {e} vs {m:?}"),
                (x, y) => panic!("{id}: max buy adapter {x:?} math {y:?}"),
            }
            assert_eq!(q.in_amount as u128, r.gross - r.refund, "{id}: in = the spent leg");
            assert_eq!(q.out_amount, r.tokens, "{id}");
            assert_eq!(q.fee_amount as u128, fee_total(&r.fees), "{id}");
            assert_eq!(q.fee_amount as u128 + r.net, q.in_amount as u128, "{id}: spent = net + the five legs");
            assert_eq!(sol_buy(&mut w, &row, sup, amount as u128, min_tokens).unwrap(), r, "{id}: quote = trade");
        } else {
            sells += 1;
            let r = a.simulate_sell(amount).unwrap();
            assert_eq!(r, trade::quote_sell(&b_in, &row, sup, amount).unwrap(), "{id}: the adapter's receipt is the math's");
            for (k, v) in [
                ("tokens_in", r.tokens_in as u128),
                ("unfilled", r.unfilled as u128),
                ("gross", r.gross),
                ("out", r.out),
                ("steps", r.steps as u128),
                ("sqrt_before", r.sqrt_before),
                ("sqrt_after", r.sqrt_after),
                ("from_f", r.from_f),
                ("from_k", r.from_k),
                ("from_c", r.from_c),
            ] {
                assert_eq!(v, n::<u128>(&out[k]), "{id}: {k}");
            }
            assert_eq!(r.fees, arr::<u128, 5>(&out["fees"]), "{id}: fees");
            assert_eq!(q.in_amount, r.tokens_in - r.unfilled, "{id}: in = the tokens that filled");
            assert_eq!(q.out_amount as u128, r.out, "{id}");
            assert_eq!(q.fee_amount as u128, fee_total(&r.fees), "{id}");
            assert_eq!(q.out_amount as u128 + q.fee_amount as u128, r.gross, "{id}: gross = out + the five legs");
            assert_eq!(trade::sell(&mut w, &row, sup, amount, min_out).unwrap(), r, "{id}: quote = trade");
        }
        let want = book(&c["state_out"]);
        assert_eq!(w, want, "{id}: state_out");
        let mut after = decoded;
        after.set_book(&w).unwrap();
        let back = LaunchV2Account::decode(&after.encode()).unwrap();
        assert_eq!(back.book(), want, "{id}: state_out through the bytes");
        let mut rest = back;
        rest.set_book(&b_in).unwrap();
        assert_eq!(rest, l, "{id}: a trade writes only the book's fields");
    }
    println!("book.json: {wide} states wider than u64 filtered (not Solana states; set_book refuses each)");
    assert_eq!((buys, sells, errs, wide), (23, 26, 3, 6), "book.json's trade cases changed: re-read them");
}
