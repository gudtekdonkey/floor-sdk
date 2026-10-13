
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from './node-test.ts';

import { computeGolden, GOLDEN_VERSION } from './golden.ts';
import { SDK_ROOT } from './helpers.ts';

const file = join(SDK_ROOT, 'fixtures/golden.json');

test('fixtures/golden.json matches the SDK', async () => {
  assert.ok(existsSync(file), `${file} is missing — run \`npm run golden\``);
  const stored = JSON.parse(readFileSync(file, 'utf8')) as Record<string, unknown>;
  assert.equal(stored['version'], GOLDEN_VERSION);
  const now = await computeGolden();
  const { instructions, ...rest } = now as { instructions: Record<string, unknown> };
  // The vector is the unsplit SDK's: it also names accounts, layouts and the deployment table's contents this package
  // does not build (claims, genesis, launch creation). Every entry this package computes must equal the stored one.
  const { instructions: storedIx, pendingRecipientId: _dropped, altAddresses: _alt, ...storedRest } = stored as {
    instructions: Record<string, unknown>; pendingRecipientId: unknown; altAddresses: unknown; pdas: Record<string, unknown>; layouts: Record<string, unknown>;
  };
  const r = rest as Record<string, unknown> & { pdas: Record<string, unknown>; layouts: Record<string, unknown> };
  const only = (o: Record<string, unknown>, keys: Record<string, unknown>) => Object.fromEntries(Object.keys(keys).map((k) => [k, o[k]]));
  assert.deepEqual(r, { ...storedRest, pdas: only(storedRest.pdas, r.pdas), layouts: only(storedRest.layouts, r.layouts) });
  for (const [key, ix] of Object.entries(instructions)) assert.deepEqual(ix, storedIx[key], `instruction ${key}`);
  assert.ok(Object.keys(instructions).length > 0);
});
