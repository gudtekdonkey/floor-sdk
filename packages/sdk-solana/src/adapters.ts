/**
 * Glue for the deployment rows and the quoter: a joined deployment row → `ProgramIds`, and an `AccountSource` → the
 * quoter's inputs for a FLOOR pool (a plain pool's read is `@floorsupply/carpenter-sdk`'s `readSwapQuoteAccounts`).
 *
 * ⛔ The quoter takes RAW ACCOUNT BYTES (pool, bitmap, tick arrays, floor_pool, launch), never decoded structs: it
 *    decodes them itself with the same layouts the programs use, so a field cannot be mis-transcribed between a read and
 *    a quote. These helpers gather the bytes in as few RPC calls as possible and tag them with the OLDEST slot read.
 * ⛔ Program ids come from `deployments.json`, never a constant; `DEFAULT_PROGRAM_IDS` exists for tests and scripts.
 */

import type { ClusterDeployment } from './deployments.ts';
import type { SwapQuoteInput } from './quote/quoter.ts';
import {
  addressToBytes,
  decodePool,
  decodeTickArrayBitmap,
  TICK_ARRAY_LEN,
  TICK_ARRAY_RENT_OWNER_OFFSET,
  findBitmap,
  findTickArray,
  type AccountSource,
  type RawAccount,
  type AtSlot,
  initializedArrayStarts,
  tickArrayStartIndex,
  decodeMint,
} from '@floorsupply/carpenter-sdk';
import { address, type Address } from '@solana/kit';

import { DEFAULT_PROGRAM_IDS, findFloorPool, type ProgramIds } from './pda.ts';
/**
 * A deployment row's program ids. Every one is read off the row, never a compiled-in default: a row for another
 * cluster names that cluster's programs, and a row that lacks one is refused by name.
 */
export function programIdsFromDeployment(row: ClusterDeployment): ProgramIds {
  const p = row.programs as ClusterDeployment['programs'] & { floorHolders?: string | null };
  for (const k of ['carpenterAmm', 'floorHook', 'floorLaunch', 'floorHolders'] as const) {
    if (!p[k]) throw new Error(`programIdsFromDeployment: the ${row.name} row (${row.chainKey ?? 'local'}) names no programs.${k}`);
  }
  return {
    carpenterAmm: address(p.carpenterAmm),
    floorHook: address(p.floorHook),
    floorLaunch: address(p.floorLaunch),
    floorHolders: address(p.floorHolders!),
  };
}

/** Everything `quoteSwap` needs for one pool, as bytes, plus what was found. */
export interface SwapQuoteAccounts extends Omit<SwapQuoteInput, 'zeroForOne' | 'amountSpecified' | 'sqrtPriceLimitX64'> {
  poolKey: Uint8Array;
  pool: Uint8Array;
  bitmap: Uint8Array;
  /** Each the 3,888-byte BODY of a 3,920-byte array account — its rent owner is not a quote input. */
  tickArrays: Uint8Array[];
  /** The start index of each entry of `tickArrays`, in the same order. */
  tickArrayStarts: number[];
  /** `null` on a plain pool. */
  floorPool: Uint8Array | null;
  /** The base mint's supply (currency1) on a FLOOR pool; `null` on a plain pool. */
  supply1: bigint | null;
  isFloor: boolean;
}

/**
 * Read a pool's quoting accounts: the pool, its bitmap, the initialized tick arrays within
 * `radius` arrays of the current tick on both sides (or exactly `starts`), and on a FLOOR pool its
 * `FloorPool` and the base mint. Two round trips (pool + bitmap, then the rest).
 * ⚠ A walk that leaves the window fails `TickArrayMissing{start_index}`; widen `radius` or pass
 *    the named start in `starts` and read again. The quoter says which array it needs.
 */
export async function readSwapQuoteAccounts(
  src: AccountSource,
  pool: Address,
  opts: { radius?: number; starts?: readonly number[]; ids?: ProgramIds } = {},
): Promise<AtSlot<SwapQuoteAccounts>> {
  const ids = opts.ids ?? DEFAULT_PROGRAM_IDS;
  const bitmapAddr = (await findBitmap(pool, ids)).address;
  const first = await src.getMultipleAccounts([pool, bitmapAddr]);
  const poolRaw = owned(first.accounts[0], ids.carpenterAmm);
  const bitmapRaw = owned(first.accounts[1], ids.carpenterAmm);
  if (!poolRaw) throw new Error(`readSwapQuoteAccounts: no pool at ${pool}`);
  if (!bitmapRaw) throw new Error(`readSwapQuoteAccounts: no bitmap for ${pool}`);
  const p = decodePool(poolRaw.data);
  const bm = decodeTickArrayBitmap(bitmapRaw.data);
  const s = p.tickSpacing;
  const isFloor = p.hook === ids.floorHook;

  let starts: number[];
  if (opts.starts) {
    starts = [...new Set(opts.starts)].sort((a, b) => a - b);
  } else {
    const radius = opts.radius ?? 3;
    const here = tickArrayStartIndex(p.tickCurrent, s);
    const span = 60 * s;
    starts = initializedArrayStarts(bm.words, s).filter((st) => Math.abs(st - here) <= radius * span);
  }
  const arrayAddrs = await Promise.all(starts.map(async (st) => (await findTickArray(pool, st, ids)).address));
  const extra: Address[] = isFloor ? [(await findFloorPool(pool, ids)).address, p.mint1] : [];
  const second = await src.getMultipleAccounts([...arrayAddrs, ...extra]);

  const tickArrays: Uint8Array[] = [];
  const tickArrayStarts: number[] = [];
  starts.forEach((st, i) => {
    const a = owned(second.accounts[i], ids.carpenterAmm);
    // An array is the 3,920-byte account — the program refuses a 3,888-byte V1 account, so it is skipped here like
    // any other length, never truncated. The quoter is handed the 3,888-byte BODY (`TICK_ARRAY_BODY_LEN`).
    if (a && a.data.length === TICK_ARRAY_LEN) {
      tickArrays.push(a.data.subarray(0, TICK_ARRAY_RENT_OWNER_OFFSET));
      tickArrayStarts.push(st);
    }
  });
  let floorPool: Uint8Array | null = null;
  let supply1: bigint | null = null;
  if (isFloor) {
    const fp = owned(second.accounts[starts.length], ids.floorHook);
    if (!fp) throw new Error(`readSwapQuoteAccounts: no FloorPool for ${pool}`);
    floorPool = fp.data;
    const mint = second.accounts[starts.length + 1];
    if (!mint) throw new Error(`readSwapQuoteAccounts: no base mint ${p.mint1}`);
    supply1 = decodeMint(mint.data).supply;
  }
  const slot = first.slot < second.slot ? first.slot : second.slot;
  return {
    slot,
    value: { poolKey: addressToBytes(pool), pool: poolRaw.data, bitmap: bitmapRaw.data, tickArrays, tickArrayStarts, floorPool, supply1, isFloor },
  };
}

/** `saleBuy` / `saleSellBack` take the raw `Launch` (and `BuyerPosition`); read them as bytes. */
export async function readRawOwned(src: AccountSource, addr: Address, program: Address): Promise<AtSlot<Uint8Array | null>> {
  const { slot, account } = await src.getAccount(addr);
  const a = owned(account, program);
  return { slot, value: a ? a.data : null };
}

function owned(a: RawAccount | null | undefined, program: Address): RawAccount | null {
  return a && a.owner === program ? a : null;
}
