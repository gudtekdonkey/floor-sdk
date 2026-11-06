# FLOOR Program: aggregator adapters

Integration code for routers and aggregators that quote and route FLOOR Program launches. Each folder stands alone.

| Partner | Folder | What it is |
|---|---|---|
| Jupiter | [`solana/`](solana) | Rust: `floor-math` (the protocol math), `floor-amm-solana` (account decoding, quoting, instruction building) and the Jupiter AMM interface crate |
| Titan | [`titan/`](titan) | The Titan integration template with the FLOOR Program venue filled in |
| DFlow | [`solana/`](solana) | The Solana crates: decode, quote and build swaps |
| OKX (Solana) | [`solana/`](solana) | The Solana crates: decode, quote and build swaps |
| KyberSwap | [`kyberswap/`](kyberswap) | A patch for kyberswap-dex-lib: the FloorAmm liquidity source |
| Velora | [`velora/`](velora) | A patch for paraswap-dex-lib: the FloorAmm dex |
| 1inch | [`evm/`](evm) | TypeScript reference module for FloorAmm on EVM: discovery, state, logs, math, calldata |
| LI.FI | [`evm/`](evm) | The same module, with the LI.FI reference shape |
| Squid | [`evm/`](evm) | The same module, with the Squid hook shape |

The math in every folder is the FLOOR Program's reference math and is tested against the same golden cases.
The EVM module vendors the source files of `@floorlaunchpad/sdk` 0.3.2 it uses under `evm/vendor/sdk`.

Licence: Apache License 2.0 (each folder's `LICENSE`).
