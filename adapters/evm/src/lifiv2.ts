

import { MAX_SLIPPAGE_BPS, NATIVE, gasFor, minOutFor, onChainQuote, routable, swapCall, toApiToken, toChainQuote } from "./protocol.ts";
import { carriesNoRefund } from "../vendor/sdk/src/v2/router.ts";
import { readBook, readFeeCurve } from "./v2.ts";
import { TOKEN_SCALE, bookOf, walkV2 } from "./v2pool.ts";
import { buyLessIn, quoteBookBuy, quoteBookSell } from "../vendor/sdk/src/math/v2/trade.ts";
import type { EthCall, LaunchMeta } from "./protocol.ts";
import type { FeeCurve } from "./v2.ts";

export type LifiQuoteRequest = {
  chainId: number;
  fromToken: string;
  toToken: string;

  fromAmount: string;
  fromAddress: string;
  toAddress?: string;

  slippageBps?: number;
};

export type LifiQuote = {
  tool: "floor-amm";
  chainId: number;
  launchId: string;
  action: { fromToken: string; toToken: string; fromAmount: string; slippageBps: number };
  estimate: {
    toAmount: string;
    toAmountMin: string;

    approvalAddress: string | null;

    gasLimit: string;

    graduates: boolean;

    mathAmount: string;
    mathAgrees: boolean;
  };
  transactionRequest: { to: string; data: string; value: string; from: string };
};

export class LifiQuoteError extends Error {
  readonly code: "NO_LAUNCH" | "UNLISTED_QUOTE" | "WOULD_REFUND" | "NOTHING_OUT" | "BAD_SLIPPAGE";
  constructor(code: "NO_LAUNCH" | "UNLISTED_QUOTE" | "WOULD_REFUND" | "NOTHING_OUT" | "BAD_SLIPPAGE", detail: string) {
    super(`floor-amm: ${code}: ${detail}`);
    this.code = code;
    this.name = "LifiQuoteError";
  }
}

export type LifiContext = {
  floorAmm: string;
  call: EthCall;

  blockNumber: number;

  launchOf: (token: string) => LaunchMeta | undefined;

  routeUnlisted?: boolean;
};

export async function lifiQuoteV2(ctx: LifiContext, req: LifiQuoteRequest): Promise<LifiQuote> {
  const slippageBps = req.slippageBps ?? 50;
  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > MAX_SLIPPAGE_BPS)
    throw new LifiQuoteError("BAD_SLIPPAGE", `slippageBps ${slippageBps} is not a whole number in 0..=${MAX_SLIPPAGE_BPS}`);
  const [src, dst] = [toChainQuote(req.fromToken), toChainQuote(req.toToken)];
  let meta = ctx.launchOf(dst);
  let isBuy = true;
  if (!meta || meta.quote !== src) {
    meta = ctx.launchOf(src);
    isBuy = false;
    if (!meta || meta.quote !== dst) throw new LifiQuoteError("NO_LAUNCH", `${req.fromToken} → ${req.toToken}`);
  }
  if (!(await routable(ctx.call, ctx.floorAmm, meta, ctx.blockNumber, ctx.routeUnlisted)))
    throw new LifiQuoteError("UNLISTED_QUOTE", `launch ${meta.id} trades against ${meta.quote}, which is not on the quote registry`);
  const amount = BigInt(req.fromAmount);

  if (!isBuy && amount % TOKEN_SCALE !== 0n) throw new LifiQuoteError("WOULD_REFUND", `${amount % TOKEN_SCALE} of ${amount} is under one math unit (${TOKEN_SCALE})`);
  const [out, refund, budgetHit] = await onChainQuote(ctx.call, ctx.floorAmm, meta.id, isBuy, amount, ctx.blockNumber);

  if (!carriesNoRefund(amount, refund, budgetHit)) throw new LifiQuoteError("WOULD_REFUND", `${refund} of ${amount} would be refunded${budgetHit ? " (step budget hit)" : ""}`);
  if (out === 0n) throw new LifiQuoteError("NOTHING_OUT", `${amount}`);
  const [book, curve] = await Promise.all([readBook(ctx.call, ctx.floorAmm, meta.id, ctx.blockNumber), readFeeCurve(ctx.call, ctx.floorAmm, meta.id, ctx.blockNumber)]);
  const { amountOut: math, steps } = walkV2(book, isBuy, amount, curve);

  const unfilled = isBuy ? 0n : sellUnfilled(book, amount, curve);
  if (unfilled > 0n) throw new LifiQuoteError("WOULD_REFUND", `${unfilled * TOKEN_SCALE} of ${amount} would be handed back`);

  let minOut = isBuy ? 1n : minOutFor(out, slippageBps);
  const lessIn = isBuy ? buyLessIn(amount, slippageBps) : 0n;
  if (isBuy && lessIn === amount) minOut = out;
  else if (isBuy && lessIn > 0n) {
    const [m] = await onChainQuote(ctx.call, ctx.floorAmm, meta.id, true, lessIn, ctx.blockNumber);
    if (m > 0n) minOut = m;
  }
  const tx = swapCall({ floorAmm: ctx.floorAmm, id: meta.id, isBuy, ethIn: src === NATIVE, amountIn: amount, minOut, to: req.toAddress ?? req.fromAddress, mode: "direct" });
  return {
    tool: "floor-amm",
    chainId: req.chainId,
    launchId: meta.id.toString(),
    action: { fromToken: toApiToken(src), toToken: toApiToken(dst), fromAmount: req.fromAmount, slippageBps },
    estimate: {
      toAmount: out.toString(),
      toAmountMin: minOut.toString(),
      approvalAddress: tx.approve,
      gasLimit: String(gasFor(isBuy ? "buy" : "sell", steps)),
      graduates: isBuy && !book.graduated && graduatesNow(book, amount, curve),
      mathAmount: math.toString(),
      mathAgrees: math === out,
    },
    transactionRequest: { to: tx.to, data: tx.data, value: `0x${tx.value.toString(16)}`, from: req.fromAddress },
  };
}

function sellUnfilled(b: Parameters<typeof bookOf>[0], amount: bigint, curve: FeeCurve): bigint {
  try {
    return quoteBookSell(bookOf(b, curve), amount / TOKEN_SCALE).unfilled;
  } catch {
    return 0n;
  }
}

function graduatesNow(b: Parameters<typeof bookOf>[0], amount: bigint, curve: FeeCurve): boolean {
  try {
    return quoteBookBuy(bookOf(b, curve), amount).graduatedNow;
  } catch {
    return false;
  }
}

