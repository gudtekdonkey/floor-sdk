# FLOOR hook plugin for kyberswap-dex-lib

A `uniswap-v4` hook plugin that lets KyberSwap's router price and route through FLOOR pools: Uniswap v4 pools whose
hook is `SingletonFloorHook` at `0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc` (the same address on Ethereum, BSC,
Base, Robinhood Chain and Arc).

It is written against [KyberNetwork/kyberswap-dex-lib](https://github.com/KyberNetwork/kyberswap-dex-lib) at
`0df5a2b` (the `uniswap-v4-hyfiprop` merge). Its shape follows the merged FableHook (`hooks/fablecurve`, #1767) and
NAVJIT (`hooks/navjit`, #1740) plugins.

## What is here

| This folder | Upstream path |
|---|---|
| `pkg/liquidity-source/uniswap/v4/hooks/floor/` | the same path: the plugin, its ABIs, tests and fixtures |
| `upstream.patch` | `git diff` of the whole change against `0df5a2b`, plugin included (`git apply upstream.patch`) |

The patch also carries changes outside the plugin's folder:

- `pkg/valueobject/exchange.go`: `ExchangeUniswapV4Floor = "uniswap-v4-floor"`.
- `pkg/msgpack/register_pool_types.gen.go`: regenerated (`go generate ./pkg/msgpack/...`), registers `floor.Hook`.
- `pkg/liquidity-source/uniswap/v4/hooks.go`, `pool_simulator.go`: `AfterSwapParams.PoolSwapInfo`, the pool's own
  post-swap state (price, tick, liquidity, unspent input). The hook needs the post-swap tick: a sell's skim is charged
  at the post-swap price. Every existing hook ignores the new field.
- `pkg/liquidity-source/uniswap/v3/math.go` (+ a case in `math_test.go`): `NewPool` accepted `sqrtPrice` only strictly
  below `sqrtRatioAtTick(tick+1)`. A zeroForOne swap that stops exactly on an initialised tick T leaves
  `sqrtPrice == sqrtRatioAtTick(T)` with `tick = T-1`, a valid v3/v4 state, and the vendored
  `uniswapv3-sdk-uint256` (`entities/pool.go`) accepts it. The bound is now inclusive, as in the SDK. FLOOR pools rest
  in exactly this state after the hook's own price nudges; without the fix those pools cannot be simulated at all
  (three of the eighteen fixture states). This affects every v3/v4 source and may be best sent as its own PR.

## How it works

- **Registration.** `uniswapv4.RegisterHooksFactory(..., HookAddresses...)`. Hooks are keyed by address alone, so the
  one address serves all five chains. All five chain ids already exist in `valueobject` and in `uniswapv4.PoolManager`.
- **Tracking** (`Hook.Track`, one multicall pinned to the tracked block): `hook.poolState(poolId)` (rates, floor tick,
  orientation, tick spacing, whether the pool is bound), `hook.execute(poolId, stairs())` (the cushion's top and
  whether it stands) and `StateView.getSlot0(poolId)` (the PoolManager's protocol fee). A pool that has not graduated
  is kept untracked and is not priced. All of this state changes only in transactions that emit PoolManager events
  for the pool, so the library's event-driven refresh keeps it current.
- **BeforeSwap.** Exact input only (exact output reverts on chain; `CalcAmountIn` is refused). The LP fee is
  overridden to 2,500 pips, composed with the protocol fee as v4 does. On a buy the four takes (skim, creator, holder,
  platform; each `floor(amountIn * bps / 10,000)`) are returned as the specified delta, with the skim from the ramp
  at the pre-swap engine tick, scaled by the gap buy skim.
- **AfterSwap.** On a sell the four takes come off the quote output, the skim at the post-swap engine tick. A swap the
  pool cannot fill whole is refused (the V4Quoter refuses it too, and a buy has already paid fees on its whole input).
- **The bid re-place.** A sell that ends in the bid band (engine tick at or past `floorTick - 120`) makes the hook
  re-place its bid inside that swap. That swap's quote is exact, but the pool afterwards is not what the tick math
  says, so once such a swap is applied with `UpdateBalance` the simulator refuses every further quote on the pool
  (`ErrBidReplacePending`) until the tracker refreshes it. The re-place emits `ModifyLiquidity` for the pool, which
  triggers that refresh.
- **The pre-swap tick** is read from the pool's own extra when the hook is built and moved by `UpdateBalance`, so a
  route's second swap on a pool sees the first one's price.

## How it was tested

`go test ./pkg/liquidity-source/uniswap/v4/hooks/floor/` (go 1.25.10, the module's toolchain), `go vet` clean.

- `quoter_match_test.go` replays FLOOR's recorded fixtures (from `packages/sdk-evm/test/fixtures/simulate/`), each
  with the V4Quoter's answer: three Cook launches on a fork of Base (native ETH at currency0, USDC at currency0, USDC
  at currency1), six pool states each, 15 sizes per state; and the live graduated pool on each of Ethereum, BSC,
  Robinhood Chain, Arc and Base, read at one block, 20 sizes each. Every quote goes through
  `uniswapv4.NewPoolSimulator` with this hook registered, so it tests the integration, not a separate
  re-implementation. Of 370 quotes, 318 match the quoter to the wei, the 3 the quoter reverted are refused, 47 are
  dust the quoter answers with 0 (a zero output is an error in the library, as for every source), and 2 are Arc buys
  smaller than the library's unit for Arc's native USDC (it names native USDC by its 6-decimal wrapped entry and
  scales by 1e12 inside the v4 simulator, so the test gives and compares Arc's native amounts in that unit).
- The two buy steps between recorded states (a 3 ETH buy, then a 12 ETH buy after a ratchet) are applied with
  `UpdateBalance`: the simulator lands on the next state's exact price, tick and liquidity and quotes all its sizes
  exactly.
- A sell that reaches the band is exact and stops the pool quoting; one above the band does not.
- `math_test.go` pins the skim ramp, the gap buy skim, the takes and the fee composition to the TypeScript SDK's
  `fees.ts` test vectors; `hook_test.go` covers registration and the hook's permission bits, the refusals, the gas,
  and the Track decode path (answers ABI-encoded in the contracts' shapes, decoded as ethrpc decodes them).
- Mutation check: dropping the gap skim, charging the sell skim at the pre-swap tick, dropping the sell fee, or not
  moving the tick in `UpdateBalance` each fails the fixture replay. Shifting the currency1 frame by one tick does not
  (one tick rarely moves a whole bps); the unit test pins it.
- The v3, v4 and msgpack packages pass with `CI=1` (41 packages). Without `CI`, navjit's live RPC tests fail on
  live-market tolerances, unrelated to this change.

## Before a real PR

- **Gas is estimated, not measured.** `gasBeforeSwap` 60k, `gasTakeFees` 200k and `gasBidReplace` 350k on top of
  `uniswapv4.DefaultGas` come from the hook suite's 0.1 ETH pool buy (387,230 gas in all) less the v4 base gas; the
  bid re-place has no measured figure. Measure through the V4Quoter's gas estimate on a live pool.
- **No live test.** `Track`'s RPC wiring is untested against a node: no graduated FLOOR pool on a mainnet was
  available. A live test in the `fablecurve/hook_live_test.go` style needs one pool id per chain.
- **Pool discovery is the platform's.** The plugin only prices pools the uniswap-v4 lister finds. Robinhood Chain and
  Arc have no hosted v4 subgraph, so they need the on-chain `Initialize` path with StateView ticks
  (`FetchTickFromStateView`); `uniswapv4.Quoter` has no Robinhood entry (this plugin does not use it). Arc's native
  USDC uses the library's existing 18 / 6 decimal rescaling, which the fixtures (all on Base) do not exercise.
- **Untested orientations and settings.** No fixture has a non-zero protocol fee or a holder rate; both are handled
  and unit-tested only. The gap buy skim is always on: every FLOOR set at this address carries it.
- `poolState` is decoded positionally (57 words, 53 fields). A hook redeploy that changes `PoolState` needs the
  struct and the ABI updated; the hook address would change too, so the old registration would simply stop matching.
