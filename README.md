# floor-sdk

The SDK for [FLOOR](https://floor.supply): the FLOOR Program's math, quoting, account and event decoders, and
transaction builders, for Solana and for EVM chains. FLOOR is one program per chain, and a launch trades against it from
its first block.

| Package | Directory | What it holds |
|---|---|---|
| `@floor/sdk` | [`packages/sdk`](packages/sdk) | Everything: the math (bit for bit with the program), quotes, routes, decoders and builders for both chain families |
| `@floor/sdk-evm` | [`packages/sdk-evm`](packages/sdk-evm) | The EVM part: ABIs, call builders, event decoders, chain config and reads |
| `@floor/sdk-solana` | [`packages/sdk-solana`](packages/sdk-solana) | The Solana part: PDAs, account and event decoders, instruction and transaction builders |

Version 0.3.0. No dependencies, and no connection is opened: you pass in your own RPC, indexer and send functions.
Node 22.18 or newer.

Start with [`packages/sdk/README.md`](packages/sdk/README.md), then the guide for what you build:
[aggregators and routers](packages/sdk/docs/aggregator.md), [wallets](packages/sdk/docs/wallet.md),
[bots and indexers](packages/sdk/docs/bot.md), [frontends](packages/sdk/docs/frontend.md).

Licensed under Apache-2.0 ([LICENSE](LICENSE)). Security reports: [SECURITY.md](SECURITY.md).
