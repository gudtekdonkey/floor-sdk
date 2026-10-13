## feat(uniswap-v4): FLOOR hook pools (Base, BSC)

### What FLOOR is

[FLOOR](https://floor.supply) is a token launchpad. Each launch sells at one fixed price until it sells out, then
graduates into a Uniswap v4 pool run by FLOOR's hook, `SingletonFloorHook` at
`0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc` (the same address on Base and BSC). The raise becomes a protocol-owned bid
under the price (the floor), and the floor only moves up.

Every chain, contract, live pool, sample transaction and event in one page:
https://github.com/gudtekdonkey/floor-sdk/blob/main/docs/integration-facts.md

This adds the FLOOR hook to the `UniswapV4` dex through the library's own hooks mechanism (`src/dex/uniswap-v4/hooks/`,
as the Arena hook does on Avalanche). It is not a separate dex.

### Pricing

- Pools are dynamic-fee (`0x800000`), tick spacing 60; the hook sets the LP fee to 2,500 pips on every swap.
- **Exact input only.** SELL is priced; on BUY (exact output) the hook throws `ExactOutNotSupported`, as the contract
  reverts, so the pool returns no price.
- **Buy** (quote in): `beforeSwap` takes four fees off the quote paid in, each `floor(amountIn * bps / 10000)`: the skim
  (read at the pre-swap price, scaled up while the price stands far above the pool's cushion), the creator fee, the
  holder fee and the FLOOR Platform fee. The pool swaps the rest.
- **Sell** (token in): the pool swaps; `afterSwap` takes the same four fees off the quote paid out, the skim read at the
  post-swap price.
- Pools are found from the FloorRegistry (`page`), each pool id from `hook.poolForAccount(launch)`; per-pool state
  (`poolState(id)`, `execute(id, stairs())`) is an event subscriber re-read on the hook's events and the pool's
  `ModifyLiquidity`.

### Changes to the library's v4 code (kept small)

- `hooks/types.ts`: `beforeSwap` / `afterSwap` take an optional context (the pre-swap state; for `afterSwap` the
  post-swap price and tick). `BeforeSwapDelta` is `{ deltaSpecified, deltaUnspecified }`, v4's meaning (its old fields
  were unused). Two optional hook members: `getPools(blockNumber)` and `swapGasCost`.
- `contract-math/uniswap-v4-pool-math.ts`: `queryOutputs` follows `PoolManager.swap`: `beforeSwap` and its specified
  delta (only with `beforeSwapReturnDelta`), the returned LP-fee override on dynamic-fee pools, the swap, then
  `afterSwap` with the post-swap tick. Hookless pools and the Arena hook take the same path as before. `swapFromEvent`
  replays a `Swap` with the event's own `fee` (slot0's `lpFee` is not what a dynamic-fee hook charged).
- `uniswap-v4-pool-manager.ts`: hook-listed pools are merged into the pool list (static list, subgraph and cache
  paths); a dynamic-fee pool of a supported hook is not dropped as an "unconventional fee" on Base.
- `uniswap-v4.ts`: `gasCost` uses the hook's `swapGasCost` (FLOOR: 350,000; V4Quoter estimates 283k–354k on BSC);
  `getTopPoolsForToken` adds hook-listed pool ids.
- `config.ts`: `supportedHooks: [FloorHook]` on Base and BSC.

### Evidence

- `floor-hook.test.ts` (unit, no RPC): 40 / 40. It replays 310 V4Quoter answers through `uniswapV4PoolMath.queryOutputs`
  plus the hook, all wei-exact: the live graduated pools on Base and BSC (pool id
  `0x5259c52ce22be9d859d73e8b29bed5d316053fe4b9afab637d986812041d89ff`), and three launches on a fork of Base (a native
  quote and USDC on either side, six pool states each).
- `floor-hook-integration.test.ts` (RPC): passes on Base and BSC.
- `floor-hook-events.test.ts` (RPC, BSC archive): 17 / 17.
- `pnpm check:tsc`, ESLint and Prettier on the touched files: clean.

Sample transactions on the live pools: Base `0x142e70249af8b890bb4fe20019a8c09a1c874574ac5440d1b8d52e637345c804`,
BSC `0x77a22e67aa805692bcfbaf9d78af3c67595ae1bff0a7afbc4bd31d18ba337bae`.

Contracts (both chains): SingletonFloorHook `0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc`, FloorRegistry
`0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804`. PoolManager: Base `0x498581fF718922c3f8e6A244956aF099B2652b2b`, BSC
`0x28e2Ea090877bF75740558f6BFB36A5ffeE9e9dF`.

The same code is kept in https://github.com/gudtekdonkey/floor-sdk/tree/main/integrations/velora.
