//! The `buy` and `sell_back_wsol` instructions this crate builds, checked against `floor_launch`'s IDL
//! (`../packages/sdk-solana/idl/floor_launch.json`): the program address, the discriminators, the
//! argument layouts, every account's position, writable and signer flags, fixed addresses, every PDA
//! re-derived from the IDL's own seeds, and the sell-back's two error codes.

mod common;

use floor_types::ids::{DEVNET, MAINNET};
use std::collections::HashMap;

use common::*;
use floor_sale_amm::accounts::{event_authority_pda, launch_program, spl_token, token_2022_ata, wsol_ata};
use floor_sale_amm::encode::swap_variant;
use floor_sale_amm::{buy_instruction_data, sell_instruction_data, BUY_METAS, SELL_METAS};
use floor_types::layout::{BuyerPosition, Launch, Layout};
use jupiter_amm_interface::{Amm, SwapMode, SwapParams};
use serde_json::Value;
use solana_pubkey::Pubkey;

fn idl() -> Value {
    serde_json::from_str(&std::fs::read_to_string("../packages/sdk-solana/idl/floor_launch.json").expect("the floor_launch IDL")).unwrap()
}

fn ix<'a>(idl: &'a Value, name: &str) -> &'a Value {
    idl["instructions"].as_array().unwrap().iter().find(|i| i["name"] == name).unwrap()
}

fn bytes(v: &Value) -> Vec<u8> {
    v.as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect()
}

#[test]
fn the_program_address_and_account_discriminators_match() {
    let idl = idl();
    assert_eq!(idl["address"].as_str().unwrap().parse::<Pubkey>().unwrap(), launch_program(&MAINNET));
    for (name, disc) in [("Launch", Launch::DISCRIMINATOR), ("BuyerPosition", BuyerPosition::DISCRIMINATOR)] {
        let a = idl["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap();
        assert_eq!(bytes(&a["discriminator"]), disc.to_vec(), "{name}");
    }
}

#[test]
fn the_buy_instruction_matches_the_idl() {
    let idl = idl();
    let buy = ix(&idl, "buy");
    // data: discriminator, then three u64 arguments in this order
    let args: Vec<(String, String)> = buy["args"].as_array().unwrap().iter().map(|a| (a["name"].as_str().unwrap().into(), a["type"].as_str().unwrap().into())).collect();
    assert_eq!(args, [("lamports_in", "u64"), ("min_tokens", "u64"), ("max_graduation_lamports", "u64")].map(|(a, b)| (a.to_string(), b.to_string())));
    let data = buy_instruction_data(&swap_variant(true), 0x0102_0304_0506_0708, 0x1112_1314_1516_1718).unwrap();
    assert_eq!(&data[..8], bytes(&buy["discriminator"]).as_slice());
    assert_eq!(data.len(), 8 + 24);
    assert_eq!(&data[8..16], &0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(&data[16..24], &0x1112_1314_1516_1718u64.to_le_bytes());
    assert_eq!(&data[24..32], &[0u8; 8], "max_graduation_lamports is 0");

    // accounts
    let (mint, l) = default_sale(PRICE_ONE, 0, 100);
    let amm = amm_of(&l);
    let user = Pubkey::new_unique();
    let metas = amm
        .get_swap_and_account_metas(&SwapParams {
            swap_mode: SwapMode::ExactIn,
            in_amount: 1,
            out_amount: 1,
            source_mint: wsol(),
            destination_mint: mint,
            source_token_account: Pubkey::new_unique(),
            destination_token_account: token_2022_ata(&user, &mint),
            token_transfer_authority: user,
            user,
            payer: user,
            quote_mint_to_referrer: None,
            jupiter_program_id: &JUP,
            missing_dynamic_accounts_as_default: false,
        })
        .unwrap()
        .account_metas;
    let accounts = buy["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), BUY_METAS);
    assert_eq!(metas.len(), BUY_METAS);
    let by_name: HashMap<&str, Pubkey> = accounts.iter().zip(&metas).map(|(a, m)| (a["name"].as_str().unwrap(), m.pubkey)).collect();
    for (a, m) in accounts.iter().zip(&metas) {
        let name = a["name"].as_str().unwrap();
        assert_eq!(m.is_writable, a["writable"].as_bool().unwrap_or(false), "{name} writable");
        assert_eq!(m.is_signer, a["signer"].as_bool().unwrap_or(false), "{name} signer");
        if let Some(addr) = a["address"].as_str() {
            assert_eq!(m.pubkey, addr.parse::<Pubkey>().unwrap(), "{name} address");
        }
        if let Some(seeds) = a["pda"]["seeds"].as_array() {
            let parts: Vec<Vec<u8>> = seeds
                .iter()
                .map(|s| match s["kind"].as_str().unwrap() {
                    "const" => bytes(&s["value"]),
                    "account" => by_name[s["path"].as_str().unwrap()].to_bytes().to_vec(),
                    k => panic!("{name}: seed kind {k}"),
                })
                .collect();
            let refs: Vec<&[u8]> = parts.iter().map(|p| p.as_slice()).collect();
            assert_eq!(m.pubkey, Pubkey::find_program_address(&refs, &launch_program(&DEVNET)).0, "{name} PDA");
        }
    }
    assert_eq!(by_name["buyer"], user);
    assert_eq!(by_name["mint"], mint);
    assert_eq!(by_name["token_vault"], Pubkey::new_from_array(l.token_vault));
    assert_eq!(by_name["buyer_ata"], token_2022_ata(&user, &mint));
    assert_eq!(by_name["event_authority"], event_authority_pda(&DEVNET));
    assert_eq!(by_name["program"], launch_program(&DEVNET));
}

#[test]
fn the_sell_back_wsol_instruction_matches_the_idl() {
    let idl = idl();
    let sell = ix(&idl, "sell_back_wsol");
    let args: Vec<(String, String)> = sell["args"].as_array().unwrap().iter().map(|a| (a["name"].as_str().unwrap().into(), a["type"].as_str().unwrap().into())).collect();
    assert_eq!(args, vec![("tokens".to_string(), "u64".to_string())]);
    let data = sell_instruction_data(&swap_variant(false), 0x0102_0304_0506_0708).unwrap();
    assert_eq!(&data[..8], bytes(&sell["discriminator"]).as_slice());
    assert_eq!(data[8..], 0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(&data[..8], floor_types::disc::launch::SELL_BACK_WSOL.as_slice());
    // the named accounts are `sell_back`'s, in order, and the adapter's first 11 metas are them
    let named: Vec<&str> = sell["accounts"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    let plain: Vec<&str> = ix(&idl, "sell_back")["accounts"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(named, plain, "sell_back_wsol takes sell_back's accounts");
    let (mint, mut l) = default_sale(PRICE_ONE, 1_000_000, 100);
    l.floor_pool = Pubkey::new_unique().to_bytes();
    let amm = amm_of(&l);
    let user = Pubkey::new_unique();
    let metas = amm
        .get_swap_and_account_metas(&SwapParams {
            swap_mode: SwapMode::ExactIn,
            in_amount: 1,
            out_amount: 1,
            source_mint: mint,
            destination_mint: wsol(),
            source_token_account: token_2022_ata(&user, &mint),
            destination_token_account: wsol_ata(&user),
            token_transfer_authority: user,
            user,
            payer: user,
            quote_mint_to_referrer: None,
            jupiter_program_id: &JUP,
            missing_dynamic_accounts_as_default: false,
        })
        .unwrap()
        .account_metas;
    assert_eq!(metas.len(), SELL_METAS);
    let accounts = sell["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 11);
    let by_name: HashMap<&str, Pubkey> = accounts.iter().zip(&metas).map(|(a, m)| (a["name"].as_str().unwrap(), m.pubkey)).collect();
    for (a, m) in accounts.iter().zip(&metas) {
        let name = a["name"].as_str().unwrap();
        assert_eq!(m.is_writable, a["writable"].as_bool().unwrap_or(false), "{name} writable");
        assert_eq!(m.is_signer, a["signer"].as_bool().unwrap_or(false), "{name} signer");
        if let Some(addr) = a["address"].as_str() {
            assert_eq!(m.pubkey, addr.parse::<Pubkey>().unwrap(), "{name} address");
        }
        if let Some(seeds) = a["pda"]["seeds"].as_array() {
            let parts: Vec<Vec<u8>> = seeds
                .iter()
                .map(|s| match s["kind"].as_str().unwrap() {
                    "const" => bytes(&s["value"]),
                    "account" => by_name[s["path"].as_str().unwrap()].to_bytes().to_vec(),
                    k => panic!("{name}: seed kind {k}"),
                })
                .collect();
            let refs: Vec<&[u8]> = parts.iter().map(|p| p.as_slice()).collect();
            assert_eq!(m.pubkey, Pubkey::find_program_address(&refs, &launch_program(&DEVNET)).0, "{name} PDA");
        }
    }
    assert_eq!((by_name["buyer"], by_name["mint"], by_name["buyer_ata"]), (user, mint, token_2022_ata(&user, &mint)));
    assert_eq!((metas[11].pubkey, metas[11].is_writable, metas[12].pubkey, metas[12].is_writable), (wsol_ata(&user), true, spl_token(), false));
    // the program's two appended errors
    let codes: Vec<(u64, &str)> = idl["errors"].as_array().unwrap().iter().map(|e| (e["code"].as_u64().unwrap(), e["name"].as_str().unwrap())).collect();
    assert!(codes.contains(&(6099, "BadWsolDestination")) && codes.contains(&(6100, "WsolPayoutNotNative")), "{codes:?}");
}
