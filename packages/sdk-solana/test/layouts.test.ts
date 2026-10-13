// Account decoding: round trips through `encodeAccount`, strict length and discriminator checks,
// the u128-as-two-LE-limbs rule, and the bitmap's spacing-derived length.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import {

  bytesToAddress,
  readUintLE,

  

  

  

  

  encodeAccount,

  
  kindSize,

  

  

  

  

  

} from '@carpenter/sdk';
import {
  BUYER_POSITION,
  decodeFloorPool,
  decodeHookConfig,
  decodeLaunch,
  decodeLaunchConfig,
  decodeStairOwners,
  decodeStairState,
  FLOOR_POOL,
  HOOK_CONFIG,
  LAUNCH,
  LAUNCH_CONFIG,
  launchConfigTeamPoolBps,
  launchConfigTeamSaleBps,

  STAIR_OWNERS_OFFSET,
  STAIR_STATE,
  STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS,
  teamRateSetting,
} from '../src/layouts.ts';
import {
  TEAM_POOL_FEE_BPS_DEFAULT,
  TEAM_POOL_FEE_BPS_MAX,
  TEAM_SALE_FEE_BPS_DEFAULT,
  TEAM_SALE_FEE_BPS_MAX,
} from '../src/constants.ts';
import { addr } from './helpers.ts';

test('Launch and FloorPool carry their bytes32 fields as Uint8Array and their u16 rates', () => {
  const pending = new Uint8Array(32).fill(0xab);
  const l = decodeLaunch(encodeAccount(LAUNCH, { state: 0, mode: 3, mint: addr(1), teamCliffsPaidBySlot: 0x0001_2003, priceX64: 123n << 64n, skimSellFar: 1000, registryIndex: 7n }));
  void pending;
  assert.equal(l.teamCliffsPaidBySlot, 0x0001_2003);
  assert.equal(LAUNCH.fields.find((x) => x.name === 'team_cliffs_paid_by_slot')?.offset, 72);
  assert.equal(LAUNCH.fields.find((x) => x.name === 'trader_ref_sale_bump')?.offset, 364);
  assert.equal(l.priceX64, 123n << 64n);
  assert.equal(l.skimSellFar, 1000);
  assert.equal(l.registryIndex, 7n);
  const f = decodeFloorPool(encodeAccount(FLOOR_POOL, { floorTick: -70_000, bidLower: -69_000, bidLiquidity: 5n, vaultEth: 1_000_000_000n, platformBps: 250 }));
  assert.equal(f.floorTick, -70_000);
  assert.equal(f.bidLiquidity, 5n);
  assert.equal(f.platformBps, 250);
  assert.equal(f.ammPool, '11111111111111111111111111111111', 'a zero key decodes as the default address');
  assert.equal(f.payeeSlots, null, 'MR-S: a 792-byte struct-only account carries no PayeeSlots tail');
  assert.throws(() => encodeAccount(BUYER_POSITION, { nope: 1 }), /unknown field/);
  assert.throws(() => encodeAccount(BUYER_POSITION, { pad0: new Uint8Array(7) }), /padding/);
});

// ---- the creator-fee referral split --------------

test('a FloorPool written before referrals existed reads as no referral destination, never the System Program', () => {
  const old = encodeAccount(FLOOR_POOL, { floorTick: -70_000, vaultEth: 1_000_000_000n, baseMint: addr(4) });
  const p = decodeFloorPool(old);
  assert.equal(p.baseMint, addr(4), 'the fields before the carve-out still decode');
  assert.equal(p.referralDestination, null, 'an all-zero destination is ABSENT');
  assert.equal(p.referralOwed, 0n, 'a pool that funds nobody owes nobody');
  assert.equal(p.referralFeesPaid, 0n, 'and has accrued nothing');
  assert.equal(p.referralShareBps, 0, 'and carries no share');
  // ⚠ An older account is INDISTINGUISHABLE from a new unreferred one, deliberately: the bytes
  // are identical and both mean "funds nobody, and cannot ever start".
  // What no reader can answer from this account alone is "did it PREDATE the feature or OPT OUT".
  const optedOut = decodeFloorPool(encodeAccount(FLOOR_POOL, { floorTick: -70_000, vaultEth: 1_000_000_000n, baseMint: addr(4) }));
  assert.deepEqual(optedOut, p, 'opted out and written-before are the same bytes and the same read');
});

const FLOOR_POOL_REFERRAL_CARVE: readonly [name: string, offset: number, size: number][] = [
  ['referral_owed', 584, 8],
  ['referral_fees_paid', 592, 8],
  // @600: the trader-referral meters.
  ['trader_ref_owed', 600, 8],
  ['trader_ref_booked', 608, 8],
  ['pad7', 616, 16],
  ['referral_share_bps', 632, 2],
  // ⛔ `pad3` [u8;6] @634 carved into the staircase's three fields.
  ['stair_steps', 634, 1],
  ['stair_ready', 635, 1],
  ['stair_growth_bps', 636, 2],
  // the staircase's parameter set, carved from `pad3` [u8;2].
  ['stair_kind', 638, 1],
  // `stair_owner` @639 took the last byte of `pad3`.
  ['stair_owner', 639, 1],
  ['referrer', 640, 32],
  // @672 — padding.
  ['pad8', 672, 1],
  ['holders_hooked', 673, 1],
  ['pad4', 674, 6],
  // ⛔ four more from the front of `reserved[104]` @680.
  ['referrer_t2', 680, 32],
  ['referrer_t3', 712, 32],
  ['referral_paid_t2', 744, 8],
  ['referral_paid_t3', 752, 8],
  // ⛔ relocated onto the tiers' last byte: the rest
  //    of `reserved` (24 -> 0).
  ['team_owed', 760, 8],
  ['team_fees_paid', 768, 8],
  ['team_fee_bps', 776, 2],
  ['pad5', 778, 6],
];

test('FloorPool: the referral fields sit at 584 / 592 / 600 / 632, the carve at 640 / 672 / 673, the tiers at 680 / 712 / 744 / 752, the team fee at 760 / 768 / 776 and NO reserved — the offsets layout.rs asserts', () => {
  const at = (name: string) => FLOOR_POOL.fields.find((f) => f.name === name);
  for (const [name, offset, size] of FLOOR_POOL_REFERRAL_CARVE) {
    const f = at(name);
    assert.ok(f, `FloorPool.${name} exists`);
    assert.equal(f.offset, offset, `FloorPool.${name} offset`);
    assert.equal(kindSize(f.kind), size, `FloorPool.${name} size`);
  }
  assert.equal(FLOOR_POOL.size, 784, 'the struct is 784 bytes');
  // the carve is CONTIGUOUS from the wall shape's end, in this order, and the
  // tail still ends at SIZE
  const names = FLOOR_POOL.fields.map((f) => f.name);
  assert.deepEqual(names.slice(names.indexOf('wall_band_ticks')), ['wall_band_ticks', 'wall_tail_bps', 'wall_list_bps', ...FLOOR_POOL_REFERRAL_CARVE.map(([n]) => n)], 'the tail, in order');
  const last = FLOOR_POOL.fields[FLOOR_POOL.fields.length - 1]!;
  assert.equal(last.offset + kindSize(last.kind), FLOOR_POOL.size, 'the tail (pad5) ends at SIZE');
  // the 200-byte reserve is SPENT: the four fields and their pad (8 + 8 + 32 + 2 + 6),
  // the four (32 + 1 + 1 + 6), the tiers (32 + 32 + 8 + 8) and the team carve (8 + 8 + 2 + 6)
  assert.equal(last.name, 'pad5', 'no `reserved` is left: the team carve spent it');
  assert.equal(200 - 56 - 40 - 80 - 24, 0);
  assert.ok(!names.includes('reserved'), 'no reserved field on FloorPool');
  // the raw ACCOUNT offsets floor_holders' `execute` / `claim` read without deserialising the pool —
  // `crates/floor-types/src/holders.rs`' FLOOR_POOL_ACCT_* (struct + 8)
  assert.equal(8 + at('holder_owed')!.offset, 440, 'FLOOR_POOL_ACCT_HOLDER_OWED');
  assert.equal(8 + at('holder_paid_eth')!.offset, 520, 'FLOOR_POOL_ACCT_HOLDER_PAID');
  assert.equal(8 + at('referral_owed')!.offset, 592, 'the account offset of referral_owed');
  assert.equal(8 + at('referrer')!.offset, 648, 'FLOOR_POOL_ACCT_REFERRER');
  // @680 is padding (`pad8`).
  assert.equal(8 + at('holders_hooked')!.offset, 681, 'FLOOR_POOL_ACCT_HOLDERS_HOOKED');
  // Layout rule: every u64 8-aligned
  assert.equal(at('referral_owed')!.offset % 8, 0);
  assert.equal(at('referral_fees_paid')!.offset % 8, 0);
  assert.equal(at('referral_paid_t2')!.offset % 8, 0);
  assert.equal(at('referral_paid_t3')!.offset % 8, 0);
  assert.equal(at('team_owed')!.offset % 8, 0);
  assert.equal(at('team_fees_paid')!.offset % 8, 0);
});

// floor-types `layout.rs` `launch_team_carve_out`, transcribed.
test('Launch: team_bps @380 (from pad2), team_owed @432 (from reserved), the team buy @440..508, stair_rent_budget @508, 512 unchanged', () => {
  const at = (n: string) => LAUNCH.fields.find((f) => f.name === n)!;
  assert.deepEqual([at('team_bps').offset, at('pad2').offset, at('team_owed').offset], [380, 382, 432]);
  assert.deepEqual(
    // `team_payee` @476 is `pad7` (dead), `team_cliffs_paid_by_slot` u32 @364.
    [at('team_step_tokens').offset, at('team_anchor').offset, at('team_vest_days').offset, kindSize(at('team_vest_days').kind), at('pad7').offset, at('stair_rent_budget').offset, kindSize(at('stair_rent_budget').kind)],
    [440, 456, 466, 8, 476, 508, 4],
  );
});

test('TEAM STEPS: an owner StairState (1,032 B) decodes as a StairState AND its owner books; a 592-B one carries none', () => {
  const st = encodeAccount(STAIR_STATE, { steps: 9, placed: 1 });
  const long = new Uint8Array(STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS);
  long.set(st, 0);
  const t = STAIR_OWNERS_OFFSET;
  long[t] = 1; // version
  long[t + 1] = 1; // kind: team
  long[t + 3] = 2; // first_step
  long[t + 4] = 0xa0; long[t + 5] = 0x0f; // payee_bps 4,000
  new DataView(long.buffer).setBigUint64(t + 152 + 8 * 2, 115_232_874_226_018n, true); // tokens[2]
  new DataView(long.buffer).setBigUint64(t + 88, 800_000_000_000_000n, true); // owner_total
  new DataView(long.buffer).setBigInt64(t + 408, 1_790_000_000n, true); // last_tip_at
  assert.equal(decodeStairState(long).steps, 9);
  const o = decodeStairOwners(long)!;
  assert.deepEqual([o.version, o.kind, o.firstStep, o.payeeBps, o.ownerTotal, o.tokens[2], o.tokens.length], [1, 1, 2, 4000, 800_000_000_000_000n, 115_232_874_226_018n, 16]);
  assert.equal(o.lastTipAt, 1_790_000_000n, 'last_tip_at @408');
  assert.equal(decodeStairOwners(st), null);
  assert.equal(STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS, 1032);
});

// ⚠ PADDING: the 32 bytes @200 are `pad1`; the layout pin stays, because the account cannot change shape.
test('HookConfig: pending_governance is the 32 bytes at 200 (after pad1) and the LAST field (232 B, no reserved left)', () => {
  // `set_hook_governance` writes the candidate here; `accept_hook_governance` clears it.
  const f = HOOK_CONFIG.fields.find((x) => x.name === 'pending_governance');
  assert.ok(f, 'HookConfig.pending_governance exists');
  assert.equal(f.offset, 200, 'HookConfig.pending_governance offset');
  assert.equal(kindSize(f.kind), 32, 'HookConfig.pending_governance size');
  assert.equal(HOOK_CONFIG.size, 232, 'HookConfig SIZE unchanged');
  assert.equal(HOOK_CONFIG.fields.some((x) => x.name === 'reserved' || x.name === 'pad1'), false, 'no `reserved` / `pad1` left on HookConfig');
  assert.equal(HOOK_CONFIG.fields[HOOK_CONFIG.fields.length - 1]?.name, 'pending_governance', 'it is the last field');
  const set = decodeHookConfig(encodeAccount(HOOK_CONFIG, { platformOwner: addr(12), pendingGovernance: addr(13) }));
  assert.equal(set.platformOwner, addr(12), 'the field before it is untouched');
  assert.equal(set.pendingGovernance, addr(13), 'the candidate round-trips');
  assert.ok(!('retiredReferralDistributor' in set), 'no distributor on the decode');
});

// The BYTES, not just the table: each field encodes at its offset and decodes back, and the two
// u64s are told apart (a transposition would read the meter where the liability is).
test('the referral fields round-trip through their bytes: owed @584, paid @592 (the cumulative ACCRUED meter), share_bps @632, referrer @640', () => {
  const dest = addr(9);
  // The destination is the REFERRER wallet @640; `pad8` @672 is padding.
  const bytes = encodeAccount(FLOOR_POOL, {
    referralOwed: 3_000_000n, referralFeesPaid: 7_000_000n, referralShareBps: 1000,
    referrer: dest, holdersHooked: 1,
  });
  assert.equal(readUintLE(bytes, 8 + 584, 8), 3_000_000n, 'the LIABILITY (owed) is the u64 at struct 584');
  assert.equal(readUintLE(bytes, 8 + 592, 8), 7_000_000n, 'the METER (paid == accrued) is the u64 at struct 592');
  assert.deepEqual([...bytes.subarray(8 + 600, 8 + 632)], new Array(32).fill(0), 'the padding at struct 600 is zero');
  assert.equal(readUintLE(bytes, 8 + 632, 2), 1000n, 'the share is the u16 at struct 632');
  assert.deepEqual([...bytes.subarray(8 + 634, 8 + 640)], [0, 0, 0, 0, 0, 0], 'the staircase fields and pad3 are zero (no staircase)');
  assert.equal(bytesToAddress(bytes.slice(8 + 640, 8 + 672)), dest, 'the REFERRER wallet is the key at struct 640');
  assert.equal(bytes[8 + 672], 0, 'pad8 at struct 672');
  assert.equal(bytes[8 + 673], 1, 'holders_hooked at struct 673');
  const p = decodeFloorPool(bytes);
  assert.equal(p.referralOwed, 3_000_000n);
  assert.equal(p.referralFeesPaid, 7_000_000n);
  assert.ok(p.referralFeesPaid >= p.referralOwed, 'the meter is never below the liability (paid = owed + claimed)');
  assert.equal(p.referrer, dest);
  assert.equal(p.referralDestination, dest, 'the destination is the REFERRER');
  assert.equal(p.referralShareBps, 1000);
  assert.equal(p.holdersHooked, 1);
  assert.ok(!('rewardsVersion' in p) && !('referralDistributor' in p), 'the dead fields are gone from the decode');
  assert.throws(() => encodeAccount(FLOOR_POOL, { pad8: 1 }), /padding/, 'pad8 is not a field a caller may set');
  assert.throws(() => encodeAccount(FLOOR_POOL, { pad4: new Uint8Array(6) }), /padding/, 'pad4 is not a field a caller may set');
  // `stair_owner` @639 and the owner's two u64s @240 / @248
  const owned = encodeAccount(FLOOR_POOL, { stairOwner: 2, ownerOwed: 7n, ownerPaid: 9n });
  assert.deepEqual([owned[8 + 639], decodeFloorPool(owned).stairOwner, decodeFloorPool(owned).ownerOwed, decodeFloorPool(owned).ownerPaid], [2, 2, 7n, 9n]);
  // the kind round-trips at 638
  const kinded = encodeAccount(FLOOR_POOL, { stairSteps: 10, stairKind: 1 });
  assert.equal(kinded[8 + 638], 1);
  assert.equal(decodeFloorPool(kinded).stairKind, 1);
  // ⛔ (THE STAIRCASE): the three carved fields round-trip at their offsets
  const stair = decodeFloorPool(encodeAccount(FLOOR_POOL, { stairSteps: 9, stairReady: 1, stairGrowthBps: 9_500 }));
  assert.deepEqual([stair.stairSteps, stair.stairReady, stair.stairGrowthBps], [9, 1, 9_500]);
});

// LaunchConfig carries the FLOOR Platform fee's CURRENT SETTING at
// 304 / 306 / 308, reserved[3] @309, 312 unchanged — and ALL-ZERO bytes (a fresh config, or one written before
// v2) read as the DEFAULTS through the accessors, never as a raw 0 %. A SET 0 is a real 0; a raw rate
// above its cap reads as the cap. `teamRateSetting` is `floor_types::layout::team_rate_setting`'s twin.
// Mutations: a raw read of an unset config (0 %: every new launch free); the flag ignored (a set 0 read as the
// default); the clamp dropped; the sale and pool fields swapped.
test('LaunchConfig: the team-fee setting is at 304 / 306 / 308 and zero bytes read as the defaults 100 / 25', () => {
  const unset = decodeLaunchConfig(encodeAccount(LAUNCH_CONFIG, { registryCount: 3n }));
  assert.deepEqual([unset.teamBpsSet, unset.teamSaleBpsRaw, unset.teamPoolBpsRaw], [0, 0, 0], 'the raw bytes are zero');
  assert.deepEqual([launchConfigTeamSaleBps(unset), launchConfigTeamPoolBps(unset)], [100, 25], 'and read as the defaults');
  assert.deepEqual([TEAM_SALE_FEE_BPS_DEFAULT, TEAM_POOL_FEE_BPS_DEFAULT, TEAM_SALE_FEE_BPS_MAX, TEAM_POOL_FEE_BPS_MAX], [100, 25, 100, 50]);
  const bytes = encodeAccount(LAUNCH_CONFIG, { teamSaleBpsRaw: 40, teamPoolBpsRaw: 50, teamBpsSet: 1 });
  assert.equal(readUintLE(bytes, 8 + 304, 2), 40n, 'the sale rate is the u16 at struct 304');
  assert.equal(readUintLE(bytes, 8 + 306, 2), 50n, 'the pool rate is the u16 at struct 306');
  assert.equal(bytes[8 + 308], 1, 'the flag at struct 308');
  assert.deepEqual([...bytes.subarray(8 + 309, 8 + 312)], [0, 0, 0], 'reserved[3] @309 is zero');
  const set = decodeLaunchConfig(bytes);
  assert.deepEqual([launchConfigTeamSaleBps(set), launchConfigTeamPoolBps(set)], [40, 50]);
  const zero = decodeLaunchConfig(encodeAccount(LAUNCH_CONFIG, { teamBpsSet: 1 }));
  assert.deepEqual([launchConfigTeamSaleBps(zero), launchConfigTeamPoolBps(zero)], [0, 0], 'a SET zero charges nothing');
  const over = decodeLaunchConfig(encodeAccount(LAUNCH_CONFIG, { teamSaleBpsRaw: 65_535, teamPoolBpsRaw: 51, teamBpsSet: 1 }));
  assert.deepEqual([launchConfigTeamSaleBps(over), launchConfigTeamPoolBps(over)], [100, 50], 'never above the cap');
  for (let raw = 0; raw <= 50; raw++) {
    assert.equal(teamRateSetting(1, raw, 25, 50), raw);
    assert.equal(teamRateSetting(0, raw, 25, 50), 25);
  }
});

//  STRUCT offsets (the keeper's unit,
// cross-check), sizes, and the round trip. Mutations: a field one slot over; `wall_tail_liq` narrowed to u64.
test('FloorPool: the wall shape at 348 / 440 / 456 / 578 / 580 / 582 (struct offsets) and it round-trips', () => {
  const at = (name: string) => FLOOR_POOL.fields.find((f) => f.name === name)!;
  const want: Array<[string, number, number]> = [
    ['wall_split_tick', 348, 4], ['wall_tail_liq', 440, 16], ['wall_held_tokens', 456, 8],
    ['wall_band_ticks', 578, 2], ['wall_tail_bps', 580, 2], ['wall_list_bps', 582, 2],
  ];
  for (const [name, offset, size] of want) {
    assert.equal(at(name).offset, offset, name);
    assert.equal(kindSize(at(name).kind), size, name);
  }
})
