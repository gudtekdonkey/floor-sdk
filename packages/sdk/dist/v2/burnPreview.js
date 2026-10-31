

import { moneyHeld } from "../math/v2/book.js";

import { sCirc } from "../math/v2/supply.js";

import { priceX64 } from "../math/v2/sqrt.js";
import { tokensToMathV2 } from "./solanaSettle.js";

                        

export function floorAfterBurnV2(
  inputs                                                            ,
  burned        ,
  opts                                             = {},
)                {
  const math = (x        )         => (opts.decimals === undefined ? x : tokensToMathV2(x, opts.decimals));
  if (burned <= 0n) throw new RangeError(`burn ${burned}: must be > 0`);
  const b = math(burned);
  const stored = inputs.sup.live;
  const mint = opts.mintSupply === undefined ? stored : math(opts.mintSupply);
  const liveBefore = mint < stored ? mint : stored;
  const sup0         = { live: liveBefore, dead: inputs.sup.dead };
  const sCircBefore = sCirc(inputs.book, sup0);
  if (b > sCircBefore) throw new RangeError(`burn ${b} > ${sCircBefore} circulating`);
  const liveAfter = liveBefore - b;
  const sCircAfter = sCirc(inputs.book, { live: liveAfter, dead: inputs.sup.dead });
  const beforeX64 = priceX64(inputs.book.sqrtF);
  let swept = 0n;

  if (sCircAfter === 0n) swept = moneyHeld(inputs.book);
  const afterX64 = beforeX64;
  const riseBps = beforeX64 === 0n ? 0n : ((afterX64 - beforeX64) * 10_000n) / beforeX64;
  return { beforeX64, afterX64, riseBps, liveBefore, liveAfter, sCircBefore, sCircAfter, allBurned: swept > 0n || sCircAfter === 0n, swept };
}
