

import { FN, OUT, TOPIC, calldata, decode } from "./abi.ts";
import { launchFromLog, toApiToken } from "./protocol.ts";
import { MULTICALL3_ADDRESS, aggregate3Call, decodeAggregate3Result } from "../vendor/sdk/src/evm/multicall.ts";
import type { EthCall, LaunchMeta, Log } from "./protocol.ts";

export type TokenListEntry = {
  chainId: number;
  address: string;
  name: string;
  symbol: string;
  decimals: 18;
  tags: string[];
  extensions: { launchId: string; quote: string; floorAmm: string };
};

export type TokenList = {
  name: string;
  timestamp: string;
  version: { major: number; minor: number; patch: number };
  tags: Record<string, { name: string; description: string }>;
  tokens: TokenListEntry[];
};

const SYMBOL_OK = /^\S{1,20}$/;

export type GetLogs = (f: { address: string; topic0: string; fromBlock: number; toBlock: number | "latest" }) => Promise<Log[]>;

const NAME_MAX = 40;

export const TOKEN_LIST_PER_READ = 50;

function entryOf(chainId: number, floorAmm: string, m: LaunchMeta, nameRet: string, symbolRet: string): TokenListEntry | string {
  const [name] = decode(OUT.name, nameRet) as string[];
  const [symbol] = decode(OUT.symbol, symbolRet) as string[];
  if (name.length === 0 || name.length > NAME_MAX) return `name ${JSON.stringify(name)}`;
  if (!SYMBOL_OK.test(symbol)) return `symbol ${JSON.stringify(symbol)}`;
  return { chainId, address: m.token, name, symbol, decimals: 18, tags: ["floor-amm"], extensions: { launchId: m.id.toString(), quote: toApiToken(m.quote), floorAmm: floorAmm.toLowerCase() } };
}

async function namesOf(call: EthCall, ms: LaunchMeta[], multicall: string | null): Promise<[string, string][]> {
  const nameData = calldata(FN.name, []), symbolData = calldata(FN.symbol, []);
  if (multicall === null) {
    const out: [string, string][] = [];
    for (const m of ms) out.push([await call(m.token, nameData, "latest"), await call(m.token, symbolData, "latest")]);
    return out;
  }
  const chunks: LaunchMeta[][] = [];
  for (let i = 0; i < ms.length; i += TOKEN_LIST_PER_READ) chunks.push(ms.slice(i, i + TOKEN_LIST_PER_READ));
  const got = await Promise.all(chunks.map(async (chunk) => {
    const c = aggregate3Call(chunk.flatMap((m) => [{ target: m.token, allowFailure: true, callData: nameData }, { target: m.token, allowFailure: true, callData: symbolData }]), multicall);
    const r = decodeAggregate3Result(await call(c.to, c.data, "latest"));
    if (r.length !== chunk.length * 2) throw new RangeError(`aggregate3: ${r.length} results, ${chunk.length * 2} calls`);
    return chunk.map((m, i): [string, string] => {
      const [a, b] = [r[2 * i]!, r[2 * i + 1]!];
      if (!a.success || !b.success) throw new Error(`token ${m.token}: name() / symbol() reverted`);
      return [a.returnData, b.returnData];
    });
  }));
  return got.flat();
}

function listOf(timestamp: string, version: TokenList["version"], tokens: TokenListEntry[]): TokenList {
  return {
    name: "FLOOR Program launches",
    timestamp,
    version,
    tags: { "floor-amm": { name: "FLOOR Program", description: "Trades against the FLOOR Program (FloorAmm); backed by the floor its pots hold" } },
    tokens,
  };
}

export async function buildTokenListV2(p: { call: EthCall; getLogs: GetLogs; floorAmm: string; chainId: number; fromBlock: number; timestamp: string; version: TokenList["version"]; multicall?: string | null }): Promise<{ list: TokenList; skipped: { launchId: string; why: string }[] }> {
  const logs = await p.getLogs({ address: p.floorAmm, topic0: TOPIC.LaunchCreated, fromBlock: p.fromBlock, toBlock: "latest" });
  const born = new Map<bigint, LaunchMeta>();
  for (const log of logs) {
    const m = launchFromLog(p.floorAmm, log);
    if (m && !born.has(m.id)) born.set(m.id, m);
  }
  const tokens: TokenListEntry[] = [];
  const skipped: { launchId: string; why: string }[] = [];
  const listed = [...born.values()].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)).filter((m) => m.listed);
  const rets = await namesOf(p.call, listed, p.multicall === undefined ? MULTICALL3_ADDRESS : p.multicall);
  listed.forEach((m, i) => {
    const e = entryOf(p.chainId, p.floorAmm, m, rets[i]![0], rets[i]![1]);
    if (typeof e === "string") skipped.push({ launchId: m.id.toString(), why: e });
    else tokens.push(e);
  });
  return { list: listOf(p.timestamp, p.version, tokens), skipped };
}
