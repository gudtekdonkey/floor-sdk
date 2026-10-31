

import { checksumAddress, decodeAbi, encodeAbi, encodeCall, bytesToHex, functionSelector, hexToBytes } from "./abi.js";

export const MULTICALL3_ADDRESS = "0xcA11bde05977b3631167028862bE2a173976CA11";

const AGGREGATE3 = "aggregate3((address,bool,bytes)[])";
const RESULTS = ["(bool,bytes)[]"]         ;

export const AGGREGATE3_SELECTOR = bytesToHex(functionSelector(AGGREGATE3));

export function aggregate3Call(calls                  , multicall         = MULTICALL3_ADDRESS)          {
  const data = encodeCall(AGGREGATE3, [calls.map((c)           => [checksumAddress(c.target), c.allowFailure, c.callData])]);
  return { to: checksumAddress(multicall), data: bytesToHex(data), value: 0n };
}

export function decodeAggregate3Result(data        )                {
  const [rows] = decodeAbi(RESULTS, data)                         ;
  return rows.map(([success, returnData]) => ({ success, returnData }));
}

export function decodeAggregate3Call(data        )          {
  const b = hexToBytes(data);
  if (bytesToHex(b.slice(0, 4)) !== AGGREGATE3_SELECTOR) throw new RangeError("not aggregate3 calldata");
  const [rows] = decodeAbi(["(address,bool,bytes)[]"], b.slice(4))                                 ;
  return rows.map(([target, allowFailure, callData]) => ({ target, allowFailure, callData }));
}

export const encodeAggregate3Result = (results                        )         =>
  bytesToHex(encodeAbi(RESULTS, [results.map((r)           => [r.success, r.returnData])]));
