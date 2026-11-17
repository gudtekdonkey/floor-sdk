# Changelog

All notable changes to `@floorlaunchpad/sdk`. The version follows semver; while the major number is 0, a minor bump may rename or remove an export and a patch bump never does.

## 0.4.1

Fixed:

- `zapSwapData` appends `SWEEP(address(0), MSG_SENDER, 0)` to a native-input Universal Router swap (Uniswap, Aerodrome, Pancake and Ramses forks, V2 / V3 and v4). A Squid hook forwards the whole bridged amount as the zap's value, the plan spent only `amountIn`, and the surplus stayed on the router for anyone to sweep; it now returns to the zap, which refunds it. Token-in and native-out routes, and every non-Universal-Router family, encode the same bytes as before.
- `zapSwapData` encodes a zap whose input is an ERC-20 (`buyWithToken`) and whose output is the native coin (a launch quoted in the native coin). Before, every route began with `WRAP_ETH` (or a native-only call) and paid the router's output in its wrapped form, so an ERC-20 → native-coin zap through a Universal Router, a V3 SwapRouter, SwapRouter02 or Algebra router reverted (`QuoteShort`; measured on forks of every zap row). New optional params: `input` ("native", the default, or "token"), `output` ("token", the default, or "native": the router unwraps and pays the zap, which accepts the native coin only from its router), and `router` (a SwapRouter v1 native output is paid to the router, then unwrapped). The defaults encode the same bytes as before.
- The Universal Router forks' swap input: `urFork` ("uniswap", the default, "aerodrome", "pancake" or "ramses"; each fork row in `EVM_CHAINS` names its own as `urFork`). Aerodrome's router reads a sixth `bool isUni` (sent false: its Slipstream pools, each key `0x100000 | tickSpacing`, `AERODROME_SLIPSTREAM_KEY`); PancakeSwap's and Ramses' read five fields. Before, every fork got Uniswap's sixth `uint256[]`, which Aerodrome's router reads as `isUni = true` and routes through Uniswap's pools instead of its own. A V2 leg through a fork's router is refused (its input is its own).
- `buyHookNative` (the Squid postHook that buys through the zap with a bridged native coin) takes the launch's `quote` and, when it is an ERC-20, adds a third call that passes the zap's quote refund on to `user` (`FullTokenBalance quote.transfer(user, FULL)`). Before, the unspent quote the program paid back stayed on the multicall, where anyone could sweep it. `quote` is optional: absent or address(0) (a launch quoted in the native coin), the hook is the same two calls as 0.4.0, byte for byte, so a 0.4.0 call still compiles and encodes as before; pass the ERC-20 quote to get the refund call.
- `devBuyFromTrade` takes an optional `ctx.buyer`: the person a known zap's or router's own event proves for a buy through it. A buy through the zap or the router has the contract as the trade's `trader`, so a creator's or team member's buy through either was not named a dev buy; with `ctx.buyer` it is, and the returned `trader` is that person. The trade's recipient alone never names one (anyone can buy with the creator as recipient).
- `EVM_ERRORS` and `decodeEvmProgramError` now know five errors the v2 contracts revert with from their math libraries, which the contracts' ABI does not list: `PriceOrder()`, `ZeroLiquidity()`, `SqrtRange()`, `Zero()` and `VaultShareOutOfRange()`. Before, a revert with one of them decoded as unknown; now it decodes by its selector like any other EVM error.
- `zapSwapData` now refuses a route that names address(0) (the wrapped native stands for the native coin) or swaps a coin into itself (neither names a pool a router can trade).

Added:

- `FLOOR_ZAP_ADDRESSES`: the deployed FloorZap instance address per chain and zap slot (15 instances on 8 chains, Elysium's `uniswap-ur` included; one address per slot on every chain that has it), and `zap` on each `EVM_CHAINS` zap row (the instance to call, or `null` where that slot is not deployed). A wallet or an integrator no longer needs its own copy of the zap addresses.
- Elysium's zap: `EVM_CHAINS.elysium.zapRouters` holds Uniswap's Universal Router 2.1.2 (`0xA13f7611115752DEB2B13813BDDE16A5073e4b45`, slot `uniswap-ur`) with its deployed zap booked (`FLOOR_ZAP_ADDRESSES.elysium`, the same `uniswap-ur` address as every other chain: 15 zaps in all), and `wrappedNative` is WHYPE (`0xbA876237B0F021A6c6D852d6c26E107997cC5E81`, 18 decimals). Elysium's depth is in Uniswap v4 only (native HYPE / USDC pools), so its zap routes through the v4 route below, never through WRAP_ETH.
- `zapSwapData` encodes Uniswap v4 through the Universal Router: one pool, exact in, given as `route.v4` (`ZapV4Pool`: fee, tick spacing, hooks, hook data) with `route.tokens` = [currency in, currency out] and `V4_NATIVE` (address(0)) for the native coin. The calldata is `V4_SWAP` with `SWAP_EXACT_IN_SINGLE`, `SETTLE` (paid by the router: the zap pays an ERC-20 input to it first; a native input rides as the call's value) and `TAKE`; a native output is taken to the router and then swept to the zap (`SWEEP`, at the same minimum), since the zap accepts the native coin only from its router. New beside it: `v4PoolKey` (the sorted pool key and the swap's direction), the type `V4PoolKey`, and the constants `UR_V4_SWAP`, `V4_SWAP_EXACT_IN_SINGLE`, `V4_SETTLE`, `V4_TAKE`, `V4_OPEN_DELTA` and `V4_NATIVE`. A route without `v4` encodes as before.
- Indexer batched reads: `floor-amm/launch-lists` (`indexerPath.launchLists`, `readLaunchLists`, body `LAUNCH_LISTS`) returns one `launches?quote=` page per asked quote in one request, each list resumed with its own cursor (`LAUNCH_LISTS_FIRST` asks a first page; at most `LAUNCH_LISTS_QUOTES_MAX` quotes and `LAUNCH_LISTS_ROWS_MAX` rows); `floor-amm/candles/many` (`indexerPath.candlesMany`, `readCandlesMany`, body `CANDLES_MANY`) returns many launches' newest candles at one interval in one request (at most `CANDLES_MANY_IDS_MAX` launches, `missing` names the unknown ones). Both are in `INDEXER_CONTRACT` and the client; against an indexer that does not serve them the readers throw `IndexerError`.
- `readPresetMenuEvmV2` reads the preset rows through one Multicall3 `aggregate3` per 100 ids (1 + ⌈K ÷ 100⌉ calls instead of 1 + K); a failed row throws, never a gap in the menu. New optional third argument `{ multicall }`: another Multicall3 address, or `null` to send one `presetRow` call per id on a chain without Multicall3.
- The Solana client's `claim` reads its launches side by side instead of one after another: the round trips no longer grow with the number of launches (5 launches: 15 round trips to 3). Same transaction, same refusals; the quote's fields still come from the last launch asked.
- FloorRouter addresses: `FLOOR_ROUTER_ADDRESSES` (per chain and zap slot), `floorRouter` on each `EVM_CHAINS` zap row, `FLOOR_ROUTER_DEPLOYER` and `floorRouterAddress(slot)`. One address per slot, the same on every chain that has it (7 routers over the 15 zap rows). The addresses are derived ahead of the routers' deploy, not read back from chain: each is usable only once it has code on that chain. A call to one with no code still succeeds and buys nothing, and a native-in buy's coin sent to it stays at that empty address and is lost. `floorRouterLive` and `assertFloorRouterLive` (`evm/router`) check the code through your `getCode` ("0x" or `undefined` is no code); check before you build a call to one, as `evmRouteCallV2` itself reads no chain.

Changed:

- Elysium is switched on: its chain registry row is no longer `v2DayOne: false`, so `v2DayOneChains()` now includes Elysium (eight EVM chains). Elysium is live at the same address as the other EVM chains.

Added (to carry the fixes; nothing removed or renamed): the type `ZapUrFork`, the constant `AERODROME_SLIPSTREAM_KEY`, the optional `ZapSwapParams` keys `input`, `output`, `router` and `urFork`, and the optional `urFork` on each `EVM_CHAINS` zap row.

## 0.4.0

Removed:

- `LAUNCHES_ALL` (the body of `floor-amm/launches` with no query key). No request answers every launch any more: a bare
  `floor-amm/launches` is one page, decoded by `LAUNCHES`.

Changed:

- `INDEXER_CONTRACT.launches`: the default `limit` is `LAUNCHES_PAGE_DEFAULT` (100, was 1000); the page kind is the new
  `keyset` (`Pagination` gains `{ kind: "keyset", limit, cursor: "cursor", next: "next" }`); the keys gain `preset` and
  `cursor` (`cursor` not with `offset` or `ids`).
- `LAUNCHES` (the list body) gains `cursors` (the cursor after each row), `keys` (each row's ranked key as a decimal
  string, null in the unread tail) and `next` (the last cursor while rows remain, else null). `cursors` and `keys` are []
  with `ids` or `offset`; an indexer without them decodes with `cursors` / `keys` [] and `next` absent.
- A `string` query key's `max` bounds the raw length as sent; a `list` key may bound each item (`item`); `required` is
  refused by the table on `search` only.
- `drain("launches")` follows `next` as `cursor` (by `total` with `offset`).

- The six wallet routes page on request: `positions`, `walletLocks`, `claimHistory`, `people` and `roles` take `limit`
  (default `WALLET_PAGE_DEFAULT` 100, at most `WALLET_PAGE_MAX` 500) and `cursor` (opaque, at most `WALLET_CURSOR_MAX`
  characters) and answer `next`; `inbox` takes `before` (`""` the newest; default `INBOX_PAGE_DEFAULT` 100, at most
  `INBOX_PAGE_MAX` 500) and answers `next`. With neither key each answers every row, as before. Their decoders
  (`POSITIONS`, `WALLET_LOCKS`, `CLAIM_HISTORY`, `PEOPLE`, `ROLES`, `INBOX`) gain an optional `next`; the readers
  (`readPositions`, `readWalletLocks`, `readClaimHistory`, `readPeople`, `readRoles`, `readInbox`) take the page keys.
- Four more routes page, with a page of 100 rows when no key is sent: `floorRouted` (`limit` up to `LAUNCHES_PAGE_MAX`
  1,000, `cursor` the last launch id, create order; `pending` stays the whole per-quote sum on every page), `devBuys`
  (`limit` up to 500, `before` a block or a row id, newest first), `claimView` (`limit` up to `WALLET_PAGE_MAX` 500,
  `cursor` the last row key: `l:<launch>`, `r:<account>`, `p:<traderRef>` or `o:<quote>`; `quotes` and `totals` stay
  whole) and `handlePlaces` (`limit` up to 500, `cursor` `<launch>:<place>`). Each body (`FLOOR_ROUTED`, `DEV_BUYS`,
  `CLAIM_VIEW`, `HANDLE_PLACES`) gains an optional `next`, null on the last page; `indexerPath`, `routePath` and the readers
  (`readDevBuys`, `readClaimView`, `readHandlePlaces`) take the page keys, and `DRAIN_NAMES` gains the four.
- **Breaking:** a `DEV_BUY` row carries a required `id` (`<block>:<txnIndex>:<eventIndex>`, the `before` cursor). This
  build's decoder refuses a dev-buys body from an indexer older than it, which sends rows without `id`.
- A candle (`readCandles`, `floor-amm/candles` and the candle bundle) gains `floorX64`: the floor at the bucket's close, a
  Q64.64 price as a launch's own `floorX64`. It is absent from an indexer older than this build and null where the bucket
  has none.
- `DEX_PAIR`: the pair gains `dexKey`, `createdAtTxnId` (optional) and `creator`; the body gains an optional `launch`
  (`{ id, token }`), so a pair resolves to its launch in one read.
- **Breaking:** `roles.traderRefOf[].tier` is 1-based (1..3), as `creatorRefOf`'s; it was 0-based. A client that
  added 1 to show a tier must stop.

Added:

- Indexer: the route `GET /floor-amm/search?q=` (also under `/floor-amm/v2/`; `INDEXER_CONTRACT.search`,
  `indexerPath.search`, reader `search`, `readLaunchSearch`, decoder `LAUNCH_SEARCH`, type `LaunchSearch`): launches whose
  symbol, name or token match, best first, each row with its `tier` (0..4), keyset pages by `next`; keys `q`, `limit`
  (default `SEARCH_PAGE_DEFAULT` 12, at most `SEARCH_PAGE_MAX` 100), `cursor`, `rankable`, `graduated`, `quote`, `preset`.
- `src/indexer/search.ts`: `searchFold`, `searchNeedle`, `searchNeedles` (a query with a leading `$` has two readings,
  without and with it; its tier is the better), `searchTier`, `searchTierFolded`, `searchServed`, `searchServedTier`,
  `SEARCH_PARITY_CORPUS`, `SEARCH_Q_MAX`, `SEARCH_PAGE_DEFAULT`, `SEARCH_PAGE_MAX`, `ADDRESS_PREFIX_MIN`,
  `SEARCH_SUBSTRING_MIN`; `paths.ts`: `LAUNCHES_PAGE_DEFAULT`, `LAUNCH_CURSOR_MAX`, `PRESET_FILTER_MAX`, the `preset` and
  `cursor` keys of `LaunchesQuery`, `LaunchSearchQuery`.
- `cursorPage` / `CursorPage` (one `{ rows, next }` off any page body), `pageAsked`, `pageLimit`, the type `WalletPage`.
- Indexer (TON only): `GET /floor-amm/wallets/<wallet>/ton-activity` (`limit` ≤ 200, `before`; `INDEXER_CONTRACT.tonActivity`, `indexerPath.tonActivity`, reader `readTonActivity`, decoder `TON_ACTIVITY` / `TON_ACTIVITY_ROW`) and `GET /floor-amm/wallets/<wallet>/ton-outcome?queryId=` (`tonOutcome`, `readTonOutcome`, `TON_OUTCOME`): a TON wallet's recent FLOOR events, and one pending / landed / failed outcome with its fill by the message's query id. Other chains answer `unknown route`.
- TON (repository source only, not in the published package): `getTeamSlot` (`get_team_slot`, places decoded and ordered), `getPresetRow` (`get_preset_row`, typed result), `parseRefMap` / `decodeTonSlotPlace`, and `defaultClaimValue` exported from the barrel.
- TON (repository source only, not in the published package): the price write: `tonPriceCell`, `tonPriceDigest`, `tonSetQuotePriceBody`, `buildTonSetQuotePrice`, `tonQuotePriceFresh` (`TON_PRICE_WINDOW_S`, `TON_PRICE_AHEAD_CAP_S`, `TON_PRICE_WRITE_VALUE`, type `TonPriceArgs`), `getTonQuotePrice` (type `TonQuotePrice`); `addressItem` and `getQuote` take `null` (the GRAM quote).
- Chains: `elysium` (Elysium, chain 99901, native HYPE; Kinetiq's Arbitrum Orbit L2 on HyperEVM) in the chain registry,
  `EVM_CHAINS` and the config rows, marked `v2DayOne: false` (an added chain, not v2 day one): `v2DayOneChains()` still
  returns the seven, `FLOOR_AMM_FROM_BLOCK.elysium` is 6058368 (no code at block 6058367; the live proxy has code from 6058368), no zap routers and no wrapped native yet.
- Elysium is live: FLOOR is deployed on Elysium 99901 at the same address as the other EVM chains, and the config now lists stock quotes (`deploy/config/quotes/elysium.json`).

Fixed:

- TON `fromBoc`: a BoC whose data ends in a zero completion byte no longer loops forever, and a truncated, oversized-count or out-of-range one throws a `RangeError` (never `undefined`, never a partial cell).

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
