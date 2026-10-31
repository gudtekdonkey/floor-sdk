

import { MEMBERS_MAX as MAX_MEMBERS } from "../../v2/layouts.js";
import { checksumAddress } from "../abi.js";
import { encodeFunctionData } from "../items.js";
import { FLOOR_AMM_ABI, MEMBER_WALLET, NATIVE_QUOTE } from "./iface.js";
import { approveCall, createLaunchCall, createQuoteTotal, decodeFloorAmmResult, floorAmmCall, teamSlotAddCall } from "./calls.js";

export const launchCountCall = (floorAmm        )          => ({ to: checksumAddress(floorAmm), data: encodeFunctionData(FLOOR_AMM_ABI, "launchCount", {}), value: 0n });
export const decodeLaunchCount = (data        )         => (decodeFloorAmmResult("launchCount", data)                           )["0"]          ;

export function teamSlotCalls(p                                                                                            )            {
  if (p.members.length > MAX_MEMBERS) throw new RangeError(`a Team Slot has at most ${MAX_MEMBERS} members (R103)`);
  return [
    ...p.members.map((m) => teamSlotAddCall({ floorAmm: p.floorAmm, launchId: p.launchId, key: m.key, kind: m.kind ?? MEMBER_WALLET, amount: m.amount, payNow: m.payNow })),
  ];
}

export function planCreateWithSlot(p

                                                     

                                                                            

 )                                                             {
  const launchId = p.launchCount + 1n;
  if (!Number.isInteger(p.slot.rateBps) || p.slot.rateBps < 0) throw new RangeError(`the Team Slot's rate is bps, 0 ..= the row's max: ${p.slot.rateBps}`);
  const quoteTotal = createQuoteTotal(p);
  const native = p.params.quote.toLowerCase() === NATIVE_QUOTE;
  const calls = [
    ...(p.approve && !native ? [approveCall(p.params.quote, p.floorAmm, quoteTotal)] : []),
    createLaunchCall({ floorAmm: p.floorAmm, createFee: p.createFee, params: { ...p.params, teamSlotRateBps: BigInt(p.slot.rateBps) }, registry: p.registry, nativeCreateFee: p.nativeCreateFee }),
    ...teamSlotCalls({ floorAmm: p.floorAmm, quote: p.params.quote, launchId, members: p.slot.members }),
  ];
  return { launchId, calls, quoteTotal };
}

export function eip5792Batch(p                                                                                        )

  {
  const hex = (v        )         => "0x" + v.toString(16);
  return {
    version: "2.0.0",
    chainId: hex(p.chainId),
    from: checksumAddress(p.from),
    atomicRequired: p.atomicRequired ?? true,
    calls: p.calls.map((c) => ({ to: c.to, data: c.data, value: hex(c.value) })),
  };
}
