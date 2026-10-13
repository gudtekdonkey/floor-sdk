/**
 * FLOOR's tick arithmetic: `open_tick`, the staircase edges, the tranche price and the quote lineage's conversions.
 * The AMM's tick math (`getSqrtPriceAtTick`, `getTickAtSqrtPrice`, the spacing snaps, tick-array starts) is
 * `@carpenter/sdk`'s.
 */

import {
  getSqrtPriceAtTick,
  getTickAtSqrtPrice,
  isqrt,
  MAX_TICK,
  MIN_TICK,
  MAX_SQRT_PRICE_X64,
  MIN_SQRT_PRICE_X64,
  ceilToSpacing,
} from '@carpenter/sdk';

/**
 *  `open_tick(price_x64)`: `sqrt = isqrt(2^192 / price_x64)` (U256), then
 * `tick_at_sqrt_price`. The pool is initialized at `sqrt_price_at_tick(open_tick)`.
 */
export function openTick(priceX64: bigint): number {
  if (priceX64 <= 0n) throw new RangeError('price_x64 must be positive');
  return getTickAtSqrtPrice(isqrt((1n << 192n) / priceX64));
}

/**
 * `floor_math::stair::doubling_ticks(i)`:
 * ticks for `i` doublings of the token price, `get_tick_at_sqrt_price(sqrt_price_from_ratio(1, 2^i)) + 1` =
 * `ceil(i · ln 2 / ln 1.0001)`, computed per `i` (never `i · 6,932`). 0 for 0.
 */
export function doublingTicks(i: number): number {
  if (!Number.isInteger(i) || i < 0 || i > 64) throw new RangeError(`doublingTicks: ${i}`);
  if (i === 0) return 0;
  let sqrt = isqrt(1n << BigInt(96 + i)) << 16n;
  if (sqrt < MIN_SQRT_PRICE_X64) sqrt = MIN_SQRT_PRICE_X64;
  if (sqrt >= MAX_SQRT_PRICE_X64) sqrt = MAX_SQRT_PRICE_X64 - 1n;
  return getTickAtSqrtPrice(sqrt) + 1;
}

/** `floor_math::stair::stair_edge`: the staircase's cheap edge `i` doublings above `openEdge`, on the grid. Step i is
 *  `[stairEdge(i + 1), stairEdge(i)]`; the reserve from step `f` is `[stairEdge(steps), stairEdge(f)]`; the tail
 *  `[ceilToSpacing(MIN_TICK), stairEdge(steps)]`. */
export function stairEdge(openEdge: number, i: number, spacing: number): number {
  return openEdge - ceilToSpacing(doublingTicks(i), spacing);
}

/**
 * `floor_math::stair::cushion_home_top`, step for step: the top a cushion takes when `min(spot, TWAP)` has no
 * range above the bid band — the CHEAPEST full-depth range just above the band, `[dear − ceilToSpacing(width), dear]`
 * (`dear = floorTick − ceilToSpacing(BID_BAND_TICKS)`), or the spot when the spot is cheaper still (in ticks the HIGHER).
 */
export function cushionHomeTop(spotTick: number, floorTick: number, spacing: number, width: number, bandTicks = 120): number {
  const dear = floorTick - ceilToSpacing(bandTicks, spacing);
  return Math.max(spotTick, dear - ceilToSpacing(width, spacing) - 1);
}

/** `floor_math::stair::stair_fits`: a staircase of `steps` from `openEdge` leaves at least one spacing for the open tail
 *  above it (`stairEdge(steps) − spacing ≥ ceilToSpacing(MIN_TICK)`). `resolve_config` refuses a launch that does not
 *  (`StairDoesNotFit`, 6059) — a quote and a cap whose open tick sits too near the top of the range. */
export function stairFits(openEdge: number, steps: number, spacing: number): boolean {
  return stairEdge(openEdge, steps, spacing) - spacing >= ceilToSpacing(MIN_TICK, spacing);
}

/** The admissible decimals of a launch's QUOTE mint, inclusive — the twin of `floor_math::constants::MIN/MAX_QUOTE_DECIMALS`;
 * `floor_hook::list_quote` refuses a mint outside it and `tranche_price_x64` refuses to price one. The only thing the
 * arithmetic bounds is the tranche price itself, `0 < p < 2^77`: past `d = 36` no (cap, quote price) a u64 can carry
 * prices a launch (`p_min(d) = 100·10^d·2^64/(u64::MAX·SUPPLY) >= 2^77` from 37 up), so 36 is derived, not chosen.
 * ⚠ The EFFECTIVE ceiling for a given launch is far lower and value-shaped: {@link tranchePriceX64} throws
 * `tranche price out of range` once `mcap·100·10^d/(cents·SUPPLY) >= 2^13` — `d <= 14` at a $30k cap against a $1.00
 * quote. */
export const MIN_QUOTE_DECIMALS = 0;
export const MAX_QUOTE_DECIMALS = 36;

/**
 * `tranche_price_x64(mcap_usd, quote_usd_cents, quote_decimals)`:
 * `floor(mcap_usd·100·10^d·2^64 / (quote_usd_cents·SUPPLY))`, required `0 < price < 2^77`.
 *
 * ⛔ The quote price is in CENTS and the quote's decimals are explicit, so a $1.00 quote prices exactly. This is the ONE
 * place a decimals count enters the mechanism: everything downstream (`openTick`, the sale walk, the fee split) is raw
 * base units and Q64.64 and reads no decimals at all.
 */
export function tranchePriceX64(mcapUsd: bigint, quoteUsdCents: bigint, quoteDecimals: number): bigint {
  if (mcapUsd <= 0n || quoteUsdCents <= 0n) throw new RangeError('mcap_usd and quote_usd_cents must be positive');
  if (!Number.isInteger(quoteDecimals) || quoteDecimals < MIN_QUOTE_DECIMALS || quoteDecimals > MAX_QUOTE_DECIMALS) {
    throw new RangeError(`quote decimals out of range: ${quoteDecimals} (must be ${MIN_QUOTE_DECIMALS}..${MAX_QUOTE_DECIMALS})`);
  }
  const unit = 10n ** BigInt(quoteDecimals);
  const p = (mcapUsd * 100n * unit * (1n << 64n)) / (quoteUsdCents * 1_000_000_000_000_000n);
  if (p <= 0n || p >= 1n << 77n) throw new RangeError(`tranche price out of range: ${p}`);
  return p;
}

/**
 * The twins of `floor_math::lineage`.
 * `minBidFor(d)` = `ceil(10^d / 1,000)`, at least 1, saturating at u64::MAX: the default `min_bid` of a quote no
 * governance roster names. `quoteAmountAtTick(amount, t)` = `amount · sqrt(t)^2 / 2^128` in two `>> 64` steps,
 * rounded down, saturating at u64::MAX, 0 off the tick range: the smart tip cap's conversion (the parent's raw
 * `max_bounty` → raw units of the parent's base, at the parent's 1,800 s TWAP tick).
 */
export const SMART_CAP_TWAP_WINDOW_SECS = 1_800;
const U64_MAX = (1n << 64n) - 1n;
export function minBidFor(decimals: number): bigint {
  if (!Number.isInteger(decimals) || decimals < 0) throw new RangeError(`decimals: ${decimals}`);
  if (decimals > 38) return U64_MAX;
  const unit = 10n ** BigInt(decimals);
  const v = (unit + 999n) / 1000n;
  const r = v < 1n ? 1n : v;
  return r > U64_MAX ? U64_MAX : r;
}
export function quoteAmountAtSqrt(amount: bigint, sqrtPriceX64: bigint): bigint {
  if (amount === 0n || sqrtPriceX64 === 0n) return 0n;
  const a = (amount * sqrtPriceX64) >> 64n;
  const v = (a * sqrtPriceX64) >> 64n;
  return v > U64_MAX ? U64_MAX : v;
}
export function quoteAmountAtTick(amount: bigint, tick: number): bigint {
  if (!Number.isInteger(tick) || tick < MIN_TICK || tick > MAX_TICK) return 0n;
  return quoteAmountAtSqrt(amount, getSqrtPriceAtTick(tick));
}
