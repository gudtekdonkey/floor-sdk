

import { test } from "node:test";
import assert from "node:assert/strict";
import { bytesToHex, encodeAbi, functionSelector } from "../vendor/sdk/src/evm/abi.ts";
import { keccak256 } from "../vendor/sdk/src/evm/keccak.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import { bookFrom, loadGoldenV2 } from "./golden-v2.ts";
import type { Book } from "../vendor/sdk/src/math/v2/book.ts";
import { quoteBookBuy, quoteBookSell } from "../src/v2.ts";
import { FN_V2, OUT_V2, RANGES_MAX, dexLiquidityV2 } from "../src/v2.ts";
import type { LaunchBook } from "../src/v2.ts";
import { lifiQuoteV2 } from "../src/lifiv2.ts";
import { buyLessIn } from "../vendor/sdk/src/math/v2/trade.ts";
import { OneInchFloorAmmV2, TOKEN_SCALE, amountOutV2, bookOf } from "../src/v2pool.ts";
import { QUOTE_WRITTEN_TOPIC } from "../src/protocol.ts";
import { TOPIC } from "../src/abi.ts";

function viewOf(b: Book): LaunchBook {
  return {
    sqrtP: b.sqrtP, sqrtF: b.sqrtF, f: b.floor, bandMoney: [...b.bandMoney], nRanges: b.nRanges,
    ranges: b.ranges.slice(0, b.nRanges).map((r) => ({ sqrtA: r.sqrtA, sqrtB: r.sqrtB, tokens: r.tokens, askLeft: r.askLeft, back: r.back, place: r.place, thetaBps: r.thetaBps, psiBps: r.psiBps, kind: r.kind, class: r.class })),
    sqrtLMax: b.sqrtLMax, floorTokens: b.floorTokens, bandTokens: [...b.bandTokens], bandEdge: [...b.bandEdge],
    nBands: b.nBands, nC: b.nC, bandF: [...b.bandF], bandFClaim: [...b.bandFClaim], ckAtF: b.ckAtF, ckFloorTokens: b.ckFloorTokens,
    graduated: b.graduated, feePpm: [...b.feePpm],
  };
}

const words = (v: LaunchBook): AbiValue[] => {
  const ranges: AbiValue[] = [];
  for (let i = 0; i < RANGES_MAX; i++) {
    const r = v.ranges[i];
    ranges.push(r ? [r.sqrtA, r.sqrtB, r.tokens, r.askLeft, r.back, r.place, BigInt(r.thetaBps), BigInt(r.psiBps), BigInt(r.kind), BigInt(r.class)] : Array(10).fill(0n));
  }
  return [[v.sqrtP, v.sqrtF, v.f, v.bandMoney, BigInt(v.nRanges), ranges, v.sqrtLMax, v.floorTokens, v.bandTokens, v.bandEdge,
    BigInt(v.nBands), BigInt(v.nC), v.bandF, v.bandFClaim, v.ckAtF, v.ckFloorTokens, v.graduated, v.feePpm.map(BigInt)]];
};

function walkOut(b: Book, isBuy: boolean, n: bigint): bigint {
  try {
    if (isBuy) {
      const r = quoteBookBuy(b, n);
      return r.refund > 0n && (r.budgetHit || r.refund * 1_000_000n > n) ? 0n : r.tokens * TOKEN_SCALE;
    }
    const r = quoteBookSell(b, n / TOKEN_SCALE);
    return r.unfilled > 0n ? 0n : r.out;
  } catch {
    return 0n;
  }
}

test("golden/v2 book: the views' book is the math's book, and amountOutV2 is the walk (refund → 0 but dust)", () => {
  let n = 0, nonzero = 0;
  for (const c of loadGoldenV2("book").cases) {
    if ((c.fn !== "buy" && c.fn !== "sell") || !c.state_in) continue;
    const b = bookFrom(c.state_in);
    const v = viewOf(b);
    const curve = { feeStartPpm: b.feeStartPpm, sqrtFeeA: b.sqrtFeeA, sqrtFeeB: b.sqrtFeeB };
    assert.deepEqual(bookOf(v, curve), { ...b, ranges: b.ranges.map((r) => ({ ...r, msCapPpm: 0 })) }, `${c.id}: view → book`);
    const isBuy = c.fn === "buy";
    const amount = isBuy ? BigInt(c.args.gross) : BigInt(c.args.tokens) * TOKEN_SCALE;
    const want = walkOut(b, isBuy, amount);
    assert.equal(amountOutV2(v, isBuy, amount, curve), want, c.id);
    n++;
    if (want > 0n) nonzero++;
  }
  assert.ok(n > 20 && nonzero > 10, `${n} cases, ${nonzero} priced`);
});

test("a sell of a non-whole math unit prices its whole units (sellFromBalance returns the sub-unit rest)", () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "sell" && x.state_in)!;
  const b = bookFrom(c.state_in!);
  const v = viewOf(b);
  assert.equal(amountOutV2(v, false, BigInt(c.args.tokens) * TOKEN_SCALE + 1n), walkOut(b, false, BigInt(c.args.tokens) * TOKEN_SCALE));
  assert.equal(amountOutV2(v, false, TOKEN_SCALE - 1n), 0n);
});

test("OneInchFloorAmmV2 reads the views: liquidity, tokens, getReturn and the router swap", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in && walkOut(bookFrom(x.state_in), true, BigInt(x.args.gross)) > 0n)!;
  const b = bookFrom(c.state_in!);
  const v = viewOf(b);
  const data = { book: bytesToHex(encodeAbi(OUT_V2.launchBook, words(v))), curve: bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]])) };
  const amm = "0x1234567890abcdef1234567890abcdef12345678", token = "0x00000000000000000000000000000000000000bb", quote = "0x0000000000000000000000000000000000000000";
  const call = async (_to: string, cd: string): Promise<string> => (cd.slice(0, 10) === sel("launchBook") ? data.book : data.curve);
  const pool = new OneInchFloorAmmV2(1, amm, call);
  await pool.refresh({ id: 1n, token, quote, createdBlock: 1, listed: true }, "latest");
  const [p] = pool.pools();
  assert.equal(p.liquidityQuote, dexLiquidityV2(v));
  assert.deepEqual(p.tokens, ["0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", token]);

  assert.equal(p.pairId, "0x99e61528d26b8142c5975683c5533b7afbe872a9ef426c9e9cfe43f5c9ce53a4");
  assert.equal(p.pairId, bytesToHex(keccak256(encodeAbi(["address", "uint256"], [amm, 1n]))));
  const gross = BigInt(c.args.gross);
  assert.equal(pool.getReturn(p.id, "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", token, gross).amountOut, walkOut(b, true, gross));
  const s = pool.swap(p.id, "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", token, gross, 1n, "0x00000000000000000000000000000000000000cc");
  assert.equal(s.value, gross);
  assert.equal(s.to, amm);
  assert.throws(() => pool.getReturn(p.id, token, token, 1n), /does not trade/);
});

function sel(name: "launchBook" | "launchFeeCurve"): string {
  return bytesToHex(functionSelector(FN_V2[name]));
}

test("lifiQuoteV2: the contract's quote is the amount, the book walk agrees, a refund throws WOULD_REFUND", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in && walkOut(bookFrom(x.state_in), true, BigInt(x.args.gross)) > 0n)!;
  const b = bookFrom(c.state_in!);
  const gross = BigInt(c.args.gross);
  const out = walkOut(b, true, gross);
  const book = bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b))));
  const amm = "0x00000000000000000000000000000000000000aa", token = "0x00000000000000000000000000000000000000bb", quote = "0x0000000000000000000000000000000000000000";
  const quoteBuyWords = (o: bigint, refund: bigint): string => bytesToHex(encodeAbi(["(uint256,uint256,uint256,uint256,bool,bool,bool)"], [[o, 0n, refund, 0n, false, false, false]]));
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const quoted: bigint[] = [];
  const mk = (refund: bigint) => async (_to: string, cd: string): Promise<string> => {
    const s4 = cd.slice(0, 10);
    if (s4 === sel("launchBook")) return book;
    if (s4 === sel("launchFeeCurve")) return curve;
    quoted.push(BigInt("0x" + cd.slice(10 + 64, 10 + 128)));
    return quoteBuyWords(out, refund);
  };
  const ctx = (refund: bigint) => ({ floorAmm: amm, call: mk(refund), blockNumber: 1, launchOf: (t: string) => (t === token ? { id: 1n, token, quote, createdBlock: 1, listed: true } : undefined) });
  const req = { chainId: 1, fromToken: "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", toToken: token, fromAmount: gross.toString(), fromAddress: "0x00000000000000000000000000000000000000cc" };
  const q = await lifiQuoteV2(ctx(0n), req);
  assert.equal(q.estimate.toAmount, out.toString());
  assert.equal(q.estimate.mathAgrees, true);

  assert.deepEqual(quoted, [gross, buyLessIn(gross, 50)]);
  assert.equal(q.estimate.toAmountMin, out.toString());
  await assert.rejects(lifiQuoteV2(ctx(gross), req), /WOULD_REFUND/);
});

test("lifiQuoteV2 carries a dust refund (≤ 1 ppm of the input), refuses a larger one and any after a budget hit", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in && walkOut(bookFrom(x.state_in), true, BigInt(x.args.gross)) > 0n)!;
  const b = bookFrom(c.state_in!);
  const gross = BigInt(c.args.gross);
  const out = walkOut(b, true, gross);
  const book = bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b))));
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const amm = "0x00000000000000000000000000000000000000aa", token = "0x00000000000000000000000000000000000000bb", quote = "0x0000000000000000000000000000000000000000";
  const mk = (refund: bigint, budgetHit: boolean) => async (_to: string, cd: string): Promise<string> => {
    const s4 = cd.slice(0, 10);
    if (s4 === sel("launchBook")) return book;
    if (s4 === sel("launchFeeCurve")) return curve;
    return bytesToHex(encodeAbi(["(uint256,uint256,uint256,uint256,bool,bool,bool)"], [[out, 0n, refund, 0n, false, false, budgetHit]]));
  };
  const ctx = (refund: bigint, budgetHit = false) => ({ floorAmm: amm, call: mk(refund, budgetHit), blockNumber: 1, launchOf: (t: string) => (t === token ? { id: 1n, token, quote, createdBlock: 1, listed: true } : undefined) });
  const req = { chainId: 1, fromToken: "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", toToken: token, fromAmount: gross.toString(), fromAddress: "0x00000000000000000000000000000000000000cc" };

  const big = 10n ** 9n, dust = big / 1_000_000n;
  Object.assign(req, { fromAmount: big.toString() });
  assert.equal((await lifiQuoteV2(ctx(dust), req)).estimate.toAmount, out.toString(), "a dust refund rides");
  await assert.rejects(lifiQuoteV2(ctx(dust + 1n), req), /WOULD_REFUND/);
  await assert.rejects(lifiQuoteV2(ctx(dust, true), req), /WOULD_REFUND.*budget/);
});

test("getReturn's gas is gasFor(side, the walk's steps), never below the measured flat figure", async () => {
  const { gasFor, GAS_FLAT } = await import("../vendor/sdk/src/v2/router.ts");
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in && walkOut(bookFrom(x.state_in), true, BigInt(x.args.gross)) > 0n)!;
  const b = bookFrom(c.state_in!);
  const data = { book: bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b)))), curve: bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]])) };
  const amm = "0x1234567890abcdef1234567890abcdef12345678", token = "0x00000000000000000000000000000000000000bb";
  const pool = new OneInchFloorAmmV2(1, amm, async (_t: string, cd: string) => (cd.slice(0, 10) === sel("launchBook") ? data.book : data.curve));
  await pool.refresh({ id: 1n, token, quote: "0x0000000000000000000000000000000000000000", createdBlock: 1, listed: true }, "latest");
  const p = pool.pools()[0];
  const gross = BigInt(c.args.gross);
  const steps = quoteBookBuy(b, gross).steps;
  const r = pool.getReturn(p.id, "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", token, gross);
  assert.equal(r.gas, gasFor("buy", steps));
  assert.ok(r.gas >= GAS_FLAT.buy);
});

test("a launch on a quote unlisted at create is routed once the quote is written (QuoteWritten re-check)", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in)!;
  const b = bookFrom(c.state_in!);
  const book = bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b))));
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const amm = "0x1234567890abcdef1234567890abcdef12345678", token = "0x00000000000000000000000000000000000000bb", quote = "0x00000000000000000000000000000000000000ee";
  let listed = false;
  const call = async (_to: string, cd: string): Promise<string> => {
    const s4 = cd.slice(0, 10);
    if (s4 === sel("launchBook")) return book;
    if (s4 === sel("launchFeeCurve")) return curve;

    return bytesToHex(encodeAbi(["(bool,bool,uint8,uint256,uint256,uint256,uint64)"], [[listed, true, 18n, 0n, 1n, 0n, 1n]]));
  };
  const pool = new OneInchFloorAmmV2(1, amm, call as never);
  const w = (n: bigint): string => "0x" + n.toString(16).padStart(64, "0");
  const a32 = (a: string): string => "0x" + a.slice(2).padStart(64, "0");
  const created = { address: amm, topics: [TOPIC.LaunchCreated, w(5n), a32(token), a32(quote)], data: "", blockNumber: 3 };
  const { EV_DATA } = await import("../src/abi.ts");
  created.data = bytesToHex(encodeAbi([...EV_DATA.LaunchCreated], EV_DATA.LaunchCreated.map((ty) => (ty === "bool" ? false : ty === "string" ? "" : ty === "address" ? "0x" + "00".repeat(20) : ty === "bytes32" ? new Uint8Array(32) : 0n)) as never));
  await pool.onLogs([created as never]);
  assert.equal(pool.pools().length, 0, "unlisted at create and now: not routed");
  assert.equal(pool.pending.size, 1);
  listed = true;
  await pool.onLogs([{ address: amm, topics: [QUOTE_WRITTEN_TOPIC, a32(quote)], data: "0x", blockNumber: 8 } as never]);
  assert.equal(pool.pools().length, 1, "listed later: routed");
  assert.equal(pool.pending.size, 0);
});

test("dexLiquidityV2 on the view is the SDK's on the book (F + C + K over nBands + Σ place), slots past nBands unread", async () => {
  const { dexLiquidityV2: sdkLiquidity } = await import("../vendor/sdk/src/math/v2/display.ts");
  let n = 0;
  for (const c of loadGoldenV2("book").cases) {
    if (!c.state_in) continue;
    const b = bookFrom(c.state_in);
    const v = viewOf(b);
    assert.equal(dexLiquidityV2(v), sdkLiquidity(b), c.id);

    if (b.nBands < v.bandMoney.length) {
      const stale = { ...v, bandMoney: v.bandMoney.map((m, i) => (i >= b.nBands ? m + 7n : m)) };
      assert.equal(dexLiquidityV2(stale), sdkLiquidity(b), `${c.id}: past nBands`);
      n++;
    }
  }
  assert.ok(n > 0, "a case with fewer segments than slots");
});

test("lifiQuoteV2 refuses a sell the bid would partly hand back, or one under a whole math unit (WOULD_REFUND)", async () => {

  const c = loadGoldenV2("book").cases.find((x) => x.id === "book-0009")!;
  const b = bookFrom(c.state_in!);
  const tokens = BigInt(c.args.tokens) * 3n;
  assert.ok(quoteBookSell(b, tokens).unfilled > 0n, "the walk leaves part unfilled");
  const book = bytesToHex(encodeAbi(OUT_V2.launchBook, words(viewOf(b))));
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const amm = "0x00000000000000000000000000000000000000aa", token = "0x00000000000000000000000000000000000000bb", quote = "0x0000000000000000000000000000000000000000";
  const call = async (_to: string, cd: string): Promise<string> => {
    const s4 = cd.slice(0, 10);
    if (s4 === sel("launchBook")) return book;
    if (s4 === sel("launchFeeCurve")) return curve;
    return bytesToHex(encodeAbi(["uint256"], [1_000n]));
  };
  const ctx = { floorAmm: amm, call, blockNumber: 1, launchOf: (t: string) => (t === token ? { id: 9n, token, quote, createdBlock: 1, listed: true } : undefined) };
  const req = (n: bigint) => ({ chainId: 1, fromToken: token, toToken: "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeeEeE", fromAmount: n.toString(), fromAddress: "0x00000000000000000000000000000000000000cc" });
  await assert.rejects(lifiQuoteV2(ctx, req(tokens * TOKEN_SCALE)), /WOULD_REFUND/);
  await assert.rejects(lifiQuoteV2(ctx, req(BigInt(c.args.tokens) * TOKEN_SCALE + 1n)), /WOULD_REFUND/);

  const whole = await lifiQuoteV2(ctx, req(BigInt(c.args.tokens) * TOKEN_SCALE));
  assert.equal(whole.estimate.toAmount, "1000");
});

test("OneInchFloorAmmV2.onLogs: a BackingCredited log re-reads the launch's book at its block (F rose)", async () => {
  const c = loadGoldenV2("book").cases.find((x) => x.fn === "buy" && x.state_in)!;
  const b = bookFrom(c.state_in!);
  const before = viewOf(b);
  const after = { ...before, f: before.f + 1_000n };
  const curve = bytesToHex(encodeAbi(OUT_V2.launchFeeCurve, [[b.feeStartPpm.map(BigInt), b.sqrtFeeA, b.sqrtFeeB]]));
  const amm = "0x1234567890abcdef1234567890abcdef12345678", token = "0x00000000000000000000000000000000000000bb", quote = "0x0000000000000000000000000000000000000000";
  const blocks: string[] = [];
  const call = async (_to: string, cd: string, block?: number | "latest"): Promise<string> => {
    if (cd.slice(0, 10) !== sel("launchBook")) return curve;
    blocks.push(String(block));
    return bytesToHex(encodeAbi(OUT_V2.launchBook, words(block === 9 ? after : before)));
  };
  const pool = new OneInchFloorAmmV2(1, amm, call as never);
  await pool.refresh({ id: 1n, token, quote, createdBlock: 1, listed: true }, 5);
  const l0 = pool.pools()[0].liquidityQuote;
  const topic = bytesToHex(keccak256(new TextEncoder().encode("BackingCredited(uint256,address,uint256,uint256,uint64)")));
  const id = "0x" + 1n.toString(16).padStart(64, "0");
  const donor = "0x" + "dd".padStart(64, "0");
  await pool.onLogs([{ address: amm, topics: [topic, id, donor], data: bytesToHex(encodeAbi(["uint256", "uint256", "uint64"], [1_000n, after.f, 9n])), blockNumber: 9 } as never]);
  assert.deepEqual(blocks, ["5", "9"]);
  assert.equal(pool.pools()[0].liquidityQuote, l0 + 1_000n, "the credit is in the pool's liquidity");
});
