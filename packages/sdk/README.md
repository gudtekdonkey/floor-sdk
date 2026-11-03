# @floorlaunchpad/sdk

The FLOOR Program in TypeScript. It holds the program's math (bit for bit with the Rust crate), quoting, account and event decoders, and transaction builders for Solana and for EVM chains. It has no dependencies and opens no connection: you pass in your own RPC, indexer `get` and send functions.

> **A request: show the floor.** If you route or list FLOOR pools, please show the FLOOR line on the token's price chart and
> the floor price somewhere on the token's page. That is the bare minimum we ask for; anything more is a bonus. It is
> not a requirement, and it is highly appreciated. The field is `floorPriceNative`; the full guide is [docs/floor-line.md](docs/floor-line.md).

`npm install @floorlaunchpad/sdk`. Chain-scoped entries over it: `@floorlaunchpad/sdk-evm` and `@floorlaunchpad/sdk-solana`. Apache-2.0.

## Install and import

```ts
import { quote, solanaPda, v2Route } from "@floorlaunchpad/sdk";        // the root entry: one namespace per module
import { findRouteV2, quoteRouteV2 } from "@floorlaunchpad/sdk/v2/route"; // or one module by its path
```

The root entry re-exports every subpath as a namespace named after its path: `./solana/floor/events` is `solanaFloorEvents`, `./v2/route` is `v2Route`. Importing the package runs no code (`"sideEffects": false`), so a bundler drops the namespaces you never read. Subpath imports stay valid and are the smallest option for a server script.

Node 22.18 or newer. The package ships compiled JavaScript in `dist/` with the types pointing at `src/`.

## Pick your guide

| You are | Start here |
|---|---|
| An aggregator or router | [docs/aggregator.md](docs/aggregator.md): find a route, quote it, build the Solana instructions or the EVM call |
| A wallet | [docs/wallet.md](docs/wallet.md): decode what a transaction does, read errors, build a claim |
| A bot or indexer | [docs/bot.md](docs/bot.md): read launches, follow the stream, decode logs |
| A frontend | [docs/frontend.md](docs/frontend.md): the root entry, the indexer readers, the math for display |
| An aggregator or chart that lists FLOOR pools | [docs/floor-line.md](docs/floor-line.md): draw the floor line (a request, not a requirement) |

## Show the floor on your chart

A launch's floor is the price the program's own money stands behind. Drawing it under the price line shows holders what backs them. [docs/floor-line.md](docs/floor-line.md) says which field to read, how to draw the line and what to watch for (units, burns, the gap between floor and price, graduation). [examples/floor-line.html](examples/floor-line.html) is a working page.

## Versions

The version follows semver. While the major number is 0, a minor bump (0.2 to 0.3) may rename or remove an export and a patch bump never does. The plan and the history are in [CHANGELOG.md](CHANGELOG.md).

**Fees in a quote.** A quote's `fees` has the five fee legs, each added on top of the trade, buys and sells alike: the
Backing Fee (`backing`), the Team Fee (`team`), the Platform fee (`platform`), the $FLOOR fee (`floor`) and the Holder Fee
(`holder`), with their `total`. Any leg can be 0. Before graduation a preset launch pays only the Platform fee and the
$FLOOR fee; the other three start at graduation. The rates are the launch's own, read from its state, so take them from
the quote rather than from a table. Marketcap Milestones are not fees: they sit inside the price, are paid only by buys on
the way up and never appear in `fees`.

**Reading an indexer.** `@floorlaunchpad/sdk/indexer/read` has one reader per indexer route and a decoder for each body. The SDK opens no connection and names no host. You pass a `Get`, a function from a path such as `/floor-amm/launches/<id>` to `{ status, body }`, and it can be `fetch` against your own indexer, a proxy, or a cache:

```ts
import { readLaunch, readTrades, IndexerError, type Get } from "@floorlaunchpad/sdk/indexer/read";

const get: Get = async (path) => {
  const r = await fetch(`https://indexer.example/chains/base${path}`);
  return { status: r.status, body: await r.json() };
};

const { launch } = await readLaunch(get, "42");
const page = await readTrades(get, launch.id, { limit: 50 });
const older = page.next === null ? null : await readTrades(get, launch.id, { limit: 50, before: page.next });
```

Amounts come back as `bigint`. A non-200 answer throws an `IndexerError`: `notReady` while the indexer is still catching up to the chain, `unknownLaunch` for an id it does not know. A body of the wrong shape throws a `ShapeError` naming the field. `@floorlaunchpad/sdk/indexer/stream` parses the live stream's frames in the same way, and `indexerPath` builds any route's path if you want to do the request yourself.

**Launch ids.** FLOOR has no pool: a launch trades against the program. On EVM it is FloorAmm's `launchId` (a uint256, written as its decimal string); on Solana it is the address of the `Launch` account. Every launch read (the `LaunchId` type) takes either that id or the launch's token address, and both name the same launch. The answer names the launch by its id: `launch.id` on a launch read, `launch` on each row elsewhere. The transaction builders take the id itself, so look a token up with `readLaunch` before you build a trade.
