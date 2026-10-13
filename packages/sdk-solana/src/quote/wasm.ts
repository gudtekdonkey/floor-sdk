/**
 * `initQuote(bytes)` over `floor_quote_bg.wasm`.
 *
 * ⛔ Load the wasm with `fetch` + `WebAssembly.instantiate` (or read the file in Node), NEVER through a bundler import:
 *      const q = await initQuote(await (await fetch(url)).arrayBuffer());
 *    `initQuote` hands the bytes to wasm-bindgen's `--target web` glue (`wasm/floor_quote.js`), whose init instantiates
 *    them with its own import object; its URL-fetching path (`new URL('floor_quote_bg.wasm', import.meta.url)`) is never
 *    taken because the bytes are always given.
 * ⛔ `wasm/` is copied in by `scripts/sync-wasm.mjs`; never edit it. `WASM_SHA256` is the hash of the shipped
 *    `floor_quote_bg.wasm`.
 * ⛔ The glue is a module-level singleton: the first `initQuote` wins, and a second call with different bytes returns a
 *    quoter over the FIRST instance. One wasm per page.
 */

import * as glue from '../../wasm/floor_quote.js';
import init from '../../wasm/floor_quote.js';

import { WASM_SHA256, WASM_BYTES } from '../generated/wasm.ts';
import { createQuoter, type QuoteFns, type Quoter } from './quoter.ts';

export { WASM_SHA256, WASM_BYTES };

/** The wasm-bindgen exports, held to `QuoteFns` at type-check time (a renamed export fails `tsc`). */
const fns: QuoteFns = glue;

let ready: Promise<Quoter> | null = null;

/**
 * Instantiate the quoter from the wasm bytes (an `ArrayBuffer`, a `Uint8Array`, or a compiled
 * `WebAssembly.Module`). Idempotent: concurrent and repeated calls share one instance.
 */
export function initQuote(source: ArrayBuffer | Uint8Array | WebAssembly.Module): Promise<Quoter> {
  if (!ready) {
    ready = (async () => {
      await init({ module_or_path: source });
      return createQuoter(fns);
    })().catch((e: unknown) => {
      ready = null;
      throw e;
    });
  }
  return ready;
}
