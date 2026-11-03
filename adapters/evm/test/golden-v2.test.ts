

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { bookFrom, buyJson } from "./golden-v2.ts";
import type { Case, Row } from "./golden-v2.ts";
import { bookBuy } from "../src/v2.ts";

const climb: Case[] = JSON.parse(readFileSync(fileURLToPath(new URL("../fixtures/crates/floor-math/golden/v2/climb.json", import.meta.url)), "utf8")).cases;

test("golden/v2 climb: the EVM walk (bookBuy) pays every trader what the trade pays, on all 85 buys", () => {
  assert.equal(climb.length, 85);
  const trader = (o: Row): Row => {
    const { split: _s, relay: _r, ...rest } = o;
    return rest;
  };
  for (const c of climb) {
    const b = bookFrom(c.state_in!);
    assert.deepEqual(trader(buyJson(bookBuy(b, BigInt(c.args.gross), BigInt(c.args.min_tokens)))), trader(c.out), c.id);
  }
});
