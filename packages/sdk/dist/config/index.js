

import { CHAIN_ROWS, PROGRAM_IDS } from "./generated.js";

export { PROGRAM_IDS };

import { CHAIN_REGISTRY, CHAIN_SLUGS, EVM_CHAIN_SLUGS } from "./registry.gen.js";
export { CHAIN_REGISTRY, CHAIN_SLUGS, EVM_CHAIN_SLUGS };

                             

export const CHAINS                      = CHAIN_ROWS                                  ;

export function chainByName(name        )                       {
  return CHAINS.find((c) => c.name === name);
}

export function chainById(chainId        , soak = false)                       {
  return CHAINS.find((c) => c.family === "evm" && (soak ? c.soak.chainId : c.chainId) === chainId);
}

export function dayOneChains()                      {
  return CHAINS.filter((c) => c.v2DayOne);
}

export function quotesOf(name        , withDeferred = false)                      {
  return (chainByName(name)?.quotes ?? []).filter((q) => withDeferred || !q.deferred);
}

export function findQuote(name        , address        )                       {
  const evm = chainByName(name)?.family === "evm";
  const a = evm ? address.toLowerCase() : address;
  return quotesOf(name, true).find((q) => (evm ? q.address.toLowerCase() : q.address) === a);
}

export function quoteDecimals(name        , address        )         {
  const q = findQuote(name, address);
  if (!q) throw new RangeError(`config: ${address} is not a quote on ${name}`);
  return q.decimals;
}

export function explorerUrl(name        , kind                  , id        , soak = false)         {
  const c = chainByName(name);
  if (!c) throw new RangeError(`config: unknown chain ${name}`);
  const e = c.explorer;
  const base = soak ? e.soak.url : e.url;
  const path = kind === "tx" ? (e.tx ?? "/tx/") : (e.address ?? "/address/");
  return `${base}${path}${id}${soak ? (e.soak.query ?? "") : ""}`;
}
