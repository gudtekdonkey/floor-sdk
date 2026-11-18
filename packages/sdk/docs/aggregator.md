# Aggregator quickstart

You want to route a swap through a FLOOR Program launch: find the path, price it, and get something to sign. The SDK does the routing and building. You supply the pool list, the per-launch quote and the signing.

A launch trades against the program from its first block. There is no separate pool to deploy, so a launch token is a route hop like any other coin.

## 1. Find a path

```ts
import { findRouteV2 } from "@floorlaunchpad/sdk/v2/route";

const path = findRouteV2(pools, fromCoin, toCoin);
```

`pools` is a list of `PoolV2` entries, one per launch (the type keeps its name, but a launch has no pool: each entry is a launch that trades against the program). Each has its chain (`"solana"` or `"evm"`), the launch token, the coin it trades against (its quote coin, such as SOL or USDC), and the launch state your quoter reads. On EVM each pool also carries its `launchId`. An outside leg, such as a swap on another venue between two coins, is an "edge" hop that you price yourself.

## 2. Quote it

```ts
import { quoteRouteV2 } from "@floorlaunchpad/sdk/v2/route";

const q = quoteRouteV2({
  path,
  amountIn: 1_000_000n,
  slippageBps: 50,
  quote: (launch, side, amountIn) => myQuoter(launch, side, amountIn), // returns { out, ... } for one hop
});
// q.amountOut, and q.hops[i].minOut for each hop
```

The hop quoter is yours: it runs the program's math on a copy of the launch state and returns what one hop pays. `slippageBps` is 0 to 10,000 (100 % is allowed; every minimum is still at least 1). A path that trades the same launch token twice, or has no hop on a FLOOR launch, is refused with a `RangeError`. `findRouteV2` throws a `NoRouteError` (exported from `@floorlaunchpad/sdk/route`) when there is no path. When the path includes an outside leg, use `quoteRouteEdgeV2` and pass an `edge` function that prices it.

## 3. Build the transaction

Solana returns instructions. You give the trader, the quote, and the trader's balance of every coin on the path before the trade. Use `0n` for a coin the trader holds none of. A missing entry throws, so a route never spends a balance you did not mean to:

```ts
import { routeInstructionsV2, routeTransactionsV2 } from "@floorlaunchpad/sdk/v2/route";

const ixs = routeInstructionsV2({ trader, quote: q, balances });
// or, split across transactions when one will not fit:
const txs = routeTransactionsV2({ trader, quote: q, balances, lookupTables });
```

EVM returns calldata. `evmRouteCallV2` builds one call to the router; `evmHopCallsV2` builds one call per hop for a caller that sends them in turn:

```ts
import { evmRouteCallV2 } from "@floorlaunchpad/sdk/v2/route";

const call = evmRouteCallV2(q, { router, to: recipient, deadline });
// { to, data, ... }: send it from the trader's wallet
```

## Pay with another coin (EVM zap)

A buyer who holds the native coin, or an ERC-20 that is not the launch's quote, buys through a FloorZap, which swaps into the quote and buys in one transaction.

- Addresses: `FLOOR_ZAP_ADDRESSES` (`@floorlaunchpad/sdk/evm/chains`) holds the deployed zap per chain and router slot, 15 instances on 8 chains. Each `EVM_CHAINS` zap row carries its own `zap`, `null` where that slot is not deployed. Keep no copy of your own.
- Swap calldata: `zapSwapData` (`@floorlaunchpad/sdk/evm/zap`) builds the zap's `swapData`. The input is the native coin (the default) or an ERC-20 (`input: "token"`). The output is the launch's ERC-20 quote (the default) or, for a launch quoted in the native coin, the native coin (`output: "native"`).
- A Universal Router fork row passes that row's `urFork` (`"uniswap"`, `"aerodrome"`, `"pancake"` or `"ramses"`). Uniswap v4 is one pool given as `route.v4`. A native-input Universal Router swap ends with a `SWEEP`, so anything the swap did not spend goes back to the zap, which refunds it.
- Bridged buys: `buyHookNative` (in `src/evm/squid.ts`) builds the destination-chain hook calls. Pass the launch's `quote`. For an ERC-20 quote the hook adds a third call that passes the quote refund on to the user. Absent or address(0), it is the two calls of 0.4.0.

## FloorRouter addresses (EVM)

`evmRouteCallV2` takes the FloorRouter's address as `router`. The SDK books one per chain and zap slot: `FLOOR_ROUTER_ADDRESSES` and each zap row's `floorRouter` (`@floorlaunchpad/sdk/evm/chains`), the same address for a slot on every chain that has it. All 15 are deployed (2026-11-17) on the eight EVM chains, behind `FLOOR_ROUTER_DEPLOYER`; `FLOOR_ROUTER_FROM_BLOCK` is the block that deployer was created in on each chain, where an indexer of router events starts. The same addresses are also live on two soak testnets (2026-11-17): Sepolia `uniswap-ur` and HyperEVM testnet `hyperswap`; `FLOOR_ROUTER_FROM_BLOCK_SOAK` is the deployer's creation block there (null on a soak that has no router). The addresses are still derived, not read back, so a slot added later is usable only once it has code on its chain: a call to an address with no code succeeds and buys nothing, and a native-in buy's coin goes to the empty address and is lost. Check it first (`getCode` answering "0x" or `undefined` is no code):

```ts
import { FLOOR_ROUTER_ADDRESSES } from "@floorlaunchpad/sdk/evm/chains";
import { assertFloorRouterLive } from "@floorlaunchpad/sdk/evm/router";

const router = await assertFloorRouterLive({ getCode: (a) => rpc.getCode(a) }, { chain: "base", slot: "uniswap-ur" });
// throws while the router has no code; otherwise pass `router` to evmRouteCallV2
```

## Things to know

- The SDK never sends or signs. It returns instructions or calldata.
- Fees are added on top of the amount in and come out of the quote you build the call from. Do not subtract them again.
- Every launch trades from its first block and neither buys nor sells can be paused. A hop that fails does so because of the amount or the slippage bound, not a switch.
- Amounts are `bigint` in the coin's smallest unit on both chains.
- A failed EVM call decodes with `decodeEvmProgramError` (`@floorlaunchpad/sdk/errors`), which also knows the five errors the contracts' math libraries revert with: `PriceOrder()`, `ZeroLiquidity()`, `SqrtRange()`, `Zero()` and `VaultShareOutOfRange()`.

See also: [README](../README.md), [bot guide](bot.md) for reading launches to build your pool list.
