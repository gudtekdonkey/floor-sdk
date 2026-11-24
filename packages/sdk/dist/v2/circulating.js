

import { circulatingV2 } from "../math/v2/supply.js";

export function circulating(p                  )         {
  try {
    return circulatingV2(p.live, 0n, p.forSale, p.heldByBid, p.dead);
  } catch (e) {
    throw new RangeError("Overflow", { cause: e });
  }
}
