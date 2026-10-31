

export function lockVested(amount        , start        , cliff        , end        , now        , scale = 1n)         {
  if (now < cliff) return 0n;
  if (now >= end) return amount;
  return ((amount / scale) * (now - start)) / (end - start) * scale;
}

export function lockClaimable(l                                                                                , now        , scale = 1n)         {
  const v = lockVested(l.amount, l.start, l.cliff, l.end, now, scale);
  return v > l.claimed ? v - l.claimed : 0n;
}

export function lockScheduleOk(amount        , start        , cliff        , end        , now        )          {
  return amount > 0n && start <= cliff && cliff <= end && end > now;
}
