# Reading FLOOR's EVM events

How FLOOR's events on EVM chains become launches, pools and trades. `@floorsupply/sdk-evm`'s `decodeFloorLog` decodes every event
named here; the addresses are in its `deployments.ts`.

## Launches and pools

- **Launches.** Each chain's `FloorRegistry` emits `Launched(launch, token, hook, rewards, poolId, mode, name, symbol,
  registrar)` once per launch, and lists them all through `count()` / `page(start, n)`. Read the registry rather than the
  factory's `LaunchCreated`, whose field list differs between deployment sets.
- **Pools.** A launch trades on its sale contract (`launch`) until it graduates, then on a Uniswap v4 pool whose hook is
  the chain's `singletonHook`. The pool id is `hook.poolForAccount(launch)`: zero before graduation. The hook emits
  `PoolRegistered(poolId, launch, factory)` when it binds the pool, and the PoolManager's `Initialize` names the hook.
- **Orientation.** A pool's quote is `currency0` when its address sorts below the launch token's (the native coin,
  `address(0)`, always does), else `currency1`. `launch.quoteToken()` gives the quote.

## Trades

**While a launch sells** (events from the `launch` contract, in the launch's quote units):

| Event | Trade |
|---|---|
| `Bought(who, tokens, ethSpent, soldTotal, raisedTotal)` | A buy. `ethSpent` is net of the FLOOR Platform fee; the fee is the `TeamFeeCharged(who, fee, sellBack = false)` emitted just before it. Gross = `ethSpent + fee`. |
| `SoldBack(who, tokens, ethReturned, soldTotal, raisedTotal)` | A sell back to the sale. `TeamFeeCharged(who, fee, sellBack = true)` precedes it. |
| `TeamBought(buyer, stepTokens, walletTokens, cliffs, cost, fee)` | The deployer's team buy: `cost + fee` paid. It emits no `Bought`. |
| `TeamBuySoldBack(buyer, tokens, cost, fee)` | The team buy sold back: `cost - fee` paid out. It emits no `SoldBack`. |
| `Graduated(raised, floorEth, polEth, ladderTokens, openTick)` | The sale sold out and the pool opened. |

`BoughtFor(payer, recipient, quoteSpent)` follows a `Bought` paid for by another address; it is not a second trade.

**After graduation** (the PoolManager's `Swap(id, sender, amount0, amount1, sqrtPriceX96, liquidity, tick, fee)` where `id`
is a FLOOR pool):

- The quote leg is `amount1` when the quote is `currency1`, else `amount0`. Amounts are the swapper's deltas: a negative
  quote leg means quote was paid in, a buy.
- **A swap whose `sender` is the hook is not a trade.** The hook swaps in its own name when it re-places the bid during a
  sell and at a ratchet. Leave these out of volume and trade lists, and never let them move a candle's open.
- The price is `sqrtPriceX96` in the pool's frame: quote per token is `2^192 / sqrtP^2` when the quote is `currency0`,
  `sqrtP^2 / 2^192` when it is `currency1` (then scaled by the two decimals).

## The trader

The trader of a pool swap is the one address, other than FLOOR's and Uniswap's contracts and the zero address, that
appears in the launch token's `Transfer` events in the swap's transaction. None, or two different ones, is no trader.
Never `tx.from`: under account abstraction it is a bundler.

## The floor

`Ratcheted(poolId, floorTick, askTick, vaultEth, inventoryTokens, bounty)` (from the hook) moves the floor. The published
floor is the price at `floorTick`, and it never moves down. The bid sits in the band `[floorTick - width, floorTick]`;
`hook.execute(poolId, backingEth())` gives the backing (the vault plus the deployed bid) in raw quote units.

## Candles

A candle's open is the previous trade's close (the pool's first trade opens at its own price), and its high and low
include the open. Hook swaps are not trades and do not move the close. Bucket by `floor(timestamp / bucket) * bucket`.
