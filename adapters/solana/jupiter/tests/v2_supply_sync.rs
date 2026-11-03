// SPDX-License-Identifier: Apache-2.0
mod common;

use common::v2::*;
use common::{account, accounts, base_mint_account, mint_data, BASE_MINT};
use floor_amm_jupiter::keys::{self, PROGRAM_ID, TOKEN_PROGRAM_ID};
use floor_amm_jupiter::v2::{FloorAmmV2, LaunchV2Account};
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount};

const CIRC: u64 = 1_000_000_000_000;

fn amm_mint(l: &LaunchV2Account, mint: solana_account::Account) -> Result<FloorAmmV2, jupiter_amm_interface::AmmError> {
    let key = keys::launch_pda(&l.base_mint);
    let acc = account(PROGRAM_ID, l.encode());
    let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &AmmContext::default()).unwrap();
    a.update(accounts([(key, acc), (BASE_MINT, mint)]))?;
    Ok(a)
}

#[test]
fn an_unsynced_burn_is_quoted_as_the_program_syncs_it() {
    let g = load("book");
    let (mut seen, mut moved) = (0, 0);
    for c in g["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let b = book(&c["state_in"]);
        if !fits_u64(&b) {
            continue;
        }
        let l = launch_circ(&b, CIRC);
        let burned = l.supply - CIRC / 2;
        let a = amm_mint(&l, base_mint_account(burned)).unwrap();
        assert_eq!((a.launch().supply, a.launch().incinerated), (burned, l.incinerated + CIRC / 2), "{id}: the fall is synced, added to incinerated");
        let mut synced = l.clone();
        synced.supply = burned;
        for gross in [1_000_000u64, 1_000_000_000, 1_000_000_000_000] {
            let (Ok(prog), Ok(stale)) = (sol_quote_buy(&b, &l.split_row(), synced.supply(), gross as u128), sol_quote_buy(&b, &l.split_row(), l.supply(), gross as u128)) else { continue };
            seen += 1;
            assert_eq!(a.simulate_buy(gross).unwrap(), prog, "{id} {gross}: the buy is the program's on the synced supply");
            moved += (prog != stale) as u32;
        }
    }
    eprintln!("buys {seen}, receipts the burn moved {moved}");
    assert!(moved >= 1, "no buy's receipt depends on the supply: the sync is untested ({seen} buys)");
}

#[test]
fn a_mint_supply_at_or_above_the_stored_one_changes_nothing() {
    let b = book(&load("book")["cases"].as_array().unwrap()[0]["state_in"]);
    let l = launch_circ(&b, CIRC);
    for s in [l.supply, l.supply + 1, u64::MAX] {
        let a = amm_mint(&l, base_mint_account(s)).unwrap();
        assert_eq!((a.launch().supply, a.launch().incinerated), (l.supply, l.incinerated));
    }
}

#[test]
fn a_base_mint_that_is_not_token_2022_or_overflows_refuses_the_update() {
    let b = book(&load("book")["cases"].as_array().unwrap()[0]["state_in"]);
    let l = launch_circ(&b, CIRC);
    let a = amm_mint(&l, account(TOKEN_PROGRAM_ID, mint_data(1, 6)));
    assert!(a.is_err(), "a launch token is Token-2022");
    let mut o = l.clone();
    o.incinerated = u64::MAX;
    assert!(amm_mint(&o, base_mint_account(1)).is_err(), "the program refuses Overflow");
}
