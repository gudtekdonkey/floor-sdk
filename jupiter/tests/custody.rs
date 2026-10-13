//! How a sale buy and sell-back move value, measured against `floor_launch` in LiteSVM, on the
//! devnet sale snapshots `tests/parity.rs` replays (run that test with `RPC` once to create them).
//!
//! These are the facts a router integration depends on:
//!
//! 1. the input is native SOL debited from the signing wallet; a wSOL token account is not touched,
//!    and a wallet whose SOL sits in a wSOL account cannot pay until that account is unwrapped
//!    earlier in the same transaction;
//! 2. the tokens land in the signer's own Token-2022 ATA and nowhere else, and the mint is paused
//!    again before the instruction returns, so no later instruction can move them (the token cannot
//!    be the intermediate of a route);
//! 3. a buy that sells the sale out fails without the graduation accounts, which is why the adapter
//!    refuses such an amount;
//! 4. `sell_back` pays native SOL into the wallet: exactly the stateless amount
//!    `floor(tokens · price / 2^64)` less the fee, whatever the seller's rounding history, never above
//!    the position's pro-rata share; the position's `contributed` falls by the gross payout.
//! 5. the adapter quotes a sell at exactly that stateless amount; `sell_back_wsol` pays it as wSOL to
//!    the unit (`tests/sell_parity.rs`).

mod common;

use floor_types::ids::DEVNET;
use std::collections::HashMap;

use common::*;
use floor_sale_amm::accounts::{buyer_position_pda, lock_authority_pda, event_authority_pda, sale_vault_pda, token_2022, token_2022_ata, launch_program};
use floor_sale_amm::amm::refusal;
use floor_sale_amm::{buy_instruction_data, FloorSaleAmm, BUY_METAS};
use floor_types::disc;
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, QuoteParams, SwapMode, SwapParams};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

const SPL_TOKEN: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ATA_PROGRAM: Pubkey = Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const SYSTEM: Pubkey = Pubkey::from_str_const("11111111111111111111111111111111");
const SOL: u64 = 1_000_000_000;
const DECIMALS: u8 = 6;

struct World {
    svm: LiteSVM,
    amm: FloorSaleAmm,
    launch: Pubkey,
    mint: Pubkey,
}

/// A fresh SVM with the programs and one sale's snapshot, or `None` (reported) when the
/// fixtures are absent.
fn world(index: usize) -> Option<World> {
    let programs = match programs_ready() {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: {why}");
            return None;
        }
    };
    let (launch, mint) = (pk(SALES[index].0), pk(SALES[index].1));
    let dir = fixtures_dir(&launch);
    if !dir.exists() {
        eprintln!("SKIPPED: no snapshot at {dir:?}; run `RPC=<devnet url> cargo test --test parity` first");
        return None;
    }
    let accounts = load_accounts(&dir);
    let mut svm = LiteSVM::new();
    for (id, bytes) in &programs {
        svm.add_program(*id, bytes).unwrap();
    }
    for (k, a) in &accounts {
        svm.set_account(*k, a.clone()).unwrap();
    }
    let amm = FloorSaleAmm::from_keyed_account(&KeyedAccount { key: launch, account: accounts[&launch].clone(), params: None }, &AmmContext::default()).unwrap();
    Some(World { svm, amm, launch, mint })
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, extra: &[&Keypair]) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    svm.expire_blockhash();
    let mut all: Vec<Instruction> = vec![ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)];
    all.extend_from_slice(ixs);
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra);
    let tx = Transaction::new_signed_with_payer(&all, Some(&payer.pubkey()), &signers, svm.latest_blockhash());
    svm.send_transaction(tx)
}

fn lamports(svm: &LiteSVM, k: &Pubkey) -> u64 {
    svm.get_account(k).map(|a| a.lamports).unwrap_or(0)
}

fn token_amount(svm: &LiteSVM, k: &Pubkey) -> u64 {
    svm.get_account(k).map(|a| u64::from_le_bytes(a.data[64..72].try_into().unwrap())).unwrap_or(0)
}

/// `owner`'s ATA for `mint` under `program`, created through the ATA program (idempotent).
fn create_ata(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, mint: &Pubkey, program: &Pubkey) -> Pubkey {
    let ata = Pubkey::find_program_address(&[owner.as_ref(), program.as_ref(), mint.as_ref()], &ATA_PROGRAM).0;
    let ix = Instruction {
        program_id: ATA_PROGRAM,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(SYSTEM, false),
            AccountMeta::new_readonly(*program, false),
        ],
        data: vec![1],
    };
    send(svm, &[ix], payer, &[]).expect("create ATA");
    ata
}

/// A wSOL ATA for `owner` holding `amount` (backed by lamports, as a wrapped account is).
fn funded_wsol(svm: &mut LiteSVM, owner: &Keypair, amount: u64) -> Pubkey {
    let ata = create_ata(svm, owner, &owner.pubkey(), &wsol(), &SPL_TOKEN);
    let mut a = svm.get_account(&ata).unwrap();
    a.data[64..72].copy_from_slice(&amount.to_le_bytes());
    a.lamports += amount;
    svm.set_account(ata, a).unwrap();
    ata
}

fn params<'a>(w: &World, user: &Pubkey, source: Pubkey, destination: Pubkey, authority: Pubkey, amount: u64, jup: &'a Pubkey) -> SwapParams<'a, 'a> {
    SwapParams {
        swap_mode: SwapMode::ExactIn,
        in_amount: amount,
        out_amount: 0,
        source_mint: wsol(),
        destination_mint: w.mint,
        source_token_account: source,
        destination_token_account: destination,
        token_transfer_authority: authority,
        user: *user,
        payer: *user,
        quote_mint_to_referrer: None,
        jupiter_program_id: jup,
        missing_dynamic_accounts_as_default: false,
    }
}

/// The adapter's `buy` instruction for `user`, the destination being the user's ATA.
fn buy_ix(w: &World, user: &Pubkey, amount: u64, min_tokens: u64) -> Instruction {
    let p = params(w, user, Pubkey::new_unique(), token_2022_ata(user, &w.mint), *user, amount, &JUP);
    let sam = w.amm.get_swap_and_account_metas(&p).unwrap();
    assert_eq!(sam.account_metas.len(), BUY_METAS);
    Instruction { program_id: w.amm.program_id(), accounts: sam.account_metas, data: buy_instruction_data(&sam.swap, amount, min_tokens).unwrap() }
}

fn quote(w: &World, amount: u64) -> Result<jupiter_amm_interface::Quote, jupiter_amm_interface::AmmError> {
    w.amm.quote(&QuoteParams { amount, input_mint: wsol(), output_mint: w.mint, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() })
}

fn logs_of(e: &litesvm::types::FailedTransactionMetadata) -> String {
    format!("{:?} {}", e.err, e.meta.logs.join("\n"))
}

/// The Token-2022 `TransferChecked` of `amount` from `from` (signed by `authority`) to `to`.
fn transfer_checked(from: Pubkey, mint: Pubkey, to: Pubkey, authority: Pubkey, amount: u64) -> Instruction {
    let mut data = vec![12u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(DECIMALS);
    Instruction {
        program_id: token_2022(),
        accounts: vec![AccountMeta::new(from, false), AccountMeta::new_readonly(mint, false), AccountMeta::new(to, false), AccountMeta::new_readonly(authority, true)],
        data,
    }
}

/// `sell_back(tokens)` for `seller` on a wSOL launch (no remaining accounts on an unhooked mint).
fn sell_back_ix(svm: &LiteSVM, w: &World, seller: &Pubkey, tokens: u64) -> Instruction {
    let mut data = disc::launch::SELL_BACK.to_vec();
    data.extend_from_slice(&tokens.to_le_bytes());
    let l = w.amm.launch();
    let mut ix = Instruction {
        program_id: launch_program(&DEVNET),
        accounts: vec![
            AccountMeta::new(*seller, true),
            AccountMeta::new(w.launch, false),
            AccountMeta::new(sale_vault_pda(&DEVNET, &w.launch), false),
            AccountMeta::new(buyer_position_pda(&DEVNET, &w.launch, seller), false),
            AccountMeta::new(token_2022_ata(seller, &w.mint), false),
            AccountMeta::new(Pubkey::new_from_array(l.token_vault), false),
            AccountMeta::new(w.mint, false),
            AccountMeta::new_readonly(lock_authority_pda(&DEVNET), false),
            AccountMeta::new_readonly(token_2022(), false),
            AccountMeta::new_readonly(event_authority_pda(&DEVNET), false),
            AccountMeta::new_readonly(launch_program(&DEVNET), false),
        ],
        data,
    };
    if hooked(svm, &w.mint) {
        // A mint carrying the (still empty) transfer-hook slot takes the holder tail:
        // [floor_holders, holder_book (w), floor_pool, holder_record (w)].
        let holders = Pubkey::new_from_array(floor_types::ids::DEVNET.floor_holders);
        let ata = token_2022_ata(seller, &w.mint);
        ix.accounts.extend([
            AccountMeta::new_readonly(holders, false),
            AccountMeta::new(Pubkey::find_program_address(&[floor_types::seeds::HOLDER_BOOK, w.mint.as_ref()], &holders).0, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(l.floor_pool), false),
            AccountMeta::new(Pubkey::find_program_address(&[floor_types::seeds::HOLDER_RECORD, w.mint.as_ref(), ata.as_ref()], &holders).0, false),
        ]);
    }
    ix
}

/// `sell_back_wsol(tokens)` for `seller`, paid into `destination`: `sell_back`'s accounts with the wSOL
/// pair `[destination (w), spl_token]` before any holder tail.
fn sell_back_wsol_ix(svm: &LiteSVM, w: &World, seller: &Pubkey, destination: &Pubkey, tokens: u64) -> Instruction {
    let mut ix = sell_back_ix(svm, w, seller, tokens);
    ix.data[..8].copy_from_slice(&disc::launch::SELL_BACK_WSOL);
    ix.accounts.splice(11..11, [AccountMeta::new(*destination, false), AccountMeta::new_readonly(SPL_TOKEN, false)]);
    ix
}

/// Does the Token-2022 mint carry the TransferHook extension (type 14)? The TLV area starts after the
/// 165-byte base (82-byte mint, zero padding) and the one account-type byte.
fn hooked(svm: &LiteSVM, mint: &Pubkey) -> bool {
    let d = svm.get_account(mint).unwrap().data;
    let mut i = 166;
    while i + 4 <= d.len() {
        let ty = u16::from_le_bytes([d[i], d[i + 1]]);
        let len = u16::from_le_bytes([d[i + 2], d[i + 3]]) as usize;
        if ty == 14 {
            return true;
        }
        i += 4 + len;
    }
    false
}

/// The `BuyerPosition` of `buyer`: `(allocation, contributed)`.
fn position(svm: &LiteSVM, launch: &Pubkey, buyer: &Pubkey) -> (u64, u64) {
    let a: Account = svm.get_account(&buyer_position_pda(&DEVNET, launch, buyer)).unwrap();
    (u64::from_le_bytes(a.data[16..24].try_into().unwrap()), u64::from_le_bytes(a.data[24..32].try_into().unwrap()))
}

// ---- 1. the input is native SOL ------------------------------------------------------------------

#[test]
fn a_buy_debits_native_sol_from_the_signer_and_never_the_source_token_account() {
    let Some(mut w) = world(0) else { return };
    let user = Keypair::new();
    w.svm.airdrop(&user.pubkey(), 10 * SOL).unwrap();
    let amount = SOL / 10;
    let source = funded_wsol(&mut w.svm, &user, amount);
    let q = quote(&w, amount).unwrap();
    let vault = sale_vault_pda(&DEVNET, &w.launch);
    let (wallet0, vault0) = (lamports(&w.svm, &user.pubkey()), lamports(&w.svm, &vault));

    let ix = buy_ix(&w, &user.pubkey(), amount, q.out_amount);
    send(&mut w.svm, &[ix], &user, &[]).expect("buy");

    let ata = token_2022_ata(&user.pubkey(), &w.mint);
    let pos = buyer_position_pda(&DEVNET, &w.launch, &user.pubkey());
    assert_eq!(token_amount(&w.svm, &ata), q.out_amount, "the tokens are in the signer's Token-2022 ATA");
    assert_eq!(token_amount(&w.svm, &source), amount, "the wSOL source account is untouched");
    assert_eq!(lamports(&w.svm, &vault) - vault0, q.in_amount, "the escrow receives exactly what the quote says is spent");
    let rent = lamports(&w.svm, &pos) + lamports(&w.svm, &ata);
    let paid = wallet0 - lamports(&w.svm, &user.pubkey());
    // 5,000: the one signature's fee (no priority fee is set).
    assert_eq!(paid, q.in_amount + rent + 5_000, "the wallet pays the input, the two accounts' rent and the fee in native SOL");
    eprintln!("buy {amount}: spent {} native lamports + rent {rent} (position + ATA); wSOL account unchanged", q.in_amount);
}

#[test]
fn sol_held_as_wsol_pays_the_buy_only_once_unwrapped_in_the_same_transaction() {
    let Some(mut w) = world(0) else { return };
    let user = Keypair::new();
    let amount = SOL;
    // Enough native SOL for the fee and the two accounts' rent, not for the input; the input is in wSOL.
    w.svm.airdrop(&user.pubkey(), SOL / 100).unwrap();
    let source = funded_wsol(&mut w.svm, &user, amount);
    let buy = buy_ix(&w, &user.pubkey(), amount, 1);

    let e = send(&mut w.svm, &[buy.clone()], &user, &[]).expect_err("the input is not in native SOL");
    eprintln!("without an unwrap: {:?}", e.err);
    assert!(logs_of(&e).contains("insufficient lamports"), "{}", logs_of(&e));

    // SPL Token CloseAccount(source -> the wallet): the wrapped SOL becomes native first.
    let unwrap = Instruction {
        program_id: SPL_TOKEN,
        accounts: vec![AccountMeta::new(source, false), AccountMeta::new(user.pubkey(), false), AccountMeta::new_readonly(user.pubkey(), true)],
        data: vec![9],
    };
    send(&mut w.svm, &[unwrap, buy], &user, &[]).expect("unwrap, then buy");
    assert_eq!(token_amount(&w.svm, &token_2022_ata(&user.pubkey(), &w.mint)), quote(&w, amount).unwrap().out_amount);
}

// ---- 2. the token cannot move on ------------------------------------------------------------------

#[test]
fn the_bought_tokens_cannot_be_moved_on_while_the_sale_runs() {
    let Some(mut w) = world(0) else { return };
    let user = Keypair::new();
    w.svm.airdrop(&user.pubkey(), 10 * SOL).unwrap();
    let other = Keypair::new();
    let next_hop = create_ata(&mut w.svm, &user, &other.pubkey(), &w.mint, &token_2022());
    let ata = token_2022_ata(&user.pubkey(), &w.mint);
    let q = quote(&w, SOL / 10).unwrap();

    // A route in which the token is an intermediate: buy, then hand the tokens to the next venue.
    let ixs = [buy_ix(&w, &user.pubkey(), SOL / 10, 1), transfer_checked(ata, w.mint, next_hop, user.pubkey(), q.out_amount)];
    let e = send(&mut w.svm, &ixs, &user, &[])
        .expect_err("the second leg moves a paused token");
    let logs = logs_of(&e);
    eprintln!("buy + onward transfer: {:?}", e.err);
    assert!(logs.contains("paused"), "{logs}");
    // Token-2022's MintPaused (67), raised by the second instruction.
    assert!(logs.starts_with("InstructionError(2, Custom(67))"), "{logs}");
    assert_eq!(token_amount(&w.svm, &ata), 0, "the whole transaction reverted");

    // The buy alone lands; the tokens then stay where they are.
    let ix = buy_ix(&w, &user.pubkey(), SOL / 10, 1);
    send(&mut w.svm, &[ix], &user, &[]).expect("buy alone");
    assert_eq!(token_amount(&w.svm, &ata), q.out_amount);
    let e = send(&mut w.svm, &[transfer_checked(ata, w.mint, next_hop, user.pubkey(), 1)], &user, &[]).expect_err("paused");
    assert!(logs_of(&e).contains("paused"));
}

#[test]
fn the_program_delivers_only_into_the_signers_own_ata() {
    let Some(mut w) = world(0) else { return };
    let user = Keypair::new();
    w.svm.airdrop(&user.pubkey(), 10 * SOL).unwrap();
    let router = Keypair::new(); // stands in for a program authority holding a route's intermediate accounts
    let routers_ata = create_ata(&mut w.svm, &user, &router.pubkey(), &w.mint, &token_2022());

    // The adapter refuses both shapes up front.
    let user_ata = token_2022_ata(&user.pubkey(), &w.mint);
    for (dest, auth, name) in [(routers_ata, user.pubkey(), refusal::DESTINATION_NOT_USER_ATA), (user_ata, router.pubkey(), refusal::AUTHORITY_NOT_USER)] {
        let e = w.amm.get_swap_and_account_metas(&params(&w, &user.pubkey(), Pubkey::new_unique(), dest, auth, SOL, &JUP)).err().unwrap();
        assert!(e.to_string().starts_with(name), "{e}");
    }

    // The program refuses a buy whose destination is not the signer's ATA (NotAssociatedTokenAccount, 6021).
    let mut ix = buy_ix(&w, &user.pubkey(), SOL / 10, 1);
    ix.accounts[5].pubkey = routers_ata;
    let e = send(&mut w.svm, &[ix], &user, &[]).expect_err("destination is not the signer's ATA");
    assert!(logs_of(&e).contains("custom program error: 0x1785"), "{}", logs_of(&e)); // 6021
    assert_eq!(token_amount(&w.svm, &routers_ata), 0);
}

// ---- 3. the graduating buy ------------------------------------------------------------------------

#[test]
fn the_largest_routable_buy_lands_and_one_lamport_more_is_refused_and_fails() {
    let Some(mut w) = world(0) else { return };
    let max = w.amm.max_buy_in().expect("a routable sale");
    let user = Keypair::new();
    w.svm.airdrop(&user.pubkey(), max + 10 * SOL).unwrap();

    // max + 1 sells the sale out: the adapter refuses it, and the program fails it without the
    // graduation accounts.
    let over = max + 1;
    let e = quote(&w, over).err().expect("refused");
    assert!(e.to_string().starts_with(refusal::SALE_WOULD_GRADUATE), "{e}");
    let ix = {
        let p = params(&w, &user.pubkey(), Pubkey::new_unique(), token_2022_ata(&user.pubkey(), &w.mint), user.pubkey(), over, &JUP);
        let sam = w.amm.get_swap_and_account_metas(&p).unwrap();
        Instruction { program_id: w.amm.program_id(), accounts: sam.account_metas, data: buy_instruction_data(&sam.swap, over, 1).unwrap() }
    };
    let e = send(&mut w.svm, &[ix], &user, &[]).expect_err("a selling-out buy without the graduation accounts");
    eprintln!("max + 1 = {over}: {:?}", e.err);
    // Anchor's AccountNotEnoughKeys (3005): the graduation accounts are absent.
    assert!(logs_of(&e).starts_with("InstructionError(1, Custom(3005))"), "{}", logs_of(&e));

    let q = quote(&w, max).unwrap();
    let ix = buy_ix(&w, &user.pubkey(), max, q.out_amount);
    send(&mut w.svm, &[ix], &user, &[]).expect("the largest routable buy");
    assert_eq!(token_amount(&w.svm, &token_2022_ata(&user.pubkey(), &w.mint)), q.out_amount);
    let l = floor_quote::state::decode_launch(&w.svm.get_account(&w.launch).unwrap().data).unwrap();
    assert_eq!(l.state, floor_types::constants::LAUNCH_STATE_BONDING);
    assert!(l.sale_tokens - l.sold >= 1, "at least one unit is left for the graduating buy");
    eprintln!("largest routable buy {max}: {} tokens, {} left unsold", q.out_amount, l.sale_tokens - l.sold);
}

// ---- 4. the sell-back -----------------------------------------------------------------------------

/// The stateless sell-back amount: `floor(tokens · price / 2^64)` gross, less the fee floored on it.
fn stateless_received(price_x64: u128, team_bps: u16, tokens: u64) -> u64 {
    let gross = ((tokens as u128 * price_x64) >> 64) as u64;
    gross - (gross as u128 * team_bps as u128 / 10_000) as u64
}

#[test]
fn a_sell_back_pays_native_sol_and_exactly_the_stateless_amount() {
    for index in 0..SALES.len() {
        let Some(mut w) = world(index) else { return };
        let (price, bps) = (w.amm.launch().price_x64.get(), w.amm.launch().team_bps);
        let user = Keypair::new();
        w.svm.airdrop(&user.pubkey(), 20 * SOL).unwrap();
        // Several buys of awkward sizes, each rounding its cost up on its own.
        let buys = [1_234_567u64, 98_765_431, 7_777_777, 333_333_333, 1_000_003];
        for (i, b) in buys.iter().enumerate() {
            let ix = buy_ix(&w, &user.pubkey(), *b, 1);
            send(&mut w.svm, &[ix], &user, &[]).unwrap_or_else(|e| panic!("buy {i}: {}", logs_of(&e)));
        }
        let ata = token_2022_ata(&user.pubkey(), &w.mint);
        let wsol_before = 0u64;
        let mut held = token_amount(&w.svm, &ata);
        let mut worst = 0u64;
        for (sells, part) in [held / 7, held / 3, held / 2 + 1, 0].into_iter().enumerate() {
            let tokens = if part == 0 { held } else { part };
            let (alloc, contributed) = position(&w.svm, &w.launch, &user.pubkey());
            let pro_rata = {
                let payout = (contributed as u128 * tokens as u128 / alloc as u128) as u64;
                payout - (payout as u128 * bps as u128 / 10_000) as u64
            };
            let exact = stateless_received(price, bps, tokens);
            let gross = ((tokens as u128 * price) >> 64) as u64;
            let before = lamports(&w.svm, &user.pubkey());
            let ix = sell_back_ix(&w.svm, &w, &user.pubkey(), tokens);
            send(&mut w.svm, &[ix], &user, &[]).unwrap_or_else(|e| panic!("sell {tokens}: {}", logs_of(&e)));
            let got = lamports(&w.svm, &user.pubkey()) + 5_000 - before;
            assert_eq!(got, exact, "the program pays the stateless amount");
            assert!(got <= pro_rata, "{got} > the position's pro-rata share {pro_rata}");
            assert!(pro_rata - got <= (buys.len() + sells) as u64, "{pro_rata} - {got} exceeds the number of prior operations");
            assert_eq!(position(&w.svm, &w.launch, &user.pubkey()), (alloc - tokens, contributed - gross), "the position falls by the tokens and the gross payout");
            worst = worst.max(pro_rata - got);
            held -= tokens;
            eprintln!("sale {index}: sell {tokens:>16} tokens -> {got:>11} lamports native; pro-rata would pay {pro_rata:>11}");
            if held == 0 {
                break;
            }
        }
        assert_eq!(wsol_before, 0);
        eprintln!("sale {index}: the stateless amount is under the pro-rata share by at most {worst} lamport(s)");
    }
}

#[test]
fn a_sell_quote_is_the_stateless_amount() {
    for index in 0..SALES.len() {
        let Some(w) = world(index) else { return };
        let (price, bps) = (w.amm.launch().price_x64.get(), w.amm.launch().team_bps);
        let tokens = w.amm.launch().sold.min(1_000_000_000);
        if tokens == 0 {
            eprintln!("sale {index}: nothing sold in the snapshot; only the refusal is checked");
        } else {
            let q = w
            .amm
            .quote(&QuoteParams { amount: tokens, input_mint: w.mint, output_mint: wsol(), swap_mode: SwapMode::ExactIn, fee_mode: Default::default() })
            .unwrap();
            assert_eq!(q.out_amount, stateless_received(price, bps, tokens), "sale {index}: the sell quote is the stateless amount");
            eprintln!("sale {index}: a sell of {tokens} tokens quotes {}", q.out_amount);
        }
        assert!(!w.amm.unidirectional());
        assert_eq!(w.amm.get_reserve_mints(), vec![wsol(), w.mint]);
        // More than the whole sale has sold cannot be covered by any position.
        let e = w
            .amm
            .quote(&QuoteParams { amount: w.amm.launch().sold + 1, input_mint: w.mint, output_mint: wsol(), swap_mode: SwapMode::ExactIn, fee_mode: Default::default() })
            .err()
            .unwrap();
        assert!(e.to_string().starts_with(refusal::EXCEEDS_SOLD));
    }
    let _ = HashMap::<u8, u8>::new();
}

#[test]
fn sell_back_wsol_pays_the_stateless_amount_as_wsol_and_refuses_any_other_destination() {
    for index in 0..SALES.len() {
        let Some(mut w) = world(index) else { return };
        let (price, bps) = (w.amm.launch().price_x64.get(), w.amm.launch().team_bps);
        let user = Keypair::new();
        let other = Keypair::new();
        w.svm.airdrop(&user.pubkey(), 20 * SOL).unwrap();
        w.svm.airdrop(&other.pubkey(), SOL).unwrap();
        let ix = buy_ix(&w, &user.pubkey(), 123_456_789, 1);
        send(&mut w.svm, &[ix], &user, &[]).unwrap_or_else(|e| panic!("buy: {}", logs_of(&e)));
        let ata = token_2022_ata(&user.pubkey(), &w.mint);
        let held = token_amount(&w.svm, &ata);
        let tokens = held / 3;
        // BadWsolDestination (6099): another wallet's wSOL account, and the seller's own launch-token account
        let foreign = create_ata(&mut w.svm, &other, &other.pubkey(), &wsol(), &SPL_TOKEN);
        for bad in [foreign, ata] {
            let ix = sell_back_wsol_ix(&w.svm, &w, &user.pubkey(), &bad, tokens);
            let e = send(&mut w.svm, &[ix], &user, &[]).err().expect("refused");
            assert!(format!("{:?}", e.err).contains("Custom(6099)"), "sale {index}: {:?} {}", e.err, logs_of(&e));
        }
        // the seller's own wSOL account: paid exactly the stateless amount, synced
        let mine = create_ata(&mut w.svm, &user, &user.pubkey(), &wsol(), &SPL_TOKEN);
        let (amount0, lamports0) = (token_amount(&w.svm, &mine), lamports(&w.svm, &mine));
        let ix = sell_back_wsol_ix(&w.svm, &w, &user.pubkey(), &mine, tokens);
        send(&mut w.svm, &[ix], &user, &[]).unwrap_or_else(|e| panic!("sale {index}: sell_back_wsol: {}", logs_of(&e)));
        let want = stateless_received(price, bps, tokens);
        assert_eq!(token_amount(&w.svm, &mine) - amount0, want, "sale {index}: the wSOL amount");
        assert_eq!(lamports(&w.svm, &mine) - lamports0, want, "sale {index}: the wSOL lamports");
        assert_eq!(token_amount(&w.svm, &ata), held - tokens, "sale {index}: the tokens debited");
        eprintln!("sale {index}: sell_back_wsol {tokens} tokens -> {want} lamports as wSOL; 6099 for two wrong destinations");
    }
}
