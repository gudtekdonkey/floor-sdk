

export const DEFAULT_GAS               = {
  buyFwdMin: 300_000_000n,
  sellFwdMin: 300_000_000n,
  refundFwdMin: 60_000_000n,
  reclaimAfter: 3600n,
};

export const TON_CLAIM = {
  CREATOR: 1,
  CREATOR_REF: 2,
  TEAM_SLOT: 4,
  TRADER_REF: 8,
  EXTRA_RESERVED: 32,
  PAYOUT: 64,
  TEAM_BUY: 128,
  SHARE_BOUGHT: 256,
  HOLDER: 512,
}         ;
export const TON_CLAIM_KNOWN = 975;

export const TON_WALK_STEPS = 16;

                                     

export const REFERRED_HOP = 60_000_000n;

export const REFERRED_FWD_MIN = 240_000_000n;

export const REFERRED_SELL_FWD_MIN = 240_000_000n;

export const REFERRED_GIFT_FWD_MIN = REFERRED_FWD_MIN + REFERRED_HOP;

export const REFERRED_FWD_DEFAULT = DEFAULT_GAS.buyFwdMin + REFERRED_HOP;
export const REFERRED_SELL_FWD_DEFAULT = DEFAULT_GAS.sellFwdMin + REFERRED_HOP;
export const REFERRED_GIFT_FWD_DEFAULT = REFERRED_FWD_DEFAULT + REFERRED_HOP;
export const REFERRED_SELL_GIFT_FWD_DEFAULT = REFERRED_SELL_FWD_DEFAULT + REFERRED_HOP;

                             

                                                                                                                                               

