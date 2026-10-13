// The shipped quoter wasm against the Rust quoter's golden vectors (`rust/fixtures/quote-vectors/floor.json`): every
// FLOOR-pool swap recorded there, replayed through `quote_swap` in the wasm, gives the recorded answer — the amounts,
// the whole fee breakdown, the prices and ticks, the arrays, the bid re-place and the hook delta, or the same refusal.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { getAddressEncoder } from '@solana/kit';

import { FLOOR_HOOK_PROGRAM_ID } from '../src/constants.ts';
import { initQuote } from '../src/quote/wasm.ts';
import { test } from './node-test.ts';

const here = dirname(fileURLToPath(import.meta.url));
const VECTORS = join(here, '../../../rust/fixtures/quote-vectors/floor.json');

interface Swap {
  zero_for_one: boolean; amount_specified: string; sqrt_price_limit_x64: string; supply1: string; cushion_top: number | null;
  expect: { ok?: Record<string, unknown>; err?: string };
}
interface Case { name: string; pool_key: string; pool: string; bitmap: string | null; floor_pool: string | null; tick_arrays: string[]; swaps: Swap[] }

const hex = (s: string | null) => (s === null ? new Uint8Array(0) : Uint8Array.from(Buffer.from(s, 'hex')));

/** `Pool.hook`: the 8-byte discriminator, then the struct's bytes 88..120. */
const POOL_HOOK = 8 + 88;

/**
 * The vectors name the `floor_hook` id of the deployment they were recorded under (the first case's pool hook), which
 * the shipped wasm does not list; those pools are replayed under this build's devnet `floor_hook`. Any other hook
 * (a test-only one) is kept. The recorded answers do not include the pool's bytes.
 */
function rehook(pool: Uint8Array, recorded: Uint8Array): Uint8Array {
  const current = getAddressEncoder().encode(FLOOR_HOOK_PROGRAM_ID);
  const hook = pool.subarray(POOL_HOOK, POOL_HOOK + 32);
  if (Buffer.from(hook).equals(Buffer.from(recorded))) pool.set(current, POOL_HOOK);
  return pool;
}

test('the shipped wasm answers every FLOOR-pool vector exactly as the Rust quoter recorded it', async () => {
  await initQuote(readFileSync(join(here, '../wasm/floor_quote_bg.wasm')));
  const glue = await import('../wasm/floor_quote.js');
  const { cases } = JSON.parse(readFileSync(VECTORS, 'utf8')) as { cases: Case[] };
  const recorded = hex(cases[0]!.pool).slice(POOL_HOOK, POOL_HOOK + 32);
  let answered = 0;
  let refused = 0;
  let replaced = 0;
  for (const c of cases) {
    // The stored FloorPool states predate the program's `ladder_seeded` check; the Rust test replays them seeded too.
    const floorPool = hex(c.floor_pool);
    if (floorPool.length > 12) floorPool[12] = 1;
    const bodies = c.tick_arrays.map((a) => hex(a).subarray(0, 3_888));
    const arrays = new Uint8Array(bodies.length * 3_888);
    bodies.forEach((b, i) => arrays.set(b, i * 3_888));
    for (const [n, s] of c.swaps.entries()) {
      if (s.cushion_top !== null) continue; // the wasm reads the cushion top from the account's tail, which these states lack
      const out = JSON.parse(glue.quote_swap(hex(c.pool_key), rehook(hex(c.pool), recorded), hex(c.bitmap), arrays, floorPool, s.zero_for_one, s.amount_specified, s.sqrt_price_limit_x64, s.supply1)) as {
        ok?: Record<string, unknown>; err?: string;
      };
      const what = `${c.name} #${n}`;
      if (s.expect.err !== undefined) {
        assert.equal(out.err, s.expect.err, what);
        refused++;
        continue;
      }
      const want = s.expect.ok!;
      const got = out.ok;
      assert.ok(got, `${what}: ${JSON.stringify(out)}`);
      for (const k of ['amount_in', 'amount_out', 'sqrt_after', 'tick_after', 'sqrt_after_trade', 'tick_after_trade', 'arrays_touched', 'action_arrays', 'all_arrays', 'signals']) {
        assert.deepEqual(got[k], want[k], `${what} ${k}`);
      }
      assert.deepEqual(got['fees'], want['fees'], `${what} fees`);
      assert.deepEqual([got['hook_delta0'], got['hook_delta1']], want['hook_delta'], `${what} hook delta`);
      const r = want['replace'] as Record<string, unknown> | null;
      if (r === null) assert.equal(got['replace'], null, `${what} replace`);
      else {
        replaced++;
        for (const [k, v] of Object.entries(r)) assert.deepEqual((got['replace'] as Record<string, unknown>)[k], v, `${what} replace.${k}`);
      }
      answered++;
    }
  }
  assert.ok(answered > 250 && refused > 100 && replaced >= 10, `answered ${answered}, refused ${refused}, re-placed ${replaced}`);
});
