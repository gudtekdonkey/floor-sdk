// The shipped quoter wasm (`wasm/floor_quote_bg.wasm`) on the bonding sale's sell-back: the payout is
// `floor(tokens · price_x64 / 2^64)` at the launch's price, less `floor(payout · team_bps / 10 000)` — read from the
// `Launch` alone; the position only has to cover `tokens`. Skipped by name when the wasm has not been synced in.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { encodeAccount, QuoteError } from '@floorsupply/carpenter-sdk';

import { LAUNCH_STATE_BONDING } from '../src/constants.ts';
import { BUYER_POSITION, LAUNCH } from '../src/layouts.ts';
import { initQuote, WASM_SHA256 } from '../src/quote/wasm.ts';
import { fixtureLaunch, SDK_ROOT } from './helpers.ts';
import { test } from './node-test.ts';

const wasmPath = join(SDK_ROOT, 'wasm/floor_quote_bg.wasm');
const present = existsSync(wasmPath) && existsSync(join(SDK_ROOT, 'wasm/floor_quote.js'));

test('the shipped wasm: the stateless sell-back, and the position only checked for coverage', { skip: !present && 'wasm/ not synced (npm run sync-wasm)' }, async () => {
  const bytes = readFileSync(wasmPath);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), WASM_SHA256, 'wasm/ is the build src/generated/wasm.ts names');
  const q = await initQuote(new Uint8Array(bytes));
  const f = await fixtureLaunch();
  const launch = (teamBps: number, state = LAUNCH_STATE_BONDING) =>
    encodeAccount(LAUNCH, { ...f.state, state, priceX64: 3n << 63n /* 1.5 base units of quote a token unit */, teamBps });
  // a position carrying 7 units of its own rounding: the old pro-rata rule would pay floor(4_507 · 1_000 / 3_000) = 1_502
  const pos = encodeAccount(BUYER_POSITION, { allocation: 3_000n, contributed: 4_507n });
  assert.deepEqual(q.saleSellBack(launch(25), pos, 1_000n), { payout: 1_500n, teamFee: 3n, received: 1_497n });
  assert.deepEqual(q.saleSellBack(launch(25), pos, 3_000n), { payout: 4_500n, teamFee: 11n, received: 4_489n });
  assert.equal(q.saleSellBack(launch(0), pos, 1_001n).payout, 1_501n, 'floor(1_001 · 1.5)');
  // any covering position quotes the same
  const other = encodeAccount(BUYER_POSITION, { allocation: 1_000_000n, contributed: 9_999_999n });
  for (const t of [1n, 2n, 999n, 3_000n]) assert.deepEqual(q.saleSellBack(launch(100), pos, t), q.saleSellBack(launch(100), other, t));
  assert.throws(() => q.saleSellBack(launch(25), pos, 3_001n), (e: unknown) => e instanceof QuoteError && /InsufficientAllocation/.test(e.message));
  assert.throws(() => q.saleSellBack(launch(25, LAUNCH_STATE_BONDING + 1), pos, 1n), (e: unknown) => e instanceof QuoteError && /SaleOver/.test(e.message));
});
