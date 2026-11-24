

import { checksumAddress, decodeAbi, encodeCall, bytesToHex } from "./abi.js";
import { MULTICALL3_ADDRESS, aggregate3Call, decodeAggregate3Result } from "./multicall.js";

import { evmSignature } from "../generated/index.js";

const BALANCE_OF = evmSignature("balanceOf");

export const balanceOfData = (owner        )         => bytesToHex(encodeCall(BALANCE_OF, [checksumAddress(owner)]));

export function balancesCall(owner        , tokens                   , multicall         = MULTICALL3_ADDRESS)          {
  const data = balanceOfData(owner);
  return aggregate3Call(tokens.map((t) => ({ target: t, allowFailure: true, callData: data })), multicall);
}

export function holdersBalanceCall(token        , owners                   , multicall         = MULTICALL3_ADDRESS)          {
  return aggregate3Call(owners.map((o) => ({ target: token, allowFailure: true, callData: balanceOfData(o) })), multicall);
}

export function decodeBalances(data        )                    {
  return decodeAggregate3Result(data).map((r) => {
    if (!r.success || r.returnData.length < 2 + 64) return null;
    return (decodeAbi(["uint256"], r.returnData)            )[0];
  });
}
