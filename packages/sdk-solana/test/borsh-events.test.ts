// Borsh writer/reader round trips (strict: trailing bytes refused) and the event decoders —
// every discriminator equals sha256("event:<Name>")[..8] computed with node:crypto, an encoded
// `Swapped` / `LaunchCreated` decodes field for field, and every schema in `EVENT_SCHEMAS` equals
// the built IDL's field list for that event (name, order and type).

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from './node-test.ts';

import {  BorshWriter, addressToBytes, bytesToBase64, anchorDiscriminator, EVENT_IX_TAG } from '@floorlaunchpad/carpenter-sdk';
import {
  decodeEmitCpiData,
  decodeEventPayload,
  decodeInnerInstructionEvents,
  decodeLogEvents,
  EVENT_LEGACY_SCHEMAS,
  EVENT_NAMES,
  EVENT_SCHEMAS,
  eventDiscriminator,

  
} from '../src/events.ts';
import { addr,  } from './helpers.ts';

test('every event discriminator is sha256("event:<Name>")[..8] (node:crypto), and EVENT_IX_TAG is the LE u64 of sha256("anchor:event")[..8]', () => {
  for (const names of Object.values(EVENT_NAMES)) {
    for (const name of names) {
      const expected = createHash('sha256').update(`event:${name}`).digest().subarray(0, 8);
      assert.deepEqual([...eventDiscriminator(name)], [...expected], name);
      assert.deepEqual([...anchorDiscriminator(`event:${name}`)], [...expected]);
    }
  }
  // the wire tag is the u64 0x1d9acb512ea545e4 LITTLE-endian: the sha prefix reversed
  assert.deepEqual([...EVENT_IX_TAG], [...createHash('sha256').update('anchor:event').digest().subarray(0, 8)].reverse());
  assert.deepEqual([...EVENT_IX_TAG], [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d]);
  assert.equal(EVENT_NAMES.carpenter_amm.length, 34);
  assert.equal(EVENT_NAMES.floor_hook.length, 45);
  assert.equal(EVENT_NAMES.floor_launch.length, 44);
  // floor_holders' six and floor_vesting's three.
  assert.equal(EVENT_NAMES.floor_holders.length, 6);
  // 4 ("same vesting sechdule"): + AbsoluteSchedule (`create_schedule_at`).
  assert.equal(EVENT_NAMES.floor_vesting.length, 4);
});

function swappedPayload(): Uint8Array {
  const body = new BorshWriter()
    .pubkey(addr(1))
    .pubkey(addr(2))
    .bool(true)
    .u64(1_000n)
    .u64(999n)
    .i128(30n)
    .i128(0n)
    .u64(3n)
    .optionU32(2500)
    .u128(1n << 64n)
    .i32(0)
    .u128((1n << 64n) - 5n)
    .i32(-1)
    .u8(3)
    .bool(true)
    .u64(5n)
    .u64(6n)
    // `protocol_fee_total`, `protocol_fee_currency`, before the splits (which stay the last 80 bytes).
    .u64(9n)
    .u8(1)
    .u64(70n)
    .u64(71n)
    .u64(72n)
    .u64(73n)
    .u64(74n)
    .u64(80n)
    .u64(81n)
    .u64(82n)
    .u64(83n)
    .u64(84n)
    // `walk_sqrt_price_x64`, `walk_tick` — APPENDED after the splits (the last 20 B).
    .u128((1n << 64n) - 9n)
    .i32(-2)
    .toBytes();
  return new Uint8Array([...eventDiscriminator('Swapped'), ...body]);
}

test('Swapped decodes field for field; a stale schema (trailing bytes) is an error, not a partial event', () => {
  const ev = decodeEventPayload(swappedPayload());
  assert.ok(ev);
  assert.equal(ev.program, 'carpenter_amm');
  assert.equal(ev.name, 'Swapped');
  assert.deepEqual(ev.data, {
    pool: addr(1),
    trader: addr(2),
    zeroForOne: true,
    amountIn: 1_000n,
    amountOut: 999n,
    hookDelta0: 30n,
    hookDelta1: 0n,
    lpFeeTotal: 3n,
    feeOverride: 2500,
    preSqrtPriceX64: 1n << 64n,
    preTick: 0,
    postSqrtPriceX64: (1n << 64n) - 5n,
    postTick: -1,
    signals: 3,
    bidReplaced: true,
    absorbed: 5n,
    nudgeSpent: 6n,
    protocolFeeTotal: 9n,
    protocolFeeCurrency: 1,
    feeSplit0: [70n, 71n, 72n, 73n, 74n],
    feeSplit1: [80n, 81n, 82n, 83n, 84n],
    walkSqrtPriceX64: (1n << 64n) - 9n,
    walkTick: -2,
  });
  assert.throws(() => decodeEventPayload(new Uint8Array([...swappedPayload(), 0])), /trailing/);
  assert.equal(decodeEventPayload(swappedPayload(), 'floor_hook'), null, 'a program filter excludes other programs');
  assert.equal(decodeEventPayload(new Uint8Array(8)), null);
});

test('emit_cpi! inner instructions and emit! logs both decode; foreign programs are ignored', () => {
  const payload = swappedPayload();
  const cpi = new Uint8Array([...EVENT_IX_TAG, ...payload]);
  assert.equal(decodeEmitCpiData(cpi)?.name, 'Swapped');
  assert.equal(decodeEmitCpiData(payload), null, 'no EVENT_IX_TAG → not an event CPI');
  // ⚠ Partial: a caller names only the programs it reads events of.
  const programs = { carpenter_amm: addr(100), floor_hook: addr(101), floor_launch: addr(102) };
  const keys = [addr(100), addr(200)];
  const events = decodeInnerInstructionEvents(
    [{ instructions: [{ programIdIndex: 0, data: cpi }, { programIdIndex: 1, data: cpi }, { programIdIndex: 0, data: new Uint8Array(3) }] }],
    keys,
    programs,
  );
  assert.equal(events.length, 1);
  const logs = ['Program log: x', `Program data: ${bytesToBase64(payload)}`, 'Program data: !!!'];
  assert.equal(decodeLogEvents(logs).length, 1);
  assert.equal(decodeLogEvents(logs, 'floor_launch').length, 0);
});

test('LaunchCreated carries three strings and PayeeSlotsSet both slot lists', () => {
  const body = new BorshWriter()
    .pubkey(addr(1)).pubkey(addr(2)).pubkey(addr(3)).pubkey(addr(5)).pubkey(addr(6))
    .u64(0n).u8(3).u128(1n).i32(-5).string('Name').string('SYM').string('')
    .pubkey(addr(9)).u8(9).u64(15_000n).u64(0n)
    .toBytes();
  const ev = decodeEventPayload(new Uint8Array([...eventDiscriminator('LaunchCreated'), ...body]));
  assert.equal(ev?.program, 'floor_launch');
  assert.equal(ev?.data['symbol'], 'SYM');
  assert.equal(ev?.data['uri'], '');
  assert.equal(ev?.data['ammPool'], addr(5), 'the pool follows the creator: no recipient between them');
  assert.equal('feeRecipient' in (ev?.data ?? {}), false, 'no feeRecipient field');
  const id = new Uint8Array(32).fill(1);
  const w = new BorshWriter().pubkey(addr(1)).u32(2);
  w.u16(7_500).u8(0).fixed(addressToBytes(addr(3)), 32).u16(2_500).u8(2).fixed(id, 32);
  w.u32(1).u16(10_000).u8(4).fixed(id, 32);
  const slotsSet = w.toBytes();
  assert.equal(slotsSet.length, 32 + 4 + 2 * 35 + 4 + 35, 'two lists, 35 bytes a slot');
  const pe = decodeEventPayload(new Uint8Array([...eventDiscriminator('PayeeSlotsSet'), ...slotsSet]));
  assert.equal(pe?.program, 'floor_launch');
  const creator = pe?.data['creator'] as { bps: number; platform: number; key: Uint8Array }[];
  const team = pe?.data['team'] as { bps: number; platform: number; key: Uint8Array }[];
  assert.deepEqual(creator.map((x) => [x.bps, x.platform]), [[7_500, 0], [2_500, 2]], 'bps then platform, in order');
  assert.deepEqual([...creator[0]!.key], [...addressToBytes(addr(3))]);
  assert.deepEqual([...creator[1]!.key], [...id]);
  assert.deepEqual(team.map((x) => [x.bps, x.platform]), [[10_000, 4]]);
  assert.throws(() => decodeEventPayload(new Uint8Array([...eventDiscriminator('PayeeSlotsSet'), ...slotsSet, 0])), RangeError, 'strict');
  assert.throws(() => eventDiscriminator('PayeeSlotSet'), /unknown event PayeeSlotSet/, 'the per-slot event is gone');
  assert.throws(() => eventDiscriminator('RecipientPending'), /unknown event RecipientPending/, 'RecipientPending is no event any more');
});

test('LaunchCreated is the 16-field body — the quote and terms after uri — and the deleted recipient\'s old bodies are refused', () => {
  const head = (recipient: boolean) => {
    const w = new BorshWriter().pubkey(addr(1)).pubkey(addr(2)).pubkey(addr(3));
    if (recipient) w.pubkey(addr(4));
    return w.pubkey(addr(5)).pubkey(addr(6)).u64(7n).u8(0).u128(2_459_565n).i32(-200_311).string('Name').string('SYM').string('').toBytes();
  };
  const tail = new BorshWriter().pubkey(addr(9)).u8(0).u64(15_000n).u64(20_000n).toBytes();
  assert.equal(tail.length, 32 + 1 + 8 + 8, 'the quote-and-terms tail is 49 bytes');
  const disc = eventDiscriminator('LaunchCreated');
  const current = decodeEventPayload(new Uint8Array([...disc, ...head(false), ...tail]));
  assert.equal(EVENT_SCHEMAS.floor_launch['LaunchCreated']!.length, 16);
  assert.equal(current?.data['quoteMint'], addr(9), 'the quote mint');
  assert.equal(current?.data['quoteDecimals'], 0, 'a 0-decimal quote is a real 0');
  assert.equal(current?.data['quoteUsdCents'], 15_000n, 'the declared price, whole cents');
  assert.equal(current?.data['graduationMcapUsd'], 20_000n, 'the cap AS ASKED');
  assert.equal(current?.data['priceX64'], 2_459_565n, '…beside the price the chain realised');
  // The two old generations: with the recipient and the tail, and with neither (before).
  assert.throws(() => decodeEventPayload(new Uint8Array([...disc, ...head(true), ...tail])), RangeError, 'the 17-field body with the recipient');
  assert.throws(() => decodeEventPayload(new Uint8Array([...disc, ...head(true)])), RangeError, 'a 13-field body');
  assert.throws(() => decodeEventPayload(new Uint8Array([...disc, ...head(false)])), RangeError, 'a body cut after `uri`');
  // A body that stops INSIDE the tail, or runs past it, still throws.
  for (const cut of [1, 8, 48]) {
    assert.throws(() => decodeEventPayload(new Uint8Array([...disc, ...head(false), ...tail.subarray(0, tail.length - cut)])), RangeError, `cut ${cut}`);
  }
  assert.throws(() => decodeEventPayload(new Uint8Array([...disc, ...head(false), ...tail, 0])), RangeError, 'one byte past the tail');
  assert.equal(EVENT_LEGACY_SCHEMAS.floor_launch?.['LaunchCreated'], undefined, 'no legacy generation of LaunchCreated');
});

// `TeamFeesSet` — the setting's old and new values, u16s in
// the program's order (sale, then pool), and a ZERO setting decodes as a real 0, never an absent field.
// Mutations: the two pairs swapped (a pool rate read as the sale's); a trailing byte accepted.
test('TeamFeesSet decodes old and new, sale then pool, and a zero setting is a real 0', () => {
  const body = new BorshWriter().pubkey(addr(1)).pubkey(addr(2)).u16(100).u16(40).u16(25).u16(0).toBytes();
  assert.equal(body.length, 32 + 32 + 2 * 4);
  const ev = decodeEventPayload(new Uint8Array([...eventDiscriminator('TeamFeesSet'), ...body]));
  assert.equal(ev?.program, 'floor_launch');
  assert.equal(ev?.name, 'TeamFeesSet');
  assert.deepEqual(
    [ev?.data['launchConfig'], ev?.data['governance'], ev?.data['oldSaleBps'], ev?.data['saleBps'], ev?.data['oldPoolBps'], ev?.data['poolBps']],
    [addr(1), addr(2), 100, 40, 25, 0],
  );
  assert.deepEqual([...eventDiscriminator('TeamFeesSet')], [0x71, 0xad, 0x12, 0xa7, 0xcc, 0x82, 0x2f, 0x17], 'sha256("event:TeamFeesSet")[..8]');
  assert.throws(() => decodeEventPayload(new Uint8Array([...eventDiscriminator('TeamFeesSet'), ...body, 0])), /trailing/);
});

// StairFeesSwept: the program's
// `#[event] StairFeesSwept { pool, quote, tokens }` — 48 bytes, field for field, in floor_hook's set between its staircase
// siblings, its discriminator Anchor's rule; a short or long body is an error, never a partial event.
test('StairFeesSwept decodes pool, quote, tokens, bounty; a short or long body throws', () => {
  assert.deepEqual(EVENT_NAMES.floor_hook.slice(EVENT_NAMES.floor_hook.indexOf('StairRatcheted'), EVENT_NAMES.floor_hook.indexOf('StairRatcheted') + 2), ['StairRatcheted', 'StairFeesSwept']);
  assert.deepEqual([...eventDiscriminator('StairFeesSwept')], [0x68, 0x29, 0x50, 0xc4, 0xa0, 0x20, 0x6d, 0x06], 'sha256("event:StairFeesSwept")[..8]');
  const body = new BorshWriter().pubkey(addr(7)).u64(1_234_567n).u64(18_446_744_073_709_551_615n).u64(50_000_000n).toBytes();
  assert.equal(body.length, 32 + 8 + 8 + 8);
  const ev = decodeEventPayload(new Uint8Array([...eventDiscriminator('StairFeesSwept'), ...body]));
  assert.equal(ev?.program, 'floor_hook');
  assert.equal(ev?.name, 'StairFeesSwept');
  assert.deepEqual([ev?.data['pool'], ev?.data['quote'], ev?.data['tokens'], ev?.data['bounty']], [addr(7), 1_234_567n, 18_446_744_073_709_551_615n, 50_000_000n]);
  assert.throws(() => decodeEventPayload(new Uint8Array([...eventDiscriminator('StairFeesSwept'), ...body, 0])), /trailing/);
  assert.throws(() => decodeEventPayload(new Uint8Array([...eventDiscriminator('StairFeesSwept'), ...body.subarray(0, 48)])), 'the pre-tip body is refused');
});
