// SPDX-License-Identifier: Apache-2.0
use floor_amm_solana::hook::{holder_position_pda, FLOOR_HOOK_ID, SWAP_METAS_V2, SWAP_METAS_V2_QUOTE_HOOKED};
use floor_amm_solana::keys::{self, ata, launch_pda, trader_link_pda, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use floor_amm_solana::metas::{full_swap_metas, swap_metas, trader_party, SwapAccounts};
use floor_amm_solana::SWAP_METAS;
use solana_pubkey::Pubkey;

fn k(n: u8) -> Pubkey {
    Pubkey::new_from_array([n; 32])
}

const BASE: u8 = 1;
const QUOTE: u8 = 2;

fn launch() -> Pubkey {
    launch_pda(&k(BASE))
}

#[test]
fn holder_is_the_user_for_the_users_token_2022_ata_else_the_authority() {
    let (user, auth) = (k(10), k(11));
    let a = SwapAccounts { user, token_transfer_authority: auth, source_token_account: k(20), destination_token_account: k(21) };
    assert_eq!(trader_party(&ata(&user, &k(BASE), &TOKEN_2022_PROGRAM_ID), &a, &k(BASE), &TOKEN_2022_PROGRAM_ID), user);
    assert_eq!(trader_party(&k(20), &a, &k(BASE), &TOKEN_2022_PROGRAM_ID), auth);
    assert_eq!(trader_party(&ata(&user, &k(BASE), &TOKEN_PROGRAM_ID), &a, &k(BASE), &TOKEN_2022_PROGRAM_ID), auth);
    assert_eq!(trader_party(&ata(&user, &k(QUOTE), &TOKEN_2022_PROGRAM_ID), &a, &k(BASE), &TOKEN_2022_PROGRAM_ID), auth);
}

#[test]
fn a_router_authority_that_is_not_the_user_names_the_users_position_on_the_users_ata() {
    let (user, auth) = (k(10), k(11));
    let user_ata = ata(&user, &k(BASE), &TOKEN_2022_PROGRAM_ID);
    let buy = SwapAccounts { user, token_transfer_authority: auth, source_token_account: k(20), destination_token_account: user_ata };
    let m = full_swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_PROGRAM_ID, false, &buy, true);
    assert_eq!(m.len(), SWAP_METAS_V2);
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&user));
    assert_eq!(m[SWAP_METAS + 4].pubkey, holder_position_pda(&launch(), &user));
    assert_eq!(m[SWAP_METAS + 5].pubkey, FLOOR_HOOK_ID);
    let other = SwapAccounts { destination_token_account: k(21), ..buy };
    let m = full_swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_PROGRAM_ID, false, &other, true);
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&auth));
    assert_eq!(m[SWAP_METAS + 4].pubkey, holder_position_pda(&launch(), &auth));
}

#[test]
fn a_sell_from_the_users_ata_names_the_users_source_position() {
    let (user, auth) = (k(10), k(11));
    let a = SwapAccounts { user, token_transfer_authority: auth, source_token_account: ata(&user, &k(BASE), &TOKEN_2022_PROGRAM_ID), destination_token_account: k(21) };
    let m = full_swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_PROGRAM_ID, false, &a, false);
    assert_eq!(m[SWAP_METAS + 3].pubkey, holder_position_pda(&launch(), &user));
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&auth));
    let a2 = SwapAccounts { destination_token_account: ata(&user, &k(QUOTE), &TOKEN_PROGRAM_ID), ..a };
    let m = swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_PROGRAM_ID, &a2, false);
    assert_eq!(m[SWAP_METAS - 1].pubkey, trader_link_pda(&user));
}

#[test]
fn a_hooked_quote_names_the_users_quote_position_on_the_users_quote_ata() {
    let (user, auth) = (k(10), k(11));
    let qlaunch = launch_pda(&k(QUOTE));
    let a = SwapAccounts { user, token_transfer_authority: auth, source_token_account: ata(&user, &k(QUOTE), &TOKEN_2022_PROGRAM_ID), destination_token_account: k(21) };
    let m = full_swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_2022_PROGRAM_ID, true, &a, true);
    assert_eq!(m.len(), SWAP_METAS_V2_QUOTE_HOOKED);
    assert_eq!(m[SWAP_METAS_V2 + 3].pubkey, holder_position_pda(&qlaunch, &user));
    assert_eq!(m[SWAP_METAS_V2 + 4].pubkey, holder_position_pda(&qlaunch, &launch()));
    let b = SwapAccounts { source_token_account: k(20), ..a };
    let m = full_swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_2022_PROGRAM_ID, true, &b, true);
    assert_eq!(m[SWAP_METAS_V2 + 3].pubkey, holder_position_pda(&qlaunch, &auth));
}

#[test]
fn the_first_nineteen_do_not_depend_on_the_hook() {
    let a = SwapAccounts { user: k(10), token_transfer_authority: k(10), source_token_account: k(20), destination_token_account: k(21) };
    let m = swap_metas(&launch(), &k(BASE), &k(QUOTE), &TOKEN_PROGRAM_ID, &a, true);
    assert_eq!(m.len(), SWAP_METAS);
    assert_eq!(m[0].pubkey, k(10));
    assert!(m[0].is_signer && m[14].is_signer);
    assert_eq!(m[12].pubkey, keys::event_authority_pda());
}
