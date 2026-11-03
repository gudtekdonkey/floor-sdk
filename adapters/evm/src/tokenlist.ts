

import { FN, OUT, TOPIC, calldata, decode } from "./abi.ts";
import { launchFromLog, toApiToken } from "./protocol.ts";
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

async function entryOf(call: EthCall, chainId: number, floorAmm: string, m: LaunchMeta): Promise<TokenListEntry | string> {
  const [name] = decode(OUT.name, await call(m.token, calldata(FN.name, []), "latest")) as string[];
  const [symbol] = decode(OUT.symbol, await call(m.token, calldata(FN.symbol, []), "latest")) as string[];
  if (name.length === 0 || name.length > NAME_MAX) return `name ${JSON.stringify(name)}`;
  if (!SYMBOL_OK.test(symbol)) return `symbol ${JSON.stringify(symbol)}`;
  return { chainId, address: m.token, name, symbol, decimals: 18, tags: ["floor-amm"], extensions: { launchId: m.id.toString(), quote: toApiToken(m.quote), floorAmm: floorAmm.toLowerCase() } };
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

export async function buildTokenListV2(p: { call: EthCall; getLogs: GetLogs; floorAmm: string; chainId: number; fromBlock: number; timestamp: string; version: TokenList["version"] }): Promise<{ list: TokenList; skipped: { launchId: string; why: string }[] }> {
  const logs = await p.getLogs({ address: p.floorAmm, topic0: TOPIC.LaunchCreated, fromBlock: p.fromBlock, toBlock: "latest" });
  const born = new Map<bigint, LaunchMeta>();
  for (const log of logs) {
    const m = launchFromLog(p.floorAmm, log);
    if (m && !born.has(m.id)) born.set(m.id, m);
  }
  const tokens: TokenListEntry[] = [];
  const skipped: { launchId: string; why: string }[] = [];
  for (const m of [...born.values()].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))) {
    if (!m.listed) continue;
    const e = await entryOf(p.call, p.chainId, p.floorAmm, m);
    if (typeof e === "string") skipped.push({ launchId: m.id.toString(), why: e });
    else tokens.push(e);
  }
  return { list: listOf(p.timestamp, p.version, tokens), skipped };
}
