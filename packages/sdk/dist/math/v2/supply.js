

import { fail } from "../index.js";
import { boughtBack, forSale } from "./book.js";

export const sCirc = (b      , sup        )         => circulatingV2(sup.live, 0n, forSale(b), boughtBack(b), sup.dead);

export function circulatingV2(minted        , burned        , forSale        , boughtBack        , dead        )         {
  const s = minted - burned - forSale - boughtBack - dead;
  let x = minted;
  for (const y of [burned, forSale, boughtBack, dead]) {
    if (x < y) fail("Overflow");
    x -= y;
  }
  return s;
}

export function sBelow(s        , xC        , xK        )         {
  const v = s - xC - xK;
  return v < 1n ? 1n : v;
}
