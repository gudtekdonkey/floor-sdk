/**
 * `createEvmAdapter` — `@floorsupply/sdk`'s `FloorChainAdapter` for EVM chains.
 *
 * One adapter serves any number of chains; each chain needs a FLOOR set in its deployment row (`floorDeploymentFor` refuses
 * the others by name at construction) and a transport. Reads go through viem `PublicClient`s; `build*` returns unsigned
 * transactions and never signs.
 */
import { createPublicClient, getAddress, http, type Address, type PublicClient, type Transport } from 'viem';
import type { Amount, ChainRef, FloorChainAdapter, LaunchRef, Quote, Side, TradeParams, UnsignedTx } from '@floorsupply/sdk';
import { V4_QUOTER_ABI } from './abi/index.js';
import { EVM_DEPLOYMENTS, checkDeployment, floorDeploymentFor, permit2For, rulesOf, swapRatchetOf, swapRouterOf, type Deployment } from './deployments.js';
import { FloorEvmError } from './errors.js';
import { assertWholeTake, bondingFeeParts, gapSkimBps, poolFeeParts, skimBpsAt } from './fees.js';
import { bandTicks, marketTick } from './pool.js';
import { launchOfToken, readCushion, readLaunchFromEntry, readRegistryCount, readRegistryPage, readSalePosition, readSlot0, REGISTRY_PAGE_MAX, type EvmLaunch, type RegistryEntry } from './reads.js';
import { quoteParamsForKey } from './router.js';
import { saleBuyQuote, sellBackQuote } from './sale.js';
import { BOND_BUY_GAS_FLOOR, SWAP_DEADLINE_SECONDS, type ApprovalAmount, bondBuyApprovals, buildBondBuy, buildPoolSwap, buildSellBack, buildSwapRatchet, evmTx, pendingApprovals, poolSwapApprovals } from './trade.js';

/** A quote with what an EVM integrator needs beside the shared shape. */
export interface EvmQuote extends Quote {
  /** `sale` before graduation (the bonding sale), `pool` after (the v4 pool). */
  readonly venue: 'sale' | 'pool';
  /**
   * True when every fee part is the contracts' own integer arithmetic (a bonding trade). False on a pool swap: `amountOut` is
   * the V4Quoter's simulation, but the parts are computed from the pool's rates at the PRE-trade tick — a sell's skim is
   * charged at the post-swap tick (a minimum here) and the LP fee is stated in the quote at the swap's own rate.
   */
  readonly feesExact: boolean;
  /** A bonding buy past what the sale has left: the quote the sale refunds. 0 otherwise. */
  readonly refund: Amount;
  /** True when this buy sells the sale out, so it graduates the launch in the same transaction. */
  readonly graduates: boolean;
}

export interface EvmAdapterOptions {
  /** The chain ids this adapter serves. Each must have a FLOOR set (in `deployments` or the committed table). */
  readonly chains: readonly number[];
  /** One transport per chain id. */
  readonly transports?: Readonly<Record<number, Transport>>;
  /** RPC URLs per chain id, used with viem's `http` transport where no `transports` / `clients` entry is given. */
  readonly rpcUrls?: Readonly<Record<number, string>>;
  /** Ready-made viem clients per chain id. Win over `transports` and `rpcUrls`. */
  readonly clients?: Readonly<Record<number, PublicClient>>;
  /** Extra or replacement deployment rows (a local Anvil set, a set not yet in the committed table), checked like the rest. */
  readonly deployments?: Readonly<Record<number, Deployment>>;
  /** The most registry entries `getLaunch` walks (newest first) to find a token's entry. Default 5,000 (ten pages of 500). */
  readonly maxRegistryScan?: number;
  /** The clock, in ms, for swap deadlines. Default `Date.now`. */
  readonly now?: () => number;
  /** The native asset's symbol per chain where `EVM_NETWORKS` does not name one. */
  readonly nativeSymbols?: Readonly<Record<number, string>>;
}

/** The adapter, with the EVM-only calls beside the shared interface. */
export interface EvmAdapter extends FloorChainAdapter {
  getLaunch(ref: LaunchRef): Promise<EvmLaunch>;
  listLaunches(chain: ChainRef, opts?: { readonly limit?: number; readonly cursor?: string }): Promise<{ readonly launches: readonly EvmLaunch[]; readonly cursor: string | null }>;
  /** A sell before graduation is a sell-back, at the sale price and bounded by the SELLER's position: pass `opts.trader`. */
  quote(ref: LaunchRef, side: Side, amountIn: Amount, opts?: { readonly trader?: string }): Promise<EvmQuote>;
  /** Alias of `quote(ref, 'sell', tokens, { trader: seller })` before graduation. */
  quoteSellBack(ref: LaunchRef, seller: string, tokens: Amount): Promise<EvmQuote>;
  /**
   * Every call the trade needs, in order: the approvals the trader has NOT yet granted (read now: the token's ERC-20
   * approval of the sale or of Permit2, Permit2's on-chain approval of the router), then the trade. Send it with
   * `sendEvmCalls` — one wallet prompt where the wallet takes an EIP-5792 batch.
   */
  buildTrade(params: TradeParams, opts?: TradeOptions): Promise<UnsignedTx>;
  /** The deployment row a chain is served from. */
  deploymentOf(chainId: number): Deployment;
  /** The viem client a chain is read through. */
  clientOf(chainId: number): PublicClient;
}

/** EVM-only choices for a trade. */
export interface TradeOptions {
  /** Swap deadline (unix seconds); overrides `TradeParams.deadline`. Default: now + 600 s. */
  readonly deadline?: bigint;
  /** Send a pool swap through the chain's `FloorSwapRatchet` (where its row has one), running a due ratchet in the same tx. */
  readonly swapRatchet?: { readonly ratchet: boolean; readonly sweep?: boolean };
  /**
   * How much each missing approval grants. Default `exact` for a bonding buy (the quote approved to the launch's own sale
   * contract), `unlimited` for a pool swap (Permit2 and the UniversalRouter / `FloorSwapRatchet`).
   */
  readonly approvalAmount?: ApprovalAmount;
}

const MAX_LIST = 100;

export function createEvmAdapter(options: EvmAdapterOptions): EvmAdapter {
  if (options.chains.length === 0) throw new FloorEvmError('bad_params', 'createEvmAdapter: no chains');
  const table: Record<number, Deployment> = { ...EVM_DEPLOYMENTS };
  for (const [k, d] of Object.entries(options.deployments ?? {})) table[Number(k)] = checkDeployment(Number(k), d);
  const deps = new Map<number, Deployment>();
  const clients = new Map<number, PublicClient>();
  for (const chainId of options.chains) {
    deps.set(chainId, floorDeploymentFor(chainId, table));
    const given = options.clients?.[chainId];
    if (given) { clients.set(chainId, given); continue; }
    const t = options.transports?.[chainId];
    const url = options.rpcUrls?.[chainId];
    if (!t && !url) throw new FloorEvmError('bad_params', `createEvmAdapter: chain ${chainId} has no client, transport or rpc URL`);
    clients.set(chainId, createPublicClient({ transport: t ?? http(url) }) as PublicClient);
  }
  const now = options.now ?? Date.now;
  const maxScan = BigInt(options.maxRegistryScan ?? 5_000);
  const entries = new Map<string, RegistryEntry>();
  const checkedChain = new Map<number, Promise<void>>();

  const chainIdOf = (chain: ChainRef): number => {
    if (chain.family !== 'evm') throw new FloorEvmError('bad_params', `the EVM adapter serves no ${chain.family} chain`);
    if (!deps.has(chain.chainId)) throw new FloorEvmError('bad_params', `the EVM adapter does not serve chain ${chain.chainId}`);
    return chain.chainId;
  };
  const deploymentOf = (chainId: number): Deployment => {
    const d = deps.get(chainId);
    if (!d) throw new FloorEvmError('bad_params', `the EVM adapter does not serve chain ${chainId}`);
    return d;
  };
  /** ⛔ The client must be on the chain it is used for: a registry address read on another chain is a different contract. */
  const clientOf = (chainId: number): PublicClient => {
    const c = clients.get(chainId);
    if (!c) throw new FloorEvmError('bad_params', `the EVM adapter does not serve chain ${chainId}`);
    return c;
  };
  const ready = (chainId: number): Promise<PublicClient> => {
    const c = clientOf(chainId);
    let p = checkedChain.get(chainId);
    if (!p) {
      p = c.getChainId().then((id) => {
        if (id !== chainId) throw new FloorEvmError('contract_mismatch', `the client for chain ${chainId} is connected to chain ${id}`);
      });
      p.catch(() => checkedChain.delete(chainId));
      checkedChain.set(chainId, p);
    }
    return p.then(() => c);
  };
  const entryKey = (chainId: number, token: string) => `${chainId}:${token.toLowerCase()}`;
  const remember = (chainId: number, list: readonly RegistryEntry[]) => { for (const e of list) entries.set(entryKey(chainId, e.token), e); };
  const nativeSymbol = (chainId: number) => options.nativeSymbols?.[chainId];

  async function entryFor(chainId: number, token: Address): Promise<RegistryEntry> {
    const hit = entries.get(entryKey(chainId, token));
    if (hit) return hit;
    const client = await ready(chainId), d = deploymentOf(chainId);
    await launchOfToken(client, d, token);
    const count = await readRegistryCount(client, d);
    let end = count, walked = 0n;
    while (end > 0n && walked < maxScan) {
      const start = end > REGISTRY_PAGE_MAX ? end - REGISTRY_PAGE_MAX : 0n;
      const page = await readRegistryPage(client, d, start, end - start);
      remember(chainId, page);
      const found = page.find((e) => e.token.toLowerCase() === token.toLowerCase());
      if (found) return found;
      walked += end - start;
      end = start;
    }
    throw new FloorEvmError('not_found', `${token} is registered on chain ${chainId}, but its entry is not among the newest ${walked} of ${count} (raise maxRegistryScan)`);
  }

  async function getLaunch(ref: LaunchRef): Promise<EvmLaunch> {
    const chainId = chainIdOf(ref.chain);
    const token = getAddress(ref.token);
    const e = await entryFor(chainId, token);
    return readLaunchFromEntry(await ready(chainId), deploymentOf(chainId), e, { nativeSymbol: nativeSymbol(chainId) });
  }

  async function listLaunches(chain: ChainRef, opts: { readonly limit?: number; readonly cursor?: string } = {}) {
    const chainId = chainIdOf(chain);
    const client = await ready(chainId), d = deploymentOf(chainId);
    const limit = opts.limit ?? 20;
    if (!Number.isInteger(limit) || limit < 1 || limit > MAX_LIST) throw new FloorEvmError('bad_params', `listLaunches: limit ${limit} is not in 1..${MAX_LIST}`);
    const count = await readRegistryCount(client, d);
    let end = count;
    if (opts.cursor !== undefined) {
      if (!/^[0-9]+$/.test(opts.cursor) || BigInt(opts.cursor) > count) throw new FloorEvmError('bad_params', `listLaunches: cursor ${opts.cursor} is not a registry position`);
      end = BigInt(opts.cursor);
    }
    const start = end > BigInt(limit) ? end - BigInt(limit) : 0n;
    const page = end > start ? await readRegistryPage(client, d, start, end - start) : [];
    remember(chainId, page);
    const read = await Promise.all([...page].reverse().map((e) => readLaunchFromEntry(client, d, e, { nativeSymbol: nativeSymbol(chainId) }).catch((err: unknown) => {
      /* A registry entry this hook does not serve is dropped (a reverted `poolForAccount`);
         a transport failure is not an answer and rejects the page. */
      if (err instanceof FloorEvmError && (err.code === 'not_a_launch' || err.code === 'contract_mismatch')) return null;
      throw err;
    })));
    return { launches: read.filter((l): l is EvmLaunch => l !== null), cursor: start > 0n ? start.toString() : null };
  }

  async function poolQuote(L: EvmLaunch, side: Side, amountIn: Amount): Promise<EvmQuote> {
    const chainId = L.raw.chainId, d = deploymentOf(chainId), client = await ready(chainId);
    if (!d.quoter) throw new FloorEvmError('no_quote', `chain ${chainId} records no V4Quoter: a pool trade cannot be priced`);
    const buying = side === 'buy';
    const zeroForOne = buying ? !L.raw.quoteIsCurrency1 : L.raw.quoteIsCurrency1;
    const [sim, slot0] = await Promise.all([
      client.simulateContract({ address: d.quoter, abi: V4_QUOTER_ABI, functionName: 'quoteExactInputSingle', args: [quoteParamsForKey(L.raw.poolKey, zeroForOne, amountIn)] }).catch((e: unknown) => {
        throw new FloorEvmError('no_quote', `the V4Quoter could not price ${side} ${amountIn} on pool ${L.raw.poolId}`, { cause: e });
      }),
      readSlot0(client, d.poolManager, L.raw.poolId),
    ]);
    const [amountOut] = sim.result;
    if (amountOut <= 0n) throw new FloorEvmError('no_quote', `the V4Quoter priced ${side} ${amountIn} at nothing`);
    const dearEdge = L.raw.floorTick - bandTicks(L.raw.poolState.tickSpacing);
    const current = marketTick(slot0.tick, L.raw.quoteIsCurrency1);
    let skim = skimBpsAt(side, dearEdge - current, L.raw.poolRates);
    /* The engine scales a buy's skim by the cushion gap on contracts that carry the rule (`_takeFees` → `_gapSkim`). */
    if (buying && rulesOf(d).gapSkim) skim = gapSkimBps(skim, await readCushion(client, L.raw.hook, L.raw.poolId), current);
    const fees = poolFeeParts(side, amountIn, amountOut, skim, L.raw.poolRates);
    assertWholeTake(fees);
    return { launch: L.ref, side, amountIn, amountOut, fees, venue: 'pool', feesExact: false, refund: 0n, graduates: false };
  }

  function saleBuy(L: EvmLaunch, amountIn: Amount): EvmQuote {
    const sale = L.raw.sale;
    if (!sale) throw new FloorEvmError('contract_mismatch', 'a bonding launch with no sale book');
    if (!sale.funded) throw new FloorEvmError('no_quote', `the sale ${L.raw.launch} is not funded yet`);
    const q = saleBuyQuote(sale, amountIn, L.decimals, rulesOf(deploymentOf(L.raw.chainId)).saleCost);
    if (q.got <= 0n) throw new FloorEvmError('no_quote', `the sale ${L.raw.launch} sells nothing for ${amountIn}`);
    const fees = bondingFeeParts(q.fee);
    assertWholeTake(fees);
    const paid = q.spent + q.fee;
    return { launch: L.ref, side: 'buy', amountIn: paid, amountOut: q.got, fees, venue: 'sale', feesExact: true, refund: amountIn - paid, graduates: q.got === q.maxGot };
  }

  async function sellBack(L: EvmLaunch, seller: Address, tokens: Amount): Promise<EvmQuote> {
    if (L.raw.sale === null) throw new FloorEvmError('bad_params', 'the launch has graduated: there is no sell-back, sell into the pool');
    const pos = await readSalePosition(await ready(L.raw.chainId), L.raw.launch, seller);
    const q = sellBackQuote(L.raw.sale.p0, pos.contributed, pos.allocation, tokens, L.raw.sale.teamFeeBps, rulesOf(deploymentOf(L.raw.chainId)).sellBack);
    if (!q) throw new FloorEvmError('bad_params', `${seller} can sell back at most ${pos.allocation} tokens of this sale (asked ${tokens})`);
    const fees = bondingFeeParts(q.fee);
    assertWholeTake(fees);
    return { launch: L.ref, side: 'sell', amountIn: tokens, amountOut: q.paid, fees, venue: 'sale', feesExact: true, refund: 0n, graduates: false };
  }

  async function quote(ref: LaunchRef, side: Side, amountIn: Amount, opts: { readonly trader?: string } = {}): Promise<EvmQuote> {
    if (amountIn <= 0n) throw new FloorEvmError('bad_params', 'amountIn must be > 0');
    const L = await getLaunch(ref);
    if (L.phase === 'graduated') return poolQuote(L, side, amountIn);
    if (side === 'buy') return saleBuy(L, amountIn);
    if (!opts.trader) throw new FloorEvmError('bad_params', 'a sell before graduation is a sell-back, bounded by the seller\'s position: pass opts.trader');
    return sellBack(L, getAddress(opts.trader), amountIn);
  }

  async function quoteSellBack(ref: LaunchRef, seller: string, tokens: Amount): Promise<EvmQuote> {
    return quote(ref, 'sell', tokens, { trader: seller });
  }

  function poolRoute(L: EvmLaunch, opts: TradeOptions) {
    const d = deploymentOf(L.raw.chainId);
    const router = swapRouterOf(d);
    if (!router) throw new FloorEvmError('bad_params', `chain ${L.raw.chainId} records no swap router`);
    if (router.kind !== 'universal') throw new FloorEvmError('bad_params', `chain ${L.raw.chainId}'s router is PoolSwapTest, which enforces no minimum received — refused`);
    const ratchet = opts.swapRatchet ? swapRatchetOf(d) : null;
    if (opts.swapRatchet && !ratchet) throw new FloorEvmError('bad_params', `chain ${L.raw.chainId} records no FloorSwapRatchet`);
    return { d, router, ratchet, permit2: permit2For(d, router) };
  }

  async function buildTrade(params: TradeParams, opts: TradeOptions = {}): Promise<UnsignedTx> {
    if (params.amountIn <= 0n) throw new FloorEvmError('bad_params', 'amountIn must be > 0');
    if (params.minAmountOut <= 0n) throw new FloorEvmError('bad_params', 'minAmountOut must be > 0');
    const trader = getAddress(params.trader);
    const L = await getLaunch(params.launch);
    const chainId = L.raw.chainId, client = await ready(chainId);
    if (L.phase !== 'graduated') {
      const q = params.side === 'buy' ? saleBuy(L, params.amountIn) : await sellBack(L, trader, params.amountIn);
      /* ⛔ The sale takes no minimum on chain: the minimum is checked HERE, against the book as just read. */
      if (q.amountOut < params.minAmountOut) throw new FloorEvmError('min_out_unmet', `the sale gives ${q.amountOut}, under the minimum ${params.minAmountOut}`);
      if (params.side === 'sell') return evmTx(chainId, [buildSellBack({ token: L.raw.token, tokens: params.amountIn })]);
      const buy = { launch: L.raw.launch, quote: L.raw.quoteToken, amountIn: params.amountIn };
      const missing = await pendingApprovals(client, trader, bondBuyApprovals(buy, opts.approvalAmount ?? 'exact'), now());
      /* Any buy while the sale is open can become the one that sells it out and graduates it inline (other buys may land
         first): every bonding buy carries the gas floor. */
      return evmTx(chainId, [...missing.map((s) => s.call), { ...buildBondBuy(buy), gas: BOND_BUY_GAS_FLOOR }]);
    }
    const { d, router, ratchet, permit2 } = poolRoute(L, opts);
    const deadline = opts.deadline ?? params.deadline ?? BigInt(Math.floor(now() / 1000)) + SWAP_DEADLINE_SECONDS;
    const args = {
      side: params.side, token: L.raw.token, hook: L.raw.hook, poolId: L.raw.poolId, quote: L.raw.quoteToken,
      quoteIsCurrency1: L.raw.quoteIsCurrency1, amountIn: params.amountIn, minOut: params.minAmountOut, deadline,
      router: { address: router.address, release: router.release },
    };
    const trade = ratchet && opts.swapRatchet
      ? buildSwapRatchet({ ...args, swapRatchet: ratchet, ratchet: opts.swapRatchet.ratchet, ...(opts.swapRatchet.sweep !== undefined ? { sweep: opts.swapRatchet.sweep } : {}) })
      : buildPoolSwap(args);
    const steps = poolSwapApprovals({ side: params.side, token: L.raw.token, quote: L.raw.quoteToken, amountIn: params.amountIn, spender: ratchet ?? router.address, permit2 }, opts.approvalAmount ?? 'unlimited');
    const missing = await pendingApprovals(client, trader, steps, now());
    return evmTx(chainId, [...missing.map((s) => s.call), trade]);
  }

  return {
    chains: options.chains.map((chainId) => ({ family: 'evm', chainId })),
    getLaunch, listLaunches, quote, quoteSellBack, buildTrade,
    deploymentOf, clientOf,
  };
}
