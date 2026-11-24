

import { bytesToHex, decodeAbi, encodeCall } from "../evm/abi.js";

import { decodeTeamShareOf, decodeTeamSlotOf, viewCall } from "./evm.js";

import { EVM_TOKEN_SCALE } from "./market-evm.js";
import { evmSignature } from "../generated/index.js";

export const LAUNCH_STATE_VIEW_EVM = {
  sig: evmSignature("launchState"),
  ret: ["(address,address,address,uint8,uint32,bytes32,bool,uint64,uint64,uint64,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint16)"],
}         ;

const KEYS = [
  "token", "quote", "creator", "presetId", "presetVersion", "presetHash", "graduated", "createdAt", "createdBlock",
  "creatorUnlockAt", "supply", "sOut", "creatorLocked", "floor", "cushion", "p0X64", "priceX64", "floorX64", "slotRateBps",
]         ;
const NUMBERS = new Set        (["presetId", "presetVersion", "slotRateBps"]);

export const launchStateCallV2 = (launchId        )         => bytesToHex(encodeCall(LAUNCH_STATE_VIEW_EVM.sig, [launchId]));

export function decodeLaunchStateV2(data                     )                   {
  const [t] = decodeAbi(LAUNCH_STATE_VIEW_EVM.ret, data)                ;
  const o                          = {};
  KEYS.forEach((k, i) => { o[k] = NUMBERS.has(k) ? Number(t[i]          ) : t[i]; });
  return o                               ;
}

                        

const PLACES = 10;
const ZERO_ADDRESS = "0x" + "00".repeat(20);
const ZERO_WORD = "0x" + "00".repeat(32);

function checkSource(s                     )       {
  for (const k of ["members", "handles", "share", "weightPpb", "teamBuyPart", "slotOwed", "slotBoughtOwed", "wallets"]         ) {
    if (s[k].length !== PLACES) throw new RangeError(`TeamSlotSourceEvmV2.${k}: ${PLACES} places, got ${s[k].length}`);
  }
}

export function teamBuyOwedEvmV2(v                  , opts                                               = {})                   {
  const now = opts.now ?? BigInt(Math.floor(Date.now() / 1000));
  if (opts.slot) checkSource(opts.slot);
  return {
    locked: v.creatorLocked, unlockAt: v.creatorUnlockAt, unlocked: now >= v.creatorUnlockAt,
    perPlace: opts.slot ? opts.slot.teamBuyPart.map((x) => x * EVM_TOKEN_SCALE) : null,
  };
}

export async function readTeamSlotEvmV2(
  rpc                                                            ,
  p                                                                                              ,
)                         {
  const [state, slot, share] = await Promise.all([
    rpc.ethCall(p.amm, launchStateCallV2(p.launchId)),
    p.slot ? Promise.resolve(null) : rpc.ethCall(p.amm, viewCall("teamSlotOf", [p.launchId])),
    p.wallet === undefined ? Promise.resolve(null) : rpc.ethCall(p.amm, viewCall("teamShareOf", [p.launchId, p.wallet])),
  ]);
  const v = decodeLaunchStateV2(state);
  const s = p.slot ?? decodeTeamSlotOf(slot );
  checkSource(s);
  const addr = (x        )                => (x.toLowerCase() === ZERO_ADDRESS ? null : x);
  const places = Array.from({ length: PLACES }, (_, i)                     => {
    const w = addr(s.wallets[i]) ?? addr(s.members[i]);
    return {
      place: i, wallet: i === 0 ? w ?? v.creator : w, handle: s.handles[i].toLowerCase() === ZERO_WORD ? null : s.handles[i],
      share: s.share[i], weightPpb: s.weightPpb[i], active: s.share[i] > 0n, teamShareOwed: s.slotOwed[i],
      boughtOwed: s.slotBoughtOwed[i] * EVM_TOKEN_SCALE, teamBuyPart: s.teamBuyPart[i] * EVM_TOKEN_SCALE,
    };
  });
  return {
    rateBps: v.slotRateBps, creator: v.creator, teamBuy: teamBuyOwedEvmV2(v, { now: p.now, slot: s }),
    wallet: share === null ? null : decodeTeamShareOf(share), places,
  };
}
