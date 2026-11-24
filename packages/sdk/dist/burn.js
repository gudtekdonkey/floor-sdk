

export const BURN_SCENE_BPS = 5_000n;

export const BURN_SCENE_MIN_SUPPLY_BPS = 10n;

export const BURN_SCENE_COOLDOWN_SECS = 3_600;

export function burnPlaysScene(b                )          {
  if (b.role === 0 || b.burned <= 0n || b.balanceBefore <= 0n || b.sOutBefore <= 0n) return false;
  if (b.burned * 10_000n <= b.balanceBefore * BURN_SCENE_BPS) return false;
  if (b.burned * 10_000n < b.sOutBefore * BURN_SCENE_MIN_SUPPLY_BPS) return false;
  return b.lastSceneAt == null || b.now - b.lastSceneAt >= BURN_SCENE_COOLDOWN_SECS;
}

export function planBurn(amount        , teamBuyPart        , wallet        )                                              {
  if (amount <= 0n) throw new Error("planBurn: amount must be > 0");
  if (amount > teamBuyPart + wallet) throw new Error("planBurn: more than the balance");
  const fromTeamBuy = amount < teamBuyPart ? amount : teamBuyPart;
  return { fromTeamBuy, fromWallet: amount - fromTeamBuy };
}
