

import { test } from "node:test";
import assert from "node:assert/strict";
import { isDeepStrictEqual } from "node:util";
import { NOT_IN_MATH, SECTIONS, bookFrom, bookJson, buyJson, loadGoldenV2, replaySection, sellJson } from "./golden-v2.ts";
import type { Row } from "./golden-v2.ts";
import { bookBuy, bookSell, quoteBookBuy, quoteBookSell } from "../src/v2.ts";
import type { Book } from "../src/v2.ts";

for (const section of SECTIONS) {
  test(`golden/v2 ${section}: every case bit for bit`, () => {
    const r = replaySection(section);
    assert.ok(r.ran > 0, `${r.source}: no case ran`);
    for (const fn of Object.keys(r.notRun)) assert.ok(fn in NOT_IN_MATH, fn);
    assert.deepEqual(r.failed, [], `${r.source}: ${r.failed.length} / ${r.ran} differ: ${r.failed.join(" ")}`);
  });
}

const traderSide = (o: Row): Row => {
  const { split: _s, relay: _r, ...rest } = o;
  return rest;
};

test("golden/v2 book: the EVM reference's walk (bookBuy / bookSell) pays every trader what the trade pays", () => {
  let ran = 0;
  for (const c of loadGoldenV2("book").cases) {
    if (c.fn !== "buy" && c.fn !== "sell") continue;
    const b = bookFrom(c.state_in!);
    let out: Row;
    try {
      out = c.fn === "buy" ? buyJson(bookBuy(b, BigInt(c.args.gross), BigInt(c.args.min_tokens))) : sellJson(bookSell(b, BigInt(c.args.tokens), BigInt(c.args.min_out)));
    } catch (e) {
      out = { err: (e as { code?: string }).code };
    }
    assert.deepEqual(traderSide(out), traderSide(c.out), `${c.id}: the walk's receipt`);
    ran++;
  }
  assert.ok(ran > 0, "no trade case ran");
});

test("golden/v2 book: a quote equals the trade (min 0) and leaves the book untouched", () => {
  for (const c of loadGoldenV2("book").cases) {
    if (c.fn !== "buy" && c.fn !== "sell") continue;
    const b: Book = bookFrom(c.state_in!);
    const before = bookJson(b);
    const trade = bookFrom(c.state_in!);
    const run = (f: () => object): object => {
      try {
        return f();
      } catch (e) {
        return { err: (e as { code?: string }).code };
      }
    };
    const q = run(() => (c.fn === "buy" ? buyJson(quoteBookBuy(b, BigInt(c.args.gross))) : sellJson(quoteBookSell(b, BigInt(c.args.tokens)))));
    const t = run(() => (c.fn === "buy" ? buyJson(bookBuy(trade, BigInt(c.args.gross), 0n)) : sellJson(bookSell(trade, BigInt(c.args.tokens), 0n))));
    assert.ok(isDeepStrictEqual(bookJson(b), before), `${c.id}: a quote moved the book`);
    assert.deepEqual(q, t, `${c.id}: quote ≠ trade`);
  }
});
