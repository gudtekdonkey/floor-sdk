// the SDK's side:
//   · the RATE helpers (`src/protocolFee.ts`) against `crates/carpenter-types/src/protocol_fee.rs`'s own vectors;
//   · the carves (`AmmConfig` @177..@184, `Pool.protocol_fee_currency` @538) and the `["protocol_treasury"]` account;
//   · the six builders (`src/tx/protocolTreasury.ts`): open, the permissionless sweep, the uncapped withdraw, the
//     two-step handover and the capped setter — each naming carpenter_amm's treasury, never floor_hook's;
//   · the four new events.
// ⛔ The CHARGED amounts are the quoter's (the program's crate as wasm) and the chain's — `quote_parity.rs` and
//    `amm_protocol_fee.rs` hold those. What is pinned here is what a client builds, reads and shows.
// The (the setter's bytes, the read-back of a config
// written to (250, 750)), "try also 0 fee" (a 0 setting is expressible and a pool at 0 shows NO fee), "above the
// cap refused" (1,001 by name, before a signature), "non-governance signer refused" (the setter's only signer
// slot is the treasury's governance — never `AmmConfig.authority`).

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import {
  BorshWriter,

  

  
  PROTOCOL_FEE_DEFAULT_PACKED,

  

  
  SYSTEM_PROGRAM_ID,

  WSOL_MINT,

  AccountDecodeError,

  
  PROTOCOL_TREASURY,

  

  decodeProtocolTreasury,
  encodeAccount,
  findPda,
  findProtocolTreasury,

  

  

  

  

  
} from '@floorlaunchpad/carpenter-sdk';
import { decodeEventPayload, eventDiscriminator } from '../src/events.ts';
import { PLATFORM_TREASURY, decodePlatformTreasury } from '../src/layouts.ts';
import { DEFAULT_PROGRAM_IDS, findPlatformTreasury, findTeamTreasury } from '../src/pda.ts';
import { addr } from './helpers.ts';

test('ProtocolTreasury: 112 B, governance @16, pending @48, its OWN discriminator (never read as PlatformTreasury)', () => {
  assert.equal(PROTOCOL_TREASURY.size, 104);
  const bytes = encodeAccount(PROTOCOL_TREASURY, { version: 1, bump: 254, governance: addr(41), pendingGovernance: addr(44) });
  assert.equal(bytes.length, 112);
  const t = decodeProtocolTreasury(bytes);
  assert.deepEqual([t.version, t.bump, t.governance, t.pendingGovernance], [1, 254, addr(41), addr(44)]);
  assert.notDeepEqual([...PROTOCOL_TREASURY.discriminator], [...PLATFORM_TREASURY.discriminator]);
  assert.throws(() => decodePlatformTreasury(bytes), AccountDecodeError, "floor_hook's layout refuses the AMM's account");
  assert.throws(() => decodeProtocolTreasury(encodeAccount(PLATFORM_TREASURY, { governance: addr(41) })), AccountDecodeError);
});

test('findProtocolTreasury is ["protocol_treasury"] under carpenter_amm — never a floor_hook treasury', async () => {
  const t = await findProtocolTreasury();
  assert.equal(t.address, (await findPda(DEFAULT_PROGRAM_IDS.carpenterAmm, [new TextEncoder().encode('protocol_treasury')])).address);
  assert.notEqual(t.address, (await findPda(DEFAULT_PROGRAM_IDS.floorHook, [new TextEncoder().encode('protocol_treasury')])).address);
  assert.notEqual(t.address, (await findPlatformTreasury()).address);
  assert.notEqual(t.address, (await findTeamTreasury()).address);
});

// ---- the four events --------------------------------------------------------------------------------

test('the four protocol-treasury events decode field for field', () => {
  const ev = (name: string, body: BorshWriter) => decodeEventPayload(new Uint8Array([...eventDiscriminator(name), ...body.toBytes()]));
  assert.deepEqual(ev('ProtocolFeeSet', new BorshWriter().u32(PROTOCOL_FEE_DEFAULT_PACKED).u32(0)), {
    program: 'carpenter_amm', name: 'ProtocolFeeSet', data: { previous: PROTOCOL_FEE_DEFAULT_PACKED, protocolFee: 0 },
  });
  assert.deepEqual(ev('ProtocolFeesSwept', new BorshWriter().pubkey(addr(4)).u8(0).pubkey(WSOL_MINT).pubkey(addr(42)).u64(123n).pubkey(addr(3)))?.data, {
    pool: addr(4), currency: 0, mint: WSOL_MINT, destination: addr(42), amount: 123n, sweeper: addr(3),
  });
  assert.deepEqual(ev('ProtocolFeesWithdrawn', new BorshWriter().pubkey(WSOL_MINT).pubkey(addr(42)).pubkey(addr(43)).pubkey(addr(41)).u64(100n).u64(23n))?.data, {
    mint: WSOL_MINT, source: addr(42), destination: addr(43), governance: addr(41), amount: 100n, remaining: 23n,
  });
  assert.deepEqual(ev('ProtocolTreasuryGovernanceChanged', new BorshWriter().pubkey(addr(40)).pubkey(SYSTEM_PROGRAM_ID).pubkey(addr(41)).bool(true))?.data, {
    treasury: addr(40), previous: SYSTEM_PROGRAM_ID, governance: addr(41), accepted: true,
  });
  // the treasury's handover event is NOT floor_hook's TreasuryGovernanceChanged: different names, different discriminators
  assert.notDeepEqual([...eventDiscriminator('ProtocolTreasuryGovernanceChanged')], [...eventDiscriminator('TreasuryGovernanceChanged')]);
});
