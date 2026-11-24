

import { bytesToHex, hexToBytes } from "../evm/abi.js";
import { keccak256 } from "../evm/keccak.js";

export function evmCodeHash(code        )         {
  if (typeof code !== "string" || !/^0x([0-9a-fA-F]{2})*$/.test(code) || /^0x0*$/.test(code)) throw new Error(`evmCodeHash: not runtime code: ${String(code).slice(0, 20)}`);
  return bytesToHex(keccak256(hexToBytes(code))).toLowerCase();
}
