# DefiLlama adapters

FLOOR's two DefiLlama adapters, in DefiLlama's own formats. Each file is copied as-is into a fork of DefiLlama's
repository, at the same path, and opened as a pull request there.

| File | Goes to | Measures |
|---|---|---|
| [`DefiLlama-Adapters/projects/floor-launchpad/index.js`](DefiLlama-Adapters/projects/floor-launchpad/index.js) | [DefiLlama/DefiLlama-Adapters](https://github.com/DefiLlama/DefiLlama-Adapters) | TVL: Ethereum, Base, BSC, Robinhood Chain, Arc, Solana |
| [`dimension-adapters/dexs/floor-launchpad/index.ts`](dimension-adapters/dexs/floor-launchpad/index.ts) | [DefiLlama/dimension-adapters](https://github.com/DefiLlama/dimension-adapters) | Volume: Ethereum, Base, BSC, Robinhood Chain, Arc |
| [`PR-tvl.md`](PR-tvl.md), [`PR-volume.md`](PR-volume.md) | the two pull request bodies | |

## What they count

Only launches quoted in the chain's native asset or a quote asset FLOOR lists are counted. Each chain's list is inlined
in the adapter (`quotes`, and `SOLANA_QUOTES` for Solana), because an adapter cannot import anything outside
DefiLlama's repository.

**TVL, EVM.** Launches come from both FLOOR registries (`count()` / `page()`): the current deployment set and the one
before it. While a launch sells, its sale contract's `raised()`. Once it graduates, the pool's `backingEth()`, which is
the vault plus the deployed bid in raw quote units, read through `hook.execute(poolId, backingEth())` with the pool id
from `hook.poolForAccount(launch)`. Robinhood Chain's native ETH is priced as its WETH and Arc's native USDC (18
decimals at the EVM level) as USDC. Marked `doublecounted`: the bid is a Uniswap v4 position.

**TVL, Solana.** Launches come from `floor_launch`'s registry (`LaunchConfig.registry_count`, then each
`["entry", u64le(i)]` account). While a launch sells, the SOL in its `["sale_vault", launch]` account and its quote vault.
Once it graduates, the FloorPool account's `vault_eth + bid_eth_deployed`. Account offsets include the 8-byte
discriminator.

**Volume, EVM.** Sale buys (`Bought.ethSpent` plus the buy's `TeamFeeCharged.fee`), sale sell-backs
(`SoldBack.ethReturned`), and the quote side of every Uniswap v4 `Swap` on a FLOOR pool. Swaps the hook sends itself
during a ratchet are left out. One side of each swap only.

Launch tokens and the platform's 2.5% cut are counted in neither.

## Testing

Run each adapter on DefiLlama's own harness from a checkout of its repository:

```sh
# DefiLlama-Adapters
node test.js projects/floor-launchpad/index.js

# dimension-adapters
pnpm test dexs floor-launchpad
```

Robinhood Chain and Arc may need `ROBINHOOD_RPC` / `ARC_RPC`. Public Robinhood Chain nodes do not keep historical state,
so test volume on a recent hour (`pnpm test dexs floor-launchpad <ISO hour>`).

## Keeping them current

- A new chain goes into both adapters' chain maps, with its approved quotes (the volume adapter also needs the chain's
  Uniswap v4 PoolManager and `start`, the date of its first FLOOR block).
- A change to the approved quote assets is copied into every list.
- Solana volume is not covered yet.
