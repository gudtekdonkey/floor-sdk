//! Parity for sell-backs against `floor_launch` (`tests/fixtures/floor.so`), on the devnet sale
//! snapshots `tests/parity.rs` replays (that test covers the buys through Jupiter's own kit).
//!
//! The kit cannot sell: its user is a fresh random wallet whose source account it funds by writing a
//! balance, and a sale sell-back needs the seller's position (`["buyer", launch, seller]`). So this test
//! follows the kit step for step (a fresh SVM seeded from the snapshot and the programs; a fresh user
//! with 100 SOL; `Amm::quote`; `get_swap_and_account_metas`; the kit's encoder; one transaction with a
//! 1.4M CU limit; the output delta against `quote.out_amount`) with one setup step before it: the user
//! BUYS through the adapter first (several buys of odd sizes, each also checked to the unit), so the
//! position is a real one carrying its own rounding history. Before every quote the adapter is updated
//! from the SVM's `Launch`, as a router updates a market. Each sell-back is checked to the unit on both
//! legs: the wSOL account's `amount` and lamports rise by exactly `quote.out_amount`, the source token
//! account falls by exactly `quote.in_amount`, the wallet's lamports fall by exactly the signature fee,
//! and the mint is paused again afterwards. A sell-back past the position is refused by the program.
//!
//! `37cg…` is a hooked launch (a holder rate), so its sell-backs carry the 4-account holder tail.

mod common;

use floor_types::ids::DEVNET;
use common::*;
use floor_sale_amm::accounts::{token_2022_ata, wsol_ata};
use floor_sale_amm::{instruction_data, FloorSaleAmm};
use jupiter_amm_interface::{AccountProvider, Amm, AmmContext, KeyedAccount, QuoteParams, Swap, SwapAndAccountMetas, SwapMode, SwapParams};
use litesvm::LiteSVM;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

const SPL_TOKEN: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM: Pubkey = Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const SYSTEM: Pubkey = Pubkey::from_str_const("11111111111111111111111111111111");
const SIG_FEE: u64 = 5_000;

/// The setup buys before the sell-backs: odd sizes, so the position carries rounding.
const SETUP_BUYS: &[u64] = &[777_777_777, 1_234_567, 3_333_333_331, 9_999];

fn encode(swap: &Swap, in_amount: u64) -> Vec<u8> {
    instruction_data(swap, in_amount).expect("encode")
}

struct Svm(LiteSVM);
#[allow(refining_impl_trait)]
impl AccountProvider for &Svm {
    fn get(&self, k: &Pubkey) -> Option<impl solana_account::ReadableAccount + use<'_>> {
        self.0.get_account(k)
    }
}

fn token_amount(svm: &LiteSVM, k: &Pubkey) -> u64 {
    svm.get_account(k).map(|a| u64::from_le_bytes(a.data[64..72].try_into().unwrap())).unwrap_or(0)
}

fn lamports(svm: &LiteSVM, k: &Pubkey) -> u64 {
    svm.get_account(k).map(|a| a.lamports).unwrap_or(0)
}

/// `PausableConfig.paused` off the mint's Token-2022 TLVs (type 26: a 32-byte authority, then the flag).
fn mint_paused(svm: &LiteSVM, mint: &Pubkey) -> bool {
    let d = svm.get_account(mint).unwrap().data;
    let mut i = 166;
    while i + 4 <= d.len() {
        let (ty, len) = (u16::from_le_bytes([d[i], d[i + 1]]), u16::from_le_bytes([d[i + 2], d[i + 3]]) as usize);
        if ty == 26 {
            return d[i + 4 + 32] == 1;
        }
        i += 4 + len;
    }
    panic!("no PausableConfig on {mint}");
}

fn send(svm: &mut LiteSVM, ix: Instruction, user: &Keypair) -> Result<(), String> {
    svm.expire_blockhash();
    let ixs = [ComputeBudgetInstruction::set_compute_unit_limit(1_400_000), ix];
    let tx = Transaction::new_signed_with_payer(&ixs, Some(&user.pubkey()), &[user], svm.latest_blockhash());
    svm.send_transaction(tx).map(|_| ()).map_err(|e| format!("{:?} {:?}", e.err, e.meta.logs.iter().rev().take(6).collect::<Vec<_>>()))
}

fn quote(amm: &FloorSaleAmm, input: Pubkey, output: Pubkey, amount: u64) -> Result<jupiter_amm_interface::Quote, String> {
    amm.quote(&QuoteParams { amount, input_mint: input, output_mint: output, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() })
        .map_err(|e| e.to_string())
}

fn swap_ix(amm: &FloorSaleAmm, user: &Pubkey, input: Pubkey, output: Pubkey, source: Pubkey, destination: Pubkey, amount: u64) -> Instruction {
    let SwapAndAccountMetas { swap, account_metas } = amm
        .get_swap_and_account_metas(&SwapParams {
            swap_mode: SwapMode::ExactIn,
            in_amount: amount,
            out_amount: amount,
            source_mint: input,
            destination_mint: output,
            source_token_account: source,
            destination_token_account: destination,
            token_transfer_authority: *user,
            user: *user,
            payer: *user,
            quote_mint_to_referrer: None,
            jupiter_program_id: &JUP,
            missing_dynamic_accounts_as_default: false,
        })
        .expect("get_swap_and_account_metas");
    Instruction { program_id: amm.program_id(), accounts: account_metas, data: encode(&swap, amount) }
}

/// The user's wSOL ATA under SPL Token, created through the ATA program, empty (the kit's
/// `create_funded_ata` with an amount of 0).
fn create_wsol_ata(svm: &mut LiteSVM, user: &Keypair) -> Pubkey {
    let ata = wsol_ata(&user.pubkey());
    let ix = Instruction {
        program_id: ATA_PROGRAM,
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(user.pubkey(), false),
            AccountMeta::new_readonly(wsol(), false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(SPL_TOKEN, false),
        ],
        data: vec![0],
    };
    send(svm, ix, user).expect("create the wSOL ATA");
    ata
}

/// One sale: setup buys, then partial sell-backs and the whole rest, each to the unit. Returns
/// `(buys, sells)` checked.
fn sells_for(launch: &Pubkey, mint: &Pubkey, floor_launch: &[u8], token_2022: &[u8], failures: &mut Vec<String>) -> (usize, usize) {
    let dir = fixtures_dir(launch);
    let accounts = load_accounts(&dir);
    let mut svm = Svm(LiteSVM::new());
    svm.0.add_program(launch_program_id(), floor_launch).unwrap();
    svm.0.add_program(pk("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"), token_2022).unwrap();
    for (k, a) in &accounts {
        if !a.executable {
            svm.0.set_account(*k, a.clone()).unwrap();
        }
    }
    let mut amm = FloorSaleAmm::from_keyed_account(&KeyedAccount { key: *launch, account: accounts[launch].clone(), params: None }, &AmmContext::default()).unwrap();
    let user = Keypair::new();
    let u = user.pubkey();
    svm.0.airdrop(&u, 100 * 1_000_000_000).unwrap();
    let source = token_2022_ata(&u, mint);
    let (mut buys, mut sells) = (0, 0);

    for &want in SETUP_BUYS {
        amm.update(&svm).unwrap();
        // a sale with little left: keep each setup buy under a quarter of what can still be bought
        let amount = want.min(amm.max_buy_in().unwrap_or(0) / 4);
        if amount == 0 {
            continue;
        }
        let q = quote(&amm, wsol(), *mint, amount).unwrap();
        let before = token_amount(&svm.0, &source);
        let ix = swap_ix(&amm, &u, wsol(), *mint, Pubkey::new_unique(), source, amount);
        match send(&mut svm.0, ix, &user) {
            Ok(()) if token_amount(&svm.0, &source) - before == q.out_amount => buys += 1,
            r => failures.push(format!("{launch} setup buy {amount}: {r:?}, delta {} vs quote {}", token_amount(&svm.0, &source) - before, q.out_amount)),
        }
    }
    let wsol_dst = create_wsol_ata(&mut svm.0, &user);
    let held = token_amount(&svm.0, &source);
    let mut plan: Vec<u64> = vec![1, 7, 1_000, 1_000_003, held / 7, held / 3];
    let mut sold = 0u64;
    plan.retain(|t| *t > 0);
    let n = plan.len();
    for (i, &t) in plan.iter().chain(std::iter::once(&0)).enumerate() {
        let tokens = if i == n { held - sold } else { t };
        amm.update(&svm).unwrap();
        let q = match quote(&amm, *mint, wsol(), tokens) {
            Ok(q) => q,
            Err(e) if e.starts_with("Nothing") => {
                eprintln!("  sell {tokens:>20} tokens: refused by the adapter ({e})");
                continue;
            }
            Err(e) => {
                failures.push(format!("{launch} sell {tokens}: quote {e}"));
                continue;
            }
        };
        let (src0, dst0, dst_lam0, wallet0) = (token_amount(&svm.0, &source), token_amount(&svm.0, &wsol_dst), lamports(&svm.0, &wsol_dst), lamports(&svm.0, &u));
        let ix = swap_ix(&amm, &u, *mint, wsol(), source, wsol_dst, tokens);
        let metas = ix.accounts.len();
        let r = send(&mut svm.0, ix, &user);
        let got = (
            src0 - token_amount(&svm.0, &source),
            token_amount(&svm.0, &wsol_dst) - dst0,
            lamports(&svm.0, &wsol_dst) - dst_lam0,
            wallet0 - lamports(&svm.0, &u),
        );
        let ok = r.is_ok() && got == (q.in_amount, q.out_amount, q.out_amount, SIG_FEE) && mint_paused(&svm.0, mint);
        eprintln!(
            "  {} sell {tokens:>20} tokens -> {:>14} lamports as wSOL (fee {}, {metas} accounts)",
            if ok { "ok  " } else { "FAIL" },
            q.out_amount,
            q.fee_amount
        );
        if !ok {
            eprintln!("    {r:?} got {got:?}");
        }
        if ok {
            sells += 1;
            sold += tokens;
        } else {
            failures.push(format!("{launch} sell {tokens}: {r:?}, (in, out, out lamports, wallet) {got:?} vs quote ({}, {}, {}, {SIG_FEE})", q.in_amount, q.out_amount, q.out_amount));
        }
    }
    // a sell larger than the position: quoted (the quote cannot see the position), refused on chain
    if token_amount(&svm.0, &source) == 0 {
        amm.update(&svm).unwrap();
        if let Ok(q) = quote(&amm, *mint, wsol(), 1_000_000) {
            let ix = swap_ix(&amm, &u, *mint, wsol(), source, wsol_dst, q.in_amount);
            if send(&mut svm.0, ix, &user).is_ok() {
                failures.push(format!("{launch}: a sell-back past the position landed"));
            }
        }
    }
    (buys, sells)
}

fn launch_program_id() -> Pubkey {
    floor_sale_amm::accounts::launch_program(&DEVNET)
}

#[test]
fn sell_backs_match_the_program() {
    let programs = match programs_ready() {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: {why}");
            return;
        }
    };
    let floor_launch = &programs[0].1;
    let token_2022 = &programs[1].1;
    let present: Vec<_> = SALES.iter().filter(|(l, _)| fixtures_dir(&pk(l)).exists()).collect();
    assert!(!present.is_empty(), "no sale snapshot; run `RPC=<devnet url> cargo test --test parity` once");
    let mut failures = Vec::new();
    let (mut setup_buys, mut sell_backs) = (0, 0);
    for (launch, mint) in &present {
        let (launch, mint) = (pk(launch), pk(mint));
        eprintln!("{launch} (mint {mint}): sell-backs");
        let (b, s) = sells_for(&launch, &mint, floor_launch, token_2022, &mut failures);
        setup_buys += b;
        sell_backs += s;
    }
    eprintln!("sell parity: {setup_buys} setup buys, {sell_backs} sell-backs, {} failed", failures.len());
    assert!(sell_backs > 0, "no sell-back ran");
    assert!(failures.is_empty(), "sell parity failures: {failures:#?}");
}
