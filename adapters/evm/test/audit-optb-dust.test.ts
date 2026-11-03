

import { test } from "node:test";
import assert from "node:assert/strict";
import { bookFrom, loadGoldenV2 } from "./golden-v2.ts";
import type { Book } from "../vendor/sdk/src/math/v2/book.ts";
import type { Case } from "./golden-v2.ts";
import { TOKEN_SCALE, amountOutV2, isDustRefund } from "../src/v2pool.ts";
import type { LaunchBook } from "../src/v2.ts";

const big = (x: unknown): bigint => BigInt(x as string);

function viewOf(b: Book): LaunchBook {
  return {
    sqrtP: b.sqrtP, sqrtF: b.sqrtF, f: b.floor, bandMoney: [...b.bandMoney], nRanges: b.nRanges,
    ranges: b.ranges.slice(0, b.nRanges).map((r) => ({ sqrtA: r.sqrtA, sqrtB: r.sqrtB, tokens: r.tokens, askLeft: r.askLeft, back: r.back, place: r.place, thetaBps: r.thetaBps, psiBps: r.psiBps, kind: r.kind, class: r.class })),
    sqrtLMax: b.sqrtLMax, floorTokens: b.floorTokens, bandTokens: [...b.bandTokens], bandEdge: [...b.bandEdge],
    nBands: b.nBands, nC: b.nC, bandF: [...b.bandF], bandFClaim: [...b.bandFClaim], ckAtF: b.ckAtF, ckFloorTokens: b.ckFloorTokens,
    graduated: b.graduated, feePpm: [...b.feePpm],
  };
}
const curveOf = (b: Book) => ({ feeStartPpm: b.feeStartPpm, sqrtFeeA: b.sqrtFeeA, sqrtFeeB: b.sqrtFeeB });

const walks: Case[] = loadGoldenV2("book").cases.filter(
  (c) => (c.fn === "buy" || c.fn === "sell") && c.state_in && big(c.fn === "buy" ? c.args.min_tokens : c.args.min_out) === 0n,
);

test("a sell of whole units + a sub-unit rest prices the golden sell of the whole units (sellFromBalance's fill)", () => {
  let n = 0, priced = 0;
  for (const c of walks) {
    if (c.fn !== "sell") continue;
    const b = bookFrom(c.state_in!);
    const fill = c.out.err !== undefined || big(c.out.unfilled) > 0n ? 0n : big(c.out.out);
    const whole = big(c.args.tokens) * TOKEN_SCALE;
    for (const rest of [0n, 1n, 987_654_321n, TOKEN_SCALE - 1n]) {
      assert.equal(amountOutV2(viewOf(b), false, whole + rest, curveOf(b)), fill, `${c.id} + ${rest}`);
      n++;
    }
    if (fill > 0n) priced++;

    assert.equal(amountOutV2(viewOf(b), false, TOKEN_SCALE - 1n, curveOf(b)), 0n, c.id);
  }
  assert.ok(n >= 20 && priced >= 5, `${n} sells, ${priced} priced`);
});

test("a golden buy filled whole but for a dust refund prices its tokens; a larger refund still 0", () => {
  let dust = 0, refused = 0;
  for (const c of walks) {
    if (c.fn !== "buy" || c.out.err !== undefined) continue;
    const b = bookFrom(c.state_in!);
    const gross = big(c.args.gross), refund = big(c.out.refund);
    const got = amountOutV2(viewOf(b), true, gross, curveOf(b));
    if (refund === 0n) { assert.equal(got, big(c.out.tokens) * TOKEN_SCALE, c.id); continue; }
    if (!c.out.budget_hit && refund * 1_000_000n <= gross) {

      assert.equal(got, big(c.out.tokens) * TOKEN_SCALE, `${c.id}: dust ${refund} of ${gross}`);
      dust++;
    } else {
      assert.equal(got, 0n, `${c.id}: refund ${refund} of ${gross}`);
      refused++;
    }
  }

  assert.ok(dust >= 6 && refused >= 1, `${dust} dust, ${refused} refused`);
});

test("isDustRefund is ≤ 1 ppm of gross with no step budget hit", () => {
  assert.equal(isDustRefund(1_000_000n, 1n, false), true);
  assert.equal(isDustRefund(999_999n, 1n, false), false);
  assert.equal(isDustRefund(10n ** 30n, 4n, true), false);
  assert.equal(isDustRefund(10n ** 30n, 0n, false), false);
});
