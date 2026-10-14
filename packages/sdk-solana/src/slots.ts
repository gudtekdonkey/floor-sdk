/**
 * The `PayeeSlots` tail of a FLOOR pool: up to {@link MAX_SLOTS} creator-fee RECIPIENTS and up to five team-buy PAYEES,
 * bps shares summing to {@link SLOTS_TOTAL_BPS}. Each slot is an ADDRESS (platform 0, `key` = the wallet) or a PENDING
 * HANDLE (platform 1..=4, `key` = `keccak256(abi.encode(uint8 platform, string handle))`). The pool books ONE total per
 * book; a slot's part is split LAZILY at claim time over the book's LIFETIME:
 *
 * - `earned(L) = floor(L × bps / 10,000)` — per-slot floor, no remainder slot; dust ≤ n − 1 stays in custody.
 * - `due = min(earned(L) − paid, owed)` (saturating).
 *
 * ⛔ One slot at 10,000 bps behaves exactly like a single recipient.
 * ⛔ No handle string rides the Solana wire — the handle is display only.
 */

import type { Address } from '@solana/kit';

import { bytesToAddress } from '@floorlaunchpad/carpenter-sdk';
import {
  FLOOR_POOL_ACCOUNT_LEN_WITH_PARTS,
  FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS,
  MAX_SLOTS,
  PAYEE_SLOT_SIZE,
  PAYEE_SLOTS_OFFSET,
  PAYEE_SLOTS_VERSION,
  SLOT_STATE_PENDING,

  SLOTS_TOTAL_BPS,
  TEAM_PARTS_OFFSET,
} from './constants.ts';

// ---- the PayeeSlots tail (floor_types::slots) -------------------------------------------------------------------------------

/** ONE slot as stored (56 B). */
export interface PayeeSlot {
  /** The wallet (address / resolved) or, while `state === SLOT_STATE_PENDING`, the pending id — raw 32 bytes. */
  key: Uint8Array;
  /** The slot's share of 10,000. */
  bps: number;
  /** The handle's platform 1..=4 (kept after the claim, a record); 0 = an address slot. */
  platform: number;
  /** `SLOT_STATE_WALLET` 0 / `_PENDING` 1 / `_RESOLVED` 2. */
  state: number;
  /** Lifetime QUOTE paid to this slot (a team slot: the keepers' tips charged to it included). */
  paid: bigint;
  /** Lifetime team-step tokens paid to this slot (`withdraw_stair_tokens`); 0 on creator slots. */
  tokensPaid: bigint;
  /** The wallet this slot pays, `null` while pending (or a zero key). Derived. */
  wallet: Address | null;
}

/** The tail (568 B). Every amount is raw units of the pool's own quote / base. */
export interface PayeeSlots {
  /** 0 = never written; {@link PAYEE_SLOTS_VERSION} once `register_pool` wrote the lists. */
  version: number;
  creatorCount: number;
  teamCount: number;
  /** Bit i ⇔ creator slot i still pending. */
  creatorPendingMask: number;
  /** Bit i ⇔ team slot i still pending. */
  teamPendingMask: number;
  /** The slots IN USE (`creatorCount` / `teamCount` of the five stored). */
  creator: PayeeSlot[];
  team: PayeeSlot[];
}

/** Offsets inside one `PayeeSlot`. */
export const PAYEE_SLOT_OFFSETS = Object.freeze({ key: 0, bps: 32, platform: 34, state: 35, pad: 36, paid: 40, tokensPaid: 48 });
/** Offsets inside `PayeeSlots`. */
export const PAYEE_SLOTS_OFFSETS = Object.freeze({
  version: 0, creatorCount: 1, teamCount: 2, creatorPendingMask: 3, teamPendingMask: 4, pad: 5, creator: 8, team: 8 + 5 * PAYEE_SLOT_SIZE,
});

function readSlot(v: DataView, b: Uint8Array, at: number): PayeeSlot {
  const key = b.slice(at, at + 32);
  const state = v.getUint8(at + PAYEE_SLOT_OFFSETS.state);
  const zero = key.every((x) => x === 0);
  return {
    key,
    bps: v.getUint16(at + PAYEE_SLOT_OFFSETS.bps, true),
    platform: v.getUint8(at + PAYEE_SLOT_OFFSETS.platform),
    state,
    paid: v.getBigUint64(at + PAYEE_SLOT_OFFSETS.paid, true),
    tokensPaid: v.getBigUint64(at + PAYEE_SLOT_OFFSETS.tokensPaid, true),
    wallet: state !== SLOT_STATE_PENDING && !zero ? bytesToAddress(key) : null,
  };
}

/**
 * The `PayeeSlots` tail of a `FloorPool` ACCOUNT's bytes (the whole account, discriminator included), or `null` when the
 * account carries none (shorter than 1,576 B). ⛔ A reader never pins the account at 1,576: a longer one (a later tail) still
 * decodes. A count above five is clamped to five (a corrupt book reads what fits).
 */
export function decodePayeeSlots(account: Uint8Array): PayeeSlots | null {
  if (account.length < FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS) return null;
  const b = account.subarray(PAYEE_SLOTS_OFFSET, FLOOR_POOL_ACCOUNT_LEN_WITH_SLOTS);
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const creatorCount = v.getUint8(PAYEE_SLOTS_OFFSETS.creatorCount);
  const teamCount = v.getUint8(PAYEE_SLOTS_OFFSETS.teamCount);
  const list = (base: number, n: number) =>
    Array.from({ length: Math.min(n, MAX_SLOTS) }, (_, i) => readSlot(v, b, base + i * PAYEE_SLOT_SIZE));
  return {
    version: v.getUint8(PAYEE_SLOTS_OFFSETS.version),
    creatorCount,
    teamCount,
    creatorPendingMask: v.getUint8(PAYEE_SLOTS_OFFSETS.creatorPendingMask),
    teamPendingMask: v.getUint8(PAYEE_SLOTS_OFFSETS.teamPendingMask),
    creator: list(PAYEE_SLOTS_OFFSETS.creator, creatorCount),
    team: list(PAYEE_SLOTS_OFFSETS.team, teamCount),
  };
}

// ---- the TeamParts tail (floor_types::slots::TeamParts) ---------------------------------------------------------------------

/** The team's per-slot staircase books. All zero on an account without the tail (no slot has exited a step). */
export interface TeamParts {
  outMask: bigint;
  partTokens: bigint;
  partQuote: bigint;
  tokenCredit: bigint[];
  quoteCredit: bigint[];
  pendingAsk: bigint[];
}

export const NO_TEAM_PARTS: TeamParts = Object.freeze({
  outMask: 0n,
  partTokens: 0n,
  partQuote: 0n,
  tokenCredit: [0n, 0n, 0n, 0n, 0n],
  quoteCredit: [0n, 0n, 0n, 0n, 0n],
  pendingAsk: [0n, 0n, 0n, 0n, 0n],
}) as TeamParts;

/** Decode a `FloorPool` account's `TeamParts` tail (@1,576, 152 B); all zero when the account is shorter. */
export function decodeTeamParts(account: Uint8Array): TeamParts {
  if (account.length < FLOOR_POOL_ACCOUNT_LEN_WITH_PARTS) return NO_TEAM_PARTS;
  const v = new DataView(account.buffer, account.byteOffset + TEAM_PARTS_OFFSET, FLOOR_POOL_ACCOUNT_LEN_WITH_PARTS - TEAM_PARTS_OFFSET);
  const u = (o: number) => v.getBigUint64(o, true);
  const five = (o: number) => [0, 1, 2, 3, 4].map((k) => u(o + 8 * k));
  return {
    outMask: u(0) | (u(8) << 64n),
    partTokens: u(16),
    partQuote: u(24),
    tokenCredit: five(32),
    quoteCredit: five(72),
    pendingAsk: five(112),
  };
}

/** Step `step`'s out-bits (bit s = team slot s exited it); 0 past step 15. */
export function teamOutOf(p: TeamParts, step: number): number {
  if (step < 0 || step >= 16) return 0;
  return Number((p.outMask >> BigInt(5 * step)) & 0x1fn);
}
