# Aggregator quickstart

You want to route a swap through a FLOOR Program launch: find the path, price it, and get something to sign. The SDK does the routing and building. You supply the pool list, the per-launch quote and the signing.

A launch trades against the program from its first block. There is no separate pool to deploy, so a launch token is a route hop like any other coin.

## 1. Find a path

```ts
import { findRouteV2 } from "@floor/sdk/v2/route";

const path = findRouteV2(pools, fromCoin, toCoin);
```

`pools` is a list of `PoolV2` entries, one per launch (the type keeps its name, but a launch has no pool: each entry is a launch that trades against the program). Each has its chain (`"solana"` or `"evm"`), the launch token, the coin it trades against (its quote coin, such as SOL or USDC), and the launch state your quoter reads. On EVM each pool also carries its `launchId`. An outside leg, such as a swap on another venue between two coins, is an "edge" hop that you price yourself.

## 2. Quote it

```ts
import { quoteRouteV2 } from "@floor/sdk/v2/route";

const q = quoteRouteV2({
  path,
  amountIn: 1_000_000n,
  slippageBps: 50,
  quote: (launch, side, amountIn) => myQuoter(launch, side, amountIn), // returns { out, ... } for one hop
});
// q.amountOut, and q.hops[i].minOut for each hop
```

The hop quoter is yours: it runs the program's math on a copy of the launch state and returns what one hop pays. `slippageBps` is 0 to 10,000 (100 % is allowed; every minimum is still at least 1). A path that trades the same launch token twice, or has no hop on a FLOOR launch, is refused with a `RangeError`. `findRouteV2` throws a `NoRouteError` (exported from `@floor/sdk/route`) when there is no path. When the path includes an outside leg, use `quoteRouteEdgeV2` and pass an `edge` function that prices it.

## 3. Build the transaction

Solana returns instructions. You give the trader, the quote, and the trader's balance of every coin on the path before the trade. Use `0n` for a coin the trader holds none of. A missing entry throws, so a route never spends a balance you did not mean to:

```ts
import { routeInstructionsV2, routeTransactionsV2 } from "@floor/sdk/v2/route";

const ixs = routeInstructionsV2({ trader, quote: q, balances });
// or, split across transactions when one will not fit:
const txs = routeTransactionsV2({ trader, quote: q, balances, lookupTables });
```

EVM returns calldata. `evmRouteCallV2` builds one call to the router; `evmHopCallsV2` builds one call per hop for a caller that sends them in turn:

```ts
import { evmRouteCallV2 } from "@floor/sdk/v2/route";

const call = evmRouteCallV2(q, { router, to: recipient, deadline });
// { to, data, ... }: send it from the trader's wallet
```

## Things to know

- The SDK never sends or signs. It returns instructions or calldata.
- Fees are added on top of the amount in and come out of the quote you build the call from. Do not subtract them again.
- Every launch trades from its first block and neither buys nor sells can be paused. A hop that fails does so because of the amount or the slippage bound, not a switch.
- Amounts are `bigint` in the coin's smallest unit on both chains.

See also: [README](../README.md), [bot guide](bot.md) for reading launches to build your pool list.
