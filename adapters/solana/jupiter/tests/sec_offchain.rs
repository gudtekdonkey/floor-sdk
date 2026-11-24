// SPDX-License-Identifier: Apache-2.0
mod common;

use common::*;
use floor_amm_jupiter::keys::{self, PROGRAM_ID};
use floor_amm_jupiter::swap_instruction_data;
use floor_amm_jupiter::v2::FloorAmmV2;
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount};

#[test]
fn poc_sec_off_d14_v2_n_ranges_over_48_panics_the_quote() {
    let g = common::v2::load("book");
    let b = common::v2::book(&g["cases"].as_array().unwrap()[0]["state_in"]);
    let mut l = common::v2::launch(&b);
    assert!(common::v2::amm(&l).quote(&common::v2::qp(1_000_000, true)).is_ok());
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for n in [49u8, 200, 255] {
        l.n_ranges = n;
        let acc = account(PROGRAM_ID, l.encode());
        let key = keys::launch_pda(&BASE_MINT);
        let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &AmmContext::default()).expect("decoded");
        a.update(accounts([(key, acc)])).expect("updated");
        assert!(std::panic::catch_unwind(|| a.quote(&common::v2::qp(1_000_000, true))).is_err(), "n_ranges {n}: buy panics");
        assert!(std::panic::catch_unwind(|| a.quote(&common::v2::qp(1_000_000, false))).is_err(), "n_ranges {n}: sell panics");
    }
    std::panic::set_hook(prev);
}

#[test]
fn poc_sec_off_d15_the_encoder_writes_min_out_zero() {
    for is_buy in [true, false] {
        let d = swap_instruction_data(&floor_amm_jupiter::encode::placeholder(is_buy), 5_000).unwrap();
        assert_eq!(u64::from_le_bytes(d[8..16].try_into().unwrap()), 5_000);
        assert_eq!(u64::from_le_bytes(d[16..24].try_into().unwrap()), 0, "min-out is 0");
    }
}

#[test]
fn refuted_d18_huge_amounts_do_not_panic() {
    let g = common::v2::load("book");
    for c in g["cases"].as_array().unwrap().iter().take(20) {
        let b = common::v2::book(&c["state_in"]);
        let v = common::v2::amm(&common::v2::launch(&b));
        for buy in [true, false] {
            assert!(std::panic::catch_unwind(|| v.quote(&common::v2::qp(u64::MAX, buy))).is_ok());
        }
    }
}
