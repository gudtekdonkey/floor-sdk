

import { base58Encode } from "../codec.js";
import { SLOT_ACTIVE } from "../solana/floor/accounts.js";
import { TEAM_SLOT_WALLET } from "../solana/floor/instructions.js";
import { priceX64 } from "../math/v2/sqrt.js";
import { TRADE_KIND_TEAM_BUY, TRADE_KIND_TEAM_SHARE_BUY } from "./events.js";

                                             

                                   

                                        

export function teamSlotWallets(slot                     )           {
  return (slot?.members ?? []).filter((m) => m.kind === TEAM_SLOT_WALLET && m.state === SLOT_ACTIVE).map((m) => base58Encode(m.key));
}

export const impactBps = (before        , after        )         => (before === 0n ? 0n : ((after - before) * 10_000n) / before);

export function devBuyFromTrade(t              , team         , ctx                                                = {})                {
  if (!t.isBuy) return null;
  let role                    = null;
  if (t.kind === TRADE_KIND_TEAM_BUY || ctx.teamBuy) role = "team_buy";
  else if (t.kind === TRADE_KIND_TEAM_SHARE_BUY || ctx.teamShareBuy) role = "team_share_buy";
  else if (t.trader === team.creator) role = "creator";
  else if (teamSlotWallets(team.teamSlot).includes(t.trader)) role = "team_member";
  if (role === null) return null;
  const priceBeforeX64 = priceX64(t.sqrtBefore);
  const priceAfterX64 = priceX64(t.sqrtAfter);
  return {
    launch: t.launch, trader: t.trader, role, quoteIn: t.quoteGross - t.refund, tokens: t.tokens, priceBeforeX64, priceAfterX64,
    impactBps: impactBps(priceBeforeX64, priceAfterX64), slot: t.slot,
  };
}

export const devBuyChangeSinceBps = (d        , sqrtNow        )         => impactBps(d.priceAfterX64, priceX64(sqrtNow));

export function memberSoldFromTrade(
  t                                                                                                                   ,
  seats                     ,
  same                                    = (a, b) => a === b,
)                    {
  if (t.isBuy) return null;
  for (const who of t.recipient === undefined ? [t.trader] : [t.trader, t.recipient]) {
    const held = seats.filter((s) => same(s.wallet, who));
    if (held.length === 0) continue;
    const seat = held.reduce((lo, s) => (s.place < lo.place ? s : lo));
    return { launch: t.launch, member: seat.wallet, place: seat.place, amount: t.tokens, quoteOut: t.quoteNet, ...(t.slot === undefined ? {} : { slot: t.slot }) };
  }
  return null;
}
