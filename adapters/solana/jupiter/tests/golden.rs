// SPDX-License-Identifier: Apache-2.0
mod common;

use common::v2::*;
use floor_amm_jupiter::v2::LaunchV2Account;
use floor_math::v2::fees::fee_total;
use floor_math::v2::trade;
use jupiter_amm_interface::Amm;

const OPS: usize = 10;

fn mix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Default, Debug)]
struct Seen {
    walks: usize,
    buys: usize,
    sells: usize,
    refused: usize,
    graduated_walks: usize,
    max_bands: u8,
}

#[test]
fn every_v2_walk_replays_receipt_for_receipt_through_the_account_bytes() {
    let g = load("oracle/launch_book");
    let mut seen = Seen::default();
    for (ci, c) in g["cases"].as_array().unwrap().iter().enumerate() {
        let id = c["id"].as_str().unwrap();
        let mut b = book(&c["state_in"]);
        let mut acc = launch(&b);
        let row = acc.split_row();
        let sup = acc.supply();
        let mut held: u64 = 0;
        let mut rng = 0xF100_7A11u64 ^ ci as u64;
        seen.walks += 1;
        seen.graduated_walks += b.graduated as usize;
        for op in 0..OPS {
            acc.set_book(&b).unwrap();
            let bytes = LaunchV2Account::decode(&acc.encode()).unwrap();
            assert_eq!(bytes.book(), b, "{id} op {op}: the book through the bytes");
            let a = amm(&bytes);
            assert_eq!((bytes.split_row(), bytes.supply()), (row, sup), "{id} op {op}: the row and the supply are constant");
            let r = mix(&mut rng);
            let is_buy = held == 0 || r % 3 != 0;
            if is_buy {
                let gross = (10_000u64 << (r >> 8) % 30) + (r >> 40) % 1_000;
                let mut w = b;
                let want = sol_buy(&mut w, &row, sup, gross as u128, 0);
                let got = a.simulate_buy(gross);
                let q = a.quote(&qp(gross, true));
                match want {
                    Err(e) => {
                        seen.refused += 1;
                        assert!(got.unwrap_err().to_string().contains(&format!("{e:?}")), "{id} op {op}: buy {gross}");
                        assert!(q.is_err(), "{id} op {op}");
                        continue;
                    }
                    Ok(rc) => {
                        let got = got.unwrap_or_else(|e| panic!("{id} op {op}: buy {gross}: {e}"));
                        assert_eq!(got, rc, "{id} op {op}: the buy (walk, split, re-lay)");
                        let q = q.unwrap();
                        assert_eq!(q.in_amount as u128, rc.gross - rc.refund, "{id} op {op}");
                        assert_eq!(q.out_amount, rc.tokens, "{id} op {op}");
                        assert_eq!(q.fee_amount as u128, fee_total(&rc.fees), "{id} op {op}");
                        seen.buys += 1;
                        seen.max_bands = seen.max_bands.max(w.n_bands);
                        held += rc.tokens;
                        b = w;
                    }
                }
            } else {
                let tokens = (held >> (r >> 8) % 3).max(1);
                let mut w = b;
                let want = trade::sell(&mut w, &row, sup, tokens, 0);
                let got = a.simulate_sell(tokens);
                let q = a.quote(&qp(tokens, false));
                match want {
                    Err(e) => {
                        seen.refused += 1;
                        assert!(got.unwrap_err().to_string().contains(&format!("{e:?}")), "{id} op {op}: sell {tokens}");
                        assert!(q.is_err(), "{id} op {op}");
                        held -= tokens;
                        continue;
                    }
                    Ok(rc) => {
                        assert_eq!(got.unwrap(), rc, "{id} op {op}: the sell");
                        let q = q.unwrap();
                        assert_eq!(q.in_amount, rc.tokens_in - rc.unfilled, "{id} op {op}");
                        assert_eq!(q.out_amount as u128, rc.out, "{id} op {op}");
                        assert_eq!(q.fee_amount as u128, fee_total(&rc.fees), "{id} op {op}");
                        seen.sells += 1;
                        held -= rc.tokens_in - rc.unfilled;
                        b = w;
                    }
                }
            }
        }
    }
    eprintln!("{seen:?}");
    assert_eq!(seen.walks, 1_020, "launch_book.json's cases changed: re-read them");
    assert!(seen.buys >= 5_000 && seen.sells >= 1_000, "{seen:?}");
    assert!(seen.graduated_walks >= 200, "the walks reach graduated books: {seen:?}");
}
