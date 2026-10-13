/**
 * A FLOOR pool's key, id and price arithmetic, in bigint.
 *
 * - `sqrtPriceAtTick` is v4-core `TickMath.getSqrtPriceAtTick`, exactly.
 * - `bandTicks`, `marketTick` mirror the hook engine's `_bandTicks` and `_market`.
 */
import { encodeAbiParameters, keccak256, zeroAddress, type Address, type Hex } from 'viem';

/** The native currency in a v4 key. */
export const NATIVE = zeroAddress as Address;
/** Every FLOOR pool is a dynamic-fee pool: the hook overrides the LP fee on every swap. */
export const DYNAMIC_FEE = 0x800000;
export const TICK_SPACING = 60;
/** The LP fee the hook force-overrides onto every swap, in pips (hundredths of a bip): 2,500 = 0.25%. `SingletonFloorEngine.LP_FEE`. */
export const LP_FEE_PIPS = 2500n;
/** The skim ramp's length in ticks (`SingletonFloorEngine.SKIM_RAMP_TICKS`). */
export const SKIM_RAMP_TICKS = 16094;
/** The bid band's width before rounding to spacing (`FloorPolicy.BID_BAND_TICKS`). */
export const BID_BAND_TICKS = 120;
export const MIN_TICK = -887272;
export const MAX_TICK = 887272;
export const MIN_SQRT = 4295128739n;
export const MAX_SQRT = 1461446703485210103287273052203988822378723970342n;

export interface PoolKey {
  currency0: Address;
  currency1: Address;
  fee: number;
  tickSpacing: number;
  hooks: Address;
}

/** The launch's pool key: the token against its quote (native by default), sorted, dynamic fee, spacing 60, the hook. */
export function poolKey(token: Address, hook: Address, quote: Address = NATIVE): PoolKey {
  const [currency0, currency1] = token.toLowerCase() < quote.toLowerCase() ? [token, quote] : [quote, token];
  return { currency0, currency1, fee: DYNAMIC_FEE, tickSpacing: TICK_SPACING, hooks: hook };
}

/** Whether `quote` sits at currency1 of the key; `null` when it is neither currency. */
export function quoteAtCurrency1(key: Pick<PoolKey, 'currency0' | 'currency1'>, quote: Address): boolean | null {
  const q = quote.toLowerCase();
  if (key.currency1.toLowerCase() === q) return true;
  if (key.currency0.toLowerCase() === q) return false;
  return null;
}

export function poolIdOfKey(k: PoolKey): Hex {
  return keccak256(
    encodeAbiParameters(
      [{ type: 'address' }, { type: 'address' }, { type: 'uint24' }, { type: 'int24' }, { type: 'address' }],
      [k.currency0, k.currency1, k.fee, k.tickSpacing, k.hooks],
    ),
  );
}

/** The pool id a key DERIVES. ⛔ The hook stays the source of a launch's id (`hook.poolForAccount(launch)`); this is for
 *  checking that a key the caller is about to sign hashes to it. */
export function poolIdOf(token: Address, hook: Address, quote: Address = NATIVE): Hex {
  return poolIdOfKey(poolKey(token, hook, quote));
}

/** v4 `PoolManager._pools` mapping slot. */
export const POOLS_SLOT = 6n;
/** The storage slot of a pool's `Slot0` word in the PoolManager, read through `extsload`. */
export function poolStateSlot(poolId: Hex): Hex {
  return keccak256(encodeAbiParameters([{ type: 'bytes32' }, { type: 'uint256' }], [poolId, POOLS_SLOT]));
}

export interface Slot0 {
  sqrtPriceX96: bigint;
  tick: number;
  protocolFee: number;
  lpFee: number;
}

export function decodeSlot0(word: Hex): Slot0 {
  const v = BigInt(word);
  const raw = Number((v >> 160n) & 0xffffffn);
  return {
    sqrtPriceX96: v & ((1n << 160n) - 1n),
    tick: raw >= 0x800000 ? raw - 0x1000000 : raw,
    protocolFee: Number((v >> 184n) & 0xffffffn),
    lpFee: Number((v >> 208n) & 0xffffffn),
  };
}

const MAX_U256 = (1n << 256n) - 1n;

/** v4-core `TickMath.getSqrtPriceAtTick`, exactly: Q64.96, rounded up out of Q128.128. */
export function sqrtPriceAtTick(tick: number): bigint {
  if (!Number.isInteger(tick) || tick < MIN_TICK || tick > MAX_TICK) throw new RangeError(`sqrtPriceAtTick: tick ${tick} out of range`);
  const abs = BigInt(Math.abs(tick));
  let r = (abs & 0x1n) !== 0n ? 0xfffcb933bd6fad37aa2d162d1a594001n : 0x100000000000000000000000000000000n;
  const M: [bigint, bigint][] = [
    [0x2n, 0xfff97272373d413259a46990580e213an],
    [0x4n, 0xfff2e50f5f656932ef12357cf3c7fdccn],
    [0x8n, 0xffe5caca7e10e4e61c3624eaa0941cd0n],
    [0x10n, 0xffcb9843d60f6159c9db58835c926644n],
    [0x20n, 0xff973b41fa98c081472e6896dfb254c0n],
    [0x40n, 0xff2ea16466c96a3843ec78b326b52861n],
    [0x80n, 0xfe5dee046a99a2a811c461f1969c3053n],
    [0x100n, 0xfcbe86c7900a88aedcffc83b479aa3a4n],
    [0x200n, 0xf987a7253ac413176f2b074cf7815e54n],
    [0x400n, 0xf3392b0822b70005940c7a398e4b70f3n],
    [0x800n, 0xe7159475a2c29b7443b29c7fa6e889d9n],
    [0x1000n, 0xd097f3bdfd2022b8845ad8f792aa5825n],
    [0x2000n, 0xa9f746462d870fdf8a65dc1f90e061e5n],
    [0x4000n, 0x70d869a156d2a1b890bb3df62baf32f7n],
    [0x8000n, 0x31be135f97d08fd981231505542fcfa6n],
    [0x10000n, 0x9aa508b5b7a84e1c677de54f3e99bc9n],
    [0x20000n, 0x5d6af8dedb81196699c329225ee604n],
    [0x40000n, 0x2216e584f5fa1ea926041bedfe98n],
    [0x80000n, 0x48a170391f7dc42444e8fa2n],
  ];
  for (const [bit, m] of M) if ((abs & bit) !== 0n) r = (r * m) >> 128n;
  if (tick > 0) r = MAX_U256 / r;
  return (r >> 32n) + (r % (1n << 32n) === 0n ? 0n : 1n);
}

/**
 * The price at an ENGINE-FRAME tick (quote at currency0 for every pool, so a dearer token is a LOWER tick), as WHOLE quote
 * per WHOLE token scaled by 1e18 — the shape of `@floor/sdk` `Launch.floorPriceX18`. Rounded down.
 *
 * `1.0001^-tick` is raw quote per raw token (`lib/pool.ts priceFromTick`); `sqrtPriceAtTick(t)² / 2^192 = 1.0001^t`, so
 * raw-per-raw = `2^192 / sqrtP²`, and whole-per-whole × 1e18 = raw-per-raw × 10^(tokenDecimals − quoteDecimals + 18).
 */
export function priceX18AtTick(tick: number, tokenDecimals: number, quoteDecimals: number): bigint {
  const s = sqrtPriceAtTick(tick);
  const shift = tokenDecimals - quoteDecimals + 18;
  const num = 1n << 192n;
  const den = s * s;
  return shift >= 0 ? (num * 10n ** BigInt(shift)) / den : num / (den * 10n ** BigInt(-shift));
}

/** `FloorMath._ceilToSpacing(BID_BAND_TICKS, spacing)` for a positive band: the engine's `_bandTicks()`. 120 at spacing 60. */
export function bandTicks(tickSpacing: number): number {
  return Math.ceil(BID_BAND_TICKS / tickSpacing) * tickSpacing;
}

/** The pool's current tick in the ENGINE frame (`_market`): a quote at currency1 maps `-tick - 1`. ⛔ `-tick` is one tick off. */
export function marketTick(rawTick: number, quoteIsCurrency1: boolean): number {
  return quoteIsCurrency1 ? -rawTick - 1 : rawTick;
}
