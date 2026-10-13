// `createQuoter` over a fake `QuoteFns`: the bigint ↔ decimal-string conversion, the
// `{"ok"}` / `{"err"}` envelope, tick-array concatenation, and that no number is computed here.
// The wasm itself is exercised by `crates/carpenter-quote/tests/vectors.rs` and artifact tier.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import {

  QuoteError,

  

  

} from '@carpenter/sdk';
import { createQuoter, type QuoteFns } from '../src/quote/quoter.ts';

const VERSION = '{"floor_quote_wasm":"0.1.0","hook_interface":1,"floor_hook_id":"22"}';

/** A fake that records every call and answers from a table. */
function fake(answers: Partial<Record<keyof QuoteFns, string | number>>): { fns: QuoteFns; calls: { fn: string; args: unknown[] }[] } {
  const calls: { fn: string; args: unknown[] }[] = [];
  const mk =
    (fn: keyof QuoteFns, fallback: string | number) =>
    (...args: unknown[]) => {
      calls.push({ fn, args });
      return answers[fn] ?? fallback;
    };
  const fns = {
    quote_swap: mk('quote_swap', '{"err":"AmountZero"}'),
    amounts_for_liquidity: mk('amounts_for_liquidity', '{"ok":{"amount0":"1","amount1":"2"}}'),
    quote_liquidity: mk('quote_liquidity', '{"ok":{"liquidity":"3"}}'),
    fees_owed: mk('fees_owed', '{"ok":{"fees0":"4","fees1":"5"}}'),
    sqrt_price_at_tick: mk('sqrt_price_at_tick', '{"ok":{"sqrt_price_x64":"18446744073709551616"}}'),
    tick_at_sqrt_price: mk('tick_at_sqrt_price', '{"ok":{"tick":0}}'),
    tick_at_price_ui: mk('tick_at_price_ui', '{"ok":{"tick":-69081}}'),
    price_ui_at_tick: mk('price_ui_at_tick', 1.5),
    floor_to_spacing: mk('floor_to_spacing', '{"ok":{"tick":-60}}'),
    ceil_to_spacing: mk('ceil_to_spacing', '{"ok":{"tick":60}}'),
    // the buy's fee on top of the price (spent = cost + team_fee), the
    // sell-back's fee out of the gross share (received = payout - team_fee).
    sale_buy: mk('sale_buy', '{"ok":{"tokens":"1000000","cost":"12345","team_fee":"30","spent":"12375","graduates":false}}'),
    sale_sell_back: mk('sale_sell_back', '{"ok":{"payout":"777","team_fee":"1","received":"776"}}'),
    referral_tier_dues: mk('referral_tier_dues', '{"ok":{"due1":"450","due2":"45","due3":"5"}}'),
    version: mk('version', VERSION),
  } as unknown as QuoteFns;
  return { fns, calls };
}

test('the scalar helpers convert both ways and pass ticks as numbers', () => {
  const { fns, calls } = fake({});
  const q = createQuoter(fns);
  assert.equal(q.sqrtPriceAtTick(0), 1n << 64n);
  assert.equal(q.tickAtSqrtPrice(1n << 64n), 0);
  assert.equal(q.tickAtPriceUi(1.0, 9, 6), -69081);
  assert.equal(q.priceUiAtTick(0, 9, 6), 1.5);
  assert.equal(q.floorToSpacing(-1, 60), -60);
  assert.equal(q.ceilToSpacing(1, 60), 60);
  assert.equal(q.quoteLiquidity(1n << 64n, -60, 60, 10n, 20n), 3n);
  assert.deepEqual(q.amountsForLiquidity(1n << 64n, 0, -60, 60, -7n), { amount0: 1n, amount1: 2n });
  assert.deepEqual(q.feesOwed(new Uint8Array(32), new Uint8Array(640), new Uint8Array(176), new Uint8Array(3888), new Uint8Array(3888)), { fees0: 4n, fees1: 5n });
  assert.deepEqual(q.saleBuy(new Uint8Array(520), 12345n), { tokens: 1_000_000n, cost: 12345n, teamFee: 30n, spent: 12375n, graduates: false });
  assert.deepEqual(q.saleSellBack(new Uint8Array(520), new Uint8Array(48), 5n), { payout: 777n, teamFee: 1n, received: 776n });
  const byFn = Object.fromEntries(calls.map((c) => [c.fn, c.args]));
  assert.deepEqual(byFn['tick_at_sqrt_price'], ['18446744073709551616']);
  assert.deepEqual(byFn['amounts_for_liquidity'], ['18446744073709551616', 0, -60, 60, '-7']);
  assert.deepEqual(byFn['quote_liquidity'], ['18446744073709551616', -60, 60, '10', '20']);
  assert.deepEqual(byFn['sale_buy']?.[1], '12345');
  assert.deepEqual(byFn['sale_sell_back']?.[2], '5');
});

test('referralTierDues: the wasm\'s three decimal strings become bigints, in tier order; a refusal is a QuoteError', () => {
  const { fns, calls } = fake({});
  const q = createQuoter(fns);
  const fp = new Uint8Array(792);
  assert.deepEqual(q.referralTierDues(fp), [450n, 45n, 5n]);
  assert.equal(calls.at(-1)?.fn, 'referral_tier_dues');
  assert.equal(calls.at(-1)?.args[0], fp, 'the FloorPool bytes go through unchanged');
  const bad = createQuoter(fake({ referral_tier_dues: '{"err":"QuoteBadAccount","account":"FloorPool"}' }).fns);
  assert.throws(() => bad.referralTierDues(fp), (e: unknown) => e instanceof QuoteError && e.code === 'QuoteBadAccount');
});
