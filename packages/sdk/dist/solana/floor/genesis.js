

import { BPS, fail } from "../../math/index.js";
import { fixedRaiseClaim, genesisClaim, genesisWeightListed } from "../../math/genesis.js";
import { GENESIS_ALLOC_CLAIMED, GENESIS_ALLOC_MIGRATED, GENESIS_ALLOC_REFUNDED, GENESIS_ALLOC_WAGMI_CLAIMED } from "./accounts.js";

const U64_MAX = (1n << 64n) - 1n;

export const GENESIS_CONVERT_DISCOUNT_MAX_BPS = 100;

export const GENESIS_LISTED_GETS_DISCOUNT = true;

export const GENESIS_USDC_DECIMALS = 6;

export function genesisConvertQuote(lamports        , solCentsX64        , solDecimals        , quoteCentsX64        , quoteDecimals        , discountBps        )         {
  if (solCentsX64 === 0n || quoteCentsX64 === 0n) fail("Zero");
  if (discountBps > GENESIS_CONVERT_DISCOUNT_MAX_BPS) fail("Overflow");
  const qd = 10n ** BigInt(quoteDecimals);
  if (qd > U64_MAX) fail("Overflow");
  const sd = 10n ** BigInt(solDecimals);
  if (sd >> 128n !== 0n) fail("Overflow");
  const num = lamports * solCentsX64 * qd * (BPS - BigInt(discountBps));
  if (num >> 256n !== 0n) fail("Overflow");
  const den = sd * quoteCentsX64 * BPS;
  if (den >> 128n !== 0n) fail("Overflow");
  const v = (num + den - 1n) / den;
  if (v >> 128n !== 0n) fail("Overflow");
  return v;
}

export function genesisRoomUsdc(floorCapUsdc        , wagmiPinned         )         {
  return wagmiPinned ? U64_MAX : floorCapUsdc;
}

export function genesisConvertRoom(usdcConverted        , floorCapUsdc        , wagmiPinned         )         {
  if (typeof wagmiPinned !== "boolean") throw new TypeError("genesisConvertRoom: the third argument is wagmiPinned (there is no $WAGMI cap)");
  const cap = genesisRoomUsdc(floorCapUsdc, wagmiPinned);
  return usdcConverted >= cap ? 0n : cap - usdcConverted;
}

export function genesisConvertTake(want        , room        , quote                              )                   {
  if (want === 0n) return [0n, 0n];
  const u64 = (v        ) => { if (v > U64_MAX) fail("Overflow"); return v; };
  const full = u64(quote(want));
  if (full <= room) return [want, full];
  const take = u64((want * room) / full);
  if (take === 0n) return [0n, 0n];
  const usdc = u64(quote(take));
  if (usdc > room) fail("Overflow");
  return [take, usdc];
}

                                                                       

export function genesisWaterfall(p                                                                                                                                                           )                   {
  if (p.solConverted > p.raisedFinal) fail("Overflow");
  const floorUsdc = p.usdcConverted < p.floorCapUsdc ? p.usdcConverted : p.floorCapUsdc;
  const used = p.floorUsed ?? floorUsdc;
  if (used > floorUsdc) fail("Overflow");
  const left = p.usdcConverted - used;
  const wagmiOffer = p.wagmiPinned ? left : 0n;
  const wagmiUsdc = p.wagmiSpent ?? wagmiOffer;
  if (wagmiUsdc > wagmiOffer) fail("Overflow");
  return { floorUsdc, wagmiOffer, wagmiUsdc, usdcRefund: left - wagmiUsdc, solUnconverted: p.raisedFinal - p.solConverted };
}

export function genesisMigratedLive(a                                     )         {
  return (a.flags & GENESIS_ALLOC_MIGRATED) !== 0 && (a.flags & GENESIS_ALLOC_REFUNDED) === 0 ? a.migrated : 0n;
}

export function genesisWalletCredit(a                                                                          )                                                                             {
  const migrated = genesisMigratedLive(a);
  const phase3 = a.contributed > migrated + a.listed ? a.contributed - migrated - a.listed : 0n;
  const c = migrated + phase3 + a.listed;
  if (c > U64_MAX) fail("Overflow");
  const w = genesisWeightListed(migrated, phase3, a.listed, GENESIS_LISTED_GETS_DISCOUNT);
  if (w === null || w > U64_MAX) fail("Overflow");
  return { migrated, phase3, listed: a.listed, c, w: w           };
}

export function genesisWalletClaim(g                     , a                     )                     {
  const { c, w } = genesisWalletCredit(a       );
  const raised = g.raisedFinal          ;
  const [tokens, refundLamports] = genesisClaim(c, w, raised, g.weightTotal, g.tokensOut, g.refundTotal);
  const [, refundUsdc] = fixedRaiseClaim(c, raised, 0n, g.usdcRefundTotal);
  const flags = a.flags          ;
  const floor = { tokens, refundLamports, refundUsdc, claimed: (flags & GENESIS_ALLOC_CLAIMED) !== 0 };
  if (g.wagmiOpened !== 1) return { c, w, floor, wagmi: null };
  const [wt, floorBack] = fixedRaiseClaim(c, raised, g.wagmiTokensOut, g.wagmiFloorRefund);
  return { c, w, floor, wagmi: { tokens: wt, floorBack, claimed: (flags & GENESIS_ALLOC_WAGMI_CLAIMED) !== 0 } };
}
