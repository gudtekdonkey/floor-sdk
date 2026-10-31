

export const SOL_NATIVE = "SOL";

export const EVM_NATIVE = "0x0000000000000000000000000000000000000000";

export class NoRouteError extends Error {
           reason               ;
  constructor(reason               , detail        ) {
    super(`NoRoute (${reason}): ${detail}`);
    this.name = "NoRouteError";
    this.reason = reason;
  }
}

export const coinKey = (a        )         => (a.startsWith("0x") ? a.toLowerCase() : a);

export const MAX_QUOTE_CHAIN = 8;

export async function quoteGraph(p                                                                                                                  )                      {
  const g             = { pools: new Map(), edge: new Set(p.edge.map(coinKey)) };
  const seen = new Set        ();
  for (const t of p.tokens) {
    let coin = t;
    for (let depth = 0; ; depth++) {
      const k = coinKey(coin);
      if (g.edge.has(k) || seen.has(k)) break;
      if (depth > MAX_QUOTE_CHAIN) throw new NoRouteError("QuoteCycle", `${t}'s quote chain passes ${MAX_QUOTE_CHAIN} pools`);
      seen.add(k);
      const pool = await p.readPool(coin);
      if (pool === null) break;
      if (pool.chain !== p.chain) throw new NoRouteError("Mixed", `${coin} is a ${pool.chain} launch on a ${p.chain} graph`);
      g.pools.set(k, pool);
      coin = pool.quote;
    }
  }
  return g;
}

export function quoteChain(g            , token        )                                  {
  const pools         = [];
  let coin = token;
  for (;;) {
    const k = coinKey(coin);
    const pool = g.edge.has(k) ? undefined : g.pools.get(k);
    if (!pool) return { pools, base: coin };
    if (pools.length >= MAX_QUOTE_CHAIN || pools.includes(pool)) throw new NoRouteError("QuoteCycle", `${token}'s quote chain does not end`);
    pools.push(pool);
    coin = pool.quote;
  }
}
