# Changelog

Notable changes to the packages and crates in this repository. Versions follow [Semantic Versioning](https://semver.org);
while the version is `0.x`, a minor release may change the API.

## Unreleased

### Changed

- The packages are published under the `@floorsupply` npm scope: `@floorsupply/sdk`, `@floorsupply/sdk-evm`, `@floorsupply/sdk-solana`.

### Added

- `@floorsupply/sdk`: the approved quote assets per live chain (`APPROVED_QUOTE_TOKENS`, `isApprovedQuote`, `approvedQuoteSymbol`).
- `@floorsupply/sdk`: `dex`, the DEX adapter's pair model and event mapping (GeckoTerminal / DEX Screener integration API).
- `integrations/defillama`: a treasury adapter.
- `integrations/velora`: FLOOR hook pools in Velora's `paraswap-dex-lib` for Base and BSC.
- `integrations/kyberswap`: the FLOOR hook for KyberSwap's `kyberswap-dex-lib`, tested against the same V4Quoter fixtures.
- `@floorsupply/sdk-evm`: `simulateSwap` and `readSwapSnapshot`, an off-chain simulator of a swap on a FLOOR pool (Uniswap v4's
  swap with the FLOOR hook's fees), tested against Uniswap's V4Quoter on pools created on a fork of Base and on the live
  graduated pools of Ethereum, BSC, Base, Robinhood Chain and Arc (`scripts/live-parity.mts`).
- `@floorsupply/sdk-evm`: event decoders (`decodeFloorLog`, `decodeFloorLogs`, `floorEventTopics`, `FLOOR_EVENTS`).
- `@floorsupply/sdk-evm`: each mainnet deployment row carries `singletonHook`, `upgradeTimelock` and `platformTreasury`.
- `docs/evm-events.md`: how FLOOR's EVM events become launches, pools, trades, the floor and candles.
- `integrations/defillama`: DefiLlama TVL, volume, fees and revenue adapters, EVM and Solana.
- Continuous integration.

## 0.1.0

First release: `@floorsupply/sdk`, `@floorsupply/sdk-evm`, `@floorsupply/sdk-solana`; the `floor-types`, `floor-math`, `floor-quote` and
`floor-quote-wasm` crates; `floor-sale-amm`.
