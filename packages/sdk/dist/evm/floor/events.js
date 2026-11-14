

import { checksumAddress } from "../abi.js";
import { decodeEventLog, itemTopic } from "../items.js";

import { LEGACY_CLAIMED_TOPIC, LEGACY_EVENTS, normalizeLegacyClaimed } from "./legacy-events.js";
import { FLOOR_AMM_ABI, FLOOR_AMM_UNFROZEN_EVENTS, FLOOR_ZAP_ABI } from "./iface.js";

       

const ALL = [...FLOOR_AMM_ABI, ...FLOOR_ZAP_ABI, ...FLOOR_AMM_UNFROZEN_EVENTS];

let unfrozen                         ;
const UNFROZEN = ()              => (unfrozen ??= new Set(FLOOR_AMM_UNFROZEN_EVENTS.map((e) => itemTopic(e))));

export const FLOOR_EVENT_TOPICS                         = {};
for (const i of ALL.filter((i) => i.type === "event").reverse()) {
  const name = i.name;
  const first = ALL.find((j) => j.type === "event" && j.name === name) ;
  if (!(name in FLOOR_EVENT_TOPICS))
    Object.defineProperty(FLOOR_EVENT_TOPICS, name, {
      enumerable: true,
      configurable: true,
      get: () => {
        const value = itemTopic(first);
        Object.defineProperty(FLOOR_EVENT_TOPICS, name, { value, enumerable: true, configurable: true, writable: true });
        return value;
      },
    });
}

export const FLOOR_LEGACY_EVENT_TOPICS                         = { Claimed: LEGACY_CLAIMED_TOPIC };

export function decodeFloorLog(log     , from                    )                       {
  if (from && (!log.address || !from.some((a) => a.toLowerCase() === log.address .toLowerCase()))) return null;
  const d                       = decodeEventLog(ALL, log);
  if (!d) {
    const old = decodeEventLog(LEGACY_EVENTS, log);
    if (!old) return null;
    const e                = { ...old, args: normalizeLegacyClaimed(old.args), legacy: true };
    return log.address ? { ...e, address: checksumAddress(log.address) } : e;
  }
  if (UNFROZEN().has((log.topics[0] ?? "").toLowerCase())) d.unfrozen = true;
  return log.address ? { ...d, address: checksumAddress(log.address) } : d;
}

export function decodeFloorLogs(logs                , from                    )                  {
  return logs.map((l) => decodeFloorLog(l, from)).filter((e)                     => e !== null);
}
