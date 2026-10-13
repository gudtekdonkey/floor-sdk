/**
 * The per-launch address lookup table a sale buy is compiled through. A graduating buy does not fit the 1,232-byte
 * packet through the deployment table alone, so every launch has its own table, made frozen when the launch is created
 * (its authority `None`: nobody can extend, deactivate or close it). Its address is not derivable from chain state; the
 * launchpad records it.
 * `planBuy` refuses a table that is not frozen ({@link LaunchTableNotFrozenError}) or is deactivated
 * ({@link LaunchTableDeactivatedError}).
 * ⚠ A table is usable one slot after its last extension.
 */
import {  type Address } from '@solana/kit';

import type { LookupTableContents } from '@carpenter/sdk';

// ---- the consumers' guard ---------------------------------------------------------------------------------------------

/** `u64::MAX`, the deactivation slot of a table never deactivated (`lookupTable.ts`'s `LOOKUP_TABLE_ACTIVE_SLOT`). */
const NOT_DEACTIVATED = 0xffff_ffff_ffff_ffffn;

/**
 * A per-launch table as READ BACK from chain: its contents in stored order plus the two header fields the guard needs.
 * `{ address, ...decodeLookupTable(data) }` is one.
 */
export interface LaunchTableState extends LookupTableContents {
  /** `null` once frozen. */
  readonly authority: Address | null;
  /** `u64::MAX` while not deactivated. */
  readonly deactivationSlot: bigint;
}

/** The table still has an authority: whoever holds it can deactivate and close it under the launch's buys. Rebuild it. */
export class LaunchTableNotFrozenError extends Error {
  override readonly name = 'LaunchTableNotFrozenError';
  readonly table: Address;
  readonly authority: Address;
  constructor(what: string, table: Address, authority: Address) {
    super(`${what}: lookup table ${table} is not frozen (authority ${authority}) — a per-launch table must be frozen on creation; rebuild it`);
    this.table = table;
    this.authority = authority;
  }
}

/** The table is deactivated: it stops resolving once its cooldown ends and can then be closed. Rebuild it. */
export class LaunchTableDeactivatedError extends Error {
  override readonly name = 'LaunchTableDeactivatedError';
  readonly table: Address;
  readonly deactivationSlot: bigint;
  constructor(what: string, table: Address, deactivationSlot: bigint) {
    super(`${what}: lookup table ${table} was deactivated in slot ${deactivationSlot} — rebuild the launch's table`);
    this.table = table;
    this.deactivationSlot = deactivationSlot;
  }
}

/**
 * ⛔ Refuses, BY NAME, a per-launch table that is deactivated ({@link LaunchTableDeactivatedError}) or not frozen
 * ({@link LaunchTableNotFrozenError}). `planBuy` calls it; anything else that compiles through a launch's table should too.
 */
export function assertLaunchTableFrozen(what: string, t: LaunchTableState): void {
  if (t.deactivationSlot === undefined || t.authority === undefined) {
    throw new Error(`${what}: lookup table ${t.address} carries no \`authority\` / \`deactivationSlot\` — pass the table READ BACK from chain ({ address, ...decodeLookupTable(data) })`);
  }
  if (t.deactivationSlot !== NOT_DEACTIVATED) throw new LaunchTableDeactivatedError(what, t.address, t.deactivationSlot);
  if (t.authority !== null) throw new LaunchTableNotFrozenError(what, t.address, t.authority);
}
