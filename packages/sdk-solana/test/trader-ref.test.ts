// Trader referrals: the SDK's link transaction, the sale prefix, the sale sweep, and the
// per-launch lookup table they require.
import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import { AccountRole, type Address,  } from '@solana/kit';

import { addressToBytes, WSOL_MINT, decodeLookupTable } from '@floorlaunchpad/carpenter-sdk';
import {
  DEFAULT_PROGRAM_IDS,
  findHookConfig,
  findLaunch,
  findLaunchAuthority,
  findLaunchConfig,
  findTraderBucket,
  findTraderLink,
  findTraderRefSale,
} from '../src/pda.ts';
import { buildBuy, buildSellBack } from '../src/tx/builders.ts';
import { DISC } from '../src/tx/instructions.ts';
import { assertLaunchTableFrozen, LaunchTableDeactivatedError, LaunchTableNotFrozenError } from '../src/tx/launchAlt.ts';
import {
  buildLinkTrader,
  ED25519_PROGRAM_ID,
  planLinkThenTrade,
  saleRefPrefix,
  traderLinkMessage,
  traderLinkVerifyIx,
  TRADER_LINK_MSG_LEN,
} from '../src/tx/traderRef.ts';
import { addr, FIXTURE_BUYER, FIXTURE_STAIR_STEPS, fixtureLaunch, specRent } from './helpers.ts';

const hex = (b: Uint8Array) => Buffer.from(b).toString('hex');
const fill = (n: number) => addr(0) && (Buffer.alloc(32, n) as unknown as Uint8Array);
const key = async (n: number): Promise<Address> => {
  const { getAddressDecoder } = await import('@solana/kit');
  return getAddressDecoder().decode(fill(n));
};

// The same vector `crates/floor-types/tests/trader_link_vector.rs` pins against the Rust `trader_link_message`.
const VECTOR =
  '464c4f4f525f5452414445525f5245465f4c494e4b5f5631' +
  '03'.repeat(32) + '02' + '04'.repeat(32) + '05'.repeat(32) + '06'.repeat(32) + '00'.repeat(32) + '00b33f7100000000';

test('traderLinkMessage is floor-types trader_link_message byte for byte (the pinned vector)', async () => {
  const m = traderLinkMessage({ floorLaunch: await key(3), cluster: 2, trader: await key(4), tier1: await key(5), tier2: await key(6), tier3: null, deadline: 1_900_000_000n });
  assert.equal(m.length, TRADER_LINK_MSG_LEN);
  assert.equal(hex(m), VECTOR);
});

test('traderLinkVerifyIx: one signature at the offsets link_trader pins ([48, ffff, 16, ffff, 112, 193, ffff])', async () => {
  const msg = new Uint8Array(193).fill(9);
  const sig = new Uint8Array(64).fill(7);
  const attestor = await key(8);
  const ix = traderLinkVerifyIx({ attestor, signature: sig, message: msg });
  assert.equal(ix.programAddress, ED25519_PROGRAM_ID);
  const d = ix.data!;
  assert.equal(d.length, 112 + 193);
  assert.deepEqual([d[0], d[1]], [1, 0]);
  const v = new DataView(d.buffer, d.byteOffset);
  assert.deepEqual([0, 1, 2, 3, 4, 5, 6].map((i) => v.getUint16(2 + 2 * i, true)), [48, 0xffff, 16, 0xffff, 112, 193, 0xffff]);
  assert.deepEqual([...d.slice(16, 48)], [...addressToBytes(attestor)]);
  assert.deepEqual([...d.slice(48, 112)], [...sig]);
  assert.deepEqual([...d.slice(112)], [...msg]);
  assert.throws(() => traderLinkVerifyIx({ attestor, signature: sig.slice(1), message: msg }), /64 bytes/);
});

test('buildLinkTrader: [CU, Ed25519, link_trader]; the accounts in link_trader.rs order; the shape refused by name', async () => {
  const trader = FIXTURE_BUYER;
  const [t1, t2, attestor] = [await key(11), await key(12), await key(13)];
  const att = { tier1: t1, tier2: t2, tier3: null, deadline: 1_900_000_000n, signature: new Uint8Array(64).fill(1) };
  const b = await buildLinkTrader({ trader, attestor, cluster: 1, attestation: att });
  assert.equal(b.instructions.length, 3);
  assert.equal(b.instructions[1]!.programAddress, ED25519_PROGRAM_ID, 'the verify is IMMEDIATELY before the link');
  assert.equal(b.instructions[2], b.main);
  assert.deepEqual([...b.main.data.slice(0, 8)], [...DISC.launch.link_trader]);
  assert.equal(b.main.data.length, 8 + 32 * 3 + 8);
  assert.deepEqual(b.main.accountNames, ['trader', 'launch_config', 'trader_link', 'instructions', 'wsol_mint', 'wsol_bucket', 'floor_hook', 'system_program', 'event_authority', 'program']);
  const acc = b.main.accounts.map((a) => a.address);
  assert.equal(acc[0], trader);
  assert.equal(b.main.accounts[0]!.role, AccountRole.WRITABLE_SIGNER, 'the trader pays the link and the wSOL bucket');
  assert.equal(acc[1], (await findLaunchConfig()).address);
  assert.equal(acc[2], (await findTraderLink(trader)).address);
  assert.equal(acc[5], (await findTraderBucket(trader, WSOL_MINT)).address, 'D4: the link opens the wSOL bucket');
  // The Ed25519 message names THIS trader and these tiers (tier 3 absent = zeros).
  const msg = b.instructions[1]!.data!.slice(112);
  assert.equal(hex(msg), hex(traderLinkMessage({ floorLaunch: DEFAULT_PROGRAM_IDS.floorLaunch, cluster: 1, trader, tier1: t1, tier2: t2, tier3: null, deadline: att.deadline })));
  await assert.rejects(buildLinkTrader({ trader, attestor, cluster: 1, attestation: { ...att, tier2: null, tier3: t2 } }), /tier 3 without tier 2/);
  await assert.rejects(buildLinkTrader({ trader, attestor, cluster: 1, attestation: { ...att, tier2: t1 } }), /repeated/);
  await assert.rejects(buildLinkTrader({ trader, attestor, cluster: 1, attestation: { ...att, tier1: trader } }), /repeated tier or the trader/);
});

test('the sale prefix: five accounts at the FRONT of buy / sell_back remaining, in trader_ref.rs order; absent = byte-identical', async () => {
  const f = await fixtureLaunch();
  const base = { buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS };
  const plain = await buildBuy(base);
  const referred = await buildBuy({ ...base, traderRef: true });
  const named = plain.main.accountNames.length;
  const launch = (await findLaunch(f.mint)).address;
  const want = [
    (await findTraderBucket(FIXTURE_BUYER, WSOL_MINT)).address,
    (await findTraderRefSale(launch)).address,
    (await findLaunchAuthority()).address,
    (await findHookConfig()).address,
    DEFAULT_PROGRAM_IDS.floorHook,
  ];
  // the named part (the event pair included) is unchanged; the prefix sits right after it
  const pre = referred.main.accounts.slice(0, named);
  assert.deepEqual(pre, plain.main.accounts.slice(0, named));
  const prefix = referred.main.accounts.slice(named, named + 5);
  assert.deepEqual(prefix.map((a) => a.address), want);
  assert.deepEqual(prefix.map((a) => a.role), [AccountRole.WRITABLE, AccountRole.WRITABLE, AccountRole.READONLY, AccountRole.READONLY, AccountRole.READONLY]);
  assert.deepEqual(referred.main.accounts.slice(named + 5), plain.main.accounts.slice(named), 'what follows the prefix is the old list');
  assert.deepEqual(referred.main.data, plain.main.data);
  // sell-back, both quotes
  const sb = { buyer: FIXTURE_BUYER, launch: f.state, tokens: 1_000n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } };
  const s0 = await buildSellBack(sb);
  const s1 = await buildSellBack({ ...sb, traderRef: true });
  const sn = s0.main.accountNames.length;
  assert.deepEqual(s1.main.accounts.slice(sn, sn + 5).map((a) => a.address), want);
  assert.ok(s1.cuLimit > s0.cuLimit, 'a referred sell-back budgets the credit CPI');
  // a token quote's prefix names the trader's bucket IN THAT QUOTE
  const quote = addr(4242);
  const p = await saleRefPrefix({ trader: FIXTURE_BUYER, launchMint: f.mint, quoteMint: quote });
  assert.equal(p.traderBucket, (await findTraderBucket(FIXTURE_BUYER, quote)).address);
});

test('planLinkThenTrade: [link, ...trade] while unlinked, the trade alone once linked; refuses an unlinked trader with no link', async () => {
  const f = await fixtureLaunch();
  const link = await buildLinkTrader({ trader: FIXTURE_BUYER, attestor: await key(13), cluster: 1, attestation: { tier1: await key(11), tier2: null, tier3: null, deadline: 1n, signature: new Uint8Array(64) } });
  const buy = await buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, traderRef: true });
  assert.deepEqual(planLinkThenTrade({ trader: FIXTURE_BUYER, linked: false, link, trade: [{ step: 'buy', tx: buy }] }).map((s) => s.step), ['link', 'buy']);
  assert.deepEqual(planLinkThenTrade({ trader: FIXTURE_BUYER, linked: true, trade: [{ step: 'buy', tx: buy }] }).map((s) => s.step), ['buy']);
  assert.throws(() => planLinkThenTrade({ trader: FIXTURE_BUYER, linked: false, trade: [] }), /needs `link`/);
});

test('assertLaunchTableFrozen: refuses an unfrozen or deactivated table by name; a frozen active one passes', () => {
  const t = { address: FIXTURE_BUYER, addresses: [], authority: null, deactivationSlot: 0xffff_ffff_ffff_ffffn };
  assertLaunchTableFrozen('x', t);
  assert.throws(() => assertLaunchTableFrozen('x', { ...t, authority: FIXTURE_BUYER }), (e: unknown) => e instanceof LaunchTableNotFrozenError && e.table === FIXTURE_BUYER);
  assert.throws(() => assertLaunchTableFrozen('x', { ...t, deactivationSlot: 5n }), (e: unknown) => e instanceof LaunchTableDeactivatedError && e.deactivationSlot === 5n);
  // decodeLookupTable's shape is accepted as is: a frozen header reads authority null.
  const data = new Uint8Array(56 + 32);
  const dv = new DataView(data.buffer);
  dv.setUint32(0, 1, true);
  dv.setBigUint64(4, 0xffff_ffff_ffff_ffffn, true);
  data.set(addressToBytes(FIXTURE_BUYER), 56);
  assertLaunchTableFrozen('x', { address: FIXTURE_BUYER, ...decodeLookupTable(data) });
  data[21] = 1;
  data.set(addressToBytes(FIXTURE_BUYER), 22);
  assert.throws(() => assertLaunchTableFrozen('x', { address: FIXTURE_BUYER, ...decodeLookupTable(data) }), LaunchTableNotFrozenError);
});
