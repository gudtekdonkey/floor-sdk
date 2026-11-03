// SPDX-License-Identifier: Apache-2.0
mod fixture;

use floor_amm_solana::encode::{BUY_DISCRIMINATOR, SELL_DISCRIMINATOR, instruction_data};
use floor_amm_solana::hook::SWAP_METAS_V2;
use floor_amm_solana::keys;
use floor_amm_solana::quote_mint::{EXT_TRANSFER_FEE, EXT_TRANSFER_HOOK};
use titan_integration_template::your_venue::FloorVenue;
use titan_integration_template::swap_route::{Venue, protocol_to_venue};
use titan_integration_template::your_venue::{LABEL, program_id, to_titan};
use solana_account::Account;
use solana_pubkey::Pubkey;
use titan_integration_template::trading_venue::{AddressLookupTableTrait, FromAccount, QuoteRequest, SwapType, TradingVenue};

fn req(i: Pubkey, o: Pubkey, amount: u64) -> QuoteRequest {
    QuoteRequest { input_mint: i, output_mint: o, amount, swap_type: SwapType::ExactIn }
}

fn t22_mint_with(ext: u16, value: Vec<u8>, decimals: u8) -> Account {
    let mut a = fixture::mint_account(to_titan(&keys::TOKEN_2022_PROGRAM_ID), 1 << 50, decimals);
    a.data.resize(165, 0);
    a.data.push(1);
    a.data.extend_from_slice(&ext.to_le_bytes());
    a.data.extend_from_slice(&(value.len() as u16).to_le_bytes());
    a.data.extend_from_slice(&value);
    a
}

async fn venue_with_quote(quote_mint: Account, t22: bool) -> FloorVenue {
    let s = fixture::states().into_iter().find(|s| s.name.ends_with("stacks")).unwrap();
    let mut l = fixture::launch_account(&s);
    if t22 {
        l.quote_token_program = keys::TOKEN_2022_PROGRAM_ID;
    }
    let (key, cache) = fixture::cache_for(&l, quote_mint);
    let mut v = FloorVenue::from_account(&key, &cache.0[&key]).unwrap();
    v.update_state(&cache).await.expect("update_state");
    v
}

#[tokio::test]
async fn a_launch_token_quote_and_a_transfer_fee_quote_are_not_routable_yet() {
    let mut hook = vec![0u8; 64];
    hook[32..].copy_from_slice(floor_amm_solana::hook::FLOOR_HOOK_ID.as_ref());
    let hooked = venue_with_quote(t22_mint_with(EXT_TRANSFER_HOOK, hook, 6), true).await;
    assert!(!hooked.initialized(), "a FLOOR Program token as the quote waits on Titan's hook answer");
    let fee = venue_with_quote(t22_mint_with(EXT_TRANSFER_FEE, vec![0u8; 108], 6), true).await;
    assert!(!fee.initialized(), "a transfer-fee quote waits on Titan's hook answer");
    for v in [hooked, fee] {
        let (b, q) = (v.base_mint(), v.quote_mint());
        assert!(v.quote(req(q, b, 1_000)).is_err(), "a venue that is not initialized quotes nothing");
    }
}

#[tokio::test]
async fn a_launch_that_is_not_live_is_not_routable() {
    let s = fixture::states().into_iter().find(|s| s.name.ends_with("stacks")).unwrap();
    let mut l = fixture::launch_account(&s);
    l.status = 1;
    let (key, cache) = fixture::cache_for(&l, fixture::mint_account(to_titan(&keys::TOKEN_PROGRAM_ID), 1 << 50, 6));
    let mut v = FloorVenue::from_account(&key, &cache.0[&key]).unwrap();
    v.update_state(&cache).await.unwrap();
    assert!(!v.initialized());
}

#[tokio::test]
async fn from_account_refuses_another_programs_account_and_a_wrong_key() {
    let s = &fixture::states()[0];
    let l = fixture::launch_account(s);
    let (key, cache) = fixture::cache_for(&l, fixture::mint_account(to_titan(&keys::TOKEN_PROGRAM_ID), 1, 6));
    let mut acc = cache.0[&key].clone();
    assert!(FloorVenue::from_account(&Pubkey::new_unique(), &acc).is_err(), "not the mint's launch PDA");
    acc.owner = Pubkey::new_unique();
    assert!(FloorVenue::from_account(&key, &acc).is_err(), "not the program's account");
}

#[tokio::test]
async fn the_swap_instruction_is_the_programs_buy_and_sell() {
    let s = fixture::states().into_iter().find(|s| s.name.ends_with("stacks")).unwrap();
    let (v, l) = fixture::venue(&s).await;
    let user = Pubkey::new_unique();
    let (b, q) = (v.base_mint(), v.quote_mint());
    for (is_buy, i, o) in [(true, q, b), (false, b, q)] {
        let ix = v.generate_swap_instruction(req(i, o, 123_456), user).unwrap();
        assert_eq!(ix.program_id, program_id());
        assert_eq!(ix.accounts.len(), SWAP_METAS_V2, "25 metas: the trade's 19 + the hook's six");
        assert_eq!(ix.data, instruction_data(is_buy, 123_456, 0).to_vec());
        assert_eq!(ix.accounts[0].pubkey, user, "the transfer authority signs");
        assert!(ix.accounts[0].is_signer);
        assert_eq!(ix.accounts[1].pubkey, v.key);
        let base_ata = to_titan(&keys::ata(&titan_integration_template::your_venue::to_core(&user), &l.base_mint, &keys::TOKEN_2022_PROGRAM_ID));
        assert_eq!(ix.accounts[6].pubkey, base_ata, "trader_base is the user's launch-token ATA");
        assert_eq!(protocol_to_venue(&v, &req(i, o, 1)).unwrap(), Venue::FloorAmm { is_buy });
    }
    assert_eq!(v.label(), LABEL);
    assert!(v.generate_swap_instruction(QuoteRequest { swap_type: SwapType::ExactOut, ..req(q, b, 1) }, user).is_err());
    assert!(v.quote(QuoteRequest { swap_type: SwapType::ExactOut, ..req(q, b, 1) }).is_err(), "exact-out is refused, never a panic");
    assert!(v.quote(req(q, Pubkey::new_unique(), 1)).is_err(), "another mint is refused");
    let alt = v.get_lookup_table_keys(None).await.unwrap();
    assert!(alt.contains(&program_id()) && alt.contains(&v.key) && alt.contains(&b) && alt.contains(&q));
    assert!(v.program_dependencies().contains(&to_titan(&floor_amm_solana::hook::FLOOR_HOOK_ID)));
}

#[test]
fn the_route_venue_bytes_match_the_program_enum_position() {
    assert_eq!(borsh::to_vec(&Venue::RaydiumAmm).unwrap(), vec![0]);
    assert_eq!(borsh::to_vec(&Venue::FloorAmm { is_buy: false }).unwrap(), vec![1, 0]);
    assert_eq!(borsh::to_vec(&Venue::FloorAmm { is_buy: true }).unwrap(), vec![1, 1]);
}

#[test]
fn the_cpi_adapter_names_the_programs_id_and_discriminators() {
    let src = include_str!("../program-template/programs/titan-v3-venue-template/src/instructions/venues/floor.rs");
    let arr = |d: [u8; 8]| format!("[{}]", d.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", "));
    assert!(src.contains(&arr(BUY_DISCRIMINATOR)), "buy discriminator");
    assert!(src.contains(&arr(SELL_DISCRIMINATOR)), "sell discriminator");
    assert!(src.contains(&format!("pubkey!(\"{}\")", keys::PROGRAM_ID)), "program id");
}
