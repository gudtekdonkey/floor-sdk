// The wasm-bindgen exports the quoter wrapper calls (`QuoteFns`: `@floorsupply/carpenter-sdk`'s and this package's `src/quote/quoter.ts`).
// One list, read by `sync-wasm.mjs` (fails on a glue missing any) and by
// `test/glue.test.ts` (fails when `QuoteFns` and this list drift).
export const REQUIRED_EXPORTS = [
  'quote_swap',
  'amounts_for_liquidity',
  'quote_liquidity',
  'fees_owed',
  'sqrt_price_at_tick',
  'tick_at_sqrt_price',
  'tick_at_price_ui',
  'price_ui_at_tick',
  'floor_to_spacing',
  'ceil_to_spacing',
  'sale_buy',
  'sale_sell_back',
  // the holder tally's claimable, through
  // floor_math::holders::claimable. ⚠ It THROWS and returns a bigint,
  // unlike every other export's JSON string.
  'holder_claimable',
  // each tier's due, through floor_math::referral_tier_dues.
  // A JSON string like every other export (`{"ok":{"due1","due2","due3"}}`).
  'referral_tier_dues',
  'version',
];

/**
 * What makes a glue a `wasm-bindgen --target web` module: a DEFAULT export (the async init
 * `initQuote` calls) and `initSync`.
 *
 * ⛔ One list, read by `sync-wasm.mjs` AND `test/glue.test.ts` — a gate and its test
 * that spell the same thing twice can disagree, and did: both were written against a hand-made stub
 * and looked for `export default __wbg_init;` + `export { initSync }`, while wasm-bindgen 0.2.128
 * emits ONE statement, `export { initSync, __wbg_init as default };`. The real glue failed the gate
 * the day it was first generated. Each entry accepts either spelling.
 */
export const WEB_TARGET_MARKERS = [
  ['a default export (the async init)', /export\s+default\s+\w+|export\s*\{[^}]*\bas\s+default\b[^}]*\}/],
  ['initSync', /export\s+(?:function\s+)?initSync\b|export\s*\{[^}]*\binitSync\b[^}]*\}/],
];
