## feat(uniswap-v4): add FLOOR hook (uniswap-v4-floor)

### What FLOOR is

[FLOOR](https://floor.supply) is a token launchpad. Each launch sells at one fixed price until it sells out, then
graduates into a Uniswap v4 pool run by FLOOR's hook, `SingletonFloorHook` at
`0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc`, the same address on Ethereum, BSC, Base, Robinhood Chain and Arc. The raise
becomes a protocol-owned bid under the price (the floor), and the floor only moves up.

Every chain, contract, live pool, sample transaction and event in one page:
https://github.com/gudtekdonkey/floor-sdk/blob/main/docs/integration-facts.md

### Pricing

- Pool key: native or ERC-20 quote, `fee` `0x800000` (dynamic), `tickSpacing` 60. The hook overrides the LP fee to 2,500
  pips on every swap.
- Hook permissions: `beforeInitialize`, `beforeSwap`, `afterSwap`, `beforeSwapReturnDelta`, `afterSwapReturnDelta`.
  **Exact input only**: exact output reverts on chain (`ExactOutNotSupported`), so `CalcAmountIn` is refused.
- **Buy** (quote in): `beforeSwap` takes four fees off the quote paid in, each `floor(amountIn * bps / 10,000)`: the skim
  (a ramp read at the pre-swap price, scaled up while the price stands far above the pool's cushion), the creator fee,
  the holder fee and the FLOOR Platform fee. The pool swaps the rest.
- **Sell** (token in): the pool swaps; `afterSwap` takes the same four fees off the quote paid out, the skim read at the
  **post-swap** price.
- A sell that ends inside the bid band makes the hook re-place its bid within that swap. That swap's quote is exact; the
  simulator then stops quoting the pool until the tracker refreshes it (the re-place emits `ModifyLiquidity`).
- Each pool's rates, floor tick and cushion come from `hook.poolState(poolId)` and `hook.execute(poolId, stairs())`, read
  in one multicall pinned to the tracked block. They change only in transactions that emit PoolManager events for the
  pool.

### Changes outside the plugin's folder

- `pkg/valueobject/exchange.go`: `ExchangeUniswapV4Floor = "uniswap-v4-floor"`.
- `pkg/msgpack/register_pool_types.gen.go`: regenerated, registers `floor.Hook`.
- `pkg/liquidity-source/uniswap/v4/hooks.go`, `pool_simulator.go`: `AfterSwapParams.PoolSwapInfo`, the pool's own
  post-swap state (price, tick, liquidity, unspent input). A FLOOR sell's skim is charged at the post-swap price, so the
  hook needs it. Every existing hook ignores the new field.
- `pkg/liquidity-source/uniswap/v3/math.go` (+ a case in `math_test.go`): `NewPool` accepted `sqrtPrice` only strictly
  below `sqrtRatioAtTick(tick+1)`. A zeroForOne swap that stops exactly on an initialised tick T leaves
  `sqrtPrice == sqrtRatioAtTick(T)` with `tick = T-1`, a valid v3/v4 state that the vendored `uniswapv3-sdk-uint256`
  accepts. The bound is now inclusive, as in the SDK. FLOOR pools rest in exactly this state after the hook's own price
  moves. Happy to send this as its own PR if you prefer.

### Evidence

`go test ./pkg/liquidity-source/uniswap/v4/hooks/floor/ ./pkg/liquidity-source/uniswap/v3/ ./pkg/liquidity-source/uniswap/v4/`
pass, `go vet` clean, `go build ./...` clean, `goimports` and `go generate ./pkg/msgpack/...` leave the tree unchanged.

`quoter_match_test.go` replays recorded states with Uniswap's V4Quoter answer for each swap, through
`uniswapv4.NewPoolSimulator` with this hook registered:

- The live graduated pool on each of Ethereum, BSC, Base, Robinhood Chain and Arc (pool id
  `0x5259c52ce22be9d859d73e8b29bed5d316053fe4b9afab637d986812041d89ff` on every chain), read at one block, 20 sizes each.
- Three launches created on a fork of Base (native ETH at currency0, USDC at currency0, USDC at currency1), six pool
  states each, 15 sizes per state.

Of 370 quotes: 318 match the quoter to the wei, the 3 the quoter reverted are refused, 47 are dust the quoter answers
with 0 (a zero output is an error here), and 2 are Arc buys smaller than the library's 6-decimal unit for Arc's native
USDC. Applying the recorded buys with `UpdateBalance` lands on the next state's exact price, tick and liquidity.

Sample transactions on the live pools: Base `0x142e70249af8b890bb4fe20019a8c09a1c874574ac5440d1b8d52e637345c804`,
BSC `0x77a22e67aa805692bcfbaf9d78af3c67595ae1bff0a7afbc4bd31d18ba337bae`, Ethereum
`0x16246ee291f8cc003d49b6ea5ce6b62ead121749c1d04f229a8d7cadbc13a24a`.

### Known limits

- Gas is estimated from the hook's own test suite (a 0.1 ETH pool buy, 387,230 gas in all), not measured on a live pool.
- Robinhood Chain and Arc have no hosted v4 subgraph, so their pools need the on-chain `Initialize` path.
- No pool has a non-zero Uniswap protocol fee yet; that path is unit-tested only.

The same code is kept in https://github.com/gudtekdonkey/floor-sdk/tree/main/integrations/kyberswap.
