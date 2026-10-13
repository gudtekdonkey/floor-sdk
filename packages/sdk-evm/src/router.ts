/**
 * The UniversalRouter / v4 payloads a FLOOR pool swap is sent as.
 *
 * ⛔ No off-chain Permit2 signature anywhere: instead of a signed permit inside the swap (PERMIT2_PERMIT, 0x8a), Permit2's
 *    on-chain `approve` goes in the same EIP-5792 batch as the trade, so the batch is one wallet prompt.
 *
 * ⛔ TWO byte tables overlap: 0x10 is V4_SWAP as a COMMAND and TAKE_PORTION as an ACTION.
 * ⛔ TAKE_ALL's amount is the MINIMUM received: at zero it accepts any output. Every swap here carries a positive one.
 * ⛔ The struct layout depends on the router's RELEASE (2.1.1 adds `minHopPriceX36`); a UniversalRouter with no recorded
 *    release is refused before a byte is encoded.
 */
import { encodeAbiParameters, type Address, type Hex } from 'viem';
import type { PoolKey } from './pool.js';
import type { UrRelease } from './deployments.js';

const CMD_SWEEP = '04';
const CMD_V4_SWAP = '10';
const ACT_SWAP_EXACT_IN_SINGLE = '06';
const ACT_SETTLE = '0b';
const ACT_SETTLE_ALL = '0c';
const ACT_TAKE_ALL = '0f';
/** `ActionConstants.MSG_SENDER`: the router maps address(1) to the real caller. */
const MSG_SENDER = '0x0000000000000000000000000000000000000001' as Address;
const NATIVE = '0x0000000000000000000000000000000000000000' as Address;

export type V4Layout = 'plain' | 'minHop';

export function v4LayoutOf(kind: 'universal' | 'v4router', release: UrRelease | undefined): V4Layout {
  if (kind === 'v4router') return 'plain';
  if (kind !== 'universal') throw new Error(`v4 actions: ${String(kind)} is not an actions router`);
  if (release === '2.1.1') return 'minHop';
  if (release === '2.0.0') return 'plain';
  throw new Error(`v4LayoutOf: a UniversalRouter needs its recorded release (got ${String(release)})`);
}

const POOL_KEY_TUPLE = {
  type: 'tuple',
  components: [
    { name: 'currency0', type: 'address' }, { name: 'currency1', type: 'address' },
    { name: 'fee', type: 'uint24' }, { name: 'tickSpacing', type: 'int24' },
    { name: 'hooks', type: 'address' },
  ],
} as const;

/** `IV4Router.ExactInputSingleParams` in the router's layout. There is NO price-limit field. */
export function exactInSingleTuple(layout: V4Layout) {
  return {
    type: 'tuple',
    components: [
      { ...POOL_KEY_TUPLE, name: 'poolKey' },
      { name: 'zeroForOne', type: 'bool' },
      { name: 'amountIn', type: 'uint128' },
      { name: 'amountOutMinimum', type: 'uint128' },
      ...(layout === 'minHop' ? [{ name: 'minHopPriceX36', type: 'uint256' } as const] : []),
      { name: 'hookData', type: 'bytes' },
    ],
  } as const;
}

const CURRENCY_AND_AMOUNT = [{ type: 'address' }, { type: 'uint256' }] as const;
const UINT128_MAX = (1n << 128n) - 1n;

function u128(v: bigint, what: string): bigint {
  if (v < 0n) throw new Error(`${what}: an amount cannot be negative`);
  if (v > UINT128_MAX) throw new Error(`${what}: ${v} does not fit uint128`);
  return v;
}

export interface V4SingleIn { key: PoolKey; zeroForOne: boolean; amountIn: bigint; amountOutMinimum: bigint; minHopPriceX36?: bigint; hookData?: Hex }

function minHopFor(layout: V4Layout, v: bigint | undefined): bigint | null {
  if (layout === 'plain') {
    if (v !== undefined) throw new Error('minHopPriceX36: this router\'s release has no such field');
    return null;
  }
  return v ?? 0n;
}

export function encodeV4ExactInSingle(layout: V4Layout, p: V4SingleIn): Hex {
  const minHop = minHopFor(layout, p.minHopPriceX36);
  const base = { poolKey: p.key, zeroForOne: p.zeroForOne, amountIn: u128(p.amountIn, 'amountIn'), amountOutMinimum: u128(p.amountOutMinimum, 'amountOutMinimum'), hookData: p.hookData ?? '0x' };
  return encodeAbiParameters([exactInSingleTuple(layout)] as never, [minHop === null ? base : { ...base, minHopPriceX36: minHop }] as never);
}

export interface SwapPlan {
  commands: Hex;
  inputs: readonly Hex[];
}

export function encodeV4Actions(actions: Hex, params: readonly Hex[]): Hex {
  return encodeAbiParameters([{ type: 'bytes' }, { type: 'bytes[]' }], [actions, params]);
}

export interface V4SwapArgs {
  layout: V4Layout;
  key: PoolKey;
  zeroForOne: boolean;
  currencyIn: Address;
  currencyOut: Address;
  amountIn: bigint;
  minOut: bigint;
  /** `router`: the input is already in the router (a `FloorSwapRatchet` paid it there) — SETTLE from the router's balance. */
  payer?: 'user' | 'router';
}

/**
 * One exact-in single-hop v4 swap as UniversalRouter commands: V4_SWAP (SWAP_EXACT_IN_SINGLE, SETTLE_ALL, TAKE_ALL), plus a
 * SWEEP of native back to the caller when the input is native (the router keeps no change).
 */
export function encodeV4Swap(a: V4SwapArgs): SwapPlan {
  const key = a.key;
  const amountIn = u128(a.amountIn, 'amountIn');
  const minOut = u128(a.minOut, 'minOut');
  if (amountIn === 0n) throw new Error('amountIn: the amount must be greater than zero');
  const [in0, in1] = a.zeroForOne ? [key.currency0, key.currency1] : [key.currency1, key.currency0];
  if (a.currencyIn.toLowerCase() !== in0.toLowerCase() || a.currencyOut.toLowerCase() !== in1.toLowerCase()) {
    throw new Error('v4 swap: currencyIn / currencyOut are not the key\'s currencies in this direction');
  }
  const { currencyIn, currencyOut } = a;
  const fromRouter = a.payer === 'router' && currencyIn.toLowerCase() !== NATIVE;
  const actions = ('0x' + ACT_SWAP_EXACT_IN_SINGLE + (fromRouter ? ACT_SETTLE : ACT_SETTLE_ALL) + ACT_TAKE_ALL) as Hex;
  const params: Hex[] = [
    encodeV4ExactInSingle(a.layout, { key, zeroForOne: a.zeroForOne, amountIn, amountOutMinimum: minOut }),
    fromRouter
      ? encodeAbiParameters([{ type: 'address' }, { type: 'uint256' }, { type: 'bool' }], [currencyIn, 0n, false])
      : encodeAbiParameters(CURRENCY_AND_AMOUNT, [currencyIn, amountIn]),
    encodeAbiParameters(CURRENCY_AND_AMOUNT, [currencyOut, minOut]),
  ];
  const v4Input = encodeV4Actions(actions, params);
  if (currencyIn.toLowerCase() !== NATIVE) {
    return { commands: `0x${CMD_V4_SWAP}`, inputs: [v4Input] };
  }
  return {
    commands: `0x${CMD_V4_SWAP}${CMD_SWEEP}`,
    inputs: [
      v4Input,
      encodeAbiParameters([{ type: 'address' }, { type: 'address' }, { type: 'uint256' }], [NATIVE, MSG_SENDER, 0n]),
    ],
  };
}

/** `IV4Quoter.QuoteExactSingleParams` for the key and direction a swap will sign. */
export function quoteParamsForKey(key: PoolKey, zeroForOne: boolean, exactAmount: bigint) {
  return { poolKey: key, zeroForOne, exactAmount: u128(exactAmount, 'exactAmount'), hookData: '0x' as Hex };
}

/** A minimum received `slippageBps` under a quote, rounded DOWN; 0 for a quote of nothing (which callers refuse). 100 bps
 *  is a 1% slippage allowance. */
export function minOutFromBps(quoted: bigint, slippageBps: number): bigint {
  if (quoted <= 0n) return 0n;
  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > 9_900) throw new RangeError(`slippage ${slippageBps} bps is not in 0..9900`);
  return (quoted * BigInt(10_000 - slippageBps)) / 10_000n;
}

/** Permit2's largest grant (uint160) and expiration (uint48). */
export const PERMIT2_MAX_AMOUNT = (1n << 160n) - 1n;
export const PERMIT2_MAX_EXPIRATION = (1n << 48n) - 1n;

