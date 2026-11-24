# Bot and indexer quickstart

You want to watch launches and trades, keep your own copy of the data, or react to events. The SDK gives you typed readers for an indexer and decoders for raw chain data. It opens no connection itself.

## Read an indexer

Pass a `get`: a function from a path to `{ status, body }`. It can be `fetch`, a proxy, or a cache.

```ts
import { readLaunches, readLaunch, readTrades, type Get } from "@floorlaunchpad/sdk/indexer/read";

const get: Get = async (path) => {
  const r = await fetch(`${INDEXER}${path}`);
  return { status: r.status, body: await r.json() };
};

const { launch } = await readLaunch(get, "42");
const page = await readTrades(get, launch.id, { limit: 100 });
```

Pages carry a `next` cursor; stop when it is `null`. On trades, marks and the inbox pass it back as `before`; on `launches`, `search` and the other wallet routes pass it back as `cursor`. Amounts are `bigint`. A non-200 answer throws an `IndexerError` (`notReady` while the indexer catches up, `unknownLaunch` for an id it does not know). A body of the wrong shape throws a `ShapeError` that names the field.

### List and search launches

`readLaunches` answers one page, 100 launches by default. No request answers every launch, so walk the pages:

```ts
import { readLaunches, readLaunchSearch, readCandles } from "@floorlaunchpad/sdk/indexer/read";

let cursor: string | undefined;
do {
  const page = await readLaunches(get, { limit: 100, cursor });
  // page.launches …
  cursor = page.next ?? undefined;
} while (cursor !== undefined);

const hits = await readLaunchSearch(get, { q: "wagmi" }); // best match first, each row with its tier
```

A client made with `createIndexerClient` walks them for you: `for await (const page of ix.drain("launches", {}))`.

### Wallet lists

Called with neither `limit` nor `cursor`, `readPositions`, `readWalletLocks`, `readClaimHistory`, `readRoles` and
`readPeople` return the whole list. Pass either one to get pages: a page is 100 rows unless `limit` asks for another
size, and 500 at most. `readInbox` pages with `before`. Every list is in [cursor-pages.md](cursor-pages.md).

### Candles with the floor

```ts
const { candles } = await readCandles(get, { launch: launch.id, interval: "1h" });
```

Each candle carries `floorX64`, the floor at the bucket's close, in the same Q64.64 units as a launch's `floorX64`. It is
`null` where a bucket has none, and absent from an indexer older than this build. Use the launch's own `floorX64` for
that candle then. [floor-line.md](floor-line.md) shows how to draw it.

## Follow the live stream

```ts
import { parseStreamFrames, streamPath } from "@floorlaunchpad/sdk/indexer/stream";

let rest = "";
// feed each text chunk from your server-sent-events connection to path streamPath()
const { frames, rest: left } = parseStreamFrames(rest + chunk);
rest = left;
```

`parseStreamFrames` returns the complete frames and the unfinished tail; keep the tail for the next chunk.

## Decode chain data yourself

For raw logs and accounts, skip the indexer: `decodeLogV2` (EVM logs) and `decodeEventData` (Solana events) decode events; `@floorlaunchpad/sdk/v2/layouts` decodes Solana accounts. Every decoder is exact and throws on a length mismatch.

## Trade

A bot that trades builds the same transactions as an aggregator: see [docs/aggregator.md](aggregator.md). Run the quote against fresh launch state just before you send, and set `slippageBps` for the time between the quote and the block.

## Things to know

- Neither buys nor sells can be paused, so a bot that stops quoting is not waiting on a switch.
- Launch ids are the pool's id: a uint256 decimal string on EVM, the `Launch` account address on Solana. A token address also works for reads.
- The SDK never signs or sends.

See also: [README](../README.md).
