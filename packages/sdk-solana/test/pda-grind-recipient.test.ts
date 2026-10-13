// PDAs, the B1D grind (bump-255
// candidate == kit's derivation when canonical; the fallback still sorts above wSOL), the
// pending-recipient id (Solidity abi.encode layout), the SPL opcodes, and Token-2022 TLV parsing.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import { getProgramDerivedAddress } from '@solana/kit';

import {
  addressToBytes,
  i32le,
  u64le,
  CARPENTER_AMM_PROGRAM_ID,
  SPL_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  findPool,
  findPosition,
  findTickArray,
  NO_HOOK,
  sortMints,
  decodeMint,
  decodeTokenAccount,
  EXTENSION_TYPES,
  approveCheckedIx,
  closeAccountIx,
  createAtaIdempotentIx,
  requestHeapFrameIx,
  revokeIx,
  setComputeUnitLimitIx,
  syncNativeIx,
  systemTransferIx,
} from '@carpenter/sdk';
import { FLOOR_LAUNCH_PROGRAM_ID, SEEDS } from '../src/constants.ts';
import { findRegistryEntry } from '../src/pda.ts';
import { addr } from './helpers.ts';

test('sortMints is bytewise and findPool refuses unsorted mints instead of sorting them', async () => {
  const [m0, m1] = sortMints(addr(2), addr(1));
  assert.equal(m0, addr(1));
  assert.equal(m1, addr(2));
  assert.throws(() => sortMints(addr(1), addr(1)));
  await assert.rejects(findPool({ mint0: addr(2), mint1: addr(1), fee: 3000, tickSpacing: 60, hook: NO_HOOK }), /sort below/);
  const p = await findPool({ mint0: addr(1), mint1: addr(2), fee: 0x800000, tickSpacing: 60, hook: addr(3) });
  const [expected] = await getProgramDerivedAddress({
    programAddress: CARPENTER_AMM_PROGRAM_ID,
    seeds: [SEEDS.pool, addressToBytes(addr(1)), addressToBytes(addr(2)), new Uint8Array([0, 0, 0x80, 0]), new Uint8Array([60, 0]), addressToBytes(addr(3))],
  });
  assert.equal(p.address, expected, 'fee u32 LE, tick_spacing u16 LE');
});

test('integer seeds are little-endian: tick_array i32, position salt u64, registry index u64', async () => {
  const pool = addr(5);
  const ta = await findTickArray(pool, -3600);
  const [expectedTa] = await getProgramDerivedAddress({ programAddress: CARPENTER_AMM_PROGRAM_ID, seeds: [SEEDS.tickArray, addressToBytes(pool), i32le(-3600)] });
  assert.equal(ta.address, expectedTa);
  assert.deepEqual([...i32le(-3600)], [0xf0, 0xf1, 0xff, 0xff]);
  const pos = await findPosition(pool, addr(6), 100n);
  const [expectedPos] = await getProgramDerivedAddress({ programAddress: CARPENTER_AMM_PROGRAM_ID, seeds: [SEEDS.position, addressToBytes(pool), addressToBytes(addr(6)), u64le(100n)] });
  assert.equal(pos.address, expectedPos);
  const entry = await findRegistryEntry(258n);
  const [expectedEntry] = await getProgramDerivedAddress({ programAddress: FLOOR_LAUNCH_PROGRAM_ID, seeds: [SEEDS.registryEntry, new Uint8Array([2, 1, 0, 0, 0, 0, 0, 0])] });
  assert.equal(entry.address, expectedEntry);
});

test('SPL / ComputeBudget / System opcodes', () => {
  assert.deepEqual([...setComputeUnitLimitIx(1_400_000).data], [2, 0xc0, 0x5c, 0x15, 0]);
  assert.throws(() => setComputeUnitLimitIx(1_400_001), RangeError);
  assert.throws(() => setComputeUnitLimitIx(0), RangeError);
  // RequestHeapFrame (tag 1, u32 LE): the runtime's own bounds, refused by name.
  assert.deepEqual([...requestHeapFrameIx(65_536).data], [1, 0, 0, 1, 0]);
  assert.deepEqual([...requestHeapFrameIx(262_144).data], [1, 0, 0, 4, 0]);
  for (const bad of [32_767, 32_768 + 512, 262_144 + 1024, 0, 1.5]) assert.throws(() => requestHeapFrameIx(bad), RangeError);
  assert.deepEqual([...systemTransferIx(addr(1), addr(2), 7n).data], [2, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual([...createAtaIdempotentIx(addr(1), addr(2), addr(1), addr(3), SPL_TOKEN_PROGRAM_ID).data], [1]);
  assert.deepEqual([...syncNativeIx(addr(1)).data], [17]);
  assert.deepEqual([...closeAccountIx(addr(1), addr(2), addr(3)).data], [9]);
  assert.deepEqual([...approveCheckedIx(addr(1), addr(2), addr(3), addr(4), 5n, 6, TOKEN_2022_PROGRAM_ID).data], [13, 5, 0, 0, 0, 0, 0, 0, 0, 6]);
  assert.deepEqual([...revokeIx(addr(1), addr(2), TOKEN_2022_PROGRAM_ID).data], [5]);
  assert.equal(approveCheckedIx(addr(1), addr(2), addr(3), addr(4), 5n, 6, TOKEN_2022_PROGRAM_ID).programAddress, TOKEN_2022_PROGRAM_ID);
});

/** A Token-2022 account: base, then account_type, then TLV entries. */
function tlvAccount(base: Uint8Array, accountType: number, entries: [number, Uint8Array][]): Uint8Array {
  const padded = new Uint8Array(165);
  padded.set(base);
  const parts: number[] = [...padded, accountType];
  for (const [t, v] of entries) parts.push(t & 0xff, t >> 8, v.length & 0xff, v.length >> 8, ...v);
  return new Uint8Array(parts);
}

test('decodeMint reads the base and the Pausable / MetadataPointer / TokenMetadata TLVs', () => {
  const base = new Uint8Array(82);
  base[0] = 1; // mint authority tag
  base.set(addressToBytes(addr(1)), 4);
  base.set(u64le(1_000_000_000_000_000n), 36);
  base[44] = 6;
  base[45] = 1;
  const pausable = new Uint8Array(33);
  pausable.set(addressToBytes(addr(2)), 0);
  pausable[32] = 1;
  const pointer = new Uint8Array(64);
  pointer.set(addressToBytes(addr(3)), 32);
  const name = new TextEncoder().encode('Test');
  const meta = new Uint8Array([...new Uint8Array(32), ...addressToBytes(addr(3)), 4, 0, 0, 0, ...name, 3, 0, 0, 0, ...new TextEncoder().encode('TST'), 0, 0, 0, 0, 0, 0, 0, 0]);
  const m = decodeMint(tlvAccount(base, 1, [[EXTENSION_TYPES.Pausable, pausable], [EXTENSION_TYPES.MetadataPointer, pointer], [EXTENSION_TYPES.TokenMetadata, meta]]));
  assert.equal(m.mintAuthority, addr(1));
  assert.equal(m.supply, 1_000_000_000_000_000n);
  assert.equal(m.decimals, 6);
  assert.equal(m.freezeAuthority, null);
  assert.deepEqual(m.pausable, { authority: addr(2), paused: true });
  assert.deepEqual(m.metadataPointer, { authority: null, metadataAddress: addr(3) });
  assert.equal(m.tokenMetadata?.name, 'Test');
  assert.equal(m.tokenMetadata?.symbol, 'TST');
  assert.equal(m.tokenMetadata?.updateAuthority, null);
  assert.deepEqual(m.extensionTypes, [26, 18, 19]);
  const plain = decodeMint(base);
  assert.equal(plain.pausable, null);
  assert.deepEqual(plain.extensionTypes, []);
});

test('decodeTokenAccount reads CPI Guard, MemoTransfer and the delegation', () => {
  const base = new Uint8Array(165);
  base.set(addressToBytes(addr(1)), 0);
  base.set(addressToBytes(addr(2)), 32);
  base.set(u64le(500n), 64);
  base[72] = 1;
  base.set(addressToBytes(addr(3)), 76);
  base[108] = 1;
  base.set(u64le(120n), 121);
  const t = decodeTokenAccount(tlvAccount(base, 2, [[EXTENSION_TYPES.CpiGuard, new Uint8Array([1])], [EXTENSION_TYPES.MemoTransfer, new Uint8Array([0])], [EXTENSION_TYPES.ImmutableOwner, new Uint8Array(0)]]));
  assert.equal(t.mint, addr(1));
  assert.equal(t.owner, addr(2));
  assert.equal(t.amount, 500n);
  assert.equal(t.delegate, addr(3));
  assert.equal(t.delegatedAmount, 120n);
  assert.equal(t.cpiGuardLocked, true);
  assert.equal(t.memoRequired, false);
  assert.equal(t.immutableOwner, true);
  assert.throws(() => decodeTokenAccount(tlvAccount(base, 1, [])), /account_type/);
});
