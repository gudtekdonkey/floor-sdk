// Shared test helpers: repo paths, Rust-source parsers (the SDK's tables are held to the Rust
// files they transcribe), deterministic addresses, and one instance of every raw instruction.

import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import type { AccountMeta, Address } from '@solana/kit';

import {
  bytesToAddress,
  closePositionIx,
  closeTickArrayIx,
  initializePoolIx,
  initializeTickArrayIx,
  modifyLiquidityIx,
  swapIx,
  swapFromBalanceIx,
  transferPositionIx,
  DYNAMIC_FEE_FLAG,
  SPL_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  addressToBytes,
  compareBytes,
  findAta,
  findPool,
} from '@floorlaunchpad/carpenter-sdk';
import {
  buyIx,
  linkTraderIx,
  closeBuyerPositionIx,
  holdersTouchIx,
  prepareGraduationIx,
  sellBackIx,
  sellBackWsolIx,
  type FloorInstruction,
} from '../src/tx/instructions.ts';
// carpenter_amm's `["protocol_treasury"]` encoders.

/** This package's root (`fixtures/`, `idl/`, `src/`). */
export const SDK_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** `@floorlaunchpad/carpenter-sdk`'s package root (its `idl/`, `src/`), found through the dependency itself. */
export const CARPENTER_ROOT = dirname(createRequire(import.meta.url).resolve('@floorlaunchpad/carpenter-sdk/deployments.json'));
/**
 * The program source checkout the parity suites read the Rust programs and crates from, named by `$FLOOR_PROGRAM_SOURCE`.
 * Without it those cases are skipped (`SOURCE_ONLY`, and the whole of `rust-parity.test.ts`), never passed vacuously.
 */
const SOURCE_ENV = process.env['FLOOR_PROGRAM_SOURCE'];
export const REPO_ROOT = SOURCE_ENV ? resolve(SOURCE_ENV) : '';
export const HAS_SOURCE = REPO_ROOT !== '' && existsSync(join(REPO_ROOT, 'crates/floor-types/src'));
/** Test options for a case that reads the Rust source: skipped, by name, without a checkout. */
export const SOURCE_ONLY = { skip: HAS_SOURCE ? false : 'needs the program source (set FLOOR_PROGRAM_SOURCE)' } as const;

function localPath(rel: string): string | null {
  if (rel.startsWith('fixtures/')) return join(SDK_ROOT, rel);
  if (rel === 'crates/floor-math/fixtures/mr-s-pv1.json') return join(SDK_ROOT, 'fixtures/mr-s-pv1.json');
  return null;
}

export function repoFile(rel: string): string {
  const p = localPath(rel) ?? join(REPO_ROOT, rel);
  if (!existsSync(p)) throw new Error(`guard is vacuous without ${rel} (looked at ${p})`);
  return readFileSync(p, 'utf8');
}

/**
 * One `src/<file>` of the types crates, BOTH halves concatenated: `crates/carpenter-types` first, then
 * `crates/floor-types` (the mechanism's, which re-exports the AMM's). A parser over the result sees
 * every `pub const` / `layout!` / `pub mod` the SDK transcribes, whichever crate defines it; a
 * table that appears in both files (`disc.rs`'s `pub mod account`) is MERGED by its parser. Either
 * half may be absent (`keys.rs` and `hook.rs` are carpenter-only, `metadata.rs` and `register.rs`
 * floor-only); both absent is the vacuous guard `repoFile` refuses.
 */
export function typesSource(file: string): string {
  const parts = ['crates/carpenter-types/src/', 'crates/floor-types/src/']
    .map((dir) => join(REPO_ROOT, dir + file))
    .filter((p) => existsSync(p))
    .map((p) => readFileSync(p, 'utf8'));
  if (parts.length === 0) throw new Error(`guard is vacuous without crates/{carpenter,floor}-types/src/${file}`);
  return parts.join('\n');
}

/** A deterministic, distinct address per small integer. */
export function addr(n: number): Address {
  const b = new Uint8Array(32);
  b[0] = 0xa0;
  b[30] = (n >> 8) & 0xff;
  b[31] = n & 0xff;
  return bytesToAddress(b);
}

/** Strip `//` comments (doc comments included) so field lists parse cleanly. */
export function stripLineComments(src: string): string {
  return src
    .split('\n')
    .map((l) => l.replace(/\/\/.*$/, ''))
    .join('\n');
}

export interface RustLayoutField {
  name: string;
  type: string;
  offset: number;
  size: number;
}
export interface RustLayout {
  struct: string;
  name: string;
  size: number;
  disc: string;
  fields: RustLayoutField[];
}

/**
 * `pub const NAME: usize = N;` in `crates/{carpenter,floor}-types/src/constants.rs`, for the array lengths
 * `layout!` spells as a CONSTANT rather than a literal.
 *
 * `` wrote `metadata_base: [u8;
 * MAX_METADATA_BASE_LEN]` into `LaunchConfig`, and `rustTypeSize` threw on it — the correct
 * failure for a parser that must not skip, and the fix is to TEACH it, never to hard-code 64 here.
 * A second copy of the number is exactly the drift this whole suite exists to catch.
 */
function rustUsizeConst(name: string): number {
  const src = stripLineComments(typesSource('constants.rs'));
  const m = new RegExp(`pub const ${name}\\s*:\\s*usize\\s*=\\s*([0-9_]+)`).exec(src);
  if (!m) throw new Error(`no \`pub const ${name}: usize\` in crates/{carpenter,floor}-types/src/constants.rs`);
  return Number((m[1] ?? '').replace(/_/g, ''));
}

export function rustTypeSize(t: string): number {
  const s = t.replace(/\s+/g, '');
  const prim: Record<string, number> = { u8: 1, u16: 2, u32: 4, i32: 4, u64: 8, i64: 8, U128: 16, I128: 16 };
  if (prim[s] !== undefined) return prim[s];
  let m = /^\[\[u8;(\d+)\];(\d+)\]$/.exec(s);
  if (m) return Number(m[1]) * Number(m[2]);
  m = /^\[u8;(\d+)\]$/.exec(s);
  if (m) return Number(m[1]);
  m = /^\[u8;([A-Z_][A-Z0-9_]*)\]$/.exec(s);
  if (m) return rustUsizeConst(m[1] ?? '');
  m = /^\[Tick;(\d+)\]$/.exec(s);
  if (m) return 64 * Number(m[1]);
  // `StairState`'s per-step arrays.
  m = /^\[i32;(\d+)\]$/.exec(s);
  if (m) return 4 * Number(m[1]);
  m = /^\[U128;(\d+)\]$/.exec(s);
  if (m) return 16 * Number(m[1]);
  // `TraderBucket.owed`.
  m = /^\[u64;(\d+)\]$/.exec(s);
  if (m) return 8 * Number(m[1]);
  // `Launch.team_vest_days`.
  m = /^\[u16;(\d+)\]$/.exec(s);
  if (m) return 2 * Number(m[1]);
  // Trader referrals: `TraderBucket.owed`.
  m = /^\[u64;(\d+)\]$/.exec(s);
  if (m) return 8 * Number(m[1]);
  // `[u8; MAX_METADATA_BASE_LEN]` — a NAMED length. ⛔ Resolved out of `constants.rs`, never
  // transcribed: widening that constant must move this size by itself, or the layout twin would go
  // on agreeing with a stale number (`constants.rs`: widening it shrinks `LaunchConfig.reserved`).
  m = /^\[u8;([A-Z_][A-Z0-9_]*)\]$/.exec(s);
  if (m) {
    const n = parseRustIntConsts()[m[1] ?? ''];
    if (n === undefined) throw new Error(`unknown length constant in layout.rs: ${m[1]} (not an integer pub const in constants.rs)`);
    return Number(n);
  }
  throw new Error(`unknown Rust type in layout.rs: ${t}`);
}

/**
 * Every `layout!(…)` invocation in `crates/{carpenter,floor}-types/src/layout.rs`
 * AND `crates/floor-types/src/holders.rs` — the three
 * `floor_holders` accounts are declared THERE, beside their seeds and the pool offsets they read, and a
 * parser that read `layout.rs` alone would never see them.
 */
export function parseRustLayouts(): RustLayout[] {
  const src = stripLineComments(`${typesSource('layout.rs')}\n${typesSource('holders.rs')}`);
  const out: RustLayout[] = [];
  const re = /layout!\((\w+),\s*name\s*=\s*"(\w+)",\s*size\s*=\s*(\d+),\s*disc\s*=\s*disc::(\w+),\s*\{([\s\S]*?)\}\);/g;
  for (const m of src.matchAll(re)) {
    const fields: RustLayoutField[] = [];
    const body = m[5] ?? '';
    for (const f of body.matchAll(/(\w+)\s*:\s*((?:\[\[[^\]]*\];\s*\d+\])|(?:\[[^\]]*\])|\w+)\s*=\s*(\d+)/g)) {
      const raw = f[2] ?? '';
      // A NAMED array length is resolved to its number here, so the TS twin is compared against the
      // length `constants.rs` actually defines rather than against the constant's SPELLING — the TS
      // side has no constant to spell. `rustTypeSize` resolves it from the same place.
      const size = rustTypeSize(raw);
      const named = /^\[u8;[A-Z_][A-Z0-9_]*\]$/.test(raw.replace(/\s+/g, ''));
      fields.push({ name: f[1] ?? '', type: named ? `[u8; ${size}]` : raw, offset: Number(f[3]), size });
    }
    out.push({ struct: m[1] ?? '', name: m[2] ?? '', size: Number(m[3]), disc: m[4] ?? '', fields });
  }
  return out;
}

/**
 * `pub const NAME: [u8; 8] = [..];` per `pub mod <mod>` in `disc.rs` — both crates', MERGED per
 * module: `account` is defined in both (the AMM's accounts in carpenter-types, FLOOR's in
 * floor-types, which re-exports the first into the second), so the two tables are one table here.
 */
export function parseRustDiscriminators(): Record<string, Record<string, Uint8Array>> {
  const src = stripLineComments(typesSource('disc.rs'));
  const out: Record<string, Record<string, Uint8Array>> = {};
  for (const mod of src.matchAll(/pub mod (\w+)\s*\{([\s\S]*?)\n\}/g)) {
    const table: Record<string, Uint8Array> = out[mod[1] ?? ''] ?? {};
    for (const c of (mod[2] ?? '').matchAll(/pub const (\w+): \[u8; 8\] = \[([^\]]+)\];/g)) {
      table[c[1] ?? ''] = new Uint8Array((c[2] ?? '').split(',').map((x) => Number(x.trim())));
    }
    out[mod[1] ?? ''] = table;
  }
  return out;
}

/** `pub const NAME: [u8; 32] = [..];` (possibly multi-line) in a floor-types file. */
export function parseRustKeyConsts(rel: string): Record<string, Uint8Array> {
  const src = stripLineComments(repoFile(rel));
  const out: Record<string, Uint8Array> = {};
  for (const c of src.matchAll(/pub const (\w+): \[u8; 32\] = (\[[\s\S]*?\]|\[0; 32\]);/g)) {
    const body = c[2] ?? '';
    out[c[1] ?? ''] = body.replace(/\s/g, '') === '[0;32]' ? new Uint8Array(32) : new Uint8Array(body.slice(1, -1).split(',').filter((x) => x.trim() !== '').map((x) => Number(x.trim())));
  }
  return out;
}

/** `pub const NAME: &[u8] = b"…";` in `seeds.rs` — both crates'. */
export function parseRustSeeds(): Record<string, string> {
  const src = stripLineComments(typesSource('seeds.rs'));
  const out: Record<string, string> = {};
  for (const c of src.matchAll(/pub const (\w+): &\[u8\] = b"([^"]*)";/g)) out[c[1] ?? ''] = c[2] ?? '';
  return out;
}

/**
 * `floor_holders`' own table: its byte-string seeds,
 * its `[u8; 32]` ids, and its integer constants — the three kinds the SDK transcribes from that file.
 */
export function parseRustHolders(): { seeds: Record<string, string>; keys: Record<string, Uint8Array>; ints: Record<string, bigint> } {
  const src = stripLineComments(repoFile('crates/floor-types/src/holders.rs'));
  const seeds: Record<string, string> = {};
  for (const c of src.matchAll(/pub const (\w+): &\[u8\] = b"([^"]*)";/g)) seeds[c[1] ?? ''] = c[2] ?? '';
  const keys: Record<string, Uint8Array> = {};
  for (const c of src.matchAll(/pub const (\w+): \[u8; 32\] = (\[[\s\S]*?\]);/g)) {
    keys[c[1] ?? ''] = new Uint8Array((c[2] ?? '').slice(1, -1).split(',').filter((x) => x.trim() !== '').map((x) => Number(x.trim())));
  }
  const ints: Record<string, bigint> = {};
  for (const c of src.matchAll(/pub const (\w+): (?:u8|u16|u32|u64|usize) = ([0-9_]+);/g)) ints[c[1] ?? ''] = BigInt((c[2] ?? '').replace(/_/g, ''));
  return { seeds, keys, ints };
}

/** `pub const NAME: &[u8] = b"…";` in `programs/floor_vesting/src/constants.rs` — the STANDALONE program's own seeds. */
export function parseRustVestingSeeds(): Record<string, string> {
  const src = stripLineComments(repoFile('programs/floor_vesting/src/constants.rs'));
  const out: Record<string, string> = {};
  for (const c of src.matchAll(/pub const (\w+): &\[u8\] = b"([^"]*)";/g)) out[c[1] ?? ''] = c[2] ?? '';
  return out;
}

/** Integer `pub const NAME: T = <literal>;` in `constants.rs` — both crates' (hex, underscores; simple expressions skipped). */
export function parseRustIntConsts(): Record<string, bigint> {
  const src = stripLineComments(typesSource('constants.rs'));
  const out: Record<string, bigint> = {};
  for (const c of src.matchAll(/pub const (\w+): (?:u8|u16|u32|u64|u128|i32|i64|usize) = (-?(?:0x[0-9a-fA-F_]+|[0-9_]+)|1 << \d+);/g)) {
    const lit = (c[2] ?? '').replace(/_/g, '');
    const shift = /^1 << (\d+)$/.exec(lit);
    out[c[1] ?? ''] = shift ? 1n << BigInt(shift[1] ?? '0') : BigInt(lit);
  }
  return out;
}

export function parseRustExtensionTypes(rels: readonly string[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const rel of rels) {
    const src = stripLineComments(repoFile(rel));
    for (const c of src.matchAll(/const EXT_(\w+): u16 = (\d+);/g)) {
      const name = (c[1] ?? '').toLowerCase().replace(/(^|_)([a-z0-9])/g, (_, __, ch: string) => ch.toUpperCase());
      const value = Number(c[2]);
      const seen = out[name];
      if (seen !== undefined && seen !== value) throw new Error(`EXT_${c[1]} is ${seen} and ${value} in different suites`);
      out[name] = value;
    }
  }
  return out;
}

export interface RustGraduationList {
  /** `want[]`'s name literals, ordered by their `idx::*` constant. */
  names: string[];
  /** `GRADUATION_FIXED_ACCOUNTS` — i.e. `idx::TICK_ARRAYS`, where the tick arrays begin. */
  fixedCount: number;
  /** The names `WRITABLE_FIXED` marks, resolved through `idx::*`. */
  writable: Set<string>;
}

export function parseRustGraduationList(): RustGraduationList {
  const src = stripLineComments(repoFile('programs/floor_launch/src/graduate.rs'));
  const idxBody = /pub mod idx\s*\{([\s\S]*?)\n\}/.exec(src);
  if (!idxBody) throw new Error('graduate.rs: no `pub mod idx`');
  const idx = new Map<string, number>();
  for (const c of (idxBody[1] ?? '').matchAll(/pub const (\w+): usize = (\d+);/g)) idx.set(c[1] ?? '', Number(c[2]));

  const fixedM = /pub const GRADUATION_FIXED_ACCOUNTS: usize = idx::(\w+);/.exec(src);
  if (!fixedM) throw new Error('graduate.rs: no GRADUATION_FIXED_ACCOUNTS');
  const fixedCount = idx.get(fixedM[1] ?? '');
  if (fixedCount === undefined) throw new Error(`graduate.rs: GRADUATION_FIXED_ACCOUNTS = idx::${fixedM[1]}, which is not an idx constant`);

  // `want[]`: `(idx::NAME, <key expr>, "name"),`. The key expression is the program's own
  // derivation and is not read here; the ORDER and the names are.
  const wantM = /let want: \[\(usize, Pubkey, &'static str\); GRADUATION_FIXED_ACCOUNTS\] = \[([\s\S]*?)\n {4}\];/.exec(src);
  if (!wantM) throw new Error('graduate.rs: no want[] array');
  const names: string[] = [];
  for (const c of (wantM[1] ?? '').matchAll(/\(idx::(\w+),[^\n]*?"([a-z0-9_]+)"\)/g)) {
    const at = idx.get(c[1] ?? '');
    if (at === undefined) throw new Error(`graduate.rs: want[] names idx::${c[1]}, which is not an idx constant`);
    if (names[at] !== undefined) throw new Error(`graduate.rs: two want[] rows at index ${at}`);
    names[at] = c[2] ?? '';
  }
  if (names.length !== fixedCount || [...names.keys()].some((i) => names[i] === undefined)) {
    throw new Error(`graduate.rs: want[] fills ${names.filter((n) => n !== undefined).length} of ${fixedCount} indices`);
  }

  const writableM = /pub const WRITABLE_FIXED: \[usize; \d+\] = \[([\s\S]*?)\n\];/.exec(src);
  if (!writableM) throw new Error('graduate.rs: no WRITABLE_FIXED');
  const writable = new Set<string>();
  for (const c of (writableM[1] ?? '').matchAll(/idx::(\w+)/g)) {
    const at = idx.get(c[1] ?? '');
    if (at === undefined) throw new Error(`graduate.rs: WRITABLE_FIXED names idx::${c[1]}`);
    writable.add(names[at]!);
  }
  return { names, fixedCount, writable };
}

/**
 * The programs whose instructions and events the SDK encodes. FIVE: `floor_holders` (seven wallet instructions, six events)
 * and `floor_vesting` (three; a hooked mint makes them carry the hook's accounts, so the SDK builds them).
 */
export type ProgramName = 'carpenter_amm' | 'floor_hook' | 'floor_launch' | 'floor_holders' | 'floor_vesting';

/**
 * The instructions declared over ANOTHER instruction's `#[derive(Accounts)]` struct, keyed
 * `program/ix`. `carpenter_amm::swap_from_balance` takes
 * `Context<'info, Swap<'info>>` — `swap`'s struct, reused so the account list is `swap`'s by
 * construction — and there is no `SwapFromBalance` struct for the naming rule below to find.
 * ⛔ An entry is not taken on trust: `sharedAccountsStruct` requires `lib.rs`'s `pub fn <ix>` to
 * name that struct in its `Context`, so an entry that stops being true throws by name.
 */
const SHARED_ACCOUNTS: Readonly<Record<string, { struct: string; file: string }>> = {
  'carpenter_amm/swap_from_balance': { struct: 'Swap', file: 'swap' },

  // the instruction: `touch` is declared over `open_record`'s `OpenRecord` (the same nine accounts, open or
  // sync), `sync` over `SyncRecord` and `claim` over `ClaimRewards` (`Sync` / `Claim` would shadow std and
  // Anchor names). Each entry is re-checked against `programs/floor_holders/src/lib.rs`, as the AMM's is.
  'floor_holders/touch': { struct: 'OpenRecord', file: 'open_record' },
  'floor_holders/sync': { struct: 'SyncRecord', file: 'sync' },
  'floor_holders/claim': { struct: 'ClaimRewards', file: 'claim' },
  // the authority's two setters share ONE `SetGenesis` struct
  // (`instructions/genesis.rs`), re-checked against `lib.rs` like every entry here.
  'floor_launch/set_genesis_schedule': { struct: 'SetGenesis', file: 'genesis' },
  'floor_launch/set_genesis_floor_share': { struct: 'SetGenesis', file: 'genesis' },
  // `sell_back_wsol` is declared over `sell_back`'s `SellBack`; the wSOL pair rides in the remaining accounts.
  'floor_launch/sell_back_wsol': { struct: 'SellBack', file: 'sell_back' },
  'floor_launch/transfer_genesis_authority': { struct: 'SetGenesis', file: 'genesis' },
  'floor_launch/accept_genesis_authority': { struct: 'SetGenesis', file: 'genesis' },
  // the two pending-payee claims share `ClaimStairPayee`.
  'floor_launch/claim_team_payee': { struct: 'ClaimStairPayee', file: 'stair_payee' },
  'floor_launch/claim_genesis_payee': { struct: 'ClaimStairPayee', file: 'stair_payee' },
  'floor_launch/claim_team_vested': { struct: 'ClaimTeamVested', file: 'stair_payee' },
  'floor_hook/sweep_stair_payee': { struct: 'SweepStairPayee', file: 'stair_owner' },
  'floor_hook/withdraw_stair_tokens': { struct: 'WithdrawStairTokens', file: 'stair_owner' },
  // the top's setter shares it too.
  'floor_launch/set_genesis_stair_top': { struct: 'SetGenesis', file: 'genesis' },
  // the root's setter too.
  'floor_launch/set_genesis_whitelist': { struct: 'SetGenesis', file: 'genesis' },
  // the authority's three
  // setters share ONE `PresetAuthority` struct (`instructions/preset_registry.rs`); `init_preset_registry` /
  // `accept_preset_authority` are found by the naming rule in the same file.
  'floor_launch/add_preset': { struct: 'PresetAuthority', file: 'preset_registry' },
  'floor_launch/set_preset_enabled': { struct: 'PresetAuthority', file: 'preset_registry' },
  'floor_launch/set_preset_authority': { struct: 'PresetAuthority', file: 'preset_registry' },
  'floor_holders/authorize_claim': { struct: 'ClaimRewards', file: 'claim' },
  'floor_hook/pay_holder_authorized': { struct: 'PayHolder', file: 'pay_holder' },
};

function sharedAccountsStruct(program: ProgramName, ix: string): { struct: string; file: string } | undefined {
  const entry = SHARED_ACCOUNTS[`${program}/${ix}`];
  if (!entry) return undefined;
  const lib = stripLineComments(repoFile(`programs/${program}/src/lib.rs`));
  const sig = new RegExp(`pub fn ${ix}\\s*(?:<[^>]*>)?\\s*\\(\\s*ctx\\s*:\\s*Context<\\s*(?:'\\w+\\s*,\\s*)?${entry.struct}\\s*(?:<[^>]*>)?\\s*>`);
  if (!sig.test(lib)) throw new Error(`SHARED_ACCOUNTS says ${program}::${ix} takes Context<${entry.struct}>, and programs/${program}/src/lib.rs does not`);
  return entry;
}

/** `list_quote` → `ListQuote`: the `#[derive(Accounts)]` struct an instruction is declared over. */
const accountsStructName = (ix: string): string => ix.replace(/(?:^|_)([a-z0-9])/g, (_, c: string) => c.toUpperCase());

/**
 * The `#[derive(Accounts)]` struct of one instruction: its field body, and whether `#[event_cpi]`
 * sits on THAT struct.
 *
 * ⛔ Found BY NAME (`accountsStructName(ix)`), in `instructions/<ix>.rs` when that file exists and
 * otherwise in any `.rs` under the program's `instructions/`. Reading the FIRST struct of `<ix>.rs`
 * would be wrong: `floor_hook`'s quote roster declares THREE
 * (`ListQuote`, `SetQuoteEconomics`, `RetireQuote`) in one `quote_policy.rs`. A struct-by-name
 * lookup is what keeps "the accounts equal the struct" a claim about the right struct; the
 * attribute block is the text between the previous top-level item's closing `}` (or the file's
 * start) and the struct line, so an `#[event_cpi]` on a NEIGHBOUR cannot vouch for this one.
 * ⛔ Throws by name when no file declares the struct — never a silent empty account list.
 */
function accountsStructBody(program: ProgramName, ix: string): { body: string; eventCpi: boolean } {
  const shared = sharedAccountsStruct(program, ix);
  const name = shared?.struct ?? accountsStructName(ix);
  const own = `programs/${program}/src/instructions/${shared?.file ?? ix}.rs`;
  const dir = join(REPO_ROOT, 'programs', program, 'src', 'instructions');
  if (!existsSync(dir)) throw new Error(`guard is vacuous without programs/${program}/src/instructions`);
  const files = existsSync(join(REPO_ROOT, own))
    ? [own]
    : readdirSync(dir).filter((f) => f.endsWith('.rs')).map((f) => `programs/${program}/src/instructions/${f}`);
  for (const rel of files) {
    const src = stripLineComments(repoFile(rel));
    const m = new RegExp(`pub struct ${name}<'info>\\s*\\{([\\s\\S]*?)\\n\\}`).exec(src);
    if (!m) continue;
    // The attribute block: from the previous top-level `}` (a line that IS `}`) to the struct.
    const before = src.slice(0, m.index);
    const prevItem = before.lastIndexOf('\n}');
    const attrs = before.slice(prevItem < 0 ? 0 : prevItem + 2);
    if (!/#\[derive\(Accounts\)\]/.test(attrs)) throw new Error(`${rel}: pub struct ${name} is not #[derive(Accounts)]`);
    return { body: m[1] ?? '', eventCpi: /#\[event_cpi\]/.test(attrs) };
  }
  throw new Error(`no #[derive(Accounts)] pub struct ${name}<'info> for ${ix} under programs/${program}/src/instructions (looked in ${files.length} file(s))`);
}

/** The `#[derive(Accounts)]` field names of an instruction, `#[event_cpi]` accounts appended. */
export function parseRustAccounts(program: ProgramName, ix: string): string[] {
  const { body, eventCpi } = accountsStructBody(program, ix);
  const names = [...body.matchAll(/pub (\w+)\s*:/g)].map((x) => x[1] ?? '');
  if (eventCpi) names.push('event_authority', 'program');
  return names;
}

export interface RustAccountFlags {
  writable: boolean;
  signer: boolean;
  optional: boolean;
}

/**
 * Per field: writable (`mut` / `init` / `init_if_needed` / `close` / `realloc` in its
 * `#[account(…)]`, which may span lines), signer (`Signer<'info>`, possibly inside `Option<…>`),
 * optional (`Option<…>`).
 */
export function parseRustAccountFlags(program: ProgramName, ix: string): Map<string, RustAccountFlags> {
  const { body } = accountsStructBody(program, ix);
  const out = new Map<string, RustAccountFlags>();
  let attrs = '';
  for (const raw of body.split('\n')) {
    const l = raw.trim();
    if (!l) continue;
    const fm = /^pub (\w+)\s*:\s*(.+?),?$/.exec(l);
    if (fm) {
      const type = fm[2] ?? '';
      out.set(fm[1] ?? '', {
        writable: /\b(mut|init|init_if_needed|close|realloc)\b/.test(attrs),
        signer: /\bSigner<'info>/.test(type),
        optional: /^Option</.test(type),
      });
      attrs = '';
    } else {
      attrs += ` ${l}`;
    }
  }
  return out;
}

// ---- the wire: instruction ARGS and EVENT FIELDS -------------------------------------------------
//
// ⛔ Borsh carries no field names, so ORDER is the whole contract: a reordered pair of same-typed
//    fields is a silent wire break. These parsers produce ONE shape — `WireField[]`, name and type
//    in order — from three independent places, so `idl-parity.test.ts` can compare them:
//      · the Rust source        (`lib.rs` signatures, `events.rs` structs) — always available;
//      · the built IDL          (`target/idl/*.json`) — authoritative, needs `anchor build`;
//      · the SDK's own encoders (`src/tx/instructions.ts`'s `BorshWriter` call sequence) and
//        decoders (`src/events.ts`'s `EVENT_SCHEMAS` read sequence).

export type WireType =
  | 'u8' | 'u16' | 'u32' | 'u64' | 'u128'
  | 'i32' | 'i64' | 'i128'
  | 'bool' | 'pubkey' | 'string'
  // `fix-fee-split` put `fee_split0` / `fee_split1: [u64; 4]` on
  // `Swapped`. Every parser here THROWS on a type it does not know, so the vocabulary is widened
  // rather than the field skipped. FIVE slots (the team fee): `u64x5` replaces `u64x4`.
  | 'bytes32' | 'u64x5' | 'vec_u8' | 'option_u32' | 'vec_pubkey'
  // `VestingCreated.schedules: [Pubkey; 5]`.
  | 'pubkey5'
  // `CreateLaunchParams.team_vest_days` / `TeamBought.vest_days: [u16; 4]`.
  | 'u16x4'
  // `TraderReferralBooked.s: [u64; 3]`.
  | 'u64x3'
  // `contribute`'s `proof: Vec<[u8; 32]>`.
  | 'vec_bytes32'
  // `CreateLaunchParams.recipients` / `.team_payees: Vec<SlotArg>` (35 B a slot).
  | 'vec_slot_args';

export interface WireField {
  name: string;
  type: WireType;
}

/** `sqrt_price_x64` and `sqrtPriceX64` name the same field; compare on this. */
export const normName = (s: string): string => s.replace(/_/g, '').toLowerCase();

export const sameWire = (a: readonly WireField[], b: readonly WireField[]): boolean =>
  a.length === b.length && a.every((f, i) => normName(f.name) === normName(b[i]!.name) && f.type === b[i]!.type);

export const showWire = (f: readonly WireField[]): string => (f.length === 0 ? '(no args)' : f.map((x) => `${x.name}: ${x.type}`).join(', '));

/** Split `a, b<c, d>, [u8; 32]` on top-level commas only. */
function splitTopLevel(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = '';
  for (const ch of s) {
    if (ch === '<' || ch === '[' || ch === '(') depth++;
    else if (ch === '>' || ch === ']' || ch === ')') depth--;
    if (ch === ',' && depth === 0) {
      out.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  if (cur.trim() !== '') out.push(cur);
  return out.map((x) => x.trim()).filter((x) => x !== '');
}

const RUST_WIRE: Record<string, WireType> = {
  u8: 'u8', u16: 'u16', u32: 'u32', u64: 'u64', u128: 'u128',
  i32: 'i32', i64: 'i64', i128: 'i128',
  bool: 'bool', Pubkey: 'pubkey', String: 'string',
  '[u8;32]': 'bytes32', '[u64;5]': 'u64x5', 'Vec<u8>': 'vec_u8', 'Option<u32>': 'option_u32', 'Vec<Pubkey>': 'vec_pubkey',
  '[Pubkey;5]': 'pubkey5', '[u16;4]': 'u16x4', '[u64;3]': 'u64x3', 'Vec<[u8;32]>': 'vec_bytes32', 'Vec<SlotArg>': 'vec_slot_args',
  // `PayeeSlotsSet`'s fields name the program's own twin by path.
  'Vec<crate::state::SlotArg>': 'vec_slot_args',
};

/** `null` for a type this table does not know — the caller decides whether that is a struct to expand. */
function rustWire(t: string): WireType | null {
  return RUST_WIRE[t.replace(/\s+/g, '')] ?? null;
}

/**
 * Remove every `#[…]` attribute span, brackets counted so a nested one goes whole.
 * ⛔ An attribute is NOT part of the field, and a field wearing one is still on the wire: Anchor's
 * `#[event]` derives `AnchorSerialize` over EVERY field whatever its visibility or attributes.
 */
function stripAttributes(s: string): string {
  let out = '';
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '#' && s[i + 1] === '[') {
      let depth = 0;
      let j = i + 1;
      for (; j < s.length; j++) {
        if (s[j] === '[') depth++;
        else if (s[j] === ']' && --depth === 0) break;
      }
      i = j;
      continue;
    }
    out += s[i];
  }
  return out;
}

/**
 * The Borsh field list of a `{ … }` struct body: every field in order, attributes stripped and `pub`
 * OPTIONAL (Borsh does not read visibility).
 * ⛔ THROWS on a chunk it cannot read as a field — "teach the parser, never skip". Until
 * both callers did `if (!fm) continue`, so `#[allow(dead_code)] pub x: u64` and a bare `x: u64` were
 * DROPPED from the wire list with no error: two mutants, each a real Borsh field, passed 8/8.
 */
function parseFieldList(body: string, where: string, program?: ProgramName): WireField[] {
  const fields: WireField[] = [];
  for (const chunk of splitTopLevel(stripAttributes(body))) {
    const fm = /^(?:pub(?:\s*\([^)]*\))?\s+)?(\w+)\s*:\s*([\s\S]+)$/.exec(chunk);
    if (!fm) throw new Error(`${where}: cannot read \`${chunk}\` as a field — teach parseFieldList, never skip`);
    const t = rustWire(fm[2] ?? '');
    if (t !== null) {
      fields.push({ name: fm[1] ?? '', type: t });
      continue;
    }
    const struct = program ? rustStructName(fm[2] ?? '') : null;
    if (struct === null || program === undefined) {
      throw new Error(`${where}.${fm[1]}: unhandled Rust wire type \`${fm[2]}\` — teach RUST_WIRE, never skip`);
    }
    fields.push(...parseRustStructFields(program, struct));
  }
  return fields;
}

/** `crate::state::PresetRowArgs` / `state::PresetRowArgs` / `PresetRowArgs` → `PresetRowArgs`; `null` for anything else. */
function rustStructName(t: string): string | null {
  const m = /^(?:(?:crate|super|self|\w+)::)*([A-Z]\w*)$/.exec(t.replace(/\s+/g, ''));
  return m ? (m[1] ?? null) : null;
}

/** Every `.rs` under `programs/<program>/src`, comments stripped, concatenated. */
function programSource(program: ProgramName): string {
  const root = join(REPO_ROOT, 'programs', program, 'src');
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.rs')) out.push(readFileSync(p, 'utf8'));
    }
  };
  if (!existsSync(root)) throw new Error(`guard is vacuous without programs/${program}/src`);
  walk(root);
  return stripLineComments(out.join('\n'));
}

/** The `pub` field list of `pub struct <name> { … }`, wherever it lives in that program. */
export function parseRustStructFields(program: ProgramName, name: string): WireField[] {
  const src = programSource(program);
  const m = new RegExp(`pub struct ${name}\\s*\\{([\\s\\S]*?)\\n\\}`).exec(src);
  if (!m) throw new Error(`no pub struct ${name} under programs/${program}/src`);
  return parseFieldList(m[1] ?? '', `${program}::${name}`, program);
}

/**
 * The Borsh ARG list of `pub fn <ix>(…)` in `programs/<program>/src/lib.rs`: `ctx` dropped, and a
 * parameter whose type is a program struct (`CreateLaunchParams`) FLATTENED into its fields, which
 * is what Borsh puts on the wire.
 */
export function parseRustArgs(program: ProgramName, ix: string): WireField[] {
  const src = stripLineComments(repoFile(`programs/${program}/src/lib.rs`));
  const m = new RegExp(`pub fn ${ix}\\s*(?:<[^>]*>)?\\s*\\(([\\s\\S]*?)\\)\\s*->`).exec(src);
  if (!m) throw new Error(`no pub fn ${ix} in programs/${program}/src/lib.rs`);
  const out: WireField[] = [];
  for (const p of splitTopLevel(stripAttributes(m[1] ?? ''))) {
    // Same "teach, never skip" rule as the field lists: a parameter this cannot read is a hole in
    // the wire list, not a thing to drop. `mut name: T` is a legal binding pattern.
    const pm = /^(?:mut\s+)?(\w+)\s*:\s*([\s\S]+)$/.exec(p);
    if (!pm) throw new Error(`${program}::${ix}: cannot read \`${p}\` as a parameter — teach parseRustArgs, never skip`);
    const [, name = '', type = ''] = pm;
    if (/^Context\s*</.test(type.trim())) continue;
    const t = rustWire(type);
    if (t !== null) {
      out.push({ name, type: t });
      continue;
    }
    // A defined struct: Borsh writes its fields inline, so the wire is the flattened list. A PATH
    // names the struct by its last segment.
    const struct = rustStructName(type);
    if (struct === null) throw new Error(`${program}::${ix}.${name}: unhandled Rust wire type \`${type}\` — teach RUST_WIRE, never skip`);
    out.push(...parseRustStructFields(program, struct));
  }
  return out;
}

/** The field list of `#[event] pub struct <name>` in `programs/<program>/src/events.rs`. */
export function parseRustEventFields(program: ProgramName, name: string): WireField[] {
  const src = stripLineComments(repoFile(`programs/${program}/src/events.rs`));
  const m = new RegExp(`#\\[event\\][\\s\\S]*?pub struct ${name}\\s*\\{([\\s\\S]*?)\\n\\}`).exec(src);
  if (!m) throw new Error(`no #[event] pub struct ${name} in programs/${program}/src/events.rs`);
  return parseFieldList(m[1] ?? '', `${program}::${name}`, program);
}

/** Every `#[event] pub struct` name in a program, in source order. */
export function parseRustEventNames(program: ProgramName): string[] {
  const src = stripLineComments(repoFile(`programs/${program}/src/events.rs`));
  return [...src.matchAll(/#\[event\]\s*pub struct (\w+)/g)].map((m) => m[1] ?? '');
}

// ---- the built IDL ------------------------------------------------------------------------------

export interface IdlTypeNode {
  defined?: { name: string } | string;
  array?: [unknown, number];
  vec?: unknown;
  option?: unknown;
}
export type IdlType = string | IdlTypeNode;

export interface Idl {
  instructions: { name: string; discriminator: number[]; accounts: unknown[]; args: { name: string; type: IdlType }[] }[];
  events?: { name: string; discriminator: number[] }[];
  types?: { name: string; type: { kind: string; fields?: { name: string; type: IdlType }[] } }[];
}

export const idlPath = (program: ProgramName): string => join(program === 'carpenter_amm' ? CARPENTER_ROOT : SDK_ROOT, 'idl', `${program}.json`);
export const hasIdl = (program: ProgramName): boolean => existsSync(idlPath(program));
export function readIdl(program: ProgramName): Idl {
  return JSON.parse(readFileSync(idlPath(program), 'utf8')) as Idl;
}

const definedName = (t: IdlType): string | null => {
  if (typeof t === 'string') return null;
  const d = t.defined;
  if (d === undefined) return null;
  return typeof d === 'string' ? d : d.name;
};

/** One IDL type node → a `WireType`, or `null` when it is a defined struct to flatten. */
function idlWire(t: IdlType, where: string): WireType | null {
  if (typeof t === 'string') {
    const prim: Record<string, WireType> = {
      u8: 'u8', u16: 'u16', u32: 'u32', u64: 'u64', u128: 'u128',
      i32: 'i32', i64: 'i64', i128: 'i128',
      bool: 'bool', pubkey: 'pubkey', publicKey: 'pubkey', string: 'string',
      // Anchor 1.2 renders `Vec<u8>` as `bytes` (a u32 length prefix then the bytes — `vecU8`).
      bytes: 'vec_u8',
    };
    const p = prim[t];
    if (p === undefined) throw new Error(`${where}: unhandled IDL type \`${t}\` — teach idlWire, never skip`);
    return p;
  }
  if (definedName(t) !== null) return null;
  if (t.array) {
    const [inner, n] = t.array;
    if (inner === 'u8' && n === 32) return 'bytes32';
    if (inner === 'u64' && n === 5) return 'u64x5';
    if (inner === 'u64' && n === 3) return 'u64x3';
    if ((inner === 'pubkey' || inner === 'publicKey') && n === 5) return 'pubkey5';
    if (inner === 'u16' && n === 4) return 'u16x4';
    throw new Error(`${where}: unhandled IDL array [${String(inner)}; ${n}] — teach idlWire, never skip`);
  }
  if (t.vec !== undefined) {
    if (t.vec === 'u8') return 'vec_u8';
    if (t.vec === 'pubkey' || t.vec === 'publicKey') return 'vec_pubkey';
    const v = t.vec as unknown as { array?: [unknown, number] };
    if (typeof v === 'object' && v?.array && v.array[0] === 'u8' && v.array[1] === 32) return 'vec_bytes32';
    // `Vec<SlotArg>` — the struct is `{ bps u16, platform u8, key [u8; 32] }` (35 B).
    if (definedName(t.vec as IdlType) === 'SlotArg') return 'vec_slot_args';
    throw new Error(`${where}: unhandled IDL vec<${String(t.vec)}> — teach idlWire, never skip`);
  }
  if (t.option !== undefined) {
    if (t.option === 'u32') return 'option_u32';
    throw new Error(`${where}: unhandled IDL option<${String(t.option)}> — teach idlWire, never skip`);
  }
  throw new Error(`${where}: unhandled IDL type ${JSON.stringify(t)} — teach idlWire, never skip`);
}

/** A named IDL struct's fields, defined members flattened (Borsh writes them inline). */
export function idlStructFields(idl: Idl, name: string, where: string): WireField[] {
  const row = (idl.types ?? []).find((x) => x.name === name);
  if (!row) throw new Error(`${where}: no type \`${name}\` in the IDL`);
  if (row.type.kind !== 'struct' || !row.type.fields) throw new Error(`${where}: IDL type \`${name}\` is not a struct`);
  return row.type.fields.flatMap((f) => {
    const t = idlWire(f.type, `${where}.${f.name}`);
    return t === null ? idlStructFields(idl, definedName(f.type)!, `${where}.${f.name}`) : [{ name: f.name, type: t }];
  });
}

/** The IDL's Borsh ARG list for one instruction, defined structs flattened. */
export function idlArgs(idl: Idl, ix: string): WireField[] {
  const row = idl.instructions.find((x) => normName(x.name) === normName(ix));
  if (!row) throw new Error(`no instruction \`${ix}\` in the IDL`);
  return row.args.flatMap((a) => {
    const t = idlWire(a.type, `${ix}.${a.name}`);
    return t === null ? idlStructFields(idl, definedName(a.type)!, `${ix}.${a.name}`) : [{ name: a.name, type: t }];
  });
}

/** The IDL's field list for one event. */
export function idlEventFields(idl: Idl, name: string): WireField[] {
  const ev = (idl.events ?? []).find((x) => x.name === name);
  if (!ev) throw new Error(`no event \`${name}\` in the IDL`);
  return idlStructFields(idl, name, `event:${name}`);
}

// ---- the SDK's own encoders: the `BorshWriter` call sequence -------------------------------------

const WRITER_WIRE: Record<string, WireType> = {
  u8: 'u8', u16: 'u16', u32: 'u32', u64: 'u64', u128: 'u128',
  i32: 'i32', i64: 'i64', i128: 'i128',
  bool: 'bool', pubkey: 'pubkey', string: 'string',
  fixed: 'bytes32', vecU8: 'vec_u8', optionU32: 'option_u32', u64x5: 'u64x5', u64x3: 'u64x3', u16x4: 'u16x4', vecBytes32: 'vec_bytes32',
  vecSlotArgs: 'vec_slot_args',
};

/**
 * Strip every `//` and `/* … *\/` comment out of TypeScript source, string and template literals
 * left intact (a `//` inside a quoted string is not a comment). A block comment is replaced by ONE
 * space per line it spanned, so line structure — and `tsFunctionBody`'s `\nexport ` boundary — survive.
 *
 * `readWriterChain` reads a `BorshWriter` chain with
 * `/^\s*\.(\w+)\(/` from just past the previous `)`. A `// …` comment BETWEEN two calls of the chain
 * which is exactly how `encodeCreateLaunchParams` annotates its appended `.bool(p.fundReferrals)`
 * is not whitespace, so the regex failed to match and the chain reader stopped one call short:
 * the SDK-side write list ended at `pending_handle` while the Rust list carried `fund_referrals`,
 * and the encoder-parity case reported a drift that did not exist (the bytes were right; the
 * PARSER was blind). Stripping comments first is the only fix that does not forbid a comment where
 * a comment is most needed — beside the wire-format field it explains. Applied to the whole source
 * once, in `instructionsSource`, so `sdkEncoderNames` also stops counting a commented-out
 * `// export function …Ix(` as an encoder.
 */
export function stripTsComments(src: string): string {
  let out = '';
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i]!;
    const d = src[i + 1];
    // string / template literal: copy verbatim, honouring escapes
    if (c === '"' || c === "'" || c === '`') {
      let j = i + 1;
      for (; j < n; j++) {
        if (src[j] === '\\') {
          j++;
          continue;
        }
        if (src[j] === c) break;
        // a plain string cannot span a line; a template can
        if (c !== '`' && src[j] === '\n') break;
      }
      out += src.slice(i, j + 1);
      i = j + 1;
      continue;
    }
    if (c === '/' && d === '/') {
      let j = i;
      while (j < n && src[j] !== '\n') j++;
      i = j; // keep the newline itself
      continue;
    }
    if (c === '/' && d === '*') {
      const end = src.indexOf('*/', i + 2);
      const j = end < 0 ? n : end + 2;
      const lines = src.slice(i, j).split('\n').length - 1;
      out += ' ' + '\n'.repeat(lines);
      i = j;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

/** Both packages' encoders: `@floorlaunchpad/carpenter-sdk`'s `carpenter_amm` file, then this package's. */
const instructionsSource = (): string =>
  stripTsComments(readFileSync(join(CARPENTER_ROOT, 'src/tx/instructions.ts'), 'utf8') + '\n' + readFileSync(join(SDK_ROOT, 'src/tx/instructions.ts'), 'utf8'));

/** `create_launch` → `createLaunchIx`. */
export const ixFnName = (ix: string): string => `${ix.replace(/_(\w)/g, (_, c: string) => c.toUpperCase())}Ix`;

/**
 * ⛔ The encoders whose name is NOT `ixFnName(ix)`, keyed `program/ix`. Two reasons, both
 * collisions: `floor_holders` and `floor_vesting` BOTH declare `claim`, so their raw encoders carry the
 * program's prefix (`holdersClaimIx`, `vestingClaimIx`); and the SDK's PUBLIC, deriving `holders.ts`
 * exports `openRecordIx` / `touchIx` / `syncIx` / `claimReferralFeesIx` / … under their public names, so
 * the RAW encoders beside them cannot use those names too. Every other instruction keeps the rule.
 * ⛔ An entry here is not trusted: `everyInstruction` is closed against the SOURCE's encoder list both
 *    ways, so a mapped name that stops existing fails `idl-parity.test.ts` by name.
 */
export const ENCODER_FN: Readonly<Record<string, string>> = {
  'floor_hook/claim_referral_fees': 'hookClaimReferralFeesIx',
  'floor_holders/initialize_claims_registry': 'holdersInitializeClaimsRegistryIx',
  'floor_holders/rotate_claims_table': 'holdersRotateClaimsTableIx',
  'floor_holders/open_record': 'holdersOpenRecordIx',
  'floor_holders/touch': 'holdersTouchIx',
  'floor_holders/sync': 'holdersSyncIx',
  'floor_holders/claim': 'holdersClaimIx',
  'floor_holders/authorize_claim': 'holdersAuthorizeClaimIx',
  'floor_holders/close_record': 'holdersCloseRecordIx',
  'floor_vesting/create_schedule': 'vestingCreateScheduleIx',
  'floor_vesting/claim': 'vestingClaimIx',
  'floor_vesting/close_schedule': 'vestingCloseScheduleIx',
  // the vesting prefix again (floor_vesting's names collide).
  'floor_vesting/create_schedule_at': 'vestingCreateScheduleAtIx',
};

/** The raw encoder of `program/ix`: `ENCODER_FN`'s entry, else `ixFnName(ix)`. */
export const encoderFnName = (program: string | undefined, ix: string): string => (program ? ENCODER_FN[`${program}/${ix}`] : undefined) ?? ixFnName(ix);

/**
 * Every `export function <name>Ix(` in `src/tx/instructions.ts`, in source order.
 * `everyInstruction()` is hand-maintained, and every arg, byte and ACCOUNT case iterates it — so an
 * encoder missing from that map is invisible to all six. This is what closes the map against the
 * source: the guard must be able to see the NEXT encoder, not only today's sixteen.
 */
export function sdkEncoderNames(): string[] {
  return [...instructionsSource().matchAll(/^export function (\w+Ix)\s*\(/gm)].map((m) => m[1] ?? '');
}

/** The body of a top-level `export function <fn>(`, up to the next top-level `export`. */
function tsFunctionBody(fn: string): string {
  const src = instructionsSource();
  const at = src.indexOf(`export function ${fn}(`);
  if (at < 0) throw new Error(`no export function ${fn} in src/tx/instructions.ts`);
  const next = src.indexOf('\nexport ', at + 1);
  return src.slice(at, next < 0 ? src.length : next);
}

/** From `pos` (just past a `)`), read a `.m(…).m(…)` chain; returns the calls and the end offset. */
function readWriterChain(src: string, pos: number): { calls: { method: string; arg: string }[]; end: number } {
  const calls: { method: string; arg: string }[] = [];
  let i = pos;
  for (;;) {
    const m = /^\s*\.(\w+)\(/.exec(src.slice(i));
    if (!m) break;
    const open = i + m[0].length - 1;
    let depth = 0;
    let j = open;
    for (; j < src.length; j++) {
      if (src[j] === '(') depth++;
      else if (src[j] === ')' && --depth === 0) break;
    }
    calls.push({ method: m[1] ?? '', arg: src.slice(open + 1, j) });
    i = j + 1;
  }
  return { calls, end: i };
}

/**
 * The FIELD a `BorshWriter` call writes, named from its argument expression:
 * `args.hookData ?? new Uint8Array()` → `hookData`; `p.pendingRecipientId, 32` → `pendingRecipientId`;
 * `BigInt(args.amount0Max)` → `amount0Max`; `args.params.mode` → `mode`.
 *
 * ⛔ **The encoder must write a BARE FIELD REFERENCE** (optionally inside ONE call, or with a `??`
 * default). Anything else THROWS by name. The first cut took the SECOND dotted segment off the first
 * identifier it saw anywhere in the string, which named the CAST on `BigInt(args.amount0Max)`
 * (→ `BigInt`), the intermediate on `args.params.mode` (→ `params`) and the bigint SUFFIX on a
 * literal `0n` (→ `n`) — every one a red that reads as a guard bug on an ordinary refactor, which is
 * the pressure that gets a positional guard loosened. The comparison being positional is the whole
 * contract, so the parser fails LOUDLY and says what the encoder must look like instead.
 */
function argFieldName(arg: string): string {
  // `.fixed(x, 32)` passes the length as a second argument; the field is the first.
  let e = (splitTopLevel(arg)[0] ?? '').trim();
  e = e.replace(/\s*\?\?[\s\S]*$/, '').trim(); // `args.hookData ?? new Uint8Array()`
  const call = /^[A-Za-z_$][\w$]*\s*\(([\s\S]*)\)$/.exec(e); // ONE wrapper: `BigInt(args.x)`, `Number(a.x)`
  if (call) e = (splitTopLevel(call[1] ?? '')[0] ?? '').trim();
  if (!/^[A-Za-z_$][\w$]*(?:\s*\.\s*[A-Za-z_$][\w$]*)*$/.test(e)) {
    throw new Error(`cannot name the field written from \`${arg}\` — the encoder must write a bare field reference (\`args.<field>\`), optionally inside one call or with a \`??\` default`);
  }
  const path = e.split('.').map((s) => s.trim());
  return path[path.length - 1] ?? '';
}

function callsToWire(calls: { method: string; arg: string }[], where: string): WireField[] {
  return calls.map((c) => {
    const t = WRITER_WIRE[c.method];
    if (t === undefined) throw new Error(`${where}: BorshWriter.${c.method}() writes an unknown wire type — teach WRITER_WIRE, never skip`);
    if (c.method === 'fixed' && !/,\s*32\s*$/.test(c.arg)) throw new Error(`${where}: .fixed(${c.arg}) is not a [u8; 32]`);
    return { name: argFieldName(c.arg), type: t };
  });
}

/**
 * The SDK encoder's WRITE SEQUENCE for one instruction, read out of the source: every
 * `BorshWriter` call in order, with a `encodeXxx(new BorshWriter(), …)` wrapper expanded inline.
 * ⛔ An instruction whose encoder passes `null` for args writes nothing and returns `[]`.
 */
export function parseSdkWrites(ix: string, program?: string): WireField[] {
  const fn = encoderFnName(program, ix);
  const body = tsFunctionBody(fn);
  const at = body.indexOf('new BorshWriter()');
  if (at < 0) return [];
  if (body.indexOf('new BorshWriter()', at + 1) >= 0) throw new Error(`${ix}: more than one BorshWriter in ${fn} — the parser reads one chain`);

  const before = body.slice(0, at);
  const wrap = /(\w+)\(\s*$/.exec(before);
  if (!wrap) return callsToWire(readWriterChain(body, at + 'new BorshWriter()'.length).calls, ix);

  // `encodeCreateLaunchParams(new BorshWriter(), args.params).u64(…)…`
  const open = before.length - wrap[0].length + (wrap[1] ?? '').length;
  let depth = 0;
  let j = open;
  for (; j < body.length; j++) {
    if (body[j] === '(') depth++;
    else if (body[j] === ')' && --depth === 0) break;
  }
  const inner = tsFunctionBody(wrap[1] ?? '');
  const ret = inner.indexOf('return w');
  if (ret < 0) throw new Error(`${ix}: ${wrap[1]} does not \`return w\` — the parser reads its chain from there`);
  const prefix = callsToWire(readWriterChain(inner, ret + 'return w'.length).calls, `${ix}/${wrap[1]}`);
  return [...prefix, ...callsToWire(readWriterChain(body, j + 1).calls, ix)];
}

const A = addr;
const none: readonly AccountMeta[] = [];

/** One instance of every raw instruction the SDK encodes, keyed `program/instruction`. */
export function everyInstruction(): Record<string, [program: ProgramName, ix: FloorInstruction]> {
  const sale = {
    program: A(1), eventAuthority: A(2), buyer: A(10), launch: A(11), saleVault: A(12), tokenVault: A(13), buyerPosition: A(14),
    buyerAta: A(15), mint: A(16), lockAuthority: A(17), token2022: A(18),
  };
  return {
    // (`close_tick_array`): a NAMED rent owner, so the golden shows the 44-byte wire with a non-zero tail.
    'carpenter_amm/initialize_tick_array': ['carpenter_amm', initializeTickArrayIx({ program: A(1), payer: A(2), pool: A(3), tickArray: A(4), startIndex: -3600, rentOwner: A(5) })],
    // no args; rent_owner s,w · pool · bitmap · tick_array w · event_cpi.
    'carpenter_amm/close_tick_array': ['carpenter_amm', closeTickArrayIx({ program: A(1), eventAuthority: A(2), rentOwner: A(3), pool: A(4), bitmap: A(5), tickArray: A(6) })],
    'carpenter_amm/initialize_pool': [
      'carpenter_amm',
      initializePoolIx(
        {
          program: A(1), eventAuthority: A(2), payer: A(3), initializer: A(4), config: A(5), pool: A(6), bitmap: A(7), vault0: A(8), vault1: A(9),
          mint0: A(10), mint1: A(11), tokenProgram0: A(12), tokenProgram1: A(13), mintExclusivity0: A(14), mintExclusivity1: A(15),
          mintPools0: A(16), mintPools1: A(17), mintPools0Writable: false, mintPools1Writable: true, poolIndexEntry: A(18),
          approvedHook: A(19), hookProgram: A(20), hookProgramdata: A(21), quoteMint0: A(22), quoteMint1: A(23), hookSlice: none,
        },
        { fee: 0x800000, tickSpacing: 60, hook: A(20), sqrtPriceX64: 1n << 64n },
      ),
    ],
    'carpenter_amm/modify_liquidity': [
      'carpenter_amm',
      modifyLiquidityIx(
        {
          program: A(1), eventAuthority: A(2), owner: A(3), payer: A(4), pool: A(5), bitmap: A(6), position: A(7), tickArrayLower: A(8),
          tickArrayUpper: A(9), ownerToken0: A(10), ownerToken1: A(11), vault0: A(12), vault1: A(13), mint0: A(14), mint1: A(15),
          tokenProgram0: A(16), tokenProgram1: A(17), hookProgram: null, positionAuthority: null, hookSlice: none,
        },
        { positionId: new Uint8Array(32).fill(3), salt: 1n, tickLower: -60, tickUpper: 60, liquidityDelta: 1000n, amount0Max: 1n, amount1Max: 1n, rentOwner: A(18) },
      ),
    ],
    'carpenter_amm/close_position': ['carpenter_amm', closePositionIx({ program: A(1), eventAuthority: A(2), owner: A(3), position: A(4) })],
    'carpenter_amm/transfer_position': ['carpenter_amm', transferPositionIx({ program: A(1), eventAuthority: A(2), owner: A(3), position: A(4), pool: A(5) }, A(6))],
    'carpenter_amm/swap': [
      'carpenter_amm',
      swapIx(
        {
          program: A(1), eventAuthority: A(2), trader: A(3), pool: A(4), bitmap: A(5), traderTokenIn: A(6), traderTokenOut: A(7), vault0: A(8),
          vault1: A(9), mint0: A(10), mint1: A(11), tokenProgram0: A(12), tokenProgram1: A(13), hookProgram: A(14), hookSlice: none,
          tickArrays: [], actionPositions: [],
        },
        { zeroForOne: true, amountSpecified: -5n, sqrtPriceLimitX64: 4295048017n, amountLimit: 0n },
      ),
    ],
    // `swap`'s accounts (the SAME fixture addresses as the row above, so
    // the golden shows the two account lists equal), its own discriminator and args.
    'carpenter_amm/swap_from_balance': [
      'carpenter_amm',
      swapFromBalanceIx(
        {
          program: A(1), eventAuthority: A(2), trader: A(3), pool: A(4), bitmap: A(5), traderTokenIn: A(6), traderTokenOut: A(7), vault0: A(8),
          vault1: A(9), mint0: A(10), mint1: A(11), tokenProgram0: A(12), tokenProgram1: A(13), hookProgram: A(14), hookSlice: none,
          tickArrays: [], actionPositions: [],
        },
        { zeroForOne: true, keep: 7n, sqrtPriceLimitX64: 4295048017n, amountLimit: 3n, limitInBasis: 11n },
      ),
    ],
    'floor_launch/buy': ['floor_launch', buyIx({ ...sale, ataProgram: A(19), graduationAccounts: none }, { lamportsIn: 1n, minTokens: 1n, maxGraduationLamports: 1n })],
    // a HOOKED launch's sell-back carries the 4-account holder tail.
    'floor_launch/sell_back': ['floor_launch', sellBackIx({ ...sale, holderTail: { floorHolders: A(60), holderBook: A(61), floorPool: A(62), holderRecord: A(63) } }, 1n)],
    'floor_launch/sell_back_wsol': ['floor_launch', sellBackWsolIx({ ...sale, sellerWsol: A(64), splToken: A(65), holderTail: { floorHolders: A(60), holderBook: A(61), floorPool: A(62), holderRecord: A(63) } }, 1n)],
    'floor_launch/close_buyer_position': ['floor_launch', closeBuyerPositionIx({ program: A(1), buyer: A(2), launch: A(3), buyerPosition: A(4) })],
    'floor_launch/prepare_graduation': [
      'floor_launch',
      prepareGraduationIx({
        program: A(1), payer: A(2), launch: A(3), mint: A(4), token2022: A(5), eventAuthority: A(6),
        graduationAccounts: none,
      }),
    ],
    // the attestor-signed link and the sale account's sweep.
    'floor_launch/link_trader': [
      'floor_launch',
      linkTraderIx(
        { program: A(1), eventAuthority: A(2), trader: A(3), launchConfig: A(4), traderLink: A(5), instructionsSysvar: A(6), wsolMint: A(7), wsolBucket: A(8), floorHook: A(9) },
        { tier1: A(10), tier2: A(11), tier3: A(12), deadline: 1_900_000_000n },
      ),
    ],
    'floor_holders/touch': [
      'floor_holders',
      holdersTouchIx({ program: A(1), eventAuthority: A(2), payer: A(3), owner: A(4), mint: A(5), holderBook: A(6), floorPool: A(7), tokenAccount: A(8), record: A(9) }),
    ],
  };
}

// ---- a fixture launch (Meme at 150 USD/SOL) whose derived keys are consistent -------------------

import type { Launch } from '../src/layouts.ts';
import { FLOOR_TICK_SPACING } from '../src/constants.ts';
import { DEFAULT_PROGRAM_IDS, findFloorPool, findLaunch, findLaunchMint, type ProgramIds } from '../src/pda.ts';
import { openTick, tranchePriceX64 } from '../src/tickmath.ts';

export const FIXTURE_CREATOR = addr(1);
export const FIXTURE_BUYER = addr(2);

export interface FixtureLaunch {
  nonce: bigint;
  mint: Address;
  launch: Address;
  state: Pick<Launch, 'mint' | 'tokenVault' | 'wsolVault' | 'priceX64' | 'openTick' | 'ammPool' | 'floorPool' | 'creator' | 'saleTokens' | 'sold' | 'raised' | 'mode' | 'nonce'>;
}

/** The lowest nonce from 0 whose mint sorts above wSOL, with every derived key filled in. */
export async function fixtureLaunch(ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<FixtureLaunch> {
  for (let nonce = 0n; ; nonce++) {
    const m = await findLaunchMint(FIXTURE_CREATOR, nonce, ids);
    if (compareBytes(addressToBytes(m.address), addressToBytes(WSOL_MINT)) <= 0) continue;
    const mint = m.address;
    const launch = (await findLaunch(mint, ids)).address;
    const ammPool = (await findPool({ mint0: WSOL_MINT, mint1: mint, fee: DYNAMIC_FEE_FLAG, tickSpacing: FLOOR_TICK_SPACING, hook: ids.floorHook }, ids)).address;
    const floorPool = (await findFloorPool(ammPool, ids)).address;
    const priceX64 = tranchePriceX64(20_000n, 150n * 100n, 9);
    return {
      nonce,
      mint,
      launch,
      state: {
        mint,
        tokenVault: (await findAta(launch, mint, TOKEN_2022_PROGRAM_ID)).address,
        wsolVault: (await findAta(launch, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)).address,
        priceX64,
        openTick: openTick(priceX64),
        ammPool,
        floorPool,
        creator: FIXTURE_CREATOR,
        saleTokens: 550_000_000_000_000n,
        sold: 0n,
        raised: 0n,
        mode: 3,
        nonce,
      },
    };
  }
}

export const FIXTURE_STAIR_STEPS = 12;

/** Rent as prints it; tests only — the SDK reads rent at runtime. */
export const specRent = (len: number): bigint => BigInt(128 + len) * 5_080n;

/**
 * The account LENGTHS `graduation_rent.rs` pins in its own test
 * (`the_byte_table_is_the_accounts_graduation_creates`: `assert_eq!(POOL_LEN, 640);` …), keyed by the
 * Rust constant name. The program derives each from `8 + <layout>::SIZE` and asserts the number; this
 * reads the asserted numbers, so the SDK's `GRADUATION_ACCOUNT_LENS` is pinned to what the program
 * ITSELF says it creates — never to a figure typed into a test beside it.
 */
export function parseRustGraduationLens(): ReadonlyMap<string, number> {
  const src = stripLineComments(repoFile('programs/floor_launch/src/graduation_rent.rs'));
  const out = new Map<string, number>();
  for (const c of src.matchAll(/assert_eq!\((\w+_LEN), (\d+)(?:,|\))/g)) out.set(c[1] ?? '', Number(c[2]));
  if (out.size === 0) throw new Error('graduation_rent.rs: no `assert_eq!(<NAME>_LEN, <n>)` lines');
  return out;
}
