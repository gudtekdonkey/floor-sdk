

import { test } from "node:test";
import assert from "node:assert/strict";
import { AMM, MockChain } from "./mock-floor.ts";
import { LifiQuoteError, lifiQuoteV2 } from "../src/lifiv2.ts";
import { ETH_PLACEHOLDER, NATIVE, routable } from "../src/protocol.ts";
import type { EthCall } from "../src/protocol.ts";

const TOKEN = "0x2000000000000000000000000000000000000001";
const USER = "0x6000000000000000000000000000000000000006";
const ETH_CENTS = 240_000n << 64n;

function chainOf(n: number): MockChain {
  const chain = new MockChain();
  for (let i = 0; i < n; i++) {
    const token = i === 0 ? TOKEN : `0x20000000000000000000000000000000000000${(i + 1).toString(16).padStart(2, "0")}`;
    chain.add({ id: BigInt(i + 1), token, quote: NATIVE, quoteUsdCentsX64: ETH_CENTS, createdBlock: 1, symbol: i === 1 ? "BAD SYMBOL" : undefined });
  }
  chain.block = 1;
  return chain;
}
const callOf = (chain: MockChain): EthCall => (to, data, b) => chain.call({ to, data }, b);

test("an unlisted quote: routing and LI.FI leave the launch out unless it opts in", async () => {
  const chain = chainOf(2);
  const q = "0x3000000000000000000000000000000000000003";
  chain.launches.get(2n)!.listed = false;
  chain.launches.get(2n)!.quote = q;
  const call = callOf(chain);
  const un = { id: 2n, token: chain.launches.get(2n)!.token, quote: q, createdBlock: 1, listed: false };
  assert.equal(await routable(call, AMM, un, chain.block), false);
  assert.equal(await routable(call, AMM, un, chain.block, true), true);
  const ctx = { floorAmm: AMM, call, blockNumber: chain.block, launchOf: (t: string) => (t === un.token ? un : undefined) };
  await assert.rejects(lifiQuoteV2(ctx, { chainId: 8453, fromToken: un.token, toToken: un.quote, fromAmount: "1", fromAddress: USER }), (e: unknown) => e instanceof LifiQuoteError && e.code === "UNLISTED_QUOTE");

  chain.listedQuotes.add(q);
  assert.equal(await routable(call, AMM, un, chain.block), true);
});

import { buildTokenListV2 } from "../src/tokenlist.ts";
import type { GetLogs } from "../src/tokenlist.ts";
import { EV_DATA, TOPIC } from "../src/abi.ts";
import { bytesToHex, encodeAbi } from "../vendor/sdk/src/evm/abi.ts";
import type { Log } from "../src/protocol.ts";

const word = (v: bigint | string): string => `0x${BigInt(v).toString(16).padStart(64, "0")}`;

function createdLog(address: string, id: bigint, token: string, quote: string, listed: boolean): Log {
  const z = `0x${"00".repeat(32)}`;
  const data = bytesToHex(encodeAbi(EV_DATA.LaunchCreated, [USER, 1n, 1n, z, 0n, 0n, 0n, 0n, 0n, `Launch ${id}`, `L${id}`, "u", listed, 0n, 0n]));
  return { address, topics: [TOPIC.LaunchCreated, word(id), word(token), word(quote)], data, blockNumber: 1 };
}

function logsOf(logs: Log[]): GetLogs {
  return async (f) => logs.filter((l) => l.address.toLowerCase() === f.address.toLowerCase() && l.topics[0] === f.topic0 && l.blockNumber >= f.fromBlock);
}

const V = { major: 1, minor: 0, patch: 0 };

test("token list: one entry per listed create log, ids ascending; the schema refusal is skipped", async () => {
  const chain = chainOf(3);
  const ls = [...chain.launches.values()].reverse().map((m) => createdLog(AMM, m.id, m.token, m.quote, true));
  const { list, skipped } = await buildTokenListV2({ call: callOf(chain), getLogs: logsOf(ls), floorAmm: AMM, chainId: 8453, fromBlock: 0, timestamp: "2026-10-28T00:00:00Z", version: V });
  assert.deepEqual(list.tokens.map((t) => t.extensions.launchId), ["1", "3"]);
  assert.deepEqual(skipped, [{ launchId: "2", why: 'symbol "BAD SYMBOL"' }]);
  for (const t of list.tokens) {
    assert.equal(t.decimals, 18);
    assert.equal(t.extensions.quote, ETH_PLACEHOLDER);
    assert.equal(t.extensions.floorAmm, AMM.toLowerCase());
  }
});

test("⛔ R6 guard: a launch whose create log says unlisted is excluded entirely, even once its quote is listed", async () => {
  const chain = chainOf(2);
  const q = "0x3000000000000000000000000000000000000003";
  chain.launches.get(2n)!.quote = q;
  const ls = [createdLog(AMM, 1n, chain.launches.get(1n)!.token, NATIVE, true), createdLog(AMM, 2n, chain.launches.get(2n)!.token, q, false)];
  chain.listedQuotes.add(q);
  const { list, skipped } = await buildTokenListV2({ call: callOf(chain), getLogs: logsOf(ls), floorAmm: AMM, chainId: 1, fromBlock: 0, timestamp: "t", version: V });
  assert.deepEqual(list.tokens.map((t) => t.extensions.launchId), ["1"]);
  assert.deepEqual(skipped, [], "never reported");
});

test("token list: a LaunchCreated of another address is ignored, a repeated log counted once", async () => {
  const chain = chainOf(1);
  const other = "0x9000000000000000000000000000000000000009";
  const own = createdLog(AMM, 1n, TOKEN, NATIVE, true);
  const forged = createdLog(other, 7n, TOKEN, NATIVE, true);

  const loose: GetLogs = async () => [own, forged, own];
  const { list } = await buildTokenListV2({ call: callOf(chain), getLogs: loose, floorAmm: AMM, chainId: 1, fromBlock: 0, timestamp: "t", version: V });
  assert.deepEqual(list.tokens.map((t) => t.extensions.launchId), ["1"]);
});
