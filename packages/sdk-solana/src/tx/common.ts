/**
 * What FLOOR's builders share: `BuiltTransaction` (its `main` a FLOOR instruction), the quote-mint spec, and `finalize` /
 * `lockCount` / `sumCu` over THIS package's `CU_LIMITS` / `HEAP_FRAMES` (Carpenter's rows plus FLOOR's).
 * `TooManyAccountsError`, `uniqueAccounts` and `finalizeUnits` are `@carpenter/sdk`'s.
 */

import type { Address, Instruction } from '@solana/kit';

import {
  finalizeUnits,
  uniqueAccounts,
  type BuiltTransaction as CarpenterBuiltTransaction,
  MAX_TX_CU,
  SPL_TOKEN_PROGRAM_ID,
  WSOL_DECIMALS,
  WSOL_MINT,
  requestHeapFrameIx,
  setComputeUnitLimitIx,
} from '@carpenter/sdk';
import { CU_LIMITS, HEAP_FRAMES, type CuLimitKey } from '../constants.ts';
import type { FloorInstruction } from './instructions.ts';

export interface BuiltTransaction extends CarpenterBuiltTransaction {
  /** The FLOOR program instruction the transaction exists for. */
  readonly main: FloorInstruction;
}

/**
 * ⛔ Every builder: `SetComputeUnitLimit` from `CU_LIMITS[cu]` and — for the keys `HEAP_FRAMES` lists — a
 * `RequestHeapFrame` right after it. This is the one `finalize` every builder uses.
 */
export function finalize(what: string, cu: CuLimitKey, body: readonly Instruction[], main: FloorInstruction): BuiltTransaction {
  return finalizeUnits(what, CU_LIMITS[cu], body, main, HEAP_FRAMES[cu] ?? null);
}

/** The locks {@link finalize} would count for `body` under `cu`'s budget instructions — program ids included. */
export function lockCount(cu: CuLimitKey, body: readonly Instruction[]): number {
  const heapFrame = HEAP_FRAMES[cu] ?? null;
  const limit = Math.min(CU_LIMITS[cu], MAX_TX_CU);
  const budget: Instruction[] = heapFrame === null ? [setComputeUnitLimitIx(limit)] : [setComputeUnitLimitIx(limit), requestHeapFrameIx(heapFrame)];
  return uniqueAccounts([...budget, ...body]).length;
}

/** The `CU_LIMITS` rows a composed transaction spends, summed (never above the per-transaction cap). */
export function sumCu(...keys: readonly CuLimitKey[]): number {
  return Math.min(keys.reduce((n, k) => n + CU_LIMITS[k], 0), MAX_TX_CU);
}

/**
 * The mint a launch is QUOTED in, with the two facts the builders cannot derive offline — its token program and its
 * `decimals` (read off the mint account, never assumed). ⛔ Omitted, every builder is the wSOL path; given, it must be
 * the launch's own quote mint.
 */
export interface QuoteSpec {
  mint: Address;
  tokenProgram: Address;
  decimals: number;
}

export const WSOL_QUOTE: QuoteSpec = { mint: WSOL_MINT, tokenProgram: SPL_TOKEN_PROGRAM_ID, decimals: WSOL_DECIMALS };
