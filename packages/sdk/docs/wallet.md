# Wallet quickstart

You want to show a person what a transaction will do, explain why one failed, and let them claim what they are owed. The SDK decodes; you display.

## Read what happened

A Solana transaction's program log carries events. Decode one data blob at a time:

```ts
import { decodeEventData } from "@floor/sdk/solana/floor/events";

const ev = decodeEventData(bytes); // { name, fields } or null when it is not a FLOOR event
```

On EVM, decode a log from the FloorAmm contract:

```ts
import { decodeLogV2 } from "@floor/sdk/v2/evm";

const ev = decodeLogV2({ topics, data }); // { name, fields } or null
```

Both decoders are exact. A body with bytes left over, or too few, throws rather than guess, so a wallet never shows a half-read event as a real one. A log that is not a FLOOR event returns `null`.

## Explain a failure

```ts
import { decodeErrorV2 } from "@floor/sdk/v2/evm";

const name = decodeErrorV2(revertData); // the custom error's name, or null
```

Show the name and your own plain wording for it. The SDK gives you the identifier, not a sentence.

## Claim

The program has one claim for everything a wallet is owed: the creator's fee share (Team Fees), referral shares, a Team Buy, and holder rewards. The claim takes bits that say which kinds to collect.

```ts
import { claimV2Call } from "@floor/sdk/v2/evm";

const call = claimV2Call({ floorAmm, wallet, types: bits, quotes: [quoteCoin] });
// { to, data }: the wallet signs and sends it
```

To list what a wallet can claim first, use the indexer's claim view (`readClaimView` in `@floor/sdk/indexer/read`) and show the amounts before you build the call. Holder rewards on EVM need the holder to opt in to the token first (`optInCall`).

## Things to know

- Amounts are `bigint` in the coin's smallest unit.
- The SDK builds no claim lookup table on Solana; it only records a wallet's own table.
- Nothing here signs. You pass the result to your own signer.

See also: [README](../README.md), [frontend guide](frontend.md).
