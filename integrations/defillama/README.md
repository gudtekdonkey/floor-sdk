# DefiLlama adapters

FLOOR's two DefiLlama adapters, in DefiLlama's own formats. Each file is copied as-is into a fork of DefiLlama's
repository, at the same path, and opened as a pull request there.

| File | Goes to | Measures |
|---|---|---|
| [`DefiLlama-Adapters/projects/floor-launchpad/index.js`](DefiLlama-Adapters/projects/floor-launchpad/index.js) | [DefiLlama/DefiLlama-Adapters](https://github.com/DefiLlama/DefiLlama-Adapters) | TVL: Ethereum, Base, BSC, Robinhood Chain, Arc, Solana |
| [`dimension-adapters/dexs/floor-launchpad/index.ts`](dimension-adapters/dexs/floor-launchpad/index.ts) | [DefiLlama/dimension-adapters](https://github.com/DefiLlama/dimension-adapters) | Volume, fees and revenue: Ethereum, Base, BSC, Robinhood Chain, Arc |
| [`dimension-adapters/dexs/floor-launchpad-solana/index.ts`](dimension-adapters/dexs/floor-launchpad-solana/index.ts) | [DefiLlama/dimension-adapters](https://github.com/DefiLlama/dimension-adapters) | Volume, fees and revenue: Solana |
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

**Volume, Solana.** A separate adapter, because it reads Dune, and a Dune Solana adapter must stay single-chain (DefiLlama
drops every chain's record when one throws, and Dune's Solana data lands hours late). One query over
`solana.instruction_calls` sums the `emit_cpi!` events: `floor_launch`'s `Bought` / `TeamBought` (buys) and `SoldBack` /
`TeamBuySoldBack` (sell-backs) per launch, and `carpenter_amm`'s `Swapped` per pool, by direction. Launches, their pools
and quote mints come from `floor_launch`'s registry; a pool swap counts its quote side. Marked `doublecounted`: the pools
are Carpenter pools.

**Fees and revenue.** The same two adapters report every fee a trader pays, split by who it goes to:

| Part | EVM source | Solana source | Goes to |
|---|---|---|---|
| FLOOR Platform fee | `TeamFeeCharged` and the team buy's `fee` on the sale; on the pool, `HookFee` less the logged parts | `Swapped` fee split `team`; `TeamBought` / `TeamBuySoldBack` `fee` | protocol revenue |
| Skim: FLOOR's cut (2.5%) | the `PlatformCut` logged with each `Skimmed` | fee split `platform` | protocol revenue |
| Skim: the rest | `Skimmed` less the cut | fee split `skim` less `platform` | holders' revenue (the pool's backing) |
| Holder fee | `HolderTax` | fee split `holder` | holders' revenue |
| Creator fee | `CreatorFee`, and its referral share `ReferralAccrued` | fee split `creator` | supply side |
| LP fee | `Swap.fee` of the input | `lp_fee_total` | supply side |

On a sell the LP fee is charged on the token and stated in the quote at the swap's own rate. On Solana the sale's other buys
and sell-backs log no fee of their own, so only the team buy's sale fee is counted there. Checked on pools created on a fork
of Base: on every trade the derived FLOOR Platform fee is exactly the pool's rate of the quote leg, and the paired cut
exactly 2.5% of the skim.

Launch tokens are not counted in volume, fees or TVL.

## Testing

Run each adapter on DefiLlama's own harness from a checkout of its repository:

```sh
# DefiLlama-Adapters
node test.js projects/floor-launchpad/index.js

# dimension-adapters
pnpm test dexs floor-launchpad
pnpm test dexs floor-launchpad-solana   # needs DUNE_API_KEYS
```

Robinhood Chain and Arc may need `ROBINHOOD_RPC` / `ARC_RPC`. Public Robinhood Chain nodes do not keep historical state,
so test volume on a recent hour (`pnpm test dexs floor-launchpad <ISO hour>`).

## Keeping them current

- A new chain goes into both adapters' chain maps, with its approved quotes (the volume adapter also needs the chain's
  Uniswap v4 PoolManager and `start`, the date of its first FLOOR block).
- A change to the approved quote assets is copied into every list.
