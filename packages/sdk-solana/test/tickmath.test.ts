// Tick math pins: Raydium's `get_sqrt_price_at_tick` at its bounds, the inverse's definition,
// the spacing snaps at the ±443,636 edges (FloorMath.sol), bitmap sizing against
// floor-types' own tests, `open_tick` and `tranche_price_x64`.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import {

  

  

  

  

  
  getSqrtPriceAtTick,

  

  isqrt,

} from '@floorlaunchpad/carpenter-sdk';
import { MAX_QUOTE_DECIMALS, openTick, tranchePriceX64 } from '../src/tickmath.ts';

test('tranche_price_x64 and open_tick: 20k USD at 150 USD/SOL', () => {
  const p = tranchePriceX64(20_000n, 150n * 100n, 9);
  assert.equal(p, (20_000n * (1n << 64n)) / 150_000_000n);
  assert.ok(p > 0n && p < 1n << 77n);
  // at SUPPLY 1e15 the whole supply costs mcap/usd SOL: 133.33 SOL = 133,333,333,333 lamports
  const supplyCost = (1_000_000_000_000_000n * p) >> 64n;
  assert.ok(supplyCost >= 133_333_333_000n && supplyCost <= 133_333_333_334n, `${supplyCost}`);
  // open_tick: sqrt = isqrt(2^192 / p) → the tick whose price is ≈ 1/p base units per lamport
  const t = openTick(p);
  const sqrtAt = getSqrtPriceAtTick(t);
  const sqrtNext = getSqrtPriceAtTick(t + 1);
  const sqrt = isqrt((1n << 192n) / p);
  assert.ok(sqrtAt <= sqrt && sqrt < sqrtNext, 'open_tick is tick_at_sqrt_price(isqrt(2^192 / p))');
  // pool price = base units per lamport = 1 / 1.333e-4 = 7,500 → tick = ln(7500) / ln(1.0001) ≈ 89,231
  assert.ok(t >= 89_200 && t <= 89_260, `${t}`);
  assert.throws(() => tranchePriceX64(0n, 150n * 100n, 9), RangeError);
  assert.throws(() => tranchePriceX64(1n << 90n, 100n, 9), RangeError);
  assert.ok(tranchePriceX64(20_000n, 100n, 0) > 0n);
  assert.ok(tranchePriceX64(20_000n, 100n, 2) > 0n);
  assert.equal(tranchePriceX64(20_000n, 100n, 2), tranchePriceX64(20_000n, 1n, 0));
  const U64_MAX = (1n << 64n) - 1n;
  const pMin = (d: number) => (100n * 10n ** BigInt(d) * (1n << 64n)) / (U64_MAX * 1_000_000_000_000_000n);
  assert.equal(MAX_QUOTE_DECIMALS, 36);
  assert.ok(pMin(36) < 1n << 77n && pMin(37) >= 1n << 77n, 'the window ends where p_min crosses 2^77');
  // 10^23 * 2^64 / (2^64 - 1): the 5,421 is the u64::MAX denominator, not 2^64
  assert.equal(pMin(36), 10n ** 23n + 5_421n);
  assert.equal(tranchePriceX64(1n, U64_MAX, 36), 10n ** 23n + 5_421n, 'the derivation\'s own input prices at 36');
  assert.throws(() => tranchePriceX64(1n, U64_MAX, 37), /quote decimals out of range/);
  // 20 is a VALUE refusal at an ordinary cap and price, never a decimals one
  assert.throws(() => tranchePriceX64(20_000n, 100n, 20), /tranche price out of range/);
  assert.throws(() => tranchePriceX64(20_000n, 100n, 255), /quote decimals out of range/);
  assert.throws(() => openTick(0n), RangeError);
});

