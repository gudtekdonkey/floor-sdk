// SPDX-License-Identifier: Apache-2.0
mod common;

use common::*;
use floor_amm_jupiter::keys::{self, ata, trader_link_pda, PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use floor_amm_jupiter::v2::layout::VERSION_V2;
use floor_amm_jupiter::v2::{holder_position_pda, FloorAmmV2, LaunchV2Account};
use floor_amm_jupiter::SWAP_METAS;
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, SwapMode, SwapParams};
use solana_pubkey::Pubkey;

fn amm() -> FloorAmmV2 {
    let l = LaunchV2Account { version: VERSION_V2, base_mint: BASE_MINT, quote_mint: QUOTE_MINT, quote_token_program: TOKEN_PROGRAM_ID, decimals: 6, quote_decimals: 6, ..Default::default() };
    let key = keys::launch_pda(&BASE_MINT);
    let acc = account(PROGRAM_ID, l.encode());
    let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &AmmContext::default()).unwrap();
    a.update(accounts([(key, acc)])).unwrap();
    a
}

fn params<'a>(jup: &'a Pubkey, buy: bool, src: Pubkey, dst: Pubkey, user: Pubkey, auth: Pubkey) -> SwapParams<'a, 'a> {
    let (i, o) = if buy { (QUOTE_MINT, BASE_MINT) } else { (BASE_MINT, QUOTE_MINT) };
    SwapParams {
        swap_mode: SwapMode::ExactIn, in_amount: 1_000_000, out_amount: 0, source_mint: i, destination_mint: o, source_token_account: src,
        destination_token_account: dst, token_transfer_authority: auth, user, payer: user, quote_mint_to_referrer: None, jupiter_program_id: jup,
        missing_dynamic_accounts_as_default: false,
    }
}

#[test]
fn a_buy_into_the_users_ata_through_a_router_authority_names_the_user() {
    let (user, auth, jup) = (Pubkey::new_from_array([7; 32]), Pubkey::new_from_array([8; 32]), Pubkey::new_from_array([9; 32]));
    let a = amm();
    let launch = keys::launch_pda(&BASE_MINT);
    let users_ata = ata(&user, &BASE_MINT, &TOKEN_2022_PROGRAM_ID);
    let m = a.get_swap_and_account_metas(&params(&jup, true, Pubkey::new_unique(), users_ata, user, auth)).unwrap().account_metas;
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&user), "rem[4]");
    assert_eq!(m[SWAP_METAS + 4].pubkey, holder_position_pda(&launch, &user), "the hook's destination position");
    let m = a.get_swap_and_account_metas(&params(&jup, true, Pubkey::new_unique(), Pubkey::new_unique(), user, auth)).unwrap().account_metas;
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&auth));
    assert_eq!(m[SWAP_METAS + 4].pubkey, holder_position_pda(&launch, &auth));
}

#[test]
fn a_sell_from_the_users_ata_through_a_router_authority_names_the_user() {
    let (user, auth, jup) = (Pubkey::new_from_array([7; 32]), Pubkey::new_from_array([8; 32]), Pubkey::new_from_array([9; 32]));
    let a = amm();
    let launch = keys::launch_pda(&BASE_MINT);
    let m = a.get_swap_and_account_metas(&params(&jup, false, ata(&user, &BASE_MINT, &TOKEN_2022_PROGRAM_ID), Pubkey::new_unique(), user, auth)).unwrap().account_metas;
    assert_eq!(m[SWAP_METAS + 3].pubkey, holder_position_pda(&launch, &user), "the hook's source position");
}
