
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { bytesToHex, encodeAbi, functionSelector } from "../vendor/sdk/src/evm/abi.ts";
import { buyLessIn } from "../vendor/sdk/src/math/v2/trade.ts";
import { FN_V2, OUT_V2, RANGES_MAX } from "../src/v2.ts";
import type { LaunchBook } from "../src/v2.ts";
import { FN, OUT } from "../src/abi.ts";
import { bookFrom, loadGoldenV2 } from "./golden-v2.ts";
import type { Book } from "../vendor/sdk/src/math/v2/book.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import { quoteBookSell } from "../src/v2.ts";
import { planSquidBuy, planSquidSell } from "../src/squidv2.ts";

const AMM = "0x00000000000000000000000000000000000000aa";
const TOKEN = "0x00000000000000000000000000000000000000bb";
const USDC = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
const MC = "0x1111111111111111111111111111111111111111";
const USER = "0x00000000000000000000000000000000000000cc";
const sel = (s: string): string => bytesToHex(functionSelector(s));
const meta = (listed = true) => ({ id: 3n, token: TOKEN, quote: USDC, createdBlock: 1, listed });
const quoteBuyWords = (o: bigint, refund: bigint): string => bytesToHex(encodeAbi(["(uint256,uint256,uint256,uint256,bool,bool,bool)"], [[o, 0n, refund, 0n, false, false, false]]));
const mkCtx = (refund = 0n, listed = true) => {
  const amounts: bigint[] = [];
  const call = async (_to: string, cd: string): Promise<string> => {
    if (cd.slice(0, 10) === sel(FN_V2.quoteBuy)) {
      const a = BigInt("0x" + cd.slice(10 + 64, 10 + 128));
      amounts.push(a);
      return quoteBuyWords(a * 1000n, refund);
    }
    if (cd.slice(0, 10) === sel(FN.quoteEntry)) return bytesToHex(encodeAbi(OUT.quoteEntry, [[false, false, 18n, 0n, 0n, 0n, 0n]]));
    throw new Error("unexpected " + cd.slice(0, 10));
  };
  return { ctx: { floorAmm: AMM, call, blockNumber: 1, launchOf: (t: string) => (t === TOKEN ? meta(listed) : undefined) }, amounts };
};
const req = { chainId: 8453, multicall: MC, launchToken: TOKEN, user: USER, deadline: 99n, integratorId: "test-integrator-id", quoteAmount: 1_000_000n, quoteAmountMin: 990_000n };

test("buy: pass 1 at the expected amount, pass 2 at the bridge minimum less the slippage; minOut from pass 2", async () => {
  const { ctx, amounts } = mkCtx();
  const p = await planSquidBuy(ctx, req);
  assert.deepEqual(amounts, [1_000_000n, buyLessIn(990_000n, 300)]);
  assert.equal(p.expectedOut, "1000000000");
  assert.equal(p.minOut, (buyLessIn(990_000n, 300) * 1000n).toString());
  assert.equal(p.hook.calls.length, 2);
  assert.deepEqual(p.headers, { "x-integrator-id": "test-integrator-id" });
});

test("the integrator id is required, injected, and not in the source", async () => {
  const { ctx } = mkCtx();
  await assert.rejects(planSquidBuy(ctx, { ...req, integratorId: "" }), /NO_INTEGRATOR_ID/);
  assert.ok(!/x-integrator-id["']?\s*:\s*["'][A-Za-z0-9_-]{6,}/.test(readFileSync(new URL("../src/squidv2.ts", import.meta.url), "utf8")));
});

test("refusals: refund, unknown launch, unlisted quote, bad slippage, Solana", async () => {
  await assert.rejects(planSquidBuy(mkCtx(5n).ctx, req), /WOULD_REFUND/);
  await assert.rejects(planSquidBuy(mkCtx().ctx, { ...req, launchToken: "0x00000000000000000000000000000000000000dd" }), /NO_LAUNCH/);
  await assert.rejects(planSquidBuy(mkCtx(0n, false).ctx, req), /UNLISTED_QUOTE/);
  await assert.rejects(planSquidBuy(mkCtx().ctx, { ...req, slippageBps: 10_001 }), /BAD_SLIPPAGE/);
  await assert.rejects(planSquidBuy(mkCtx().ctx, { ...req, chainId: 101 }), /SOLANA/);
  await assert.rejects(planSquidSell(mkCtx().ctx, { ...req, tokensIn: 10n ** 12n + 1n }), /WOULD_REFUND/);
});

const viewOf = (b: Book): LaunchBook => ({
  sqrtP: b.sqrtP, sqrtF: b.sqrtF, f: b.floor, bandMoney: [...b.bandMoney], nRanges: b.nRanges,
  ranges: b.ranges.slice(0, b.nRanges).map((r) => ({ sqrtA: r.sqrtA, sqrtB: r.sqrtB, tokens: r.tokens, askLeft: r.askLeft, back: r.back, place: r.place, thetaBps: r.thetaBps, psiBps: r.psiBps, kind: r.kind, class_: r.class })),
  sqrtLMax: b.sqrtLMax, floorTokens: b.floorTokens, bandTokens: [...b.bandTokens], bandEdge: [...b.bandEdge],
  nBands: b.nBands, nC: b.nC, bandF: [...b.bandF], bandFClaim: [...b.bandFClaim], ckAtF: b.ckAtF, ckFloorTokens: b.ckFloorTokens,
  graduated: b.graduated, feePpm: [...b.feePpm],
});
const words = (v: LaunchBook): AbiValue[] => {
  const ranges: AbiValue[] = [];
  for (let i = 0; i < RANGES_MAX; i++) {
    const r = v.ranges[i];
    ranges.push(r ? [r.sqrtA, r.sqrtB, r.tokens, r.askLeft, r.back, r.place, BigInt(r.thetaBps), BigInt(r.psiBps), BigInt(r.kind), BigInt(r.class_)] : Array(10).fill(0n));
  }
  return [[v.sqrtP, v.sqrtF, v.f, v.bandMoney, BigInt(v.nRanges), ranges, v.sqrtLMax, v.floorTokens, v.bandTokens, v.bandEdge,
    BigInt(v.nBands), BigInt(v.nC), v.bandF, v.bandFClaim, v.ckAtF, v.ckFloorTokens, v.graduated, v.feePpm.map(BigInt)]];
};

test("sell: whole fill gives a preHook with the slippage minimum; a hand-back is refused", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.id === "book-0009")!;
  const b = bookFrom(c.state_in!);
  const book = bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b))));
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const call = async (_to: string, cd: string): Promise<string> => {
    const s4 = cd.slice(0, 10);
    if (s4 === sel(FN_V2.launchBook)) return book;
    if (s4 === sel(FN_V2.launchFeeCurve)) return curve;
    if (s4 === sel(FN.quoteEntry)) return bytesToHex(encodeAbi(OUT.quoteEntry, [[true, true, 6n, 0n, 0n, 0n, 0n]]));
    return bytesToHex(encodeAbi(["uint256"], [1_000_000n]));
  };
  const ctx = { floorAmm: AMM, call, blockNumber: 1, launchOf: (t: string) => (t === TOKEN ? meta() : undefined) };
  const whole = BigInt(c.args.tokens) * 10n ** 12n;
  assert.equal(quoteBookSell(b, BigInt(c.args.tokens)).unfilled, 0n);
  const p = await planSquidSell(ctx, { ...req, tokensIn: whole });
  assert.equal(p.expectedOut, "1000000");
  assert.equal(p.minOut, "970000");
  assert.equal(p.hook.fundAmount, whole.toString());
  await assert.rejects(planSquidSell(ctx, { ...req, tokensIn: whole * 3n }), /WOULD_REFUND/);
});

test("buy: a dust refund (≤ 1 ppm of the input) rides, a larger one is refused", async () => {
  const r1 = { ...req, quoteAmountMin: req.quoteAmount, slippageBps: 0 };
  const p = await planSquidBuy(mkCtx(1n).ctx, r1);
  assert.equal(p.launchId, "3");
  await assert.rejects(planSquidBuy(mkCtx(2n).ctx, r1), /WOULD_REFUND/);
});
