# DefiLlama adapters

FLOOR's two DefiLlama adapters, in DefiLlama's own formats. Each file is copied as-is into a fork of DefiLlama's
repository, at the same path, and opened as a pull request there.

| File | Goes to | Measures |
|---|---|---|
| [`DefiLlama-Adapters/projects/floor-launchpad/index.js`](DefiLlama-Adapters/projects/floor-launchpad/index.js) | [DefiLlama/DefiLlama-Adapters](https://github.com/DefiLlama/DefiLlama-Adapters) | TVL: Ethereum, Base, BSC, Robinhood Chain, Arc, Solana |
| [`dimension-adapters/dexs/floor-launchpad/index.ts`](dimension-adapters/dexs/floor-launchpad/index.ts) | [DefiLlama/dimension-adapters](https://github.com/DefiLlama/dimension-adapters) | Volume, fees and revenue: Ethereum, Base, BSC, Robinhood Chain, Arc |
| [`dimension-adapters/dexs/floor-launchpad-solana/index.ts`](dimension-adapters/dexs/floor-launchpad-solana/index.ts) | [DefiLlama/dimension-adapters](https://github.com/DefiLlama/dimension-adapters) | Volume, fees and revenue: Solana |
| [`DefiLlama-Adapters/projects/treasury/floor-launchpad.js`](DefiLlama-Adapters/projects/treasury/floor-launchpad.js) | DefiLlama-Adapters | Treasury: the platform and team treasuries, EVM and Solana |
| [`PR-tvl.md`](PR-tvl.md), [`PR-volume.md`](PR-volume.md) | the two pull request bodies | |

## What they count

Only launches quoted in the chain's native asset or a quote asset FLOOR lists are counted. Each chain's list is inlined
in the adapter (`quotes`, and `SOLANA_QUOTES` for Solana), because an adapter cannot import anything outside
DefiLlama's repository.

**TVL, EVM.** Launches come from FLOOR's registry (`count()` / `page()`). While a launch sells, its sale contract's
quote balance. Once it graduates (pool id from `hook.poolForAccount(launch)`), three places, none counted twice:

- the quote in the pool's liquidity at the current price, summed over every initialized range from Uniswap's StateView
  (the bid, the protocol-owned liquidity, the cushion and any step the price is in);
- the pool's custody vault (`poolState` field 41): the rest of the backing and every fee owed but not yet claimed;
- the hook's ERC-6909 claims of the quote on the PoolManager (`balanceOf(hook, uint256(uint160(quote)))`): an in-swap
  bid re-place on a sell mints its net quote credit to the hook as claims (`liftClaims`), which stay part of the vault
  until the next ratchet (or `closeExhausted`) burns them and takes the quote to custody. The PoolManager keeps one
  balance per owner and currency, so the chain's one hook holds every pool's claims of a quote together: each
  (hook, quote) is read once.

Not counted: LP fees the hook's positions have earned but not yet swept (they sit in the PoolManager outside the
positions' liquidity until the next ratchet or fee sweep). The pool's liquidity is summed over every initialized tick,
so a position added by anyone but the hook would be counted too: the hook does not gate adding liquidity (none is
known). Separating one needs each hook position's range and salt (seed 3, bid 1, tail 201, re-list 202, cushion 203,
step `300 + i`) read through StateView's `getPositionInfo`, or the pool's `ModifyLiquidity` history from its creation.

Robinhood Chain's native ETH is priced as its WETH and Arc's native USDC (18 decimals at the EVM level) as USDC. Marked
`doublecounted`: the bid is a Uniswap v4 position.

**TVL, Solana.** Launches come from `floor_launch`'s registry (`LaunchConfig.registry_count`, then each
`["entry", u64le(i)]` account). While a launch sells, the SOL in its `["sale_vault", launch]` account and its quote vault.
Once it graduates, the Carpenter pool's quote vault (every position's quote, the hook's claims and every unswept LP fee),
less Carpenter's own protocol fees owed in it. A position added to the pool by anyone but the hook would be counted too.
Account offsets include the 8-byte discriminator.

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
| LP fee on a buy (paid in the quote) | `Swap.fee` pips of the quote paid in, less FLOOR's 2.5% (the cut is the `PlatformCut` logged at the sweep) | `lp_fee_total` less 2.5% (FLOOR's cut, booked as protocol revenue) | holders' revenue (the pool's backing) |
| LP fee on a sell (paid in the launch token) | `Swap.fee` pips of the token paid in, stated in the quote | `lp_fee_total`, stated in the quote | holders' revenue (the pool vault's inventory, not backing) |

The LP fee is charged in the swap's input currency and earned by the pool's positions, which are the hook's own. On a buy
it is quote, which the hook sweeps into the pool's vault through FLOOR's cut. On a sell it is the launch token, which joins
the pool vault's inventory at zero cost and leaves the circulating supply: no quote enters the backing and no cut is taken.
It is stated in the quote at the swap's own rate (`quote out × fee / (1e6 − fee)`), since the launch token has no price of
its own on DefiLlama. The hook does not gate adding liquidity: a position added by anyone else would earn its share of
the LP fee, which these adapters book as the hook's. Telling it apart needs that position's in-range liquidity at every
swap, replayed from the pool's `ModifyLiquidity` history (EVM: `sender` other than the hook) across the ticks each swap
crosses; no such position is known, so the adapters do not. On Solana the sale's other buys
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
