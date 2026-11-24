

import { beginCell } from "./cell.js";

import { addr } from "./addresses.js";
import { bodyOf, sig512Cell } from "./messages.js";

export const TON_PRICE_WINDOW_S = 86_400;

export const TON_PRICE_AHEAD_CAP_S = 87_000;

export const TON_PRICE_WRITE_VALUE = 20_000_000n;

const U128 = (1n << 128n) - 1n;
const U32 = (1n << 32n) - 1n;

function checked(usdCentsX64        , validUntil                 )                              {
  if (typeof usdCentsX64 !== "bigint" || usdCentsX64 < 0n || usdCentsX64 > U128) throw new RangeError("usdCentsX64 out of uint128 range");
  if (typeof validUntil === "number" && !Number.isInteger(validUntil)) throw new RangeError("validUntil out of uint32 range");
  const vu = BigInt(validUntil);
  if (vu < 0n || vu > U32) throw new RangeError("validUntil out of uint32 range");
  return { usd: usdCentsX64, vu };
}

export function tonPriceCell(a              )       {
  const { usd, vu } = checked(a.usdCentsX64, a.validUntil);

  return beginCell().storeAddress(addr(a.factory)).storeAddress(a.quote === null ? null : addr(a.quote)).storeUint(usd, 128).storeUint(vu, 32).endCell();
}

export function tonPriceDigest(a              )             {
  return tonPriceCell(a).hash();
}

export function tonSetQuotePriceBody(a                                                                                                                          )       {
  const { usd, vu } = checked(a.usdCentsX64, a.validUntil);
  const sig = sig512Cell(a.signature);
  return bodyOf("set_quote_price", a.queryId ?? 0n).storeAddress(a.quote === null ? null : addr(a.quote)).storeUint(usd, 128).storeUint(vu, 32).storeRef(sig).endCell();
}

export function buildTonSetQuotePrice(chain                           , a                                                                                                                                          )        {
  if (a.usdCentsX64 === 0n) throw new RangeError("usdCentsX64 is zero: every create would refuse this price");
  const value = a.value ?? TON_PRICE_WRITE_VALUE;
  if (value <= 0n) throw new RangeError("value must be positive");
  const body = tonSetQuotePriceBody(a);
  return { family: "ton", messages: [{ to: addr(chain.factory), value, bounce: true, body }] };
}

export function tonQuotePriceFresh(p                                                      , nowS                 )          {
  return p.usdCentsX64 > 0n && BigInt(nowS) <= BigInt(p.validUntil);
}
