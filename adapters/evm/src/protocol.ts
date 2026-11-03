

import { EV_DATA, FN, LAUNCH_CREATED_LISTED, NO_DEADLINE, OUT, TOPIC, calldata, decode, topicAddress, topicUint } from "./abi.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import { EVM_EVENT_SCHEMAS } from "../vendor/sdk/src/abi/evm.gen.ts";

export type EthCall = (to: string, data: string, blockNumber: number | "latest") => Promise<string>;

export type LaunchMeta = {
  id: bigint;
  token: string;

  quote: string;
  createdBlock: number;

  listed: boolean;
};

export interface Log {
  address: string;
  topics: string[];
  data: string;
  blockNumber: number;
  transactionHash?: string;
  logIndex?: number;
}

export type GetLogs = (filter: { address: string; topic0: string; fromBlock: number; toBlock: number | "latest" }) => Promise<Log[]>;

export const NATIVE = "0x0000000000000000000000000000000000000000";
export const ETH_PLACEHOLDER = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
export const toChainQuote = (a: string): string => (a.toLowerCase() === ETH_PLACEHOLDER ? NATIVE : a.toLowerCase());
export const toApiToken = (a: string): string => (a.toLowerCase() === NATIVE ? ETH_PLACEHOLDER : a.toLowerCase());

export const GAS = { buy: 700_000, sell: 750_000 } as const;
export { gasFor, isDustRefund } from "../vendor/sdk/src/v2/router.ts";

export const QUOTE_WRITTEN_TOPIC: string = (() => {
  const e = EVM_EVENT_SCHEMAS.find((x) => x.name === "QuoteWritten");
  if (!e) throw new Error("no generated event schema QuoteWritten");
  return e.topic;
})();

export async function routable(call: EthCall, floorAmm: string, meta: LaunchMeta, blockNumber: number | "latest", routeUnlisted = false): Promise<boolean> {
  if (meta.listed || routeUnlisted) return true;
  const [entry] = decode(OUT.quoteEntry, await call(floorAmm, calldata(FN.quoteEntry, [meta.quote]), blockNumber)) as AbiValue[][];
  return entry[0] === true;
}

export function launchFromLog(floorAmm: string, log: Log): LaunchMeta | null {
  if (log.address.toLowerCase() !== floorAmm.toLowerCase() || log.topics[0] !== TOPIC.LaunchCreated) return null;
  const data = decode(EV_DATA.LaunchCreated, log.data);
  return { id: topicUint(log.topics[1]), token: topicAddress(log.topics[2]), quote: topicAddress(log.topics[3]), createdBlock: log.blockNumber, listed: data[LAUNCH_CREATED_LISTED] === true };
}

export async function onChainQuote(call: EthCall, floorAmm: string, id: bigint, isBuy: boolean, amount: bigint, blockNumber: number | "latest", to: string = NATIVE): Promise<[bigint, bigint, boolean]> {
  if (isBuy) {
    const [r] = decode(OUT.quoteBuy, await call(floorAmm, calldata(FN.quoteBuy, [id, amount, to]), blockNumber)) as AbiValue[][];
    return [r[0] as bigint, r[2] as bigint, r[6] === true];
  }
  const [q] = decode(OUT.quoteSell, await call(floorAmm, calldata(FN.quoteSell, [id, amount, to]), blockNumber)) as bigint[];
  return [q, 0n, false];
}

export type SwapMode = "direct" | "router";

export function swapCall(p: { floorAmm: string; id: bigint; isBuy: boolean; ethIn: boolean; amountIn: bigint; minOut: bigint; to: string; mode: SwapMode; deadline?: bigint }): {
  to: string;
  data: string;
  value: bigint;
  approve: string | null;
  transferFirst: boolean;
} {
  const { floorAmm, id, isBuy, ethIn, amountIn, minOut, to, mode } = p;
  const deadline = p.deadline ?? NO_DEADLINE;
  if (isBuy && ethIn) return { to: floorAmm, data: calldata(FN.buy, [id, amountIn, minOut, to, deadline]), value: amountIn, approve: null, transferFirst: false };
  if (mode === "router") {
    const data = isBuy ? calldata(FN.buyFromBalance, [id, minOut, to, to, deadline]) : calldata(FN.sellFromBalance, [id, minOut, to, deadline]);
    return { to: floorAmm, data, value: 0n, approve: null, transferFirst: true };
  }
  return { to: floorAmm, data: calldata(isBuy ? FN.buy : FN.sell, [id, amountIn, minOut, to, deadline]), value: 0n, approve: floorAmm, transferFirst: false };
}

export { MAX_SLIPPAGE_BPS, minOutAt as minOutFor } from "../vendor/sdk/src/v2/press.ts";
