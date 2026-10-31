# @floor/sdk-solana

The FLOOR Program on Solana: PDAs, account and event decoders, instruction and transaction builders, over @floor/sdk.

```sh
npm install @floor/sdk-solana
```

```ts
import { solanaInstruction, math } from "@floor/sdk-solana";
```

The root entry re-exports, from `@floor/sdk` at the same version, the namespaces for Solana and the ones both chains
share. Nothing runs at import, so a bundler keeps only what a file reads. Every other module, and every subpath import,
is in `@floor/sdk`; its README has the guides.

Solana namespaces: `solanaInstruction`, `solanaPda`, `solanaTx`, `solanaAlt`, `solanaFloorProgram`, `solanaFloorAccounts`, `solanaFloorInstructions`, `solanaFloorCreate`, `solanaFloorEvents`, `solanaFloorRegistry`, `solanaFloorMetadata`, `solanaFloorGenesis`, `solanaFloorMint`, `v2Layouts`, `v2Solana`, `v2SolanaSettle`, `v2BurnPreview`, `v2QuoteMint`, `v2Genesis`, `v2Events`, `v2Devbuy`, `v2Claims`, `v2RouteAlt`.

Shared namespaces: `math`, `codec`, `quote`, `handle`, `burn`, `config`, `errors`, `events`, `abi`, `client`, `generated`, `indexerRead`, `indexerStream`, `v2Route`, `v2Tx`, `v2Press`, `v2ReceiptTx`, `v2Types`, `v2Circulating`, `v2Dead`.

Node 22.18 or newer. No other dependency. Licensed under the Apache License, Version 2.0 (`LICENSE`).
