# FLOOR — Rust crates

FLOOR is a launchpad mechanism. On Solana it runs as an approved hook on the Carpenter AMM
(`floor_hook`, with `floor_launch`, `floor_holders` and `floor_vesting` beside it). These crates are
its off-chain half: the mechanism's account layouts, its math, and the FLOOR hook's quoter, which
plugs into Carpenter's quoter through `carpenter_quote::HookQuoter`.

| Crate | What it answers | Normal dependencies |
|---|---|---|
| `floor-types` | FLOOR's account layouts (`FloorPool`, `Launch`, `LaunchConfig`, `HookConfig`, the holder-reward and payee accounts, the pool-price TWAP tail), seeds, constants, discriminators, program ids — over `carpenter-types`, re-exported under the same paths | `carpenter-types`, `borsh`, `bytemuck` |
| `floor-math` | the skim ramp, fee booking (`take_fees`, `intake`), the bid band, positions, the floor solve, the staircase, modes and presets, the sale, holder rewards, referrals, the net-flow meter | `carpenter-clmm-math`, `bytemuck` |
| `floor-quote` | `FloorHookQuoter` (the `HookQuoter` for `floor_hook`), `quote_swap` for FLOOR pools with FLOOR's fee breakdown and the in-swap bid re-place, `sale_buy` / `sale_sell_back`, `holder_claimable`, `referral_tier_dues`, FLOOR's decoders and tick snaps | `carpenter-quote`, `floor-types`, `floor-math`, `bytemuck` |

No crate depends on a Solana crate. The Carpenter crates come from
[`carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk), as git dependencies pinned to its
release tag (`Cargo.toml`, `[workspace.dependencies]`).

## Quoting a FLOOR pool

```rust
use floor_quote::state::{decode_cushion_top, decode_floor_pool, decode_pool, TickArraySet};
use floor_quote::{no_limit, quote_swap, PoolAccount, SwapQuoteParams};

let pool = decode_pool(&pool_bytes)?;
let floor_pool = decode_floor_pool(&floor_pool_bytes)?;
let set = TickArraySet::from_accounts(&pool_key, pool.tick_spacing, &tick_arrays, Some(&bitmap))?;
let q = quote_swap(&PoolAccount { key: pool_key, pool }, &set, Some(&floor_pool), &SwapQuoteParams {
    zero_for_one: true,                 // a buy: the quote (token0) in
    amount_specified: -1_000_000_000,   // exact-in; FLOOR pools refuse exact-out
    sqrt_price_limit_x64: no_limit(true),
    supply0, supply1,                   // the two mints' supplies
    cushion_top: decode_cushion_top(&floor_pool_bytes),
})?;
// q.amount_in, q.amount_out, q.fees { lp, skim, creator, holder, platform_cut, team, protocol, .. },
// q.replace (the bid re-place on a sell), q.all_arrays(), q.pool_after, q.floor_after
```

The trader's take on a FLOOR pool is listed whole: LP fee, floor skim, creator fee, holder tax, FLOOR
Platform fee, and the Carpenter Platform fee (`QuoteFees`; `skim + creator + holder + team` is the
hook's delta, the platform cut is inside the skim).

### Composing with other hooks

`floor_quote::quote_swap` is `carpenter_quote::quote_swap` with a `FloorHookQuoter`. A caller that
routes over several hooks registers the quoters itself:

```rust
use floor_quote::amm::{quote_swap_with, HookQuoter};
// `new` accepts `floor_hook` on every cluster in `floor_types::ids::CLUSTERS`; `for_cluster` one cluster's only.
let mut floor_q = floor_quote::FloorHookQuoter::new(Some(floor_pool), cushion_top);
let q = quote_swap_with(&pool, &set, &mut [&mut floor_q as &mut dyn HookQuoter /*, other hooks */], &params.amm())?;
// FLOOR's breakdown: floor_q.taken, floor_q.replace, floor_q.state (FloorPool after)
```

## Golden vectors

`fixtures/` (in this folder; the tests reach it as `../../fixtures/…`):

| File | What the expectation is | Test |
|---|---|---|
| `quote-vectors/floor.json` | 454 FLOOR-pool swaps over 37 cases, recorded by an earlier single-crate build of the quoter, every field of the answer; replayed through `carpenter-quote` + `floor-quote` | `floor-quote/tests/floor_vectors.rs` |
| `netflow/netflow-vectors.json` | the net-flow meter's transcript, shared with the EVM twin and the TypeScript SDK | `floor-math/tests/netflow_vectors.rs` |
| `twap/twap-vectors.json` | the pool-price TWAP oracle's transcript, shared with the TypeScript SDK | `floor-types/tests/twap_sdk_vectors.rs` |

`floor.json` is the Rust quoter's word: no chain run has measured it.

## Building

```bash
cargo test --workspace
cargo tree --workspace -e normal --depth 1
```

## Status

The program ids in `floor-types` are per cluster (`floor_types::ids::CLUSTERS`: mainnet-beta and
devnet; the `FLOOR_*_ID` constants are mainnet-beta's) and a cluster is added as one more entry. The
programs are upgradeable through a timelock on mainnet-beta (a 10-minute delay that can only be raised, proposed by the deployer key) and by the deployer key directly on devnet, and pausable by the deployer key. No security audit has been completed.

## Licence

Apache-2.0 (`../LICENSE`).
