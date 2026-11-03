

import { test } from "node:test";
import assert from "node:assert/strict";
import { AMM, MockChain } from "./mock-floor.ts";
import { buildTokenListV2 } from "../src/tokenlist.ts";
import type { GetLogs } from "../src/tokenlist.ts";
import { EV_DATA, TOPIC } from "../src/abi.ts";
import { bytesToHex, encodeAbi } from "../vendor/sdk/src/evm/abi.ts";
import { MAX_SLIPPAGE_BPS, NATIVE, minOutFor, swapCall } from "../src/protocol.ts";
import { LifiQuoteError, lifiQuoteV2 } from "../src/lifiv2.ts";
import type { LifiContext } from "../src/lifiv2.ts";
import { FloorError } from "../vendor/sdk/src/errors.ts";
import { MAX_SLIPPAGE_BPS as SDK_MAX_SLIPPAGE_BPS, minOutAt } from "../vendor/sdk/src/v2/press.ts";
import type { EthCall } from "../src/protocol.ts";

const TOKEN = "0x2000000000000000000000000000000000000001";
const USER = "0x6000000000000000000000000000000000000006";
const ONE = 1n << 64n;
const ETH_CENTS = 240_000n * ONE;

function chainWith(symbol?: string): MockChain {
  const chain = new MockChain();
  chain.add({ id: 1n, token: TOKEN, quote: NATIVE, quoteUsdCentsX64: ETH_CENTS, createdBlock: 1, symbol });
  chain.block = 2_000;
  return chain;
}
const callOf = (chain: MockChain): EthCall => (to, data, b) => chain.call({ to, data }, b);
const word = (v: bigint | string): string => `0x${BigInt(v).toString(16).padStart(64, "0")}`;

const createdLogs: GetLogs = async () => {
  const z = `0x${"00".repeat(32)}`;
  const data = bytesToHex(encodeAbi(EV_DATA.LaunchCreated, [USER, 1n, 1n, z, 0n, 0n, 0n, 0n, 0n, "Launch 1", "L1", "u", true, 0n, 0n]));
  return [{ address: AMM, topics: [TOPIC.LaunchCreated, word(1n), word(TOKEN), word(NATIVE)], data, blockNumber: 1 }];
};

test("SEC-OFF-D3: minOutFor refuses slippage above 100 %, negatives, fractions and NaN; never signs a 0 minimum", () => {
  for (const s of [10_001, 20_000, -1, 0.5, Number.NaN, Number.POSITIVE_INFINITY]) assert.throws(() => minOutFor(1_000n, s), FloorError, `slippage ${s}`);
  assert.throws(() => minOutFor(0n, 50), FloorError);
  assert.throws(() => minOutFor(-1n, 50), FloorError);
  assert.equal(MAX_SLIPPAGE_BPS, 10_000);
  assert.equal(minOutFor(1_000n, 10_000), 1n);
  assert.equal(minOutFor(1_000n, 0), 1_000n);
  assert.equal(minOutFor(1_000n, 50), 995n);
  assert.equal(minOutFor(1_000n, 9_999), 1n);
  assert.equal(minOutFor(1n, 9_999), 1n);
  assert.equal(minOutFor(999n, 1), 999n);

  const z = swapCall({ floorAmm: AMM, id: 1n, isBuy: false, ethIn: false, amountIn: 1n, minOut: minOutFor(1_000n, 9_999), to: USER, mode: "direct" });
  assert.equal(BigInt(`0x${z.data.slice(10 + 128, 10 + 192)}`), 1n);
});

test("SEC-OFF-D3: lifiQuoteV2 refuses a request slippage outside 0..=10,000 before any read", async () => {
  const never = (): never => { throw new Error("read"); };
  const ctx = { call: never, floorAmm: AMM, launchOf: () => ({ id: 1n, token: TOKEN, quote: NATIVE, listed: true }), blockNumber: 2_000 } as unknown as LifiContext;
  for (const slippageBps of [10_001, -1, 2.5]) {
    await assert.rejects(lifiQuoteV2(ctx, { chainId: 8453, fromToken: NATIVE, toToken: TOKEN, fromAmount: "1000", fromAddress: USER, slippageBps }), (e: unknown) => e instanceof LifiQuoteError && e.code === "BAD_SLIPPAGE");
  }
});

test("SEC-OFF-D3: minOutFor is the SDK's minOutAt", () => {
  assert.equal(minOutFor, minOutAt);
  assert.equal(MAX_SLIPPAGE_BPS, SDK_MAX_SLIPPAGE_BPS);
});

test("PoC SEC-OFF-D4: the token list admits a launch that names itself USDC", async () => {
  const chain = chainWith("USDC");
  const { list, skipped } = await buildTokenListV2({ call: callOf(chain), getLogs: createdLogs, floorAmm: AMM, chainId: 8453, fromBlock: 0, timestamp: "2026-10-24T00:00:00Z", version: { major: 1, minor: 0, patch: 0 } });
  assert.equal(skipped.length, 0);
  assert.equal(list.tokens[0].symbol, "USDC");

  const chain2 = chainWith("‮CDSU");
  const r2 = await buildTokenListV2({ call: callOf(chain2), getLogs: createdLogs, floorAmm: AMM, chainId: 8453, fromBlock: 0, timestamp: "2026-10-24T00:00:00Z", version: { major: 1, minor: 0, patch: 0 } });
  assert.equal(r2.list.tokens.length, 1);
});
