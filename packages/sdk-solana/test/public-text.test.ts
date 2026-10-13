// The published IDLs and the clients generated from them carry the programs' error names and codes unchanged, and
// error messages without internal review or decision tags.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { test } from './node-test.ts';

const pkg = join(dirname(fileURLToPath(import.meta.url)), '..');
const TAG = /\bruling\b|\brulings\b|\bR-\d+|\bDV-\d+|\bD-\d+|§/i;

test('no IDL error message carries an internal tag', () => {
  for (const f of readdirSync(join(pkg, 'idl'))) {
    const idl = JSON.parse(readFileSync(join(pkg, 'idl', f), 'utf8')) as { errors?: { code: number; name: string; msg?: string }[] };
    for (const e of idl.errors ?? []) assert.doesNotMatch(e.msg ?? '', TAG, `${f} ${e.code} ${e.name}`);
  }
});

test('no generated client error message carries an internal tag', () => {
  const root = join(pkg, 'src/generated/client');
  for (const program of readdirSync(root)) {
    const dir = join(root, program, 'errors');
    for (const f of readdirSync(dir)) assert.doesNotMatch(readFileSync(join(dir, f), 'utf8'), TAG, `${program}/errors/${f}`);
  }
});
