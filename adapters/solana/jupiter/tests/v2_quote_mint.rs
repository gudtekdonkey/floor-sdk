// SPDX-License-Identifier: Apache-2.0
mod common;

use std::sync::atomic::Ordering::Relaxed;

use common::v2::*;
use common::{account, accounts, mint_data, BASE_MINT, QUOTE_MINT};
use floor_amm_jupiter::keys::{self, PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use floor_amm_jupiter::v2::hook::{holder_position_pda, hook_metas, trade_hook_metas};
use floor_amm_jupiter::v2::quote_mint::{EXT_TRANSFER_FEE, EXT_TRANSFER_HOOK};
use floor_amm_jupiter::v2::{FloorAmmV2, LaunchV2Account, QuoteMint, FLOOR_HOOK_ID, SWAP_METAS_V2, SWAP_METAS_V2_QUOTE_HOOKED};
use floor_amm_jupiter::SWAP_METAS;
use floor_math::v2::book::{MAX_WALK_STEPS, SOL_BUY_WALK_STEPS};
use floor_math::v2::create::{create, CreateCtx};
use floor_math::v2::preset::from_design;
use floor_math::v2::rows;
use floor_math::v2::trade;
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, SwapMode, SwapParams};
use solana_pubkey::Pubkey;

fn mint_with(ext: u16, v: &[u8]) -> Vec<u8> {
    let mut d = mint_data(0, 6);
    d.resize(166, 0);
    d[165] = 1;
    d.extend_from_slice(&ext.to_le_bytes());
    d.extend_from_slice(&(v.len() as u16).to_le_bytes());
    d.extend_from_slice(v);
    d
}

fn fee_config(older: (u64, u64, u16), newer: (u64, u64, u16)) -> Vec<u8> {
    let mut v = vec![0u8; 108];
    for (o, (e, m, b)) in [(72, older), (90, newer)] {
        v[o..o + 8].copy_from_slice(&e.to_le_bytes());
        v[o + 8..o + 16].copy_from_slice(&m.to_le_bytes());
        v[o + 16..o + 18].copy_from_slice(&b.to_le_bytes());
    }
    v
}

fn amm_with_quote(l: &LaunchV2Account, quote: Vec<u8>, epoch: u64) -> FloorAmmV2 {
    let mut l = *l;
    l.quote_token_program = TOKEN_2022_PROGRAM_ID;
    let key = keys::launch_pda(&l.base_mint);
    let acc = account(PROGRAM_ID, l.encode());
    let ctx = AmmContext::default();
    ctx.clock_ref.epoch.store(epoch, Relaxed);
    let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &ctx).unwrap();
    a.update(accounts([(key, acc), (QUOTE_MINT, account(TOKEN_2022_PROGRAM_ID, quote))])).unwrap();
    a
}

fn first_book() -> LaunchV2Account {
    let g = load("book");
    launch(&book(&g["cases"].as_array().unwrap()[0]["state_in"]))
}

#[test]
fn a_buy_walks_at_most_sixteen_pieces_and_refunds_the_rest() {
    let g = load("oracle/launch_book");
    let mut books: Vec<(String, floor_math::v2::book::Book)> = g["cases"].as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap().to_string(), book(&c["state_in"]))).collect();
    for (name, row) in [("wagmi", rows::wagmi()), ("moonshot", rows::moonshot()), ("hodl", rows::hodl())] {
        let rv = from_design(&row).unwrap();
        let ctx = CreateCtx { quote_decimals: 6, quote_usd_cents_x64: 100u128 << 64, token_decimals: 6, clock: Default::default() };
        books.push((format!("fresh {name}"), create(&rv, &ctx).unwrap().book));
    }
    let (mut capped, mut seen) = (0, 0);
    for (id, b) in &books {
        let l = launch(b);
        let a = amm(&l);
        let (row, sup) = (l.split_row(), l.supply());
        for gross in [u64::MAX / 4, 1u64 << 50, 1u64 << 40, 10_000_000_000_000] {
            let (Ok(sol), Ok(all)) = (sol_quote_buy(b, &row, sup, gross as u128), trade::quote_buy(b, &row, sup, gross as u128)) else { continue };
            seen += 1;
            let r = a.simulate_buy(gross).unwrap();
            assert_eq!(r, sol, "{id} {gross}: the adapter's buy is the program's 16-step walk");
            assert!(r.steps <= SOL_BUY_WALK_STEPS, "{id}: {} steps", r.steps);
            let q = a.quote(&qp(gross, true)).unwrap();
            assert_eq!((q.in_amount as u128, q.out_amount), (r.gross - r.refund, r.tokens), "{id}: the quote is the capped buy");
            if all.steps > SOL_BUY_WALK_STEPS {
                capped += 1;
                assert!(all.steps <= MAX_WALK_STEPS);
                assert!(r.tokens < all.tokens && r.refund > all.refund, "{id} {gross}: the cap fills less and refunds more ({r:?} vs {all:?})");
            }
        }
    }
    eprintln!("buys {seen}, past 16 pieces {capped}");
    assert!(capped >= 1, "no launch_book buy walks past 16 pieces: the cap is untested ({seen} buys)");
}

#[test]
fn the_transfer_fee_is_the_programs() {
    let q = QuoteMint::decode(&TOKEN_2022_PROGRAM_ID, &mint_with(EXT_TRANSFER_FEE, &fee_config((0, 0, 0), (10, 700, 50))));
    assert_eq!(q.transfer_fee(9, 1_000_000), 0, "the older fee before the newer one's epoch");
    assert_eq!(q.transfer_fee(10, 1_000), 5);
    assert_eq!(q.transfer_fee(10, 1_001), 6, "rounded up");
    assert_eq!(q.transfer_fee(11, 1_000_000), 700, "capped at its maximum");
    assert_eq!(QuoteMint::decode(&keys::TOKEN_PROGRAM_ID, &mint_data(0, 6)), QuoteMint::default(), "SPL Token");
    assert!(!q.hooked);
}

#[test]
fn a_fee_quote_is_netted_as_the_program_nets_it() {
    let l = first_book();
    let (row, sup) = (l.split_row(), l.supply());
    let b = l.book();
    let mint = mint_with(EXT_TRANSFER_FEE, &fee_config((0, 0, 0), (5, u64::MAX, 100)));
    for (epoch, bps) in [(0u64, 0u128), (5, 100)] {
        let a = amm_with_quote(&l, mint.clone(), epoch);
        for amount in [1_000_000u64, 123_456_789, 50_000_000_000] {
            let fee = |x: u64| ((x as u128 * bps).div_ceil(10_000)) as u64;
            let r = sol_quote_buy(&b, &row, sup, (amount - fee(amount)) as u128).unwrap();
            let refund = r.refund as u64;
            let q = a.quote(&qp(amount, true)).unwrap();
            assert_eq!(q.in_amount, amount - (refund - fee(refund)), "epoch {epoch} buy {amount}: sent less the refund that arrives");
            assert_eq!(q.out_amount, r.tokens, "epoch {epoch} buy {amount}");
            let tokens = r.tokens / 2;
            let s = trade::quote_sell(&b, &row, sup, tokens).unwrap();
            let out = s.out as u64;
            let q = a.quote(&qp(tokens, false)).unwrap();
            assert_eq!(q.out_amount, out - fee(out), "epoch {epoch} sell {tokens}: the payout that arrives");
        }
    }
    let a = amm_with_quote(&l, mint_with(EXT_TRANSFER_FEE, &fee_config((0, 0, 10_000), (0, u64::MAX, 10_000))), 0);
    assert!(a.quote(&qp(1_000_000, true)).unwrap_err().to_string().contains("QuoteTransferShort"));
}

fn swap<'a>(jup: &'a Pubkey, is_buy: bool, auth: Pubkey) -> SwapParams<'a, 'a> {
    let (i, o) = if is_buy { (QUOTE_MINT, BASE_MINT) } else { (BASE_MINT, QUOTE_MINT) };
    SwapParams {
        swap_mode: SwapMode::ExactIn,
        in_amount: 1_000_000,
        out_amount: 0,
        source_mint: i,
        destination_mint: o,
        source_token_account: Pubkey::new_unique(),
        destination_token_account: Pubkey::new_unique(),
        token_transfer_authority: auth,
        user: auth,
        payer: auth,
        quote_mint_to_referrer: None,
        jupiter_program_id: jup,
        missing_dynamic_accounts_as_default: false,
    }
}

#[test]
fn a_launch_token_quote_sends_its_six_and_the_ref_position() {
    let l = first_book();
    let mut hook = vec![0u8; 64];
    hook[32..].copy_from_slice(FLOOR_HOOK_ID.as_ref());
    let a = amm_with_quote(&l, mint_with(EXT_TRANSFER_HOOK, &hook), 0);
    assert!(a.quote_mint().hooked);
    assert_eq!(a.get_accounts_len(), SWAP_METAS_V2_QUOTE_HOOKED);
    assert_eq!(SWAP_METAS_V2_QUOTE_HOOKED, 32);
    let (jup, auth) = (Pubkey::new_unique(), Pubkey::new_unique());
    let launch = a.key();
    for is_buy in [true, false] {
        let m = a.get_swap_and_account_metas(&swap(&jup, is_buy, auth)).unwrap().account_metas;
        assert_eq!(m.len(), 32);
        assert_eq!(m[SWAP_METAS..SWAP_METAS_V2], trade_hook_metas(&BASE_MINT, is_buy, &auth)[..], "rem[5..=10]: the base's six");
        let (src, dst) = if is_buy { (auth, launch) } else { (launch, auth) };
        assert_eq!(m[SWAP_METAS_V2..SWAP_METAS_V2 + 6], hook_metas(&QUOTE_MINT, &src, &dst)[..], "rem[11..=16]: the quote's six");
        assert_eq!(m[SWAP_METAS_V2 + 1].pubkey, keys::launch_pda(&QUOTE_MINT), "the quote's launch");
        let tr = keys::trader_ref_pda(&auth, &QUOTE_MINT);
        assert_eq!(m[31].pubkey, holder_position_pda(&keys::launch_pda(&QUOTE_MINT), &tr));
        assert!(m[31].is_writable && !m[31].is_signer);
        assert_eq!(m[10].pubkey, tr, "the named trader_ref is the one rem[17] is the position of");
    }
    let mut other = hook.clone();
    other[32..].copy_from_slice(Pubkey::new_unique().as_ref());
    for d in [mint_with(EXT_TRANSFER_HOOK, &other), mint_data(0, 6)] {
        let a = amm_with_quote(&l, d, 0);
        assert!(!a.quote_mint().hooked);
        assert_eq!(a.get_swap_and_account_metas(&swap(&jup, true, auth)).unwrap().account_metas.len(), SWAP_METAS_V2);
    }
}

#[test]
fn update_reads_the_quote_mint_all_or_nothing() {
    let l = first_book();
    let key = keys::launch_pda(&l.base_mint);
    let acc = account(PROGRAM_ID, l.encode());
    let mut a = FloorAmmV2::from_keyed_account(&KeyedAccount { key, account: acc.clone(), params: None }, &AmmContext::default()).unwrap();
    assert!(a.update(common::Accounts([(key, acc.clone())].into_iter().collect())).is_err());
    assert!(!a.is_active());
    assert!(a.update(accounts([(key, acc.clone()), (QUOTE_MINT, account(TOKEN_2022_PROGRAM_ID, mint_data(0, 6)))])).is_err());
    assert!(!a.is_active());
    a.update(accounts([(key, acc)])).unwrap();
    assert!(a.is_active());
}
