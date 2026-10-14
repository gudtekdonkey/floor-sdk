# FLOOR: facts for integrators

Everything an aggregator, indexer or data site needs to list FLOOR, in one place. Every address and transaction here is
live on mainnet; the figures were read from the chains on 2026-10-13.

- Site: https://floor.supply · Technical write-up: https://floor.supply/technical · X: https://x.com/floordotsupply
- SDKs: this repo (`@floorsupply/sdk`, `@floorsupply/sdk-evm`, `@floorsupply/sdk-solana`) and
  [carpenter-sdk](https://github.com/gudtekdonkey/carpenter-sdk) (`@floorsupply/carpenter-sdk`, Carpenter's AMM on Solana).

## What FLOOR is

A token launchpad. A launch sells at one fixed price on its own sale contract until it sells out, then graduates into a
pool: on the EVM chains a Uniswap v4 pool run by FLOOR's hook, on Solana a Carpenter pool run by FLOOR's hook program. The
raise becomes a protocol-owned bid under the price (the floor), and the floor only moves up. Every launch has a supply of
1,000,000,000 tokens (18 decimals on EVM).

## Where it runs

| Chain | Chain id | FLOOR's public DEX endpoint key | Explorer |
|---|---|---|---|
| Ethereum | 1 | `eth` | https://etherscan.io |
| Base | 8453 | `base` | https://basescan.org |
| BSC | 56 | `bsc` | https://bscscan.com |
| Robinhood Chain | 4663 | `rh` | — |
| Arc | 5042 | `arc` | https://explorer.arc.io |
| Solana | mainnet-beta | `sol` | https://solscan.io |

## Live pools

One launch per EVM chain sits at the same addresses on every chain (the factory deploys every launch at a deterministic address ending `…B1D`), but it is a different token on each: Soft Blue (SBLUE) on Ethereum, Blue (BLUE) on Base, Yellow (YELLOW) on BSC, Green (GREEN) on Robinhood Chain, Vivid Blue (VBLUE) on Arc.

| | |
|---|---|
| Token | `0x7935f6EcC173913aC06186d2892BfA73e2FBdB1D` (18 decimals; name and symbol per chain, above) |
| Sale (FloorLaunch) | `0xc0A603ad69e4487e9Cb557Aab87F4Cc82a58f2Ea` |
| Pool id (Uniswap v4) | `0x5259c52ce22be9d859d73e8b29bed5d316053fe4b9afab637d986812041d89ff` |
| Quote | the chain's native asset (`0x000…000` at currency0; on Arc, native USDC, 18 decimals) |

| Chain | Pool created (graduation) | A trade on the pool |
|---|---|---|
| Ethereum | `0x7175e3dfdbb23be998cb488c3b7df593861d6d4c4325c49b9e8133f1a207e4a7` | `0x16246ee291f8cc003d49b6ea5ce6b62ead121749c1d04f229a8d7cadbc13a24a` |
| Base | `0x21999f26f37bcdab993bae8ede58101742c7d51eb8de4338686dfc43f966d391` | `0x142e70249af8b890bb4fe20019a8c09a1c874574ac5440d1b8d52e637345c804` |
| BSC | `0x8d63210f9ae26c49f713a5b7ae350cdd46f9d1388e311a3df4f8354da0a6a798` | `0x77a22e67aa805692bcfbaf9d78af3c67595ae1bff0a7afbc4bd31d18ba337bae` |
| Robinhood Chain | `0x71f15ee29a424f95004d55ba6dbe2f8261172488abc7fbbaf40a02b308021d35` | `0x8224eb28c47c3be44867637765408b8f3e5f6684288d3bf19bd947a2cd2e89f7` |
| Arc | `0xcd99ca591c7c5ede9f6504f6e656ea89cee5952217e36734fc29182fd05aa29b` | `0x7e15385a8bbdf91ee3e008f8d6ae1a21a2f1b289c30dca3a2b3080e809ab1c5f` |

Solana: the `floor_launch` registry lists two launches. The graduated one is launch
`BQ8BKPW2RNmTjwnpL6zFKVDNygPZzMS54U9YEhrSQwWN`, mint `8SpNeQW8Eszw91J5cPMbrmDbPUnsywRVXQmwBSoasB1D`, quoted in wSOL,
trading on the Carpenter pool `FzNAs2cCbeKVhD4ckoSbFhgeFL8Jo3hoRQ35QoxqVcRA` (quote vault
`7yHRxN53NafFezUdinvRiARrn1349kiBqpwzdVjuMh7v`). `$FLOOR`, the platform's own token, is the Solana mint
`5qhVgzoXcy8LWzwfZAWJWNEwZCVqfeJFK6MLhAWHUB1D`.

## EVM contracts

FLOOR's contracts sit at the same address on Ethereum, Base, BSC, Robinhood Chain and Arc (deployment set `0x…1a`):

| Contract | Address |
|---|---|
| SingletonFloorHook (the Uniswap v4 hook) | `0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc` |
| FloorRegistry (every launch) | `0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804` |
| FloorSingletonFactory | `0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F` |
| FloorUpgradeTimelock (governance) | `0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E` |
| Platform treasury | `0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c` |
| Team treasury | `0x062B6c421E37579728877337B47Cf528AE4d49bb` |
| FloorHolderRewards | `0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD` |
| FloorRecipientClaims | `0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40` |
| FloorTraderReferrals | `0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE` |
| FloorPresetRegistry | `0x2d5b6582C0EA925be1e356F7ECE585238d911505` |
| FloorVesting | `0xa826B75Ca94206C51De45c8112c5ce8541e357A3` |
| FloorSwapRatchet (not on Ethereum) | `0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371` |
| FloorZap | `0x0117F2686F195eb7b85C27C96EEFDC8040079d9D` |

Uniswap v4 on each chain (FLOOR uses Uniswap's own deployments):

| Chain | PoolManager | StateView | V4Quoter | UniversalRouter (2.1.1) |
|---|---|---|---|---|
| Ethereum | `0x000000000004444c5dc75cB358380D2e3dE08A90` | `0x7fFE42C4a5DEeA5b0feC41C94C136Cf115597227` | `0x52F0E24D1c21C8A0cB1e5a5dD6198556BD9E1203` | `0x4C82D1fBFe28C977cBB58D8C7FF8FCF9F70a2cCA` |
| Base | `0x498581fF718922c3f8e6A244956aF099B2652b2b` | `0xA3c0c9b65baD0b08107Aa264b0f3dB444b867A71` | `0x0d5e0F971ED27FBfF6c2837bf31316121532048D` | `0xFdf682F51FE81Aa4898F0AE2163d8A55c127fbC7` |
| BSC | `0x28e2Ea090877bF75740558f6BFB36A5ffeE9e9dF` | `0xd13Dd3D6E93f276FAfc9Db9E6BB47C1180aeE0c4` | `0x9F75dD27D6664c475B90e105573E550ff69437B0` | `0x8B844f885672f333Bc0042cB669255f93a4C1E6b` |
| Robinhood Chain | `0x8366a39CC670B4001A1121B8F6A443A643e40951` | `0xF3334192D15450CdD385c8B70e03f9A6bD9E673b` | `0x8Dc178eFB8111BB0973Dd9d722ebeFF267c98F94` | `0x8876789976dEcBfCbBbe364623C63652db8C0904` |
| Arc | `0x8366a39CC670B4001A1121B8F6A443A643e40951` | `0xF3334192D15450CdD385c8B70e03f9A6bD9E673b` | `0x8Dc178eFB8111BB0973Dd9d722ebeFF267c98F94` | `0x4fcA4a51Ab4F23A7447b3284fBd7D73289A89Fb1` |

Permit2 is `0x000000000022D473030F116dDEE9F6B43aC78BA3` everywhere.

## The pools

- **Pool key**: `currency0` / `currency1` sorted by address (a native quote is always `currency0` = `0x0`), `fee`
  `0x800000` (dynamic), `tickSpacing` 60, `hooks` = the hook above. The hook sets each swap's LP fee (`Swap.fee`).
- **Hook permissions**: `beforeInitialize`, `beforeSwap`, `afterSwap`, `beforeSwapReturnDelta`, `afterSwapReturnDelta`.
  Fees are taken from the quote through the return deltas. **Exact input only**: an exact-output swap reverts
  (`ExactOutNotSupported`).
- **The take on a trade**, all on the quote leg: the skim (ramps from its far rate down to 0 at the floor on buys, and up
  to a multiple of it at the floor on sells), the creator fee, the holder fee and the FLOOR Platform fee. Each pool copies
  its rates at creation and keeps them; read them from the hook's `poolState(poolId)`. The live pools: skim 500 bps far
  (1,500 bps on a sell at the floor), holder 500, creator 25 (0 on BSC), FLOOR Platform fee 25. On a buy the take comes
  off the quote paid in, at the pre-swap price; on a sell off the quote paid out, at the post-swap price.
- **Upgradeability**: the hook and its modules are immutable. Each pool's custody vault is a beacon proxy behind the
  upgrade timelock.
- **Every position in a FLOOR pool is FLOOR's own** (the protocol-owned liquidity, the bid, the cushion, the staircase's
  steps). The hook does not gate adding liquidity, so a third party could add some; none has.

## Finding launches and pools

1. `FloorRegistry.count()`, then `page(start, n)`: `(launch, token, hook, rewards, poolId, mode, at, registrar)[]`.
2. A launch is selling until `FloorLaunch(launch).graduated()`; its pool id is then `hook.poolForAccount(launch)`
   (zero before graduation).
3. `hook.poolState(poolId)`: the whole record of one pool (quote, orientation, rates, floor tick, custody vault, fee
   counters).
4. Events: every hook event carries the indexed `poolId`; trades are the PoolManager's `Swap` for the pool id, and the
   hook's `HookFee` (the Uniswap Foundation hook data standard) gives its take, so trader net = `Swap` ± `HookFee`. The
   full map from events to launches, pools, trades, the floor and candles: [`docs/evm-events.md`](evm-events.md).

## Solana programs

| | Address |
|---|---|
| carpenter_amm | `DdeiDx4ncHo9DXGZm1GrPe7mFKv7kNU3yx95uQEV2APA` |
| floor_hook | `9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy` |
| floor_launch | `8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi` |
| floor_holders | `2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te` |
| floor_vesting | `HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC` |
| LaunchConfig (registry count) | `Ck9wU4tSb4dLtaW4PyEfg8NYrRJkCS6daBVu13cxtVQz` |
| Carpenter AmmConfig | `85S8wiRLyv9Urc6g9tF8gDC56F5Fq4JBu9Zub3BEtyuV` |
| HookConfig | `CfhjSM2zTcxnKggQvHjUzYPTc6W9QxC2cABJnCHq4Cxx` |
| Platform treasury (`["platform_treasury"]` under `floor_hook`) | `B8SRexSGm2p2nJFpMmVQPB9PepHSXeB3oQMzyaqrsGvC` |
| Team treasury (`["team_treasury"]` under `floor_hook`) | `CqQXgHT6MsUmEaXgwuUZURwRvfF7w5wkcD1BzWNt7vi9` |
| Carpenter protocol treasury (under `carpenter_amm`) | `G7URMTZpyVendaTXEki7emtvvB1UirZFfdvruYy8h66r` |
| Address lookup table | `GLu7tLCby32SPS43p9wdHW7EbV7ZoaEfyeCWx6hkxAr3` |

Each treasury account owns the token accounts its fees are swept into; DefiLlama's treasury adapters read them.

A launch is `["entry", u64le(i)]` under `floor_launch` for i < `LaunchConfig.registry_count`. Account layouts, PDAs and
the events (`emit_cpi`, decoded from the instruction data) are in `@floorsupply/sdk-solana` and
`@floorsupply/carpenter-sdk`.

## FLOOR's public DEX endpoint

GeckoTerminal's Integration API Standard (DEX Screener reads the same shape), served by FLOOR for every chain key above:

- `https://floor.supply/api/dex/{chain}/latest-block`
- `https://floor.supply/api/dex/{chain}/asset?id={token}` — name, symbol, decimals, total and circulating supply, logo
- `https://floor.supply/api/dex/{chain}/pair?id={poolId | token | sale}` — the pool (or the sale while it sells)
- `https://floor.supply/api/dex/{chain}/events?fromBlock=&toBlock=` — every swap, join and exit, with fees in `metadata`

Example: https://floor.supply/api/dex/base/pair?id=0x5259c52ce22be9d859d73e8b29bed5d316053fe4b9afab637d986812041d89ff

The same pair model and event mapping, as code: `@floorsupply/sdk`'s `dex` namespace.

## Quoting off-chain

`@floorsupply/sdk-evm` has `readSwapSnapshot(client, chainId, poolId)` and `simulateSwap(snapshot, side, amountIn)`:
Uniswap v4's swap ported from v4-core with the hook's fees around it. Checked against the V4Quoter:

- 100 of 100 quotes exact on the live pools of Ethereum, BSC, Robinhood Chain, Arc and Base, at one block each
  (`packages/sdk-evm/scripts/live-parity.mts`, fixtures in `packages/sdk-evm/test/fixtures/simulate/live-*.json`).
- 270 of 270 exact on pools created on a fork of Base: a native quote and USDC on either side of the pool, at six states
  each (`scripts/fork-parity.mts`).

## Integrations

| Where | What | Code |
|---|---|---|
| DefiLlama | TVL, treasury | [`integrations/defillama/DefiLlama-Adapters`](../integrations/defillama/DefiLlama-Adapters) |
| DefiLlama | Volume, fees, revenue (EVM; Solana through Dune) | [`integrations/defillama/dimension-adapters`](../integrations/defillama/dimension-adapters) |
| KyberSwap | The FLOOR hook for `kyberswap-dex-lib` | [`integrations/kyberswap`](../integrations/kyberswap) |
| Velora | FLOOR hook pools in `paraswap-dex-lib` (Base, BSC) | [`integrations/velora`](../integrations/velora) |
| Jupiter | Carpenter pools, FLOOR's hook included; FLOOR's bonding sales (`floor-sale-amm`) | [carpenter-sdk `jupiter/`](https://github.com/gudtekdonkey/carpenter-sdk/tree/main/jupiter), [`jupiter/`](../jupiter) |
| GeckoTerminal, DEX Screener | FLOOR's public DEX endpoint | above |

The DefiLlama adapters were checked against each pool's own on-chain counters in `poolState`: pool volume equals
`cumulativeVolumeEth`; protocol revenue equals `teamFeesOwed + teamFeesPaid + platformPaidEth`; the creator and referral
fees equal their owed + paid counters; and TVL's pool part equals the hook's own positions, with the custody vault's
balance equal to `vaultEth` plus every owed fee. All to the wei, on all five EVM chains.
