
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { chainKeyFromGenesisHash, DEVNET_CHAIN_KEY, MAINNET_CHAIN_KEY } from '@floorsupply/carpenter-sdk';

import { deploymentFor, PROGRAM_NAMES, type DeploymentsFile } from '../src/deployments.ts';
import { REPO_ROOT } from './helpers.ts';
import { test } from './node-test.ts';

const here = dirname(fileURLToPath(import.meta.url));
const file = JSON.parse(readFileSync(join(here, '../deployments.json'), 'utf8')) as DeploymentsFile;
const B58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;

test('every cluster row is complete and keyed by a CAIP-2 id or `local`', () => {
  assert.equal(file.version, 1);
  const keys = Object.keys(file.clusters);
  assert.ok(keys.includes(MAINNET_CHAIN_KEY));
  assert.ok(keys.includes(DEVNET_CHAIN_KEY));
  assert.ok(keys.includes('local'));
  for (const [key, row] of Object.entries(file.clusters)) {
    assert.ok(['mainnet', 'devnet', 'local'].includes(row.name), key);
    if (key === 'local') assert.equal(row.chainKey, null);
    else {
      assert.equal(row.chainKey, key);
      assert.match(key, /^solana:[1-9A-HJ-NP-Za-km-z]{32}$/);
    }
    for (const p of ['floorHook', 'floorLaunch', 'floorHolders', 'floorVesting'] as const) assert.match(row.programs[p], B58, `${key}.programs.${p}`);
    // The round distributor is deleted: no row may name it again, in either table.
    assert.equal((row.programs as Record<string, unknown>)['floorRewards'], undefined, `${key}.programs.floorRewards is deleted`);
    assert.equal((row.programDataHash as Record<string, unknown>)['floorRewards'], undefined, `${key}.programDataHash.floorRewards is deleted`);
    // A FLOOR row names no Carpenter program: that is `@floorsupply/carpenter-sdk`'s file.
    assert.equal((row.programs as Record<string, unknown>)['carpenterAmm'], undefined, `${key}: carpenterAmm lives in @floorsupply/carpenter-sdk`);
    for (const a of ['hookConfig', 'launchConfig'] as const) assert.match(row.accounts[a], B58, `${key}.accounts.${a}`);
    assert.ok(row.alt === null || typeof row.alt === 'string');
  }
  assert.deepEqual([...PROGRAM_NAMES], ['floor_hook', 'floor_launch', 'floor_holders', 'floor_vesting']);
});

test('deploymentFor throws by name; the joined row carries both packages\' ids; chainKeyFromGenesisHash takes 32 characters', () => {
  assert.throws(() => deploymentFor('solana:nope'), /no deployment for cluster "solana:nope"/);
  const row = deploymentFor(DEVNET_CHAIN_KEY);
  assert.match(row.programs.carpenterAmm, B58);
  assert.match(row.accounts.ammConfig, B58);
  assert.equal(row.programs.floorHook, file.clusters[DEVNET_CHAIN_KEY]!.programs.floorHook);
  assert.equal(chainKeyFromGenesisHash('EtWTRABZaYq6iMfeYKouRu166VU2xqa1Cr7xE9bbG1vv'), DEVNET_CHAIN_KEY);
});

const sourceFile = join(REPO_ROOT, 'clients/js/packages/idl/deployments.json');
test('the joined rows equal the source repository deployments.json, row for row (source checkout)', (t) => {
  if (!existsSync(sourceFile)) {
    t.skip(`no source checkout at ${REPO_ROOT}`);
    return;
  }
  const src = JSON.parse(readFileSync(sourceFile, 'utf8')) as { clusters: Record<string, unknown> };
  assert.deepEqual(Object.keys(src.clusters).sort(), Object.keys(file.clusters).sort());
  // The public copy keeps addresses and ids and drops the source's record-keeping: transaction signatures, update times
  // and the genesis record.
  const publicCopy = (row: unknown) => {
    const r = structuredClone(row) as { genesis?: unknown; quotes?: Record<string, { updatedAt?: unknown; hook?: { signatures?: unknown }; amm?: { signatures?: unknown } }> };
    delete r.genesis;
    for (const q of Object.values(r.quotes ?? {})) {
      delete q.updatedAt;
      delete q.hook?.signatures;
      delete q.amm?.signatures;
    }
    return r;
  };
  for (const key of Object.keys(src.clusters)) assert.deepEqual(deploymentFor(key), publicCopy(src.clusters[key]), key);
});

test('the mainnet row\'s program ids equal Anchor.toml\'s program-id map (source checkout)', (t) => {
  const toml = join(REPO_ROOT, 'Anchor.toml');
  if (!existsSync(toml)) {
    t.skip(`no source checkout at ${REPO_ROOT}`);
    return;
  }
  const section = /\[programs\.devnet\]([\s\S]*?)(?:\n\[|$)/.exec(readFileSync(toml, 'utf8'))?.[1] ?? '';
  const ids = Object.fromEntries([...section.matchAll(/^(\w+) = "([^"]+)"/gm)].map((m) => [m[1], m[2]]));
  const row = file.clusters[MAINNET_CHAIN_KEY]!;
  assert.equal(row.programs.floorHook, ids['floor_hook']);
  assert.equal(row.programs.floorLaunch, ids['floor_launch']);
  assert.equal(row.programs.floorHolders, ids['floor_holders']);
  assert.equal(row.programs.floorVesting, ids['floor_vesting']);
  assert.equal(ids['floor_rewards'], undefined, 'floor_rewards is deleted');
});

test('the public deployments.json carries addresses and ids only: no transaction signatures, update times or genesis record', () => {
  const json = readFileSync(join(here, '../deployments.json'), 'utf8');
  assert.doesNotMatch(json, /signatures|updatedAt|genesis/);
});
