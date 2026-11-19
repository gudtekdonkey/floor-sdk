

export const TON_MAINNET_GAS_CONFIG               = {
  gasPrice: 400n,
  flatGasLimit: 100n,
  flatGasPrice: 40_000n,
  lumpPrice: 400_000n,
  bitPrice: 26_214_400n,
  cellPrice: 2_621_440_000n,
  storageBitPs: 1n,
  storageCellPs: 500n,
};

export const TON_CLAIM_COST_CONSTS = {
  CLAIM_UNITS: 30000n,
  LAUNCH_CLAIM_UNITS: 200000n,
  QUOTE_HOP_UNITS: 60000n,
  BOOK_UNITS: 15000n,
  PAYOUT_NOTIFY_UNITS: 20000n,
  HOP_MSG_BITS: 1400n,
  HOP_MSG_CELLS: 4n,
  ACCOUNT_STATE_BITS: 33200n,
  ACCOUNT_STATE_CELLS: 68n,
  LAUNCH_STATE_BITS: 843700n,
  LAUNCH_STATE_CELLS: 1303n,
  QUOTE_STATE_BITS: 20000n,
  QUOTE_STATE_CELLS: 50n,
  RENT_SECONDS: 31536000n,
  QUOTE_RENT_SECONDS: 157680000n,
  WALLET_SEND_GAS: 22100n,
  WALLET_RECEIVE_GAS: 20800n,
  WALLET_INDEX_GAS: 9300n,
  LAUNCH_DELTA_GAS: 13600n,
  DELTA_MSG_BITS: 800n,
  DELTA_MSG_CELLS: 2n,
  INDEX_MSG_BITS: 1100n,
  INDEX_MSG_CELLS: 2n,
  INIT_STATE_BITS: 29000n,
  INIT_STATE_CELLS: 58n,
  WALLET_STORAGE_BITS: 29000n,
  WALLET_STORAGE_CELLS: 58n,
  MIN_STORAGE_SECONDS: 157680000n,
  TRADER_UNITS: 17300n,
  LAUNCH_BURN_GAS: 163100n,
  LAUNCH_SELL_RETURN_GAS: 80000n,
  SELL_MSG_BITS: 3200n,
  SELL_MSG_CELLS: 8n,
  CREATE_UNITS: 325400n,
  SETUP_UNITS: 1500000n,
  UPGRADE_UNITS: 150000n,
  CHILD_UPGRADE_UNITS: 40000n,
  FACTORY_OP_UNITS: 60000n,
  FAN_UNITS: 200000n,
  GAS_PAYOUT_FWD_MIN: 60_000_000n,
  GAS_BUY_FWD_MIN: 120000000n,
  TRADE_UNITS: 1000000n,
  TEAM_SHARE_BOOKS: 8n,
  TEAM_SHARE_REF_BOOKS: 11n,
}         ;
const K = TON_CLAIM_COST_CONSTS;

const ceilDiv = (a        , b        )         => (a + b - 1n) / b;
const max = (a        , b        )         => (a > b ? a : b);

export const CLAIM_GAS_MARGIN_NUM = 3n;
export const CLAIM_GAS_MARGIN_DEN = 2n;

const gasCost = (c              , units        )         => (units <= c.flatGasLimit ? c.flatGasPrice : c.flatGasPrice + (units - c.flatGasLimit) * c.gasPrice);
const fwdNoLump = (c              , bits        , cells        )         => ceilDiv(c.bitPrice * bits + c.cellPrice * cells, 65536n);
const fwdCost = (c              , bits        , cells        )         => c.lumpPrice + fwdNoLump(c, bits, cells);
const rentCost = (c              , seconds        , bits        , cells        )         => ceilDiv((bits * c.storageBitPs + cells * c.storageCellPs) * seconds, 65536n);

export function hopFwd(c               = TON_MAINNET_GAS_CONFIG)         { return fwdCost(c, K.HOP_MSG_BITS, K.HOP_MSG_CELLS); }
export function bookValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.BOOK_UNITS) + rentCost(c, K.RENT_SECONDS, K.ACCOUNT_STATE_BITS, K.ACCOUNT_STATE_CELLS);
}
export function deltaCost(c               = TON_MAINNET_GAS_CONFIG)         {
  return fwdCost(c, K.DELTA_MSG_BITS, K.DELTA_MSG_CELLS) + gasCost(c, K.LAUNCH_DELTA_GAS) + fwdCost(c, K.INDEX_MSG_BITS, K.INDEX_MSG_CELLS) + gasCost(c, K.WALLET_INDEX_GAS);
}
export function jettonValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.QUOTE_HOP_UNITS) + 3n * hopFwd(c) + fwdCost(c, K.QUOTE_STATE_BITS, K.QUOTE_STATE_CELLS)
    + rentCost(c, K.QUOTE_RENT_SECONDS, K.QUOTE_STATE_BITS, K.QUOTE_STATE_CELLS);
}
export function tokensValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return 3n * hopFwd(c) + fwdNoLump(c, K.INIT_STATE_BITS, K.INIT_STATE_CELLS) + gasCost(c, K.WALLET_SEND_GAS) + gasCost(c, K.WALLET_RECEIVE_GAS)
    + 2n * deltaCost(c) + rentCost(c, K.MIN_STORAGE_SECONDS, K.WALLET_STORAGE_BITS, K.WALLET_STORAGE_CELLS);
}
export function onwardValue(c               = TON_MAINNET_GAS_CONFIG)         { return max(jettonValue(c), tokensValue(c)); }
export function claimFwdValue(c               = TON_MAINNET_GAS_CONFIG)         {
  const onward = onwardValue(c);
  const payoutFwd = gasCost(c, K.PAYOUT_NOTIFY_UNITS) + hopFwd(c) + onward;
  return gasCost(c, K.LAUNCH_CLAIM_UNITS) + onward + hopFwd(c) + max(payoutFwd, K.GAS_PAYOUT_FWD_MIN) + bookValue(c);
}
export function claimMinValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.CLAIM_UNITS) + 2n * hopFwd(c) + claimFwdValue(c);
}

export function claimMinValueOf(relay         , pays         , c               = TON_MAINNET_GAS_CONFIG)         {
  return relay && pays ? claimMinValue(c) + claimFwdValue(c) + hopFwd(c) : claimMinValue(c);
}

export function accountReserve(c               = TON_MAINNET_GAS_CONFIG)         {
  return rentCost(c, K.RENT_SECONDS, K.ACCOUNT_STATE_BITS, K.ACCOUNT_STATE_CELLS);
}

export function bookSendValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return bookValue(c) + fwdCost(c, K.HOP_MSG_BITS + K.ACCOUNT_STATE_BITS, K.HOP_MSG_CELLS + K.ACCOUNT_STATE_CELLS);
}

export function reclaimMinValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.CLAIM_UNITS) + 2n * hopFwd(c) + gasCost(c, K.QUOTE_HOP_UNITS)
    + max(onwardValue(c) + hopFwd(c), gasCost(c, K.LAUNCH_CLAIM_UNITS) + hopFwd(c) + bookSendValue(c)) + accountReserve(c);
}

const HOLDER_BIT = 512;
const withMargin = (v        )         => ceilDiv(v * CLAIM_GAS_MARGIN_NUM, CLAIM_GAS_MARGIN_DEN);

export function defaultClaimValue(types        , c               = TON_MAINNET_GAS_CONFIG)         {
  return withMargin(claimMinValueOf((types & HOLDER_BIT) !== 0, (types & ~HOLDER_BIT) !== 0, c));
}

export function defaultReclaimValue(c               = TON_MAINNET_GAS_CONFIG)         { return withMargin(reclaimMinValue(c)); }

export function teamShareBuyMinValue(referred         , thenClaim         , c               = TON_MAINNET_GAS_CONFIG)         {
  const books = referred ? K.TEAM_SHARE_REF_BOOKS : K.TEAM_SHARE_BOOKS;
  const need = gasCost(c, K.TRADE_UNITS) + books * bookSendValue(c) + hopFwd(c) + (thenClaim ? tokensValue(c) : 0n);
  return max(need, K.GAS_BUY_FWD_MIN) + (referred ? traderMinValue(c) : 0n);
}

export function defaultTeamShareBuyValue(referred         , thenClaim         , c               = TON_MAINNET_GAS_CONFIG)         {
  return withMargin(teamShareBuyMinValue(referred, thenClaim, c));
}

export function traderMinValue(c               = TON_MAINNET_GAS_CONFIG)         { return gasCost(c, K.TRADER_UNITS) + hopFwd(c); }

export function traderDeployValue(c               = TON_MAINNET_GAS_CONFIG)         { return gasCost(c, K.TRADER_UNITS) + accountReserve(c); }

export function traderDeploySend(c               = TON_MAINNET_GAS_CONFIG)         {
  return traderDeployValue(c) + fwdCost(c, K.HOP_MSG_BITS + K.ACCOUNT_STATE_BITS, K.HOP_MSG_CELLS + K.ACCOUNT_STATE_CELLS);
}

export function fallbackMinValue(c               = TON_MAINNET_GAS_CONFIG)         { return 2n * traderMinValue(c) + traderDeploySend(c); }

export function refChainValue(c               = TON_MAINNET_GAS_CONFIG)         { return 2n * traderMinValue(c); }

export function burnNeed(origFwd        , c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.WALLET_SEND_GAS) + origFwd + gasCost(c, K.LAUNCH_BURN_GAS) + deltaCost(c);
}

export function transferBase(c               = TON_MAINNET_GAS_CONFIG)         { return tokensValue(c) - 3n * hopFwd(c); }

export function transferNeed(forwardTon        , fwdCount        , origFwd        , c               = TON_MAINNET_GAS_CONFIG)         {
  return forwardTon + fwdCount * origFwd + transferBase(c);
}

export function sellReturnMinFrom(origFwd        , c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.LAUNCH_SELL_RETURN_GAS) + 3n * origFwd + transferBase(c);
}

export function sellReturnMin(c               = TON_MAINNET_GAS_CONFIG)         {
  return sellReturnMinFrom(fwdCost(c, K.SELL_MSG_BITS, K.SELL_MSG_CELLS), c);
}

export function teamBurnValue(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.WALLET_SEND_GAS + K.LAUNCH_BURN_GAS) + 2n * hopFwd(c);
}

export function launchReserve(c               = TON_MAINNET_GAS_CONFIG)         {
  return rentCost(c, K.RENT_SECONDS, K.LAUNCH_STATE_BITS, K.LAUNCH_STATE_CELLS);
}

export function createGas(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.CREATE_UNITS) + 2n * fwdCost(c, K.LAUNCH_STATE_BITS, K.LAUNCH_STATE_CELLS);
}

export function launchSetupMin(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.SETUP_UNITS) + 2n * jettonValue(c) + launchReserve(c);
}

export function createMinValue(createFee        , teamBuyMoney        , c               = TON_MAINNET_GAS_CONFIG)         {
  return createFee + teamBuyMoney + createGas(c) + launchSetupMin(c);
}

export function pushEach(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.UPGRADE_UNITS) + hopFwd(c) + fwdCost(c, K.LAUNCH_STATE_BITS, K.LAUNCH_STATE_CELLS);
}

export function childEach(c               = TON_MAINNET_GAS_CONFIG)         {
  return gasCost(c, K.CHILD_UPGRADE_UNITS) + hopFwd(c) + fwdCost(c, K.ACCOUNT_STATE_BITS, K.ACCOUNT_STATE_CELLS);
}

export function pushChildMin(family       , n        , c               = TON_MAINNET_GAS_CONFIG)         {
  if (family !== 3 && family !== 4) throw new RangeError("family: 3 (account) or 4 (trader)");
  if (!Number.isInteger(n) || n < 1 || n > 32) throw new RangeError("owners: 1..32");
  const factoryGas = gasCost(c, K.FACTORY_OP_UNITS) + hopFwd(c);
  const each = BigInt(n) * childEach(c);
  return family === 4 ? each + factoryGas : each + gasCost(c, K.FAN_UNITS) + pushEach(c) + factoryGas;
}
