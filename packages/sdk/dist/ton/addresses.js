

import { Address, beginCell, contractAddress, EMPTY_CELL } from "./cell.js";

export function addr(a             )          {
  return typeof a === "string" ? Address.parse(a) : a;
}

export const factoryData = (authority             , salt        )       => beginCell().storeAddress(addr(authority)).storeUint(salt, 64).endCell();
export const launchData = (factory             , id        )       => beginCell().storeAddress(addr(factory)).storeUint(id, 64).endCell();

export const walletData = (owner             , master             , position       = EMPTY_CELL)       =>
  beginCell().storeCoins(0).storeAddress(addr(owner)).storeAddress(addr(master)).storeRef(position).endCell();
export const accountData = (launch             , owner             )       => beginCell().storeAddress(addr(launch)).storeAddress(addr(owner)).endCell();
export const traderData = (factory             , trader             )       => beginCell().storeAddress(addr(factory)).storeAddress(addr(trader)).endCell();

export function factoryAddress(codes                           , authority             , salt        )          {
  if (!codes.factory) throw new RangeError("factory init code not supplied");
  return contractAddress(codes.factory, factoryData(authority, salt));
}

export function launchAddress(chain                                     , id        )          {
  if (id < 1n) throw new RangeError("launch ids start at 1");
  if (!chain.codes.launch) throw new RangeError("launch init code not supplied: read the address with getLaunchAddress, or pass codes.launch");
  return contractAddress(chain.codes.launch, launchData(chain.factory, id));
}

export function launchWalletAddress(chain                         , launch             , owner             , position       )          {
  return contractAddress(chain.codes.wallet, walletData(owner, launch, position));
}
export function accountAddress(chain                         , launch             , owner             )          {
  return contractAddress(chain.codes.account, accountData(launch, owner));
}
export function traderAddress(chain                                     , trader             )          {
  return contractAddress(chain.codes.trader, traderData(chain.factory, trader));
}
