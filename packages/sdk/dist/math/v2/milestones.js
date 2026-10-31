

import { U128_MAX } from "./wide.js";
import { v2Fail } from "./error.js";

const u128 = (x        )         => (x > U128_MAX ? v2Fail("Overflow") : x);

export function bookedFromRemainder(r        , thetaBps        )         {
  const keep = 10_000n - BigInt(thetaBps);
  if (keep < 0n) v2Fail("Overflow");
  return u128(r * keep) / 10_000n;
}

export function splitByRow(amount        , platformBps        , floorBps        )             {
  if (platformBps + floorBps > 10_000) v2Fail("RowInvalid");
  const part = (b        )         => u128(amount * BigInt(b)) / 10_000n;
  const platform = part(platformBps);
  const floorToken = part(floorBps);
  return { platform, floorToken, class: amount - platform - floorToken };
}

export function teamSplit(amount        , levelsPresent                                      )            {
  const tenth = amount / 10n;
  const w = [90n, 9n, 1n];
  const referrers                           = [0n, 0n, 0n];
  let paid = 0n;
  for (let i = 0; i < 3; i++) {
    if (levelsPresent[i]) {
      referrers[i] = (tenth * w[i] ) / 100n;
      paid += referrers[i];
    }
  }
  return { referrers, slot: amount - paid };
}

export function slotSplit(slot        , sharesBps                   )                                    {
  const parts = sharesBps.map((s) => (slot * BigInt(s)) / 10_000n);
  const paid = parts.reduce((a, b) => a + b, 0n);
  return { parts, dust: slot - paid };
}
