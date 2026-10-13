/**
 * An off-chain simulator of a swap on a FLOOR pool: Uniswap v4's swap, ported from v4-core statement for statement, with the
 * FLOOR hook's fees around it. It needs no network once it has a `SwapSnapshot` (`readSwapSnapshot`), and the same snapshot
 * and amount always give the same answer, so an aggregator's router can call it as often as it likes.
 *
 * What the hook does to a trader's swap (`SingletonFloorEngine._beforeSwap` / `_afterSwap`):
 * - Exact input only, both directions (`ExactOutNotSupported`).
 * - A BUY (quote in): before the swap the hook takes its four fees off the quote paid in, at the skim rate of the PRE-swap
 *   price (the gap buy skim included where the set has it); the pool swaps what is left.
 * - A SELL (tokens in): the pool swaps the tokens; after the swap the hook takes its four fees off the quote paid out, at the
 *   skim rate of the POST-swap price.
 * - The LP fee is the pool's override, 0.25%, on every swap; v4's protocol fee, if any, composes with it as v4 does.
 * The staircase, the cushion, the seed and the bid are ordinary positions in the pool; the walk below crosses them like any
 * other liquidity. The in-swap bid re-place that follows a sell into the bid happens after the trader's amounts are settled,
 * so it changes nothing here; it does change the pool for the NEXT swap, which is why a snapshot must be re-read after a sell
 * that reached the band (`SwapResult.reachedBand`).
 */
import type { Address, Hex, PublicClient } from 'viem';
import { floorDeploymentFor, rulesOf, type Deployment } from './deployments.js';
import { FloorEvmError } from './errors.js';
import { gapSkimBps, hookTakes, skimBpsAt, type CushionEdge, type PoolRates } from './fees.js';
import { bandTicks, LP_FEE_PIPS, marketTick, MAX_SQRT, MAX_TICK, MIN_SQRT, MIN_TICK, sqrtPriceAtTick } from './pool.js';
import { poolRatesOf, readCushion, readPoolState } from './reads.js';

// ---------------------------------------------------------------- v4-core math (FullMath, SqrtPriceMath, SwapMath, TickMath)

const Q96 = 1n << 96n;
const MAX_U256 = (1n << 256n) - 1n;
const MAX_SWAP_FEE = 1_000_000n;

const mulDiv = (a: bigint, b: bigint, d: bigint): bigint => (a * b) / d;
const mulDivRoundingUp = (a: bigint, b: bigint, d: bigint): bigint => {
  const p = a * b;
  return p / d + (p % d === 0n ? 0n : 1n);
};
const divRoundingUp = (a: bigint, b: bigint): bigint => a / b + (a % b === 0n ? 0n : 1n);

/** `SqrtPriceMath.getAmount0Delta(a, b, liquidity, roundUp)`. */
export function getAmount0Delta(a: bigint, b: bigint, liquidity: bigint, roundUp: boolean): bigint {
  if (a > b) [a, b] = [b, a];
  if (a === 0n) throw new RangeError('getAmount0Delta: zero price');
  const n1 = liquidity << 96n;
  const n2 = b - a;
  return roundUp ? divRoundingUp(mulDivRoundingUp(n1, n2, b), a) : mulDiv(n1, n2, b) / a;
}

/** `SqrtPriceMath.getAmount1Delta(a, b, liquidity, roundUp)`. */
export function getAmount1Delta(a: bigint, b: bigint, liquidity: bigint, roundUp: boolean): bigint {
  const diff = a > b ? a - b : b - a;
  const p = liquidity * diff;
  return p / Q96 + (roundUp && p % Q96 !== 0n ? 1n : 0n);
}

function nextSqrtFromAmount0RoundingUp(sqrtP: bigint, liquidity: bigint, amount: bigint): bigint {
  if (amount === 0n) return sqrtP;
  const n1 = liquidity << 96n;
  const product = amount * sqrtP;
  if (product <= MAX_U256) {
    const denominator = n1 + product;
    if (denominator <= MAX_U256) return mulDivRoundingUp(n1, sqrtP, denominator);
  }
  return divRoundingUp(n1, n1 / sqrtP + amount);
}

function nextSqrtFromAmount1RoundingDown(sqrtP: bigint, liquidity: bigint, amount: bigint): bigint {
  const quotient = amount <= (1n << 160n) - 1n ? (amount << 96n) / liquidity : mulDiv(amount, Q96, liquidity);
  const next = sqrtP + quotient;
  if (next >= 1n << 160n) throw new RangeError('getNextSqrtPriceFromAmount1RoundingDown: overflow');
  return next;
}

/** `SqrtPriceMath.getNextSqrtPriceFromInput`. */
export function getNextSqrtPriceFromInput(sqrtP: bigint, liquidity: bigint, amountIn: bigint, zeroForOne: boolean): bigint {
  if (sqrtP === 0n || liquidity === 0n) throw new RangeError('getNextSqrtPriceFromInput: zero price or liquidity');
  return zeroForOne ? nextSqrtFromAmount0RoundingUp(sqrtP, liquidity, amountIn) : nextSqrtFromAmount1RoundingDown(sqrtP, liquidity, amountIn);
}

/** `SwapMath.computeSwapStep`, exact input only (an exact-output swap never reaches a FLOOR pool). */
export function computeSwapStepExactIn(sqrtCurrent: bigint, sqrtTarget: bigint, liquidity: bigint, amountRemaining: bigint, feePips: bigint): {
  sqrtNext: bigint; amountIn: bigint; amountOut: bigint; feeAmount: bigint;
} {
  const zeroForOne = sqrtCurrent >= sqrtTarget;
  const lessFee = mulDiv(amountRemaining, MAX_SWAP_FEE - feePips, MAX_SWAP_FEE);
  let amountIn = zeroForOne ? getAmount0Delta(sqrtTarget, sqrtCurrent, liquidity, true) : getAmount1Delta(sqrtCurrent, sqrtTarget, liquidity, true);
  let sqrtNext: bigint;
  let feeAmount: bigint;
  if (lessFee >= amountIn) {
    sqrtNext = sqrtTarget;
    feeAmount = feePips === MAX_SWAP_FEE ? amountIn : mulDivRoundingUp(amountIn, feePips, MAX_SWAP_FEE - feePips);
  } else {
    amountIn = lessFee;
    sqrtNext = getNextSqrtPriceFromInput(sqrtCurrent, liquidity, lessFee, zeroForOne);
    feeAmount = amountRemaining - amountIn;
  }
  const amountOut = zeroForOne ? getAmount1Delta(sqrtNext, sqrtCurrent, liquidity, false) : getAmount0Delta(sqrtCurrent, sqrtNext, liquidity, false);
  return { sqrtNext, amountIn, amountOut, feeAmount };
}

const msb = (x: bigint): number => x.toString(2).length - 1;
const lsb = (x: bigint): number => msb(x & -x);

/** `TickMath.getTickAtSqrtPrice`, exactly. */
export function getTickAtSqrtPrice(sqrtPriceX96: bigint): number {
  if (sqrtPriceX96 < MIN_SQRT || sqrtPriceX96 >= MAX_SQRT) throw new RangeError(`getTickAtSqrtPrice: ${sqrtPriceX96} out of range`);
  const price = sqrtPriceX96 << 32n;
  const m = msb(price);
  let r = m >= 128 ? price >> BigInt(m - 127) : price << BigInt(127 - m);
  let log2 = BigInt(m - 128) << 64n;
  for (let shift = 63n; shift >= 50n; shift--) {
    r = (r * r) >> 127n;
    const f = r >> 128n;
    log2 |= f << shift;
    r >>= f;
  }
  const logSqrt10001 = log2 * 255738958999603826347141n;
  const tickLow = Number((logSqrt10001 - 3402992956809132418596140100660247210n) >> 128n);
  const tickHi = Number((logSqrt10001 + 291339464771989622907027621153398088495n) >> 128n);
  return tickLow === tickHi ? tickLow : sqrtPriceAtTick(tickHi) <= sqrtPriceX96 ? tickHi : tickLow;
}

// ---------------------------------------------------------------- the pool's liquidity map

/** A v4 pool as the swap walk reads it. Ticks are the PoolManager's own (raw, not the engine frame). */
export interface V4PoolState {
  sqrtPriceX96: bigint;
  tick: number;
  liquidity: bigint;
  tickSpacing: number;
  /** v4's packed protocol fee (`Slot0.protocolFee`): zeroForOne in the low 12 bits, oneForZero in the next 12. */
  protocolFee: number;
  /** The tick bitmap, word position → word. A word left out is read as empty. */
  bitmap: ReadonlyMap<number, bigint>;
  /** `liquidityNet` of every initialized tick. */
  liquidityNet: ReadonlyMap<number, bigint>;
}

const compress = (tick: number, spacing: number): number => Math.floor(tick / spacing);

/** `TickBitmap.nextInitializedTickWithinOneWord`. */
function nextInitializedTickWithinOneWord(bitmap: ReadonlyMap<number, bigint>, tick: number, spacing: number, lte: boolean): { next: number; initialized: boolean } {
  let compressed = compress(tick, spacing);
  if (lte) {
    const wordPos = compressed >> 8;
    const bitPos = compressed & 0xff;
    const mask = MAX_U256 >> BigInt(255 - bitPos);
    const masked = (bitmap.get(wordPos) ?? 0n) & mask;
    const initialized = masked !== 0n;
    const next = initialized ? (compressed - (bitPos - msb(masked))) * spacing : (compressed - bitPos) * spacing;
    return { next, initialized };
  }
  compressed += 1;
  const wordPos = compressed >> 8;
  const bitPos = compressed & 0xff;
  const mask = MAX_U256 ^ ((1n << BigInt(bitPos)) - 1n);
  const masked = (bitmap.get(wordPos) ?? 0n) & mask;
  const initialized = masked !== 0n;
  const next = initialized ? (compressed + (lsb(masked) - bitPos)) * spacing : (compressed + (255 - bitPos)) * spacing;
  return { next, initialized };
}

export interface V4SwapResult {
  /** The input the pool consumed, LP and protocol fees included. Less than asked only when the pool ran out of liquidity. */
  amountIn: bigint;
  amountOut: bigint;
  /** The LP fee and v4's protocol fee charged, in the input token. */
  lpFee: bigint;
  protocolFee: bigint;
  sqrtPriceX96: bigint;
  tick: number;
  liquidity: bigint;
}

/**
 * `Pool.swap`, exact input, with the price limit the V4Quoter and the UniversalRouter use when none is given (the edge of
 * the price range). `lpFeePips` is the swap's LP fee (a FLOOR pool overrides it to 2,500 on every swap).
 */
export function v4SwapExactIn(pool: V4PoolState, zeroForOne: boolean, amountIn: bigint, lpFeePips: bigint, sqrtPriceLimitX96?: bigint): V4SwapResult {
  if (amountIn <= 0n) throw new RangeError('v4SwapExactIn: amountIn must be positive');
  const limit = sqrtPriceLimitX96 ?? (zeroForOne ? MIN_SQRT + 1n : MAX_SQRT - 1n);
  const protocolFee = BigInt(zeroForOne ? pool.protocolFee & 0xfff : (pool.protocolFee >> 12) & 0xfff);
  const swapFee = protocolFee === 0n ? lpFeePips : protocolFee + lpFeePips - (protocolFee * lpFeePips) / MAX_SWAP_FEE;
  if (swapFee >= MAX_SWAP_FEE) throw new RangeError('v4SwapExactIn: a 100% fee leaves nothing to swap');

  let remaining = amountIn;
  let out = 0n;
  let lpFee = 0n;
  let toProtocol = 0n;
  let sqrtP = pool.sqrtPriceX96;
  let tick = pool.tick;
  let liquidity = pool.liquidity;

  while (remaining !== 0n && sqrtP !== limit) {
    const start = sqrtP;
    let { next: tickNext, initialized } = nextInitializedTickWithinOneWord(pool.bitmap, tick, pool.tickSpacing, zeroForOne);
    if (tickNext <= MIN_TICK) tickNext = MIN_TICK;
    if (tickNext >= MAX_TICK) tickNext = MAX_TICK;
    const sqrtNext = sqrtPriceAtTick(tickNext);
    const target = zeroForOne ? (sqrtNext < limit ? limit : sqrtNext) : (sqrtNext > limit ? limit : sqrtNext);
    const step = computeSwapStepExactIn(sqrtP, target, liquidity, remaining, swapFee);
    sqrtP = step.sqrtNext;
    remaining -= step.amountIn + step.feeAmount;
    out += step.amountOut;
    let fee = step.feeAmount;
    if (protocolFee > 0n) {
      const delta = swapFee === protocolFee ? fee : ((step.amountIn + step.feeAmount) * protocolFee) / MAX_SWAP_FEE;
      fee -= delta;
      toProtocol += delta;
    }
    lpFee += fee;
    if (sqrtP === sqrtNext) {
      if (initialized) {
        let net = pool.liquidityNet.get(tickNext) ?? 0n;
        if (zeroForOne) net = -net;
        liquidity += net;
        if (liquidity < 0n) throw new RangeError('v4SwapExactIn: liquidity underflow (an inconsistent snapshot)');
      }
      tick = zeroForOne ? tickNext - 1 : tickNext;
    } else if (sqrtP !== start) {
      tick = getTickAtSqrtPrice(sqrtP);
    }
  }
  return { amountIn: amountIn - remaining, amountOut: out, lpFee, protocolFee: toProtocol, sqrtPriceX96: sqrtP, tick, liquidity };
}

// ---------------------------------------------------------------- the FLOOR hook around it

/** Everything a FLOOR pool swap is computed from. Read it with `readSwapSnapshot`; refresh it after any swap on the pool. */
export interface SwapSnapshot {
  pool: V4PoolState;
  /** The quote is currency1 (an ERC-20 quote that sorts above the token); false for a native quote. */
  quoteIsCurrency1: boolean;
  /** `poolState.floorTick`, engine frame. */
  floorTick: number;
  rates: PoolRates;
  cushion: CushionEdge;
  /** The set has the gap buy skim (`ContractRules.gapSkim`). */
  gapSkim: boolean;
}

export interface FloorSwapFees {
  /** The v4 LP fee, in the swap's input token (the quote on a buy, the launch token on a sell). */
  lp: bigint;
  /** v4's protocol fee, in the input token; 0 unless the PoolManager's owner set one. */
  protocol: bigint;
  /** The hook's four takes, in the quote. */
  floorSkim: bigint;
  creator: bigint;
  holder: bigint;
  floorPlatform: bigint;
}

export interface FloorSwapResult {
  side: 'buy' | 'sell';
  amountIn: bigint;
  amountOut: bigint;
  /** The skim rate charged, bps. */
  skimBps: number;
  fees: FloorSwapFees;
  /** The pool's price and tick after the swap (before any bid re-place). */
  sqrtPriceX96After: bigint;
  tickAfter: number;
  /** A sell whose walk reached the bid band's dear edge: the hook re-places the bid after it, so re-read the snapshot. */
  reachedBand: boolean;
}

/**
 * Quote an exact-input swap on a FLOOR pool. `side` is the trader's: `buy` pays `amountIn` of the quote for launch tokens,
 * `sell` pays `amountIn` launch tokens for the quote. Throws `no_quote` when the pool cannot fill the whole
 * input (the V4Quoter refuses such a swap too).
 */
export function simulateSwap(s: SwapSnapshot, side: 'buy' | 'sell', amountIn: bigint): FloorSwapResult {
  if (amountIn <= 0n) throw new RangeError('simulateSwap: amountIn must be positive');
  const zeroForOne = side === 'buy' ? !s.quoteIsCurrency1 : s.quoteIsCurrency1;
  const dearEdge = s.floorTick - bandTicks(s.pool.tickSpacing);

  if (side === 'buy') {
    const pre = marketTick(s.pool.tick, s.quoteIsCurrency1);
    let bps = skimBpsAt('buy', dearEdge - pre, s.rates);
    if (s.gapSkim) bps = gapSkimBps(bps, s.cushion, pre);
    const takes = hookTakes('buy', amountIn, bps, s.rates);
    const toSwap = amountIn - takes.total;
    const swap = toSwap > 0n ? v4SwapExactIn(s.pool, zeroForOne, toSwap, LP_FEE_PIPS) : null;
    if (swap && swap.amountIn < toSwap) throw new FloorEvmError('no_quote', `the pool fills ${swap.amountIn} of ${toSwap}`);
    return {
      side, amountIn, amountOut: swap?.amountOut ?? 0n, skimBps: bps,
      fees: { lp: swap?.lpFee ?? 0n, protocol: swap?.protocolFee ?? 0n, floorSkim: takes.floorSkim, creator: takes.creator, holder: takes.holder, floorPlatform: takes.floorPlatform },
      sqrtPriceX96After: swap?.sqrtPriceX96 ?? s.pool.sqrtPriceX96, tickAfter: swap?.tick ?? s.pool.tick, reachedBand: false,
    };
  }

  const swap = v4SwapExactIn(s.pool, zeroForOne, amountIn, LP_FEE_PIPS);
  if (swap.amountIn < amountIn) throw new FloorEvmError('no_quote', `the pool fills ${swap.amountIn} of ${amountIn}`);
  const post = marketTick(swap.tick, s.quoteIsCurrency1);
  const bps = skimBpsAt('sell', dearEdge - post, s.rates);
  const takes = hookTakes('sell', swap.amountOut, bps, s.rates);
  return {
    side, amountIn, amountOut: swap.amountOut - takes.total, skimBps: bps,
    fees: { lp: swap.lpFee, protocol: swap.protocolFee, floorSkim: takes.floorSkim, creator: takes.creator, holder: takes.holder, floorPlatform: takes.floorPlatform },
    sqrtPriceX96After: swap.sqrtPriceX96, tickAfter: swap.tick, reachedBand: post >= dearEdge,
  };
}

// ---------------------------------------------------------------- reading a snapshot

/** Uniswap v4's StateView: the four reads a snapshot needs. */
export const STATE_VIEW_ABI = [
  { type: 'function', name: 'getSlot0', stateMutability: 'view', inputs: [{ name: 'poolId', type: 'bytes32' }], outputs: [{ name: 'sqrtPriceX96', type: 'uint160' }, { name: 'tick', type: 'int24' }, { name: 'protocolFee', type: 'uint24' }, { name: 'lpFee', type: 'uint24' }] },
  { type: 'function', name: 'getLiquidity', stateMutability: 'view', inputs: [{ name: 'poolId', type: 'bytes32' }], outputs: [{ name: 'liquidity', type: 'uint128' }] },
  { type: 'function', name: 'getTickBitmap', stateMutability: 'view', inputs: [{ name: 'poolId', type: 'bytes32' }, { name: 'tick', type: 'int16' }], outputs: [{ name: 'tickBitmap', type: 'uint256' }] },
  { type: 'function', name: 'getTickLiquidity', stateMutability: 'view', inputs: [{ name: 'poolId', type: 'bytes32' }, { name: 'tick', type: 'int24' }], outputs: [{ name: 'liquidityGross', type: 'uint128' }, { name: 'liquidityNet', type: 'int128' }] },
] as const;

/**
 * Read everything `simulateSwap` needs for one pool: the hook's `poolState` (rates, floor, orientation), the cushion, the
 * PoolManager's slot0 and liquidity through StateView, the whole tick bitmap and every initialized tick's `liquidityNet`.
 * About 120 bitmap reads plus one per initialized tick; batch them with a viem client that has `batch.multicall` set.
 */
export async function readSwapSnapshot(client: PublicClient, chainId: number, poolId: Hex): Promise<SwapSnapshot> {
  const d: Deployment = floorDeploymentFor(chainId);
  const hook = d.singletonHook;
  if (!hook) throw new FloorEvmError('bad_params', `chain ${chainId} has no singleton hook address`);
  if (!d.stateView) throw new FloorEvmError('bad_params', `chain ${chainId} has no StateView address`);
  const sv = d.stateView;
  const [st, cushion, slot0, liquidity] = await Promise.all([
    readPoolState(client, hook as Address, poolId),
    readCushion(client, hook as Address, poolId),
    client.readContract({ address: sv, abi: STATE_VIEW_ABI, functionName: 'getSlot0', args: [poolId] }),
    client.readContract({ address: sv, abi: STATE_VIEW_ABI, functionName: 'getLiquidity', args: [poolId] }),
  ]);
  const spacing = Number(st.tickSpacing);
  const minWord = compress(MIN_TICK, spacing) >> 8;
  const maxWord = compress(MAX_TICK, spacing) >> 8;
  const words: number[] = [];
  for (let w = minWord; w <= maxWord; w++) words.push(w);
  const values = await Promise.all(words.map((w) => client.readContract({ address: sv, abi: STATE_VIEW_ABI, functionName: 'getTickBitmap', args: [poolId, w] })));
  const bitmap = new Map<number, bigint>();
  const ticks: number[] = [];
  words.forEach((w, i) => {
    const word = values[i]!;
    if (word === 0n) return;
    bitmap.set(w, word);
    for (let bit = 0; bit < 256; bit++) if ((word >> BigInt(bit)) & 1n) ticks.push(((w << 8) + bit) * spacing);
  });
  const nets = await Promise.all(ticks.map((t) => client.readContract({ address: sv, abi: STATE_VIEW_ABI, functionName: 'getTickLiquidity', args: [poolId, t] })));
  const liquidityNet = new Map<number, bigint>();
  ticks.forEach((t, i) => liquidityNet.set(t, nets[i]![1]));
  return {
    pool: { sqrtPriceX96: slot0[0], tick: slot0[1], liquidity, tickSpacing: spacing, protocolFee: slot0[2], bitmap, liquidityNet },
    quoteIsCurrency1: st.quoteIsCurrency1,
    floorTick: Number(st.floorTick),
    rates: poolRatesOf(st),
    cushion,
    gapSkim: rulesOf(d).gapSkim,
  };
}
