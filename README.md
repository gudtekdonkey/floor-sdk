# floor-sdk

SDKs for FLOOR, a launchpad whose launches trade on a pool with a published floor. On EVM chains FLOOR is a Uniswap v4
hook; on Solana it is a set of programs (`floor_hook`, `floor_launch`, `floor_holders`, `floor_vesting`) on the
[Carpenter](https://github.com/gudtekdonkey/carpenter-sdk) AMM, where `floor_hook` is an approved hook.

The packages read launches, quote trades and build unsigned trade transactions. Launch creation, claims, vesting,
team steps and governance are not part of them; the ABIs and IDLs they ship are complete.

| Path | What |
|---|---|
| [`packages/sdk`](packages/sdk) | `@floor/sdk`: chain-neutral types (`ChainRef`, `LaunchRef`, `Launch`, `Quote`, `UnsignedTx`), the `FloorChainAdapter` interface and `FloorClient`, which routes each call to the adapter for the launch's chain |
| [`packages/sdk-evm`](packages/sdk-evm) | `@floor/sdk-evm`: the EVM adapter (ABIs, per-chain deployments, reads and trade builders over viem) |
| [`packages/sdk-solana`](packages/sdk-solana) | `@floor/sdk-solana`: the Solana adapter (the four programs' IDLs and Codama clients, layouts, PDAs, events, the sale buy / sell-back and graduated-pool trade builders, the quoter wasm), built on `@carpenter/sdk` |
| [`rust/`](rust) | Rust crates: `floor-types` (layouts, seeds, constants), `floor-math` (the mechanism's math), `floor-quote` (the FLOOR hook's quoter for Carpenter's `HookQuoter`), `floor-quote-wasm` (the wasm build `@floor/sdk-solana` ships) |
| [`jupiter/`](jupiter) | `floor-sale-amm`: a `jupiter-amm-interface` `Amm` for FLOOR bonding sales; its own Cargo workspace |
| [`integrations/defillama/`](integrations/defillama) | DefiLlama's TVL and volume adapters for FLOOR, kept here and copied into DefiLlama's repositories |
| [`scripts/build-wasm.sh`](scripts/build-wasm.sh) | builds the quoter wasm reproducibly from `rust/crates/floor-quote-wasm` |

## Install

```sh
npm install @floor/sdk @floor/sdk-solana @solana/kit   # Solana
npm install @floor/sdk @floor/sdk-evm viem             # EVM
```

```ts
import { FloorClient } from '@floor/sdk';
import { createSolanaAdapter } from '@floor/sdk-solana';

const floor = new FloorClient([createSolanaAdapter({ clusters: { mainnet: { rpc } }, quoter })]);
```

Amounts are bigints in base units. A quote lists the trader's whole take or the call throws. The Solana programs are
upgradeable through a timelock on mainnet-beta (a 10-minute delay that can only be raised, proposed by the deployer key) and by the deployer key directly on devnet, and pausable by the deployer key; the EVM contracts are upgradeable and pausable by their deployer key. No security audit has been completed.

## EVM chains

`@floor/sdk-evm` ships a FLOOR set for Ethereum (1), Base (8453), BSC (56), Robinhood Chain (4663) and Arc (5042), which
share one set of contract addresses, and for the Sepolia test set (11155111). Other rows in its table carry Uniswap v4's
contracts only and are refused by name. See [`packages/sdk-evm/README.md`](packages/sdk-evm/README.md).

## Required program version (Solana)

`@floor/sdk-solana` and the Rust crates in this release build for the FLOOR program set whose hook callbacks take
**four** accounts (`ApprovedHook.callback_accounts` = 4), whose bonding-sale sell-back is stateless and which has
`sell_back_wsol`; the mainnet-beta and devnet programs this release lists are that set. On a cluster running an earlier set (one
callback account), the Solana adapter refuses to build trades with `SolanaAdapterError('program_version')`, and sell-back
quotes are a lower bound of what the earlier program pays. See [`packages/sdk-solana/README.md`](packages/sdk-solana/README.md#program-version).

## Two repositories

FLOOR's Solana side is a hook on Carpenter, and Carpenter's SDK is its own repository,
[`carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk):

- `@floor/sdk-solana` depends on `@carpenter/sdk` (npm, `^0.1.0`).
- `floor-types` and `floor-quote` depend on `carpenter-types`, `carpenter-clmm-math` and `carpenter-quote`;
  `floor-quote` implements `carpenter_quote::HookQuoter`. `carpenter-sdk`'s Jupiter adapter depends back on `floor-quote`.

The Rust workspace takes the Carpenter crates from `carpenter-sdk` at the release commit with the same version
(`rust/Cargo.toml`, `[workspace.dependencies]`), and `carpenter-sdk`'s Jupiter adapter takes `floor-quote` the same way:

```toml
# rust/Cargo.toml
carpenter-quote = { git = "https://github.com/gudtekdonkey/carpenter-sdk", rev = "<release commit>" }
# carpenter-sdk/jupiter/Cargo.toml
floor-quote = { git = "https://github.com/gudtekdonkey/floor-sdk", rev = "<release commit>" }
```

The npm packages are not on the registry yet: the root `package.json` overrides `@carpenter/sdk` with a
`carpenter-sdk` checkout beside this one, so the TypeScript build needs both repositories side by side.

Both repositories must point at the same release: Cargo treats `carpenter-quote` from two sources as two crates, and
`floor-quote`'s `HookQuoter` would then not be the trait Carpenter's adapter calls.

## Build and test

```sh
# with carpenter-sdk checked out beside this repository (for the npm workspace)
npm install
npm run typecheck && npm test
(cd rust && cargo test --workspace)
(cd jupiter && cargo test)
bash scripts/build-wasm.sh --check     # rebuild the quoter wasm; it must equal the committed one
```

## Solana clusters

| | mainnet-beta (`MAINNET_KEY`) | devnet (`DEVNET_KEY`) |
|---|---|---|
| `floor_hook` | `9uXLyyZginMWdUeu22kp7XV7cpV8183xfsVqUxKi5Viy` | `6foted69N6wB1oJzsxccbXUxefBt3Hkn7tN2yHifXnTS` |
| `floor_launch` | `8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi` | `93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG` |
| `floor_holders` | `2EUkJXSwCdRAuhesw2D9FXyGfbi6Kw2yZDyQUnCwc7te` | `DShmCjpc34oRb3mqjLvp1fs1rYfY6ZLvj1G38d1s6XPZ` |
| `floor_vesting` | `HHiwbXs3egRwtgEPEKx1PHpBU8gq5VSbhfrGrcCd4CSC` | `5UfmuRmDdsVCE4q9UXW82thrzcaTAGMNgQZcugy7jjfm` |
| `hook_config` | `CfhjSM2zTcxnKggQvHjUzYPTc6W9QxC2cABJnCHq4Cxx` | `FwRffZ8ayx6v44qn8iB5NvyPevUBtEkaYZG5bF2yRwU2` |
| `launch_config` | `Ck9wU4tSb4dLtaW4PyEfg8NYrRJkCS6daBVu13cxtVQz` | `EktftZaTm7h12CSmkubw9R1oZZJubMayspbcZ9tM9LSQ` |
| address lookup table | `GLu7tLCby32SPS43p9wdHW7EbV7ZoaEfyeCWx6hkxAr3` | `DLwVDkiQP2XUPjoDQcEFu2oyaKoCLZbqaR62Hwb6ZjKD` |

Carpenter's ids for each cluster are in `carpenter-sdk`. The shipped IDLs and generated clients name the mainnet
programs, and `DEFAULT_PROGRAM_IDS` is mainnet's; devnet runs the same program source under its own ids.

## Adding a cluster

One build serves every cluster it lists; nothing assumes a single cluster's program ids. To add a cluster, with
Carpenter's row already added in `carpenter-sdk` (its README, "Adding a cluster"):

1. **`packages/sdk-solana/deployments.json`**: add a row keyed by the cluster's CAIP-2 id.

   | Field | Value | |
   |---|---|---|
   | `name` | the cluster's name (`"testnet"`, ...) | must equal the Carpenter row's |
   | `chainKey` | the row's key (`solana:` + the first 32 characters of the genesis hash) | must equal the Carpenter row's |
   | `programs.floorHook`, `.floorLaunch`, `.floorHolders`, `.floorVesting` | the deployed program ids | may differ from the IDLs' `address` |
   | `accounts.hookConfig` | `["hook_config"]` under `floorHook` | derivable; checked |
   | `accounts.launchConfig` | `["launch_config"]` under `floorLaunch` | derivable; checked |
   | `accounts.teamTreasury` | `["team_treasury"]` under `floorHook` | derivable; checked when present |
   | `alt` | the cluster's deployment address lookup table, or `null` | recorded when created; not derivable |
   | `programDataHash.*` | each deployed executable's hash, or `null` | |
   | `quotes` | optional: `{ <mint>: { state, decimals, tokenProgram, terms, hook: { policy } } }` | `policy` is `["quote", mint]` under `floorHook`; checked |

   Only the `mainnet` and `local` rows must equal the shipped IDLs' `address`; any other row is checked to name base58
   32-byte keys, and its derivable accounts are checked against that row's program ids.
2. **`rust/crates/floor-types/src/ids.rs`**: add a `ClusterIds { name, carpenter_amm, floor_hook, floor_launch,
   floor_holders, floor_vesting }` entry to `CLUSTERS` (its `carpenter_amm` must be in
   `carpenter_types::ids::AMM_CLUSTERS`). The quoter accepts each listed cluster's `floor_hook`
   (`FloorHookQuoter::for_cluster` / `quote_swap_for` restrict it to one), and the Jupiter adapters select the cluster
   by the market account's owner.
3. Regenerate and rebuild:

   ```sh
   npm run generate -w @floor/sdk-solana        # validates deployments.json → src/generated/
   npm run build-wasm -w @floor/sdk-solana      # the quoter wasm → packages/sdk-solana/wasm
   npm run check-wasm -w @floor/sdk-solana      # a rebuild equals the committed wasm
   npm run typecheck && npm test && (cd rust && cargo test --workspace) && (cd jupiter && cargo test)
   ```

`createSolanaAdapter({ clusters: { mainnet: { rpc } } })` picks the shipped row up by its key and throws by name for a
cluster with no row; `clusters.<name>.row` supplies a joined row the shipped files do not list yet. Builders, PDA helpers and
reads take `ids: programIdsFromDeployment(deploymentFor(chainKey))`.

## Adding an EVM chain

An EVM chain is one row of `EVM_DEPLOYMENTS` in `packages/sdk-evm/src/deployments.ts`, keyed by chain id: FLOOR's
`registry`, `factory`, `vesting`, the set's `deploymentId` and `startBlock`, the optional contracts the set deployed
(`recipientClaims`, `holderRewards`, `teamTreasury`, `traderReferrals`, `presetRegistry`, `zap`, `swapRatchet`), and
Uniswap's v4 contracts on the chain (`poolManager`, `routers` with each UniversalRouter's release, `permit2`, `quoter`,
`stateView`). `checkDeployment` checks a row; a caller can pass a row the table does not list
(`createEvmAdapter({ deployments })`). A row for contracts older than the current ones names their `rules`.

## Licence

Apache-2.0 (`LICENSE`). `NOTICE` reproduces the notice of `carpenter-clmm-math`, which the shipped quoter wasm is
compiled from; `rust/THIRD_PARTY.md` and `jupiter/THIRD_PARTY.md` list the Rust dependencies.
