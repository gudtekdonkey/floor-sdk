

import { U128_MAX } from "./wide.js";
import { v2Fail } from "./error.js";

export function circulatingS(nLive        , forSale        , held             , dead        )         {
  let out = forSale;
  for (const x of [held.c, held.k, held.f, dead]) {
    out += x;
    if (out > U128_MAX) v2Fail("Overflow");
  }
  if (nLive < out) v2Fail("Overflow");
  return nLive - out;
}

export function sweepIfEmpty(s        , f        , c        , k        )                                     {
  if (s > 0n) return [0n, [f, c, k]];
  const rest = f + c + k;
  if (f + c > U128_MAX || rest > U128_MAX) v2Fail("Overflow");
  return [rest, [0n, 0n, 0n]];
}

export function sweepUnsettled(b      )         {
  let moved = 0n;
  for (let i = 0; i < b.nRanges; i++) {
    moved += b.ranges[i].place;
    if (moved > U128_MAX) v2Fail("Overflow");
  }
  if (moved === 0n) return 0n;
  for (let i = 0; i < b.nRanges; i++) b.ranges[i].place = 0n;
  if (b.sqrtP > b.sqrtLMax) b.sqrtLMax = b.sqrtP;
  return moved;
}

const bytes = (n        , tail          )             => {
  const a = new Uint8Array(n);
  a.set(tail, n - tail.length);
  return a;
};

export const EVM_DEAD                        = [
  new Uint8Array(20),
  bytes(20, [0xde, 0xad]),
  bytes(20, [1]),
  Uint8Array.from([0xde, 0xad, 0, 0, 0, 0, 0, 0, 0, 0, 0x42, 0x06, 0x94, 0x20, 0x69, 0x42, 0x06, 0x94, 0x20, 0x69]),
];

const eq = (a            , b            )          => a.length === b.length && a.every((x, i) => x === b[i]);

export function isEvmDead(a            )          {
  return a.length === 20 && EVM_DEAD.some((d) => eq(d, a));
}

export const SOLANA_INCINERATOR = Uint8Array.from([
  0x00, 0x33, 0x90, 0x72, 0x8d, 0x34, 0x11, 0x60, 0x79, 0xbd, 0xc9, 0x11, 0xbf, 0xff, 0x00, 0xdb, 0xd4, 0x4d, 0x2e, 0xcd, 0xcc, 0xf7, 0x9c, 0xa6, 0xe1, 0x00, 0x38,
  0xe1, 0x00, 0x00, 0x00, 0x00,
]);
export const SOLANA_SYSTEM_PROGRAM = new Uint8Array(32);

export function isSolanaDeadOwner(owner            )          {
  return eq(owner, SOLANA_INCINERATOR) || eq(owner, SOLANA_SYSTEM_PROGRAM);
}
