

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EV, FN, OUT } from "../src/abi.ts";
import { FN_V2, OUT_V2 } from "../src/v2.ts";

const print = JSON.parse(readFileSync(new URL("../fixtures/sdk/test/fixtures/evm/v2_abi.json", import.meta.url), "utf8")) as Record<string, string | string[]>;
const lines = Object.entries(print).filter(([k]) => k !== "source").flatMap(([, v]) => v as string[]);
const fns = new Map<string, string>();
const evs = new Set<string>();
for (const l of lines) {
  const f = /^function (\w+\([^)]*\))(?: returns \((.*)\))?$/.exec(l);
  if (f) fns.set(f[1], f[2] ?? "");
  const e = /^event (\w+)\((.*)\)$/.exec(l);
  if (e) evs.add(`${e[1]}(${e[2].replaceAll(" indexed", "")})`);
}

const NOT_PRINTED_FN = new Set(["quoteEntry(address)"]);
const NOT_PRINTED_EV = new Set(["Graduated"]);

test("abi.ts FN / OUT and v2.ts FN_V2 / OUT_V2 equal solc's print", () => {
  const out = { ...OUT, ...OUT_V2 } as Record<string, readonly string[]>;
  for (const [k, sig] of [...Object.entries(FN), ...Object.entries(FN_V2)]) {
    if (NOT_PRINTED_FN.has(sig)) continue;
    assert.ok(fns.has(sig), `${sig} is not in the print`);
    if (out[k]) assert.equal(out[k].join(","), fns.get(sig), `${sig} returns`);
  }
});

test("abi.ts EV equals solc's print", () => {
  for (const [k, sig] of Object.entries(EV)) if (!NOT_PRINTED_EV.has(k)) assert.ok(evs.has(sig), `${sig} is not in the print`);
});

test("quoteEntry and Graduated equal the frozen interface's source", () => {
  const src = (p: string): string => readFileSync(new URL(`../fixtures/contracts/src/interfaces/${p}`, import.meta.url), "utf8").replace(/\/\/[^\n]*/g, "");
  const reg = /struct QuoteEntry \{([^}]*)\}/.exec(src("IRegistries.sol"))![1];
  const types = reg.split(";").map((s) => s.trim().split(/\s+/)[0]).filter(Boolean);
  assert.equal(OUT.quoteEntry[0], `(${types.join(",")})`);
  const g = /event Graduated\(([^)]*)\)/.exec(src("IFloorAmm.sol"))![1];
  assert.equal(EV.Graduated, `Graduated(${g.split(",").map((s) => s.trim().split(/\s+/)[0]).join(",")})`);
});

test("the v2 Trade the SPEC prints is the built one (topic from the print's signature)", async () => {
  const { eventTopic } = await import("../vendor/sdk/src/evm/abi.ts");
  const trade = [...evs].find((e) => e.startsWith("Trade("))!;
  const spec = readFileSync(new URL("../SPEC.md", import.meta.url), "utf8");
  assert.ok(spec.includes(eventTopic(trade)), `SPEC.md prints the Trade topic ${eventTopic(trade)}`);
});
