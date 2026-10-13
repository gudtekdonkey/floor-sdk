# @floor/sdk-evm

FLOOR on EVM chains: the ABIs, where the contracts are on each chain, and [viem](https://viem.sh) code to read FLOOR
launches, quote trades and build unsigned trade transactions. It implements `@floor/sdk`'s `FloorChainAdapter`, so the
same `FloorClient` serves EVM and Solana launches.

`buildTrade` never signs: it returns `{ family: 'evm', chainId, calls }` — every call the action needs, in order (any
approval the trader has not yet granted, then the trade). `sendEvmCalls` sends them with as few wallet prompts as the wallet
allows.

## Install

```sh
npm install @floor/sdk @floor/sdk-evm viem
```

`@floor/sdk` and `viem` (^2.56.3) are peer dependencies.

## Example

```ts
import { FloorClient, evmRef } from '@floor/sdk';
import { createEvmAdapter, sendEvmCalls } from '@floor/sdk-evm';

const evm = createEvmAdapter({ chains: [11155111], rpcUrls: { 11155111: 'https://your-sepolia-rpc' } });
const floor = new FloorClient([evm]);

// One registry page, newest first.
const { launches, cursor } = await floor.listLaunches(evmRef(11155111), { limit: 20 });
const launch = launches[0]!;

// A quote lists the trader's whole take: LP fee, floor skim, creator fee, holder tax, FLOOR Platform fee.
const q = await evm.quote(launch.ref, 'buy', 10n ** 16n);

// The approvals still missing (read now) and the trade, as one batch.
const tx = await evm.buildTrade({ launch: launch.ref, side: 'buy', amountIn: 10n ** 16n, minAmountOut: (q.amountOut * 99n) / 100n, trader: account.address });
const sent = await sendEvmCalls(walletClient, tx, {
  onSubmitted: (id) => saveBatchId(id), // a 5792 batch id, handed over before any wait
  onSent: (hash) => saveHash(hash),     // each transaction on the one-per-call path
}); // { path: 'eip5792', id, hashes } or { path: 'transactions', hashes }
```

### When sending fails

A rejection after something was submitted is not proof that nothing was sent. `sendEvmCalls` throws a `FloorEvmError`
whose `code` says which case it is:

| `code` | Meaning | `id` / `hashes` |
|---|---|---|
| `wrong_chain` | The wallet is on another chain. Nothing was sent. | — |
| `pending` | Something was submitted, but the wait timed out or the status read failed. Keep waiting on what was submitted; do not send the action again. | the batch id / every hash sent |
| `batch_failed` | The wallet reports the 5792 batch as failed. | the batch id, any receipts' hashes |
| `reverted` | A transaction on the one-per-call path reverted; the calls after it were not sent. | every hash sent |

The waits are explicit options: `batchTimeoutMs` (default `DEFAULT_BATCH_TIMEOUT_MS`, 60 s) for a batch's final status,
`receiptTimeoutMs` (default `DEFAULT_RECEIPT_TIMEOUT_MS`, 180 s) for each receipt. `wait: false` returns at the batch id.

## Wallet prompts per action

| Wallet | Prompts |
|---|---|
| Supports an atomic EIP-5792 batch on the chain (`wallet_getCapabilities` → `atomic.status` `supported` / `ready`, or an older wallet's `atomicBatch.supported`; the chain's own entry, else the chain-agnostic `"0x0"` entry) | **1** — one `wallet_sendCalls` (atomic required), the approvals and the trade land together or not at all; the receipts are awaited with `waitForCallsStatus` |
| No EIP-5792 (`getCapabilities` throws — Privy's embedded wallet did not implement `wallet_sendCalls`), no atomic batch on that chain, or a `wallet_sendCalls` refused as unsupported before anything was sent (codes 4200, -32601, 5700, 5710, 5760) | **one per call** — each call is its own transaction, sent in order, each receipt awaited before the next; a reverted call stops the rest |

A first sell of a FloorToken through the UniversalRouter is two calls: Permit2's approval of the router, then the swap. A
FloorToken reports Permit2 as approved for unlimited by default, so the token's own approval of Permit2 is not needed;
it is three calls only when the holder has turned that default off, and for a buy paid in an ERC-20 quote (the quote's
approval of Permit2, Permit2's approval of the router, the swap). After that a trade is one call. A native-quote buy and a
sell-back are always one call; a bonding buy paid in an ERC-20 quote is two (the approval of the sale, the buy). No action
ever asks for an off-chain signature (a Permit2 `signTypedData`): Permit2 is approved ON CHAIN, inside the same batch.

### Approvals

`buildTrade(params, { approvalAmount })` chooses how much each missing approval grants: `'exact'` (the amount this trade
spends) or `'unlimited'` (2^255 to a token's spender, Permit2's maximum to a Permit2 spender). The default is `'exact'` for
a bonding buy, whose spender is the launch's own sale contract, and `'unlimited'` for a pool swap (Permit2, then the
UniversalRouter or `FloorSwapRatchet`). An ERC-20 allowance that is above 0 but short is set to 0 first, then to the new
amount, for tokens (USDT among them) that refuse to move an allowance between two non-zero values.

### Gas

Every bonding buy carries `gas: BOND_BUY_GAS_FLOOR` (5.1M): any buy while the sale is open can become the one that sells
it out, and that buy graduates the launch in the same transaction. The limit is honoured on the one-per-call path. On the
EIP-5792 path calls carry no gas limit and the wallet estimates the batch, so the floor is dropped there.

Zero prompts happen only with an embedded wallet the integrator itself controls and signs with; a user's own wallet always
asks.

A local chain (Anvil) or a set not yet in the committed table is passed as a row:

```ts
createEvmAdapter({ chains: [31337], rpcUrls: { 31337: 'http://127.0.0.1:8545' }, deployments: { 31337: { chainId: 31337, ... } } });
```

## What it covers

- **ABIs** (`abi/`): the singleton hook (and the per-pool views called through `execute`), launch / sale, token, factory,
  registry, PoolManager `extsload`, UniversalRouter, V4Router, V4Quoter, v3 QuoterV2, Permit2, PoolSwapTest, holder rewards,
  recipient claims, vesting, preset registry, platform treasury, trader referrals, `FloorSwapRatchet`, `FloorZap`, team steps,
  and `FLOOR_ERRORS_ABI` (every error the FLOOR contracts declare, for decoding a revert that bubbled out of a nested call).
  FLOOR's own ABIs are generated from the contracts' build artifacts by `scripts/gen-abi.mjs` (`src/abi/generated.ts`).
- **Deployments** (`deployments.ts`): per chain id, FLOOR's addresses and Uniswap's v4 addresses, start blocks, router kinds
  and releases. A chain with no FLOOR set is refused by name (`floorDeploymentFor`), never given a zero address. Six chains
  carry a FLOOR set: the mainnets Ethereum (1), Base (8453), BSC (56), Robinhood Chain (4663) and Arc (5042), which share
  one set of contract addresses (deployment `0x…1a`), and the Sepolia test set (11155111). Base also carries
  `zapAerodrome` (a `FloorZap` over Aerodrome's UniversalRouter). Ethereum has no `FloorSwapRatchet`: a ratchet costs about
  1.4M gas there, so a pool trade goes straight through the UniversalRouter.
- **Reads**: `getLaunch`, `listLaunches` (one registry page per call), the pool state (`readPoolState`, width-checked), the
  published floor (`floorPriceX18`, off the pool's `floorTick`), and every rate read per launch or per pool.
- **Quotes**: a bonding buy (the sale's tranche walk), a sell-back (`quoteSellBack`, less the FLOOR Platform fee), each in
  the contract version's own rounding (see "Limits worth knowing"), and a pool trade (Uniswap's V4Quoter).
- **Trades**: bonding buy, sell-back, pool buy / sell through the UniversalRouter (a token input pulled through Permit2,
  approved on chain in the same batch), and the same through `FloorSwapRatchet` (`buildTrade(params, { swapRatchet })`).
- **Sending**: `sendEvmCalls(walletClient, tx)` — EIP-5792 where atomic, else one transaction per call.

## What it does not cover

- Creating launches, claims (creator fees, holder rewards, referrals), vesting, team steps, the zap (paying a stock-quoted
  sale in ETH) and multi-hop routes: the ABIs are here, the builders are not.
- History, charts and indexer reads.
- Solana: see `@floor/sdk-solana`.

## Limits worth knowing

- A bonding buy and a sell-back carry **no on-chain minimum** (the sale takes none). `buildTrade` refuses a trade whose
  quote, read just before, is under `minAmountOut`; a buy racing another buy can still fill at a later tranche's price.
- A pool quote's `amountOut` is the V4Quoter's simulation; its fee parts are computed from the pool's own rates at the
  pre-trade tick (`feesExact: false`). A sell's skim is charged at the post-swap tick, so its skim part is a minimum.
- PoolSwapTest (a testnet harness with no minimum received) is refused as a trade route.
- Contract versions differ, and a quote models the version its deployment row names (`Deployment.rules`, read with
  `rulesOf`; a row that names none gets `CURRENT_RULES`):
  | rule | current contracts (every row here) | older contracts (`OLDER_RULES`) |
  |---|---|---|
  | `saleCost` — a sale tranche's cost | rounded up | rounded down |
  | `sellBack` — what a sell-back pays | the tokens at the sale price, rounded down, capped at what the position paid | the position's pro-rata share of what it paid |
  | `gapSkim` — a pool buy's floor skim | scaled by how far the price stands above the staircase cushion (1× up to 2,232 ticks, rising linearly to 1.5× at 5,109, capped at 15%; `gapSkimBps`) | not scaled |
  A caller's row for a set built from other sources should name its rules.

Apache-2.0.
