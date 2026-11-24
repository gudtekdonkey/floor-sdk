

export const SEARCH_Q_MAX = 64;

export const SEARCH_PAGE_DEFAULT = 12;

export const SEARCH_PAGE_MAX = 100;

export const ADDRESS_PREFIX_MIN = 4;

export const SEARCH_SUBSTRING_MIN = 3;

export function searchFold(s        )         {
  return s.normalize("NFKC").toLowerCase().replace(/\s+/g, " ").trim();
}

export function searchNeedles(q        )           {
  const f = searchFold(q);
  if (!f.startsWith("$")) return f === "" ? [] : [f];
  const bare = f.slice(1).trim();
  return bare === "" ? [f] : [bare, f];
}

export function searchNeedle(q        )         {
  return searchNeedles(q)[0] ?? "";
}

export function searchTierFolded(name        , symbol        , token        , n        )                    {
  if (n === "") return null;
  if (symbol === n || name === n || token === n) return 0;
  if (symbol.startsWith(n)) return 1;
  if (name.startsWith(n) || name.includes(` ${n}`)) return 2;
  if (symbol.includes(n) || name.includes(n)) return 3;
  if (token.startsWith(n)) return 4;
  if ((name + symbol + token).includes(n)) return 5;
  return null;
}

export function searchServed(tier            , needle        )          {
  const n = [...needle].length;
  if (tier <= 2) return true;
  if (tier === 3) return n >= SEARCH_SUBSTRING_MIN;
  if (tier === 4) return n >= ADDRESS_PREFIX_MIN;
  return false;
}

export function searchTier(l               , q        )                    {
  const name = searchFold(l.name ?? ""), symbol = searchFold(l.symbol ?? ""), token = searchFold(l.token);
  let best                    = null;
  for (const n of searchNeedles(q)) {
    const t = searchTierFolded(name, symbol, token, n);
    if (t !== null && (best === null || t < best)) best = t;
  }
  return best;
}

export function searchServedTier(l               , q        )                    {
  const name = searchFold(l.name ?? ""), symbol = searchFold(l.symbol ?? ""), token = searchFold(l.token);
  let best                    = null;
  for (const n of searchNeedles(q)) {
    const t = searchTierFolded(name, symbol, token, n);
    if (t !== null && searchServed(t, n) && (best === null || t < best)) best = t;
  }
  return best;
}

const SOL = "So11111111111111111111111111111111111111112";
const EVM = "0xabCDeF0123456789AbcdEf0123456789aBCDEF01";

export const SEARCH_PARITY_CORPUS                                                                                          = [

  ["Doge Wif Hat", "WIF", SOL, "wif", 0],
  ["Doge Wif Hat", "WIF", SOL, "WIF", 0],
  ["Doge Wif Hat", "WIF", SOL, "$wif", 0],
  ["Doge Wif Hat", "WIF", SOL, "  doge   wif  hat ", 0],
  ["Doge Wif Hat", "WIF", SOL, SOL, 0],
  ["Doge Wif Hat", "WIF", EVM, EVM.toLowerCase(), 0],
  ["Doge Wif Hat", "WIF", EVM, EVM.toUpperCase().replace("0X", "0x"), 0],
  ["ＦＬＯＯＲ", "ＦＬＲ", SOL, "flr", 0],
  ["Café", "CAFE", SOL, "café", 0],
  ["Café", "CF", SOL, "café", 0],
  ["Ⅻ Club", "XII", SOL, "xii", 0],
  ["Dog", "DOG", SOL, "＄dog", 0],

  ["Moon Rocket", "MOONR", SOL, "moo", 1],
  ["Moon Rocket", "MOONR", SOL, "$MOO", 1],
  ["Moon Rocket", "MOONR", SOL, "ｍｏｏ", 1],

  ["Moon Rocket", "MR", SOL, "moon", 2],
  ["Moon Rocket", "MR", SOL, "rock", 2],
  ["Moon  Rocket", "MR", SOL, "moon rock", 2],
  ["Moon\tRocket", "MR", SOL, "moon r", 2],
  ["The Big Dog", "TBD", SOL, "big d", 2],

  ["Moon Rocket", "MR", SOL, "ocke", 3],
  ["Moon Rocket", "XMOONX", SOL, "oon", 3],
  ["Pepe", "PEPE", SOL, "ep", 3],
  ["Ｐｅｐｅ Coin", "PC", SOL, "pe c", 3],

  ["Moon Rocket", "MR", SOL, "so1", 4],
  ["Moon Rocket", "MR", SOL, "so11", 4],
  ["Moon Rocket", "MR", EVM, "0xab", 4],
  ["Moon Rocket", "MR", EVM, "0xA", 4],

  ["Moon Rocket", "MR", SOL, "1111", 5],
  ["Moon Rocket", "MR", EVM, "cdef01", 5],
  ["Moon Rocket", "MR", SOL, "rocketmr", 5],
  ["Moon Rocket", "MR", SOL, "mrso", 5],

  [null, "ZED", SOL, "zed", 0],
  [null, null, SOL, "zed", null],
  ["Zed", null, SOL, "ze", 2],
  [null, null, SOL, "so11", 4],

  ["Moon Rocket", "MR", SOL, "venus", null],
  ["Moon Rocket", "MR", SOL, "", null],
  ["Moon Rocket", "MR", SOL, "   ", null],
  ["Moon Rocket", "MR", SOL, "$", null],

  ["$$ Bills", "BILL", SOL, "$$", 2],
  ["Bills", "$BILL", SOL, "$$bill", 0],

  ["Big Money", "BM", SOL, "$ money", 2],

  ["$WIF Coin", "$WIF", SOL, "$wif", 0],
  ["Doge Wif Hat", "WIF", SOL, "$wif", 0],
  ["$1 Coin", "ONE", SOL, "$1", 2],
  ["Cash $ Money", "CASH", SOL, "$", 2],
  ["Cash $ Money", "CASH", SOL, "  $  ", 2],
];
