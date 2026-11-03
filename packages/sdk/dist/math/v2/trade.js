

import { fail } from "../index.js";
import { U128_MAX } from "./wide.js";
import {
  Undo, ckHeld, ckMoney, cloneBook, fClaims, fOwn, feesAt, feesNow, MAX_WALK_STEPS, moneyHeld, tokensHeld, walkDownAcc, walkUpFrom,
} from "./book.js";

import { FeeAcc, LEG_BACKING, cheaper, feeTotal, netFromGross } from "./fees.js";
import { relay } from "./relay.js";

import { split } from "./split.js";

import { sCirc } from "./supply.js";

const noSplit = ()        => ({ dF: 0n, ckAdd: 0n, ckTake: 0n, ms: 0n, booked: [0n, 0n, 0n, 0n], sqrtFNew: 0n });
const noRelay = ()        => ({ nC: 0, nK: 0, laidC: 0n, laidK: 0n, atF: 0n, kept: 0 });
const add = (a        , b        )         => (a + b > U128_MAX ? fail("Overflow") : a + b);

function commit(dst      , src      )       {
  Object.assign(dst, src);
}

function buyOn(book      , side      , gross        , minTokens        , walkSteps        , keep = true)          {
  const ppm = feesNow(book);
  const budget = netFromGross(gross, ppm);
  if (budget === 0n) fail("NothingToDo");
  const w = cloneBook(book);
  const u = new Undo();

  const acc = new FeeAcc(ppm);
  let walked = walkUpFrom(w, budget, { money: 0n, tokens: 0n, fromResale: 0n, fromFirst: 0n, steps: 0, budgetHit: false, graduatedNow: false }, acc, walkSteps, u);
  const c = cheaper(ppm, feesAt(w, w.sqrtP));
  if (c.some((x, i) => x !== ppm[i]) && walked.tokens > 0n) {
    const spent = walked.money + feeTotal(acc.legs());
    if (spent > U128_MAX || spent > gross) fail("Overflow");
    const budget2 = netFromGross(gross - spent, c);
    if (budget2 > 0n) walked = walkUpFrom(w, budget2, walked, acc, walkSteps, u);
  }
  if (walked.tokens === 0n) fail(tokensHeld(w) === 0n ? "BookEmpty" : "NothingToDo");
  if (walked.tokens < minTokens) fail("SlippageExceeded");
  const fees = acc.legs();
  const paid = walked.money + feeTotal(fees);
  if (paid > U128_MAX || paid > gross) fail("Overflow");
  w.floor = add(w.floor, fees[LEG_BACKING]);
  let sp = noSplit();
  let rl = noRelay();
  if (w.graduated && side !== null) {

    const sNew = sCirc(w, side.sup);

    if (u.bought.length > 0) {
      sp = split(side.row, w.ranges.slice(0, w.nRanges), {
        sqrtF: w.sqrtF, sqrtP1: w.sqrtP, fTotal: fOwn(w), fClaims: fClaims(w), ck0: ckMoney(w), held: ckHeld(w), sNew,
      }, u.bought);
      w.floor = add(w.floor, sp.dF);
      w.sqrtF = sp.sqrtFNew;
    }
    rl = relay(w, side.row, sp.ckAdd, sNew, sp.ckTake);
  }
  const r          = {
    gross, net: walked.money, refund: gross - paid, tokens: walked.tokens, fees, fromResale: walked.fromResale, fromFirst: walked.fromFirst,
    steps: walked.steps, budgetHit: walked.budgetHit, graduatedNow: walked.graduatedNow, sqrtBefore: book.sqrtP, sqrtAfter: w.sqrtP,
    split: sp, relay: rl,
  };
  if (keep) commit(book, w);
  return r;
}

function sellOn(book      , side      , tokens        , minOut        , keep = true)           {
  if (tokens === 0n) fail("NothingToDo");
  const ppm = feesNow(book);
  const w = cloneBook(book);

  const acc = new FeeAcc(ppm);
  const walked = walkDownAcc(w, tokens, acc);
  const fees = acc.legs();
  const total = feeTotal(fees);
  if (walked.tokens === 0n || walked.money <= total) fail(walked.money === 0n && moneyHeld(w) === 0n ? "BookEmpty" : "NothingToDo");
  const out = walked.money - total;
  if (out < minOut) fail("SlippageExceeded");
  w.floor = add(w.floor, fees[LEG_BACKING]);

  const rl = w.graduated && side !== null ? relay(w, side.row, 0n, sCirc(w, side.sup)) : noRelay();
  const r           = {
    tokensIn: tokens, unfilled: tokens - walked.tokens, gross: walked.money, fees, out, steps: walked.steps, sqrtBefore: book.sqrtP, sqrtAfter: w.sqrtP,
    fromF: walked.fromF, fromK: walked.fromK, fromC: walked.fromC, relay: rl,
  };
  if (keep) commit(book, w);
  return r;
}

export function buy(book      , row          , sup        , gross        , minTokens        , opts                         = {})          {
  return buyOn(book, { row, sup }, gross, minTokens, opts.walkSteps ?? MAX_WALK_STEPS);
}

export function sell(book      , row          , sup        , tokens        , minOut        )           {
  return sellOn(book, { row, sup }, tokens, minOut);
}

export const quoteBuy = (book      , row          , sup        , gross        , walkSteps         = MAX_WALK_STEPS)          =>
  buyOn(book, { row, sup }, gross, 0n, walkSteps, false);
export const quoteSell = (book      , row          , sup        , tokens        )           => sellOn(book, { row, sup }, tokens, 0n, false);

export function buyMinTokens(b      , row          , sup        , gross        , slippageBps        , walkSteps         = MAX_WALK_STEPS)         {
  const less = buyLessIn(gross, slippageBps);

  return slippageBps === 10_000 ? 0n : buy(cloneBook(b), row, sup, less, 0n, { walkSteps }).tokens;
}

export function buyLessIn(gross        , slippageBps        )         {

  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > 10_000) fail("VaultShareOutOfRange");
  return (gross * BigInt(10_000 - slippageBps)) / 10_000n;
}

export function bookBuy(book      , gross        , minTokens        , walkSteps         = MAX_WALK_STEPS)          {
  return buyOn(book, null, gross, minTokens, walkSteps);
}

export function bookSell(book      , tokens        , minOut        )           {
  return sellOn(book, null, tokens, minOut);
}

export const quoteBookBuy = (book      , gross        , walkSteps         = MAX_WALK_STEPS)          => buyOn(book, null, gross, 0n, walkSteps, false);
export const quoteBookSell = (book      , tokens        )           => sellOn(book, null, tokens, 0n, false);
