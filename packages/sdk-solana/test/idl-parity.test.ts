
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test, type TestContext } from './node-test.ts';

import { AccountRole } from '@solana/kit';

import { EVENT_SCHEMAS, type F, type ProgramKey } from '../src/events.ts';
import {
  encoderFnName,
  everyInstruction,
  hasIdl,
  idlArgs,
  idlEventFields,
  parseRustAccounts,
  parseRustAccountFlags,
  parseRustArgs,
  parseRustEventFields,
  parseRustEventNames,
  parseSdkWrites,
  readIdl,
  idlPath as idlPathOf,
  sameWire,
  sdkEncoderNames,
  showWire,
  stripTsComments,
  type WireField,
  type WireType, SOURCE_ONLY } from './helpers.ts';

const PROGRAMS: ProgramKey[] = ['carpenter_amm', 'floor_hook', 'floor_launch', 'floor_holders', 'floor_vesting'];

/**
 * The programs the IDL half runs over: all three, or none with a diagnostic.
 * ⛔ Never a subset — silence must not be a possible outcome of a partial `anchor idl build`.
 */
function idlPrograms(t: TestContext): ProgramKey[] {
  const present = PROGRAMS.filter(hasIdl);
  if (present.length === 0) {
    t.diagnostic('no target/idl — `anchor idl build -p <program> -o target/idl/<program>.json` for the IDL half');
    return [];
  }
  assert.deepEqual(
    present,
    PROGRAMS,
    `PARTIAL target/idl: missing ${PROGRAMS.filter((p) => !hasIdl(p)).join(', ')} — build all five or none, or the authority skips a program in silence`,
  );
  return PROGRAMS;
}

const SDK_DECIDES_WRITABLE = new Set(['carpenter_amm/initialize_pool.mint_pools0', 'carpenter_amm/initialize_pool.mint_pools1']);

const isWritable = (r: AccountRole) => r === AccountRole.WRITABLE || r === AccountRole.WRITABLE_SIGNER;
const isSigner = (r: AccountRole) => r === AccountRole.READONLY_SIGNER || r === AccountRole.WRITABLE_SIGNER;

test('account names and order equal the #[derive(Accounts)] struct, event_cpi appended', SOURCE_ONLY, () => {
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const rust = parseRustAccounts(program, ix.name);
    assert.deepEqual([...ix.accountNames], rust, key);
    assert.equal(ix.accounts.length >= ix.accountNames.length, true, `${key}: remaining accounts follow the named ones`);
  }
});

test('writable and signer flags equal the struct attributes (mut / init / close; Signer<>)', SOURCE_ONLY, () => {
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const flags = parseRustAccountFlags(program, ix.name);
    ix.accountNames.forEach((name, i) => {
      const meta = ix.accounts[i]!;
      const f = flags.get(name);
      if (name === 'event_authority' || name === 'program') {
        assert.equal(meta.role, AccountRole.READONLY, `${key}.${name} read-only`);
        return;
      }
      assert.ok(f, `${key}: struct field ${name}`);
      // An Option<…> given `null` encodes as the program id, read-only; the helper instances pass
      // a value for every optional slot except where noted, so compare on the given value.
      if (f.optional && meta.address === ix.programAddress) {
        assert.equal(meta.role, AccountRole.READONLY, `${key}.${name} None → program id, read-only`);
        return;
      }
      if (!SDK_DECIDES_WRITABLE.has(`${key}.${name}`)) assert.equal(isWritable(meta.role), f.writable, `${key}.${name} writable`);
      assert.equal(isSigner(meta.role), f.signer, `${key}.${name} signer`);
    });
  }
});

test('every instruction begins with its discriminator and the IDL (when built) agrees on accounts', (t) => {
  const ready = idlPrograms(t);
  if (ready.length === 0) return;
  let checked = 0;
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const idlPath = idlPathOf(program);
    assert.ok(existsSync(idlPath), `${key}: ${idlPath}`);
    const idl = JSON.parse(readFileSync(idlPath, 'utf8')) as {
      instructions: { name: string; discriminator: number[]; accounts: { name: string; writable?: boolean; signer?: boolean; optional?: boolean; accounts?: unknown[] }[] }[];
    };
    const row = idl.instructions.find((x) => x.name === ix.name);
    assert.ok(row, `${key} in the IDL`);
    assert.deepEqual([...ix.data.subarray(0, 8)], row.discriminator, `${key} discriminator`);
    const flat = (accs: typeof row.accounts): typeof row.accounts => accs.flatMap((a) => (a.accounts ? flat(a.accounts as typeof row.accounts) : [a]));
    const idlAccounts = flat(row.accounts);
    assert.deepEqual(
      [...ix.accountNames],
      idlAccounts.map((a) => a.name),
      `${key} IDL account names`,
    );
    idlAccounts.forEach((a, i) => {
      const meta = ix.accounts[i]!;
      if (a.optional && meta.address === ix.programAddress) return;
      // The same deliberate exemption the Rust half carries: the IDL mirrors `#[derive(Accounts)]`,
      // so a slot the SDK decides is writable is not writable there either. ⚠ This branch had never
      // run — no checkout carried a `target/idl` — and it was red the first time it did.
      if (!SDK_DECIDES_WRITABLE.has(`${key}.${a.name}`)) assert.equal(isWritable(meta.role), !!a.writable, `${key}.${a.name} IDL writable`);
      assert.equal(isSigner(meta.role), !!a.signer, `${key}.${a.name} IDL signer`);
    });
    checked++;
  }
  assert.equal(checked, Object.keys(everyInstruction()).length, 'every instruction was checked against the IDL');
});

// =================================================================================================
// ARGS — the encoder's `BorshWriter` call sequence is the instruction's Borsh arg list
// =================================================================================================

/**
 * ⛔ `everyInstruction()` is hand-maintained and EVERY case above and below iterates it, so an
 * encoder missing from it is invisible to all of them. This closes the map against the source: a
 * seventeenth `…Ix` fails here rather than shipping unguarded.
 */
test('everyInstruction() carries one instance of every encoder instructions.ts exports', () => {
  const encoders = sdkEncoderNames();
  // ⚠ `encoderFnName`, not `ixFnName`: floor_holders and floor_vesting both declare
  //   `claim`, and `holders.ts` owns the unprefixed public names (`helpers.ts` → `ENCODER_FN`).
  const covered = Object.values(everyInstruction()).map(([program, ix]) => encoderFnName(program, ix.name));
  assert.deepEqual(
    encoders.filter((n) => !covered.includes(n)),
    [],
    'encoder(s) with no instance in everyInstruction() — every arg, byte and account case is blind to them',
  );
  assert.deepEqual(
    covered.filter((n) => !encoders.includes(n)),
    [],
    'everyInstruction() names an encoder src/tx/instructions.ts does not export',
  );
  assert.equal(new Set(covered).size, covered.length, 'everyInstruction() names an encoder twice');
  assert.ok(encoders.length >= 14, `${encoders.length} encoders parsed out of instructions.ts — the parse must not go vacuous`);
});

test('the encoder parser sees a BorshWriter call behind a // or /* */ comment', () => {
  // the stripper's contract, on a synthetic chain: comments gone, strings and line structure kept
  const src = [
    "const w = new BorshWriter()",
    "  .u8(p.mode) // trailing",
    "  // ⚠ APPENDED LAST — a whole line between two calls",
    "  /* and a block",
    "     spanning lines */",
    "  .bool(p.fundReferrals ?? false)",
    "  .string('not // a comment')",
    "  .string(\"nor /* this */\");",
  ].join('\n');
  const out = stripTsComments(src);
  assert.equal(out.split('\n').length, src.split('\n').length, 'line count preserved');
  assert.ok(!/APPENDED LAST|trailing|a block|spanning/.test(out), 'every comment is gone');
  assert.ok(out.includes("'not // a comment'") && out.includes('"nor /* this */"'), 'string literals are untouched');
  // what readWriterChain sees: whitespace, then `.method(` — for EVERY call, the commented one included
  const calls = [...out.matchAll(/\.(\w+)\(/g)].map((m) => m[1]);
  assert.deepEqual(calls, ['u8', 'bool', 'string', 'string']);
  assert.match(out, /\)\s*\.bool\(/, 'nothing but whitespace stands between `)` and `.bool(`');
});

test('every encoder writes the program`s arg list, in order (Rust source)', SOURCE_ONLY, () => {
  let checked = 0;
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const rust = parseRustArgs(program, ix.name);
    const sdk = parseSdkWrites(ix.name, program);
    assert.ok(sameWire(sdk, rust), `${key} args\n  SDK  : ${showWire(sdk)}\n  Rust : ${showWire(rust)}`);
    checked++;
  }
  assert.equal(checked, Object.keys(everyInstruction()).length, 'every instruction was compared');
  assert.ok(checked >= 14, `${checked} instructions compared — the guard must not go vacuous`);
});

test('every encoder writes the program`s arg list, in order (built IDL)', (t) => {
  const ready = idlPrograms(t);
  if (ready.length === 0) return;
  let checked = 0;
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const idl = idlArgs(readIdl(program), ix.name);
    const sdk = parseSdkWrites(ix.name, program);
    assert.ok(sameWire(sdk, idl), `${key} args\n  SDK : ${showWire(sdk)}\n  IDL : ${showWire(idl)}`);
    checked++;
  }
  assert.equal(checked, Object.keys(everyInstruction()).length, 'every instruction was compared against the IDL');
});

const NOT_ENCODED: Record<ProgramKey, Record<string, string>> = {
  carpenter_amm: {
    initialize_config: 'deployment — once, an authority signs',
    set_config_authority: 'governance — `config.authority` signs',
    accept_config_authority: 'governance — the pending authority signs',
    approve_hook: 'governance — `config.authority` signs',
    set_hook_paused: 'governance — `config.authority` signs',
    revoke_mint_exclusivity: 'governance — config authority only',
    register_mint_exclusivity: 'the HOOK AUTHORITY signs, not a wallet',
    hook_withdraw: 'the HOOK AUTHORITY signs, not a wallet',
    update_dynamic_lp_fee: 'the HOOK AUTHORITY signs — "FLOOR never calls it"',
    hook_deposit: 'no SDK builder: any signer',
    donate: 'no SDK builder: a donor instruction',
    reapprove_hook: 'governance — `config.authority` re-pins an upgraded hook',
    set_pool_paused: 'governance — `config.authority` signs, per pool',
    set_pause_guardian: 'governance — `config.authority` names the pause guardian',
    guardian_pause_pool: 'governance — the pause guardian signs a 72-hour pool pause',
    guardian_pause_hook: 'governance — the pause guardian signs a 72-hour hook pause',
    initialize_position: 'CPI from floor_hook inside prepare_positions — `buildPrepareGraduation` is the wallet half',
  },
  floor_hook: {
    initialize_hook_config: "deployment — floor_hook's ProgramData upgrade authority signs, once",
    before_initialize: 'AMM→hook CALLBACK, never sent by a wallet',
    before_swap: 'AMM→hook CALLBACK, never sent by a wallet',
    after_swap: 'AMM→hook CALLBACK, never sent by a wallet',
    after_actions: 'AMM→hook CALLBACK, never sent by a wallet',
    register_pool: 'CPI from floor_launch inside create_launch',
    seed_liquidity: 'CPI from floor_launch inside graduate',
    seed_inventory: 'CPI from floor_launch inside graduate',
    resolve_recipient: 'CPI from floor_launch inside claim_recipient — `claimRecipientIx` is the wallet half',
    prepare_positions: 'CPI from floor_launch inside prepare_graduation — only this program can sign for `hook_authority`',

    pay_holder: 'CPI from floor_holders::claim, signed by its ["holders_authority"] PDA — `claimHolderIx` is the wallet half',
    // The payee claim.
    resolve_stair_payee: 'CPI from floor_launch inside claim_team_payee / claim_genesis_payee, signed by its ["launch_authority"] PDA — `claimTeamPayeeIx` / `claimGenesisPayeeIx` are the wallet half',
    clear_team_buy_books: 'CPI from floor_launch inside team_buy_sell_back, signed by its ["launch_authority"] PDA — `teamBuySellBackIx` is the wallet half',
    // Trader referrals, the sale side.
    credit_trader_bucket: 'CPI from floor_launch inside buy / sell_back, signed by its ["launch_authority"] PDA (trader referrals, D6)',
  },
  floor_launch: {
    initialize_launch_config: "deployment — floor_launch's ProgramData upgrade authority signs, once",
    set_governance: 'governance — the two-step handover, `launch_config.governance` signs',
    accept_governance: 'governance — the pending governance signs',
    set_attestor: 'governance — `launch_config.governance` rotates the attestor key',
    set_launch_economics: 'governance — the platform treasury\'s governance sets min_bid / the bounty terms',
    set_metadata_base: 'governance — the late-bound metadata host',
    fill_metadata_uri: 'no SDK builder: the one-shot uri fill, permissionless',
  },
  // ⛔. Seven instructions are encoded; these two
  //    are not a wallet's to send.
  floor_holders: {
    init_launch: "CPI from floor_launch::create_launch, signed by floor_launch's [\"lock_authority\"] PDA (NotLaunchAuthority otherwise)",
    execute: "the Token-2022 transfer-hook INTERFACE's Execute — Token-2022 alone invokes it, on every transfer of a hooked mint",
  },
  // floor_vesting: all three instructions are encoded.
  floor_vesting: {},
};
const OUT_OF_SCOPE: Record<ProgramKey, readonly string[]> = {
  carpenter_amm: ["accept_config_authority", "accept_protocol_treasury_governance", "approve_hook", "delist_quote_mint", "donate", "guardian_pause_hook", "guardian_pause_pool", "hook_deposit", "hook_list_quote_mint", "hook_withdraw", "initialize_config", "initialize_position", "initialize_protocol_treasury", "list_quote_mint", "reapprove_hook", "register_mint_exclusivity", "relist_quote_mint", "retire_quote_mint", "revoke_mint_exclusivity", "set_base_transfer_hook", "set_config_authority", "set_hook_paused", "set_hook_quotes", "set_pause_guardian", "set_pool_paused", "set_protocol_fee", "set_protocol_treasury_governance", "set_quote_caps_admitted", "sweep_protocol_fees", "update_dynamic_lp_fee", "withdraw_protocol_fees"],
  floor_hook: ["accept_hook_governance", "accept_team_treasury_governance", "accept_treasury_governance", "after_actions", "after_swap", "before_initialize", "before_swap", "claim_creator_fees", "claim_referral_fees", "claim_trader_referral", "clear_team_buy_books", "credit_floor_pool", "credit_trader_bucket", "designate_floor_pool", "donate_inventory", "initialize_hook_config", "initialize_team_treasury", "list_quote", "observe_quote", "open_trader_bucket", "open_trader_ref_vault", "pay_holder", "pay_holder_authorized", "prepare_positions", "ratchet", "reclaim_stair_array", "register_pool", "resolve_recipient", "resolve_stair_payee", "retire_quote", "seed_floor", "seed_inventory", "seed_liquidity", "set_allow_all_quotes", "set_genesis_stair", "set_hook_governance", "set_quote_economics", "set_team_treasury_governance", "set_treasury_governance", "sweep_platform_cut", "sweep_stair_fees", "sweep_stair_payee", "sweep_trader_referral", "twap_tick", "withdraw_platform", "withdraw_stair_tokens", "withdraw_team"],
  floor_launch: ["accept_genesis_authority", "accept_governance", "accept_preset_authority", "add_preset", "claim_genesis", "claim_genesis_payee", "claim_recipient", "claim_team_payee", "claim_team_vested", "close_graduation_rent", "close_prepared_tick_array", "close_quote_walk", "contribute", "create_launch", "end_genesis_sale", "fill_metadata_uri", "flush_allocation_dust", "flush_floor_fees", "fund_stair_rent", "graduate", "graduate_genesis", "init_genesis", "init_preset_registry", "initialize_launch_config", "lock_genesis", "open_sale_trader_referral", "refund_genesis", "set_attestor", "set_genesis_floor_share", "set_genesis_schedule", "set_genesis_stair_top", "set_genesis_whitelist", "set_governance", "set_launch_economics", "set_metadata_base", "set_preset_authority", "set_preset_enabled", "set_team_fees", "sweep_sale_trader_referral", "team_buy_sell_back", "transfer_genesis_authority", "walk_quote_chain"],
  floor_holders: ["authorize_claim", "claim", "close_record", "execute", "init_launch", "initialize_claims_registry", "open_record", "rotate_claims_table", "sync"],
  floor_vesting: ["claim", "close_schedule", "create_schedule", "create_schedule_at"],
};
for (const [program, names] of Object.entries(OUT_OF_SCOPE) as [ProgramKey, readonly string[]][]) {
  for (const n of names) NOT_ENCODED[program][n] ??= 'outside the public SDK: read / quote / trade only';
}

const MAY_BE_ABSENT_FROM_IDL = new Set(['floor_holders/execute']);

test('the IDL declares no instruction the SDK neither encodes nor names as deliberately unencoded', (t) => {
  for (const program of idlPrograms(t)) {
    const declared = readIdl(program)
      .instructions.map((i) => i.name)
      .slice()
      .sort();
    const encoded = Object.entries(everyInstruction())
      .filter(([key]) => key.startsWith(`${program}/`))
      .map(([, [, ix]]) => ix.name);
    const overlap = encoded.filter((n) => n in NOT_ENCODED[program]);
    assert.deepEqual(overlap, [], `${program}: named in NOT_ENCODED and encoded anyway`);
    const named = Object.keys(NOT_ENCODED[program]).filter((n) => declared.includes(n) || !MAY_BE_ABSENT_FROM_IDL.has(`${program}/${n}`));
    assert.deepEqual(
      [...encoded, ...named].sort(),
      declared,
      `${program}: the IDL's instruction SET differs from (what the SDK encodes + what NOT_ENCODED names) — encode it or name it with the reason`,
    );
  }
});

// The parse above reads SOURCE; this proves the source it read is the source that RAN — the bytes
// one real instance encodes are exactly the parsed sequence, with nothing left over.
const SIZE: Record<Exclude<WireType, 'string' | 'vec_u8' | 'option_u32' | 'vec_pubkey' | 'vec_bytes32' | 'vec_slot_args'>, number> = {
  u8: 1, u16: 2, u32: 4, u64: 8, u128: 16, i32: 4, i64: 8, i128: 16, bool: 1, pubkey: 32, bytes32: 32,
  // Swapped's two fee splits; five slots (the team fee).
  u64x5: 40,
  u64x3: 24,
  // `CreateLaunchParams.team_vest_days`.
  u16x4: 8,
  // ($FLOOR's SOLANA GENESIS): `VestingCreated.schedules: [Pubkey; 5]` (an event field; no instruction arg).
  pubkey5: 160,
};

function consume(data: Uint8Array, at: number, f: WireField): number {
  if (f.type === 'string' || f.type === 'vec_u8') {
    const n = new DataView(data.buffer, data.byteOffset).getUint32(at, true);
    return at + 4 + n;
  }
  if (f.type === 'option_u32') return at + (data[at] === 0 ? 1 : 5);
  if (f.type === 'vec_pubkey' || f.type === 'vec_bytes32') {
    const n = new DataView(data.buffer, data.byteOffset).getUint32(at, true);
    return at + 4 + 32 * n;
  }
  if (f.type === 'vec_slot_args') {
    const n = new DataView(data.buffer, data.byteOffset).getUint32(at, true);
    return at + 4 + 35 * n;
  }
  return at + SIZE[f.type];
}

test('the encoded bytes are exactly that sequence — no trailing byte, no short read', () => {
  let checked = 0;
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const sdk = parseSdkWrites(ix.name, program);
    let at = 8; // the discriminator
    for (const f of sdk) {
      at = consume(ix.data, at, f);
      assert.ok(at <= ix.data.length, `${key}: ran past the end at ${f.name}`);
    }
    // The named accounts end the data; remaining accounts carry no bytes.
    assert.equal(at, ix.data.length, `${key}: ${ix.data.length - at} byte(s) the parsed write sequence does not account for`);
    checked++;
  }
  // The case whose whole job is to prove the parse is not fiction is the one that most needs a floor.
  assert.ok(checked >= 14, `${checked} instructions walked — the guard must not go vacuous`);
});

// =================================================================================================
// the decoder's read sequence is the event's Borsh field list
// =================================================================================================

const EVENT_WIRE: Record<F, WireType> = {
  pubkey: 'pubkey', bool: 'bool', u8: 'u8', u16: 'u16', u32: 'u32', i32: 'i32', u64: 'u64', i64: 'i64', u128: 'u128',
  i128: 'i128', string: 'string', bytes32: 'bytes32', option_u32: 'option_u32', vec_pubkey: 'vec_pubkey', u16x4: 'u16x4',
  u64x5: 'u64x5',
  u64x3: 'u64x3',
  pubkey5: 'pubkey5',
  vec_slot_args: 'vec_slot_args',
};

const schemaWire = (program: ProgramKey, name: string): WireField[] =>
  (EVENT_SCHEMAS[program][name] ?? []).map(([n, f]) => ({ name: n, type: EVENT_WIRE[f] }));

test('every event decoder reads the program`s field list, in order (Rust source)', SOURCE_ONLY, () => {
  let checked = 0;
  for (const program of PROGRAMS) {
    const declared = parseRustEventNames(program);
    assert.deepEqual(
      Object.keys(EVENT_SCHEMAS[program]).slice().sort(),
      declared.slice().sort(),
      `${program}: the SDK decodes a different event SET than events.rs declares`,
    );
    for (const name of declared) {
      const rust = parseRustEventFields(program, name);
      const sdk = schemaWire(program, name);
      assert.ok(sameWire(sdk, rust), `${program}/${name}\n  SDK  : ${showWire(sdk)}\n  Rust : ${showWire(rust)}`);
      checked++;
    }
  }
  assert.ok(checked >= 30, `${checked} events compared — the guard must not go vacuous`);
});

test('every event decoder reads the program`s field list, in order (built IDL)', (t) => {
  const ready = idlPrograms(t);
  if (ready.length === 0) return;
  let checked = 0;
  for (const program of ready) {
    const idl = readIdl(program);
    assert.deepEqual(
      Object.keys(EVENT_SCHEMAS[program]).slice().sort(),
      (idl.events ?? []).map((e) => e.name).slice().sort(),
      `${program}: the SDK decodes a different event SET than the IDL declares`,
    );
    for (const name of Object.keys(EVENT_SCHEMAS[program])) {
      const fields = idlEventFields(idl, name);
      const sdk = schemaWire(program, name);
      assert.ok(sameWire(sdk, fields), `${program}/${name}\n  SDK : ${showWire(sdk)}\n  IDL : ${showWire(fields)}`);
      checked++;
    }
  }
  const expected = PROGRAMS.reduce((n, p) => n + Object.keys(EVENT_SCHEMAS[p]).length, 0);
  assert.equal(checked, expected, 'every event was compared against the IDL');
});

// ---- the ARGS half ----------------------------------------------------------------------------

type IdlType = string | { array: [IdlType, number] } | { vec: IdlType } | { option: IdlType } | { defined: { name: string } };
interface IdlField {
  name: string;
  type: IdlType;
}
interface IdlTypeDef {
  name: string;
  type: { kind: string; fields?: IdlField[] };
}

const FIXED: Record<string, number> = { u8: 1, i8: 1, u16: 2, i16: 2, u32: 4, i32: 4, f32: 4, u64: 8, i64: 8, f64: 8, u128: 16, i128: 16, bool: 1, pubkey: 32 };

function skipValue(data: Uint8Array, at: number, type: IdlType, types: Map<string, IdlTypeDef>, where: string): number {
  const need = (n: number): number => {
    assert.ok(at + n <= data.length, `${where}: reads past the end of data (${at}+${n} > ${data.length})`);
    return at + n;
  };
  if (typeof type === 'string') {
    const n = FIXED[type];
    if (n !== undefined) return need(n);
    if (type === 'string' || type === 'bytes') {
      const end = need(4);
      const len = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(at, true);
      at = end;
      return need(len);
    }
    throw new Error(`${where}: unhandled IDL type ${type}`);
  }
  if ('array' in type) {
    const [inner, n] = type.array;
    for (let i = 0; i < n; i++) at = skipValue(data, at, inner, types, `${where}[${i}]`);
    return at;
  }
  if ('vec' in type) {
    const end = need(4);
    const n = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(at, true);
    at = end;
    for (let i = 0; i < n; i++) at = skipValue(data, at, type.vec, types, `${where}[${i}]`);
    return at;
  }
  if ('option' in type) {
    const end = need(1);
    const tag = data[at]!;
    at = end;
    return tag === 0 ? at : skipValue(data, at, type.option, types, `${where}?`);
  }
  const def = types.get(type.defined.name);
  assert.ok(def, `${where}: the IDL has no type ${type.defined.name}`);
  assert.equal(def.type.kind, 'struct', `${where}: ${type.defined.name} is a ${def.type.kind}, which this walker does not read`);
  for (const f of def.type.fields ?? []) at = skipValue(data, at, f.type, types, `${where}.${f.name}`);
  return at;
}

test('every instruction ARG list and its encoded bytes equal the built IDL', (t) => {
  let checked = 0;
  for (const [key, [program, ix]] of Object.entries(everyInstruction())) {
    const idlPath = idlPathOf(program);
    if (!existsSync(idlPath)) continue;
    const idl = JSON.parse(readFileSync(idlPath, 'utf8')) as {
      instructions: { name: string; args?: IdlField[] }[];
      types?: IdlTypeDef[];
    };
    const row = idl.instructions.find((x) => x.name === ix.name);
    assert.ok(row, `${key} in the IDL`);
    const args = row.args ?? [];
    assert.deepEqual([...ix.argNames], args.map((a) => a.name), `${key} IDL arg names`);

    const types = new Map((idl.types ?? []).map((d) => [d.name, d]));
    const data = ix.data.subarray(8);
    let at = 0;
    for (const a of args) at = skipValue(data, at, a.type, types, `${key}.${a.name}`);
    assert.equal(at, data.length, `${key}: the IDL's args consume ${at} of ${data.length} encoded bytes`);
    checked++;
  }
  if (checked === 0) t.diagnostic('no target/idl — run `anchor build` for the args half of this check');
});
