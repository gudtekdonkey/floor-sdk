

import { buyMinTokens, quoteBuy, quoteSell } from "../math/v2/trade.js";

import { minOutAt } from "../v2/press.js";
import { TON_WALK_STEPS } from "./types.js";

export function quoteBuyTon(v               , amountIn        , slippageBps        )              {
  const raw = quoteBuy(v.book, v.row, v.sup, amountIn, TON_WALK_STEPS);
  return {
    amountIn, tokens: raw.tokens, minTokens: buyMinTokens(v.book, v.row, v.sup, amountIn, slippageBps, TON_WALK_STEPS),
    refund: raw.refund, budgetHit: raw.budgetHit, steps: raw.steps, fees: raw.fees, graduates: raw.graduatedNow, raw,
  };
}

export function quoteSellTon(v               , tokensIn        , slippageBps        )               {
  const raw = quoteSell(v.book, v.row, v.sup, tokensIn);
  return {
    tokensIn, out: raw.out, minOut: raw.out > 0n ? minOutAt(raw.out, slippageBps) : 0n, unfilled: raw.unfilled, fees: raw.fees, steps: raw.steps, raw,
  };
}

export function quoteBuyFromCell(decode            , bookCell      , row          , sup        , amountIn        , slippageBps        )              {
  return quoteBuyTon({ book: decode(bookCell), row, sup }, amountIn, slippageBps);
}
