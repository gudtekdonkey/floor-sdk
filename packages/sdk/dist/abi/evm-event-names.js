

export const V2_EVENT_FIELD_NAMES = {
  Trade: ["launchId", "trader", "recipient", "isBuy", "quoteGross", "quoteNet", "refund", "tokens", "fees", "traderRef", "fromResale", "fromFirst", "unfilled", "sqrtBefore", "sqrtAfter", "sqrtF", "floor", "cushion", "sCirc", "steps", "budgetHit", "slot", "ts", "kind", "creatorRef", "bidMoney", "msPaid"],
  HolderIndexRolled: ["launchId", "index", "wMatured", "wPending", "carry"],
  DeadSynced: ["launchId", "added", "deadTokens", "sCirc"],

  HolderMoved: ["launchId", "from", "to", "value", "fromBalance", "toBalance"],
  BackingCredited: ["launchId", "donor", "amount", "fAfter", "slot"],

  TokensBurnedV2: ["launchId", "wallet", "role", "place", "fromTeamBuy", "fromWallet", "balanceBefore", "owedAfter", "split", "supply", "sOut", "floorAfterX64", "fromShareBought"],

  RefSlices: ["launchId", "trader", "traderParts", "creatorParts", "og"],

  OgUserSet: ["referrer", "from", "until", "count"],
  OgUserRevoked: ["referrer", "until", "count"],

  OptedIn: ["a"],
}         ;
