

import { U128_MAX, U64_MAX, toU128 } from "./v2/wide.js";

export { U128_MAX, U64_MAX, toU128 };

export const U256_MAX         = (1n << 256n) - 1n;

export function assertU128(v        , name        )       {
  if (typeof v !== "bigint" || v < 0n || v > U128_MAX) {
    throw new RangeError(`${name} must be a bigint in [0, 2^128): got ${String(v)}`);
  }
}

export function mulU128(a        , b        )         {
  return a * b;
}

export function checkedMulU64(x        , m        )                {
  const p = x * m;
  return p > U256_MAX ? null : p;
}

export function checkedAdd(a        , b        )                {
  const p = a + b;
  return p > U256_MAX ? null : p;
}

export function checkedSub(a        , b        )                {
  return a < b ? null : a - b;
}

export function checkedShl64(x        )                {
  return x >> 192n !== 0n ? null : x << 64n;
}

export function divRemU128(x        , d        )                   {
  if (d === 0n) throw new RangeError("division by zero");
  return [x / d, x % d];
}

export function divCeilU128(x        , d        )         {
  const [q, r] = divRemU128(x, d);
  return r === 0n ? q : q + 1n;
}

export function hiLo(x        )                   {
  return [x >> 128n, x & U128_MAX];
}
