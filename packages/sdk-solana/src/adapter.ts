/**
 * `createSolanaAdapter`: the `@floorlaunchpad/sdk` `FloorChainAdapter` for FLOOR launches on Solana clusters.
 *
 * - `getLaunch` / `listLaunches` read the `Launch` account (and, once graduated, its `FloorPool`) and the mint's
 *   Token-2022 metadata.
 * - `quote` asks the wasm quoter: the sale's own arithmetic while the launch is bonding (`saleBuy` /
 *   `saleSellBack`), the AMM walk with the FLOOR hook's callbacks once graduated (`quoteSwap`).
 * - `buildTrade` returns EVERY transaction one trade needs, in send order, in ONE `UnsignedTx`: `[link?, prepare?, trade]`.
 *   `link` rides first when `traderLink` returns an attestation for a trader with no link yet (and the attestation is not
 *   about to expire); `prepare` rides in front of a sale buy while the launch's graduation accounts are not prepared.
 *   Sign the list in one wallet prompt and send it with `sendSolanaTransactions` (re-exported from `@floorlaunchpad/carpenter-sdk`),
 *   which sends in order and STOPS at the first failure: a failed `link` step means the trade is not sent, and the caller
 *   builds the trade again (the rebuilt list carries no link once the trader is linked, or no attestation is served).
 * - `TradeParams.deadline` is not used: a Solana transaction expires with its recent blockhash (~60–90 s).
 *
 * ⛔ PROGRAM VERSION. This SDK builds for the FLOOR program set whose hook callbacks take 4 accounts
 *    (`FLOOR_CALLBACK_ACCOUNTS`), whose sale sell-back is stateless and which has `sell_back_wsol`. Before building any
 *    trade the adapter reads `floor_hook`'s `ApprovedHook` record on the AMM, and on a graduated pool the pool's own
 *    `hook_callback_accounts`, and refuses (`program_version`) when either is not 4. The check is on the hook record:
 *    `floor_launch` is upgraded with it, but a cluster running an older `floor_launch` beside a newer `floor_hook` is not
 *    detected here (an older `floor_launch` lacks `sell_back_wsol`, and its sell-back pays at least the stateless quote).
 *
 * Every transaction is a v0 message compiled through the cluster's deployment lookup table and, for a sale trade, the
 * launch's own lookup table (a sale buy does not fit the 1,232-byte packet without it). The launch table's address is
 * not derivable from chain state, so a sale trade needs `launchTable` (the launchpad's API records it).
 *
 * ⛔ The fee list is the trader's WHOLE take, all six parts (LP fee, floor skim, creator fee, holder fee, FLOOR Platform
 *    fee, Carpenter Platform fee), zero where a part does not apply — never a partial list.
 * ⛔ Amounts are bigints in base units; a mint is never case-folded.
 */

import type { Amount, ChainRef, FeePart, FloorChainAdapter, Launch as FloorLaunch, LaunchPhase, LaunchRef, Quote, Side, SolanaCluster, TradeParams, UnsignedTx } from '@floorlaunchpad/sdk';
import { chainKey, solanaRef } from '@floorlaunchpad/sdk';
import {
  compileSendable,
  decodeLookupTable,
  decodeMint,
  findAta,
  findPosition,
  fromKitRpc,
  getApprovedHook,
  getAtaExtensions,
  getPool,
  getSqrtPriceAtTick,
  rentFromRpc,
  WSOL_DECIMALS,
  WSOL_MINT,
  type AccountSource,
  type LookupTableContents,
  type RawAccount,
  type SolanaRpc,
} from '@floorlaunchpad/carpenter-sdk';
import { address, getBase64EncodedWireTransaction, type Address, type GetLatestBlockhashApi, type Rpc } from '@solana/kit';

import { FLOOR_CALLBACK_ACCOUNTS, LAUNCH_STATE_BONDING, LAUNCH_STATE_GRADUATED, TOKEN_DECIMALS } from './constants.ts';
import { deploymentFor, type ClusterDeployment } from './deployments.ts';
import { decodeFloorPool, launchQuoteMint, type FloorPool, type Launch } from './layouts.ts';
import { findBuyerPosition, findFloorPool, findHookAuthority, findLaunch, findTraderLink, type ProgramIds } from './pda.ts';
import { graduationAccounts } from './predict.ts';
import { QuoteError, type Quoter, type SwapQuote } from './quote/quoter.ts';
import { readRawOwned, readSwapQuoteAccounts, programIdsFromDeployment, type SwapQuoteAccounts } from './adapters.ts';
import { getFloorPool, getLaunch, getLaunchConfig, listLaunches } from './stateView.ts';
import { buildSellBack, buildSellBackWsol, buildSwap, planBuy } from './tx/builders.ts';
import type { LaunchTableState } from './tx/launchAlt.ts';
import { buildLinkTrader, type TraderLinkAttestation } from './tx/traderRef.ts';

/** The RPC methods the adapter calls. */
export type SolanaAdapterRpc = SolanaRpc & Rpc<GetLatestBlockhashApi>;

export interface SolanaClusterConfig {
  readonly rpc: SolanaAdapterRpc;
  /**
   * The `deployments.json` key of this cluster (`solana:<genesis32>` or `local`). Defaults to the cluster's CAIP-2 key for
   * `mainnet` and `devnet` (`MAINNET_KEY`, `DEVNET_KEY`) and to `local` for `local`; a key with no row in the shipped
   * files throws by name.
   */
  readonly deployment?: string;
  /**
   * A joined deployment row to use instead of the shipped files' (a cluster this package version does not list yet).
   * Every program id, the deployment lookup table and the quote records then come from it.
   */
  readonly row?: ClusterDeployment;
  /** Override the deployment row's program ids (tests, a redeployed cluster). */
  readonly ids?: ProgramIds;
}

export interface SolanaAdapterOptions {
  readonly clusters: Partial<Record<SolanaCluster, SolanaClusterConfig>>;
  /** The quoter over `floor_quote_bg.wasm` (`initQuote` from `@floorlaunchpad/sdk-solana/quote/wasm`, or `createQuoter(fns)`). */
  readonly quoter: Quoter;
  /** The launch's own lookup table, by mint (the launchpad records it at creation). Required for sale trades only. */
  readonly launchTable?: (ref: LaunchRef) => Promise<string | null>;
  /** The quote asset's symbol for a non-wSOL quote mint, when the caller knows it (otherwise read from its metadata). */
  readonly quoteSymbol?: (mint: string) => string | undefined;
  /**
   * A FLOOR attestor's signed referral link for `trader` (FLOOR's API serves it), or `null` for none. Called only for a
   * trader with no on-chain link yet; a returned attestation puts a `link_trader` transaction first in `buildTrade`'s
   * list. The SDK cannot make the signature itself.
   */
  readonly traderLink?: (ref: LaunchRef, trader: string) => Promise<TraderLinkAttestation | null>;
  /**
   * How a bonding-sale sell-back on a wSOL-quoted launch pays out: `'native'` (default, `sell_back`: lamports to the
   * seller) or `'wsol'` (`sell_back_wsol`: wSOL into the seller's SPL Token native ATA, created if missing). The payout
   * and the fee are the same either way. A launch quoted in another token always pays that token.
   */
  readonly sellBackPayout?: 'native' | 'wsol';
  /**
   * The clock `traderLink` attestations are checked against, in unix seconds (default: this machine's clock). A link
   * whose deadline is less than {@link TRADER_LINK_DEADLINE_MARGIN_SECONDS} away is not built: it would likely expire
   * before it lands and, failing, stop the batch before the trade.
   */
  readonly now?: () => number;
}

/** A `traderLink` attestation that expires within this many seconds is skipped (the trade goes without the link). */
export const TRADER_LINK_DEADLINE_MARGIN_SECONDS = 60;

/** What `getLaunch` keeps on `raw`: the decoded accounts. */
export interface SolanaLaunchRaw {
  readonly launch: Launch;
  readonly floorPool: FloorPool | null;
  readonly slot: bigint;
}

/** The devnet CAIP-2 id (`solana:` + the first 32 characters of the genesis hash). */
export const DEVNET_KEY = 'solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1';
/** The mainnet-beta CAIP-2 id. Its row is added to the deployment files when the programs are deployed there. */
export const MAINNET_KEY = 'solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp';
const DEFAULT_KEYS: Partial<Record<SolanaCluster, string>> = { mainnet: MAINNET_KEY, devnet: DEVNET_KEY, local: 'local' };
const SIX: readonly FeePart['kind'][] = ['lp', 'floorSkim', 'creator', 'holder', 'floorPlatform', 'carpenterPlatform'];
const X18 = 10n ** 18n;

function feeList(parts: Partial<Record<FeePart['kind'], bigint>>): FeePart[] {
  return SIX.map((kind) => ({ kind, amount: parts[kind] ?? 0n }));
}

export class SolanaAdapterError extends Error {
  constructor(readonly code: 'no_cluster' | 'not_found' | 'not_tradeable' | 'needs_trader' | 'needs_launch_table' | 'program_version', message: string) {
    super(message);
    this.name = 'SolanaAdapterError';
  }
}

interface Ctx {
  readonly cluster: SolanaCluster;
  readonly cfg: SolanaClusterConfig;
  readonly src: AccountSource;
  readonly row: ClusterDeployment;
  readonly ids: ProgramIds;
}

export function createSolanaAdapter(opts: SolanaAdapterOptions): FloorChainAdapter {
  const ctxs = new Map<SolanaCluster, Ctx>();
  for (const [cluster, cfg] of Object.entries(opts.clusters) as [SolanaCluster, SolanaClusterConfig][]) {
    const key = cfg.deployment ?? DEFAULT_KEYS[cluster] ?? null;
    if (!cfg.row && !key) throw new SolanaAdapterError('no_cluster', `createSolanaAdapter: no deployment row is known for ${cluster}; pass clusters.${cluster}.deployment`);
    const row = cfg.row ?? deploymentFor(key!);
    ctxs.set(cluster, { cluster, cfg, src: fromKitRpc(cfg.rpc), row, ids: cfg.ids ?? programIdsFromDeployment(row) });
  }
  const ctxOf = (chain: ChainRef): Ctx => {
    const c = chain.family === 'solana' ? ctxs.get(chain.cluster) : undefined;
    if (!c) throw new SolanaAdapterError('no_cluster', `no Solana cluster configured for ${chainKey(chain)}`);
    return c;
  };

  async function readLaunch(ctx: Ctx, ref: LaunchRef) {
    const mint = address(ref.token);
    const { slot, value } = await getLaunch(ctx.src, mint, ctx.ids);
    if (!value) throw new SolanaAdapterError('not_found', `no FLOOR launch for mint ${ref.token} on ${chainKey(ref.chain)}`);
    const floorPool = value.state === LAUNCH_STATE_GRADUATED ? (await getFloorPool(ctx.src, value.ammPool, ctx.ids)).value : null;
    return { slot, launch: value, floorPool };
  }

  /**
   * The shared `Launch` from the decoded accounts and the mint's (and a token quote's) account. ⛔ A token-quoted launch
   * whose quote mint was not read is refused `not_found`: its decimals price the floor, and no default is right.
   */
  function toShared(ref: LaunchRef, raw: SolanaLaunchRaw, mintAccount: RawAccount | null, quoteAccount: RawAccount | null): FloorLaunch<SolanaLaunchRaw> {
    const l = raw.launch;
    const quoteMint = launchQuoteMint(l);
    const isWsol = quoteMint === WSOL_MINT;
    if (!isWsol && !quoteAccount) throw new SolanaAdapterError('not_found', `the quote mint ${quoteMint} of ${ref.token} could not be read`);
    const mintInfo = mintAccount ? decodeMint(mintAccount.data) : null;
    const quoteInfo = isWsol || !quoteAccount ? null : decodeMint(quoteAccount.data);
    const quoteDecimals = quoteInfo ? quoteInfo.decimals : WSOL_DECIMALS;
    const phase: LaunchPhase = l.state === LAUNCH_STATE_BONDING ? 'bonding' : l.state === LAUNCH_STATE_GRADUATED ? 'graduated' : 'pending';
    let floorPriceX18: bigint | null = null;
    if (raw.floorPool && phase === 'graduated') {
      // On a FLOOR pool currency 0 is the quote and currency 1 the base, so a tick prices base per quote; the floor in
      // quote per base is its inverse. Whole units: × 10^(base decimals − quote decimals).
      const sqrt = getSqrtPriceAtTick(raw.floorPool.floorTick);
      floorPriceX18 = (X18 * (1n << 128n) * 10n ** BigInt(TOKEN_DECIMALS)) / (sqrt * sqrt * 10n ** BigInt(quoteDecimals));
    }
    return {
      ref,
      name: mintInfo?.tokenMetadata?.name ?? '',
      symbol: mintInfo?.tokenMetadata?.symbol ?? '',
      decimals: mintInfo?.decimals ?? TOKEN_DECIMALS,
      totalSupply: mintInfo?.supply ?? 0n,
      quote: isWsol
        ? { address: null, symbol: 'SOL', decimals: WSOL_DECIMALS }
        : { address: quoteMint, symbol: opts.quoteSymbol?.(quoteMint) ?? quoteInfo?.tokenMetadata?.symbol ?? '', decimals: quoteDecimals },
      phase,
      mode: l.mode,
      creator: l.creator,
      floorPriceX18,
      raw,
    };
  }

  /** `getMultipleAccounts` in chunks of 100 (the RPC's per-call limit), in order. */
  async function readMany(ctx: Ctx, addrs: readonly Address[]): Promise<(RawAccount | null)[]> {
    const out: (RawAccount | null)[] = [];
    for (let i = 0; i < addrs.length; i += 100) out.push(...(await ctx.src.getMultipleAccounts(addrs.slice(i, i + 100))).accounts);
    return out;
  }

  /** The quote mint's token program, or `undefined` for wSOL; `not_found` when a token quote's mint is not on chain. */
  async function quoteSpecOf(ctx: Ctx, l: Launch): Promise<{ mint: Address; tokenProgram: Address } | undefined> {
    const quoteMint = launchQuoteMint(l);
    if (quoteMint === WSOL_MINT) return undefined;
    const { account } = await ctx.src.getAccount(quoteMint);
    if (!account) throw new SolanaAdapterError('not_found', `the quote mint ${quoteMint} of ${l.mint} could not be read`);
    return { mint: quoteMint, tokenProgram: account.owner };
  }

  /**
   * Refuse a cluster whose FLOOR programs predate this SDK: `floor_hook`'s `ApprovedHook.callback_accounts` (and, given,
   * the pool's own copy) must be {@link FLOOR_CALLBACK_ACCOUNTS}.
   */
  async function assertProgramSet(ctx: Ctx, pool?: { hookCallbackAccounts: number }): Promise<void> {
    const hook = (await getApprovedHook(ctx.src, ctx.ids.floorHook, ctx.ids)).value;
    if (!hook) throw new SolanaAdapterError('program_version', `floor_hook ${ctx.ids.floorHook} is not an approved hook of carpenter_amm ${ctx.ids.carpenterAmm} on ${ctx.cluster}`);
    if (hook.callbackAccounts !== FLOOR_CALLBACK_ACCOUNTS) {
      throw new SolanaAdapterError(
        'program_version',
        `floor_hook on ${ctx.cluster} takes ${hook.callbackAccounts} callback accounts; this SDK builds for the program set that takes ${FLOOR_CALLBACK_ACCOUNTS}`,
      );
    }
    if (pool && pool.hookCallbackAccounts !== FLOOR_CALLBACK_ACCOUNTS) {
      throw new SolanaAdapterError(
        'program_version',
        `the pool was created with ${pool.hookCallbackAccounts} hook callback accounts; this SDK builds for ${FLOOR_CALLBACK_ACCOUNTS}`,
      );
    }
  }

  /**
   * A sale buy's graduation list (what a graduating buy creates) and its rent, crediting the accounts that already exist:
   * `maxGraduationLamports` is the bound the buy instruction carries.
   */
  async function graduationOf(ctx: Ctx, l: Launch, quote: { mint: Address; tokenProgram: Address } | undefined) {
    const fp = (await getFloorPool(ctx.src, l.ammPool, ctx.ids)).value;
    if (!fp) throw new SolanaAdapterError('not_found', `no FloorPool for ${l.mint}`);
    const hooked = l.holderBuy > 0 || l.holderSell > 0;
    const rent = rentFromRpc(ctx.cfg.rpc);
    const args = { launch: l, rent, floorPool: fp, hooked, ...(quote ? { quote } : {}), ids: ctx.ids };
    const probe = await graduationAccounts(args);
    const accs = probe.accounts.map((m) => m.address);
    const got = await readMany(ctx, accs);
    const existing = new Set<Address>(accs.filter((_, i) => got[i] !== null));
    return { floorPool: fp, hooked, rent, existing, graduation: await graduationAccounts({ ...args, existing }) };
  }

  async function launchTableOf(ctx: Ctx, ref: LaunchRef): Promise<LaunchTableState> {
    const at = opts.launchTable ? await opts.launchTable(ref) : null;
    if (!at) throw new SolanaAdapterError('needs_launch_table', `a sale trade of ${ref.token} needs its launch lookup table: pass createSolanaAdapter({ launchTable })`);
    const { account } = await ctx.src.getAccount(address(at));
    if (!account) throw new SolanaAdapterError('needs_launch_table', `no lookup table at ${at}`);
    return { address: address(at), ...decodeLookupTable(account.data) };
  }

  async function deploymentTables(ctx: Ctx): Promise<LookupTableContents[]> {
    if (!ctx.row.alt) return [];
    const { account } = await ctx.src.getAccount(address(ctx.row.alt));
    return account ? [{ address: address(ctx.row.alt), addresses: decodeLookupTable(account.data).addresses }] : [];
  }

  async function swapQuote(ctx: Ctx, l: Launch, side: Side, amountIn: bigint): Promise<SwapQuote> {
    let starts: number[] | null = null;
    for (let attempt = 0; attempt < 8; attempt++) {
      const v: SwapQuoteAccounts = (await readSwapQuoteAccounts(ctx.src, l.ammPool, { radius: 4, ...(starts ? { starts } : {}), ids: ctx.ids })).value;
      try {
        return opts.quoter.quoteSwap({ poolKey: v.poolKey, pool: v.pool, bitmap: v.bitmap, tickArrays: v.tickArrays, floorPool: v.floorPool, supply1: v.supply1, zeroForOne: side === 'buy', amountSpecified: -amountIn });
      } catch (e) {
        if (!(e instanceof QuoteError) || e.startIndex === null) throw e;
        starts = [...new Set([...v.tickArrayStarts, e.startIndex])];
      }
    }
    throw new SolanaAdapterError('not_tradeable', `the walk on ${l.ammPool} did not settle within the tick arrays read`);
  }

  async function quoteRaw(ctx: Ctx, ref: LaunchRef, side: Side, amountIn: Amount, trader?: string): Promise<Quote> {
    const { launch: l } = await readLaunch(ctx, ref);
    if (l.state === LAUNCH_STATE_BONDING) {
      const launchBytes = (await readRawOwned(ctx.src, (await findLaunch(l.mint, ctx.ids)).address, ctx.ids.floorLaunch)).value;
      if (!launchBytes) throw new SolanaAdapterError('not_found', `no Launch account for ${ref.token}`);
      if (side === 'buy') {
        const s = opts.quoter.saleBuy(launchBytes, amountIn);
        const base: Quote = { launch: ref, side, amountIn, amountOut: s.tokens, fees: feeList({ floorPlatform: s.teamFee }), feesExact: true, graduates: s.graduates, ...(s.spent < amountIn ? { refund: amountIn - s.spent } : {}) };
        if (!s.graduates) return base;
        // A graduating buy also pays the rent of what graduation creates, at most `max_graduation_lamports` (lamports on
        // every quote asset).
        const { graduation } = await graduationOf(ctx, l, await quoteSpecOf(ctx, l));
        return { ...base, maxExtraNative: graduation.maxGraduationLamports };
      }
      // The payout is the launch's fixed price for `amountIn`, the same for any seller; the position is read only to
      // check that it covers `amountIn`.
      if (!trader) throw new SolanaAdapterError('needs_trader', 'a sell-back before graduation is checked against the seller\'s position: pass opts.trader');
      const pos = (await readRawOwned(ctx.src, (await findBuyerPosition((await findLaunch(l.mint, ctx.ids)).address, address(trader), ctx.ids)).address, ctx.ids.floorLaunch)).value;
      if (!pos) throw new SolanaAdapterError('not_found', `${trader} holds no position in the sale of ${ref.token}`);
      const s = opts.quoter.saleSellBack(launchBytes, pos, amountIn);
      return { launch: ref, side, amountIn, amountOut: s.received, fees: feeList({ floorPlatform: s.teamFee }), feesExact: true };
    }
    if (l.state !== LAUNCH_STATE_GRADUATED) throw new SolanaAdapterError('not_tradeable', `${ref.token} is not trading (state ${l.state})`);
    return swapToQuote(ref, side, await swapQuote(ctx, l, side, amountIn));
  }

  return {
    chains: [...ctxs.keys()].map((c) => solanaRef(c)),

    async getLaunch(ref) {
      const ctx = ctxOf(ref.chain);
      const raw = await readLaunch(ctx, ref);
      const quoteMint = launchQuoteMint(raw.launch);
      const accs = await readMany(ctx, quoteMint === WSOL_MINT ? [raw.launch.mint] : [raw.launch.mint, quoteMint]);
      return toShared(ref, raw, accs[0] ?? null, accs[1] ?? null);
    },

    /**
     * Newest first, `limit` registry indices per page (default 50). The cursor is the registry index to continue BELOW, so
     * a page whose entries are missing or unreadable still advances; a page can therefore hold fewer than `limit` launches
     * while `cursor` is not `null`. ⛔ Unfiltered: anyone can create a launch with any name and symbol. A row whose
     * `Launch` or (token quote) quote mint cannot be read is left out.
     */
    async listLaunches(chain, o) {
      const ctx = ctxOf(chain);
      const key = ctx.row.chainKey;
      if (!key) throw new SolanaAdapterError('no_cluster', `listLaunches needs a cluster with a CAIP-2 key; ${ctx.cluster} has none`);
      const before = o?.cursor ? BigInt(o.cursor) : undefined;
      const limit = o?.limit ?? 50;
      const page = await listLaunches(ctx.src, key, { limit, withState: true, ...(before !== undefined ? { before } : {}) }, ctx.ids);
      const rows = page.value.filter((r): r is typeof r & { state: Launch } => r.state !== null);
      // One batched read for every row's FloorPool (graduated only), mint and token quote mint.
      const floorPoolAddrs = await Promise.all(rows.map(async (r) => (r.state.state === LAUNCH_STATE_GRADUATED ? (await findFloorPool(r.state.ammPool, ctx.ids)).address : null)));
      const want: Address[] = [];
      const at = new Map<Address, number>();
      const need = (a: Address) => {
        if (!at.has(a)) {
          at.set(a, want.length);
          want.push(a);
        }
      };
      rows.forEach((r, i) => {
        need(r.state.mint);
        const q = launchQuoteMint(r.state);
        if (q !== WSOL_MINT) need(q);
        const fp = floorPoolAddrs[i];
        if (fp) need(fp);
      });
      const got = await readMany(ctx, want);
      const get = (a: Address) => got[at.get(a)!] ?? null;
      const launches: FloorLaunch[] = [];
      rows.forEach((r, i) => {
        const fpAddr = floorPoolAddrs[i];
        const fpAcc = fpAddr ? get(fpAddr) : null;
        const floorPool = fpAcc && fpAcc.owner === ctx.ids.floorHook ? decodeFloorPool(fpAcc.data) : null;
        const q = launchQuoteMint(r.state);
        const quoteAcc = q === WSOL_MINT ? null : get(q);
        if (q !== WSOL_MINT && !quoteAcc) return;
        launches.push(toShared({ chain, token: r.mint }, { launch: r.state, floorPool, slot: page.slot }, get(r.state.mint), quoteAcc));
      });
      return { launches, cursor: page.nextBefore !== null ? page.nextBefore.toString() : null };
    },

    async quote(ref, side, amountIn, o) {
      return quoteRaw(ctxOf(ref.chain), ref, side, amountIn, o?.trader);
    },

    async buildTrade(p: TradeParams): Promise<UnsignedTx> {
      const ctx = ctxOf(p.launch.chain);
      await assertProgramSet(ctx);
      const { launch: l, floorPool } = await readLaunch(ctx, p.launch);
      const trader = address(p.trader);
      const { value: blockhash } = await ctx.cfg.rpc.getLatestBlockhash({ commitment: 'confirmed' }).send();
      const lifetime = { blockhash: blockhash.blockhash, lastValidBlockHeight: blockhash.lastValidBlockHeight };
      const tables = await deploymentTables(ctx);
      const quote = await quoteSpecOf(ctx, l);
      const hooked = l.holderBuy > 0 || l.holderSell > 0;
      const wire = (c: { transaction: Parameters<typeof getBase64EncodedWireTransaction>[0] }) => getBase64EncodedWireTransaction(c.transaction);

      // The referral link, when the trader has none and the caller has an attestation that is not about to expire: its
      // own transaction, first. A link that fails on chain stops the batch (the trade is not sent).
      let linked = (await ctx.src.getAccount((await findTraderLink(trader, ctx.ids)).address)).account !== null;
      const pre: string[] = [];
      if (!linked && opts.traderLink) {
        const attestation = await opts.traderLink(p.launch, p.trader);
        const now = BigInt(Math.floor(opts.now ? opts.now() : Date.now() / 1000));
        if (attestation && attestation.deadline >= now + BigInt(TRADER_LINK_DEADLINE_MARGIN_SECONDS)) {
          const config = (await getLaunchConfig(ctx.src, ctx.ids)).value;
          if (!config) throw new SolanaAdapterError('not_found', 'no LaunchConfig on this cluster');
          const link = await buildLinkTrader({ trader, attestor: config.attestor, cluster: config.cluster, attestation, ids: ctx.ids });
          pre.push(wire(compileSendable('buildTrade/link', { feePayer: trader, instructions: link.instructions, lifetime, lookupTables: tables })));
          linked = true;
        }
      }
      const out = (txs: string[]): UnsignedTx => ({ family: 'solana', cluster: ctx.cluster, transactions: [...pre, ...txs] });
      // A linked trader's wSOL bucket exists (the link opens it), so a wSOL sale trade carries the referral prefix.
      const traderRef = linked && !quote ? { traderRef: true as const } : {};

      if (l.state === LAUNCH_STATE_BONDING) {
        const launchAlt = await launchTableOf(ctx, p.launch);
        const lookupTables = [...tables, launchAlt];
        if (p.side === 'buy') {
          // Which graduation accounts already exist decides whether the buy needs the prepare step first.
          const g = await graduationOf(ctx, l, quote);
          const plan = await planBuy({
            buyer: trader, launch: l, lamportsIn: p.amountIn, minTokens: p.minAmountOut, rent: g.rent, floorPool: g.floorPool,
            ...(quote ? { quote } : {}), ...(hooked ? { hooked: true, touch: {} } : {}), ...traderRef, ids: ctx.ids,
            existing: g.existing, launchAlt, lookupTables: tables, lifetime,
          });
          return out(plan.steps.map((s) => wire(s.compiled!)));
        }
        const ata = (await findAta(trader, l.mint, ctxTokenProgram())).address;
        const ext = (await getAtaExtensions(ctx.src, ata)).value;
        const sellArgs = {
          buyer: trader, launch: l, tokens: p.amountIn, buyerAta: { cpiGuardLocked: ext?.cpiGuardLocked ?? false, delegatedAmount: ext?.delegatedAmount ?? 0n },
          ...(hooked ? { holder: { floorPool: l.floorPool } } : {}), ...traderRef, ids: ctx.ids,
        };
        const tx = !quote && opts.sellBackPayout === 'wsol' ? await buildSellBackWsol(sellArgs) : await buildSellBack({ ...sellArgs, ...(quote ? { quote } : {}) });
        return out([wire(compileSendable('buildTrade/sellBack', { feePayer: trader, instructions: tx.instructions, lifetime, lookupTables }))]);
      }
      if (l.state !== LAUNCH_STATE_GRADUATED || !floorPool) throw new SolanaAdapterError('not_tradeable', `${p.launch.token} is not trading (state ${l.state})`);
      const poolState = (await getPool(ctx.src, l.ammPool, ctx.ids)).value;
      if (!poolState) throw new SolanaAdapterError('not_found', `no pool at ${l.ammPool}`);
      await assertProgramSet(ctx, poolState);
      const q = await swapQuote(ctx, l, p.side, p.amountIn);
      // The trader's walk and every hook action's walk; the actions' own arrays are created first if missing.
      const starts = [...new Set([...q.arraysTouched, ...q.actionArrays])];
      const hookAuthority = (await findHookAuthority(ctx.ids.floorHook)).address;
      const salts = [...new Set(q.replaceActions.flatMap((a) => (a.kind === 'modify_position' ? [a.salt] : [])))];
      const actionPositions = await Promise.all(salts.map(async (s) => (await findPosition(l.ammPool, hookAuthority, s, ctx.ids)).address));
      const zeroForOne = p.side === 'buy';
      const tokenIn = zeroForOne ? undefined : (await findAta(trader, l.mint, ctxTokenProgram())).address;
      const ext = tokenIn ? (await getAtaExtensions(ctx.src, tokenIn)).value : null;
      const tx = await buildSwap({
        trader, pool: l.ammPool, poolState, zeroForOne, amountSpecified: -p.amountIn, amountLimit: p.minAmountOut,
        tickArrayStarts: starts, ensureTickArrayStarts: q.actionArrays, actionPositions,
        ...(ext?.cpiGuardLocked ? { sourceHasCpiGuard: true } : {}),
        ...(hooked ? { transferHook: { floorPool: l.floorPool, ...(zeroForOne ? { openBuyerRecord: true } : {}) } } : {}),
        ids: ctx.ids,
      });
      return out([wire(compileSendable('buildTrade/swap', { feePayer: trader, instructions: tx.instructions, lifetime, lookupTables: tables }))]);
    },
  };
}

/** Every FLOOR launch mint is Token-2022. */
function ctxTokenProgram(): Address {
  return address('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');
}

/**
 * A graduated pool's quote as the shared `Quote`. On a buy every fee is in the quote asset. On a sell the LP fee is
 * charged on the INPUT (the token), so it is converted to quote at the walk's average price — `feesExact` is then false;
 * every other part is already in the quote asset.
 */
export function swapToQuote(ref: LaunchRef, side: Side, q: SwapQuote): Quote {
  const f = q.fees;
  for (const k of ['lp', 'skim', 'creator', 'holder', 'team', 'protocol'] as const) {
    if (typeof f[k] !== 'bigint') throw new SolanaAdapterError('not_tradeable', `the quoter returned no ${k} fee: the trader's take cannot be listed whole`);
  }
  let lp = f.lp;
  let exact = true;
  if (side === 'sell' && f.lp > 0n) {
    lp = q.amountIn > 0n ? (f.lp * q.amountOut) / q.amountIn : 0n;
    exact = false;
  }
  return {
    launch: ref,
    side,
    amountIn: q.amountIn,
    amountOut: q.amountOut,
    fees: feeList({ lp, floorSkim: f.skim, creator: f.creator, holder: f.holder, floorPlatform: f.team, carpenterPlatform: f.protocol }),
    feesExact: exact,
  };
}
