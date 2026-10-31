

import { fail } from "../index.js";
import { KIND_TAIL, checkBook, emptyBook, pushRange } from "./book.js";

import { FRAC, SQRT_TAIL_END, flatMoneyForTokens, liquidityForTokens, moneyBetween } from "./sqrt.js";

import { bookBuy } from "./trade.js";

import { DOWN, U128_MAX, U256_LIMIT, UP, divW, isqrt, mulShr, shlW } from "./wide.js";

export const N0 = 1_000_000_000_000_000n;

export const TOKENS_PER_PPB = N0 / 1_000_000_000n;
export const TOKEN_DECIMALS_MIN = 6;
export const TOKEN_DECIMALS_MAX = 24;
export const QUOTE_DECIMALS_MAX = 30;

                                        

export function sqrtP0(launchCapUsdCents        , quoteUsdCentsX64        , quoteDecimals        )       {
  if (quoteUsdCentsX64 === 0n || quoteDecimals > QUOTE_DECIMALS_MAX || launchCapUsdCents === 0n) fail("QuoteRowCannotOpen");
  const num = shlW(launchCapUsdCents * 10n ** BigInt(quoteDecimals), 64 + 2 * FRAC);
  if (num === null) fail("QuoteRowCannotOpen");
  const den = quoteUsdCentsX64 * N0;
  if (den >= U256_LIMIT) fail("QuoteRowCannotOpen");
  const q = divW(num, den, DOWN);
  if (q === null) fail("QuoteRowCannotOpen");
  const s = isqrt(q);
  if (s === 0n || s > SQRT_TAIL_END) fail("QuoteRowCannotOpen");
  return s;
}

export function usdCentsToUnits(cents        , ctx           )         {
  if (ctx.quoteUsdCentsX64 === 0n) fail("Zero");
  if (ctx.quoteDecimals > 30) fail("Overflow");
  const n = (cents * 10n ** BigInt(ctx.quoteDecimals)) << 64n;
  if (n >= U256_LIMIT) fail("Overflow");
  const q = n / ctx.quoteUsdCentsX64;
  if (q > U128_MAX) fail("Overflow");
  return q;
}

export function teamBuyBounds(row                                                  , ctx           )                   {
  const cents = (row.raiseUsdCents * BigInt(row.teamBuyShareBps)) / 10_000n;
  if (cents > 0xffff_ffff_ffff_ffffn) fail("Overflow");
  return [0n, usdCentsToUnits(cents, ctx)];
}

function edges(s0      , g            )               {
  const at = (rel        )       => {
    const s = mulShr(s0, rel, 64, DOWN);
    if (s === null || s === 0n || s >= SQRT_TAIL_END) fail("QuoteRowCannotOpen");
    return s;
  };
  const a = at(g.sqrtARel);
  const b = g.kind === KIND_TAIL ? SQRT_TAIL_END : g.sqrtBRel === g.sqrtARel ? a : at(g.sqrtBRel);
  if (g.sqrtBRel !== g.sqrtARel && b <= a) fail("QuoteRowCannotOpen");
  return [a, b];
}

export function createBook(row                                                                                                , s0      )                                {
  const book = emptyBook(s0, row.feePpm);

  book.feeStartPpm = [...(row.feeStartPpm ?? [0, 0, 0, 0, 0])];
  const knot = (rel                    )       => mulShr(s0, rel ?? 0n, 32, DOWN) ?? 0n;
  book.sqrtFeeA = knot(row.feeARel);
  book.sqrtFeeB = knot(row.feeBRel);
  let left = N0;
  const n = row.ranges.length;
  for (let i = 0; i < n; i++) {
    const g = row.ranges[i];
    const [a, b] = edges(s0, g);
    let t = BigInt(g.sharePpb) * TOKENS_PER_PPB;
    if (t > 0xffff_ffff_ffff_ffffn) fail("Overflow");
    if (t > left) t = left;
    if (g.kind === KIND_TAIL) {
      t = left;
    }
    if (a !== b && t > 0n) {

      const l = liquidityForTokens(a, b, t);
      if (l === null) fail("QuoteRowCannotOpen");
      if (g.kind !== KIND_TAIL && moneyBetween(l, a, b, UP) === null) fail("QuoteRowCannotOpen");
    }
    pushRange(book, a, b, t, g.kind, g.class);
    const r = book.ranges[i];
    r.thetaBps = g.thetaBps;
    r.psiBps = g.psiBps;
    r.msCapPpm = g.msCapPpm ?? 0;
    left -= t;
  }
  if (n === 0 || book.ranges[n - 1].kind !== KIND_TAIL || left !== 0n) fail("RowTail");
  checkBook(book);
  const r0 = book.ranges[0];
  if (r0.sqrtA === r0.sqrtB && flatMoneyForTokens(r0.sqrtA, r0.tokens, DOWN) === 0n) fail("QuoteRowCannotOpen");
  const raise = flatMoneyForTokens(r0.sqrtA, r0.tokens, UP);
  if (raise === null) fail("QuoteRowCannotOpen");
  return { book, raise };
}

export function quoteOpensV2(row                                                                                                                      , ctx           )       {
  if (ctx.tokenDecimals < TOKEN_DECIMALS_MIN || ctx.tokenDecimals > TOKEN_DECIMALS_MAX) fail("QuoteRowCannotOpen");
  createBook(row, sqrtP0(row.launchCapUsdCents, ctx.quoteUsdCentsX64, ctx.quoteDecimals));
}

export function createLaunchV2(row       , ctx           )            {
  quoteOpensV2(row, ctx);
  const s0 = sqrtP0(row.launchCapUsdCents, ctx.quoteUsdCentsX64, ctx.quoteDecimals);
  const { book, raise } = createBook(row, s0);
  return { book, sqrtP0: s0, raise, teamBuyMax: teamBuyBounds(row, ctx)[1], supply: N0 };
}

export function teamBuyV2(book      , gross        , max        )          {
  const take = gross < max ? gross : max;
  if (take === 0n) {
    return { gross, net: 0n, refund: gross, tokens: 0n, fees: [0n, 0n, 0n, 0n, 0n], fromResale: 0n, fromFirst: 0n, steps: 0, budgetHit: false, graduatedNow: false, sqrtBefore: book.sqrtP, sqrtAfter: book.sqrtP,
      split: { dF: 0n, ckAdd: 0n, ckTake: 0n, ms: 0n, booked: [0n, 0n, 0n, 0n], sqrtFNew: 0n }, relay: { nC: 0, nK: 0, laidC: 0n, laidK: 0n, atF: 0n, kept: 0 } };
  }
  const r = bookBuy(book, take, 0n);
  return { ...r, refund: r.refund + gross - take, gross };
}
