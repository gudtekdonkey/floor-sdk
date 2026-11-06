# Changelog

All notable changes to `@floorlaunchpad/sdk`. The version follows semver; while the major number is 0, a minor bump may rename or remove an export and a patch bump never does.

## 0.3.2

Added:

- Indexer: the route `GET /floor-amm/launches/<launch>/member-sold` (also under `/floor-amm/v2/`; `INDEXER_CONTRACT.memberSold`,
  `indexerPath.memberSold`, reader `memberSold`, `readMemberSold`, decoder `MEMBER_SOLD` / `MEMBER_SOLD_ROW`, types
  `MemberSoldBody` / `MemberSoldRow`): every sell by a wallet that held a Team Slot place at that moment, oldest first, as
  `{ indexed, memberSold: [{ launch, member, place, amount, quoteOut, block, timestamp, tx, id }] }`; `place` the lowest place
  the member held, `amount` the tokens sold in the SDK's 6-decimal token units, `quoteOut` the quote paid out in base units,
  `id` `<block>:<txnIndex>:<eventIndex>`. Derived from the trades and the Team Slot; a member who moves the tokens to another
  wallet before selling is not seen.
- Indexer: the route `GET /floor-amm/launches/<launch>/account` (also under `/floor-amm/v2/`; `INDEXER_CONTRACT.launchAccount`,
  `indexerPath.launchAccount`, reader `launchAccount`, `readLaunchAccount`, decoder `LAUNCH_ACCOUNT`, type `LaunchAccountBody`,
  helper `launchAccountBytes`): a launch's whole `LaunchV2` account read live from the node at the ask (Solana only; EVM answers
  `unknown route`), as `{ indexed, launch, mint, slot, data }` with `slot` the RPC context slot and `data` the account bytes in
  base64. `launchAccountBytes(body)` returns them as a `Uint8Array` (browser and Node) for `decodeLaunchV2`. An account that is
  absent, not owned by the program or not a `LaunchV2` answers `unknown launch`; a failed node read is a 502.
- `v2Devbuy.memberSoldFromTrade` (with `MemberSold`, `TeamSeat`): the member sale of one v2 `Trade` against a launch's
  Team Slot places, or null. The indexer serves the route above from it.

Changed:

- `FLOOR_AMM_FROM_BLOCK.hyperevm` (`evm/chains.ts`) is `50398055`, the block FloorAmm was deployed at on HyperEVM (chain 999),
  no longer `null`. FloorAmm is now deployed on all seven EVM chains at `FLOOR_AMM_ADDRESS`
  (`0xF10000bd05c2C01793C6d55F027346469683d225`).

No export removed or renamed, no signature changed.

## 0.3.1

Added:

- Solana genesis: `startGenesisPublicInstruction`, `setGenesisEndInstruction`, `GENESIS_NO_END`.
- Events: `GenesisPublicStarted` and `GenesisEndSet` on Solana (`SolGenesisPublicStartedFields`, `SolGenesisEndSetFields`);
  `SolanaEvent`, `SolanaEventMap` and `FloorEvent` gain them. A `switch` over `name` that checks it is exhaustive needs the
  two new cases.

- Config: `ExplorerRow.alt` (optional): a chain's other explorers (Blockscout). Arc and Robinhood Chain now link to Etherscan
  (`arc.etherscan.io`, `robin.etherscan.io`), with Blockscout in `alt`; HyperEVM's test network links to Hyperliquid's
  testnet explorer. The old Arc, Robinhood Chain and HyperEVM test links did not load.

- Router helpers on the root entry: `isDustRefund`, `carriesNoRefund`, `gasFor`, `GAS_FLAT`, `GAS_PER_STEP`, `GAS_FLAT_STEPS`, `GAS_MAX`.
- Indexer decoder: `CG_TICKERS` rows carry `depthUp2` and `depthDown2` (nullable decimals; the indexer serves them always).
- Indexer: the route `GET /floor-amm/holders/<token>` (`INDEXER_CONTRACT.tokenHolders`, reader `tokenHolders`): a token's
  holders, balance descending, `limit` 1..500 (default 100) and `offset`; balances in the SDK's 6-decimal token units on
  both chains.
- Indexer: the route `GET /floor-amm/handles/<handleId>/places` (`INDEXER_CONTRACT.handlePlaces`, reader `handlePlaces`,
  `readHandlePlaces`, decoder `HANDLE_PLACES`): the Team Slot places a social handle holds now, bound to a wallet or not, as
  `{ indexed, handle, places: [{ launch, place }] }`; `handleId` is 0x + 64 hex. `PathParam` gains `"handle"`.
- Config: `FLOOR_AMM_FROM_BLOCK` (`evm/chains.ts`): the block each EVM chain's FloorAmm was deployed at, `null` where it is
  not deployed yet.
- Docs: `docs/floor-line.md`, a request to integrators to draw the floor line, and `examples/floor-line.html`, a page that draws it from a run of the SDK's math (`examples/gen-floor-line.mjs` makes its data). The npm package ships the page, not the script.

Changed:

- `FLOOR_AMM_ADDRESS` (`evm/chains.ts`) is `0xF10000bd05c2C01793C6d55F027346469683d225`, the FloorAmm proxy on every v2 EVM chain
  (one CREATE2 address), no longer `null`. Deployed on Ethereum, Base, BNB Chain, Arbitrum, Arc and Robinhood Chain;
  HyperEVM gets the same address at its deploy.

No export removed or renamed, no signature changed.

## 0.3.0

The first public build, in three packages under the Apache License 2.0. `@floorlaunchpad/sdk` is the whole SDK.
`@floorlaunchpad/sdk-evm` and `@floorlaunchpad/sdk-solana` are chain-scoped entries over it. Each re-exports, from `@floorlaunchpad/sdk` at the
same version, the namespaces for its chain and the shared ones.

Removed (0.x minor):

- `displayFloorV2` (`display.ts`): it served f as `b.floor` and c / k as the summed band money, not the live
  rule. Read the single functions instead: `floorX64`, `bestBidPriceX64V2`, `nextBuyPriceX64`, `dexLiquidityV2`, `sCirc`.

Renamed (0.x minor: no alias is kept, the old spelling throws or fails the type check):

- The v2 trade builders take `amountIn` (was `amount`), `tokensIn` (was `tokens`) and `minOut` (was `minTokensOut` /
  `minQuoteOut`): `buyInstructionV2`, `buyFromBalanceInstructionV2`, `sellInstructionV2`, `sellFromBalanceInstructionV2`,
  `teamShareBuyInstructionV2`, `teamShareBuyAndClaimInstructionsV2`, `teamShareBuyCallV2`, `teamShareBuyAndClaimCallV2`;
  `solPaidBuyInstructionsV2` (the routed buy, `RoutedBuyV2`) takes `minOut`. The EVM call encoders keep the contract's own
  argument names.
- `setCreatePausedInstruction` takes `authority` (required); the `timelock` signer name is gone.

Changed:

- Slippage 0 to 10,000 bps on both chains (`MAX_SLIPPAGE_BPS` was 9,999). At 10,000 (100 %) `minOutAt` signs the least
  minimum, 1; `buyLessIn` keeps no money and `buyMinTokens` returns 0, and the client and route quotes sign 1 for it.
  Above 10,000 is still refused.
- `createFloorClient` on Solana: a quote's `minOut` is never 0, as on EVM. A buy whose input less the slippage buys nothing
  (100 %, or a buy of a few base units) quotes `minOut` 1 instead of rejecting, and a sell that pays nothing quotes 1; a
  sell of 0 tokens is refused with `BadArgument`.
- `createFloorClient` on Solana nets a Token-2022 quote mint's transfer fee as the program does: a buy quotes on
  `amountIn` less the fee (its minimum on the slippage-less input less the fee, its refund as it arrives), a sell's
  `amountOut` is what arrives (`minOut` stays the program's bound, before the fee). `SolanaTransport.rpc.getEpoch`
  (optional) picks the mint's older or newer fee; without it the higher one is used.
- `v2/circulating.ts` `circulating` runs the math's `circulatingV2`, one formula with `sCirc`; same results.
- `buyLessIn` (so `buyMinTokens` and the route's buy minimum) refuses a slippage that is negative, not a whole number of
  bps, or above 10,000 with `VaultShareOutOfRange`, as the Rust helper's `u16` bound does; before, -1 gave a minimum
  above the quote and 0.5 threw a bare `RangeError`.
- `splitRowOfRowV2` reads a decoded `RowV2`'s `ckTargetBps`, as the math crate's `split_row` does (it read a `ckBps` that
  `decodeRowV2` does not set).
- `solanaSettleInputsV2` / `quoteBuySolanaV2` take `opts.mintSupply` (the base mint's live `supply`): the program syncs
  the supply before every trade (only a fall applies), so a quote after a burn the launch has not yet seen matches it.
- EVM: `readLaunchV2` reads `launchState`'s supply in chain units and divides it by 10¹² (`launchStateSupplyMath`); before,
  the live supply and the market cap were 10¹² too large and a live launch could not be read. `decodeLaunchStateSupply`
  still returns the view's raw chain numbers.
- EVM: `planEvmClaimAll` and `evmOwedReaderOfClaimsViewV2` read the deployed `owedV2` (token columns ÷ 10¹²), the one
  claims read; the undeployed claims view's call (`OWED_V2_OF_SIG`, its call builder and decoder) and the `view` option are removed.

Added:

- Subpath exports `./v2/burnPreview`, `./v2/evmReads`, `./v2/presetRowEvm` and `./v2/claimAllEvm`, each also a root
  namespace (`v2BurnPreview`, `v2EvmReads`, `v2PresetRowEvm`, `v2ClaimAllEvm`). Their names stay exported flat from the root too.

- A root entry. `import { v2Route, indexerRead } from "@floorlaunchpad/sdk"` reaches every subpath module as a namespace named after its path. Importing it runs no code.
- Guides for four readers in `docs/`: aggregator, wallet, bot and frontend.
- This changelog and the version plan below.
- New subpath exports, each also a root namespace: `./client` (`createFloorClient`), `./errors` (`FloorError`,
  `decodeProgramError`), `./config` (chain and program ids, quotes), `./generated` (the generated interface constants),
  `./v2/press` (`minOutAt`, `deadlineFrom`, the EVM first-sell permit), `./v2/receipt-tx` (`fillOf`, `paidOf`, `createdOf`,
  `burnedOf` over a landed transaction), `./fixtures` (small synthetic accounts for your own tests; the recorded prints stay
  out of the package).
- `CG_TICKERS` carries the indexer's `high` and `low` (nullable whole-unit prices, rolling 24 h).
- Claims: `teamBuyDueV2`, `holderOwedNowV2` and `claimLaunchesPerTxV2` beside `planClaimAllV2`.
- `U64_MAX`, `U128_MAX`, `toU128`, `BPS` and `RANGES_MAX` each have one definition; the old import paths still work.
- One definition each, the old import paths still work: `PROGRAM_IDS` (`generated`; `config` re-exports), the claim bits both
  chains share (`CLAIM_CREATOR`, `CLAIM_CREATOR_REF`, `CLAIM_TEAM_SLOT`, `CLAIM_TRADER_REF`, `CLAIM_RESERVED_HOLDER`,
  `CLAIM_EXTRA`, `CLAIM_TEAM_BUY`, `CLAIM_SHARE_BOUGHT`; `CLAIM_KNOWN` stays per chain), `SWEPT_*`, `MAX_NAME_LEN` /
  `MAX_SYMBOL_LEN` / `MAX_URI_LEN`, `Ctx`, `EvmCall`. (A `floorLive` planned for this release was dropped before it: read
  the floor from a `Book`'s `sqrtF`.)
  `creatorLegsV2` / `platformLegsV2` are the same functions as `creatorLegs` / `platformLegs`.
- The indexer's dev-buy body decodes to the SDK's `DevBuyRole`; its item type is `DevBuyItem` (was `DevBuy` in
  `indexer/bodies`, a second type of that name; the SDK's `DevBuy` is `v2/devbuy`'s).

## Version plan

- 0.3.x: patch fixes only. No export is removed or renamed.
- 0.4.0: the next rename or removal, each listed here with its replacement.
- 1.0.0: when the surface has gone one release with no rename. From then on a breaking change takes a major bump.

## 0.2.0

The first version of the package. It has the v2 math (bit for bit with the math crate), the quoter and route finder, Solana and EVM decoders, instruction and call builders, and the indexer readers. Private; not published.
