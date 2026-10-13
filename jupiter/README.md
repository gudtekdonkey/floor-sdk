# floor-sale-amm

An implementation of [`jupiter-amm-interface`](https://github.com/jup-ag/jupiter-amm-interface)'s
`Amm` trait for FLOOR launch sales on Solana. A FLOOR launch sells a fixed share of its supply at one
fixed price through the `floor_launch` program; the buy that sells the last token out creates the
launch's Carpenter pool inside the same instruction. This crate is the sale venue (label `"FLOOR"`).
The pool is a separate venue, covered by `carpenter-amm` (label `"Carpenter"`) in
[`carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk).

It quotes a sale from its `Launch` account and lists the accounts of `floor_launch`'s `buy`
instruction. Every amount comes from `floor-quote`'s `sale_buy` (in `../rust/crates`), which is the
program's arithmetic.

It also quotes sell-backs (launch token → wSOL) and lists the accounts of `floor_launch`'s
`sell_back_wsol` (see "Sell-backs" below). ⚠ A `floor_launch` build without the stateless sell-back
and `sell_back_wsol` (an older deployment) cannot execute a sell-back quote; check the program a
cluster runs before routing sells.

This crate is its own Cargo workspace with its own `Cargo.lock`. It is the only crate in this
repository that links Solana crates.

## Status

- `get_swap_and_account_metas` returns `Swap::Placeholder { data: [0] }` for a buy and `[1]` for a
  sell-back. The interface
  defines that variant for venues that have no variant of their own; Jupiter's on-chain router does
  not execute it. A dedicated variant, and router support for it, are needed before a sale can be part
  of a Jupiter route (see "What routing needs" and `upstream/`). Until then this crate is usable for
  quoting, for building the native `buy` instruction (`buy_instruction_data`), and for Jupiter's test
  kit.
- The clusters the adapter routes are the entries of `floor_types::ids::CLUSTERS` (mainnet-beta and
  devnet); a cluster is added as one more entry. The programs are upgradeable through a timelock on mainnet-beta (a 10-minute delay that can only be raised, proposed by the deployer key) and by the deployer key directly on devnet, and pausable by the deployer key.
- No security review of the programs or of this crate has been completed.
- The interface is a git dependency pinned to `jup-ag/jupiter-amm-interface` @
  `95bd18485e580625a39eb92a0c0ad2061579daad`.

## What is supported

| | |
|---|---|
| Buys (wSOL → launch token), exact-in, on a sale quoted in wSOL | yes |
| A buy that sells the sale out (it creates the pool) | no: refused with `SaleWouldGraduate` |
| Sell-backs (launch token → wSOL), exact-in, paid as wSOL | yes, against a program carrying `sell_back_wsol` |
| Sales quoted in a token other than wSOL | no: inactive, `TokenQuoteNotRouted` |
| Exact-out | no: `ExactOutNotSupported` |
| The launch token as an intermediate of a route, or a fee on its output | no (the mint is paused): a buy is a route's last hop, a sell-back its first |
| A program authority signing, a buy's destination other than the user's ATA, a sell-back's source other than the user's ATA or destination other than the user's wSOL ATA | no: `AuthorityNotUser`, `DestinationNotUserAta`, `SourceNotUserAta`, `DestinationNotUserWsolAta` |
| A sell-back of more tokens than the sale has sold | no: `ExceedsSold` |

## How a sale buy moves value

These are properties of the program, each measured in `tests/custody.rs`:

- **The input is native SOL.** `buy` takes `cost + fee` from the signing wallet with a System
  transfer into the sale's escrow (`["sale_vault", launch]`). A wSOL token account is not read or
  debited. A wallet whose SOL is held as wSOL can pay once that account is unwrapped earlier in the
  same transaction.
- **The output goes to the signer's own Token-2022 associated token account.** The program derives
  the account from the signer and refuses any other (`NotAssociatedTokenAccount`, 6021). It creates
  the account if it does not exist, and creates the buyer's position account
  (`["buyer", launch, buyer]`) on a wallet's first buy; the wallet pays both rents.
- **The token cannot move until the sale ends.** The mint carries Token-2022's `Pausable` extension
  and is paused for the whole sale. Inside `buy` the program resumes it, transfers, and pauses it
  again. Any later transfer of the token fails (`MintPaused`), including in the same transaction.
- **One fixed price.** `tokens = floor(net · 2^64 / price_x64)` capped at what remains, where
  `net = floor(lamports_in · 10_000 / (10_000 + team_bps))`; `cost = ceil(tokens · price_x64 / 2^64)`;
  `fee = floor(cost · team_bps / 10_000)`. The program takes `cost + fee ≤ lamports_in`.
- **The buy that sells out creates the pool.** It needs the pool-creation accounts as remaining
  accounts (and an address lookup table to fit), which this adapter does not carry; without them it
  fails (`AccountNotEnoughKeys`). The adapter refuses any amount that would buy the last token, so the
  largest quotable buy leaves at least one unit for a direct transaction.

## What each method does

| Method | Behaviour |
|---|---|
| `from_keyed_account` | Requires an account owned by `floor_launch` of a listed cluster, holding a `Launch`, at the `["launch", mint]` address of its own mint under that program. The owner selects the cluster every program id and PDA comes from (`from_keyed_account_in` takes the table as an argument). Never reads the context's clock. |
| `label` | `"FLOOR"` (the test kit's program fixture is then `tests/fixtures/floor.so`). |
| `program_id`, `key`, `get_reserve_mints` | `floor_launch`, the `Launch` address, `[quote mint, launch mint]`. |
| `get_accounts_to_update`, `update` | The `Launch` account alone. A failed update (missing account, other owner, other mint) leaves the market inactive until one succeeds. |
| `quote` | Exact-in. A buy: `in_amount` is `cost + fee` (what leaves the wallet, at most the requested amount), `out_amount` the tokens delivered. A sell-back: `in_amount` is every token offered, `out_amount` the wSOL paid (`payout − fee`), from `floor-quote`'s `sale_sell_back_stateless`. |
| `get_swap_and_account_metas` | A buy: the 13 `buy` accounts for `swap_params.user`, refused unless `token_transfer_authority` is the user and `destination_token_account` is the user's Token-2022 ATA for the mint. A sell-back: the `sell_back_wsol` accounts (13, or 17 on a hooked launch), refused unless the authority is the user, `source_token_account` is the user's Token-2022 ATA and `destination_token_account` the user's SPL Token wSOL ATA. |
| `unidirectional` | `false`: both directions. |
| `supports_exact_out`, `has_dynamic_accounts`, `requires_update_for_reserve_mints` | `false`. |
| `program_dependencies` | Token-2022 as `spl_token_2022` (the deployed version carries `Pausable`). |
| `get_accounts_len` | 13, or 17 on a hooked launch (a holder rate on either side). |
| `is_active` | The account decoded, the sale is bonding, it is quoted in wSOL, and at least two units remain. |

`FloorSaleAmm::max_buy_in()` returns the largest input that does not sell the sale out.

### The fee triple

A buy: `fee_mint` is the input mint, `fee_amount` the sale's platform fee
(`floor(cost · team_bps / 10_000)`, read from the `Launch`), `fee_pct = fee_amount / in_amount`. A
sell-back: `fee_mint` is the output mint (wSOL), `fee_amount = floor(payout · team_bps / 10_000)`,
`fee_pct = fee_amount / payout`. No other take applies to a sale trade.

## Sell-backs

**The rule is stateless.** `floor_launch` pays a sell-back of `tokens` `floor(tokens · price_x64 / 2^64)`
— the buy's cost formula floored where the buy ceils, at the sale's one fixed price — less
`floor(payout · team_bps / 10_000)`. Both numbers are fixed for the life of the sale, so the quote is
exact to the unit for every seller, and the instruction either pays it or fails. The seller's position
must still cover `tokens` (`InsufficientAllocation`), and `contributed` falls by the payout. Every
position satisfies `contributed · 2^64 ≥ allocation · price_x64` (a buy adds a ceiled cost, a
sell-back removes a floored one), so the payout never exceeds the position's `contributed` and the
sale's escrow always covers every remaining sell-back. The payout is never above the position's
pro-rata share `floor(contributed · tokens / allocation)`, and below it by at most the number of buys
and sell-backs the position has made (`tests/sell_bound.rs`, random sequences: at most 15 lamports
over 28,798 sell-backs; against the program, `tests/custody.rs`: at most 3 lamports). What a
position's rounding left in it stays in the sale and goes to the pool at graduation.

**`sell_back_wsol(tokens)`** pays that amount as wSOL into the seller's SPL Token wSOL account
(credited, then `SyncNative`), so a route can continue from it. Accounts: `sell_back`'s 11 named
accounts, the seller's wSOL account (writable) and SPL Token; a hooked launch adds the 4-account holder
tail (the holder-rewards program, `["holder_book", mint]`, `Launch.floor_pool`, `["holder", mint,
seller_ata]`). Instruction data: `sha256("global:sell_back_wsol")[..8] ‖ tokens u64 LE`. Program
errors: the wSOL account is not the seller's initialized native SPL Token account (`BadWsolDestination`,
6099), the launch is not quoted in wSOL (`WsolPayoutNotNative`, 6100). The plain `sell_back` pays the
same amount in native SOL into the wallet.

The adapter refuses, by name: more tokens than the sale has sold (`ExceedsSold`: no position can cover
them), an amount that pays nothing (`Nothing`), a source that is not the user's Token-2022 ATA
(`SourceNotUserAta`) and a destination that is not the user's wSOL ATA (`DestinationNotUserWsolAta`).
A seller whose position does not cover the amount is refused by the program (`InsufficientAllocation`),
never mispaid.

## What routing needs

A Jupiter route through a sale needs, on the router's side:

1. a swap variant (`upstream/swap-variant.patch` proposes `Swap::FloorSale { is_buy }`, see
   `upstream/PR.md`);
2. a buy: the input unwrapped from wSOL to native SOL in the user's wallet before the swap, the user as
   the signer, the destination the user's Token-2022 associated token account, the sale the last hop,
   with no fee taken on its output;
3. a sell-back: the user as the signer, the source the user's Token-2022 associated token account (so
   the sale is the first hop), the destination the user's wSOL associated token account, which holds
   synced wSOL afterwards for the next hop. No unwrap or wrap is needed around it.

`upstream/check.sh` builds and tests this crate with the variant against a patched interface checkout
(cloned at the pinned commit when no path is given), without editing `Cargo.toml` or `Cargo.lock`.

## Building and testing

```bash
cd jupiter
cargo test
```

| Test file | What it checks |
|---|---|
| `tests/conformance.rs` | each trait method on synthetic launches against `floor_quote::sale_buy` called directly; every refusal by name; `max_buy_in`; `update`'s failure handling; the encoder; no math crate in the normal dependencies and no clock, network or file access in the source |
| `tests/vectors.rs` | 2,400 golden vectors in `tests/fixtures/sale-vectors.json` (prices, fee rates, sold levels, amounts), generated from `floor-math` and replayed through `Amm::quote` |
| `tests/idl.rs` | the `buy` and `sell_back_wsol` accounts, flags, PDAs, discriminators and arguments, and the sell-back's error codes, against `../packages/sdk-solana/idl/floor_launch.json` |
| `tests/parity.rs` | Jupiter's `jupiter-amm-test-kit`: devnet sale snapshots, `Amm::quote`, and the program's own `buy` in LiteSVM; the output delta must equal `quote.out_amount` |
| `tests/sell_parity.rs` | the kit's procedure for sell-backs on the same snapshots (a fresh user buys first, so it holds a position with its own rounding; each sell-back's wSOL delta, token debit and fee to the unit) |
| `tests/custody.rs` | the value flows above, in LiteSVM with the program; `sell_back` and `sell_back_wsol` pay the stateless amount, and a wrong wSOL destination is refused with 6099 |
| `tests/sell_bound.rs` | the stateless payout against the pro-rata share, over random buy / sell-back sequences |
| `tests/devnet_simulate.rs` | opt-in: a buy simulated on devnet against the live sales (below) |
| `tests/clusters.rs` | a second, synthetic cluster: the launch's owner selects the cluster, its program ids and PDAs alone reach a buy or sell-back, an unknown owner and another cluster's launch PDA are refused |

### Parity with the program (`tests/parity.rs`, `tests/sell_parity.rs`)

The kit loads `tests/fixtures/accounts/<launch>/<pubkey>.bin`, builds the adapter from them, quotes,
then sends the native `buy` built from `get_swap_and_account_metas` and `buy_instruction_data` to
LiteSVM with the program binaries below, from a fresh user holding 100 SOL, and asserts the
destination token delta equals `quote.out_amount` to the unit.

```bash
cargo test --test parity -- --nocapture                         # offline replay (default)
RPC=https://api.devnet.solana.com cargo test --test parity      # snapshot missing launches / Token-2022
REFRESH=1 RPC=https://api.devnet.solana.com cargo test --test parity
```

Snapshots are taken by the test's own reader (the launch, its escrow, its token vault, the mint, the
wSOL mint, and the Clock, Rent, LastRestartSlot and StakeHistory sysvars), not the kit's: the kit
discovers accounts by calling `get_swap_and_account_metas` with one placeholder key in every role,
which this adapter refuses. The Rent sysvar matters: the escrow is funded at the cluster's rent-exempt
minimum, and under LiteSVM's default rent a small first buy would leave it below that minimum.
`floor.so` is a build of the `floor_launch` program source carrying the stateless sell-back and
`sell_back_wsol`; it is not dumped from a cluster (a cluster may run an older build). Token-2022 is
dumped from its ProgramData account (payload after the 45-byte header) and refused unless the sha256
matches. Nothing is
sent to the cluster. The kit's RPC client must reach the endpoint directly; behind a TLS-intercepting
proxy, point `RPC` at a local plain-HTTP forwarder.

| File | Program | Bytes | sha256 |
|---|---|---:|---|
| `floor.so` | `floor_launch` `93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG` (a build of the program source) | 1,059,840 | `8307d1a2f2d45378f3116e89dc581607fd9119d89d7da940084f5c9b52e99090` |
| `spl_token_2022.so` | Token-2022 `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` (whole ProgramData payload) | 711,008 | `b781b71d230a9910de4d3c103e72608f597fc73c83ee207242ac82b708382a6a` |

Sales (all quoted in wSOL, still selling when snapshotted; `GQYc…` is a hooked launch with buys before
the snapshot, `4bCB…` a Cook-preset launch with none):

| Launch | Mint | Cases |
|---|---|---|
| `GQYcgETr9DSPb6bd2zibHViviE8bPTa5chZgDDGhQiii` | `5swr…siB1D` | 1,000 lamports to 1 SOL (5), and the largest routable buy (1.215 SOL) |
| `4bCBM8p8K63t6HKKjAtjEM1ov4KpuW9M5NsJxfq3XetN` | `9S4Y…rB1D` | 1,000 lamports to 1 SOL (5), and the largest routable buy (2.525 SOL) |
| `ykpUGi82DfmivZiHv2t8CDFHntJjignsMLCQFSGzzjM` | `9gij…tB1D` | 1,000 lamports to 1 SOL (5), and the largest routable buy (2.714 SOL) |
| `iqYuAz2r1m8MsMZbJM7VwG1Zkn5FApm3goYa1Y3jrhd` | `2P7s…eB1D` | 1,000 lamports to 10 SOL (6), and the largest routable buy (45.06 SOL) |

25 buys through the kit and, in `tests/sell_parity.rs`, 16 setup buys and 16 sell-backs, each equal
to the quote to the unit.

### Devnet simulation (`tests/devnet_simulate.rs`)

```bash
RPC=https://api.devnet.solana.com SIMULATE_PAYER=<funded throwaway address> cargo test --test devnet_simulate -- --nocapture
```

Reads each sale's live `Launch`, quotes 0.01 SOL, builds the transaction from the adapter and asks
the cluster to `simulateTransaction` it (`sigVerify: false`; nothing is signed or sent), returning the
payer's token account; the simulated delivery must equal the quote. The payer must hold devnet SOL.
Skipped when either variable is unset.

The dependencies on `floor-quote`, `floor-types` and `floor-math` are path dependencies on this
repository's crates; through them, the Carpenter crates come from `carpenter-sdk` at the commit `rust/Cargo.toml` pins.

## Licence

Apache-2.0.
