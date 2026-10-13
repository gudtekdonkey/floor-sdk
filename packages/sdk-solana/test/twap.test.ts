// the SDK's two encoders and the tail decoder, against
// `floor_types::twap`'s offsets (current @8: cum i64, ts u32 @16, last_tick i32 @20; checkpoints @24, 16 B each).
import { test } from './node-test.ts';
import assert from 'node:assert/strict';

test('decodeFloorPoolTwap: null on a 792-byte (pre-TWAP) account; the tail at its offsets on 1,008 bytes', () => {
  assert.equal(decodeFloorPoolTwap(new Uint8Array(792)), null);
  const acct = new Uint8Array(FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP);
  const v = new DataView(acct.buffer, TWAP_ORACLE_OFFSET);
  v.setUint8(0, 1); v.setUint8(1, 2); v.setUint8(2, 3);
  v.setBigInt64(8, -123_456_789n, true); v.setUint32(16, 1_790_000_000, true); v.setInt32(20, -76_379, true);
  v.setBigInt64(24 + 16 * 2, 42n, true); v.setUint32(24 + 16 * 2 + 8, 1_789_999_400, true);
  const t = decodeFloorPoolTwap(acct)!;
  assert.deepEqual([t.version, t.newest, t.count], [1, 2, 3]);
  assert.deepEqual(t.current, { tickCumulative: -123_456_789n, timestamp: 1_790_000_000, lastTick: -76_379 });
  assert.deepEqual(t.checkpoints[2], { tickCumulative: 42n, timestamp: 1_789_999_400 });
  assert.equal(t.checkpoints.length, 8);
});

// ---- THE TWIN ----
// `src/twap.ts` replays the Rust oracle's own transcript (`crates/floor-types/tests/twap_sdk_vectors.rs` writes it), and the
// ratchet builders carry the cushion's settled target off the FloorPool's own tail when the caller passes no TWAP.

import {
  decodeFloorPoolTwap,
  FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP,
  TWAP_ORACLE_OFFSET,
} from '../src/layouts.ts';

// the keeper's twin of `mech/ratchet.rs older_surplus_due` and the flag at byte 152.
test('decodeFloorPoolTwap reads older_surplus_held at tail byte 152', async () => {
  const { TWAP_OLDER_SURPLUS_HELD_OFFSET } = ({ ...(await import('@floorsupply/carpenter-sdk')), ...(await import('../src/index.ts')) });
  assert.equal(TWAP_OLDER_SURPLUS_HELD_OFFSET, 152);
  const acct = new Uint8Array(FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP);
  assert.equal(decodeFloorPoolTwap(acct)!.olderSurplusHeld, 0);
  acct[TWAP_ORACLE_OFFSET + 152] = 1;
  assert.equal(decodeFloorPoolTwap(acct)!.olderSurplusHeld, 1);
});

// THE GAP BUY SKIM's cache: `TwapOracle::cushion_top_set` @153, `cushion_top_tick` @156,
// written by the ratchet only and carried through every oracle write.
test('decodeFloorPoolTwap reads the cushion-top cache at tail bytes 153 / 156, and twapObserve keeps it', async () => {
  const { TWAP_CUSHION_TOP_SET_OFFSET, TWAP_CUSHION_TOP_TICK_OFFSET, twapObserve } = ({ ...(await import('@floorsupply/carpenter-sdk')), ...(await import('../src/index.ts')) });
  assert.equal(TWAP_CUSHION_TOP_SET_OFFSET, 153);
  assert.equal(TWAP_CUSHION_TOP_TICK_OFFSET, 156);
  const acct = new Uint8Array(FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP);
  assert.equal(decodeFloorPoolTwap(acct)!.cushionTopSet, 0);
  acct[TWAP_ORACLE_OFFSET + 153] = 1;
  new DataView(acct.buffer).setInt32(TWAP_ORACLE_OFFSET + 156, -81_234, true);
  const o = decodeFloorPoolTwap(acct)!;
  assert.equal(o.cushionTopSet, 1);
  assert.equal(o.cushionTopTick, -81_234);
  const w = twapObserve(twapObserve(o, 1_000, -80_000), 1_400, -79_000);
  assert.equal(w.cushionTopSet, 1);
  assert.equal(w.cushionTopTick, -81_234);
});
