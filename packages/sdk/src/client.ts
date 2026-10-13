import { chainKey, type ChainRef } from './chain.js';
import type { FloorChainAdapter } from './adapter.js';
import type { Amount, Launch, LaunchRef, Quote, Side, TradeParams, UnsignedTx } from './types.js';

export class FloorSdkError extends Error {
  constructor(readonly code: 'no_adapter' | 'bad_params', message: string) {
    super(message);
    this.name = 'FloorSdkError';
  }
}

/** One entry point over any number of chain adapters. It routes each call by the launch's chain. */
export class FloorClient {
  readonly #byChain = new Map<string, FloorChainAdapter>();

  constructor(adapters: readonly FloorChainAdapter[]) {
    for (const a of adapters) for (const c of a.chains) {
      const k = chainKey(c);
      if (this.#byChain.has(k)) throw new FloorSdkError('bad_params', `two adapters serve ${k}`);
      this.#byChain.set(k, a);
    }
  }

  adapterFor(chain: ChainRef): FloorChainAdapter {
    const a = this.#byChain.get(chainKey(chain));
    if (!a) throw new FloorSdkError('no_adapter', `no adapter serves ${chainKey(chain)}`);
    return a;
  }

  getLaunch(ref: LaunchRef): Promise<Launch> { return this.adapterFor(ref.chain).getLaunch(ref); }
  listLaunches(chain: ChainRef, opts?: { limit?: number; cursor?: string }) { return this.adapterFor(chain).listLaunches(chain, opts); }
  quote(ref: LaunchRef, side: Side, amountIn: Amount, opts?: { readonly trader?: string }): Promise<Quote> {
    if (amountIn <= 0n) throw new FloorSdkError('bad_params', 'amountIn must be > 0');
    return this.adapterFor(ref.chain).quote(ref, side, amountIn, opts);
  }
  buildTrade(params: TradeParams): Promise<UnsignedTx> {
    if (params.amountIn <= 0n) throw new FloorSdkError('bad_params', 'amountIn must be > 0');
    if (params.minAmountOut <= 0n) throw new FloorSdkError('bad_params', 'minAmountOut must be > 0');
    return this.adapterFor(params.launch.chain).buildTrade(params);
  }
}
