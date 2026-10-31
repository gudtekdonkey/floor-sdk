

import { v2Fail } from "./error.ts";
import { pow } from "./pow.ts";
import { build } from "./ask.ts";
import { DEFAULT_FEE_PPM } from "./fees.ts";
import { BPS } from "../index.ts";

export const MAX_STACKS = 9;
export const MAX_BANDS = 4;

export { BPS } from "../index.ts";

export const CLASS_TEAM = 0;
export const CLASS_HOLDER = 1;
export type MilestoneClass = 0 | 1;

export interface StackRow {

  level: bigint;

  milestone: bigint;

  spreadBps: number | null;
}

export interface Row {

  id: number;
  name: string;
  phiBps: number;

  wBps: number;

  bands: number[];
  thetaBps: number;

  depthX100: number;

  climbStepX100: number;

  spreadBps: number;
  tailBps: number;
  psiTBps: number;
  bondingRaise: bigint;
  launchCap: bigint;
  stacks: StackRow[];
  class: MilestoneClass;

  platformMsBps: number;
  floorMsBps: number;

  teamBuyBps: number;
  teamSlotMaxBps: number;

  ckBps: number;

  feeCurve: FeeCurveRow | null;
}

export interface FeeCurveRow {
  startPpm: number[];
  endPpm: number[];
  fromUsd: bigint;
  toUsd: bigint;
}

export const FEE_FROM_GRAD = 0n;

export const FEE_TO_TAIL = 0n;

export const FEE_REL_ONE = 1n << 32n;

export function feeCurve(startPpm: readonly number[], endPpm: readonly number[]): FeeCurveRow {
  return { startPpm: [...startPpm], endPpm: [...endPpm], fromUsd: FEE_FROM_GRAD, toUsd: FEE_TO_TAIL };
}

const WAGMI_FEES = [[10_000, 0, 2_000, 500, 1_000], [2_000, 0, 500, 100, 1_000]] as const;
const HODL_FEES = [[2_000, 0, 2_000, 500, 9_000], [1_000, 0, 500, 100, 2_000]] as const;
const WAGMI_GENESIS_FEES = [[6_000, 0, 2_000, 500, 0], [6_000, 0, 500, 100, 0]] as const;
const MOONSHOT_FEES = [[2_000, 9_000, 2_000, 500, 0], [1_000, 2_000, 500, 100, 0]] as const;

const FLOOR_GENESIS_FEES = [[3_000, 0, 2_000, 0, 0], [2_000, 0, 500, 0, 0]] as const;

export function feeKnotRel(usd: bigint, launchCap: bigint): bigint {
  const x = Math.sqrt(Number(usd) / Number(launchCap)) * Number(FEE_REL_ONE);
  if (!Number.isFinite(x) || x < 1 || x >= 2 ** 64) v2Fail("RowInvalid");
  return f64ToU128(x);
}

export function rowFeeFields(r: Row): { feePpm: number[]; feeStartPpm: number[]; feeARel: bigint; feeBRel: bigint } {
  if (!r.feeCurve) return { feePpm: [...DEFAULT_FEE_PPM], feeStartPpm: [0, 0, 0, 0, 0], feeARel: 0n, feeBRel: 0n };
  const c = r.feeCurve;
  const from = c.fromUsd === FEE_FROM_GRAD ? r.launchCap : c.fromUsd;
  const to = c.toUsd === FEE_TO_TAIL ? BigInt(Math.round(build(r).tailStart)) : c.toUsd;
  return { feePpm: [...c.endPpm], feeStartPpm: [...c.startPpm], feeARel: feeKnotRel(from, r.launchCap), feeBRel: feeKnotRel(to, r.launchCap) };
}

export function cloneRow(r: Row): Row {
  return {
    ...r,
    bands: [...r.bands],
    ckBps: r.ckBps,
    feeCurve: r.feeCurve ? { ...r.feeCurve, startPpm: [...r.feeCurve.startPpm], endPpm: [...r.feeCurve.endPpm] } : null,
    stacks: r.stacks.map((s) => ({ ...s })),
  };
}

export const rowWcBps = (r: Row): number => r.bands.reduce((a, b) => a + b, 0);

export const rowDepth = (r: Row): number => r.depthX100 / 100.0;

export const rowTeamBuyBounds = (r: Row): [bigint, bigint] => [0n, (r.bondingRaise * BigInt(r.teamBuyBps)) / BPS];

export function rowValidate(r: Row): void {
  const bad = r.id === 0
    || r.id === 2
    || r.phiBps === 0
    || r.phiBps >= 10_000
    || r.wBps >= 10_000
    || r.thetaBps > 10_000
    || r.tailBps >= 10_000
    || r.psiTBps === 0
    || r.psiTBps > 10_000
    || r.depthX100 < 100
    || r.climbStepX100 <= 100
    || r.spreadBps > 10_000
    || r.bands.length === 0
    || r.bands.length > MAX_BANDS
    || rowWcBps(r) >= 10_000
    || r.stacks.length === 0
    || r.stacks.length > MAX_STACKS
    || r.bondingRaise === 0n
    || r.launchCap <= r.bondingRaise
    || r.platformMsBps + r.floorMsBps > 10_000
    || r.teamBuyBps > 10_000
    || r.teamSlotMaxBps > 9_500
    || r.stacks.some((s) => s.level === 0n || (s.spreadBps !== null && s.spreadBps > 10_000))
    || r.stacks.some((s, i) => i > 0 && s.level <= r.stacks[i - 1]!.level);
  if (bad) v2Fail("RowInvalid");
}

export const DEPTH_X100 = 101;
export const STEP_PHI10_X100 = 150;
export const STEP_PHI50_X100 = 120;

export function r1DepthX100(phiBps: number, stepX100: number): number | null {
  const phi = phiBps / 1e4;
  const r = stepX100 / 100.0;
  const u = Math.sqrt(r);
  if (phi * u >= 1.0) return null;

  const need = (phi * (r - 1.0)) / (u * (1.0 - phi * u));
  for (let d = 100; d <= 400; d++) {
    if (pow(r, d / 100.0 / (1.0 / phi - 1.0)) - 1.0 >= need) return d;
  }
  return null;
}

const st = (level: bigint, milestone: bigint): StackRow => ({ level, milestone, spreadBps: null });

function base(id: number, name: string): Row {
  return {
    id,
    name,
    phiBps: 1_000,

    wBps: 2_500,
    bands: [300],
    thetaBps: 0,
    depthX100: DEPTH_X100,
    climbStepX100: STEP_PHI10_X100,
    spreadBps: 0,
    tailBps: 4_000,
    psiTBps: 8_000,
    bondingRaise: 0n,
    launchCap: 0n,
    stacks: [],
    class: CLASS_TEAM,
    platformMsBps: 0,
    floorMsBps: 0,
    teamBuyBps: 10_000,
    teamSlotMaxBps: 0,
    ckBps: 0,
    feeCurve: null,
  };
}

export const CK_TARGET_CIRC_BPS = 500;

export const CK_TARGET_DEFAULT = CK_TARGET_CIRC_BPS;

export const MOONSHOT_CK_TARGET = 700;

export function withCkTarget(row: Row, ckBps: number): Row {
  return { ...cloneRow(row), ckBps };
}

export function moonshot(): Row {
  return {
    ...base(3, "moonshot-10"),
    thetaBps: 1_000,
    spreadBps: 10_000,
    bondingRaise: 2_000n,
    launchCap: 100_000n,

    tailBps: 200,
    stacks: [
      st(250_000n, 12_000n),
      st(450_000n, 20_000n),
      st(500_000n, 60_000n),
      st(1_000_000n, 150_000n),
      st(2_500_000n, 250_000n),

      st(5_000_000n, 700_000n),
      st(9_000_000n, 1_500_000n),

      { level: 75_000_000n, milestone: 1_280_000n, spreadBps: 144 },
    ],
    platformMsBps: 1_000,
    floorMsBps: 1_000,
    teamSlotMaxBps: 6_000,
    ckBps: MOONSHOT_CK_TARGET,
    feeCurve: feeCurve(...MOONSHOT_FEES),
  };
}

export const WAGMI_SPEC_MILESTONES: readonly bigint[] = [11_550_000n];

export const WAGMI_END_USD = 177_730_000n;

export const WAGMI_THETA_OPTIONS: readonly (readonly [number, bigint])[] = [
  [250, 1_000_000n],
  [100, 1_000_000n],
  [50, 1_000_000n],
  [0, 1_000_000n],
];

export const WAGMI_THETA_BPS = 250;

export function wagmiMsScalePpm(thetaBps: number): bigint | null {
  const o = WAGMI_THETA_OPTIONS.find(([t]) => t === thetaBps);
  return o ? o[1] : null;
}

export const WAGMI_MS_SCALE_PPM: bigint = wagmiMsScalePpm(WAGMI_THETA_BPS)!;

export const STEP_WAGMI_X100 = 109;

function wagmiBase(): Row {
  return {
    ...base(1, "wagmi-75"),
    phiBps: 7_500,

    wBps: 2_000,
    climbStepX100: STEP_WAGMI_X100,

    tailBps: 1_000,
    bondingRaise: 1_000n,
    launchCap: 10_000_000n,
    stacks: [st(WAGMI_END_USD, WAGMI_SPEC_MILESTONES[0])],

    spreadBps: 144,
    class: CLASS_HOLDER,

    platformMsBps: 5_000,
    floorMsBps: 5_000,
    ckBps: CK_TARGET_DEFAULT,
    feeCurve: feeCurve(...WAGMI_FEES),
  };
}

export function wagmiAt(thetaBps: number): Row | null {
  const k = wagmiMsScalePpm(thetaBps);
  if (k === null) return null;
  const r = wagmiBase();
  r.thetaBps = thetaBps;
  r.stacks = r.stacks.map((s, i) => ({ ...s, milestone: (WAGMI_SPEC_MILESTONES[i] * k) / 1_000_000n }));
  return r;
}

export function wagmi(): Row {
  return wagmiAt(WAGMI_THETA_BPS)!;
}

export const FLOOR_GENESIS_PHI_BPS = 5_000;

export function floorGenesis(): Row {
  const phiBps = FLOOR_GENESIS_PHI_BPS;
  return {
    ...moonshot(),
    id: 4,
    name: "floor-genesis",

    ckBps: CK_TARGET_DEFAULT,
    phiBps,
    climbStepX100: STEP_PHI50_X100,
    depthX100: Math.max(r1DepthX100(phiBps, STEP_PHI50_X100) ?? 400, DEPTH_X100),
    bondingRaise: 2_000n,
    launchCap: 5_000_000n,
    teamBuyBps: 0,
    teamSlotMaxBps: 0,
    feeCurve: feeCurve(...FLOOR_GENESIS_FEES),

    stacks: [
      { level: 20_000_000n, milestone: 125_000n, spreadBps: 1_000 },
      { level: 90_000_000n, milestone: 1_125_000n, spreadBps: 1_000 },
      { level: 500_000_000n, milestone: 102_400_000n, spreadBps: 1_000 },
    ],
  };
}

export function hodl(): Row {
  return {
    ...moonshot(),
    id: 6,
    name: "hodl",

    ckBps: CK_TARGET_DEFAULT,
    phiBps: 3_000,
    climbStepX100: STEP_PHI50_X100,
    depthX100: 125,
    launchCap: 2_000_000n,
    bondingRaise: 12_600n,
    class: CLASS_HOLDER,
    teamSlotMaxBps: 0,
    feeCurve: feeCurve(...HODL_FEES),

    stacks: [
      { level: 50_000_000n, milestone: 1_000_000n, spreadBps: 144 },
      { level: 100_000_000n, milestone: 2_000_000n, spreadBps: 144 },
      { level: 250_000_000n, milestone: 5_000_000n, spreadBps: 144 },
      { level: 500_000_000n, milestone: 198_961_000n, spreadBps: 144 },
    ],
  };
}

export function floorGenesisPhiStudy(phiBps: number): Row {
  return {
    ...base(4, "floor-genesis"),
    phiBps,
    climbStepX100: phiBps <= 1_000 ? STEP_PHI10_X100 : STEP_PHI50_X100,
    depthX100: Math.max(r1DepthX100(phiBps, phiBps <= 1_000 ? STEP_PHI10_X100 : STEP_PHI50_X100) ?? 400, DEPTH_X100),
    bondingRaise: 20_000n,
    launchCap: 800_000n,
    stacks: [
      st(1_000_000n, 10_000n),
      st(5_000_000n, 50_000n),
      st(10_000_000n, 100_000n),
      st(50_000_000n, 500_000n),
      st(100_000_000n, 1_000_000n),
      st(1_000_000_000n, 10_000_000n),
    ],
    teamBuyBps: 0,
    teamSlotMaxBps: 0,
    ckBps: CK_TARGET_DEFAULT,
  };
}

export function f64ToU128(x: number): bigint {
  if (!(x > 0)) return 0n;
  if (x >= 2 ** 128) return (1n << 128n) - 1n;
  return BigInt(Math.trunc(x));
}

export const WAGMI_GENESIS_CAP_USD = 75_530_000n;

export const WAGMI_GENESIS_KEEP_USD = 1_711_410n;

export const WAGMI_GENESIS_UNUSED_MAX = 1e-4;

export function wagmiGenesisAt(raise: bigint, target: bigint): Row {
  const w = wagmi();
  return {
    ...w,
    id: 5,
    name: "wagmi-genesis",
    launchCap: WAGMI_GENESIS_CAP_USD,
    bondingRaise: raise,
    teamBuyBps: 0,
    teamSlotMaxBps: 0,
    feeCurve: feeCurve(...WAGMI_GENESIS_FEES),
    stacks: w.stacks.map((s, i) => (i === 0 ? { ...s, milestone: target } : s)),
  };
}

export function wagmiGenesisTarget(raise: bigint): bigint {
  const unused = (t: bigint) => {
    try {
      const a = build(wagmiGenesisAt(raise, t));
      return a;
    } catch {
      return undefined;
    }
  };
  const a0 = unused(0n);
  if (!a0 || !(a0.supplyExcess <= 0) || a0.stacks[0].skipped) return 0n;
  let lo = 0n;
  let hi = 1_000_000_000n;
  while (hi - lo > 1n) {
    const m = (lo + hi) / 2n;
    const a = unused(m);
    if (a && a.unusedShare > WAGMI_GENESIS_UNUSED_MAX) lo = m;
    else hi = m;
  }
  return (lo / 1_000n) * 1_000n;
}

export function wagmiGenesis(overflow: bigint): Row {
  const raise = overflow < WAGMI_GENESIS_KEEP_USD ? overflow : WAGMI_GENESIS_KEEP_USD;
  return wagmiGenesisAt(raise, wagmiGenesisTarget(raise));
}
