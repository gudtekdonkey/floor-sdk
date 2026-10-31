

import { concatBytes, uintLE } from "../codec.js";
import { sha256 } from "../sha256.js";

export const AccountRole = Object.freeze({ READONLY: 0, WRITABLE: 1, READONLY_SIGNER: 2, WRITABLE_SIGNER: 3 }         );

export const R = (address        )              => ({ address, role: 0 });
export const W = (address        )              => ({ address, role: 1 });
export const RS = (address        )              => ({ address, role: 2 });
export const WS = (address        )              => ({ address, role: 3 });

export function instructionDiscriminator(name        )             {
  return sha256(new TextEncoder().encode(`global:${name}`)).slice(0, 8);
}

export function accountDiscriminator(name        )             {
  return sha256(new TextEncoder().encode(`account:${name}`)).slice(0, 8);
}

export function eventDiscriminator(name        )             {
  return sha256(new TextEncoder().encode(`event:${name}`)).slice(0, 8);
}

export const EVENT_IX_TAG_LE = Uint8Array.from([0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d]);

export const COMPUTE_BUDGET_PROGRAM_ID = "ComputeBudget111111111111111111111111111111";

export const MAX_COMPUTE_UNIT_LIMIT = 1_400_000;

export function computeUnitLimitInstruction(units        )              {
  if (!Number.isInteger(units) || units < 0 || units > 0xffff_ffff) throw new RangeError("units must fit a u32");
  return { programAddress: COMPUTE_BUDGET_PROGRAM_ID, accounts: [], data: concatBytes(new Uint8Array([2]), uintLE(BigInt(units), 4)) };
}

export function computeUnitLimitFor(consumed        , marginBps = 1_000)         {
  if (!Number.isInteger(consumed) || consumed < 0) throw new RangeError("consumed must be a whole number of compute units");
  if (!Number.isInteger(marginBps) || marginBps < 0) throw new RangeError("marginBps must be a whole number of bps");
  return Math.min(MAX_COMPUTE_UNIT_LIMIT, consumed + Math.ceil((consumed * marginBps) / 10_000));
}

export function computeUnitPriceInstruction(microLamports        )              {
  if (microLamports < 0n || microLamports > 0xffff_ffff_ffff_ffffn) throw new RangeError("microLamports must fit a u64");
  return { programAddress: COMPUTE_BUDGET_PROGRAM_ID, accounts: [], data: concatBytes(new Uint8Array([3]), uintLE(microLamports, 8)) };
}

export function priorityFeeLamports(limit        , microLamports        )         {
  return (BigInt(limit) * microLamports + 999_999n) / 1_000_000n;
}
