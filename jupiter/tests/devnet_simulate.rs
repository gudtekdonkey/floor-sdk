//! A buy built by the adapter, simulated on devnet against a live sale (read-only: the transaction
//! is never signed or sent; `simulateTransaction` runs with `sigVerify: false`).
//!
//! Skipped unless both are set:
//!
//! ```bash
//! RPC=<devnet url> SIMULATE_PAYER=<a funded throwaway address> cargo test --test devnet_simulate -- --nocapture
//! ```
//!
//! `SIMULATE_PAYER` is the wallet the buy is simulated for: it pays the input, the rent of its
//! position and token account, and the fee, so it must hold devnet SOL (an airdropped throwaway
//! address; its key is not needed). The test reads each sale's live `Launch`, quotes 0.01 SOL, builds
//! the instruction from `get_swap_and_account_metas`, and asks the cluster to simulate it, returning
//! the post-state of the payer's token account; the simulated delivery must equal the quote.

mod common;

use base64::Engine;
use common::*;
use floor_sale_amm::accounts::token_2022_ata;
use floor_sale_amm::{buy_instruction_data, FloorSaleAmm};
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, QuoteParams, SwapMode, SwapParams};
use serde_json::{json, Value};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

fn rpc(url: &str, method: &str, params: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let r: Value = ureq::post(url).send_json(body).expect("rpc").into_json().expect("json");
    if let Some(e) = r.get("error") {
        panic!("{method}: {e}");
    }
    r["result"].clone()
}

fn compact(n: usize, out: &mut Vec<u8>) {
    let mut v = n;
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

/// A legacy transaction (one zeroed signature) with `payer` first: header, keys (signer-writable,
/// signer-readonly, writable, readonly), blockhash, instructions.
fn legacy_tx(payer: &Pubkey, ixs: &[Instruction], blockhash: [u8; 32]) -> Vec<u8> {
    let mut keys: Vec<(Pubkey, bool, bool)> = vec![(*payer, true, true)];
    let mut add = |k: Pubkey, s: bool, w: bool| {
        if let Some(e) = keys.iter_mut().find(|e| e.0 == k) {
            e.1 |= s;
            e.2 |= w;
        } else {
            keys.push((k, s, w));
        }
    };
    for ix in ixs {
        for m in &ix.accounts {
            add(m.pubkey, m.is_signer, m.is_writable);
        }
        add(ix.program_id, false, false);
    }
    let rank = |k: &(Pubkey, bool, bool)| match (k.1, k.2) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    };
    let payer_first = keys.remove(0);
    keys.sort_by_key(rank);
    keys.insert(0, payer_first);
    let signers = keys.iter().filter(|k| k.1).count();
    assert_eq!(signers, 1, "the payer is the only signer");
    let ro_signed = keys.iter().filter(|k| k.1 && !k.2).count();
    let ro_unsigned = keys.iter().filter(|k| !k.1 && !k.2).count();
    let index = |k: &Pubkey| keys.iter().position(|e| e.0 == *k).unwrap() as u8;
    let mut msg = vec![signers as u8, ro_signed as u8, ro_unsigned as u8];
    compact(keys.len(), &mut msg);
    for k in &keys {
        msg.extend_from_slice(k.0.as_ref());
    }
    msg.extend_from_slice(&blockhash);
    compact(ixs.len(), &mut msg);
    for ix in ixs {
        msg.push(index(&ix.program_id));
        compact(ix.accounts.len(), &mut msg);
        for m in &ix.accounts {
            msg.push(index(&m.pubkey));
        }
        compact(ix.data.len(), &mut msg);
        msg.extend_from_slice(&ix.data);
    }
    let mut tx = Vec::new();
    compact(1, &mut tx);
    tx.extend_from_slice(&[0u8; 64]);
    tx.extend_from_slice(&msg);
    tx
}

#[test]
fn a_buy_simulates_on_devnet_as_quoted() {
    let (Ok(url), Ok(payer)) = (std::env::var("RPC"), std::env::var("SIMULATE_PAYER")) else {
        eprintln!("SKIPPED: set RPC and SIMULATE_PAYER to simulate on devnet");
        return;
    };
    let payer: Pubkey = payer.parse().expect("SIMULATE_PAYER");
    let amount = 10_000_000; // 0.01 SOL
    let mut simulated = 0;
    for (launch, mint) in SALES {
        let (launch, mint) = (pk(launch), pk(mint));
        let Some(account) = jupiter_amm_test_kit::fetch_account(&url, &launch).expect("fetch") else { continue };
        let amm = FloorSaleAmm::from_keyed_account(&KeyedAccount { key: launch, account, params: None }, &AmmContext::default()).unwrap();
        if !amm.is_active() {
            eprintln!("{launch}: no longer a routable sale");
            continue;
        }
        let q = amm.quote(&QuoteParams { amount, input_mint: wsol(), output_mint: mint, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() }).unwrap();
        let ata = token_2022_ata(&payer, &mint);
        let sam = amm
            .get_swap_and_account_metas(&SwapParams {
                swap_mode: SwapMode::ExactIn,
                in_amount: amount,
                out_amount: q.out_amount,
                source_mint: wsol(),
                destination_mint: mint,
                source_token_account: Pubkey::new_unique(),
                destination_token_account: ata,
                token_transfer_authority: payer,
                user: payer,
                payer,
                quote_mint_to_referrer: None,
                jupiter_program_id: &JUP,
                missing_dynamic_accounts_as_default: false,
            })
            .unwrap();
        let buy = Instruction { program_id: amm.program_id(), accounts: sam.account_metas, data: buy_instruction_data(&sam.swap, amount, q.out_amount).unwrap() };
        let ixs = [ComputeBudgetInstruction::set_compute_unit_limit(200_000), buy];
        let tx = legacy_tx(&payer, &ixs, [0u8; 32]);
        let b64 = base64::engine::general_purpose::STANDARD.encode(tx);
        let r = rpc(
            &url,
            "simulateTransaction",
            json!([b64, { "encoding": "base64", "sigVerify": false, "replaceRecentBlockhash": true, "commitment": "processed",
                          "accounts": { "encoding": "base64", "addresses": [ata.to_string()] } }]),
        );
        let v = &r["value"];
        eprintln!("{launch}: err {} units {}", v["err"], v["unitsConsumed"]);
        assert!(v["err"].is_null(), "simulation failed: {}", v["logs"]);
        let data = base64::engine::general_purpose::STANDARD.decode(v["accounts"][0]["data"][0].as_str().unwrap()).unwrap();
        let delivered = u64::from_le_bytes(data[64..72].try_into().unwrap());
        eprintln!("  0.01 SOL -> quoted {} tokens, simulated delivery {delivered}", q.out_amount);
        assert_eq!(delivered, q.out_amount);
        simulated += 1;
    }
    assert!(simulated > 0, "no routable sale on the cluster");
}
