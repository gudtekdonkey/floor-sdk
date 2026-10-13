
import assert from 'node:assert/strict';
import { test as nodeTest, type TestContext } from './node-test.ts';

import {
  addressToBytes,
  bytesToAddress,
  anchorDiscriminator,
  kindSize,
  TICK_ARRAY_LEN,
  TICK_ARRAY_LEN_V1,
  TICK_ARRAY_LEN_V2,
  TICK_ARRAY_RENT_OWNER_OFFSET,
  type FieldKind,
  EXTENSION_TYPES,
  ADDRESS_LOOKUP_TABLE_PROGRAM_ID,
} from '@carpenter/sdk';
import * as CARPENTER from '@carpenter/sdk';
import * as FLOOR_CONSTANTS from '../src/constants.ts';

const C = { ...CARPENTER, ...FLOOR_CONSTANTS };
import { LAYOUTS } from '../src/layouts.ts';
import { DISC } from '../src/tx/instructions.ts';
import {
  parseRustDiscriminators,
  parseRustExtensionTypes,
  parseRustHolders,
  parseRustIntConsts,
  parseRustKeyConsts,
  parseRustLayouts,
  parseRustSeeds,
  parseRustVestingSeeds,
  repoFile,
  stripLineComments,
  SOURCE_ONLY,
} from './helpers.ts';

/** Every case in this file reads the Rust source. */
function test(name: string, body: (t: TestContext) => unknown): void {
  nodeTest(name, SOURCE_ONLY, body);
}

const tsKind = (k: FieldKind): string => {
  switch (k.t) {
    case 'u8':
    case 'u16':
    case 'u32':
    case 'i32':
    case 'u64':
    case 'i64':
      return k.t;
    case 'u128':
      return 'U128';
    case 'i128':
      return 'I128';
    case 'pubkey':
      return '[u8;32]';
    case 'pubkeys':
      return `[[u8;32];${k.n}]`;
    case 'bytes':
      return `[u8;${k.n}]`;
    case 'ticks':
      return `[Tick;${k.n}]`;
    case 'i32s':
      return `[i32;${k.n}]`;
    case 'u128s':
      return `[U128;${k.n}]`;
    case 'u16s':
      return `[u16;${k.n}]`;
    case 'u64s':
      return `[u64;${k.n}]`;
  }
};

test('every layout!() in layout.rs has a TS twin with the same fields, offsets, sizes and order', () => {
  const rust = parseRustLayouts();
  assert.ok(rust.length >= 14, `parsed ${rust.length} layouts`);
  const byName = new Map(LAYOUTS.map((l) => [l.name, l]));
  // Accounts of flows this package does not build or read: the genesis raise, the create-time quote walk, the shared
  // claims table's registry.
  const outOfScope = new Set(['GenesisSale', 'GenesisContribution', 'QuoteWalk', 'ClaimsRegistry']);
  for (const name of outOfScope) assert.ok(rust.some((r) => r.name === name), `layout.rs ${name}`);
  for (const r of rust) {
    if (outOfScope.has(r.name)) continue;
    const ts = byName.get(r.name);
    assert.ok(ts, `TS layout for ${r.name}`);
    assert.equal(ts.size, r.size, `${r.name} size`);
    assert.equal(ts.fields.length, r.fields.length, `${r.name} field count`);
    r.fields.forEach((f, i) => {
      const t = ts.fields[i]!;
      assert.equal(t.name, f.name, `${r.name}[${i}] name`);
      assert.equal(t.offset, f.offset, `${r.name}.${f.name} offset`);
      assert.equal(kindSize(t.kind), f.size, `${r.name}.${f.name} size`);
      // `layout!` may spell an array length as a CONSTANT
      // `rustTypeSize` resolves it out of
      // `constants.rs`, so compare on the RESOLVED spelling, never on the token — the size
      // assertion one line above is what actually pins the width.
      const rustType = f.type.replace(/\s+/g, '').replace(/^\[u8;[A-Z_][A-Z0-9_]*\]$/, `[u8;${f.size}]`);
      assert.equal(tsKind(t.kind), rustType, `${r.name}.${f.name} type`);
    });
    // contiguous, ending at SIZE (the layout! macro's own rule)
    let end = 0;
    for (const f of ts.fields) {
      assert.equal(f.offset, end, `${r.name}.${f.name} contiguous`);
      end += kindSize(f.kind);
    }
    assert.equal(end, ts.size, `${r.name} ends at SIZE`);
  }
  assert.equal(byName.size, rust.length - outOfScope.size, 'no TS layout without a Rust twin');
});

test('account discriminators equal disc.rs and sha256("account:<Name>")[..8]', () => {
  const rust = parseRustDiscriminators();
  const acct = rust['account'];
  assert.ok(acct);
  const constName = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toUpperCase();
  for (const l of LAYOUTS) {
    const expected: Uint8Array | undefined = acct[constName(l.name)];
    assert.ok(expected, `disc.rs account::${constName(l.name)}`);
    assert.deepEqual([...l.discriminator], [...expected], `${l.name} discriminator`);
    assert.deepEqual([...l.discriminator], [...anchorDiscriminator(`account:${l.name}`)], `${l.name} is sha256("account:…")`);
  }
});

test('instruction discriminators equal disc.rs and sha256("global:<name>")[..8]', () => {
  const rust = parseRustDiscriminators();
  const groups: [keyof typeof DISC, string][] = [
    ['amm', 'amm'],
    ['hook', 'hook'],
    ['launch', 'launch'],
    // floor_holders' instructions (`disc.rs` `pub mod holders`).
    ['holders', 'holders'],
  ];
  for (const [ts, mod] of groups) {
    const table = rust[mod];
    assert.ok(table, `disc.rs mod ${mod}`);
    for (const [ix, bytes] of Object.entries(DISC[ts])) {
      const expected: Uint8Array | undefined = table[ix.toUpperCase()];
      assert.ok(expected, `disc.rs ${mod}::${ix.toUpperCase()}`);
      assert.deepEqual([...bytes], [...expected], `${mod}/${ix}`);
      assert.deepEqual([...bytes], [...anchorDiscriminator(`global:${ix}`)], `${mod}/${ix} is sha256("global:…")`);
    }
  }
});

/**
 * ⛔ floor_vesting's instruction discriminators have NO `disc.rs` table (the program is STANDALONE and
 * `floor-types` names none of its instructions), so they are held to the hash alone here — and to the
 * built IDL by `idl-parity.test.ts`, which is the authority.
 */
test('floor_vesting discriminators are sha256("global:<name>")[..8]', () => {
  for (const [ix, bytes] of Object.entries(DISC.vesting)) {
    assert.deepEqual([...bytes], [...anchorDiscriminator(`global:${ix}`)], `vesting/${ix}`);
  }
  // `create_schedule_at`, the EVM's `lockAt` — the ONE
  // floor_vesting instruction `floor-types` names (`disc.rs` `pub mod vesting`, for `lock_genesis`' CPI), so it is
  // held to that file as well as the hash.
  const rust = parseRustDiscriminators()['vesting'];
  assert.ok(rust?.['CREATE_SCHEDULE_AT'], 'disc.rs vesting::CREATE_SCHEDULE_AT');
  assert.deepEqual([...DISC.vesting.create_schedule_at], [...rust['CREATE_SCHEDULE_AT']], 'vesting/create_schedule_at equals disc.rs');
  assert.deepEqual(Object.keys(rust), ['CREATE_SCHEDULE_AT'], 'disc.rs names no other floor_vesting instruction');
  assert.equal(Object.keys(DISC.vesting).length, 4, 'create_schedule, claim, close_schedule, create_schedule_at');
});

/**
 * ⛔ its placeholder id, the ALT program id and its
 * integer constants, held to `crates/floor-types/src/holders.rs` (not `seeds.rs` / `ids.rs`, which
 * re-export it or carry none of it): every seed the Rust spells has a TS twin and none is extra.
 */
test('HOLDERS_SEEDS, FLOOR_HOLDERS_PROGRAM_ID and the holder constants equal crates/floor-types/src/holders.rs', () => {
  const h = parseRustHolders();
  const map: Record<keyof typeof C.HOLDERS_SEEDS, string> = {
    holdersAuthority: 'HOLDERS_AUTHORITY',
    holderBook: 'HOLDER_BOOK',
    extraAccountMetas: 'EXTRA_ACCOUNT_METAS',
    holderRecord: 'HOLDER_RECORD',
  };
  for (const [ts, rs] of Object.entries(map)) {
    assert.ok(h.seeds[rs] !== undefined, `holders.rs ${rs}`);
    assert.equal(new TextDecoder().decode(C.HOLDERS_SEEDS[ts as keyof typeof C.HOLDERS_SEEDS]), h.seeds[rs], rs);
  }
  // The claims lookup-table seeds are out of this package's scope (claims are not built here).
  const outOfScope = ['CLAIMS_ALT_AUTHORITY', 'CLAIMS_REGISTRY'];
  for (const rs of outOfScope) assert.ok(h.seeds[rs] !== undefined, `holders.rs ${rs}`);
  assert.equal(Object.keys(h.seeds).length, Object.keys(map).length + outOfScope.length, 'every other holders.rs seed has a TS twin');
  // The id is spelled in `ids.rs` and holders.rs re-exports it
  //   (`pub use crate::ids::FLOOR_HOLDERS_ID`) — read it from whichever file spells the bytes.
  const holdersId = h.keys['FLOOR_HOLDERS_ID'] ?? parseRustKeyConsts('crates/floor-types/src/ids.rs')['FLOOR_HOLDERS_ID'];
  assert.ok(holdersId, 'FLOOR_HOLDERS_ID spelled in holders.rs or ids.rs');
  assert.equal(bytesToAddress(holdersId), C.FLOOR_HOLDERS_PROGRAM_ID, 'the program id');
  assert.equal(bytesToAddress(h.keys['ADDRESS_LOOKUP_TABLE_PROGRAM_ID']!), ADDRESS_LOOKUP_TABLE_PROGRAM_ID);
  const ints: [string, number][] = [
  ];
  for (const [name, value] of ints) {
    assert.ok(h.ints[name] !== undefined, `holders.rs ${name}`);
    assert.equal(h.ints[name], BigInt(value), name);
  }
});

test('seeds equal seeds.rs, byte for byte', () => {
  const rust = parseRustSeeds();
  const map: Record<keyof typeof C.SEEDS, string> = {
    ammConfig: 'AMM_CONFIG',
    approvedHook: 'APPROVED_HOOK',
    mintExclusivity: 'MINT_EXCLUSIVITY',
    mintPools: 'MINT_POOLS',
    pool: 'POOL',
    bitmap: 'BITMAP',
    tickArray: 'TICK_ARRAY',
    position: 'POSITION',
    poolIndex: 'POOL_INDEX',
    vault: 'VAULT',
    eventAuthority: 'EVENT_AUTHORITY',
    hookConfig: 'HOOK_CONFIG',
    floorPool: 'FLOOR_POOL',
    hookAuthority: 'HOOK_AUTHORITY',
    // carpenter_amm's `["hook_settle", pool]`, the signer a settling hook is called under.
    hookSettle: 'HOOK_SETTLE',
    launchConfig: 'LAUNCH_CONFIG',
    registryEntry: 'REGISTRY_ENTRY',
    launch: 'LAUNCH',
    saleVault: 'SALE_VAULT',
    buyer: 'BUYER',
    mint: 'MINT',
    mintAuthority: 'MINT_AUTHORITY',
    lockAuthority: 'LOCK_AUTHORITY',
    launchAuthority: 'LAUNCH_AUTHORITY',
    gradRent: 'GRAD_RENT',
    // `["stair_rent", amm_pool]` under floor_hook.
    stairRent: 'STAIR_RENT',
    // `["quote", quote_mint]`, the FLOOR quote roster entry — seeds.rs went
    // to 25 and the table stayed at 24 for the first pass of that day.
    quotePolicy: 'QUOTE_POLICY',
    quoteMint: 'QUOTE_MINT',
    // ?"): `["platform_treasury"]`, the
    // program-owned platform treasury — seeds.rs went to 31 and this row is what makes it loud.
    platformTreasury: 'PLATFORM_TREASURY',
    // `["team_treasury"]`, the program-owned TEAM treasury.
    teamTreasury: 'TEAM_TREASURY',
    // carpenter_amm's `["protocol_treasury"]`
    // (carpenter-types seeds.rs), the AMM's own treasury.
    protocolTreasury: 'PROTOCOL_TREASURY',
    // TRADER REFERRALS.
    traderLink: 'TRADER_LINK',
    traderRef: 'TRADER_REF',
    traderRefVault: 'TRADER_REF_VAULT',
    traderRefSale: 'TRADER_REF_SALE',
    //  $FLOOR's pool record.
    floorPlatformPool: 'FLOOR_PLATFORM_POOL',
    // a staircase pool's StairState.
    stair: 'STAIR',
  };
  for (const [ts, rs] of Object.entries(map)) {
    assert.ok(rust[rs] !== undefined, `seeds.rs ${rs}`);
    assert.equal(new TextDecoder().decode(C.SEEDS[ts as keyof typeof C.SEEDS]), rust[rs], rs);
  }
  // Seeds of accounts this package does not build or read (the genesis raise, the create-time quote walk, vesting).
  const outOfScope = ['GENESIS_SALE', 'GENESIS_CONTRIBUTION', 'QUOTE_WALK', 'VESTING_SCHEDULE', 'VESTING_ESCROW', 'VESTING_AUTHORITY'];
  for (const rs of outOfScope) assert.ok(rust[rs] !== undefined, `seeds.rs ${rs}`);
  assert.equal(Object.keys(rust).length, Object.keys(map).length + outOfScope.length, 'every other Rust seed has a TS twin');
});

test('program ids and well-known keys equal ids.rs / keys.rs', () => {
  // ids.rs is split (the AMM's and the mock hook's in carpenter-types, FLOOR's FOUR in floor-types —
  // hook, launch, rewards and vesting); keys.rs is carpenter-only and
  // floor-types re-exports it. Only three ids are compared because `src/constants.ts` carries a
  // program-id constant for those three alone: the SDK encodes nothing for `floor_rewards` or
  // `floor_vesting` (idl-parity.test.ts's PROGRAMS stays three for the same reason).
  const ids = { ...parseRustKeyConsts('crates/carpenter-types/src/ids.rs'), ...parseRustKeyConsts('crates/floor-types/src/ids.rs') };
  const keys = parseRustKeyConsts('crates/carpenter-types/src/keys.rs');
  assert.equal(bytesToAddress(ids['CARPENTER_AMM_ID']!), C.CARPENTER_AMM_PROGRAM_ID);
  assert.equal(bytesToAddress(ids['FLOOR_HOOK_ID']!), C.FLOOR_HOOK_PROGRAM_ID);
  assert.equal(bytesToAddress(ids['FLOOR_LAUNCH_ID']!), C.FLOOR_LAUNCH_PROGRAM_ID);
  // floor_vesting is encoded by the SDK now (its constant is the frozen id, for tests and scripts).
  assert.equal(bytesToAddress(ids['FLOOR_VESTING_ID']!), C.FLOOR_VESTING_PROGRAM_ID);
  const expect = (name: string, a: string) => assert.deepEqual([...addressToBytes(a as never)], [...keys[name]!], name);
  expect('WSOL_MINT', C.WSOL_MINT);
  expect('TOKEN_2022_NATIVE_MINT', C.TOKEN_2022_NATIVE_MINT);
  expect('SPL_TOKEN_PROGRAM_ID', C.SPL_TOKEN_PROGRAM_ID);
  expect('TOKEN_2022_PROGRAM_ID', C.TOKEN_2022_PROGRAM_ID);
  expect('ASSOCIATED_TOKEN_PROGRAM_ID', C.ASSOCIATED_TOKEN_PROGRAM_ID);
  expect('BPF_LOADER_UPGRADEABLE_ID', C.BPF_LOADER_UPGRADEABLE_ID);
  expect('SYSTEM_PROGRAM_ID', C.SYSTEM_PROGRAM_ID);
});

test('integer constants equal constants.rs', () => {
  const rs = parseRustIntConsts();
  const pairs: [string, bigint | number][] = [
    ['MIN_TICK', C.MIN_TICK],
    ['MAX_TICK', C.MAX_TICK],
    ['MIN_SQRT_PRICE_X64', C.MIN_SQRT_PRICE_X64],
    ['MAX_SQRT_PRICE_X64', C.MAX_SQRT_PRICE_X64],
    ['TICK_ARRAY_SIZE', C.TICK_ARRAY_SIZE],
    ['DYNAMIC_FEE_FLAG', C.DYNAMIC_FEE_FLAG],
    ['MAX_LP_FEE', C.MAX_LP_FEE],
    ['MAX_SWAP_FEE', C.MAX_SWAP_FEE],
    // carpenter-types constants.rs).
    ['PROTOCOL_FEE_DEFAULT', C.PROTOCOL_FEE_DEFAULT],
    ['MAX_PROTOCOL_FEE', C.MAX_PROTOCOL_FEE],
    ['PROTOCOL_FEE_DEFAULT_PACKED', C.PROTOCOL_FEE_DEFAULT_PACKED],
    ['PIPS_DENOMINATOR', C.PIPS_DENOMINATOR],
    ['PROTOCOL_FEE_ON_INPUT', C.PROTOCOL_FEE_ON_INPUT],
    ['PROTOCOL_FEE_ON_CURRENCY0', C.PROTOCOL_FEE_ON_CURRENCY0],
    ['PROTOCOL_FEE_ON_CURRENCY1', C.PROTOCOL_FEE_ON_CURRENCY1],
    ['BEFORE_INITIALIZE_FLAG', C.HOOK_FLAGS.BEFORE_INITIALIZE],
    ['AFTER_INITIALIZE_FLAG', C.HOOK_FLAGS.AFTER_INITIALIZE],
    ['BEFORE_ADD_LIQUIDITY_FLAG', C.HOOK_FLAGS.BEFORE_ADD_LIQUIDITY],
    ['AFTER_ADD_LIQUIDITY_FLAG', C.HOOK_FLAGS.AFTER_ADD_LIQUIDITY],
    ['BEFORE_REMOVE_LIQUIDITY_FLAG', C.HOOK_FLAGS.BEFORE_REMOVE_LIQUIDITY],
    ['AFTER_REMOVE_LIQUIDITY_FLAG', C.HOOK_FLAGS.AFTER_REMOVE_LIQUIDITY],
    ['BEFORE_SWAP_FLAG', C.HOOK_FLAGS.BEFORE_SWAP],
    ['AFTER_SWAP_FLAG', C.HOOK_FLAGS.AFTER_SWAP],
    ['BEFORE_DONATE_FLAG', C.HOOK_FLAGS.BEFORE_DONATE],
    ['AFTER_DONATE_FLAG', C.HOOK_FLAGS.AFTER_DONATE],
    ['BEFORE_SWAP_RETURNS_DELTA_FLAG', C.HOOK_FLAGS.BEFORE_SWAP_RETURNS_DELTA],
    ['AFTER_SWAP_RETURNS_DELTA_FLAG', C.HOOK_FLAGS.AFTER_SWAP_RETURNS_DELTA],
    ['AFTER_ADD_LIQUIDITY_RETURNS_DELTA_FLAG', C.HOOK_FLAGS.AFTER_ADD_LIQUIDITY_RETURNS_DELTA],
    ['AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA_FLAG', C.HOOK_FLAGS.AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA],
    ['REQUESTS_ACTIONS_FLAG', C.HOOK_FLAGS.REQUESTS_ACTIONS],
    ['FLOOR_TICK_SPACING', C.FLOOR_TICK_SPACING],
    ['TOKEN_DECIMALS', C.TOKEN_DECIMALS],
    ['SUPPLY', C.SUPPLY],
    ['SALE_TOKENS', C.SALE_TOKENS],
    ['POL_TOKENS', C.POL_TOKENS],
    ['LADDER_TOKENS', C.LADDER_TOKENS],
    ['TICKS_PER_DOUBLING', C.TICKS_PER_DOUBLING],
    ['BID_BAND_TICKS', C.BID_BAND_TICKS],
    ['LP_FEE_PIPS', C.LP_FEE_PIPS],
    ['PLATFORM_BPS', C.PLATFORM_BPS],
    // the platform fee (floor-types constants.rs, pinned to floor-math policy.rs).
    // two defaults under two caps.
    ['TEAM_SALE_FEE_BPS_DEFAULT', C.TEAM_SALE_FEE_BPS_DEFAULT],
    ['TEAM_SALE_FEE_BPS_MAX', C.TEAM_SALE_FEE_BPS_MAX],
    ['TEAM_POOL_FEE_BPS_DEFAULT', C.TEAM_POOL_FEE_BPS_DEFAULT],
    ['TEAM_POOL_FEE_BPS_MAX', C.TEAM_POOL_FEE_BPS_MAX],
    ['MAX_SKIM_FAR_BPS', C.MAX_SKIM_FAR_BPS],
    ['MAX_CREATOR_BPS', C.MAX_CREATOR_BPS],
    ['MAX_HOLDER_BPS', C.MAX_HOLDER_BPS],
    ['SALT_BID', C.SALT_BID],
    ['SALT_SEED', C.SALT_SEED],
    // The staircase's position salts.
    ['SALT_STAIR_TAIL', C.SALT_STAIR_TAIL],
    ['SALT_CUSHION', C.SALT_CUSHION],
    ['SALT_POL_RELIST', C.SALT_POL_RELIST],
    ['SALT_STAIR_RESERVE', C.SALT_STAIR_RESERVE],
    ['SALT_STAIR_STEP0', C.SALT_STAIR_STEP0],
    ['MAX_STAIR_STEPS', C.MAX_STAIR_STEPS],
    ['STAIR_STEPS_PER_RATCHET', C.STAIR_STEPS_PER_RATCHET],
    ['LAUNCH_STATE_BONDING', C.LAUNCH_STATE_BONDING],
    ['LAUNCH_STATE_GRADUATED', C.LAUNCH_STATE_GRADUATED],
    // the steps, the tail, the front-loading and the kinds.
    ['STAIR_KIND_PRESET', C.STAIR_KIND_PRESET],
    ['STAIR_KIND_GENESIS', C.STAIR_KIND_GENESIS],
    ['MODE_COOK', C.MODE_COOK],
    ['MODE_BLUECHIP', C.MODE_BLUECHIP],
    ['MODE_CUSTOM', C.MODE_CUSTOM],
    ['MODE_MOONSHOT', C.MODE_MOONSHOT],
    ['PENDING_PLATFORM_NONE', C.PENDING_PLATFORM_NONE],
    ['PENDING_PLATFORM_TWITTER', C.PENDING_PLATFORM_TWITTER],
    ['PENDING_PLATFORM_GITHUB', C.PENDING_PLATFORM_GITHUB],
    ['PENDING_PLATFORM_DISCORD', C.PENDING_PLATFORM_DISCORD],
    ['PENDING_PLATFORM_TELEGRAM', C.PENDING_PLATFORM_TELEGRAM],
  ];
  for (const [name, value] of pairs) {
    assert.ok(rs[name] !== undefined, `constants.rs ${name}`);
    assert.equal(rs[name], BigInt(value), name);
  }
  // FLOOR_HOOK_FLAGS is an expression in Rust; says 0x20CC | 0x4000.
  const evm =
    C.HOOK_FLAGS.BEFORE_INITIALIZE | C.HOOK_FLAGS.BEFORE_SWAP | C.HOOK_FLAGS.AFTER_SWAP | C.HOOK_FLAGS.BEFORE_SWAP_RETURNS_DELTA | C.HOOK_FLAGS.AFTER_SWAP_RETURNS_DELTA;
  assert.equal(evm, 0x20cc);
  assert.equal(C.FLOOR_HOOK_FLAGS, evm | C.HOOK_FLAGS.REQUESTS_ACTIONS);
  assert.equal(C.FLOOR_HOOK_FLAGS, 0x60cc);
});

test('QUOTE_STATE_* equal carpenter-types layout.rs; QUOTE_CAP_* and the two masks equal carpenter-mint-gate lib.rs', () => {
  const layout = stripLineComments(repoFile('crates/carpenter-types/src/layout.rs'));
  const state = (name: string): number => {
    const m = new RegExp(`pub const ${name}: u8 = (\\d+);`).exec(layout);
    if (!m) throw new Error(`layout.rs: no pub const ${name}: u8`);
    return Number(m[1]);
  };
  assert.equal(C.QUOTE_STATE_LISTED, state('QUOTE_STATE_LISTED'));
  assert.equal(C.QUOTE_STATE_RETIRED, state('QUOTE_STATE_RETIRED'));
  assert.notEqual(C.QUOTE_STATE_LISTED, 0, 'a zeroed account must read as ABSENT');

  const gate = stripLineComments(repoFile('crates/carpenter-mint-gate/src/lib.rs'));
  const bit = (name: string): number => {
    const m = new RegExp(`pub const ${name}: u8 = 1 << (\\d+);`).exec(gate);
    if (!m) throw new Error(`carpenter-mint-gate lib.rs: no pub const ${name}: u8 = 1 << n`);
    return 1 << Number(m[1]);
  };
  const bits: [string, number][] = [
    ['QUOTE_CAP_FREEZE', C.QUOTE_CAP_FREEZE], ['QUOTE_CAP_PERMANENT_DELEGATE', C.QUOTE_CAP_PERMANENT_DELEGATE],
    ['QUOTE_CAP_PAUSABLE', C.QUOTE_CAP_PAUSABLE], ['QUOTE_CAP_DEFAULT_STATE', C.QUOTE_CAP_DEFAULT_STATE],
    ['QUOTE_CAP_HOOK_SLOT', C.QUOTE_CAP_HOOK_SLOT], ['QUOTE_CAP_SCALED_UI', C.QUOTE_CAP_SCALED_UI],
    ['QUOTE_CAP_INTEREST', C.QUOTE_CAP_INTEREST], ['QUOTE_CAP_CONFIDENTIAL', C.QUOTE_CAP_CONFIDENTIAL],
  ];
  for (const [name, ts] of bits) assert.equal(ts, bit(name), name);
  // the masks: the crate spells each as an OR of names; resolve the names through the crate's own bits
  const mask = (name: string): number => {
    const m = new RegExp(`pub const ${name}: u8 =\\s*([A-Z_ |\\n]+?);`).exec(gate);
    if (!m) throw new Error(`carpenter-mint-gate lib.rs: no pub const ${name}: u8 = A | B …`);
    return (m[1] ?? '').split('|').map((x) => x.trim()).filter(Boolean).reduce((acc, n) => acc | bit(n), 0);
  };
  assert.equal(C.QUOTE_CAPS_CUSTODIAL, mask('QUOTE_CAPS_CUSTODIAL'));
  assert.equal(C.QUOTE_CAPS_INERT, mask('QUOTE_CAPS_INERT'));
  assert.equal(C.QUOTE_CAPS_CUSTODIAL & C.QUOTE_CAPS_INERT, 0, 'a bit is Tier A or Tier B, never both');
  assert.equal(C.QUOTE_CAPS_CUSTODIAL | C.QUOTE_CAPS_INERT, 0xff, 'the two tiers cover the byte');
  assert.deepEqual(C.QUOTE_CAP_NAMES.map(([b]) => b), bits.map(([, b]) => b), 'QUOTE_CAP_NAMES names every bit, in bit order');
  assert.deepEqual(C.QUOTE_CAP_NAMES.map(([, n]) => `QUOTE_CAP_${n}`), bits.map(([n]) => n), 'and by the crate\'s own names');
});

test('the Token-2022 extension numbers the flows branch on equal the LiteSVM suites, which hand them to the pinned program', () => {
  const rust = parseRustExtensionTypes(['tests/tests/launch_sale.rs', 'tests/tests/amm_pool.rs']);
  const table: Record<string, number> = EXTENSION_TYPES;
  assert.ok(Object.keys(rust).length >= 8, `parsed ${Object.keys(rust).length} EXT_ consts`);
  for (const [name, value] of Object.entries(rust)) {
    assert.ok(name in table, `EXTENSION_TYPES.${name} (the Rust suites name it)`);
    assert.equal(table[name], value, `EXTENSION_TYPES.${name}`);
  }
  // The two the builders and stateView branch on must be among them — never vacuous.
  for (const must of ['CpiGuard', 'Pausable', 'MemoTransfer']) assert.ok(must in rust, `a LiteSVM suite names EXT_ for ${must}`);
});

/**
 * The SDK's tick-array lengths
 * are Commit 0's (`crates/carpenter-types/src/layout.rs`), held to the numbers that file's OWN test
 * asserts — never to a figure typed beside them here — and the V2 rent at the file's live rate.
 * `DISC.amm.close_tick_array` is held to `disc.rs` `CLOSE_TICK_ARRAY` by the discriminator case above.
 */
test('TICK_ARRAY_RENT_OWNER_OFFSET / TICK_ARRAY_LEN (= the V2 length) equal what layout.rs asserts', () => {
  const src = stripLineComments(repoFile('crates/carpenter-types/src/layout.rs'));
  const asserted = (name: string): number => {
    const m = new RegExp(`assert_eq!\\(${name}, ([0-9_]+)\\)`).exec(src);
    assert.ok(m, `layout.rs asserts ${name}`);
    return Number((m[1] ?? '').replace(/_/g, ''));
  };
  // the Rust keeps ONE length, `TICK_ARRAY_LEN` (= `TICK_ARRAY_LEN_V2`); the V1 body length is the
  // owner offset, and no program accepts a V1 account.
  assert.equal(TICK_ARRAY_RENT_OWNER_OFFSET, asserted('TICK_ARRAY_RENT_OWNER_OFFSET'));
  assert.equal(TICK_ARRAY_LEN_V1, TICK_ARRAY_RENT_OWNER_OFFSET, 'the body ends where the owner starts');
  assert.equal(TICK_ARRAY_LEN, asserted('TICK_ARRAY_LEN'));
  assert.equal(TICK_ARRAY_LEN_V2, TICK_ARRAY_LEN);
  // the definitions too: the owner sits where the TickArray account ends, the length is 32 on
  assert.match(src, /pub const TICK_ARRAY_RENT_OWNER_OFFSET: usize = <TickArray as Layout>::ACCOUNT_LEN;/);
  assert.match(src, /pub const TICK_ARRAY_LEN: usize = TICK_ARRAY_RENT_OWNER_OFFSET \+ 32;/);
  // the rent at the file's (128 + len) × 5,080: 20,563,840
  const rent = /assert_eq!\(rent\(TICK_ARRAY_LEN\), ([0-9_]+)\)/.exec(src);
  assert.ok(rent, 'layout.rs pins rent(TICK_ARRAY_LEN)');
  assert.equal(BigInt(128 + TICK_ARRAY_LEN) * 5_080n, BigInt((rent[1] ?? '').replace(/_/g, '')));
  assert.equal(BigInt(128 + TICK_ARRAY_LEN) * 5_080n, 20_563_840n);
  assert.deepEqual([...DISC.amm.close_tick_array], [0xb9, 0x0a, 0x31, 0x50, 0x41, 0x70, 0xcd, 0xc7]);
});

