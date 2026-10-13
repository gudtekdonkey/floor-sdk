# Add `Swap::FloorSale { is_buy }`

Base: `jup-ag/jupiter-amm-interface` @ `95bd18485e580625a39eb92a0c0ad2061579daad`. Patch: `swap-variant.patch`
(one file, `interface/src/swap.rs`, additive).

## Change

One variant with a direction flag, appended after `Placeholder`; no existing variant moves.

```rust
FloorSale { is_buy: bool },
```

If the router's own enum orders variants differently, the variant can be placed wherever that order
requires.

## The program

`floor_launch` sells a fixed share of a new token's supply at one fixed price before the token's pool
exists (the pool is a Carpenter pool, a separate venue). The buy that sells the last token out creates
the pool inside the same instruction, and the sale is over from then on.

`buy(lamports_in: u64, min_tokens: u64, max_graduation_lamports: u64)`: `lamports_in` is the input
amount (a gross cap: the sale price and a platform fee are taken out of it, never more; what is not
used, under one token's price, stays with the wallet), `min_tokens` the minimum out,
`max_graduation_lamports` 0.

Accounts, in order (13):

| # | Account | |
|---|---|---|
| 0 | `buyer` | the user; signer, writable |
| 1 | `launch` | `["launch", mint]`, writable |
| 2 | `sale_vault` | `["sale_vault", launch]`, writable |
| 3 | `token_vault` | the launch's token account, writable |
| 4 | `buyer_position` | `["buyer", launch, buyer]`, writable (created on a wallet's first buy) |
| 5 | `buyer_ata` | the user's Token-2022 associated token account for the mint, writable (created if absent) |
| 6 | `mint` | writable |
| 7 | `lock_authority` | `["lock_authority"]` |
| 8–10 | the associated token program, Token-2022, the system program | |
| 11–12 | `event_authority` (`["__event_authority"]`), the program | |

## The sell side

⚠ A cluster running an older `floor_launch` build has no `sell_back_wsol`; check the program before
routing sells.

`sell_back_wsol(tokens: u64)`: sells `tokens` back to the sale. It pays
`floor(tokens · price_x64 / 2^64)` (the sale's one fixed price, the buy's cost formula floored) less
`floor(payout · team_bps / 10_000)` (the sale's platform fee, fixed for its life) — a function of the
input alone, so the quote is exact for any seller — into the signer's SPL Token wSOL account, credited
and then synced with `SyncNative`. The seller's position (`["buyer", launch, seller]`) must cover
`tokens`; it is a record of the seller's own buys, so a wallet holding the token holds one (the token
cannot be transferred during the sale).

Accounts, in order (13; 17 on a hooked launch):

| # | Account | |
|---|---|---|
| 0 | `buyer` | the user; signer, writable |
| 1 | `launch` | `["launch", mint]`, writable |
| 2 | `sale_vault` | `["sale_vault", launch]`, writable (the native escrow the payout leaves) |
| 3 | `buyer_position` | `["buyer", launch, buyer]`, writable |
| 4 | `buyer_ata` | the user's Token-2022 associated token account for the mint, writable (the source) |
| 5 | `token_vault` | the launch's token account, writable |
| 6 | `mint` | writable |
| 7 | `lock_authority` | `["lock_authority"]` |
| 8 | Token-2022 | |
| 9–10 | `event_authority` (`["__event_authority"]`), the program | |
| 11 | `seller_wsol` | the user's SPL Token wSOL account (an initialized native account the user owns), writable |
| 12 | SPL Token | |
| 13–16 | a hooked launch only: the holder-rewards program, `["holder_book", mint]` (w), `Launch.floor_pool`, `["holder", mint, buyer_ata]` (w) | |

## What the router has to do

- **Unwrap the input.** The program takes `lamports_in` as native SOL from `buyer` with a System
  transfer, not from a token account. The router transfers `in_amount` out of the route's wSOL into
  the user's wallet as native SOL immediately before the swap, as it does for the other bonding
  curves whose escrow is native SOL.
- **The user signs.** `buyer` is the user's wallet. The program records the purchase in a position
  account keyed by the signer, and later sell-backs before the sale ends are available to that signer
  only; a router authority signing instead would own the position.
- **Buy: deliver to the user, last hop only.** The tokens are delivered into the signer's own
  Token-2022 associated token account and nowhere else, and the mint is paused (Token-2022
  `Pausable`) except inside the program's own transfer. No later instruction can move the tokens until
  the sale ends, so a buy must be the last hop of a route, its destination the user's associated token
  account, and no fee can be taken on its output.
- **Sell: take from the user, first hop only.** The tokens can leave only the signer's own
  associated token account, through the program, so a sell is the first hop of a route and its source
  is the user's associated token account. Its output is ordinary wSOL in the user's wSOL account,
  synced inside the instruction, from which the route continues. No unwrap or wrap step is needed
  around it.

## Properties relevant to routing

- Both directions: `unidirectional() == false`, `get_reserve_mints() == [wSOL, launch token]`.
- Exact-in only; `has_dynamic_accounts() == false`; `get_accounts_len()` 13 (17 on a hooked launch).
- A sell's output is exact: no price impact and no minimum-out argument; the instruction either pays
  the quoted amount or fails (the sale ended, or the position does not cover the input).
- One fixed price per sale: no price impact until the sale's remaining supply is reached.
- The adapter refuses an amount that would sell the sale out: that buy needs the pool-creation
  accounts and is left to a direct transaction.
- The swap is a plain top-level instruction; no instruction-sysvar introspection, one signer.

## Verification

The SDK side (the `floor-sale-amm` adapter crate) builds against this patch behind a cargo feature,
emits the variant from `get_swap_and_account_metas`, and checks that:

- the variant encodes to the same instruction data as the `Placeholder` encoding it replaces, for
  both sides;
- `jupiter-amm-test-kit` parity passes with the variant: snapshots of four devnet sales, the
  `floor_launch` binary in LiteSVM, the output delta equal to `quote.out_amount` for buys from 1,000
  lamports to the largest amount that leaves the sale open;
- in LiteSVM, a buy debits native SOL and leaves a wSOL account untouched, succeeds after an unwrap in
  the same transaction, fails as a whole when followed by any transfer of the bought token, and is
  refused by the program when the destination is not the signer's associated token account;
- on the same four snapshots (`tests/sell_parity.rs`): every sell-back's wSOL delta equals
  `quote.out_amount` and its token debit `quote.in_amount` to the unit, after buys of odd sizes that
  give the position its own rounding, on unhooked and hooked launches; a wSOL destination that is not
  the seller's native account is refused (`BadWsolDestination`, 6099).
