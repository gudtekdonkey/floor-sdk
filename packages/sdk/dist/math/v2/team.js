

import { fail } from "../index.js";
import { creatorLegs, platformLegs } from "./fees.js";
import { U64_MAX, U128_MAX } from "./wide.js";

export const MAX_MEMBERS = 10;
export const WEIGHT_ONE = 1_000_000_000n;

export const CREATOR_BUY_LOCK_SECS = 86_400n;

export { CREATOR_REF_BPS, TRADER_REF_BPS } from "./fees.js";

export { platformLegs as platformLegsV2, creatorLegs as creatorLegsV2 } from "./fees.js";

const u128 = (x        )         => (x < 0n || x > U128_MAX ? fail("Overflow") : x);
const members = (n        , what        )       => {
  if (n !== MAX_MEMBERS) throw new RangeError(`${what}: ${n} entries, want ${MAX_MEMBERS}`);
};

export function weightsV2(paid                   , bought        )           {
  members(paid.length, "paid");
  const w           = Array(MAX_MEMBERS).fill(0);
  if (bought === 0n) return w;

  const t = paid.reduce((a, p) => a + p, 0n);
  if (t > U128_MAX || t > bought) fail("Overflow");
  for (let i = 0; i < MAX_MEMBERS; i++) w[i] = Number(((paid[i] < bought ? paid[i] : bought) * WEIGHT_ONE) / bought);
  return w;
}

export function slotWeightsV2(shares                   )           {
  members(shares.length, "shares");
  const total = u128(shares.reduce((a, x) => a + x, 0n));
  if (total === 0n) {
    const w           = Array(MAX_MEMBERS).fill(0);
    w[0] = Number(WEIGHT_ONE);
    return w;
  }
  return weightsV2(shares, total);
}

export function splitMembersV2(amount        , w                   )                     {
  members(w.length, "weights");
  u128(amount);
  const parts           = Array(MAX_MEMBERS).fill(0n);
  let given = 0n;
  for (let i = 0; i < MAX_MEMBERS; i++) {
    const wi = BigInt(w[i]) < WEIGHT_ONE ? BigInt(w[i]) : WEIGHT_ONE;
    let p = (amount * wi) / WEIGHT_ONE;

    if (p > amount - given) p = amount - given;
    parts[i] = p;
    given += p;
  }
  return [parts, amount > given ? amount - given : 0n];
}

export function splitReleaseV2(tokens        , w                   )           {
  members(w.length, "weights");
  if (w.every((x) => x === 0)) {
    const parts           = Array(MAX_MEMBERS).fill(0n);
    parts[0] = u128(tokens);
    return parts;
  }
  const [parts, dust] = splitMembersV2(tokens, w);
  parts[0] = u128(parts[0] + dust);
  return parts;
}

export function splitTeamBuyV2(tokens        , w                   , createdAt        , now        )           {
  const due = createdAt + CREATOR_BUY_LOCK_SECS > U64_MAX ? U64_MAX : createdAt + CREATOR_BUY_LOCK_SECS;
  if (now < due) fail("NotDue");
  return splitReleaseV2(tokens, w);
}
