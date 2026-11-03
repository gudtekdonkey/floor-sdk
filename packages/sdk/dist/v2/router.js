

export function isDustRefund(gross        , refund        , budgetHit         )          {
  return refund > 0n && !budgetHit && refund * 1_000_000n <= gross;
}

export function carriesNoRefund(gross        , refund        , budgetHit         )          {
  return refund === 0n || isDustRefund(gross, refund, budgetHit);
}

export const GAS_FLAT = { buy: 700_000, sell: 750_000 }         ;

export const GAS_PER_STEP = { buy: 40_000, sell: 40_000 }         ;

export const GAS_FLAT_STEPS = 4;

export const GAS_MAX = 1_500_000;

export function gasFor(side                , steps        )         {
  const extra = Math.max(0, Math.trunc(steps) - GAS_FLAT_STEPS);
  return Math.min(GAS_MAX, GAS_FLAT[side] + extra * GAS_PER_STEP[side]);
}
