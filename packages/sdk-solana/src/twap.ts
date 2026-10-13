/**
 * The pool-price TWAP's arithmetic, off chain — a line-for-line twin of `floor_types::twap::TwapOracle` (`observe`,
 * `twap_tick`, `settled_tick`) over the oracle tail of a `FloorPool` account (`decodeFloorPoolTwap`).
 * ⛔ Arithmetic wraps exactly as the program's: the cumulative is an `i64` (`BigInt.asIntN(64, …)`), times are `u32`
 *    (`>>> 0`), and a write whose clock reads BEHIND the stored timestamp is a no-op.
 * ⚠ A simulated `twap_tick` reads before any write in the same transaction and refuses a ring younger than the window
 *   (`TwapWindowTooLong`), where `settled_tick` answers over the oldest checkpoint.
 */
import { CUSHION_TWAP_WINDOW_SECS } from './constants.ts';
import { TWAP_CHECKPOINT_SPACING_SECS, TWAP_CHECKPOINTS, type TwapCheckpointView, type TwapOracleView } from './layouts.ts';

/** `floor_types::twap::TWAP_VERSION`: the oracle's version once written. */
export const TWAP_VERSION = 1;

/** Why a window read has no answer (`floor_types::twap::TwapError`). */
export type TwapReadError = 'Uninitialized' | 'WindowTooLong';
/** A window read: the tick, or why there is none. */
export type TwapRead = { readonly tick: number } | { readonly error: TwapReadError };

const u32 = (x: number): number => x >>> 0;
const i64 = (x: bigint): bigint => BigInt.asIntN(64, x);
const I32_MIN = -(2 ** 31);
const I32_MAX = 2 ** 31 - 1;

/** `a / b` rounded toward negative infinity (`b > 0`), `floor_types::twap::floor_div`. */
export function twapFloorDiv(a: bigint, b: bigint): bigint {
  const q = a / b;
  return a % b !== 0n && a < 0n !== b < 0n ? q - 1n : q;
}

function initialized(o: TwapOracleView): boolean {
  return o.version !== 0 && o.count !== 0;
}

function ring(o: TwapOracleView, i: number): TwapCheckpointView {
  return o.checkpoints[i % TWAP_CHECKPOINTS] ?? { tickCumulative: 0n, timestamp: 0 };
}

/** The oracle a zero tail decodes to: never written. */
export function emptyTwapOracle(): TwapOracleView {
  return {
    version: 0,
    newest: 0,
    count: 0,
    current: { tickCumulative: 0n, timestamp: 0, lastTick: 0 },
    checkpoints: Array.from({ length: TWAP_CHECKPOINTS }, () => ({ tickCumulative: 0n, timestamp: 0 })),
    olderSurplusHeld: 0,
    cushionTopSet: 0,
    cushionTopTick: 0,
    netFlow: { a: 0n, b: 0n },
  };
}

/** The cushion-top cache and the net-flow meter: carried through every oracle write as the program does
 *  (`TwapOracle::observe` keeps both; the meter is `after_swap`'s, the cache the ratchet's). */
function keptCache(o: TwapOracleView): { cushionTopSet?: number; cushionTopTick?: number; netFlow?: { a: bigint; b: bigint } } {
  return {
    ...(o.cushionTopSet === undefined ? {} : { cushionTopSet: o.cushionTopSet, cushionTopTick: o.cushionTopTick ?? 0 }),
    ...(o.netFlow === undefined ? {} : { netFlow: { a: o.netFlow.a, b: o.netFlow.b } }),
  };
}

/**
 * THE WRITE (`TwapOracle::observe`): `tick` is the pool's tick BEFORE whatever is about to move it, at unix second `now`.
 * Returns the oracle after the write (the input is never mutated); the same object when the write is a no-op (the same
 * second, or a clock behind the last write).
 */
export function twapObserve(o: TwapOracleView, now: number, tick: number): TwapOracleView {
  const t = u32(now);
  if (!initialized(o)) {
    const checkpoints = emptyTwapOracle().checkpoints.slice();
    checkpoints[0] = { tickCumulative: 0n, timestamp: t };
    return { version: TWAP_VERSION, newest: 0, count: 1, current: { tickCumulative: 0n, timestamp: t, lastTick: tick }, checkpoints, olderSurplusHeld: o.olderSurplusHeld ?? 0, ...keptCache(o) };
  }
  const elapsed = u32(t - o.current.timestamp);
  if (elapsed === 0 || (elapsed | 0) < 0) return o;
  const cum = i64(o.current.tickCumulative + i64(BigInt(tick) * BigInt(elapsed)));
  const checkpoints = o.checkpoints.slice();
  let { newest, count } = o;
  if (u32(t - ring(o, o.newest).timestamp) >= TWAP_CHECKPOINT_SPACING_SECS) {
    newest = (o.newest + 1) % TWAP_CHECKPOINTS;
    checkpoints[newest] = { tickCumulative: cum, timestamp: t };
    count = Math.min(o.count + 1, TWAP_CHECKPOINTS);
  }
  return { version: o.version, newest, count, current: { tickCumulative: cum, timestamp: t, lastTick: tick }, checkpoints, olderSurplusHeld: o.olderSurplusHeld ?? 0, ...keptCache(o) };
}

/** The cumulative at `now`, the span since the last write extrapolated with `liveTick` (`cumulative_at`). */
function cumulativeAt(o: TwapOracleView, now: number, liveTick: number): bigint {
  return i64(o.current.tickCumulative + i64(BigInt(liveTick) * BigInt(u32(now - o.current.timestamp))));
}

/**
 * THE WINDOW READ (`TwapOracle::twap_tick`, the `twap_tick` instruction's answer): the time-weighted mean RAW tick over
 * the newest checkpoint at least `window` seconds old to now; window 0 is the live tick; the live tick exactly when nothing
 * has written for the whole window.
 */
export function twapTickAt(o: TwapOracleView, now: number, liveTick: number, window: number): TwapRead {
  if (window === 0) return { tick: liveTick };
  if (!initialized(o)) return { error: 'Uninitialized' };
  const t = u32(now);
  if (u32(t - o.current.timestamp) >= window) return { tick: liveTick };
  const cumNow = cumulativeAt(o, t, liveTick);
  const count = Math.min(o.count, TWAP_CHECKPOINTS);
  for (let back = 0; back < count; back++) {
    const cp = ring(o, o.newest + TWAP_CHECKPOINTS - back);
    const age = u32(t - cp.timestamp);
    if (age >= window) {
      const avg = twapFloorDiv(i64(cumNow - cp.tickCumulative), BigInt(age));
      return { tick: Number(avg < BigInt(I32_MIN) ? BigInt(I32_MIN) : avg > BigInt(I32_MAX) ? BigInt(I32_MAX) : avg) };
    }
  }
  return { error: 'WindowTooLong' };
}

/**
 * THE SETTLED TICK (`TwapOracle::settled_tick`, what the staircase's cushion reads): {@link twapTickAt} over `window`, and
 * when the ring is younger than `window`, over its OLDEST checkpoint's age; when even that is 0, the tick at the start of
 * this second (`current.lastTick`).
 */
export function settledTickAt(o: TwapOracleView, now: number, liveTick: number, window: number = CUSHION_TWAP_WINDOW_SECS): TwapRead {
  const r = twapTickAt(o, now, liveTick, window);
  if ('error' in r && r.error === 'WindowTooLong') {
    const count = Math.min(Math.max(o.count, 1), TWAP_CHECKPOINTS);
    const oldest = ring(o, o.newest + TWAP_CHECKPOINTS + 1 - count);
    const age = u32(u32(now) - oldest.timestamp);
    return age > 0 && (age | 0) > 0 ? twapTickAt(o, now, liveTick, age) : { tick: o.current.lastTick };
  }
  return r;
}
