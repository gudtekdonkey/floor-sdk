// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use super::{account, accounts, BASE_MINT, QUOTE_MINT};
use floor_amm_jupiter::keys::{self, PROGRAM_ID, TOKEN_PROGRAM_ID};
use floor_amm_jupiter::v2::layout::VERSION_V2;
use floor_amm_jupiter::v2::{FloorAmmV2, LaunchV2Account};
use floor_math::v2::book::{Book, Range, BIDS_MAX, RANGES_MAX};
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, QuoteParams, SwapMode};
use serde_json::Value;

pub const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");

pub fn load(section: &str) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(format!("{GOLDEN}{section}.json")).unwrap()).unwrap();
    assert_eq!(v["schema"], format!("floor-math/v2/{}", section.trim_start_matches("oracle/")));
    assert_eq!(v["units"]["sqrt_frac_bits"], 72);
    v
}

pub fn n<T: std::str::FromStr>(v: &Value) -> T
where
    T::Err: std::fmt::Debug,
{
    match v {
        Value::String(s) => s.parse().unwrap(),
        Value::Number(x) => x.to_string().parse().unwrap(),
        Value::Bool(b) => (*b as u8).to_string().parse().unwrap(),
        _ => panic!("not a number: {v}"),
    }
}

pub fn opt<T: std::str::FromStr + Default>(v: &Value) -> T
where
    T::Err: std::fmt::Debug,
{
    if v.is_null() {
        T::default()
    } else {
        n(v)
    }
}

pub fn arr<T: std::str::FromStr + Copy + Default, const N: usize>(v: &Value) -> [T; N]
where
    T::Err: std::fmt::Debug,
{
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), N);
    let mut o = [T::default(); N];
    for (i, x) in a.iter().enumerate() {
        o[i] = n(x);
    }
    o
}

pub fn arr_pad<T: std::str::FromStr + Copy + Default, const N: usize>(v: &Value) -> [T; N]
where
    T::Err: std::fmt::Debug,
{
    let a = v.as_array().unwrap();
    assert!(a.len() <= N);
    let mut o = [T::default(); N];
    for (i, x) in a.iter().enumerate() {
        o[i] = n(x);
    }
    o
}

pub fn book(s: &Value) -> Book {
    let mut ranges = [Range::default(); RANGES_MAX];
    let rs = s["ranges"].as_array().unwrap();
    assert_eq!(rs.len(), n::<usize>(&s["n_ranges"]));
    for (i, r) in rs.iter().enumerate() {
        ranges[i] = Range {
            sqrt_a: n(&r["sqrt_a"]),
            sqrt_b: n(&r["sqrt_b"]),
            tokens: n(&r["tokens"]),
            ask_left: n(&r["ask_left"]),
            back: n(&r["back"]),
            place: n(&r["place"]),
            theta_bps: n(&r["theta_bps"]),
            psi_bps: n(&r["psi_bps"]),
            kind: n(&r["kind"]),
            class: n(&r["class"]),
            ms_cap_ppm: if r["ms_cap_ppm"].is_null() { 0 } else { n(&r["ms_cap_ppm"]) },
        };
    }
    Book {
        sqrt_p: n(&s["sqrt_p"]),
        sqrt_f: n(&s["sqrt_f"]),
        sqrt_l_max: n(&s["sqrt_l_max"]),
        floor: n(&s["floor"]),
        floor_tokens: n(&s["floor_tokens"]),
        n_bands: n(&s["n_bands"]),
        n_c: opt(&s["n_c"]),
        band_edge: arr_pad(&s["band_edge"]),
        band_money: arr_pad(&s["band_money"]),
        band_tokens: arr_pad(&s["band_tokens"]),
        band_f: if s["band_f"].is_null() { [0; BIDS_MAX] } else { arr_pad(&s["band_f"]) },
        band_f_claim: if s["band_f_claim"].is_null() { [0; BIDS_MAX] } else { arr_pad(&s["band_f_claim"]) },
        ck_at_f: opt(&s["ck_at_f"]),
        ck_floor_tokens: opt(&s["ck_floor_tokens"]),
        fee_ppm: arr(&s["fee_ppm"]),
        fee_start_ppm: if s["fee_start_ppm"].is_null() { [0; floor_math::v2::fees::FEE_LEGS] } else { arr(&s["fee_start_ppm"]) },
        sqrt_fee_a: opt(&s["sqrt_fee_a"]),
        sqrt_fee_b: opt(&s["sqrt_fee_b"]),
        graduated: s["graduated"].as_bool().unwrap(),
        own_fees: s["own_fees"].as_bool().unwrap_or(false),
        n_ranges: n(&s["n_ranges"]),
        ranges,
    }
}

pub fn oracle_book(s: &Value) -> Book {
    Book { own_fees: true, ..book(s) }
}

pub fn fits_u64(b: &Book) -> bool {
    let m = u64::MAX as u128;
    b.floor <= m
        && b.ck_at_f <= m
        && b.band_money.iter().all(|x| *x <= m)
        && b.band_f.iter().all(|x| *x <= m)
        && b.band_f_claim.iter().all(|x| *x <= m)
        && b.ranges.iter().all(|r| r.place <= m)
}

pub fn assert_refused_as_wide(id: &str, b: &Book) {
    let mut l = LaunchV2Account::default();
    match l.set_book(b) {
        Err(floor_amm_jupiter::v2::layout::LayoutV2Error::MoneyWide(_)) => {}
        r => panic!("{id}: filtered as wider than u64, but set_book gave {r:?}"),
    }
}

pub fn launch_circ(b: &Book, circ: u64) -> LaunchV2Account {
    let mut l = launch(b);
    l.supply = l.supply.max(b.for_sale() + b.bought_back() + circ);
    l
}

pub fn launch(b: &Book) -> LaunchV2Account {
    let r = floor_math::v2::preset::from_design(&floor_math::v2::rows::moonshot()).unwrap();
    let mut l = LaunchV2Account {
        version: VERSION_V2,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        quote_token_program: TOKEN_PROGRAM_ID,
        decimals: 6,
        quote_decimals: 6,
        supply: 1_000_000_000_000_000,
        trades: 7,
        incinerated: 3,
        phi_bps: r.phi_bps,
        ck_bps: r.ck_target_bps,
        wc_bps: r.wc_bps,
        ms_class: r.ms_class,
        flags: r.flags,
        platform_ms_bps: r.platform_ms_bps,
        floor_ms_bps: r.floor_ms_bps,
        band_width_bps: [300, 0, 0, 0],
        band_share_bps: [5_000, 0, 0, 0],
        pad_slot_rate: 0xAB,
        ..Default::default()
    };
    l.set_book(b).unwrap();
    l
}

pub fn amm(l: &LaunchV2Account) -> FloorAmmV2 {
    let key = keys::launch_pda(&l.base_mint);
    let acc = account(PROGRAM_ID, l.encode());
    let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &AmmContext::default()).unwrap();
    a.update(accounts([(key, acc)])).unwrap();
    a
}

pub fn qp(amount: u64, buy: bool) -> QuoteParams {
    let (i, o) = if buy { (QUOTE_MINT, BASE_MINT) } else { (BASE_MINT, QUOTE_MINT) };
    QuoteParams { amount, input_mint: i, output_mint: o, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() }
}

pub fn sol_buy(b: &mut Book, row: &floor_math::v2::split::SplitRow, sup: floor_math::v2::supply::Supply, gross: u128, min_tokens: u64) -> Result<floor_math::v2::trade::BookBuy, floor_math::MathError> {
    let mut u = floor_math::v2::book::Undo::EMPTY;
    floor_math::v2::trade::buy_steps_with(b, row, sup, gross, min_tokens, floor_math::v2::book::SOL_BUY_WALK_STEPS, &mut u)
}

pub fn sol_quote_buy(b: &Book, row: &floor_math::v2::split::SplitRow, sup: floor_math::v2::supply::Supply, gross: u128) -> Result<floor_math::v2::trade::BookBuy, floor_math::MathError> {
    let mut c = *b;
    sol_buy(&mut c, row, sup, gross, 0)
}
