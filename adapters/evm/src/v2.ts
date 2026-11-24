

import { calldata, decode } from "./abi.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import type { EthCall } from "./protocol.ts";
import { evmSignature } from "../vendor/sdk/src/generated/index.ts";
import { V2_VIEWS, decodeLaunchBook, decodeLaunchFeeCurve } from "../vendor/sdk/src/v2/evm.ts";
import type { BookRangeV2, FeeCurveViewV2, LaunchBookV2 } from "../vendor/sdk/src/v2/evm.ts";
import { dexLiquidityOfParts } from "../vendor/sdk/src/math/v2/display.ts";

export { onChainQuote } from "./protocol.ts";

export { bookBuy, bookSell, quoteBookBuy, quoteBookSell } from "../vendor/sdk/src/math/v2/trade.ts";
export type { BookBuy, BookSell } from "../vendor/sdk/src/math/v2/trade.ts";
export type { Book } from "../vendor/sdk/src/math/v2/book.ts";

export const RANGES_MAX = 48;

export const BANDS_MAX = 24;

const FN_V2_NAMES = ["launchBook", "launchFeeCurve", "circulatingOf", "quoteBuy", "quoteSell", "buy", "sell"] as const;
export const FN_V2 = Object.fromEntries(FN_V2_NAMES.map((n) => [n, evmSignature(n)])) as Record<(typeof FN_V2_NAMES)[number], string>;

export const OUT_V2 = {
  launchBook: V2_VIEWS.launchBook.ret,

  launchFeeCurve: V2_VIEWS.launchFeeCurve.ret,
  circulatingOf: ["uint256", "uint256", "uint256"],
} as const;

export type LaunchBook = LaunchBookV2;
export type BookRange = BookRangeV2;

function checkBook(b: LaunchBookV2): void {
  if (b.nRanges > RANGES_MAX) throw new Error(`launchBook: nRanges ${b.nRanges} > ${RANGES_MAX}`);
  const len = b.bandMoney.length;
  if (b.nBands > BANDS_MAX || b.nBands > len) throw new Error(`launchBook: nBands ${b.nBands} > ${Math.min(BANDS_MAX, len)}`);
  if (b.bandTokens.length !== len || b.bandEdge.length !== len + 1 || b.bandF.length !== len || b.bandFClaim.length !== len) {
    throw new Error(`launchBook: band arrays ${len} / ${b.bandTokens.length} / ${b.bandEdge.length} / ${b.bandF.length} / ${b.bandFClaim.length}`);
  }
}

export type FeeCurve = FeeCurveViewV2

export async function readFeeCurve(call: EthCall, floorAmm: string, id: bigint, blockNumber: number | "latest"): Promise<FeeCurve> {
  return decodeLaunchFeeCurve(await call(floorAmm, calldata(FN_V2.launchFeeCurve, [id]), blockNumber));
}

export async function readBook(call: EthCall, floorAmm: string, id: bigint, blockNumber: number | "latest"): Promise<LaunchBook> {
  const data = await call(floorAmm, calldata(FN_V2.launchBook, [id]), blockNumber);
  const b = decodeLaunchBook(data);
  checkBook(b);
  return b;
}

export async function readCirculating(call: EthCall, floorAmm: string, id: bigint, blockNumber: number | "latest"): Promise<[bigint, bigint, bigint]> {
  return decode(OUT_V2.circulatingOf, await call(floorAmm, calldata(FN_V2.circulatingOf, [id]), blockNumber)) as [bigint, bigint, bigint];
}

export function dexLiquidityV2(b: LaunchBook): bigint {

  let bidMoney = b.f;
  for (let i = 0; i < b.nBands; i++) bidMoney += b.bandMoney[i];
  let place = 0n;
  for (let i = 0; i < b.nRanges; i++) place += b.ranges[i].place;
  return dexLiquidityOfParts(bidMoney, place);
}
