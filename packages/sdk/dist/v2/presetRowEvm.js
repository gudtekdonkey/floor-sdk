

import { bytesToHex, decodeAbi, encodeCall, hexToBytes } from "../evm/abi.js";

import { ROW_LEN, decodeRowV2 } from "../math/v2/preset.js";

import { evmSignature } from "../generated/index.js";

export const PRESET_ROW_VIEW_EVM = { sig: evmSignature("presetRow"), ret: ["(uint8,bool,bool,uint32,bytes32,bytes)"] }         ;
export const NEXT_PRESET_ID_VIEW_EVM = { sig: evmSignature("nextPresetId"), ret: ["uint8"] }         ;

const RESERVED_ID = 0;
const RETIRED_ID = 2;

export function presetRowCallV2(id        )         {
  if (!Number.isInteger(id) || id < 0 || id > 255) throw new RangeError(`preset id ${id}: a u8`);
  return bytesToHex(encodeCall(PRESET_ROW_VIEW_EVM.sig, [BigInt(id)]));
}
export const nextPresetIdCallV2 = ()         => bytesToHex(encodeCall(NEXT_PRESET_ID_VIEW_EVM.sig, []));
export const decodeNextPresetIdEvmV2 = (data                     )         => Number(decodeAbi(NEXT_PRESET_ID_VIEW_EVM.ret, data)[0]          );

export function decodePresetRowEvmV2(data                     )                            {
  const [t] = decodeAbi(PRESET_ROW_VIEW_EVM.ret, data)                ;
  const spec = hexToBytes(t[5]          );
  if (spec.length === 0) return null;
  if (spec.length !== ROW_LEN) throw new RangeError(`preset row spec ${spec.length} bytes, want ${ROW_LEN}`);
  return {
    id: Number(t[0]          ), bump: 0, checked: 0, genesisRow: 0, padVersion: new Uint8Array(2), enabled: t[1] ? 1 : 0, deleted: t[2] ? 1 : 0, version: Number(t[3]          ),
    rowHash: hexToBytes(t[4]          ), row: spec, padEnd: new Uint8Array(0),
  };
}

export function presetMenuV2(rows                                        )                     {
  return rows
    .filter((r)                          => r !== null && r.enabled !== 0 && r.deleted === 0 && r.id !== RESERVED_ID && r.id !== RETIRED_ID)
    .sort((a, b) => a.id - b.id)
    .map((r) => ({ id: r.id, version: r.version, rowHash: bytesToHex(r.rowHash), row: decodeRowV2(r.row) }));
}

export async function readPresetMenuEvmV2(rpc                                                            , amm        )                              {
  const next = decodeNextPresetIdEvmV2(await rpc.ethCall(amm, nextPresetIdCallV2()));
  const ids = Array.from({ length: Math.max(0, next - 1) }, (_, i) => i + 1).filter((id) => id !== RETIRED_ID);
  const rows = await Promise.all(ids.map(async (id) => decodePresetRowEvmV2(await rpc.ethCall(amm, presetRowCallV2(id)))));
  return presetMenuV2(rows);
}
