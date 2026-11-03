

import { test } from "node:test";
import assert from "node:assert/strict";
import { bytesToHex, encodeAbi, functionSelector } from "../vendor/sdk/src/evm/abi.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import { BANDS_MAX, FN_V2, OUT_V2, RANGES_MAX, dexLiquidityV2, readBook, readCirculating } from "../src/v2.ts";
import { bookOf } from "../src/v2pool.ts";
import { bidMoneyOf, dexLiquidityV2 as sdkDexLiquidityV2 } from "../vendor/sdk/src/math/v2/display.ts";

const hex = (b: Uint8Array): string => bytesToHex(b).replace(/^0x/, "");

test("v2 selectors are the function map's (the frozen trade selectors kept)", () => {
  const want: Record<keyof typeof FN_V2, string> = {
    launchBook: "", launchFeeCurve: "", circulatingOf: "72812fcf", quoteBuy: "ec7a34c9", quoteSell: "0cff08d1",
    buy: "102d8c79", sell: "badb999b",
  };
  for (const [k, sig] of Object.entries(FN_V2)) {
    const w = want[k as keyof typeof FN_V2];
    if (w) assert.equal(hex(functionSelector(sig)), w, sig);
  }
});

const range = (i: number): AbiValue[] => [
  BigInt(1 + i) << 72n, BigInt(2 + i) << 72n, 1_000n + BigInt(i), 900n, 50n, (1n << 70n) + BigInt(i), 100n, 200n, 1n, 2n,
];

const pad = (xs: bigint[], n: number): bigint[] => [...xs, ...Array(n - xs.length).fill(0n)];

function bookWords(nRanges: number): AbiValue[] {
  const ranges: AbiValue[] = [];
  for (let i = 0; i < RANGES_MAX; i++) ranges.push(i < nRanges ? range(i) : Array(10).fill(0n));

  const bands = pad([10n, 20n, 30n], BANDS_MAX);
  const edges = pad([9n << 72n, 8n << 72n, 7n << 72n, 6n << 72n], BANDS_MAX + 1);
  const bandF = pad([4n, 5n, 6n], BANDS_MAX), bandFClaim = pad([7n, 8n, 9n], BANDS_MAX);

  return [[3n << 72n, 2n << 72n, (1n << 66n) + 5_000n, bands, BigInt(nRanges), ranges, 4n << 72n, 1_234n,
    pad([1n, 2n, 3n], BANDS_MAX), edges, 3n, 2n, bandF, bandFClaim, 77n, 11n, true, [10_000n, 0n, 5_000n, 5_000n, 0n]]];
}

test("launchBook decodes and dexLiquidityV2 = the bid money, F + C + K, + place", async () => {
  const data = bytesToHex(encodeAbi(OUT_V2.launchBook, bookWords(3)));
  let seen = "";
  const call = async (to: string, cd: string): Promise<string> => { seen = cd; assert.equal(to, "0xamm"); return data; };
  const b = await readBook(call, "0xamm", 7n, "latest");
  assert.equal(seen.slice(0, 10), bytesToHex(functionSelector(FN_V2.launchBook)));
  assert.equal(b.nRanges, 3);
  assert.equal(b.ranges.length, 3);
  assert.equal(b.bandMoney.length, BANDS_MAX);
  assert.deepEqual([b.sqrtF, b.sqrtLMax, b.floorTokens, b.nBands, b.nC], [2n << 72n, 4n << 72n, 1_234n, 3, 2]);
  assert.deepEqual(b.bandTokens.slice(0, 4), [1n, 2n, 3n, 0n]);
  assert.deepEqual(b.bandF.slice(0, 4), [4n, 5n, 6n, 0n]);
  assert.deepEqual(b.bandFClaim.slice(0, 4), [7n, 8n, 9n, 0n]);
  assert.deepEqual([b.ckAtF, b.ckFloorTokens], [77n, 11n]);
  assert.equal(b.bandEdge.length, BANDS_MAX + 1);
  assert.deepEqual([b.graduated, b.feePpm], [true, [10_000, 0, 5_000, 5_000, 0]]);
  assert.equal(b.ranges[2].sqrtA, 3n << 72n);
  assert.equal(b.ranges[1].place, (1n << 70n) + 1n);

  assert.equal(bidMoneyOf(bookOf(b)), (1n << 66n) + 5_000n + 60n);

  assert.equal(dexLiquidityV2(b), sdkDexLiquidityV2(bookOf(b)), "adapter and SDK /api/dex liquidity differ");
});

test("launchBook refuses nRanges past the capacity", async () => {
  const w = bookWords(0);
  (w[0] as AbiValue[])[4] = BigInt(RANGES_MAX + 1);
  const data = bytesToHex(encodeAbi(OUT_V2.launchBook, w));
  await assert.rejects(readBook(async () => data, "0xamm", 1n, "latest"), /nRanges/);
});

test("launchBook refuses band arrays that disagree", async () => {
  const w = bookWords(1);
  (w[0] as AbiValue[])[9] = [1n, 2n];
  const data = bytesToHex(encodeAbi(OUT_V2.launchBook, w));
  await assert.rejects(readBook(async () => data, "0xamm", 1n, "latest"), /band arrays/);
});

test("launchBook refuses a bandF / bandFClaim that disagrees with bandMoney", async () => {
  for (const idx of [12, 13]) {
    const w = bookWords(1);
    (w[0] as AbiValue[])[idx] = [1n, 2n];
    const data = bytesToHex(encodeAbi(OUT_V2.launchBook, w));
    await assert.rejects(readBook(async () => data, "0xamm", 1n, "latest"), /band arrays/);
  }
});

test("circulatingOf decodes [sCirc, sBelow, dead]", async () => {
  const data = bytesToHex(encodeAbi(OUT_V2.circulatingOf, [900n, 800n, 100n]));
  assert.deepEqual(await readCirculating(async () => data, "0xamm", 1n, "latest"), [900n, 800n, 100n]);
});

test("the view tuples are solc's ABI of L3a's FloorBookTypes (forge inspect, placeholders deleted, ph-evm)", () => {
  const r = "(uint128,uint128,uint64,uint64,uint64,uint128,uint16,uint16,uint8,uint8)";
  assert.equal(OUT_V2.launchBook[0], `(uint128,uint128,uint128,uint128[],uint8,${r}[48],uint128,uint64,uint64[],uint128[],uint8,uint8,uint128[],uint128[],uint128,uint64,bool,uint32[5])`);
});

test("evm-routing-5/8: router.ts — isDustRefund, carriesNoRefund and gasFor(side, steps)", async () => {
  const { GAS_FLAT, GAS_MAX, carriesNoRefund, gasFor, isDustRefund } = await import("../vendor/sdk/src/v2/router.ts");
  const { GAS, gasFor: protocolGasFor, isDustRefund: protocolDust } = await import("../src/protocol.ts");
  assert.equal(protocolGasFor, gasFor, "protocol.ts re-exports the SDK's, never a copy");
  assert.equal(protocolDust, isDustRefund);
  assert.deepEqual(GAS, GAS_FLAT, "GAS stays exported for the Squid lane");
  assert.equal(isDustRefund(1_000_000n, 1n, false), true);
  assert.equal(isDustRefund(999_999n, 1n, false), false);
  assert.equal(isDustRefund(10n ** 30n, 4n, true), false);
  assert.equal(carriesNoRefund(5n, 0n, false), true);
  assert.equal(carriesNoRefund(5n, 0n, true), true, "no refund, no budget question");
  assert.equal(carriesNoRefund(10n ** 9n, 2_000n, false), false);

  for (const side of ["buy", "sell"] as const) {
    let last = 0;
    for (const steps of [0, 1, 4, 5, 10, 24, 48, 10_000]) {
      const g = gasFor(side, steps);
      assert.ok(g >= GAS_FLAT[side] && g >= last && g <= GAS_MAX, `${side} ${steps}: ${g}`);
      last = g;
    }
    assert.equal(gasFor(side, 0), GAS_FLAT[side]);
  }

  assert.ok(gasFor("buy", 11) >= 955_572, "a long walk is priced above the measured big buy");
});

test("Velora's hand-written gasFor constants are the SDK router.ts's (the dex cannot import the SDK)", async () => {
  const { readFileSync } = await import("node:fs");
  const r = await import("../vendor/sdk/src/v2/router.ts");
  const src = readFileSync(new URL("../fixtures/velora/book.ts", import.meta.url), "utf8");
  const pair = (name: string): [number, number] => {
    const m = new RegExp(`${name} = \\{ buy: ([\\d_]+), sell: ([\\d_]+) \\}`).exec(src)!;
    return [Number(m[1].replaceAll("_", "")), Number(m[2].replaceAll("_", ""))];
  };
  const one = (name: string): number => Number(new RegExp(`${name} = ([\\d_]+);`).exec(src)![1].replaceAll("_", ""));
  assert.deepEqual(pair("GAS_FLAT"), [r.GAS_FLAT.buy, r.GAS_FLAT.sell]);
  assert.deepEqual(pair("GAS_PER_STEP"), [r.GAS_PER_STEP.buy, r.GAS_PER_STEP.sell]);
  assert.equal(one("GAS_FLAT_STEPS"), r.GAS_FLAT_STEPS);
  assert.equal(one("GAS_MAX"), r.GAS_MAX);
});
