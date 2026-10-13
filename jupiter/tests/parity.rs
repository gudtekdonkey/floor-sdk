//! Parity with `floor_launch` for buys, through Jupiter's `jupiter-amm-test-kit`.
//!
//! The kit loads a launch snapshot (`tests/fixtures/accounts/<launch>/<pubkey>.bin`), runs
//! `Amm::quote`, executes the program's own `buy` in LiteSVM with the programs in
//! `tests/fixtures/<name>.so` (`floor.so` is `floor_launch`, after the label), and
//! asserts that the destination token delta equals `quote.out_amount` to the unit. The kit's user
//! starts with 100 SOL and a fresh, empty destination ATA.
//!
//! Running:
//!
//! - offline (the default): every launch whose snapshot directory exists is replayed. Both `.so`
//!   files must be present and hash to `common::PROGRAMS`; a missing snapshot or binary is reported
//!   and skipped.
//! - snapshot / refresh: `RPC=<devnet url> cargo test --test parity` writes any missing snapshot and
//!   a missing Token-2022 `.so` (`floor.so` is a build of the program source, never dumped); add `REFRESH=1` to re-snapshot existing launches. Nothing is sent to the
//!   cluster. The snapshots are taken with this crate's own reader (`common::ensure_snapshot`), not
//!   the kit's: the kit discovers accounts by calling `get_swap_and_account_metas` with one
//!   placeholder key for every role, which this adapter refuses (the destination must be the user's
//!   ATA).

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use common::*;
use floor_sale_amm::{buy_instruction_data, FloorSaleAmm};
use jupiter_amm_interface::{Amm, AmmContext, KeyedAccount, QuoteParams, Swap, SwapMode};
use jupiter_amm_test_kit::{assert_pool_parity, PoolSnapshot, PoolTest};

/// The kit's encoder: the native `buy` data from the adapter's `Swap` and the input amount, with no
/// minimum out (the kit asserts the amount itself).
fn encode(swap: &Swap, in_amount: u64) -> Vec<u8> {
    buy_instruction_data(swap, in_amount, 0).expect("encode buy")
}

/// Inputs in lamports: dust that buys a handful of tokens, then 0.001 to 50 SOL (the kit's user holds
/// 100 SOL, and pays the position's and the ATA's rent besides the input).
const SIZES: &[u64] = &[1_000, 1_000_000, 10_000_000, 100_000_000, 1_000_000_000, 10_000_000_000, 50_000_000_000];

fn amm_from(dir: &std::path::Path, launch: &solana_pubkey::Pubkey) -> FloorSaleAmm {
    let snap = PoolSnapshot::load_dir(dir).unwrap();
    let account = snap.get(launch).expect("launch in snapshot").clone();
    FloorSaleAmm::from_keyed_account(&KeyedAccount { key: *launch, account, params: None }, &AmmContext::default()).unwrap()
}

#[test]
fn buys_match_the_program() {
    let programs = match programs_ready() {
        Ok(p) => p,
        Err(why) => {
            eprintln!("SKIPPED: {why}");
            return;
        }
    };
    drop(programs);
    // Take (or refresh) every snapshot first, then hide `REFRESH` from the kit, which would otherwise
    // re-snapshot with its own reader (see the module documentation).
    let dirs: Vec<_> = SALES.iter().map(|(l, _)| ensure_snapshot(&pk(l))).collect();
    std::env::remove_var("REFRESH");
    let mut ran = 0;
    let mut failures = Vec::new();
    for ((launch, mint), dir) in SALES.iter().zip(dirs) {
        let (launch, mint) = (pk(launch), pk(mint));
        let dir = match dir {
            Ok(d) => d,
            Err(why) => {
                eprintln!("SKIPPED {launch}: {why}");
                continue;
            }
        };
        let amm = amm_from(&dir, &launch);
        let l = amm.launch();
        eprintln!(
            "{launch}: mint {mint}, sold {} / {}, price_x64 {}, team_bps {}, remaining {}, max routable in {:?}",
            l.sold,
            l.sale_tokens,
            l.price_x64.get(),
            l.team_bps,
            amm.remaining(),
            amm.max_buy_in()
        );
        assert!(amm.is_active(), "{launch} is a bonding wSOL sale");
        let mut sizes: Vec<u64> = SIZES.to_vec();
        if let Some(max) = amm.max_buy_in() {
            sizes.retain(|s| *s <= max);
            if max <= 90_000_000_000 {
                sizes.push(max); // the largest buy that leaves one token unsold
            }
        }
        for amount in sizes {
            let q = amm
                .quote(&QuoteParams { amount, input_mint: wsol(), output_mint: mint, swap_mode: SwapMode::ExactIn, fee_mode: Default::default() })
                .unwrap();
            let test = PoolTest { fixtures_dir: dir.clone(), ..PoolTest::new(launch) }.add_swap(wsol(), mint, amount);
            let r = catch_unwind(AssertUnwindSafe(|| assert_pool_parity::<FloorSaleAmm>(&test, encode)));
            ran += 1;
            let ok = r.is_ok();
            eprintln!("  {} buy {amount:>14} lamports -> {:>20} tokens (spent {}, fee {})", if ok { "ok  " } else { "FAIL" }, q.out_amount, q.in_amount, q.fee_amount);
            if !ok {
                failures.push(format!("{launch} {amount}"));
            }
        }
    }
    eprintln!("parity: {ran} buys, {} failed", failures.len());
    assert!(failures.is_empty(), "parity failures: {failures:?}");
}
