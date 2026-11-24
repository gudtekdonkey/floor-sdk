

import { U64_MAX } from "./wide.js";
import { v2Fail } from "./error.js";

const Q64 = 64n;

       

export const newPosition = ()           => ({ amount: 0n, owed: 0n, indexSnap: 0n, optedOut: false });

export const optedOutPosition = ()           => ({ ...newPosition(), optedOut: true });

function u64(x        )         {
  if (x < 0n || x > U64_MAX) throw new RangeError("holders: u64 overflow");
  return x;
}

function earned(amount        , dIndex        )         {

  const hi = dIndex >> Q64;
  const lo = dIndex & U64_MAX;
  return BigInt.asUintN(128, amount * hi + ((amount * lo) >> Q64));
}

export const newHolderBook = ()             => ({ index: 0n, w: 0n, carry: 0n, bookedTotal: 0n, paidTotal: 0n });

export const cloneHolderBook = (b            )             => ({ ...b });

export function holderAddReward(b            , amount        , sCirc        )         {
  const booked = b.bookedTotal + amount;
  if (booked > U64_MAX) v2Fail("Overflow");
  b.bookedTotal = booked;
  const total = b.carry + amount;
  if (total > U64_MAX) v2Fail("Overflow");
  const w = b.w;
  if (w === 0n || w > sCirc || total === 0n) {
    b.carry = total;
    return "Carried";
  }
  const step = (total << Q64) / w;

  const out = (step * w + (1n << Q64) - 1n) >> Q64;
  const index = b.index + step;
  if (index >= 1n << 128n) v2Fail("Overflow");
  b.index = index;
  b.carry = total - (out < total ? out : total);
  return "Indexed";
}

export function holderSettle(b            , p          )       {
  if (!p.optedOut) p.owed = u64(p.owed + BigInt.asUintN(64, earned(p.amount, b.index - p.indexSnap)));
  p.indexSnap = b.index;
}

export function holderAdd(b            , p          , amount        )       {
  holderSettle(b, p);
  const a = p.amount + amount;
  if (a > U64_MAX) v2Fail("Overflow");
  p.amount = a;
  if (!p.optedOut) {
    const w = b.w + amount;
    if (w > U64_MAX) v2Fail("Overflow");
    b.w = w;
  }
}

export function holderSub(b            , p          , amount        )       {
  if (amount > p.amount) v2Fail("NotHeld");
  holderSettle(b, p);
  p.amount -= amount;
  if (!p.optedOut) b.w = u64(b.w - amount);
}

export function holderSync(b            , p          , real        )       {
  if (real > p.amount) return holderAdd(b, p, real - p.amount);
  if (real === p.amount) return;
  if (!p.optedOut) {
    const d = b.index - p.indexSnap;
    const all = earned(p.amount, d);
    const kept = earned(real, d);
    p.owed = u64(p.owed + BigInt.asUintN(64, kept));
    const carry = b.carry + BigInt.asUintN(64, all - kept);
    if (carry > U64_MAX) v2Fail("Overflow");
    b.carry = carry;
    b.w = u64(b.w - (p.amount - real));
  }
  p.indexSnap = b.index;
  p.amount = real;
}

export function holderOptIn(b            , p          )       {
  if (!p.optedOut) return;
  p.optedOut = false;
  p.indexSnap = b.index;
  b.w = u64(b.w + p.amount);
}

export function holderTake(b            , p          )         {
  holderSettle(b, p);
  const x = p.owed;
  p.owed = 0n;
  b.paidTotal = u64(b.paidTotal + x);
  return x;
}
