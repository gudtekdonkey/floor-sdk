//! Golden vectors for the sale buy, generated from `floor-math` (the program's sale arithmetic) and
//! replayed through `Amm::quote`.
//!
//! `tests/fixtures/sale-vectors.json` holds the cases and the answers. Its INPUTS are a fixed grid
//! (prices from real launch configurations and from devnet, the fee rates the program allows, sold
//! levels that put the sale far from, near and at its end, amounts from one lamport to above the
//! whole sale); its ANSWERS are `floor_math::sale_buy_with_team`. Each case is checked three ways:
//! the file against `floor-math` now (the arithmetic has not moved), the file against
//! `FloorSaleAmm::quote` (the adapter reports what the program computes), and the refusals.
//! `FLOOR_WRITE_VECTORS=1 cargo test --test vectors` rewrites the file after a deliberate change.

mod common;

use common::*;
use floor_math::modes::tranche_price_x64;
use floor_math::sale_buy_with_team;
use floor_sale_amm::amm::refusal;
use floor_types::constants::SALE_TOKENS;
use jupiter_amm_interface::{Amm, QuoteParams, SwapMode};
use serde_json::{json, Value};

const PATH: &str = "tests/fixtures/sale-vectors.json";

fn prices() -> Vec<(String, u128)> {
    let mut p = vec![
        // $5k / $10k / $25k / $100k raises at SOL = $150 (9-decimal wSOL), as `create_launch` derives them.
        ("mcap 5000 @ $150".to_string(), tranche_price_x64(5_000, 15_000, 9).unwrap()),
        ("mcap 10000 @ $150".to_string(), tranche_price_x64(10_000, 15_000, 9).unwrap()),
        ("mcap 25000 @ $150".to_string(), tranche_price_x64(25_000, 15_000, 9).unwrap()),
        ("mcap 100000 @ $150".to_string(), tranche_price_x64(100_000, 15_000, 9).unwrap()),
        ("mcap 50000 @ $250".to_string(), tranche_price_x64(50_000, 25_000, 9).unwrap()),
    ];
    // The four devnet sales' stored prices.
    for (name, v) in [("devnet sFdX", 7_682_302_212_939_176u128), ("devnet 37cg", 92_233_720_368_547), ("devnet 4jJc", 3_808_478_006_794_440), ("devnet Efh3", 7_799_891_785_923_700)] {
        p.push((name.to_string(), v));
    }
    p.push(("one lamport per unit".to_string(), PRICE_ONE));
    p
}

const AMOUNTS: &[u64] = &[1, 2, 99, 101, 1_000, 999_999, 1_000_000, 123_456_789, 1_000_000_000, 77_777_777_777, 1_000_000_000_000, u64::MAX];
const BPS: &[u16] = &[0, 1, 40, 100];

fn sold_levels() -> Vec<u64> {
    vec![0, SALE_TOKENS / 2, SALE_TOKENS - 1_000_000_000, SALE_TOKENS - 1, SALE_TOKENS - 2]
}

fn generate() -> Value {
    let mut cases = Vec::new();
    for (pname, price) in prices() {
        for bps in BPS {
            for sold in sold_levels() {
                for amount in AMOUNTS {
                    let remaining = SALE_TOKENS - sold;
                    let (tokens, cost, fee) = sale_buy_with_team(*amount, price, remaining, *bps).unwrap();
                    cases.push(json!({
                        "price": pname, "price_x64": price.to_string(), "team_bps": bps, "sold": sold.to_string(),
                        "amount": amount.to_string(), "tokens": tokens.to_string(), "cost": cost.to_string(), "fee": fee.to_string(),
                    }));
                }
            }
        }
    }
    json!({ "sale_tokens": SALE_TOKENS.to_string(), "cases": cases })
}

fn n(v: &Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}

#[test]
fn golden_sale_vectors() {
    if std::env::var_os("FLOOR_WRITE_VECTORS").is_some() {
        std::fs::write(PATH, serde_json::to_string_pretty(&generate()).unwrap() + "\n").unwrap();
    }
    let file: Value = serde_json::from_str(&std::fs::read_to_string(PATH).expect("vectors file")).unwrap();
    assert_eq!(file, generate(), "floor-math no longer produces the recorded answers");
    let cases = file["cases"].as_array().unwrap();
    let (mut answered, mut nothing, mut graduates) = (0, 0, 0);
    for c in cases {
        let (price, bps, sold, amount) = (n(&c["price_x64"]), c["team_bps"].as_u64().unwrap() as u16, n(&c["sold"]) as u64, n(&c["amount"]) as u64);
        let (tokens, cost, fee) = (n(&c["tokens"]) as u64, n(&c["cost"]) as u64, n(&c["fee"]) as u64);
        let (mint, l) = default_sale(price, sold, bps);
        let amm = amm_of(&l);
        let got = amm.quote(&QuoteParams { amount, input_mint: wsol(), output_mint: mint, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() });
        if tokens == 0 {
            assert!(got.unwrap_err().to_string().starts_with(refusal::NOTHING), "{c}");
            nothing += 1;
        } else if sold + tokens == SALE_TOKENS {
            assert!(got.unwrap_err().to_string().starts_with(refusal::SALE_WOULD_GRADUATE), "{c}");
            graduates += 1;
        } else {
            let q = got.unwrap_or_else(|e| panic!("{c}: {e}"));
            assert_eq!((q.out_amount, q.in_amount, q.fee_amount), (tokens, cost + fee, fee), "{c}");
            assert!(q.in_amount <= amount);
            answered += 1;
        }
    }
    eprintln!("{} vectors: {answered} answered, {nothing} refused Nothing, {graduates} refused SaleWouldGraduate", cases.len());
    assert!(answered > 0 && nothing > 0 && graduates > 0);
}
