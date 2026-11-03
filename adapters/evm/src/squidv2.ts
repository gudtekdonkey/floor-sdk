

import { MAX_SLIPPAGE_BPS, minOutFor, onChainQuote, routable } from "./protocol.ts";
import { readBook, readFeeCurve } from "./v2.ts";
import { TOKEN_SCALE, bookOf } from "./v2pool.ts";
import { carriesNoRefund } from "../vendor/sdk/src/v2/router.ts";
import { buyLessIn, quoteBookSell } from "../vendor/sdk/src/math/v2/trade.ts";
import { buyHookErc20, buyHookNative, sellHook } from "../vendor/sdk/src/evm/squid.ts";
import type { SquidHook } from "../vendor/sdk/src/evm/squid.ts";
import type { EthCall, LaunchMeta } from "./protocol.ts";

export const SQUID_DEFAULT_SLIPPAGE_BPS = 300;

export type SquidErrorCode = "NO_INTEGRATOR_ID" | "NO_LAUNCH" | "UNLISTED_QUOTE" | "WOULD_REFUND" | "NOTHING_OUT" | "BAD_SLIPPAGE" | "BAD_REQUEST";

export class SquidPlanError extends Error {
  readonly code: SquidErrorCode;
  constructor(code: SquidErrorCode, detail: string) {
    super(`floor-amm: ${code}: ${detail}`);
    this.code = code;
    this.name = "SquidPlanError";
  }
}

export type SquidContext = {
  floorAmm: string;
  call: EthCall;
  blockNumber: number;

  launchOf: (token: string) => LaunchMeta | undefined;
  routeUnlisted?: boolean;
};

type Base = {
  chainId: number | string;

  multicall: string;
  launchToken: string;
  user: string;
  deadline: bigint;

  integratorId: string;
  slippageBps?: number;
};

export type SquidBuyRequest = Base & {

  quoteAmount: bigint;
  quoteAmountMin: bigint;

  native?: { zap: string; swapData: string; minQuoteIn: bigint };
};

export type SquidSellRequest = Base & { tokensIn: bigint; proceedsTo?: string };

export type SquidPlan = {
  hook: SquidHook;
  headers: { "x-integrator-id": string };
  launchId: string;
  expectedOut: string;
  minOut: string;
};

function front(ctx: SquidContext, r: Base): { meta: LaunchMeta; slippageBps: number } {
  if (!r.integratorId || /\s/.test(r.integratorId)) throw new SquidPlanError("NO_INTEGRATOR_ID", "Squid answers nothing without an integrator id");
  const slippageBps = r.slippageBps ?? SQUID_DEFAULT_SLIPPAGE_BPS;
  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > MAX_SLIPPAGE_BPS) throw new SquidPlanError("BAD_SLIPPAGE", `${slippageBps}`);
  const meta = ctx.launchOf(r.launchToken.toLowerCase());
  if (!meta) throw new SquidPlanError("NO_LAUNCH", r.launchToken);
  return { meta, slippageBps };
}

async function listed(ctx: SquidContext, meta: LaunchMeta): Promise<void> {
  if (!(await routable(ctx.call, ctx.floorAmm, meta, ctx.blockNumber, ctx.routeUnlisted)))
    throw new SquidPlanError("UNLISTED_QUOTE", `launch ${meta.id} trades against ${meta.quote}, which is not on the quote registry`);
}

export async function planSquidBuy(ctx: SquidContext, r: SquidBuyRequest): Promise<SquidPlan> {
  const { meta, slippageBps } = front(ctx, r);
  await listed(ctx, meta);
  if (r.quoteAmount <= 0n || r.quoteAmountMin <= 0n || r.quoteAmountMin > r.quoteAmount) throw new SquidPlanError("BAD_REQUEST", "0 < quoteAmountMin ≤ quoteAmount");

  const [out, refund, hit] = await onChainQuote(ctx.call, ctx.floorAmm, meta.id, true, r.quoteAmount, ctx.blockNumber, r.user);

  if (!carriesNoRefund(r.quoteAmount, refund, hit)) throw new SquidPlanError("WOULD_REFUND", `${refund} of ${r.quoteAmount} would be refunded`);
  if (out === 0n) throw new SquidPlanError("NOTHING_OUT", `${r.quoteAmount}`);

  const floorIn = buyLessIn(r.quoteAmountMin, slippageBps);
  const [low, lowRefund, lowHit] = await onChainQuote(ctx.call, ctx.floorAmm, meta.id, true, floorIn > 0n ? floorIn : r.quoteAmountMin, ctx.blockNumber, r.user);
  if (low === 0n || !carriesNoRefund(floorIn > 0n ? floorIn : r.quoteAmountMin, lowRefund, lowHit)) throw new SquidPlanError("WOULD_REFUND", `the bridge's minimum ${r.quoteAmountMin} does not fill whole`);
  const minOut = low;
  const common = { chainId: r.chainId, multicall: r.multicall, floorAmm: ctx.floorAmm, launchId: meta.id, user: r.user, deadline: r.deadline };
  const hook = r.native
    ? buyHookNative({ ...common, zap: r.native.zap, swapData: r.native.swapData, minQuoteIn: r.native.minQuoteIn, minTokensOut: minOut })
    : buyHookErc20({ ...common, quote: meta.quote, minTokensOut: minOut });
  return { hook, headers: { "x-integrator-id": r.integratorId }, launchId: meta.id.toString(), expectedOut: out.toString(), minOut: minOut.toString() };
}

export async function planSquidSell(ctx: SquidContext, r: SquidSellRequest): Promise<SquidPlan> {
  const { meta, slippageBps } = front(ctx, r);
  await listed(ctx, meta);
  if (r.tokensIn % TOKEN_SCALE !== 0n) throw new SquidPlanError("WOULD_REFUND", `${r.tokensIn % TOKEN_SCALE} of ${r.tokensIn} is under one math unit`);
  const [out] = await onChainQuote(ctx.call, ctx.floorAmm, meta.id, false, r.tokensIn, ctx.blockNumber, r.user);
  if (out === 0n) throw new SquidPlanError("NOTHING_OUT", `${r.tokensIn}`);

  const [book, curve] = await Promise.all([readBook(ctx.call, ctx.floorAmm, meta.id, ctx.blockNumber), readFeeCurve(ctx.call, ctx.floorAmm, meta.id, ctx.blockNumber)]);
  let unfilled = 0n;
  try {
    unfilled = quoteBookSell(bookOf(book, curve), r.tokensIn / TOKEN_SCALE).unfilled;
  } catch {

  }
  if (unfilled > 0n) throw new SquidPlanError("WOULD_REFUND", `${unfilled * TOKEN_SCALE} of ${r.tokensIn} would be handed back`);
  const minOut = minOutFor(out, slippageBps);
  const hook = sellHook({ chainId: r.chainId, multicall: r.multicall, floorAmm: ctx.floorAmm, launchId: meta.id, user: r.user, deadline: r.deadline, token: meta.token, tokensIn: r.tokensIn, minQuoteOut: minOut, proceedsTo: r.proceedsTo });
  return { hook, headers: { "x-integrator-id": r.integratorId }, launchId: meta.id.toString(), expectedOut: out.toString(), minOut: minOut.toString() };
}
