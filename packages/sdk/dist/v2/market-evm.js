

import { MULTICALL3_ADDRESS, aggregate3Call, decodeAggregate3Result } from "../evm/multicall.js";
import { decodeCirculatingOf, decodeLaunchBook, decodeLaunchFeeCurve, decodeLaunchStateSupply, decodeBidMoney, evmSettleInputsV2, viewCall } from "./evm.js";

import { launchDisplayV2 } from "../math/v2/display.js";

export const EVM_TOKEN_SCALE = 1_000_000_000_000n;

export const LAUNCH_READS_V2 = ["book", "state", "feeCurve", "circulating", "bidMoney"]         ;

const REQUIRED                               = new Set(["book", "state", "feeCurve"]);

export function launchReadCallsV2(floorAmm        , launchId        )                                   {
  const v = (data        )          => ({ to: floorAmm, data, value: 0n });
  return {
    book: v(viewCall("launchBook", [launchId])),
    state: v(viewCall("launchState", [launchId])),
    feeCurve: v(viewCall("launchFeeCurve", [launchId])),
    circulating: v(viewCall("circulatingOf", [launchId])),
    bidMoney: v(viewCall("bidMoney", [launchId])),
  };
}

export function launchMulticallV2(floorAmm        , launchId        , multicall         = MULTICALL3_ADDRESS)          {
  const c = launchReadCallsV2(floorAmm, launchId);
  return aggregate3Call(LAUNCH_READS_V2.map((k) => ({ target: c[k].to, allowFailure: !REQUIRED.has(k), callData: c[k].data })), multicall);
}

                                                                                                              

export function decodeLaunchReadsV2(r                                        , opts                                                   = {})              {
  if (r.book === null || r.state === null || r.feeCurve === null) throw new RangeError("launchBook / launchState / launchFeeCurve failed");
  const book = decodeLaunchBook(r.book);

  const supply = launchStateSupplyMath(decodeLaunchStateSupply(r.state).supply);
  const curve = decodeLaunchFeeCurve(r.feeCurve);
  const circulating = r.circulating === null ? null : decodeCirculatingOf(r.circulating);
  const math = opts.row !== undefined && circulating !== null ? evmSettleInputsV2(book, opts.row, { live: supply, dead: circulating.dead }, opts.msCapPpm, curve) : null;

  const displayBook = math?.book ?? evmSettleInputsV2(book, NO_ROW, { live: supply, dead: 0n }, opts.msCapPpm, curve).book;
  return {
    book, supply, curve, circulating, bidMoney: r.bidMoney === null ? null : decodeBidMoney(r.bidMoney),
    math, display: launchDisplayV2(displayBook, supply),
  };
}

export function launchStateSupplyMath(chain        )         {
  if (chain % EVM_TOKEN_SCALE !== 0n) throw new RangeError(`launchState.supply ${chain} is not a whole math unit (CHAIN = math × ${EVM_TOKEN_SCALE})`);
  return chain / EVM_TOKEN_SCALE;
}

const NO_ROW           = { phiBps: 0, ckBps: 0, wcBps: 0, cShareBps: 0, msClass: 0, platformMsBps: 0, floorMsBps: 0 };

export function decodeLaunchMulticallV2(data        , opts                                                   = {})              {
  const res = decodeAggregate3Result(data);
  if (res.length !== LAUNCH_READS_V2.length) throw new RangeError(`aggregate3: ${res.length} results, ${LAUNCH_READS_V2.length} calls`);
  const r = Object.fromEntries(LAUNCH_READS_V2.map((k, i) => [k, res[i].success ? res[i].returnData : null]))                                          ;
  return decodeLaunchReadsV2(r, opts);
}

export async function readLaunchV2(ethCall         , floorAmm        , launchId        , opts                                                                              = {})                       {
  if (opts.multicall !== null) return decodeLaunchMulticallV2(await ethCall(launchMulticallV2(floorAmm, launchId, opts.multicall ?? MULTICALL3_ADDRESS)), opts);
  const c = launchReadCallsV2(floorAmm, launchId);
  const got = await Promise.all(LAUNCH_READS_V2.map(async (k) => {
    try {
      return await ethCall(c[k]);
    } catch (e) {
      if (REQUIRED.has(k)) throw e;
      return null;
    }
  }));
  return decodeLaunchReadsV2(Object.fromEntries(LAUNCH_READS_V2.map((k, i) => [k, got[i]]))                                          , opts);
}
