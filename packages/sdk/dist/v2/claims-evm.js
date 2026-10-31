

import { bytesToHex, checksumAddress, decodeAbi, encodeCall } from "../evm/abi.js";
import { CLAIM_CREATOR, CLAIM_CREATOR_REF, CLAIM_PAYOUT, CLAIM_SHARE_BOUGHT, CLAIM_TEAM_BUY, CLAIM_TEAM_SLOT, CLAIM_TRADER_REF } from "../evm/floor/iface.js";
import { CLAIM_HOLDER_EVM, claimV2Call } from "./evm.js";

import { claimPayoutToCallV2 } from "./tx/evm.js";
import { evmSignature } from "../generated/index.js";

export const MAX_CLAIM_QUOTES_PER_CALL_V2 = 25;

export const MAX_CLAIM_LAUNCHES_PER_CALL_V2 = 25;

export const launchBitsEvmV2 = (o                 )         =>
  (o.creator > 0n ? CLAIM_CREATOR : 0) | (o.creatorRef > 0n ? CLAIM_CREATOR_REF : 0) | (o.teamSlot > 0n ? CLAIM_TEAM_SLOT : 0) |
  (o.holder > 0n ? CLAIM_HOLDER_EVM : 0) | (o.teamBuy > 0n ? CLAIM_TEAM_BUY : 0) | (o.shareBought > 0n ? CLAIM_SHARE_BOUGHT : 0);
const quoteBits = (o                , payout         )         =>
  (o.creator > 0n ? CLAIM_CREATOR : 0) | (o.creatorRef > 0n ? CLAIM_CREATOR_REF : 0) | (o.teamSlot > 0n ? CLAIM_TEAM_SLOT : 0) |
  (o.traderRef > 0n ? CLAIM_TRADER_REF : 0) | (payout && o.payout > 0n ? CLAIM_PAYOUT : 0);

export function planEvmClaimAllV2(p

 )                 {
  const maxQ = p.maxQuotesPerCall ?? MAX_CLAIM_QUOTES_PER_CALL_V2;
  const maxL = p.maxLaunchesPerCall ?? MAX_CLAIM_LAUNCHES_PER_CALL_V2;
  for (const m of [maxQ, maxL]) if (!Number.isInteger(m) || m < 1) throw new RangeError("a per-call bound must be ≥ 1");
  const to = p.payoutTo === undefined ? null : checksumAddress(p.payoutTo);
  if (to !== null && BigInt(to) === 0n) throw new RangeError("payoutTo: not the zero address");
  const skipped           = [];

  const byQuote = new Map               ();
  const entry = (q        )        => {
    let e = byQuote.get(q);
    if (!e) byQuote.set(q, (e = { quote: q, types: 0, amount: 0n, launches: [] }));
    return e;
  };
  const seen = new Set        ();
  const payouts                = [];
  for (const o of p.owed.quotes) {
    const q = checksumAddress(o.quote);
    if (seen.has(q)) throw new RangeError(`quote ${q} listed twice`);
    seen.add(q);
    const t = quoteBits(o, to === null);
    if (to !== null && o.payout > 0n) payouts.push({ quote: q, amount: o.payout, call: claimPayoutToCallV2({ floorAmm: p.floorAmm, quote: q, to }) });
    if (t === 0) continue;
    const e = entry(q);
    e.types |= t;
    e.amount += o.creator + o.creatorRef + o.teamSlot + o.traderRef + (to === null ? o.payout : 0n);
  }
  const ids = new Set        ();
  for (const [at, l] of p.owed.launches.entries()) {
    if (ids.has(l.launchId)) throw new RangeError(`launch ${l.launchId} listed twice`);
    ids.add(l.launchId);
    const t = launchBitsEvmV2(l);
    if (t === 0) {
      skipped.push(`launch:${l.launchId}`);
      continue;
    }
    entry(checksumAddress(l.quote)).launches.push({ at, id: l.launchId, types: t, amount: l.creator + l.creatorRef + l.teamSlot + l.holder, tokens: l.teamBuy + l.shareBought });
  }
  const claims               = [];
  let cur                                                                  = { quotes: [], launches: [], types: 0 };
  const flush = ()       => {
    if (cur.quotes.length === 0) return;
    const quotes = cur.quotes.map((e) => e.quote);

    const launchIds = [...cur.launches].sort((a, b) => a.at - b.at).map((l) => l.id);
    claims.push({
      launchIds, quotes, types: cur.types, amount: cur.quotes.reduce((a, e) => a + e.amount, 0n) + cur.launches.reduce((a, l) => a + l.amount, 0n),
      tokens: cur.launches.reduce((a, l) => a + l.tokens, 0n), call: claimV2Call({ floorAmm: p.floorAmm, wallet: p.wallet, types: cur.types, quotes, launchIds }),
    });
    cur = { quotes: [], launches: [], types: 0 };
  };
  for (const e of byQuote.values()) {

    let first = true;
    for (let i = 0; i < Math.max(1, e.launches.length); ) {
      if (cur.quotes.length >= maxQ || cur.launches.length >= maxL) flush();
      const room = maxL - cur.launches.length;
      const slice = e.launches.slice(i, i + room);
      cur.quotes.push(first ? e : { ...e, types: 0, amount: 0n });
      cur.types |= (first ? e.types : 0) | slice.reduce((a, l) => a | l.types, 0);
      cur.launches.push(...slice);
      first = false;
      i += Math.max(1, slice.length);
    }
  }
  flush();
  return { claims, payouts, skipped };
}

export async function readAndPlanEvmClaimAllV2(
  reader                 ,
  p                                                                                                                                                                          ,
)                          {
  const owed = await reader.owed(p.wallet, p.launchIds, p.quotes);
  return planEvmClaimAllV2({ ...p, owed });
}

const OWED_LAUNCH = "(uint256,address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint64,uint256)[]";
const OWED_QUOTE = "(address,uint256,uint256,uint256,uint256,uint256)[]";

export function evmOwedReaderOfViewsV2(p                                                                              )                  {
  const to = checksumAddress(p.floorAmm);
  return {
    async owed(wallet, launchIds, quotes) {
      const qs = quotes.map((q) => checksumAddress(q));
      const [ls, rows] = decodeAbi([OWED_LAUNCH, OWED_QUOTE], await p.ethCall(to, bytesToHex(encodeCall(evmSignature("owedV2"), [wallet, [...launchIds], qs]))))                              ;
      if (ls.length !== launchIds.length || rows.length !== qs.length) throw new RangeError("owedV2: one row a launch id and a quote");
      const launches                    = ls.map((r) => ({
        launchId: r[0]          , quote: checksumAddress(r[1]          ), creator: r[3]          , creatorRef: r[4]          , teamSlot: r[5]          ,
        holder: r[6]          , teamBuy: r[7]          , teamBuyLocked: r[8]          , shareBought: r[10]          ,
      }));
      const out                   = rows.map((r) => ({
        quote: checksumAddress(r[0]          ), creator: r[1]          , creatorRef: r[2]          , teamSlot: r[3]          , traderRef: r[4]          , payout: r[5]          ,
      }));
      return { launches, quotes: out };
    },
  };
}
