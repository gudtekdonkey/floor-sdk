/**
 * Reading FLOOR launches off an EVM chain with viem.
 *
 * - `poolState` is one `eth_call`, decoded at one width (`decodePoolState` refuses any other).
 * - The pool id comes from `hook.poolForAccount(launch)` — THE HOOK IS THE SOURCE OF THE ID, never the registry entry or a
 *   derived key. The pool's own FLOOR Platform fee is read off `poolState.teamFeeBps` and cap-checked.
 * - The sale's getters are read in one batch; its `teamFeeBps()` only on a set whose row names a team vault.
 * - The deployer is `factory.creatorOf(launch)` on the entry's registrar; the published floor is `1.0001^-floorTick`.
 *
 * ⛔ Nothing here lists by draining a chain. `listLaunches` reads ONE registry page per call (newest first) and hands back a
 *    cursor; `getLaunch` resolves a token by its own `launch()` and the registry's `isRegistered`, then finds its entry in a
 *    BOUNDED newest-first scan (`maxRegistryScan`), remembering every entry it has seen.
 */
import { BaseError, ContractFunctionRevertedError, decodeFunctionResult, encodeFunctionData, getAddress, zeroAddress, zeroHash, type Address, type ContractFunctionReturnType, type Hex, type PublicClient } from 'viem';
import type { Launch } from '@floorlaunchpad/sdk';
import { HOOK_ABI, LAUNCH_ABI, POOL_ABI, POOL_STATE_ABI, POOL_STATE_WORDS, REGISTRY_ABI, TOKEN_ABI, FACTORY_ABI, WALLET_SCAN_ABI } from './abi/index.js';
import { EVM_NETWORKS, setHasTeamFee, type Deployment } from './deployments.js';
import { FloorEvmError } from './errors.js';
import { MAX_POOL_TEAM_FEE_BPS, MAX_SALE_TEAM_FEE_BPS, rateWithinCap, type CushionEdge, type PoolRates } from './fees.js';
import { decodeSlot0, poolIdOfKey, poolKey, poolStateSlot, priceX18AtTick, quoteAtCurrency1, type PoolKey, type Slot0 } from './pool.js';
import { saleReadsFrom, type SaleReads } from './sale.js';

/** One pool's stored state exactly as `hook.poolState(id)` answers it — a NAMED object. */
export type PoolStateTuple = ContractFunctionReturnType<typeof HOOK_ABI, 'view', 'poolState'>;
/** One `FloorRegistry.Entry`. */
export type RegistryEntry = ContractFunctionReturnType<typeof REGISTRY_ABI, 'view', 'page'>[number];

/** The registry's page size. */
export const REGISTRY_PAGE_MAX = 500n;

/** Everything chain-specific this package reads about a launch — `Launch.raw`. */
export interface EvmLaunchRaw {
  chainId: number;
  launch: Address;
  token: Address;
  hook: Address;
  /** The hook's own id for the pool (`poolForAccount(launch)`). */
  poolId: Hex;
  /** The pool key; it hashes to `poolId` (checked). */
  poolKey: PoolKey;
  rewards: Address;
  /** The factory that registered the launch. */
  registrar: Address;
  /** Unix seconds the registry stamped. */
  registeredAt: bigint;
  /** The quote token; `null` = native. */
  quoteToken: Address | null;
  quoteIsCurrency1: boolean;
  /** The deployer (`creatorOf`), `null` where the factory names nobody or could not be read. */
  creator: Address | null;
  /** The sale's book while bonding; `null` once graduated. */
  sale: SaleReads | null;
  /** The SALE's FLOOR Platform fee, bps — what a bonding buy or sell-back pays; 0 on a set without the fee; `null` once
   *  graduated (not read: no bonding trade is left). */
  saleTeamFeeBps: number | null;
  /** The POOL's rates (copied at creation): skim endpoints, creator fee, holder tax, FLOOR Platform fee. */
  poolRates: PoolRates;
  /** The published floor's tick (the band's CHEAP edge, engine frame) and the bid band's width in ticks. */
  floorTick: number;
  bidWidth: number;
  poolState: PoolStateTuple;
}

export type EvmLaunch = Launch<EvmLaunchRaw>;

/** `poolState(id)` calldata. */
export function poolStateCalldata(id: Hex): Hex {
  return encodeFunctionData({ abi: HOOK_ABI, functionName: 'poolState', args: [id] });
}

/** The raw `poolState` return → the named tuple. ⛔ A return of any width but `POOL_STATE_WORDS` is refused by name; viem
 *  itself would decode a longer one without complaint. */
export function decodePoolState(data: Hex): PoolStateTuple {
  const hex = data.length >= 2 ? data.slice(2) : '';
  const words = hex.length / 64;
  if (hex.length > 0 && words !== POOL_STATE_WORDS) {
    throw new FloorEvmError('contract_mismatch', `poolState returned ${words} words; the engine's layout has ${POOL_STATE_WORDS}`);
  }
  return decodeFunctionResult({ abi: HOOK_ABI, functionName: 'poolState', data });
}

/** ONE `eth_call` of `poolState(id)`, decoded through `decodePoolState`. */
export async function readPoolState(client: Pick<PublicClient, 'call'>, hook: Address, id: Hex): Promise<PoolStateTuple> {
  const { data } = await client.call({ to: hook, data: poolStateCalldata(id) });
  if (!data || data === '0x') throw new FloorEvmError('contract_mismatch', `poolState(${id}) at ${hook} returned no data`);
  return decodePoolState(data);
}

/** A pool's rates off its state, the FLOOR Platform fee checked against its cap (`RateAboveCap` past it). */
export function poolRatesOf(st: PoolStateTuple): PoolRates {
  const n = (v: bigint, what: string) => {
    if (v > 10_000n) throw new FloorEvmError('contract_mismatch', `poolState.${what} = ${v} is not a bps rate`);
    return Number(v);
  };
  return {
    skimBuyFarBps: n(st.skimBuyFarBps, 'skimBuyFarBps'),
    skimSellFarBps: n(st.skimSellFarBps, 'skimSellFarBps'),
    skimBuyAtFloorBps: n(st.skimBuyAtFloorBps, 'skimBuyAtFloorBps'),
    skimSellAtFloorBps: n(st.skimSellAtFloorBps, 'skimSellAtFloorBps'),
    creatorBuyBps: n(st.creatorBuyBps, 'creatorBuyBps'),
    creatorSellBps: n(st.creatorSellBps, 'creatorSellBps'),
    holderBuyBps: n(st.holderBuyBps, 'holderBuyBps'),
    holderSellBps: n(st.holderSellBps, 'holderSellBps'),
    teamFeeBps: rateWithinCap(Number(st.teamFeeBps), MAX_POOL_TEAM_FEE_BPS, 'the pool\'s FLOOR Platform fee (PoolState.teamFeeBps)'),
  };
}

/** The pool's `Slot0` off the PoolManager (`extsload` of the pool's state slot). */
export async function readSlot0(client: Pick<PublicClient, 'readContract'>, poolManager: Address, poolId: Hex): Promise<Slot0> {
  const word = await client.readContract({ address: poolManager, abi: POOL_STATE_ABI, functionName: 'extsload', args: [poolStateSlot(poolId)] });
  return decodeSlot0(word);
}

/** Whether a viem error is a contract REVERT (an answer: "no"), as opposed to a transport failure (not an answer). */
export function isRevert(e: unknown): boolean {
  return e instanceof BaseError && e.walk((x) => x instanceof ContractFunctionRevertedError) instanceof ContractFunctionRevertedError;
}

export async function readRegistryCount(client: Pick<PublicClient, 'readContract'>, d: Deployment): Promise<bigint> {
  return client.readContract({ address: d.registry, abi: REGISTRY_ABI, functionName: 'count' });
}

export async function readRegistryPage(client: Pick<PublicClient, 'readContract'>, d: Deployment, start: bigint, n: bigint): Promise<readonly RegistryEntry[]> {
  return client.readContract({ address: d.registry, abi: REGISTRY_ABI, functionName: 'page', args: [start, n] });
}

/**
 * The launch contract behind a token, checked against THIS chain's registry. A token whose `launch()` reverts, is zero, or is
 * not registered here is not a FLOOR launch on this chain: `FloorEvmError('not_a_launch')`.
 */
export async function launchOfToken(client: Pick<PublicClient, 'readContract'>, d: Deployment, token: Address): Promise<Address> {
  let launch: Address;
  try {
    launch = await client.readContract({ address: token, abi: WALLET_SCAN_ABI, functionName: 'launch' });
  } catch (e) {
    if (isRevert(e)) throw new FloorEvmError('not_a_launch', `${token} answers no launch() — not a FLOOR token`, { cause: e });
    throw e;
  }
  if (BigInt(launch) === 0n) throw new FloorEvmError('not_a_launch', `${token}'s launch() is zero`);
  const registered = await client.readContract({ address: d.registry, abi: REGISTRY_ABI, functionName: 'isRegistered', args: [launch] });
  if (!registered) throw new FloorEvmError('not_a_launch', `${token}'s launch ${launch} is not registered on chain ${d.chainId}'s registry ${d.registry}`);
  return getAddress(launch);
}

/** A quote asset's address, symbol and decimals. Native: the chain's symbol and 18. */
export async function readQuoteAsset(client: Pick<PublicClient, 'readContract'>, chainId: number, quote: Address | null, nativeSymbol?: string): Promise<{ address: string | null; symbol: string; decimals: number }> {
  if (quote === null) return { address: null, symbol: nativeSymbol ?? EVM_NETWORKS[chainId]?.nativeSymbol ?? 'native', decimals: 18 };
  const [symbol, decimals] = await Promise.all([
    client.readContract({ address: quote, abi: TOKEN_ABI, functionName: 'symbol' }),
    client.readContract({ address: quote, abi: TOKEN_ABI, functionName: 'decimals' }),
  ]);
  return { address: quote, symbol, decimals };
}

/** The sale's getters in one batch, checked (`saleReadsFrom`). The fee is asked only on a set that carries it. */
export async function readSale(client: Pick<PublicClient, 'readContract'>, d: Deployment, launch: Address): Promise<SaleReads> {
  const r = <F extends 'funded' | 'saleTokens' | 'sold' | 'raised' | 'p0' | 'p1' | 'tranches' | 'graduated'>(functionName: F) =>
    client.readContract({ address: launch, abi: LAUNCH_ABI, functionName });
  const [funded, saleTokens, sold, raised, p0, p1, tranches, graduated, teamFeeBps] = await Promise.all([
    r('funded'), r('saleTokens'), r('sold'), r('raised'), r('p0'), r('p1'), r('tranches'), r('graduated'),
    setHasTeamFee(d) ? client.readContract({ address: launch, abi: LAUNCH_ABI, functionName: 'teamFeeBps' }).then((v) => rateWithinCap(Number(v), MAX_SALE_TEAM_FEE_BPS, `the sale ${launch}'s FLOOR Platform fee`)) : Promise.resolve(0),
  ]);
  return saleReadsFrom({ funded, saleTokens, sold, raised, p0, p1, tranches, graduated, teamFeeBps });
}

/** The hook's id for the launch's pool. A revert or a zero id is "not a launch this hook serves". */
export async function readPoolId(client: Pick<PublicClient, 'readContract'>, hook: Address, launch: Address): Promise<Hex> {
  let id: Hex;
  try {
    id = await client.readContract({ address: hook, abi: HOOK_ABI, functionName: 'poolForAccount', args: [launch] });
  } catch (e) {
    if (isRevert(e)) throw new FloorEvmError('not_a_launch', `hook ${hook} reverted poolForAccount(${launch})`, { cause: e });
    throw e;
  }
  if (id === zeroHash) throw new FloorEvmError('not_a_launch', `hook ${hook} has no pool for launch ${launch}`);
  return id;
}

/**
 * A launch, read whole from its registry entry: the token, the sale or the pool, the rates, the floor. Every rate is read
 * off this launch's own contracts; nothing is assumed.
 */
export async function readLaunchFromEntry(client: PublicClient, d: Deployment, e: RegistryEntry, opts: { nativeSymbol?: string } = {}): Promise<EvmLaunch> {
  const token = getAddress(e.token), hook = getAddress(e.hook), launch = getAddress(e.launch);
  const poolId = await readPoolId(client, hook, launch);
  const [st, name, symbol, decimals, totalSupply, graduated, creator] = await Promise.all([
    readPoolState(client, hook, poolId),
    client.readContract({ address: token, abi: TOKEN_ABI, functionName: 'name' }),
    client.readContract({ address: token, abi: TOKEN_ABI, functionName: 'symbol' }),
    client.readContract({ address: token, abi: TOKEN_ABI, functionName: 'decimals' }),
    client.readContract({ address: token, abi: TOKEN_ABI, functionName: 'totalSupply' }),
    client.readContract({ address: launch, abi: LAUNCH_ABI, functionName: 'graduated' }),
    client.readContract({ address: e.registrar, abi: FACTORY_ABI, functionName: 'creatorOf', args: [launch] }).then((a) => (BigInt(a) === 0n ? null : getAddress(a)), () => null),
  ]);
  if (st.token.toLowerCase() !== token.toLowerCase()) throw new FloorEvmError('contract_mismatch', `pool ${poolId} names token ${st.token}, not ${token}`);
  const quoteToken = BigInt(st.quoteToken) === 0n ? null : getAddress(st.quoteToken);
  const key = poolKey(token, hook, quoteToken ?? zeroAddress);
  /* ⛔ The key this SDK will sign must hash to the hook's id for the launch. */
  if (poolIdOfKey(key).toLowerCase() !== poolId.toLowerCase()) throw new FloorEvmError('contract_mismatch', `pool ${poolId}: the key of ${token} / ${quoteToken ?? 'native'} on hook ${hook} hashes to ${poolIdOfKey(key)}, not the hook's id`);
  const orient = quoteAtCurrency1(key, quoteToken ?? zeroAddress);
  if (orient === null || orient !== st.quoteIsCurrency1) throw new FloorEvmError('contract_mismatch', `pool ${poolId}'s quote orientation disagrees with its key`);
  const [quote, sale] = await Promise.all([
    readQuoteAsset(client, d.chainId, quoteToken, opts.nativeSymbol),
    graduated ? Promise.resolve(null) : readSale(client, d, launch),
  ]);
  const poolRates = poolRatesOf(st);
  const bidWidth = st.bidWidth === 0 ? st.tickSpacing : st.bidWidth;
  return {
    ref: { chain: { family: 'evm', chainId: d.chainId }, token },
    name, symbol, decimals, totalSupply, quote,
    /* An unfunded sale takes no buy yet (`FloorLaunch._buy` reverts `NotFunded`): 'pending'. */
    phase: graduated ? 'graduated' : sale?.funded ? 'bonding' : 'pending',
    mode: e.mode,
    creator,
    floorPriceX18: graduated ? priceX18AtTick(st.floorTick, decimals, quote.decimals) : null,
    raw: {
      chainId: d.chainId, launch, token, hook, poolId, poolKey: key, rewards: getAddress(e.rewards), registrar: getAddress(e.registrar),
      registeredAt: e.at, quoteToken, quoteIsCurrency1: st.quoteIsCurrency1, creator,
      sale, saleTeamFeeBps: sale ? sale.teamFeeBps : null, poolRates, floorTick: st.floorTick, bidWidth, poolState: st,
    },
  };
}

/**
 * The pool's staircase cushion edge (`stairs()` through `hook.execute(poolId, …)`): what the gap buy skim reads.
 * `cushionLiq` 0 = no cushion standing.
 */
export async function readCushion(client: Pick<PublicClient, 'call'>, hook: Address, poolId: Hex): Promise<CushionEdge> {
  const inner = encodeFunctionData({ abi: POOL_ABI, functionName: 'stairs' });
  const { data } = await client.call({ to: hook, data: encodeFunctionData({ abi: HOOK_ABI, functionName: 'execute', args: [poolId, inner] }) });
  if (!data || data === '0x') throw new FloorEvmError('contract_mismatch', `stairs() of pool ${poolId} at ${hook} returned no data`);
  const bytes = decodeFunctionResult({ abi: HOOK_ABI, functionName: 'execute', data });
  const st = decodeFunctionResult({ abi: POOL_ABI, functionName: 'stairs', data: bytes });
  return { cushionLower: st.cushionLower, cushionLiq: st.cushionLiq };
}

/** Reads a holder's sale position (`allocation`, `contributed`) — what a sell-back pays against. */
export async function readSalePosition(client: Pick<PublicClient, 'readContract'>, launch: Address, holder: Address): Promise<{ allocation: bigint; contributed: bigint }> {
  const [allocation, contributed] = await Promise.all([
    client.readContract({ address: launch, abi: LAUNCH_ABI, functionName: 'allocation', args: [holder] }),
    client.readContract({ address: launch, abi: LAUNCH_ABI, functionName: 'contributed', args: [holder] }),
  ]);
  return { allocation, contributed };
}
