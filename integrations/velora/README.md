# FLOOR pools in Velora (paraswap-dex-lib)

Support for FLOOR's Uniswap v4 hook pools in `paraswap-dex-lib`'s `UniswapV4` dex, on Base (8453) and BSC (56). The
FLOOR hook plugs into the library's own hooks mechanism (`src/dex/uniswap-v4/hooks/`, the same one the Arena hook on
Avalanche uses). It is not a separate dex.

- Base: upstream `master` at `3babae6`. `upstream.patch` is `git diff` against it. It applies cleanly with `git apply`.
- Files: everything under `src/` here mirrors its upstream path.

## What it does

FLOOR's hook (`0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc` on both chains) has these permissions: beforeInitialize,
beforeSwap, afterSwap, beforeSwapReturnDelta and afterSwapReturnDelta. Its pools are dynamic-fee pools (`0x800000`) with
tick spacing 60, and the hook sets the LP fee to 2,500 pips on every swap.

- **Exact input only.** Velora's SELL side is priced. On the BUY side (exact output), the hook throws
  `ExactOutNotSupported` the way the contract reverts, so the pool returns no price.
- **Buy (quote in).** `beforeSwap` takes four fees off the quote paid in: skim, creator, holder and platform, each
  `floor(amountIn * bps / 10000)`. The skim rate is read at the pre-swap price, `skimBpsAt(dearEdge - preTick)`, then
  scaled by the cushion gap (`gapSkimBps`; on for both chains). The pool swaps `amountIn - total`.
- **Sell (token in).** The pool swaps normally. `afterSwap` then takes the same four fees off the quote paid out, with the
  skim read at the post-swap price. There is no gap skim on a sell.
- **Ticks** are in the engine frame: `marketTick(raw) = quoteIsCurrency1 ? -raw - 1 : raw`, and
  `dearEdge = floorTick - ceil(120 / spacing) * spacing`.

## Changes to the library's v4 code (kept small)

1. **`hooks/types.ts`**
   - `beforeSwap` and `afterSwap` take an optional `HookSwapContext`: the pre-swap `PoolState`, and for `afterSwap` the
     post-swap price and tick. On chain a hook reads these from the PoolManager; here there is nothing else to read them
     from.
   - `BeforeSwapDelta` is now `{ deltaSpecified, deltaUnspecified }`, which is v4's meaning. Its old `amount0` / `amount1`
     fields were used nowhere.
   - Two optional members: `getPools(blockNumber)` (a hook lists its own pools) and `swapGasCost`.
2. **`contract-math/uniswap-v4-pool-math.ts`**
   - `queryOutputs` now follows `PoolManager.swap`. It calls `beforeSwap` and applies the specified delta (only when
     `beforeSwapReturnDelta` is set). It applies the returned LP-fee override on dynamic-fee pools. Then it swaps, and
     `afterSwap` receives the post-swap tick. Hookless pools and the Arena hook take the same path as before.
   - `_swap` is now a wrapper around `_swapWithResult`, which also returns the final price and tick.
   - `swapFromEvent` replays a `Swap` event with the event's own `fee`, not slot0's `lpFee`. On a dynamic-fee pool,
     slot0's fee is not the fee the hook charged, so `feeGrowthGlobal` used to drift. The price, tick and liquidity were
     right either way.
3. **`uniswap-v4-pool-manager.ts`**
   - Pools from hooks that list their own are merged into the pool list. This includes networks with a static list (Base
     has one), and the subgraph and cache paths.
   - A pool from such a hook passes the static-list check on `Initialize`.
   - A dynamic-fee pool of a supported hook is not counted as having an "unconventional fee". Base sets
     `skipPoolsWithUnconventionalFees`, and `8388608 % 100 != 0`, so FLOOR pools would otherwise be dropped there.
4. **`uniswap-v4.ts`**
   - `gasCost` uses the hook's `swapGasCost` when it has one. FLOOR's is 350,000, against V4Quoter estimates of 283k to
     354k on BSC.
   - `getTopPoolsForToken` adds hook-listed pool ids to the static list's `id_in` filter.
5. **`config.ts`**: `supportedHooks: [FloorHook]` on Base and BSC.

## The FLOOR hook (`src/dex/uniswap-v4/hooks/floor/`)

- **`floor-hook-math.ts`**: `marketTick`, `bandTicks`, `dearEdge`, `skimBpsAt`, `gapSkimBps` and `takeFees`. These are
  integer ports of `FloorHookMath.skimBps`, `SingletonFloorEngine._gapSkim` and `_takeFees`.
- **`floor-hook.ts`**: `IBaseHook`. `getPools` reads the FloorRegistry pages, takes the launches on this hook, gets each
  pool id from `hook.poolForAccount(launch)` and the pool key from `poolState(id)`. It keeps only bound pools whose key
  hashes to the id. The registry's own `poolId` field is zero on the live BSC launch, so it is not used.
- **`floor-hook-state.ts`**: a `StatefulEventSubscriber` subscribed to the hook and the PoolManager. Per pool it holds the
  rates, `floorTick`, `quoteIsCurrency1` and tick spacing from `poolState(id)`, plus `cushionLower` and `cushionLiq` from
  `execute(id, stairs())`.
  - **What triggers a re-read.** It re-reads a pool at a block on two things. The first is any hook event keyed by that
    pool that can move this state: PoolRegistered, Ratcheted, BidReplaced, Seeded, LadderSeeded, CushionPlaced,
    CushionParked, StairsPlaced, StairsCollected, VaultTokensFed and ReserveSplit. The second is any PoolManager
    `ModifyLiquidity` on that pool. Every bid, cushion and staircase move is one.
  - **Failed reads.** A pool that could not be read is removed from the state (so it gets no price) and retried on the
    next block.
- **The pool's v4 state stays with the library's `UniswapV4Pool`.** It applies Swap and ModifyLiquidity from the
  PoolManager. The bid re-place after a sell into the band is a ModifyLiquidity, plus the hook's own Swap, in the same
  block, so it is replayed from events like any other liquidity change.

## How it was tested

| Test | Result |
|---|---|
| `floor-hook.test.ts` (unit, no RPC) | **40 / 40 pass** |
| `floor-hook-events.test.ts` (RPC: BSC archive) | **17 / 17 pass** |
| `floor-hook-integration.test.ts` (RPC), BSC | **3 / 3 pass** |
| `floor-hook-integration.test.ts` (RPC), Base | **3 / 3 pass** (Base's first launch) |
| `pnpm tsc` (whole repo), `eslint` (all of `src`), `prettier --check` (touched files) | clean |

**`floor-hook.test.ts`**
- It replays every one of the 310 V4Quoter answers in the five fixtures through `uniswapV4PoolMath.queryOutputs` plus
  the hook, all wei-exact. Three fixtures come from a fork of Base and cover a native quote and USDC on either side,
  across six pool states each; two are the live graduated pools on Base and BSC, read at one block, 20 sizes each.
  `amountOut: null` (the quoter reverted) is expected as 0.
- Unit tests cover the skim ramp (buy and sell, both slopes, the flat sell), the gap skim (no cushion, at or under
  2,232, the linear ramp, the full 1.5× from 5,109, the 1,500 cap), per-fee rounding, the engine frame, the LP-fee
  override and delta, exact-out refusal, and refusal with no hook state.
- It also covers the state subscriber's trigger rules (hook event, ModifyLiquidity, ignored Swap / untracked pool, a
  failed read dropped then retried).
- Mutation checks:
  - turning the gap skim off fails 6 cases;
  - charging a sell's skim at the pre-swap tick fails 13 cases.

**`floor-hook-events.test.ts`** (run against BSC through public RPCs, with logs prefetched into the harness cache)
- The hook-state subscriber, at real blocks: the pool's registration, graduation, three ratchets, two swap blocks and an
  unrelated hook event.
- `UniswapV4Pool` on the live FLOOR pool: six real swaps (3 buys, 3 sells) and three ratchet blocks.
- The six swap cases failed on `feeGrowthGlobal` before the `swapFromEvent` fix, and pass after it.
- With the refresh trigger disabled, the registration and graduation cases fail.
- Regression check for the `swapFromEvent` change, on mainnet ETH/USDC (a pool with a protocol fee): four `Swap` blocks
  through `UniswapV4Pool` pass. That was a throwaway test and is not included. The repo's own
  `uniswap-v4-events.test.ts` does not compile on `master`: its `UniswapV4Pool` constructor calls pass 14 arguments, not
  12–13.

**`floor-hook-integration.test.ts`** (BSC, run twice at different states of the live pool `0x5259…89ff`)
- It finds the pool through the registry. The subgraph was unavailable without a TheGraph key, so this also exercised the
  fallback.
- It prices 1e12…1e18 both ways from local state, and the results equal the on-chain V4Quoter.
- Exact output returns no price.

## Blockers for a real PR

- **Base has no FLOOR pool yet.** The Base half is covered only by the forked-Base fixtures, not by a live pool.
- **No e2e test.** The repo's e2e tests need FLOOR tokens in `tests/constants-e2e.ts` and a funded run through Augustus
  and the UniversalRouter. Execution is not tested: the calldata is the library's unchanged `exactInputSingle` with
  `hookData: '0x'`.
- **Pinned event blocks.** The events test pins BSC blocks from one young pool (`127476136`–`127478882`). A reviewer will
  want more history, and above all a sell that reaches the bid band (`BidReplaced`), which has not happened on chain yet.
- **Upstream API changes for review.**
  - The `BeforeSwapDelta` field rename.
  - The new optional `IBaseHook` members.
  - The `swapFromEvent` fee change, which applies to all v4 pools.
  - Static-list bypass for hook-listed pools on Base. The Velora maintainers may prefer to keep Base's list curated.
- **Discovery cost.** `getPools` reads one registry page per 500 launches, then one `poolForAccount` and one `poolState`
  per launch, on every `initializePricing` and on Base's `getTopPoolsForToken`. That is cheap now and grows with the
  registry.

## Unverified

- **The `BidReplaced` path.** No sell on chain has reached the band, so the re-place is untested against real logs. That
  covers the hook-state re-read and the `UniswapV4Pool` replay of the hook's own Swap plus ModifyLiquidity. The fixtures'
  "after a deep sell" state prices correctly, but that is a snapshot, not a replay.
- **The re-read trigger list.** The list of hook events that move `floorTick` or the cushion was taken from the
  contracts' emit sites. The trigger also fires on every PoolManager `ModifyLiquidity`, which covers any cushion or bid
  change made without one of those events (e.g. `_takeCushion`).
- **The 64-step walk cap.** The library caps the swap walk at 64 steps (`MAX_PRICING_COMPUTATION_STEPS_ALLOWED`); past
  it, it answers 0. No fixture or live quote hit it. A very large swap across sparse words could come back as "no price"
  instead of a quote.
- **Live state reads only ±12 bitmap words around the current tick** (the library's default). Positions further out (the
  full-range seed) count through active liquidity but are not crossed. That is right unless a single swap walks past
  about 184k ticks.
- **`swapGasCost`** is from four V4Quoter estimates on one BSC pool. A sell that triggers the bid re-place costs more.
- **`getTopPoolsForToken` on Base** depends on the Uniswap subgraph indexing FLOOR pools above its TVL filter.
