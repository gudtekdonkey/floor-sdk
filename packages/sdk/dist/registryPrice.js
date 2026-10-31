

export const REGISTRY_PRICE_MAX_AGE_S = 86_400n;

export class StaleRegistryPriceError extends Error {
           ageS        ;
  constructor(ageS        ) {
    super(`the quote's registry price is ${ageS} s old; a create takes one at most ${REGISTRY_PRICE_MAX_AGE_S} s (24 h) old`);
    this.name = "RegistryPriceStale";
    this.ageS = ageS;
  }
}

export const registryPriceAge = (updatedAt        , now        )         => (now > updatedAt ? now - updatedAt : 0n);

export const registryPriceFresh = (updatedAt        , now        )          => registryPriceAge(updatedAt, now) <= REGISTRY_PRICE_MAX_AGE_S;

export function freshRegistryPrice(row               , now        )         {
  if (row.usdCentsX64 <= 0n) throw new RangeError("the registry row has no price");
  const age = registryPriceAge(row.updatedAt, now);
  if (age > REGISTRY_PRICE_MAX_AGE_S) throw new StaleRegistryPriceError(age);
  return row.usdCentsX64;
}
