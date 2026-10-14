# @floorlaunchpad/sdk-solana

FLOOR on Solana: read FLOOR launches, quote trades and build unsigned trade transactions. It implements `@floorlaunchpad/sdk`'s
`FloorChainAdapter` (`createSolanaAdapter`), so the same `FloorClient` serves Solana and EVM launches.

FLOOR runs as four Solana programs — `floor_hook` (an approved hook of the Carpenter AMM), `floor_launch` (the bonding
sale and graduation), `floor_holders` (the holder tally, a Token-2022 transfer hook) and `floor_vesting` — on top of
Carpenter (`carpenter_amm`). This package is built on [`@floorlaunchpad/carpenter-sdk`](https://github.com/gudtekdonkey/carpenter-sdk/tree/main/packages/sdk) and adds:

- the four programs' IDLs and [Codama](https://github.com/codama-idl/codama) clients (`floorHook`, `floorLaunch`,
  `floorHolders`, `floorVesting`);
- FLOOR's account layouts, PDAs and event decoders, and read helpers (`stateView`, `getLaunch`, `listLaunches`);
- the sale buy (with the graduation `prepare` step it needs) and sell-back, and swaps on graduated FLOOR pools;
- the wasm quoter (`initQuote`): the sale's own arithmetic and the AMM walk with FLOOR's hook callbacks;
- `createSolanaAdapter` and, re-exported from `@floorlaunchpad/carpenter-sdk`, `sendSolanaTransactions`.

Launch creation, claims, vesting, team steps, `$FLOOR`'s raise, liquidity and governance are not in this package
(liquidity on any Carpenter pool is `@floorlaunchpad/carpenter-sdk`'s `buildModifyLiquidity`; on a launch with holder rewards pass it
this package's `transferHookAccounts`). The IDLs and generated clients are complete.

## Program version

> **This release builds for the FLOOR program set whose hook callbacks take four accounts** (`ApprovedHook.callback_accounts`
> = `FLOOR_CALLBACK_ACCOUNTS` = 4: `[floor_pool, trader_bucket, floor_hook event_authority, floor_hook]`), whose bonding-sale
> sell-back is stateless (it pays the launch's fixed price for the tokens, whoever sells them) and which has
> `sell_back_wsol`; the mainnet-beta and devnet programs this release lists are that set. A cluster running an earlier set (one callback
> account) is refused:
> before building a trade, `createSolanaAdapter` reads `floor_hook`'s `ApprovedHook` record on the AMM (and, on a
> graduated pool, the pool's own `hook_callback_accounts`) and throws `SolanaAdapterError` with code `program_version`
> when either is not 4.
>
> The check reads the hook's record. `floor_launch` is upgraded together with `floor_hook`; a cluster running an older
> `floor_launch` beside a current `floor_hook` is not detected, and there `sell_back_wsol` does not exist and the
> sell-back pays at least what `quote(…, 'sell', …)` says (the stateless quote is a lower bound of the older payout).

The IDLs in `idl/` are published without their `docs` (doc comment) fields; instructions, accounts, types, events and
errors are otherwise complete. `IDL_SHA256` is the hash of these files as shipped.

## Install

```sh
npm install @floorlaunchpad/sdk @floorlaunchpad/sdk-solana @solana/kit
```

`@floorlaunchpad/carpenter-sdk` (`^0.1.0`) is installed with it. `@solana/kit` is a peer dependency (`^8.3.0`). The quoter wasm is
shipped in the package (`wasm/`), so installing needs no Rust toolchain.

## Example

```ts
import { FloorClient, solanaRef } from '@floorlaunchpad/sdk';
import { createSolanaAdapter, sendSolanaTransactions } from '@floorlaunchpad/sdk-solana';
import { initQuote } from '@floorlaunchpad/sdk-solana/quote/wasm';
import { createSolanaRpc } from '@solana/kit';

const rpc = createSolanaRpc('https://api.mainnet-beta.solana.com');
const quoter = await initQuote(await (await fetch(wasmUrl)).arrayBuffer()); // floor_quote_bg.wasm, served as a file

const solana = createSolanaAdapter({
  clusters: { mainnet: { rpc } }, // or devnet
  quoter,
  launchTable: async (ref) => tableAddressFromLaunchpadApi(ref), // the launch's lookup table, for sale trades
  traderLink: async (ref, trader) => attestationFromFloorApi(trader), // optional: a referral link, or null
});
const floor = new FloorClient([solana]);

// ⚠ Unfiltered: anyone can create a launch, with any name and symbol. Match launches by mint, not by name.
const { launches, cursor } = await floor.listLaunches(solanaRef('mainnet'), { limit: 20 });
const launch = launches[0]!;

// The trader's whole take: LP fee, floor skim, creator fee, holder fee, FLOOR Platform fee, Carpenter Platform fee.
const q = await floor.quote(launch.ref, 'buy', 1_000_000_000n); // lamports in
// A buy that sells the sale out graduates the launch inside it: q.graduates, and q.maxExtraNative is the most lamports
// it can take ON TOP of amountIn (the rent of what graduation creates, bounded by the buy instruction).
const tx = await floor.buildTrade({ launch: launch.ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: (q.amountOut * 99n) / 100n, trader });
if (tx.family === 'solana') await sendSolanaTransactions(wallet, tx, rpc);
```

A sell-back before graduation pays `floor(tokens · price_x64 / 2^64)` at the launch's fixed sale price, less the FLOOR
Platform fee, the same for any seller; `quote(ref, 'sell', amount, { trader })` reads the trader's position only to
check that it covers `amount`. On a wSOL-quoted launch the payout is lamports by default; `sellBackPayout: 'wsol'`
in the adapter options pays it as wSOL into the seller's SPL Token native account instead (`sell_back_wsol`,
`buildSellBackWsol`), so a route can continue from it.
Amounts are bigints in base units (lamports, token base units); a mint address is never case-folded.

`floorPriceX18` on a graduated launch is the published floor in WHOLE quote per WHOLE token, × 1e18, read from the pool's
`FloorPool.floorTick`. A launch whose sale has not opened reads `phase: 'pending'`.

## Wallet prompts per action

`buildTrade` returns every transaction one action needs, in send order, in one `UnsignedTx`
(`{ family: 'solana', cluster, transactions: base64[] }`). `sendSolanaTransactions(wallet, tx, rpc, opts?)` signs them
together, then sends them in order, waiting for each confirmation before the next; the first failure stops the run and
names the step.

| Action | Transactions | Prompts (batch-signing wallet) |
|---|---|---|
| Swap on a graduated pool | 1 | 1 |
| Sell-back during the sale | 1 | 1 |
| Sale buy | 1, or 2 while the launch is not prepared (`prepare`, `buy`) | 1 |
| First trade with a referral link | +1 (`link` first) | 1 |

How the wallet is asked, in order of preference: the Wallet Standard `solana:signTransaction` feature with every
transaction in one call, a legacy `signAllTransactions`, else `signTransaction` once per transaction.

Caveats:

- Signing the transactions one by one costs one prompt each; a wallet without a batch call falls back to that.
- After a failure, re-plan (`buildTrade` again): the steps that landed changed the chain state. The run stops at the
  first failure, so a failed referral `link` step means the trade was not sent; the rebuilt list carries no `link` once
  the trader is linked.
- The transactions share one recent blockhash, valid for roughly 60–90 seconds. Pass `opts.replan` to rebuild the steps
  that have not landed when it expires; without it the run stops with reason `expired`. `TradeParams.deadline` is not
  used on Solana: the blockhash is the deadline.
- A wallet that simulates each transaction before signing cannot simulate a step whose accounts an earlier step creates
  (a lookup table that does not exist yet, the pool `prepare` makes): it shows that step as failing, and it lands once
  the earlier step has.
- The referral `link` needs a FLOOR attestor's Ed25519 signature, which a FLOOR server provides (`traderLink`); the SDK
  cannot make it. An attestation whose deadline is less than `TRADER_LINK_DEADLINE_MARGIN_SECONDS` (60 s) away is not
  built (the trade goes without it), since it would likely expire before it lands. The clock is the adapter's `now`
  option (default: this machine's).
- `listLaunches` pages by registry index, newest first; the cursor is the index to continue below, so a page can hold
  fewer than `limit` launches while `cursor` is not `null`. A launch whose `Launch` or quote mint cannot be read is left
  out.
- A sale trade is compiled through the launch's own lookup table, whose address is not derivable from chain state:
  `launchTable` supplies it (the launchpad records it when the launch is created).

## Quoter wasm

`wasm/floor_quote_bg.wasm` is `rust/crates/floor-quote-wasm` in this repository (`floor-quote`, with Carpenter's walk
and FLOOR's hook callbacks) compiled to wasm and bound with `wasm-bindgen --target web`. It is committed, with its glue,
so an install needs no Rust. `WASM_SHA256` is its hash. Load it with `fetch` (or read the file in Node) and pass the
bytes to `initQuote`, never through a bundler import.

`npm run build-wasm` rebuilds it from source (`scripts/build-wasm.sh` at the repository root: Rust 1.98.1 with the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.128) and copies it in; `npm run check-wasm` rebuilds it and
fails unless the bytes equal the committed ones. The build is reproducible: paths are remapped and doc comments are
removed from the build copy, so the output does not depend on where the checkout lives.

## Develop

From the repository root:

```sh
npm install
npm run generate -w @floorlaunchpad/sdk-solana     # idl/ + deployments.json → src/generated/ (Codama clients)
npm run build-wasm -w @floorlaunchpad/sdk-solana   # the quoter wasm, from source → wasm/
npm run typecheck -w @floorlaunchpad/sdk-solana
npm test -w @floorlaunchpad/sdk-solana
npm pack -w @floorlaunchpad/sdk-solana             # builds dist/ first (prepack)
```

Some tests compare the SDK with the Rust program source; they run when `FLOOR_PROGRAM_SOURCE` names a checkout of it and
are skipped by name otherwise.

## License

Apache-2.0.
