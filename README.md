# floor-sdk

SDKs for [FLOOR](https://floor.supply): read FLOOR launches, quote trades and build unsigned trade transactions on EVM
chains and on Solana. One TypeScript client, `FloorClient`, serves both chain families through an adapter per family.
Rust crates and a Jupiter adapter serve routers and aggregators.

On EVM chains FLOOR is a Uniswap v4 hook. On Solana it is four programs (`floor_hook`, `floor_launch`, `floor_holders`,
`floor_vesting`) on the [Carpenter](https://github.com/gudtekdonkey/carpenter-sdk) AMM, where `floor_hook` is an
approved hook.

| | |
|---|---|
| Release | `v0.1.0` |
| npm | `@floorlaunchpad/sdk`, `@floorlaunchpad/sdk-evm`, `@floorlaunchpad/sdk-solana` |
| Integration facts | [`docs/integration-facts.md`](docs/integration-facts.md): every chain, address, live pool, sample transaction, event and endpoint in one page |
| Rust | git dependencies at tag `v0.1.0` (not on crates.io) |
| Docs | [floor.supply/developers/sdk](https://floor.supply/developers/sdk) · [REST API](https://floor.supply/developers) · [contracts](https://floor.supply/developers/contracts) |
| Licence | Apache-2.0 |

The Solana programs are upgradeable through a timelock on mainnet-beta (a 10-minute delay that can only be raised,
proposed by the deployer key) and by the deployer key directly on devnet, and pausable by the deployer key. The EVM
contracts are upgradeable and pausable by their deployer key. No security audit has been completed.

## Start here

| You build | Use | Read |
|---|---|---|
| **A listing app** (launch lists, token pages, screeners, portfolio views) | FLOOR's public endpoints: the [partner API](https://floor.supply/developers), the token list per chain and the GeckoTerminal / DEX Screener-shaped DEX endpoint. Or read straight from chain: `listLaunches`, `getLaunch`. | [Public endpoints](#public-endpoints) · [Read launches](#read-launches) |
| **An aggregator or router** | EVM: graduated pools are Uniswap v4 pools under FLOOR's hook; `simulateSwap` quotes them off-chain, and [`integrations/kyberswap`](integrations/kyberswap) and [`integrations/velora`](integrations/velora) are the hook for KyberSwap's and Velora's DEX libraries. Solana: `floor-quote` (the hook's quoter for Carpenter's `HookQuoter`), `floor-sale-amm` (bonding sales) and Carpenter's `carpenter-amm` (pools) are `jupiter-amm-interface` `Amm`s. | [Integration facts](docs/integration-facts.md#quoting-off-chain) · [`jupiter/`](jupiter) · [Addresses](#addresses-and-interfaces) |
| **A terminal, trading app or bot** | `FloorClient` with both adapters: `quote`, then `buildTrade`, then send with `sendEvmCalls` / `sendSolanaTransactions` (one wallet prompt per action where the wallet allows). | [Quote and trade](#quote-and-trade) · [`sdk-evm`](packages/sdk-evm/README.md) · [`sdk-solana`](packages/sdk-solana/README.md) |
| **A wallet** | The token list per chain for names and logos; balances are plain ERC-20 / Token-2022 balances; the same reads and trades as a terminal. | [Public endpoints](#public-endpoints) · [Read launches](#read-launches) |
| **An indexer, analytics or data site** | The event decoders on both chains, the start blocks, and how FLOOR's EVM events become launches, pools, trades, the floor and candles. DefiLlama's adapters are in [`integrations/defillama`](integrations/defillama). | [Events](#events) · [`docs/evm-events.md`](docs/evm-events.md) |

Not covered by the SDKs: creating launches, claims (creator fees, holder rewards, referrals), vesting, team steps and
governance. The ABIs and IDLs shipped here are complete, so those calls can be built from them.

## Install

```sh
npm install @floorlaunchpad/sdk @floorlaunchpad/sdk-evm viem              # EVM
npm install @floorlaunchpad/sdk @floorlaunchpad/sdk-solana @solana/kit    # Solana
```

Peer dependencies: `viem` `^2.56.3`, `@solana/kit` `^8.3.0`. `@floorlaunchpad/sdk-solana` installs `@floorlaunchpad/carpenter-sdk` `^0.1.0`.
The Solana quoter is a wasm file shipped in the package, so no Rust toolchain is needed.

```toml
# Rust
[dependencies]
floor-types = { git = "https://github.com/gudtekdonkey/floor-sdk", tag = "v0.1.0" }
floor-quote = { git = "https://github.com/gudtekdonkey/floor-sdk", tag = "v0.1.0" }
```

## Public endpoints

FLOOR's site serves these; the SDK does not wrap them.

| URL | What |
|---|---|
| [`https://floor.supply/api/v1/openapi.json`](https://floor.supply/api/v1/openapi.json) | The partner API's OpenAPI spec (the API is `https://floor.supply/api/v1`, keyed; [docs](https://floor.supply/developers)) |
| `https://floor.supply/api/tokens/<chainId>` | A [token list](https://tokenlists.org) of the chain's FLOOR launches, e.g. `https://floor.supply/api/tokens/8453` for Base |
| `https://floor.supply/api/dex/<chain>/{latest-block, asset, pair, events}` | GeckoTerminal's Integration API Standard (DEX Screener reads the same shape); `<chain>` is `eth`, `base`, `bsc`, `rh`, `arc` or `sol`. The same model as code: `@floorlaunchpad/sdk`'s `dex` |

## Read launches

```ts
import { FloorClient, evmRef, solanaRef } from '@floorlaunchpad/sdk';
import { createEvmAdapter, sendEvmCalls } from '@floorlaunchpad/sdk-evm';
import { createSolanaAdapter, sendSolanaTransactions } from '@floorlaunchpad/sdk-solana';
import { initQuote } from '@floorlaunchpad/sdk-solana/quote/wasm';
import { createSolanaRpc } from '@solana/kit';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

// Solana: the quoter is wasm. In Node read the shipped file; in a browser fetch it and pass the bytes.
const wasm = readFileSync(createRequire(import.meta.url).resolve('@floorlaunchpad/sdk-solana/floor_quote_bg.wasm'));
const quoter = await initQuote(wasm);
const rpc = createSolanaRpc('https://api.mainnet-beta.solana.com');

// A Solana sale trade compiles through the launch's own lookup table, recorded by the launchpad (see below).
const launchTable = async (ref) => (await fetchLaunchFromFloorApi(ref)).lookupTable; // string | null

const floor = new FloorClient([
  createEvmAdapter({ chains: [8453], rpcUrls: { 8453: 'https://your-base-rpc' } }),
  createSolanaAdapter({ clusters: { mainnet: { rpc } }, quoter, launchTable }),
]);

const { launches, cursor } = await floor.listLaunches(solanaRef('mainnet'), { limit: 20 }); // newest first
const launch = await floor.getLaunch(launches[0]!.ref);
```

A `Launch` carries the name, symbol, decimals, supply, quote asset, phase (`pending`, `bonding`, `graduated`), mode,
creator and the published floor (`floorPriceX18`: whole quote per whole token × 1e18). `raw` carries the chain's own
decoded accounts. Amounts are bigints in base units.

⚠ Listings are unfiltered: anyone can create a launch with any name and symbol. Identify a launch by its address, never
by its name.

## Quote and trade

```ts
const q = await floor.quote(launch.ref, 'buy', 1_000_000_000n); // quote asset in, base units
// q.fees lists the trader's whole take, or the call throws:
//   EVM: LP fee, floor skim, creator fee, holder fee, FLOOR Platform fee
//   Solana: the same five, plus the Carpenter Platform fee

const tx = await floor.buildTrade({
  launch: launch.ref, side: 'buy', amountIn: 1_000_000_000n,
  minAmountOut: (q.amountOut * 99n) / 100n, // 1% slippage
  trader,
});

// tx is unsigned: every call or transaction the action needs, in order.
if (tx.family === 'evm') await sendEvmCalls(walletClient, tx);
if (tx.family === 'solana') await sendSolanaTransactions(wallet, tx, rpc);
```

Things a trading integration must know:

- **Two phases.** A launch sells at a fixed price in its bonding sale, then graduates into a pool. `buildTrade` picks
  the venue from the phase. Before graduation a sell is a sell-back to the sale at the sale price, less the FLOOR
  Platform fee. A buy that sells the sale out graduates the launch in the same transaction (`q.graduates`; on Solana
  `q.maxExtraNative` is the most lamports it may take on top of `amountIn`).
- **Solana sale trades need the launch's lookup table.** It is created with the launch and cannot be derived from chain
  state. Pass `launchTable: async (ref) => address | null` to `createSolanaAdapter`; the REST API's launch object carries it as
  `lookupTable`. Without it a sale trade throws `SolanaAdapterError('needs_launch_table')`. Pool trades do not need it.
- **Solana program version.** This release builds for the program set whose hook callbacks take four accounts
  (`ApprovedHook.callback_accounts` = 4), the set on the mainnet-beta and devnet programs listed below. On a cluster
  running an earlier set, trades are refused with `SolanaAdapterError('program_version')`. See
  [`sdk-solana`](packages/sdk-solana/README.md#program-version).
- **Wallet prompts.** EVM: one prompt with an atomic EIP-5792 batch, else one per call (a first sell through the
  UniversalRouter is Permit2's approval, then the swap). Solana: one prompt for every transaction of an action with a
  batch-signing wallet. Details and failure codes in each adapter's README.

## Addresses and interfaces

### Solana programs

| Program / account | mainnet-beta | devnet |
|---|---|---|
| `floor_hook` | `9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy` | `6foted69N6wB1oJzsxccbXUxefBt3Hkn7tN2yHifXnTS` |
| `floor_launch` | `8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi` | `93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG` |
| `floor_holders` | `2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te` | `DShmCjpc34oRb3mqjLvp1fs1rYfY6ZLvj1G38d1s6XPZ` |
| `floor_vesting` | `HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC` | `5UfmuRmDdsVCE4q9UXW82thrzcaTAGMNgQZcugy7jjfm` |
| `hook_config` | `CfhjSM2zTcxnKggQvHjUzYPTc6W9QxC2cABJnCHq4Cxx` | `FwRffZ8ayx6v44qn8iB5NvyPevUBtEkaYZG5bF2yRwU2` |
| `launch_config` | `Ck9wU4tSb4dLtaW4PyEfg8NYrRJkCS6daBVu13cxtVQz` | `EktftZaTm7h12CSmkubw9R1oZZJubMayspbcZ9tM9LSQ` |
| Deployment lookup table | `GLu7tLCby32SPS43p9wdHW7EbV7ZoaEfyeCWx6hkxAr3` | `DLwVDkiQP2XUPjoDQcEFu2oyaKoCLZbqaR62Hwb6ZjKD` |
| `carpenter_amm` | `DdeiDx4ncHo9DXGZm1GrPe7mFKv7kNU3yx95uQEV2APA` | `Arv1M4LifBrWMywvxpeAGvh37aVHPjpXPCg7sacbe3Hy` |

In code: `programIdsFromDeployment(deploymentFor(MAINNET_KEY))` (or `DEVNET_KEY`). The shipped IDLs name the mainnet
programs; devnet runs the same source under its own ids. Machine-readable: `packages/sdk-solana/deployments.json`.

### EVM contracts

Ethereum (1), Base (8453), BSC (56), Robinhood Chain (4663) and Arc (5042) share one set of FLOOR addresses. Sepolia
(11155111) is the test set.

| Contract | Ethereum · Base · BSC · Robinhood Chain · Arc | Sepolia |
|---|---|---|
| SingletonFloorHook (the Uniswap v4 hook of every launch's pool) | `0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc` | per launch: `LaunchCreated.hook` |
| Registry | `0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804` | `0x1e816626508Ca63dbb71cD6D1E4794892dbBC989` |
| Factory | `0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F` | `0xeFd78A82F486E37967dc89Bd8b57373703551134` |
| Vesting | `0xa826B75Ca94206C51De45c8112c5ce8541e357A3` | `0x112d89ae8649DB6e9e4a13F6A9f87be9c89E512C` |
| Recipient claims | `0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40` | `0x9789D099378BC19bF539189792843eCd972DCAbb` |
| Holder rewards | `0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD` | `0x16D24e5F3d93F5e53ea4e81DdCe0D7FEb39C5021` |
| Team treasury | `0x062B6c421E37579728877337B47Cf528AE4d49bb` | `0xDe11874af6a3Aa96b2d09d5039a30c43537713E4` |
| Trader referrals | `0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE` | `0x5576dcC7CaE2416DAF84d878a3460052BA97Bb0F` |
| Preset registry | `0x2d5b6582C0EA925be1e356F7ECE585238d911505` | `0x484dFb3cd0468F9c6F1f0dbf6365F5EBBc138A65` |
| FloorZap | `0x0117F2686F195eb7b85C27C96EEFDC8040079d9D` | `0x90944bb31AC6e69ae36853e16040F85BF76Aba20` |
| FloorSwapRatchet | `0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371` (not on Ethereum) | `0xeF3B37709384512b1367C031b2801567D952f038` |
| FloorZap over Aerodrome | `0x3b5adDBcEF6785ede8bDf71766Ab1275663129ED` (Base only) | — |
| Platform treasury | `0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c` | — |
| FloorUpgradeTimelock (governance) | `0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E` | — |

| Start block | Ethereum | Base | BSC | Robinhood Chain | Arc | Sepolia |
|---|---|---|---|---|---|---|
| | 26178890 | 52524628 | 127278845 | 86973488 | 25642396 | 11895000 |

Every launch's pool hook, launch (sale) contract and pool id come from the registry and the factory's `LaunchCreated`
event. Uniswap's v4 contracts per chain (PoolManager, UniversalRouter and its release, Permit2, V4Quoter, StateView) are
in the same table. In code: `floorDeploymentFor(chainId)`; machine-readable: `EVM_DEPLOYMENTS` in
[`packages/sdk-evm/src/deployments.ts`](packages/sdk-evm/src/deployments.ts). A chain with no FLOOR set is refused by
name, never given a zero address.

### IDLs and ABIs

| What | Where | Import |
|---|---|---|
| Solana IDLs: `floor_hook`, `floor_launch`, `floor_holders`, `floor_vesting` | [`packages/sdk-solana/idl/`](packages/sdk-solana/idl) | `@floorlaunchpad/sdk-solana/idl/floor_hook.json` |
| Solana Codama clients | `packages/sdk-solana/src/generated/` | `floorHook`, `floorLaunch`, `floorHolders`, `floorVesting` from `@floorlaunchpad/sdk-solana` |
| Carpenter IDL (`carpenter_amm`) | [`carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk/tree/main/packages/sdk/idl) | `@floorlaunchpad/carpenter-sdk` |
| EVM ABIs (hook, launch / sale, token, factory, registry, claims, vesting, rewards, referrals, zap, swap ratchet, every FLOOR error) | [`packages/sdk-evm/src/abi/`](packages/sdk-evm/src/abi) | `HOOK_ABI`, `LAUNCH_ABI`, `TOKEN_ABI`, `FACTORY_ABI`, `REGISTRY_ABI`, `FLOOR_ERRORS_ABI`, … from `@floorlaunchpad/sdk-evm` |

The Solana IDLs ship without their `docs` fields; instructions, accounts, types, events and errors are complete.
`IDL_SHA256` is their hash as shipped. The EVM ABIs are generated from the contracts' build artifacts.

### Events

- **EVM**: `decodeFloorLog`, `decodeFloorLogs` and `floorEventTopics` from `@floorlaunchpad/sdk-evm` decode every FLOOR event
  (`FLOOR_EVENTS` lists them): `LaunchCreated` (factory: token, hook, launch, pool id, mode, creator), `Graduated`
  (launch), `Ratcheted` (hook, keyed by pool id: the new floor tick), the hook's `HookFee`, and Uniswap v4's `Swap` on the
  PoolManager for pool trades. How they become launches, pools, trades, the floor and candles:
  [`docs/evm-events.md`](docs/evm-events.md).
- **Solana**: `decodeInnerInstructionEvents`, `decodeEmitCpiData` and `decodeEventPayload` from `@floorlaunchpad/sdk-solana`
  decode every event of the four programs and `carpenter_amm`; `EVENT_NAMES` lists them per program.

## Repository

| Path | What |
|---|---|
| [`packages/sdk`](packages/sdk) | `@floorlaunchpad/sdk`: chain-neutral types (`ChainRef`, `LaunchRef`, `Launch`, `Quote`, `UnsignedTx`), `FloorChainAdapter` and `FloorClient` |
| [`packages/sdk-evm`](packages/sdk-evm) | `@floorlaunchpad/sdk-evm`: ABIs, per-chain deployments, reads, quotes and trade builders over viem, `sendEvmCalls` |
| [`packages/sdk-solana`](packages/sdk-solana) | `@floorlaunchpad/sdk-solana`: IDLs and Codama clients, layouts, PDAs, events, sale and pool trade builders, the quoter wasm |
| [`rust/`](rust) | `floor-types` (layouts, seeds, constants, program ids), `floor-math` (the mechanism's math), `floor-quote` (`FloorHookQuoter`), `floor-quote-wasm` |
| [`jupiter/`](jupiter) | `floor-sale-amm`: a `jupiter-amm-interface` `Amm` for FLOOR bonding sales |
| [`docs/`](docs) | [`integration-facts.md`](docs/integration-facts.md) (every chain, address, live pool and endpoint) and [`evm-events.md`](docs/evm-events.md) |
| [`integrations/`](integrations) | FLOOR for KyberSwap (Go), Velora (TypeScript) and DefiLlama, at their upstream paths, each with its upstream patch or PR text |

Building, testing, the quoter wasm and adding a cluster or a chain: [`CONTRIBUTING.md`](CONTRIBUTING.md). Changes:
[`CHANGELOG.md`](CHANGELOG.md). Reporting a vulnerability: [`SECURITY.md`](SECURITY.md).

## Licence

Apache-2.0 (`LICENSE`). `NOTICE` reproduces the notice of `carpenter-clmm-math`, which the shipped quoter wasm is
compiled from; `rust/THIRD_PARTY.md` and `jupiter/THIRD_PARTY.md` list the Rust dependencies.
