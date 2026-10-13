import { describe, expect, it } from 'vitest';
import { decodeAbiParameters, decodeFunctionData, encodeAbiParameters, keccak256, toFunctionSelector, type Address, type Hex } from 'viem';
import {
  FLOOR_SWAP_RATCHET_ABI, LAUNCH_ABI, PERMIT2_ABI, TOKEN_ABI, UNIVERSAL_ROUTER_ABI, PERMIT2,
  encodeV4Swap, exactInSingleTuple, minOutFromBps, v4LayoutOf, PERMIT2_MAX_AMOUNT, PERMIT2_MAX_EXPIRATION, poolKey, poolIdOf,
  buildBondBuy, buildSellBack, buildPoolSwap, buildSwapRatchet, bondBuyApprovals, poolSwapApprovals, pendingApprovals, evmTx, APPROVE_AMOUNT, FloorEvmError,
  encodeV4Actions, tokenSpendApprovals,
} from '../src/index.js';

/* Reference fixtures. */
const TOKEN = '0x1111111111111111111111111111111111111111' as Address;
const HOOK = '0x2222222222222222222222222222222222222222' as Address;
const ROUTER = '0x5555555555555555555555555555555555555555' as Address;
const NATIVE = '0x0000000000000000000000000000000000000000' as Address;
const ONE = 10n ** 18n;
const nativeSwap = (a: { zeroForOne: boolean; amountIn: bigint; minOut: bigint }) => encodeV4Swap({
  layout: 'plain', key: poolKey(TOKEN, HOOK), zeroForOne: a.zeroForOne,
  currencyIn: a.zeroForOne ? NATIVE : TOKEN, currencyOut: a.zeroForOne ? TOKEN : NATIVE, amountIn: a.amountIn, minOut: a.minOut,
});
const unpackV4 = (input: Hex) => { const [actions, params] = decodeAbiParameters([{ type: 'bytes' }, { type: 'bytes[]' }], input); return { actions, params }; };
const digest = (p: { commands: Hex; inputs: readonly Hex[] }) => keccak256(encodeAbiParameters([{ type: 'bytes' }, { type: 'bytes[]' }], [p.commands, p.inputs]));

describe('the UniversalRouter payload', () => {
  it('is byte-identical to the reference plans (pinned digests)', () => {
    expect(digest(nativeSwap({ zeroForOne: true, amountIn: 3n * ONE, minOut: 7n }))).toBe('0xea755877aaa46e9b99ead600c15c06c8f8753f826792a7206091ffa6435710cc');
    expect(digest(nativeSwap({ zeroForOne: false, amountIn: 500n * ONE, minOut: 42n }))).toBe('0x020e0cad1678adafa1a303fd3371a588380dd196acdd30473e7248716bee4901');
  });
  it('a native buy is V4_SWAP then SWEEP to MSG_SENDER; a sell is V4_SWAP alone; actions 06 0c 0f', () => {
    const buy = nativeSwap({ zeroForOne: true, amountIn: ONE, minOut: 1n });
    expect(buy.commands).toBe('0x1004');
    expect(unpackV4(buy.inputs[0]!).actions).toBe('0x060c0f');
    const [tok, to, min] = decodeAbiParameters([{ type: 'address' }, { type: 'address' }, { type: 'uint256' }], buy.inputs[1]!);
    expect([tok, to, min]).toEqual([NATIVE, '0x0000000000000000000000000000000000000001', 0n]);
    expect(nativeSwap({ zeroForOne: false, amountIn: ONE, minOut: 1n }).commands).toBe('0x10');
  });
  it('the exact-in params are 11 words (no price limit), SETTLE_ALL capped at the input, TAKE_ALL at the minimum', () => {
    const plan = nativeSwap({ zeroForOne: true, amountIn: 2n * ONE, minOut: 42n });
    const { params } = unpackV4(plan.inputs[0]!);
    expect((params[0]!.length - 2) / 64).toBe(11);
    expect(decodeAbiParameters([{ type: 'address' }, { type: 'uint256' }], params[1]!)).toEqual([NATIVE, 2n * ONE]);
    expect(decodeAbiParameters([{ type: 'address' }, { type: 'uint256' }], params[2]!)).toEqual([TOKEN, 42n]);
    const [p] = decodeAbiParameters([exactInSingleTuple('plain')], params[0]!) as unknown as [{ poolKey: unknown; zeroForOne: boolean; amountIn: bigint; amountOutMinimum: bigint }];
    expect(p.poolKey).toEqual(poolKey(TOKEN, HOOK));
  });
  it('a 2.1.1 router gets minHopPriceX36 (12 words); a router with no release is refused', () => {
    const plan = encodeV4Swap({ layout: v4LayoutOf('universal', '2.1.1'), key: poolKey(TOKEN, HOOK), zeroForOne: true, currencyIn: NATIVE, currencyOut: TOKEN, amountIn: ONE, minOut: 1n });
    expect((unpackV4(plan.inputs[0]!).params[0]!.length - 2) / 64).toBe(12);
    expect(() => v4LayoutOf('universal', undefined)).toThrow(/release/);
  });
  it('refuses currencies that are not the key\'s in the swap direction, zero input and uint128 overflow', () => {
    expect(() => encodeV4Swap({ layout: 'plain', key: poolKey(TOKEN, HOOK), zeroForOne: true, currencyIn: TOKEN, currencyOut: NATIVE, amountIn: ONE, minOut: 1n })).toThrow(/currencies/);
    expect(() => nativeSwap({ zeroForOne: true, amountIn: 0n, minOut: 1n })).toThrow(/greater than zero/);
    expect(() => nativeSwap({ zeroForOne: true, amountIn: 1n << 128n, minOut: 1n })).toThrow(/uint128/);
  });
  it('the minimum is rounded down and a quote of nothing is no minimum', () => {
    expect(minOutFromBps(1000n, 100)).toBe(990n);
    expect(minOutFromBps(999n, 100)).toBe(989n);
    expect(minOutFromBps(0n, 100)).toBe(0n);
  });
});

const CHAIN = 11155111;
const LAUNCH = '0x3333333333333333333333333333333333333333' as Address;
const STOCK = '0x76B1Ad3252cAC3F01794634B1efB29BC5cC3b1ce' as Address;

describe('the trade builders', () => {
  it('a native bonding buy is buy() with the value; a token-quoted one is buy(amount) with none, after approving the SALE', () => {
    const n = buildBondBuy({ launch: LAUNCH, quote: null, amountIn: 5n });
    expect(n).toEqual({ to: LAUNCH, data: toFunctionSelector('buy()'), value: 5n });
    const t = buildBondBuy({ launch: LAUNCH, quote: STOCK, amountIn: 5n });
    expect(t.value).toBe(0n);
    expect(decodeFunctionData({ abi: LAUNCH_ABI, data: t.data as Hex })).toEqual({ functionName: 'buy', args: [5n] });
    expect(bondBuyApprovals({ launch: LAUNCH, quote: null, amountIn: 5n })).toEqual([]);
    const [ap] = bondBuyApprovals({ launch: LAUNCH, quote: STOCK, amountIn: 5n });
    expect(ap!.call).toMatchObject({ to: STOCK, value: 0n });
    // the exact amount by default (the sale is a per-launch contract); unlimited on request
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: ap!.call.data as Hex })).toEqual({ functionName: 'approve', args: [LAUNCH, 5n] });
    const [un] = bondBuyApprovals({ launch: LAUNCH, quote: STOCK, amountIn: 5n }, 'unlimited');
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: un!.call.data as Hex }).args).toEqual([LAUNCH, APPROVE_AMOUNT]);
    expect(() => buildBondBuy({ launch: LAUNCH, quote: null, amountIn: 0n })).toThrow(FloorEvmError);
    expect(evmTx(CHAIN, [n])).toEqual({ family: 'evm', chainId: CHAIN, calls: [n] });
    expect(() => evmTx(CHAIN, [])).toThrow(/no calls/);
  });
  it('a sell-back is the token\'s sellBack(tokens), no value, no approval', () => {
    const s = buildSellBack({ token: TOKEN, tokens: 7n });
    expect(s).toMatchObject({ to: TOKEN, value: 0n });
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: (s as { data: Hex }).data })).toEqual({ functionName: 'sellBack', args: [7n] });
  });

  const pool = { token: TOKEN, hook: HOOK, poolId: poolIdOf(TOKEN, HOOK), quote: null, quoteIsCurrency1: false, deadline: 1_900_000_000n, router: { address: ROUTER, release: '2.0.0' as const } };
  it('a native pool buy is execute() carrying the reference plan, the value and the deadline', () => {
    const b = buildPoolSwap({ ...pool, side: 'buy', amountIn: 3n * ONE, minOut: 7n }) as { to: Address; data: Hex; value: bigint };
    expect(b.to).toBe(ROUTER);
    expect(b.value).toBe(3n * ONE);
    const { args } = decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: b.data });
    expect(digest({ commands: args[0], inputs: args[1] })).toBe('0xea755877aaa46e9b99ead600c15c06c8f8753f826792a7206091ffa6435710cc');
    expect(args[2]).toBe(1_900_000_000n);
  });
  it('a pool sell sends no value, and refuses a zero minimum, a missing deadline and a key that does not hash to the pool', () => {
    const s = buildPoolSwap({ ...pool, side: 'sell', amountIn: 500n * ONE, minOut: 42n }) as { data: Hex; value: bigint };
    expect(s.value).toBe(0n);
    const { args } = decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: s.data });
    expect(digest({ commands: args[0], inputs: args[1] })).toBe('0x020e0cad1678adafa1a303fd3371a588380dd196acdd30473e7248716bee4901');
    expect(() => buildPoolSwap({ ...pool, side: 'sell', amountIn: ONE, minOut: 0n })).toThrow(/minimum/);
    expect(() => buildPoolSwap({ ...pool, side: 'sell', amountIn: ONE, minOut: 1n, deadline: 0n })).toThrow(/deadline/);
    expect(() => buildPoolSwap({ ...pool, poolId: poolIdOf(TOKEN, ROUTER), side: 'sell', amountIn: ONE, minOut: 1n })).toThrow(/does not hash/);
    expect(() => buildPoolSwap({ ...pool, quoteIsCurrency1: true, side: 'sell', amountIn: ONE, minOut: 1n })).toThrow(/orientation/);
  });
  it('a stock-quoted pool spends the quote its key says, in both orders', () => {
    const lo = '0x0000000000000000000000000000000000000abc' as Address;
    const p = { ...pool, token: lo, quote: STOCK, quoteIsCurrency1: true, poolId: poolIdOf(lo, HOOK, STOCK) };
    const b = buildPoolSwap({ ...p, side: 'buy', amountIn: ONE, minOut: 1n }) as { data: Hex; value: bigint };
    expect(b.value).toBe(0n);
    const { args } = decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: b.data });
    expect(args[0]).toBe('0x10');
    const settle = decodeAbiParameters([{ type: 'address' }, { type: 'uint256' }], unpackV4(args[1][0]!).params[1]!);
    expect(settle).toEqual([STOCK, ONE]);
  });
  it('the approvals: none for a native buy; a sell approves the token to Permit2 then Permit2 to the router ON CHAIN (no signature)', () => {
    const base = { token: TOKEN, quote: null, amountIn: ONE, spender: ROUTER, permit2: PERMIT2 };
    expect(poolSwapApprovals({ ...base, side: 'buy' })).toEqual([]);
    const two = poolSwapApprovals({ ...base, side: 'sell' });
    expect(two.map((s) => [s.kind, s.spender, s.reader])).toEqual([['erc20', PERMIT2, TOKEN], ['permit2', ROUTER, PERMIT2]]);
    expect(decodeFunctionData({ abi: PERMIT2_ABI, data: two[1]!.call.data as Hex }).args).toEqual([TOKEN, ROUTER, PERMIT2_MAX_AMOUNT, Number(PERMIT2_MAX_EXPIRATION)]);
    expect(poolSwapApprovals({ ...base, side: 'sell', permit2: null }).map((s) => [s.kind, s.spender])).toEqual([['erc20', ROUTER]]);
  });
  it('exact pool approvals grant the trade\'s amount on both legs (ERC-20 → Permit2 and Permit2 → spender); unlimited is the default', () => {
    const base = { token: TOKEN, quote: null, amountIn: 7n * ONE, spender: ROUTER, permit2: PERMIT2, side: 'sell' as const };
    const [e, p] = poolSwapApprovals(base, 'exact');
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: e!.call.data as Hex }).args).toEqual([PERMIT2, 7n * ONE]);
    expect(decodeFunctionData({ abi: PERMIT2_ABI, data: p!.call.data as Hex }).args).toEqual([TOKEN, ROUTER, 7n * ONE, Number(PERMIT2_MAX_EXPIRATION)]);
    const [eu] = poolSwapApprovals(base);
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: eu!.call.data as Hex }).args).toEqual([PERMIT2, APPROVE_AMOUNT]);
    expect(() => tokenSpendApprovals(TOKEN, ROUTER, PERMIT2, PERMIT2_MAX_AMOUNT + 1n, 'exact')).toThrow(/Permit2's largest grant/);
    expect(() => tokenSpendApprovals(TOKEN, ROUTER, null, 0n)).toThrow(FloorEvmError);
  });
  it('an ERC-20 allowance above 0 but short is reset to 0 first (USDT-style tokens); one at 0 is approved directly', async () => {
    const [step] = bondBuyApprovals({ launch: LAUNCH, quote: STOCK, amountIn: 5n * ONE });
    const reading = (have: bigint) => ({ readContract: async () => have });
    const short = await pendingApprovals(reading(ONE) as never, '0x9999999999999999999999999999999999999999', [step!]);
    expect(short.map((s) => [s.reset ?? false, decodeFunctionData({ abi: TOKEN_ABI, data: s.call.data as Hex }).args])).toEqual([[true, [LAUNCH, 0n]], [false, [LAUNCH, 5n * ONE]]]);
    expect(short[0]!.call.to).toBe(STOCK);
    const none = await pendingApprovals(reading(0n) as never, '0x9999999999999999999999999999999999999999', [step!]);
    expect(none.map((s) => s.reset ?? false)).toEqual([false]);
    expect(await pendingApprovals(reading(5n * ONE) as never, '0x9999999999999999999999999999999999999999', [step!])).toEqual([]);
  });
  it('encodeV4Actions is abi.encode(bytes actions, bytes[] params)', () => {
    const enc = encodeV4Actions('0x060c0f', ['0x01', '0x0203']);
    expect(unpackV4(enc)).toEqual({ actions: '0x060c0f', params: ['0x01', '0x0203'] });
    expect(enc).toBe(encodeAbiParameters([{ type: 'bytes' }, { type: 'bytes[]' }], ['0x060c0f', ['0x01', '0x0203']]));
  });
  it('pendingApprovals reads each allowance and keeps only the short ones (an expired Permit2 grant reads 0)', async () => {
    const steps = poolSwapApprovals({ token: TOKEN, quote: null, amountIn: ONE, spender: ROUTER, permit2: PERMIT2, side: 'sell' });
    const client = { readContract: async (a: { functionName: string }) => (a.functionName === 'allowance' && 'abi' in a && (a as { abi: unknown }).abi === TOKEN_ABI ? 2n * ONE : [5n * ONE, 1000, 0]) };
    const left = await pendingApprovals(client as never, '0x9999999999999999999999999999999999999999', steps, 2_000_000);
    expect(left.map((s) => s.kind)).toEqual(['permit2']);
    const fresh = await pendingApprovals(client as never, '0x9999999999999999999999999999999999999999', steps, 999_999);
    expect(fresh).toEqual([]);
  });
  it('through FloorSwapRatchet: the router payload is paid by the contract (SETTLE, 0x0b), minOut and the ratchet flag ride the struct', () => {
    const SR = '0x7777777777777777777777777777777777777777' as Address;
    const r = buildSwapRatchet({ ...pool, side: 'sell', amountIn: ONE, minOut: 9n, swapRatchet: SR, ratchet: true }) as { to: Address; data: Hex };
    expect(r.to).toBe(SR);
    const { functionName, args } = decodeFunctionData({ abi: FLOOR_SWAP_RATCHET_ABI, data: r.data });
    expect(functionName).toBe('swapAndRatchet');
    const s = args[0] as { tokenIn: Address; tokenOut: Address; minOut: bigint; ratchet: boolean; sweep: boolean; routerCalldata: Hex };
    expect([s.tokenIn, s.tokenOut, s.minOut, s.ratchet, s.sweep]).toEqual([TOKEN, NATIVE, 9n, true, false]);
    const inner = decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: s.routerCalldata });
    expect(unpackV4(inner.args[1][0]!).actions).toBe('0x060b0f');
    const sw = buildSwapRatchet({ ...pool, side: 'buy', amountIn: ONE, minOut: 9n, swapRatchet: SR, ratchet: false, sweep: true }) as { data: Hex; value: bigint };
    expect(sw.value).toBe(ONE);
    expect((decodeFunctionData({ abi: FLOOR_SWAP_RATCHET_ABI, data: sw.data }).args[0] as { sweep: boolean }).sweep).toBe(true);
  });
});
