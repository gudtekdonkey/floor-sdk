/**
 * Event decoders for every FLOOR contract on EVM chains: the twin of `@floor/sdk-solana`'s `events.ts`.
 *
 * A log is identified by its first topic, the event's selector. Across all of this package's ABIs each selector names
 * exactly one event layout (the tests hold that), so one table decodes any FLOOR log without knowing which contract
 * emitted it. `abis` names the ABIs that declare the event; an event several contracts emit lists them all.
 */
import { decodeEventLog, toEventSelector, type AbiEvent, type Address, type Hex } from 'viem';
import * as ABIS from './abi/index.js';

export interface FloorEventInfo {
  readonly event: AbiEvent;
  /** The exported ABI constants that declare this event (`LAUNCH_ABI`, `POOL_ABI`, …). */
  readonly abis: readonly string[];
}

/** The minimal log shape: what an `eth_getLogs` entry or a receipt log carries. */
export interface FloorLogInput {
  readonly topics: readonly Hex[];
  readonly data: Hex;
  readonly address?: Address;
  readonly blockNumber?: bigint | null;
  readonly transactionHash?: Hex | null;
  readonly logIndex?: number | null;
}

export interface DecodedFloorEvent {
  readonly name: string;
  readonly args: Readonly<Record<string, unknown>>;
  readonly topic: Hex;
  readonly abis: readonly string[];
  readonly address?: Address;
  readonly blockNumber?: bigint | null;
  readonly transactionHash?: Hex | null;
  readonly logIndex?: number | null;
}

function buildTable(): Map<Hex, FloorEventInfo> {
  const table = new Map<Hex, { event: AbiEvent; abis: string[] }>();
  for (const [name, abi] of Object.entries(ABIS)) {
    if (!Array.isArray(abi)) continue;
    for (const item of abi as readonly { type: string }[]) {
      if (item.type !== 'event') continue;
      const event = item as AbiEvent;
      const topic = toEventSelector(event);
      const entry = table.get(topic);
      if (entry) {
        if (!entry.abis.includes(name)) entry.abis.push(name);
      } else {
        table.set(topic, { event, abis: [name] });
      }
    }
  }
  return table;
}

/** Every FLOOR event, keyed by its selector (the log's first topic). */
export const FLOOR_EVENTS: ReadonlyMap<Hex, FloorEventInfo> = buildTable();

/** The selectors of every FLOOR event with this name (`Claimed` has two). Empty for a name no ABI declares. */
export function floorEventTopics(name: string): Hex[] {
  return [...FLOOR_EVENTS].filter(([, info]) => info.event.name === name).map(([topic]) => topic);
}

/**
 * Decode one log. `null` when its first topic is not a FLOOR event, or when the topics and data do not fit the event's
 * layout (a log with the same selector from some other contract, or a truncated one): a wrong log is never decoded into
 * plausible values.
 */
export function decodeFloorLog(log: FloorLogInput): DecodedFloorEvent | null {
  const topic = log.topics[0];
  if (!topic) return null;
  const info = FLOOR_EVENTS.get(topic.toLowerCase() as Hex);
  if (!info) return null;
  let decoded: { eventName: string; args: unknown };
  try {
    decoded = decodeEventLog({ abi: [info.event], topics: log.topics as [Hex, ...Hex[]], data: log.data, strict: true });
  } catch {
    return null;
  }
  return {
    name: decoded.eventName,
    args: (decoded.args ?? {}) as Record<string, unknown>,
    topic,
    abis: info.abis,
    address: log.address,
    blockNumber: log.blockNumber,
    transactionHash: log.transactionHash,
    logIndex: log.logIndex,
  };
}

/** Decode every FLOOR log in a list (a receipt's logs, an `eth_getLogs` page), in order, skipping the rest. */
export function decodeFloorLogs(logs: readonly FloorLogInput[]): DecodedFloorEvent[] {
  const out: DecodedFloorEvent[] = [];
  for (const log of logs) {
    const decoded = decodeFloorLog(log);
    if (decoded) out.push(decoded);
  }
  return out;
}
