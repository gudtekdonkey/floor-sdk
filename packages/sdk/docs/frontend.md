# Frontend quickstart

You want a page that lists launches, quotes a trade and builds the transaction for the wallet to sign. The package runs in a browser: no dependencies, no network calls of its own, no code at import.

## Import from the root

```ts
import { v2Route, indexerRead, mathV2Display } from "@floorlaunchpad/sdk";
```

The root entry exposes each module as a namespace, and a bundler keeps only the ones you use. If you want the smallest possible import, name the module: `@floorlaunchpad/sdk/indexer/read`.

## Show data

Read from your indexer with an injected `get` (see [bot guide](bot.md)). Launch reads come back with `bigint` amounts; format them at the edge of the page, not before.

`@floorlaunchpad/sdk/math/v2/display` and the other `math/v2` modules hold the figures the page shows. They use the same integer math as the program, so the number on the page is the number the program pays.

## Quote and build

Use the routing functions in [docs/aggregator.md](aggregator.md). For a single launch, find a one-hop path, quote it with `slippageBps` from the user's setting, and pass the result to `routeInstructionsV2` (Solana) or `evmRouteCallV2` (EVM). The wallet signs what you return.

## Things to know

- Route modules in `src/route` and `src/v2/route.ts` contain no host names, so a strict content security policy does not trip on them. Anything that talks to a hosted service is in `route-jupiter.ts` and is opt-in.
- Keep the quote fresh: re-quote when the user changes the amount and just before they sign.
- A buy paid in another coin goes through the chain's FloorZap: take its address from the zap row's `zap` (`FLOOR_ZAP_ADDRESSES`, `@floorlaunchpad/sdk/evm/chains`) and its swap from `zapSwapData`; [aggregator.md](aggregator.md) has the details.
- The create page's preset menu on EVM: `readPresetMenuEvmV2` (`@floorlaunchpad/sdk/v2/presetRowEvm`) reads the rows through one Multicall3 `aggregate3` per 100 ids. Pass `{ multicall }` for another Multicall3 address, or `{ multicall: null }` on a chain without one (one call per id). A row that fails throws; the menu never has a gap.
- Chain choice happens once, at the edge. Past that point the same quote and build calls work on both chains.

See also: [README](../README.md), [wallet guide](wallet.md).
