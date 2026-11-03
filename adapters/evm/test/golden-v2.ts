

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { FloorMathError } from "../vendor/sdk/src/math/index.ts";
import { BIDS_MAX, RANGES_MAX, emptyRange } from "../vendor/sdk/src/math/v2/book.ts";
import type { Book, Range } from "../vendor/sdk/src/math/v2/book.ts";
import { feeLegs, netFromGross } from "../vendor/sdk/src/math/v2/fees.ts";
import {
  flatMoneyForTokens, flatTokensForMoney, liquidityForMoney, liquidityForTokens, moneyBetween, sqrtAfterMoney,
  sqrtAfterTokens, sqrtOfRatio, tokensBetween,
} from "../vendor/sdk/src/math/v2/sqrt.ts";
import { circulatingV2, sBelow } from "../vendor/sdk/src/math/v2/supply.ts";
import { buy as sdkBuy, sell as sdkSell } from "../vendor/sdk/src/math/v2/trade.ts";
import { splitRowOfRowV2 } from "../vendor/sdk/src/math/v2/split.ts";
import type { SplitRow } from "../vendor/sdk/src/math/v2/split.ts";
import type { Supply } from "../vendor/sdk/src/math/v2/supply.ts";
import type { BookBuy, BookSell } from "../vendor/sdk/src/math/v2/trade.ts";

export type Row = Record<string, any>;
export type Section = "book" | "oracle/sqrt" | "oracle/fees" | "oracle/supply";
export const SECTIONS: Section[] = ["book", "oracle/sqrt", "oracle/fees", "oracle/supply"];
export interface Case { id: string; fn: string; state_in?: Row; args: Row; out: Row; state_out?: Row }

export const NOT_IN_MATH: Record<string, string> = {
  line_sqrt: "the line (L1b), not in the SDK's math/v2 yet",
  is_evm_dead: "sdk/src/v2/dead.ts (not replayed here)",
  is_solana_dead_owner: "sdk/src/v2/dead.ts (not replayed here)",
};

const ROOT = fileURLToPath(new URL("../fixtures/", import.meta.url));
const REL = "crates/floor-math/golden/v2";

export function loadGoldenV2(section: Section): { source: string; cases: Case[] } {
  const source = `${REL}/${section}.json`;
  return { source, cases: JSON.parse(readFileSync(`${ROOT}${source}`, "utf8")).cases };
}

const big = (v: unknown): bigint => BigInt(v as string);
const s = (v: bigint): string => v.toString();
const opt = (v: bigint | null): Row => (v === null ? { err: "Overflow" } : { v: s(v) });
const err = (e: unknown): Row => {
  if (e instanceof FloorMathError) return { err: e.code };
  throw e;
};

const rangeFrom = (r: Row): Range => ({
  sqrtA: big(r.sqrt_a), sqrtB: big(r.sqrt_b), tokens: big(r.tokens), askLeft: big(r.ask_left), back: big(r.back),
  place: big(r.place), thetaBps: r.theta_bps, psiBps: r.psi_bps, kind: r.kind, class: r.class,
  msCapPpm: Number(r.ms_cap_ppm ?? 0),
});
const rangeJson = (r: Range): Row => ({
  sqrt_a: s(r.sqrtA), sqrt_b: s(r.sqrtB), tokens: s(r.tokens), ask_left: s(r.askLeft), back: s(r.back), place: s(r.place),
  theta_bps: r.thetaBps, psi_bps: r.psiBps, kind: r.kind, class: r.class,

  ...(r.msCapPpm ? { ms_cap_ppm: r.msCapPpm } : {}),
});

export function bookFrom(b: Row): Book {
  const ranges = (b.ranges as Row[]).map(rangeFrom);
  while (ranges.length < RANGES_MAX) ranges.push(emptyRange());

  const fit = (a: string[], n: number): string[] => {
    if (a.length < n || a.slice(n).some((x) => x !== "0")) throw new Error(`golden book: band arrays (${a.length} for ${n})`);
    return a.slice(0, n);
  };
  const bandEdge = fit(b.band_edge, BIDS_MAX + 1), bandMoney = fit(b.band_money, BIDS_MAX), bandTokens = fit(b.band_tokens, BIDS_MAX);

  const need = <T>(k: string): T => {
    if (b[k] === undefined) throw new Error(`golden book: no ${k} (stale golden: regenerate golden/v2)`);
    return b[k] as T;
  };
  const bandF = fit(need("band_f"), BIDS_MAX), bandFClaim = fit(need("band_f_claim"), BIDS_MAX);
  return {
    sqrtP: big(b.sqrt_p), sqrtF: big(b.sqrt_f), sqrtLMax: big(b.sqrt_l_max), floor: big(b.floor), floorTokens: big(b.floor_tokens),
    nBands: b.n_bands, nC: need<number>("n_c"), bandEdge: bandEdge.map(big), bandMoney: bandMoney.map(big), bandTokens: bandTokens.map(big),
    bandF: bandF.map(big), bandFClaim: bandFClaim.map(big), ckAtF: big(need("ck_at_f")), ckFloorTokens: big(need("ck_floor_tokens")),
    feePpm: b.fee_ppm.map(Number), feeStartPpm: (b.fee_start_ppm ?? [0, 0, 0, 0, 0]).map(Number), sqrtFeeA: big(b.sqrt_fee_a ?? "0"),
    sqrtFeeB: big(b.sqrt_fee_b ?? "0"), graduated: b.graduated, nRanges: b.n_ranges, ranges,
  };
}

export function bookJson(b: Book): Row {
  return {
    sqrt_p: s(b.sqrtP), sqrt_f: s(b.sqrtF), sqrt_l_max: s(b.sqrtLMax), floor: s(b.floor), floor_tokens: s(b.floorTokens),
    n_bands: b.nBands, n_c: b.nC, band_edge: b.bandEdge.map(s), band_money: b.bandMoney.map(s), band_tokens: b.bandTokens.map(s),
    band_f: b.bandF.map(s), band_f_claim: b.bandFClaim.map(s), ck_at_f: s(b.ckAtF), ck_floor_tokens: s(b.ckFloorTokens), fee_ppm: b.feePpm.map(String), graduated: b.graduated,

    ...(b.sqrtFeeB ? { fee_start_ppm: b.feeStartPpm.map(String), sqrt_fee_a: s(b.sqrtFeeA), sqrt_fee_b: s(b.sqrtFeeB) } : {}),
    n_ranges: b.nRanges, ranges: b.ranges.slice(0, b.nRanges).map(rangeJson),
  };
}

export const buyJson = (r: BookBuy): Row => ({
  gross: s(r.gross), net: s(r.net), refund: s(r.refund), tokens: s(r.tokens), fees: r.fees.map(s), from_resale: s(r.fromResale),
  from_first: s(r.fromFirst), steps: r.steps, budget_hit: r.budgetHit, graduated_now: r.graduatedNow,
  sqrt_before: s(r.sqrtBefore), sqrt_after: s(r.sqrtAfter),
  split: { d_f: s(r.split.dF), ck_add: s(r.split.ckAdd), ck_take: s(r.split.ckTake), ms: s(r.split.ms), booked: r.split.booked.map(s), sqrt_f_new: s(r.split.sqrtFNew) },
  relay: relayJson(r.relay),
});

const relayJson = (l: BookSell["relay"]): Row => ({ n_c: l.nC, n_k: l.nK, laid_c: s(l.laidC), laid_k: s(l.laidK), at_f: s(l.atF) });
export const sellJson = (r: BookSell): Row => ({
  tokens_in: s(r.tokensIn), unfilled: s(r.unfilled), gross: s(r.gross), fees: r.fees.map(s), out: s(r.out), steps: r.steps,
  sqrt_before: s(r.sqrtBefore), sqrt_after: s(r.sqrtAfter), from_f: s(r.fromF), from_k: s(r.fromK), from_c: s(r.fromC),
  relay: relayJson(r.relay),
});

export interface TradeFns {
  buy: (b: Book, row: SplitRow, sup: Supply, gross: bigint, minTokens: bigint) => BookBuy;
  sell: (b: Book, row: SplitRow, sup: Supply, tokens: bigint, minOut: bigint) => BookSell;
}
const SDK_FNS: TradeFns = { buy: sdkBuy, sell: sdkSell };

const LAUNCH_PRESET: Record<string, string> = { "book-0049": "preset_v2-0001", "book-0050": "preset_v2-0001", "book-0053": "preset_v2-0003", "book-0054": "preset_v2-0003" };

export function bookSplitRow(id?: string): SplitRow {
  const pid = (id && LAUNCH_PRESET[id]) || "preset_v2-0002";
  const c = JSON.parse(readFileSync(`${ROOT}${REL}/preset_v2.json`, "utf8")).cases.find((x: Case) => x.id === pid);
  if (!c) throw new Error(`preset_v2.json: no ${pid}`);
  const r = c.state_in as Row;
  const n = (k: string): number => Number(r[k]);
  return splitRowOfRowV2({ phiBps: n("phi_bps"), ckBps: n("ck_target_bps"), msClass: n("ms_class"), flags: n("flags"), platformMsBps: n("platform_ms_bps"), floorMsBps: n("floor_ms_bps") });
}

export function caseSupply(a: Row): Supply {
  if (a.supply_live === undefined || a.supply_dead === undefined) throw new Error("golden trade case without supply_live / supply_dead");
  return { live: big(a.supply_live), dead: big(a.supply_dead) };
}

export function runCase(c: Case, fns: TradeFns = SDK_FNS): { out: Row; state_out?: Row } | null {
  const a = c.args;
  const val = (v: bigint | null): Row => (v === null ? { err: "Overflow" } : { value: s(v) });
  if (c.fn in NOT_IN_MATH) return null;
  switch (c.fn) {
    case "money_between":
      return { out: val(moneyBetween(big(a.l), big(a.a), big(a.b), a.round)) };
    case "tokens_between":
      return { out: val(tokensBetween(big(a.l), big(a.a), big(a.b), a.round)) };
    case "sqrt_after_money":
      return { out: val(sqrtAfterMoney(big(a.l), big(a.p), big(a.money))) };
    case "sqrt_after_tokens":
      return { out: val(sqrtAfterTokens(big(a.l), big(a.p), big(a.tokens))) };
    case "sqrt_of_ratio":
      return { out: val(sqrtOfRatio(big(a.money), big(a.tokens))) };
    case "flat_money_for_tokens":
      return { out: val(flatMoneyForTokens(big(a.s), big(a.tokens), a.round)) };
    case "flat_tokens_for_money":
      return { out: val(flatTokensForMoney(big(a.s), big(a.money), a.round)) };
    case "liquidity_for_tokens":
      return { out: val(liquidityForTokens(big(a.a), big(a.b), big(a.tokens))) };
    case "liquidity_for_money":
      return { out: val(liquidityForMoney(big(a.a), big(a.b), big(a.money))) };
    case "fee_legs":
      return { out: { fees: feeLegs(big(a.amount), (a.fee_ppm as string[]).map(Number)).map(s) } };
    case "book_from_gross": {
      const ppm = (a.fee_ppm as string[]).map(Number);
      const net = netFromGross(big(a.gross), ppm);
      return { out: { net: s(net), fees: feeLegs(net, ppm).map(s) } };
    }
    case "circulating":
      try {
        return { out: { s: s(circulatingV2(big(a.minted), big(a.burned), big(a.for_sale), big(a.bought_back), big(a.dead))) } };
      } catch (e) {
        return { out: err(e) };
      }
    case "s_below":
      return { out: { s_below: s(sBelow(big(a.s), big(a.x_c), big(a.x_k))) } };
    case "buy":
    case "sell": {
      const b = bookFrom(c.state_in as Row);
      let out: Row;
      try {
        const row = bookSplitRow(c.id), sup = caseSupply(a);
        out = c.fn === "buy" ? buyJson(fns.buy(b, row, sup, big(a.gross), big(a.min_tokens))) : sellJson(fns.sell(b, row, sup, big(a.tokens), big(a.min_out)));
      } catch (e) {
        out = err(e);
      }
      return { out, state_out: bookJson(b) };
    }
    default:
      throw new Error(`${c.id}: unknown fn ${c.fn} (add it here or to NOT_IN_MATH)`);
  }
}

const U128_MAX = (1n << 128n) - 1n;
function wantOut(o: Row): Row {
  const v = (o as { value?: unknown }).value;
  return typeof v === "string" && /^\d+$/.test(v) && BigInt(v) > U128_MAX ? { err: "Overflow" } : o;
}

export function replaySection(section: Section, fns: TradeFns = SDK_FNS): { source: string; ran: number; failed: string[]; notRun: Record<string, number> } {
  const { source, cases } = loadGoldenV2(section);
  const failed: string[] = [];
  const notRun: Record<string, number> = {};
  let ran = 0;
  for (const c of cases) {
    const got = runCase(c, fns);
    if (got === null) {
      notRun[c.fn] = (notRun[c.fn] ?? 0) + 1;
      continue;
    }
    ran++;
    const same = isDeepStrictEqual(got.out, wantOut(c.out)) && (c.state_out === undefined || isDeepStrictEqual(got.state_out, bookJson(bookFrom(c.state_out))));
    if (!same) failed.push(c.id);
  }
  return { source, ran, failed, notRun };
}
