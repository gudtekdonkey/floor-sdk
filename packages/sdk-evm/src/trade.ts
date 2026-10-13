/**
 * The calls a FLOOR trade on EVM is made of — the approvals it still needs, then the trade — as `EvmCall`s that
 * `buildTrade` hands back in ONE `UnsignedTx` (one wallet prompt with EIP-5792, `sendEvmCalls`). Nothing here signs or
 * sends; `pendingApprovals` is the one read.
 *
 * The four trades:
 * - BONDING BUY (before graduation): `FloorLaunch.buy()` with the native amount as value, or `buy(amount)` with no value on a
 *   token-quoted sale after the quote is approved to the SALE (the exact amount by default). The amount is the GROSS cap (the sale's FLOOR Platform fee rides
 *   on top); the change is refunded. The buy that sells the sale out graduates it in the same transaction.
 * - SELL-BACK (before graduation): `FloorToken.sellBack(tokens)` — pushed by the holder, no approval; pays in the quote
 *   what the contract version's rule gives (`sellBackQuote`), less the sale's FLOOR Platform fee.
 * - POOL BUY / SELL (after graduation): one exact-in v4 swap through the chain's UniversalRouter, oriented by where the quote
 *   sits in the key, carrying a positive minimum received and a deadline. A token input is pulled through Permit2: the
 *   token's ERC-20 approval of Permit2 and Permit2's own ON-CHAIN `approve` of the router go in the same batch when missing.
 * ⛔ No off-chain Permit2 signature (`signTypedData`) anywhere: it would be a second prompt.
 * - The same pool swap through `FloorSwapRatchet`, which can run a due ratchet in the same transaction (opt-in).
 * ⛔ The bonding calls carry NO on-chain minimum: the sale has none. `buildTrade` checks the quote against the caller's
 *    minimum at build time and refuses a trade that would not meet it; a buy racing another buy can still fill at a later
 *    tranche's price. The pool calls carry the minimum on chain (TAKE_ALL / `minOut`).
 */
import { encodeFunctionData, getAddress, type Address, type Hex } from 'viem';
import type { EvmCall, UnsignedTx } from '@floor/sdk';
import { FLOOR_SWAP_RATCHET_ABI, LAUNCH_ABI, PERMIT2_ABI, TOKEN_ABI, UNIVERSAL_ROUTER_ABI } from './abi/index.js';
import { FloorEvmError } from './errors.js';
import { NATIVE, poolIdOfKey, poolKey, quoteAtCurrency1 } from './pool.js';
import { PERMIT2_MAX_AMOUNT, PERMIT2_MAX_EXPIRATION, encodeV4Swap, v4LayoutOf } from './router.js';
import type { UrRelease } from './deployments.js';

/** The `unlimited` ERC-20 approval amount (2^255): one approval per token and spender, never re-asked. */
export const APPROVE_AMOUNT = 2n ** 255n;
/**
 * How much an approval grants: `exact` = the amount this trade spends (asked again next time), `unlimited` = 2^255 to a
 * token's spender and Permit2's maximum to a Permit2 spender (asked once).
 */
export type ApprovalAmount = 'exact' | 'unlimited';
/** The default swap deadline: ten minutes from the moment the trade is built. */
export const SWAP_DEADLINE_SECONDS = 600n;
/**
 * The gas limit every bonding buy is given while the sale is open: the buy that sells the sale out graduates the launch
 * inline and needs ~3.6–3.9M gas on the Sepolia contracts (more with a team buy), and any buy can become that one if
 * other buys land first — an estimate taken before them can sit far under it. A 5792 batch carries no gas limit (the
 * wallet estimates the batch), so there the floor does not apply.
 */
export const BOND_BUY_GAS_FLOOR = 5_100_000n;

const isEvmAddress = (a: unknown): a is Address => typeof a === 'string' && /^0x[0-9a-fA-F]{40}$/.test(a) && !/^0x0{40}$/.test(a);

function positive(n: bigint, what: string): bigint {
  if (n <= 0n) throw new FloorEvmError('bad_params', `${what}: the amount must be greater than zero`);
  return n;
}

const call = (to: Address, data: Hex, value = 0n): EvmCall => ({ to, data, value });

/** One user action's calls, in order, as `@floor/sdk`'s `UnsignedTx`. */
export function evmTx(chainId: number, calls: readonly EvmCall[]): UnsignedTx {
  if (calls.length === 0) throw new FloorEvmError('bad_params', 'a transaction with no calls');
  return { family: 'evm', chainId, calls };
}

/** A bonding buy of GROSS `amountIn` of the sale's quote. `quote` null = native. */
export function buildBondBuy(a: { launch: Address; quote: Address | null; amountIn: bigint }): EvmCall {
  const amount = positive(a.amountIn, 'bond buy');
  if (a.quote === null) return call(a.launch, encodeFunctionData({ abi: LAUNCH_ABI, functionName: 'buy', args: [] }), amount);
  return call(a.launch, encodeFunctionData({ abi: LAUNCH_ABI, functionName: 'buy', args: [amount] }));
}

/** A sell-back of `tokens` to the sale (before graduation). Sent by the holder; no approval. */
export function buildSellBack(a: { token: Address; tokens: bigint }): EvmCall {
  return call(a.token, encodeFunctionData({ abi: TOKEN_ABI, functionName: 'sellBack', args: [positive(a.tokens, 'sell-back')] }));
}

/** A graduated launch's pool, as a swap needs it. `poolId` is the HOOK's (`poolForAccount`); the key must hash to it. */
export interface PoolTarget {
  token: Address;
  hook: Address;
  poolId: Hex;
  /** The quote token; `null` = native. */
  quote: Address | null;
  quoteIsCurrency1: boolean;
}

export interface PoolSwapArgs extends PoolTarget {
  side: 'buy' | 'sell';
  /** Exact input: quote on a buy, tokens on a sell. */
  amountIn: bigint;
  /** The least received. Must be > 0. */
  minOut: bigint;
  /** Unix seconds. */
  deadline: bigint;
  router: { address: Address; release: UrRelease };
}

function checkedKey(a: PoolTarget, what: string) {
  const quote = a.quote ?? NATIVE;
  const key = poolKey(a.token, a.hook, quote);
  if (poolIdOfKey(key).toLowerCase() !== a.poolId.toLowerCase()) throw new FloorEvmError('contract_mismatch', `${what}: the pool key does not hash to this launch's pool id`);
  if (quoteAtCurrency1(key, quote) !== a.quoteIsCurrency1) throw new FloorEvmError('contract_mismatch', `${what}: the quote orientation disagrees with the pool key`);
  return { key, quote };
}

/** The UniversalRouter `execute` args for a pool swap. `payer: 'router'` when `FloorSwapRatchet` has paid the input in. */
function poolSwapPlan(a: PoolSwapArgs, payer: 'user' | 'router') {
  const what = `pool ${a.side}`;
  const { key, quote } = checkedKey(a, what);
  if (a.minOut <= 0n) throw new FloorEvmError('bad_params', `${what}: the swap carries no minimum received`);
  if (a.deadline <= 0n) throw new FloorEvmError('bad_params', `${what}: the swap carries no deadline`);
  const buying = a.side === 'buy';
  const amountIn = positive(a.amountIn, what);
  const zeroForOne = buying ? !a.quoteIsCurrency1 : a.quoteIsCurrency1;
  const [currencyIn, currencyOut] = buying ? [quote, a.token] : [a.token, quote];
  const plan = encodeV4Swap({ layout: v4LayoutOf('universal', a.router.release), key, zeroForOne, currencyIn, currencyOut, amountIn, minOut: a.minOut, payer });
  const value = buying && a.quote === null ? amountIn : 0n;
  return { plan, value, currencyIn, currencyOut, amountIn };
}

/** A pool buy or sell through the chain's UniversalRouter. */
export function buildPoolSwap(a: PoolSwapArgs): EvmCall {
  const { plan, value } = poolSwapPlan(a, 'user');
  return call(a.router.address, encodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, functionName: 'execute', args: [plan.commands, [...plan.inputs], a.deadline] }), value);
}

export interface SwapRatchetArgs extends PoolSwapArgs {
  swapRatchet: Address;
  /** Run the pool's ratchet after the swap when one is due (the keeper tip goes to the trader). */
  ratchet: boolean;
  /** Also sweep the pool's staircase fees after the swap (the sweep tip goes to the trader). Default false. */
  sweep?: boolean;
}

/** The same pool swap through `FloorSwapRatchet.swapAndRatchet`: the contract pays the router, then ratchets when asked and
 *  due (the keeper tip goes to the trader). A token input is pulled by the RATCHET CONTRACT through Permit2. */
export function buildSwapRatchet(a: SwapRatchetArgs): EvmCall {
  const { plan, value, currencyIn, currencyOut, amountIn } = poolSwapPlan(a, 'router');
  const routerCalldata = encodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, functionName: 'execute', args: [plan.commands, [...plan.inputs], a.deadline] });
  const s = { poolId: a.poolId, tokenIn: currencyIn, amountIn, tokenOut: currencyOut, minOut: a.minOut, routerCalldata, ratchet: a.ratchet, sweep: a.sweep ?? false };
  const data = encodeFunctionData({ abi: FLOOR_SWAP_RATCHET_ABI, functionName: 'swapAndRatchet', args: [s] });
  return call(a.swapRatchet, data, value);
}

/** One approval a trade needs before it can be sent. `reader` is the contract whose allowance answers whether it is done. */
export interface ApprovalStep {
  kind: 'erc20' | 'permit2';
  call: EvmCall;
  token: Address;
  spender: Address;
  /** The least allowance the trade spends (0 on a reset step). */
  need: bigint;
  /** A reset to 0 that `pendingApprovals` puts before an ERC-20 approval whose current allowance is above 0 but short. */
  reset?: true;
  /** ERC-20: the token (`allowance(owner, spender)`); Permit2: Permit2 (`allowance(owner, token, spender)`). */
  reader: Address;
}

function erc20Approval(token: Address, spender: Address, need: bigint, amount: bigint): ApprovalStep {
  return { kind: 'erc20', call: call(token, encodeFunctionData({ abi: TOKEN_ABI, functionName: 'approve', args: [spender, amount] })), token, spender, need, reader: token };
}

const erc20Amount = (need: bigint, amount: ApprovalAmount): bigint => (amount === 'exact' ? need : APPROVE_AMOUNT);

/**
 * What spending `token` through `spender` needs. Without Permit2: one ERC-20 approval of the spender. With Permit2: the
 * token's approval of Permit2, then Permit2's own ON-CHAIN approval of the spender (max expiration). `amount`: `unlimited`
 * (default) grants 2^255 / Permit2's maximum, `exact` grants `need`.
 */
export function tokenSpendApprovals(token: Address, spender: Address, permit2: Address | null, need: bigint, amount: ApprovalAmount = 'unlimited'): ApprovalStep[] {
  if (!isEvmAddress(token) || !isEvmAddress(spender)) throw new FloorEvmError('bad_params', 'approval: a token or spender is zero or malformed');
  positive(need, 'approval');
  if (!permit2) return [erc20Approval(token, spender, need, erc20Amount(need, amount))];
  if (amount === 'exact' && need > PERMIT2_MAX_AMOUNT) throw new FloorEvmError('bad_params', `approval: ${need} is above Permit2's largest grant`);
  const toPermit2 = erc20Approval(token, permit2, need, erc20Amount(need, amount));
  return [
    toPermit2,
    {
      kind: 'permit2',
      call: call(permit2, encodeFunctionData({ abi: PERMIT2_ABI, functionName: 'approve', args: [token, spender, amount === 'exact' ? need : PERMIT2_MAX_AMOUNT, Number(PERMIT2_MAX_EXPIRATION)] })),
      token, spender, need, reader: permit2,
    },
  ];
}

/**
 * A bonding buy's approvals: none on a native sale; the quote approved to the SALE (a per-launch contract) on a
 * token-quoted one — the exact amount by default.
 */
export function bondBuyApprovals(a: { launch: Address; quote: Address | null; amountIn: bigint }, amount: ApprovalAmount = 'exact'): ApprovalStep[] {
  return a.quote === null ? [] : [erc20Approval(a.quote, a.launch, a.amountIn, erc20Amount(a.amountIn, amount))];
}

/**
 * A pool swap's approvals. A native-quote buy needs none. Otherwise the input token is pulled through Permit2 by `spender`
 * (the UniversalRouter, or the `FloorSwapRatchet` when the trade goes through it).
 */
export function poolSwapApprovals(a: { side: 'buy' | 'sell'; token: Address; quote: Address | null; amountIn: bigint; spender: Address; permit2: Address | null }, amount: ApprovalAmount = 'unlimited'): ApprovalStep[] {
  const buying = a.side === 'buy';
  if (buying && a.quote === null) return [];
  const token = buying ? a.quote! : a.token;
  return tokenSpendApprovals(token, a.spender, a.permit2, a.amountIn, amount);
}

/**
 * Reads each step's allowance for `owner` and returns the steps still short, in order. A failed read rejects (never
 * "granted"). An ERC-20 approval whose current allowance is above 0 but short is preceded by an approval of 0 (`reset`):
 * some tokens (USDT among them) refuse to move an allowance from one non-zero value to another.
 */
export async function pendingApprovals(
  client: { readContract(args: { address: Address; abi: readonly unknown[]; functionName: string; args?: readonly unknown[] }): Promise<unknown> },
  owner: Address,
  steps: readonly ApprovalStep[],
  nowMs: number = Date.now(),
): Promise<ApprovalStep[]> {
  const out: ApprovalStep[] = [];
  for (const s of steps) {
    let have: bigint;
    if (s.kind === 'erc20') {
      have = (await client.readContract({ address: s.reader, abi: TOKEN_ABI, functionName: 'allowance', args: [getAddress(owner), s.spender] })) as bigint;
    } else {
      const [amount, expiration] = (await client.readContract({ address: s.reader, abi: PERMIT2_ABI, functionName: 'allowance', args: [getAddress(owner), s.token, s.spender] })) as readonly [bigint, number, number];
      have = BigInt(expiration) * 1000n > BigInt(nowMs) ? amount : 0n;
    }
    if (have >= s.need) continue;
    if (s.kind === 'erc20' && have > 0n) {
      out.push({ kind: 'erc20', call: call(s.token, encodeFunctionData({ abi: TOKEN_ABI, functionName: 'approve', args: [s.spender, 0n] })), token: s.token, spender: s.spender, need: 0n, reader: s.reader, reset: true });
    }
    out.push(s);
  }
  return out;
}
