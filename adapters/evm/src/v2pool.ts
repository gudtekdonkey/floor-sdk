

import { quoteBookBuy, quoteBookSell } from "../vendor/sdk/src/math/v2/trade.ts";
import { bookFromEvmLaunchBook, evmPairId } from "../vendor/sdk/src/v2/evm.ts";
import { topicAddress } from "./abi.ts";
import { carriesNoRefund, isDustRefund } from "../vendor/sdk/src/v2/router.ts";
import type { Book } from "../vendor/sdk/src/math/v2/book.ts";
import { NATIVE, QUOTE_WRITTEN_TOPIC, gasFor, launchFromLog, routable, swapCall, toApiToken, toChainQuote } from "./protocol.ts";
import type { EthCall, LaunchMeta, Log } from "./protocol.ts";
import { dexLiquidityV2, readBook, readFeeCurve } from "./v2.ts";
import type { FeeCurve, LaunchBook } from "./v2.ts";
export type { FeeCurve } from "./v2.ts";

export const TOKEN_SCALE = 1_000_000_000_000n;

export const NO_CURVE: FeeCurve = { feeStartPpm: [0, 0, 0, 0, 0], sqrtFeeA: 0n, sqrtFeeB: 0n };

export const bookOf = (b: LaunchBook, curve: FeeCurve = NO_CURVE): Book => bookFromEvmLaunchBook(b, { curve });

export { isDustRefund };

export function amountOutV2(b: LaunchBook, isBuy: boolean, amountIn: bigint, curve: FeeCurve = NO_CURVE): bigint {
  return walkV2(b, isBuy, amountIn, curve).amountOut;
}

export function walkV2(b: LaunchBook, isBuy: boolean, amountIn: bigint, curve: FeeCurve = NO_CURVE): { amountOut: bigint; steps: number } {
  const book = bookOf(b, curve);
  try {
    if (isBuy) {
      const r = quoteBookBuy(book, amountIn);
      return { amountOut: carriesNoRefund(amountIn, r.refund, r.budgetHit) ? r.tokens * TOKEN_SCALE : 0n, steps: r.steps };
    }

    const units = amountIn / TOKEN_SCALE;
    if (units === 0n) return { amountOut: 0n, steps: 0 };
    const r = quoteBookSell(book, units);
    return { amountOut: r.unfilled > 0n ? 0n : r.out, steps: r.steps };
  } catch {
    return { amountOut: 0n, steps: 0 };
  }
}

export interface V2PoolState { book: LaunchBook; curve: FeeCurve }

export type OneInchPoolV2 = {
  id: string;
  address: string;
  tokens: [string, string];

  pairId: string;

  liquidityQuote: bigint;
  graduated: boolean;
};

export class OneInchFloorAmmV2 {
  readonly chainId: number;
  readonly floorAmm: string;
  private readonly call: EthCall;
  readonly launches = new Map<string, { meta: LaunchMeta; state: V2PoolState }>();

  readonly pending = new Map<string, LaunchMeta>();

  constructor(chainId: number, floorAmm: string, call: EthCall) {
    this.chainId = chainId;
    this.floorAmm = floorAmm.toLowerCase();
    this.call = call;
  }

  private key(id: bigint): string {
    return `floor-amm:${this.chainId}:${id}`;
  }

  async refresh(meta: LaunchMeta, blockNumber: number | "latest"): Promise<void> {
    const [book, curve] = await Promise.all([readBook(this.call, this.floorAmm, meta.id, blockNumber), readFeeCurve(this.call, this.floorAmm, meta.id, blockNumber)]);
    this.launches.set(this.key(meta.id), { meta, state: { book, curve } });
  }

  async onLogs(logs: Log[]): Promise<void> {
    for (const log of logs) {
      const born = launchFromLog(this.floorAmm, log);
      if (born) {
        if (await routable(this.call, this.floorAmm, born, log.blockNumber)) await this.refresh(born, log.blockNumber);
        else this.pending.set(this.key(born.id), born);
        continue;
      }
      if (log.address.toLowerCase() === this.floorAmm && log.topics[0] === QUOTE_WRITTEN_TOPIC && log.topics.length >= 2) {
        await this.onQuoteWritten(topicAddress(log.topics[1]), log.blockNumber);
        continue;
      }
      if (log.address.toLowerCase() !== this.floorAmm || log.topics.length < 2) continue;
      for (const v of this.launches.values()) if (BigInt(log.topics[1]) === v.meta.id) await this.refresh(v.meta, log.blockNumber);
    }
  }

  private async onQuoteWritten(quote: string, blockNumber: number): Promise<void> {
    for (const [k, meta] of [...this.pending]) {
      if (meta.quote !== quote) continue;
      if (await routable(this.call, this.floorAmm, meta, blockNumber)) {
        this.pending.delete(k);
        await this.refresh(meta, blockNumber);
      }
    }
  }

  pools(): OneInchPoolV2[] {
    return [...this.launches.entries()].map(([id, { meta, state }]) => ({
      id, address: this.floorAmm, tokens: [toApiToken(meta.quote), meta.token], pairId: evmPairId(this.floorAmm, meta.id), liquidityQuote: dexLiquidityV2(state.book), graduated: state.book.graduated,
    }));
  }

  private side(poolId: string, src: string, dst: string): { v: { meta: LaunchMeta; state: V2PoolState }; isBuy: boolean } {
    const v = this.launches.get(poolId);
    if (!v) throw new Error(`unknown pool ${poolId}`);
    const [s, d] = [toChainQuote(src), toChainQuote(dst)];
    if (s === v.meta.quote && d === v.meta.token) return { v, isBuy: true };
    if (s === v.meta.token && d === v.meta.quote) return { v, isBuy: false };
    throw new Error(`${poolId} does not trade ${src} → ${dst}`);
  }

  getReturn(poolId: string, src: string, dst: string, amountIn: bigint): { amountOut: bigint; gas: number } {
    const { v, isBuy } = this.side(poolId, src, dst);
    const { amountOut, steps } = walkV2(v.state.book, isBuy, amountIn, v.state.curve);
    return { amountOut, gas: gasFor(isBuy ? "buy" : "sell", steps) };
  }

  swap(poolId: string, src: string, dst: string, amountIn: bigint, minOut: bigint, recipient: string) {
    const { v, isBuy } = this.side(poolId, src, dst);
    return swapCall({ floorAmm: this.floorAmm, id: v.meta.id, isBuy, ethIn: toChainQuote(src) === NATIVE, amountIn, minOut, to: recipient, mode: "router" });
  }
}
