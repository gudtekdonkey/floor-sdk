# @floor/sdk-evm

The FLOOR Program on EVM chains: ABIs, call builders, event decoders, chain config and reads, over @floor/sdk.

```sh
npm install @floor/sdk-evm
```

```ts
import { evmKeccak, math } from "@floor/sdk-evm";
```

The root entry re-exports, from `@floor/sdk` at the same version, the namespaces for EVM chains and the ones both chains
share. Nothing runs at import, so a bundler keeps only what a file reads. Every other module, and every subpath import,
is in `@floor/sdk`; its README has the guides.

EVM chains namespaces: `evmKeccak`, `evmAbi`, `evmItems`, `evmZap`, `evmRouter`, `evmChains`, `evmMulticall`, `evmFloorAbiGen`, `evmFloorIface`, `evmFloorCalls`, `evmFloorEvents`, `evmBalances`, `v2Evm`, `v2EvmReads`, `v2PresetRowEvm`, `v2ClaimAllEvm`, `v2MarketEvm`, `v2ClaimsEvm`.

Shared namespaces: `math`, `codec`, `quote`, `handle`, `burn`, `config`, `errors`, `events`, `abi`, `client`, `generated`, `indexerRead`, `indexerStream`, `v2Route`, `v2Tx`, `v2Press`, `v2ReceiptTx`, `v2Types`, `v2Circulating`, `v2Dead`.

Node 22.18 or newer. No other dependency. Licensed under the Apache License, Version 2.0 (`LICENSE`).
