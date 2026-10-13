// `QuoteFns` (what the wrapper calls) == `scripts/wasm-exports.mjs` (what sync-wasm requires of
// the glue) == the `#[wasm_bindgen] pub fn`s in `rust/crates/floor-quote-wasm/src/lib.rs` (what the
// wasm exports) == the committed `wasm/floor_quote.d.ts` and glue. A function added on one side
// and not the others fails here, not in the app — and so does an argument added, dropped or
// REORDERED, the last against the synced `wasm/floor_quote.d.ts`'s own parameter names.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join,  } from 'node:path';
import { test } from './node-test.ts';
import { fileURLToPath } from 'node:url';

import { REQUIRED_EXPORTS, WEB_TARGET_MARKERS } from '../scripts/wasm-exports.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const quoterSrc = readFileSync(join(here, '../src/quote/quoter.ts'), 'utf8');
/** The public wasm crate, in this repository. */
const crateLib = join(here, '../../../rust/crates/floor-quote-wasm/src/lib.rs');

function quoteFnsBody(): string {
  const m = [...quoterSrc.matchAll(/export interface QuoteFns \{([\s\S]*?)\n\}/g)];
  assert.equal(m.length, 1, 'quoter.ts declares QuoteFns once');
  return m[0]![1] ?? '';
}

function quoteFnsNames(): string[] {
  return [...quoteFnsBody().matchAll(/^\s{2}(\w+)\(/gm)].map((x) => x[1] ?? '');
}

/** The parameter list that starts at `src[open]` (the `(`), by bracket matching. */
function paramList(src: string, open: number): string {
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    const c = src[i];
    if (c === '(' || c === '[' || c === '{' || c === '<') depth++;
    else if (c === ')' || c === ']' || c === '}' || c === '>') {
      depth--;
      if (depth === 0) return src.slice(open + 1, i);
    }
  }
  throw new Error('unbalanced parameter list');
}

/**
 * The top-level parameters of a `(…)` body, as written (no nesting in either language's signatures
 * here). ⚠ A TRAILING COMMA is not a parameter: `QuoteFns.quote_swap` is wrapped over nine lines and
 * ends `supply1: string,`, so a naive comma count read it as TEN and would have failed the arity
 * case the day crate was checked out beside this package — a false red nobody had seen,
 * because that crate is not here.
 */
function splitParams(params: string): string[] {
  const body = params.trim();
  if (body === '') return [];
  const parts: string[] = [];
  let depth = 0;
  let at = 0;
  for (let i = 0; i <= body.length; i++) {
    const c = body[i];
    if (c === '(' || c === '[' || c === '{' || c === '<') depth++;
    else if (c === ')' || c === ']' || c === '}' || c === '>') depth--;
    if (i === body.length || (c === ',' && depth === 0)) {
      parts.push(body.slice(at, i));
      at = i + 1;
    }
  }
  if (parts.length > 0 && (parts[parts.length - 1] ?? '').trim() === '') parts.pop();
  return parts;
}

function arity(params: string): number {
  return splitParams(params).length;
}

function arities(src: string, re: RegExp): Map<string, number> {
  const out = new Map<string, number>();
  for (const m of src.matchAll(re)) out.set(m[1] ?? '', arity(paramList(src, m.index + m[0].length - 1)));
  return out;
}

/** The top-level parameter names of a `(…)` body, in order (`name: Type` on both sides). */
function paramNames(params: string): string[] {
  return splitParams(params).map((p) => /^\s*(\w+)\s*:/.exec(p)?.[1] ?? p.trim());
}

function paramNamesOf(src: string, re: RegExp): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const m of src.matchAll(re)) out.set(m[1] ?? '', paramNames(paramList(src, m.index + m[0].length - 1)));
  return out;
}

test('QuoteFns names exactly the exports sync-wasm requires', () => {
  assert.deepEqual(quoteFnsNames().sort(), [...REQUIRED_EXPORTS].sort());
});

test('the wasm crate exports exactly those functions', () => {
  const src = readFileSync(crateLib, 'utf8');
  const exported = [...src.matchAll(/#\[wasm_bindgen\]\s*(?:#\[[^\]]*\]\s*)*pub fn (\w+)\(/g)].map((m) => m[1] ?? '');
  assert.deepEqual(exported.sort(), [...REQUIRED_EXPORTS].sort());
});

test('QuoteFns and the crate agree on how many arguments each export takes', () => {
  const rust = arities(readFileSync(crateLib, 'utf8'), /#\[wasm_bindgen\]\s*(?:#\[[^\]]*\]\s*)*pub fn (\w+)\(/g);
  const ts = arities(quoteFnsBody(), /^ {2}(\w+)\(/gm);
  assert.deepEqual(Object.fromEntries([...ts].sort()), Object.fromEntries([...rust].sort()));
});

test('QuoteFns and the committed floor_quote.d.ts agree on each export\'s parameter names, in order', () => {
  const dts = join(here, '../wasm/floor_quote.d.ts');
  assert.ok(existsSync(dts), 'wasm/floor_quote.d.ts is committed');
  const wasm = paramNamesOf(readFileSync(dts, 'utf8'), /^export function (\w+)\(/gm);
  const ts = paramNamesOf(quoteFnsBody(), /^ {2}(\w+)\(/gm);
  for (const name of REQUIRED_EXPORTS) {
    const want = wasm.get(name);
    assert.ok(want, `floor_quote.d.ts declares ${name}`);
    assert.deepEqual(ts.get(name), want, `${name} parameter names`);
  }
  // never vacuous: the two that carry same-typed neighbours must have been compared
  assert.deepEqual(wasm.get('fees_owed'), ['pool_key', 'pool', 'position', 'lower_array', 'upper_array']);
  assert.equal(wasm.get('quote_swap')?.length, 9);
});

test('the committed glue exports every required function and the web-target init', () => {
  const glue = join(here, '../wasm/floor_quote.js');
  assert.ok(existsSync(glue), 'wasm/floor_quote.js is committed');
  const src = readFileSync(glue, 'utf8');
  for (const name of REQUIRED_EXPORTS) assert.match(src, new RegExp(`export function ${name}\\(`), name);
  // The same two markers `sync-wasm.mjs` gates on — one list, so the gate and its test cannot drift.
  for (const [what, re] of WEB_TARGET_MARKERS) assert.match(src, re, what);
});
