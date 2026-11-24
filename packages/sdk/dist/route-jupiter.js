

import { WSOL_MINT } from "./solana/pda.js";
import { SOL_NATIVE } from "./route/graph.js";
import { isWrapHop } from "./route/path.js";

import { jupiterSwapInstructionsBody } from "./route/edge-jupiter.js";

export const JUPITER_API = "https://lite-api.jup.ag/swap/v1";

export function jupiterQuoteUrl(p                                                                                                )         {
  if (p.lamports <= 0n) throw new RangeError("lamports must be > 0");
  if (!Number.isInteger(p.slippageBps) || p.slippageBps < 0 || p.slippageBps > 10_000) throw new RangeError("slippageBps in 0..10,000");
  const q = new URLSearchParams({ inputMint: p.inputMint ?? WSOL_MINT, outputMint: p.quoteMint, amount: p.lamports.toString(), slippageBps: String(p.slippageBps), swapMode: "ExactIn" });
  return `${p.api ?? JUPITER_API}/quote?${q.toString()}`;
}

const jupMint = (c        )         => (c === SOL_NATIVE ? WSOL_MINT : c);

export function jupiterEdgeQuoter(fetchJson                                   , opts                   = {})             {
  return async ({ from, to, amountIn, slippageBps }) => {
    const raw = (await fetchJson(jupiterQuoteUrl({ inputMint: jupMint(from), quoteMint: jupMint(to), lamports: amountIn, slippageBps, api: opts.api })))                ;
    return { inAmount: BigInt(raw.inAmount), outAmount: BigInt(raw.outAmount), minOut: BigInt(raw.otherAmountThreshold), raw };
  };
}

export function jupiterSwapRequests(q            , trader        , opts                   = {})                                               {
  const out                                               = [];
  q.hops.forEach((h, i) => {
    if (h.hop.kind !== "edge" || isWrapHop(h.hop)) return;
    if (!h.edge) throw new RangeError(`hop ${i} has no edge quote`);
    out.push({ hop: i, url: `${opts.api ?? JUPITER_API}/swap-instructions`, body: jupiterSwapInstructionsBody({ quote: h.edge.raw                , trader }) });
  });
  return out;
}
