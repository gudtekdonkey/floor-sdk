

export function isDustRefund(gross: bigint, refund: bigint, budgetHit: boolean): boolean {
  return refund > 0n && !budgetHit && refund * 1_000_000n <= gross;
}

export function carriesNoRefund(gross: bigint, refund: bigint, budgetHit: boolean): boolean {
  return refund === 0n || isDustRefund(gross, refund, budgetHit);
}

export const GAS_FLAT = { buy: 700_000, sell: 750_000 } as const;

export const GAS_PER_STEP = { buy: 40_000, sell: 40_000 } as const;

export const GAS_FLAT_STEPS = 4;

export const GAS_MAX = 1_500_000;

export function gasFor(side: "buy" | "sell", steps: number): number {
  const extra = Math.max(0, Math.trunc(steps) - GAS_FLAT_STEPS);
  return Math.min(GAS_MAX, GAS_FLAT[side] + extra * GAS_PER_STEP[side]);
}
