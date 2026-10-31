

import { approveCall, buyCall, sellCall } from "../../evm/floor/calls.js";
import { NATIVE_QUOTE } from "../../evm/floor/iface.js";
import { checkFloorHops } from "../../evm/router.js";
import { FLOOR_ERROR_CODES, FloorError, decodeEvmProgramError, errorCodeOf } from "../../errors.js";

import { FloorMathError } from "../../math/index.js";
import { nextBuyPriceX64 } from "../../math/v2/display.js";
import { feeTotal } from "../../math/v2/fees.js";
import { priceImpactV2, quoteSellImpactV2 } from "../../math/v2/receipt.js";
import { sCirc } from "../../math/v2/supply.js";

import { claimV2Call, evmSettleInputsV2, quoteBuyEvmV2 } from "../../v2/evm.js";
import { buyMinTokens } from "../../math/v2/trade.js";

import { decodeLaunchStateV2, launchStateCallV2 } from "../../v2/evmReads.js";

import { EVM_TOKEN_SCALE, readLaunchV2 } from "../../v2/market-evm.js";

import { fillsNothing } from "../../v2/route.js";
import { MAX_SLIPPAGE_BPS, minOutAt } from "../../v2/press.js";
import { DEFAULT_SLIPPAGE_BPS } from "../types.js";

const ADDRESS = /^0x[0-9a-fA-F]{40}$/;
const CODES                      = new Set(FLOOR_ERROR_CODES);

const bad = (message        )             => new FloorError("BadArgument", message, { source: "sdk" });
const legs = (f                   )          => ({ backing: f[0], team: f[1], platform: f[2], floor: f[3], holder: f[4], total: feeTotal(f) });
const bps = (ppm               )                => (ppm === null ? null : Number(ppm / 100n));

function slippageOf(s                    )         {
  const v = s ?? DEFAULT_SLIPPAGE_BPS;
  if (!Number.isInteger(v) || v < 0 || v > MAX_SLIPPAGE_BPS) throw bad(`slippageBps in 0..=${MAX_SLIPPAGE_BPS}`);
  return v;
}

function asFloorError(e         )             {
  if (e instanceof FloorError) return e;
  if (e instanceof FloorMathError) {
    const code                 = CODES.has(e.code) ? (e.code                  ) : errorCodeOf(e.code) === "Unknown" ? "MathFailed" : errorCodeOf(e.code);
    return new FloorError(code, e.message, { source: "sdk", name: e.code, mathError: e.code, cause: e });
  }
  if (e instanceof RangeError || e instanceof TypeError) return new FloorError("BadArgument", e.message, { source: "sdk", cause: e });
  return new FloorError("Unknown", e instanceof Error ? e.message : String(e), { source: "sdk", cause: e });
}
async function guard   (f                  )             {
  try {
    return await f();
  } catch (e) {
    throw asFloorError(e);
  }
}

export function createEvmClient(opts                           )                 {
  const chain = opts?.chain;
  if (chain?.family !== "evm") throw bad("createEvmClient takes an EVM chain (family \"evm\")");
  const { chainId, floorAmm } = chain;
  try {
    checkFloorHops(chainId, 1);
  } catch (e) {
    throw asFloorError(e);
  }
  if (typeof floorAmm !== "string" || !ADDRESS.test(floorAmm)) throw bad(`floorAmm ${String(floorAmm)}: a 20-byte 0x address`);
  const transport = opts.transport;
  if (typeof transport?.rpc?.call !== "function") throw bad("transport.rpc.call: the caller's eth_call");

  const call = async (c         )                  => {
    try {
      return await transport.rpc.call(c);
    } catch (e) {
      throw decodeEvmProgramError(e) ?? new FloorError("Transport", `eth_call to ${c.to} failed`, { source: "sdk", cause: e });
    }
  };
  const tx = (calls           )        => ({ family: "evm", chainId, calls });
  const launchIdOf = (launch        )         => {
    if (typeof launch !== "bigint" || launch <= 0n) throw bad("launch: an EVM launch is its launchId, a bigint > 0");
    return launch;
  };

  const caps = new Map                           ();
  const rows = new Map                  ();
  const readLaunch = (launchId        , o                                                   = {})                       => {
    if (o.msCapPpm) caps.set(launchId, o.msCapPpm);
    if (o.row) rows.set(launchId, o.row);
    const msCapPpm = o.msCapPpm ?? caps.get(launchId);
    const row = o.row ?? rows.get(launchId);
    return readLaunchV2(call, floorAmm, launchId, { ...(chain.multicall !== undefined ? { multicall: chain.multicall } : {}), ...(msCapPpm ? { msCapPpm } : {}), ...(row ? { row } : {}) });
  };

  const mathOf = (launch             , launchId        )                                   => {
    if (launch.math) return launch.math;
    if (!rows.has(launchId)) throw bad(`launch ${launchId}: no split row (no EVM view returns it): readLaunch(launchId, { row }) first`);
    throw new FloorError("Transport", `launch ${launchId}: circulatingOf failed (the dead count)`, { source: "sdk" });
  };
  const states = new Map                                   ();
  const launchState = (launchId        )                            => {
    let s = states.get(launchId);
    if (!s) {
      s = call({ to: floorAmm, data: launchStateCallV2(launchId), value: 0n }).then(decodeLaunchStateV2);
      s.catch(() => states.delete(launchId));
      states.set(launchId, s);
    }
    return s;
  };

  return {
    family: "evm",
    chain,
    readLaunch: (launchId, o) => guard(() => readLaunch(launchIdOf(launchId), o)),
    launchState: (launchId) => guard(() => launchState(launchIdOf(launchId))),

    quoteBuy: (p) => guard(async () => {
      const launchId = launchIdOf(p.launch);
      const slippageBps = slippageOf(p.slippageBps);
      if (p.amountIn <= 0n) throw bad("amountIn must be > 0");
      const launch = await readLaunch(launchId);
      const math = mathOf(launch, launchId);
      const msCapPpm = caps.get(launchId);
      const v = { book: launch.book, row: math.row, sup: math.sup };
      const o = { curve: launch.curve, ...(msCapPpm ? { msCapPpm } : {}) };
      const r = quoteBuyEvmV2(v, p.amountIn, o);

      let minTokens = 0n;
      try {
        const s = evmSettleInputsV2(v.book, v.row, v.sup, o.msCapPpm, o.curve);
        minTokens = buyMinTokens(s.book, s.row, s.sup, p.amountIn, slippageBps);
      } catch (e) {
        if (!fillsNothing(e)) throw e;
      }
      const impact = priceImpactV2(nextBuyPriceX64(math.book), r.buy.net, r.buy.tokens, "buy");
      const q        = {
        family: "evm", chainId, side: "buy", launch: launchId.toString(), amountIn: p.amountIn, amountOut: r.buy.tokens * EVM_TOKEN_SCALE,
        minOut: minTokens > 0n ? minTokens * EVM_TOKEN_SCALE : 1n, slippageBps, fees: legs(r.buy.fees), priceImpactBps: bps(impact.ppm),
        refund: r.buy.refund, graduates: r.buy.graduatedNow,
      };
      return q;
    }),

    quoteSell: (p) => guard(async () => {
      const launchId = launchIdOf(p.launch);
      const slippageBps = slippageOf(p.slippageBps);
      if (p.tokensIn <= 0n) throw bad("tokensIn must be > 0");
      const tokens = p.tokensIn / EVM_TOKEN_SCALE;
      if (tokens === 0n) throw bad("tokensIn must be at least one TOKEN_SCALE (10^12)");
      const launch = await readLaunch(launchId);
      const { book, sup } = mathOf(launch, launchId);
      const { quote: s, impact } = quoteSellImpactV2(book, tokens, sCirc(book, sup));
      const less = s.out > 0n ? minOutAt(s.out, slippageBps) : 0n;
      const q        = {
        family: "evm", chainId, side: "sell", launch: launchId.toString(), amountIn: tokens * EVM_TOKEN_SCALE, amountOut: s.out,
        minOut: less > 0n ? less : 1n, slippageBps, fees: legs(s.fees), priceImpactBps: bps(impact.ppm),
        refund: s.unfilled * EVM_TOKEN_SCALE, graduates: false,
      };
      return q;
    }),

    buildBuy: (p) => guard(async () => {
      const launchId = launchIdOf(p.launch);
      const { quote } = await launchState(launchId);
      const buy = buyCall({ floorAmm, quote, launchId, amountIn: p.amountIn, minTokensOut: p.minOut, to: p.recipient ?? p.trader, ...(p.deadline !== undefined ? { deadline: p.deadline } : {}) });
      return tx(quote.toLowerCase() === NATIVE_QUOTE ? [buy] : [approveCall(quote, floorAmm, p.amountIn), buy]);
    }),

    buildSell: (p) => guard(async () => {
      const launchId = launchIdOf(p.launch);
      const sell = sellCall({ floorAmm, launchId, tokensIn: p.tokensIn, minQuoteOut: p.minOut, to: p.recipient ?? p.trader, ...(p.deadline !== undefined ? { deadline: p.deadline } : {}) });
      const { token } = await launchState(launchId);
      return tx([approveCall(token, floorAmm, p.tokensIn), sell]);
    }),

    claim: (p) => guard(async () => tx([claimV2Call({ floorAmm, wallet: p.wallet, types: p.types, quotes: [p.quote], launchIds: p.launches.map(launchIdOf) })])),

    send: (t) => guard(async () => {
      const send = transport.send;
      if (!send) throw new FloorError("NoSend", "transport.send: none given (the SDK never signs)", { source: "sdk" });
      if (t?.family !== "evm" || t.chainId !== chainId) throw bad(`send: a tx for chain ${chainId}`);
      const hashes           = [];
      for (const c of t.calls) {
        try {
          hashes.push(await send(c));
        } catch (e) {
          throw decodeEvmProgramError(e) ?? new FloorError("Transport", `send to ${c.to} failed`, { source: "sdk", cause: e });
        }
      }
      return hashes;
    }),

    decodeError: decodeEvmProgramError,
  };
}
