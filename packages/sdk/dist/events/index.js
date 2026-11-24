

import { decodeEventData } from "../solana/floor/events.js";
import { decodeEventDataV2 } from "../v2/events.js";
import { decodeLogV2, V2_EVENTS } from "../v2/evm.js";
import { decodeFloorLog } from "../evm/floor/events.js";
import { checksumAddress } from "../evm/abi.js";

import { hex } from "../codec.js";
import { EVENT_IX_TAG_LE } from "../solana/instruction.js";

export { SHARED_EVENT_NAMES } from "./types.gen.js";
export * from "./types.gen.js";

export function decodeSolanaEvent(data            )                     {
  const d = decodeEventDataV2(data) ?? decodeEventData(data);
  return d ? ({ chain: "solana", name: d.name, fields: d.fields }               ) : null;
}

export function decodeSolanaEventCpi(ixData            )                     {
  if (ixData.length < 16 || hex(ixData.slice(0, 8)) !== hex(EVENT_IX_TAG_LE)) return null;
  return decodeSolanaEvent(ixData.slice(8));
}

export function decodeEvmLog(log     , from                    )                  {
  if (from && (!log.address || !from.some((a) => a.toLowerCase() === log.address .toLowerCase()))) return null;
  const v2 = decodeLogV2(log);
  const address = log.address ? checksumAddress(log.address) : undefined;
  if (v2) return { chain: "evm", name: v2.name, fields: v2.fields, ...(address ? { address } : {}) }                       ;
  const d = decodeFloorLog(log);
  if (!d) return null;

  const name = d.name in V2_EVENTS ? `${d.name}V1` : d.name;
  return { chain: "evm", name, fields: d.args, ...(address ? { address } : {}) }                       ;
}

export const decodeEvmLogs = (logs                , from                    )             =>
  logs.map((l) => decodeEvmLog(l, from)).filter((e)                => e !== null);

export const isEvent =                   (e            , name   )                                        => e.name === name;
