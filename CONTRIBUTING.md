# Contributing to floor-sdk

How this repository is built, tested and extended. For using the SDKs, see [`README.md`](README.md).

## Build and test

```sh
npm install
npm run typecheck && npm test
(cd rust && cargo test --workspace)
(cd jupiter && cargo test)
bash scripts/build-wasm.sh --check     # rebuild the quoter wasm; it must equal the committed one
```

## Two repositories, one release

FLOOR's Solana side is a hook on Carpenter, and Carpenter's SDK is its own repository,
[`carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk):

- `@floorlaunchpad/sdk-solana` depends on `@floorlaunchpad/carpenter-sdk` (npm, `^0.1.0`).
- `floor-types` and `floor-quote` depend on `carpenter-types`, `carpenter-clmm-math` and `carpenter-quote`;
  `floor-quote` implements `carpenter_quote::HookQuoter`. `carpenter-sdk`'s Jupiter adapter depends back on `floor-quote`.

The Rust workspace takes the Carpenter crates from `carpenter-sdk` at the release tag with the same version
(`rust/Cargo.toml`, `[workspace.dependencies]`), and `carpenter-sdk`'s Jupiter adapter takes `floor-quote` from this repository at a commit:

```toml
# rust/Cargo.toml
carpenter-quote = { git = "https://github.com/gudtekdonkey/carpenter-sdk", tag = "0.1.0" }
# carpenter-sdk/jupiter/Cargo.toml
floor-quote = { git = "https://github.com/gudtekdonkey/floor-sdk", rev = "72399c843d3177333ef3cbf8b06a6d78ec53f1be" }
```

Both repositories must point at the same release: Cargo treats `carpenter-quote` from two sources as two crates, and
`floor-quote`'s `HookQuoter` would then not be the trait Carpenter's adapter calls.

## The quoter wasm

`packages/sdk-solana/wasm/floor_quote_bg.wasm` is `rust/crates/floor-quote-wasm` compiled to wasm and bound with
`wasm-bindgen --target web`. It is committed, with its glue, so an install needs no Rust. `npm run build-wasm -w
@floorlaunchpad/sdk-solana` rebuilds it (`scripts/build-wasm.sh`: Rust 1.98.1 with `wasm32-unknown-unknown`,
`wasm-bindgen-cli` 0.2.128); `npm run check-wasm -w @floorlaunchpad/sdk-solana` rebuilds it and fails unless the bytes equal
the committed ones. The build is reproducible: paths are remapped and doc comments removed from the build copy.

## Adding a cluster

One build serves every cluster it lists; nothing assumes a single cluster's program ids. To add a cluster, with
Carpenter's row already added in `carpenter-sdk` (its `CONTRIBUTING.md`, "Adding a cluster"):

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
   npm run generate -w @floorlaunchpad/sdk-solana        # validates deployments.json → src/generated/
   npm run build-wasm -w @floorlaunchpad/sdk-solana      # the quoter wasm → packages/sdk-solana/wasm
   npm run check-wasm -w @floorlaunchpad/sdk-solana      # a rebuild equals the committed wasm
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

