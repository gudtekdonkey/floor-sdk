/**
 * Three-tier referral dues. `referralTierDuesOf` is a PORT of `floor_math::referral_tier_dues`
 * (`crates/floor-math/src/referral.rs`), the one spelling the program's `claim_referral_fees` runs; these
 * are the SAME vectors that crate's tests pin, so a drift in either shows up as a failure on one side.
 * Plus `decodeFloorPool`'s derived `referralTiers` / `referralTierDues` off real account bytes.
 */
import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import type { Address } from '@solana/kit';

import { SYSTEM_PROGRAM_ID, encodeAccount } from '@floorlaunchpad/carpenter-sdk';
import { decodeFloorPool, FLOOR_POOL, referralTierDuesOf } from '../src/layouts.ts';
import { addr } from './helpers.ts';

const Z = SYSTEM_PROGRAM_ID as Address;
const W2 = addr(2);
const W3 = addr(3);

function pool(tiers: 1 | 2 | 3, booked: bigint) {
  return {
    referralFeesPaid: booked,
    referralOwed: booked,
    referralPaidT2: 0n,
    referralPaidT3: 0n,
    referrerT2: tiers >= 2 ? W2 : Z,
    referrerT3: tiers >= 3 ? W3 : Z,
  };
}

test('the tier table: 500 → 500 / 450,50 / 450,45,5 (the same vectors as floor-math)', () => {
  assert.deepEqual(referralTierDuesOf(pool(1, 500n)), [500n, 0n, 0n]);
  assert.deepEqual(referralTierDuesOf(pool(2, 500n)), [450n, 50n, 0n]);
  assert.deepEqual(referralTierDuesOf(pool(3, 500n)), [450n, 45n, 5n]);
  assert.deepEqual(referralTierDuesOf(pool(3, 1_000n)), [900n, 90n, 10n], "Bluechip's 1 % doubles every figure");
  // a tier 3 behind a gap earns nothing (the chain refuses the gap; this is belt and braces)
  assert.deepEqual(referralTierDuesOf({ ...pool(1, 500n), referrerT3: W3 }), [500n, 0n, 0n]);
});

test('exact and monotone over a range, every tier count', () => {
  for (const t of [1, 2, 3] as const) {
    let prev = [0n, 0n, 0n];
    for (let l = 0n; l < 20_000n; l++) {
      const d = referralTierDuesOf(pool(t, l));
      assert.equal(d[0] + d[1] + d[2], l, `exact at L=${l}`);
      for (let i = 0; i < 3; i++) assert.ok(d[i]! >= prev[i]!, `due[${i}] fell at L=${l} (${t} tiers)`);
      prev = d;
    }
  }
});

test('a pre-upgrade pool that already claimed pays exactly referral_owed; claim order does not matter', () => {
  const legacy = { ...pool(1, 98_235n + 1_234n), referralOwed: 1_234n };
  assert.deepEqual(referralTierDuesOf(legacy), [1_234n, 0n, 0n]);

  const claim = (p: ReturnType<typeof pool>, tier: 1 | 2 | 3) => {
    const due = referralTierDuesOf(p)[tier - 1]!;
    return {
      ...p,
      referralOwed: p.referralOwed - due,
      referralPaidT2: p.referralPaidT2 + (tier === 2 ? due : 0n),
      referralPaidT3: p.referralPaidT3 + (tier === 3 ? due : 0n),
    };
  };
  const orders: (1 | 2 | 3)[][] = [[1, 2, 3], [3, 2, 1], [2, 1, 3], [2, 3, 1]];
  for (const order of orders) {
    let p = pool(3, 12_345n);
    for (const t of order) {
      const before = referralTierDuesOf(p);
      p = claim(p, t);
      const after = referralTierDuesOf(p);
      for (const u of [1, 2, 3] as const) {
        if (u === t) assert.equal(after[u - 1], 0n);
        else assert.equal(after[u - 1], before[u - 1], `claiming tier ${t} does not move tier ${u}'s due`);
      }
    }
    assert.equal(p.referralOwed, 0n);
  }
  // an inconsistent pool (more paid out than ever booked) is due nothing, as on chain
  assert.deepEqual(referralTierDuesOf({ ...pool(3, 100n), referralPaidT2: 200n }), [0n, 0n, 0n]);
});

test('decodeFloorPool derives the tiers and their dues off the account bytes', () => {
  const data = encodeAccount(FLOOR_POOL, {
    referrer: addr(1),
    referrerT2: W2,
    referrerT3: W3,
    referralFeesPaid: 500n,
    referralOwed: 455n,
    referralPaidT2: 45n,
    referralPaidT3: 0n,
  });
  const p = decodeFloorPool(data);
  assert.deepEqual(p.referralTiers, [addr(1), W2, W3]);
  assert.deepEqual(p.referralTierDues, [450n, 0n, 5n], 'tier 2 already took its 45');
  const single = decodeFloorPool(encodeAccount(FLOOR_POOL, { referrer: addr(1), referralFeesPaid: 7n, referralOwed: 7n }));
  assert.deepEqual(single.referralTiers, [addr(1), null, null], 'a pre-upgrade / single-tier pool');
  assert.deepEqual(single.referralTierDues, [7n, 0n, 0n]);
});
