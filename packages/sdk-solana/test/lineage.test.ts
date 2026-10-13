import { test } from './node-test.ts';
import assert from 'node:assert/strict';
import { minBidFor, quoteAmountAtTick, SMART_CAP_TWAP_WINDOW_SECS } from '../src/tickmath.ts';

// The twins of `floor_math::lineage` (its own tests, same vectors).
test('minBidFor is a thousandth of a token rounded up, never zero', () => {
  assert.equal(minBidFor(0), 1n);
  assert.equal(minBidFor(3), 1n);
  assert.equal(minBidFor(4), 10n);
  assert.equal(minBidFor(6), 1_000n);
  assert.equal(minBidFor(9), 1_000_000n);
  assert.equal(minBidFor(18), 1_000_000_000_000_000n);
  assert.equal(minBidFor(36), (1n << 64n) - 1n);
});

test('the smart cap converts at the raw ratio and rides two levels through decimals', () => {
  const at0 = quoteAmountAtTick(1_000_000n, 0);
  assert.ok(at0 >= 999_999n && at0 <= 1_000_000n);
  const f = quoteAmountAtTick(50_000_000n, 23_027);
  assert.ok(f >= 499_000_000n && f <= 500_100_000n, `${f}`);
  const g = quoteAmountAtTick(f, 6_932);
  assert.ok(g >= 998_000_000n && g <= 1_000_100_000n, `${g}`);
  assert.equal(quoteAmountAtTick(1_000n, 1_000_000), 0n);
  assert.equal(SMART_CAP_TWAP_WINDOW_SECS, 1_800);
});
