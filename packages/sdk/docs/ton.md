# FLOOR on TON

> TON is not deployed yet: the module ships in `@floorlaunchpad/sdk` 0.6.0, switched off: the TON chain row stays off until a mainnet factory is live.

`@floorlaunchpad/sdk/ton` builds the messages a wallet sends to the FLOOR program on TON and reads what the program answers.
It holds no key, sends nothing and opens no connection of its own: you give it a read function (`TonRpc`) and a wallet.
Amounts are in nanoGRAM (GRAM has 9 decimals).

## Build a trade

A trade is a `TonTx`: `{ family: "ton", messages }`, each message with a destination, a value in nanoGRAM, a body cell and a
`bounce` flag. Build the chain once (`tonChainOf()` uses the built-in mainnet constants; see "Built-in constants"), then build the message:

```ts
import { tonChainOf, launchAddress, buildBuyGram, simBuy } from "@floorlaunchpad/sdk/ton";

const chain = tonChainOf();                          // mainnet, from the SDK's own constants
const launch = launchAddress(chain, launchId);       // needs the launch init code, see below
const quote = await simBuy(rpc, launch, 1_000_000_000n, BigInt(nowSeconds));
const tx = buildBuyGram({ launch, money: 1_000_000_000n, minTokens: quote.tokens * 99n / 100n, deadline: nowSeconds + 300, recipient: myAddress });
```

`simBuy` and `simSell` run the program's own `get` methods on the launch: they are the quote path. There is no second
price formula in the SDK for TON. A builder refuses bad input (a zero amount, a deadline in the past) with a `RangeError`
before it makes a message.

`tonChainOf` can derive the wallet, account and trader addresses. It cannot derive a launch address, because the launch
init code is not in the book: `launchAddress` throws `launch init code not supplied` rather than guess. Pass the launch
address you read from the factory, or supply the code.

## Send it

`tonConnectRequest` turns a `TonTx` into the request a TON Connect wallet takes:

```ts
import { tonConnectRequest } from "@floorlaunchpad/sdk/ton";

const request = tonConnectRequest(tx, { nowS: Math.floor(Date.now() / 1000) });
await tonConnectUi.sendTransaction(request);
```

Each message maps to `{ address, amount, payload, stateInit? }`: the address with its bounce flag, the value as a decimal
string, the body and any state init as base64. `validUntil` is `nowS` plus `validForS` (default 300, at most 600). A
request carries 1 to `maxMessages` messages (default 4); more is a `RangeError`. The function is pure.

## Read the result

A message that reaches the chain can still be refused by the program. Ask the indexer, by the wallet and the message's
query id:

```ts
import { readTonOutcome } from "@floorlaunchpad/sdk";

const outcome = await readTonOutcome(get, walletAddress, queryId);
```

The answer says whether the message landed, was refused (with the program's error) or is not seen yet. Treat "not seen yet"
as pending, never as failed. Use a fresh `queryId` for each message.

## Built-in constants

The SDK carries TON mainnet's factory and timelock addresses and the three small init codes (wallet, account, trader) as
constants in a generated module (`TON_MAINNET`, `TON_INIT_CODES_BASE64`, `TON_PINNED_CODE_HASHES`). `tonChainOf()` with no
argument builds the mainnet chain from them: no file to fetch. The module is written by `node sdk/scripts/gen-ton-consts.mjs`
from the pinned codes (`ton/contracts/codes.mjs`) and the deploy output (`sdk/scripts/ton-mainnet-deploy.json`); `--check`
fails while the committed module differs. Mainnet is not deployed yet, so `TON_MAINNET.factory` and `.timelock` are `null`
(`placeholder: true`) and `tonChainOf()` throws `TonBookError` until the deploy output is filled in and the generator re-run.

## The book file (another network or deployment)

For a testnet or any other deployment, pass a book: a small JSON file for one network:

```json
{
  "chain": "ton", "network": "mainnet", "globalId": -239,
  "factory": "0:<64 hex>", "timelock": "0:<64 hex>",
  "codeHashes": { "factory": "…", "timelock": "…", "launch": "…", "wallet": "…", "account": "…", "trader": "…" },
  "familyInit": { "launch": "…", "account": "…", "trader": "…" },
  "codes": { "wallet": "<base64 BoC>", "account": "<base64 BoC>", "trader": "<base64 BoC>" },
  "readAt": "<ISO 8601 time>", "deploySeqno": 123456
}
```

```ts
import { parseTonBook, tonChainOf } from "@floorlaunchpad/sdk/ton";
const chain = tonChainOf(parseTonBook(JSON.parse(bookFileText), { file: "ton-testnet.json" }));
```

`network` is `mainnet` with `globalId` -239, or `testnet` with -3. `familyInit` and `deploySeqno` are optional; other keys
are ignored. `parseTonBook` checks every field and the three code cells against their hashes and the pinned code, and throws
`TonBookError` (`ton book <file>: <field>: <what>`) on the first fault. A book that parses derives addresses identical to
the chain's. `checkTonBookOnChain(rpc, book)` compares the book with the chain and returns a list of mismatches (empty means
consistent); run it from your own machine, not in a page.

## Gas

Every builder has a default value that covers the program's gas and forwarding on the current network prices. The claim
costs are priced in `gas.ts` from a `TonGasConfig` (basechain prices); the default is the mainnet config, and you can pass
a live one. Unused value comes back as a refund. A message that carries too little value is refused by the program, so
raise the value rather than lower it.

## What the SDK does not do

- It does not sign and it does not send. `tonConnectRequest` is the end of its work.
- It has no decoder for the book's cells beyond what `parseTonBook` checks, and no reader for the factory's internals.
- It has no price formula for TON: `simBuy` and `simSell` are the quote path.
- It does not guess an address it cannot derive: it throws.
