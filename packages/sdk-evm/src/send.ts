/**
 * Sending one action's calls with as few wallet prompts as the wallet allows.
 *
 * - EIP-5792: `wallet_getCapabilities` (for the account, at the chain id; the chain-agnostic `"0x0"` entry is read too,
 *   and the chain's own entry wins) says the wallet runs an ATOMIC batch there (`atomic.status` `supported` or `ready`; an
 *   older wallet's `atomicBatch.supported`) → ONE `wallet_sendCalls` with `atomicRequired`: one prompt, the approvals and
 *   the trade land or fail together. The batch id is handed to `onSubmitted` BEFORE any wait, then `waitForCallsStatus`
 *   waits for the receipts (`wait: false` returns at the id).
 * - Otherwise (no 5792, no atomic batch on this chain, or the capabilities call fails): each call is sent in order with
 *   `eth_sendTransaction`, and its receipt awaited before the next — one prompt per call. A reverted call stops the sequence.
 * - A `wallet_sendCalls` the wallet refuses as UNSUPPORTED (EIP-1193 4200, JSON-RPC -32601, EIP-5792 5700 / 5710 / 5760)
 *   has sent nothing, so the calls go the per-call way instead. Any other refusal (a user rejection, say) is thrown.
 * ⛔ THE FALLBACK IS REQUIRED: some embedded wallets (Privy's, among them) do not implement `wallet_sendCalls`; a wallet
 *    that throws on `getCapabilities` (method not found) is sent the calls one by one.
 * ⛔ A non-atomic 5792 batch is not used by default (`allowNonAtomic`): an approval could land without its trade.
 * ⛔ AFTER A SUBMISSION, A FAILED WAIT IS NOT "NOTHING HAPPENED": a timeout or a failed status read throws
 *    `FloorEvmError('pending')` carrying the batch `id` / the `hashes` sent; keep waiting on those instead of re-sending.
 * ⚠ Zero prompts happen only with an embedded wallet the integrator itself controls (it signs without asking); no
 *   browser wallet sends anything without the user's approval.
 */
import { BaseError, defineChain, numberToHex, type Account, type Address, type Chain, type Client, type Hex, type Transport } from 'viem';
import { parseAccount } from 'viem/accounts';
import { getChainId, sendCalls, sendTransaction, waitForCallsStatus, waitForTransactionReceipt } from 'viem/actions';
import type { UnsignedTx } from '@floorsupply/sdk';
import { FloorEvmError } from './errors.js';

/** How long `sendEvmCalls` waits for a batch's final status by default, ms. */
export const DEFAULT_BATCH_TIMEOUT_MS = 60_000;
/** How long `sendEvmCalls` waits for each transaction's receipt on the per-call path by default, ms. */
export const DEFAULT_RECEIPT_TIMEOUT_MS = 180_000;

/** Error codes with which a wallet refuses `wallet_sendCalls` as unsupported, before sending anything. */
export const SEND_CALLS_UNSUPPORTED_CODES: ReadonlySet<number> = new Set([4200, -32601, 5700, 5710, 5760]);

export interface SendEvmCallsOptions {
  /** The sender. Defaults to the wallet client's own account. */
  readonly account?: Account | Address;
  /** Use a 5792 batch even where the wallet cannot make it atomic. Default false. */
  readonly allowNonAtomic?: boolean;
  /** Called once a 5792 batch is submitted, with its id, before any wait. Keep it: it is how to follow the batch. */
  readonly onSubmitted?: (id: string) => void;
  /** Called after each call on the per-call path, with its hash, before its receipt is awaited. */
  readonly onSent?: (hash: Hex, index: number) => void;
  /** Wait for the 5792 batch's receipts (`waitForCallsStatus`). Default true. */
  readonly wait?: boolean;
  /** How long to wait for the batch's final status, ms. Default `DEFAULT_BATCH_TIMEOUT_MS`. */
  readonly batchTimeoutMs?: number;
  /** How long to wait for each receipt on the per-call path, ms. Default `DEFAULT_RECEIPT_TIMEOUT_MS`. */
  readonly receiptTimeoutMs?: number;
}

export type SendEvmCallsResult =
  /** One prompt: the wallet's batch id (`wallet_getCallsStatus` / viem `waitForCallsStatus` follow it). */
  | { readonly path: 'eip5792'; readonly id: string; readonly atomic: boolean; readonly hashes: readonly Hex[] | null }
  /** One prompt per call: every transaction hash, in order, each mined successfully before the next was sent. */
  | { readonly path: 'transactions'; readonly hashes: readonly Hex[] };

type Wallet = Client<Transport, Chain | undefined, Account | undefined>;

type Caps = { atomic?: { status?: string }; atomicBatch?: { supported?: boolean } };

/** The wallet's capabilities for `chainId`: its chain-agnostic `"0x0"` entry with the chain's own entry over it. */
async function capabilitiesFor(wallet: Wallet, account: Account | Address, chainId: number): Promise<Caps | undefined> {
  const raw = (await wallet.request({ method: 'wallet_getCapabilities', params: [parseAccount(account).address, [numberToHex(chainId)]] } as never)) as Record<string, Caps> | null | undefined;
  if (!raw || typeof raw !== 'object') return undefined;
  let any: Caps | undefined, own: Caps | undefined;
  for (const [k, v] of Object.entries(raw)) {
    let n: number;
    try { n = Number(BigInt(k)); } catch { continue; }
    if (n === 0) any = v;
    else if (n === chainId) own = v;
  }
  if (!any && !own) return undefined;
  return { ...(any ?? {}), ...(own ?? {}) };
}

/** True when `e` is the wallet refusing `wallet_sendCalls` as unsupported (nothing was sent). */
export function isSendCallsUnsupported(e: unknown): boolean {
  const hit = (x: unknown) => typeof (x as { code?: unknown })?.code === 'number' && SEND_CALLS_UNSUPPORTED_CODES.has((x as { code: number }).code);
  if (e instanceof BaseError) return e.walk(hit) !== null;
  return hit(e);
}

/** Sends `tx`'s calls in one EIP-5792 batch where the wallet supports an atomic one, else one transaction at a time. */
export async function sendEvmCalls(wallet: Wallet, tx: UnsignedTx, opts: SendEvmCallsOptions = {}): Promise<SendEvmCallsResult> {
  if (tx.family !== 'evm') throw new FloorEvmError('bad_params', `sendEvmCalls sends EVM calls, not ${tx.family}`);
  if (tx.calls.length === 0) throw new FloorEvmError('bad_params', 'sendEvmCalls: no calls');
  const account = opts.account ?? wallet.account;
  if (!account) throw new FloorEvmError('bad_params', 'sendEvmCalls: no account');
  const on = await getChainId(wallet);
  if (on !== tx.chainId) throw new FloorEvmError('wrong_chain', `the wallet is on chain ${on}; these calls are for chain ${tx.chainId}`);
  const chain: Chain = wallet.chain?.id === tx.chainId ? wallet.chain : defineChain({
    id: tx.chainId, name: `chain ${tx.chainId}`, nativeCurrency: { name: 'native', symbol: 'native', decimals: 18 }, rpcUrls: { default: { http: [] } },
  });
  const calls = tx.calls.map((c) => ({ to: c.to as Address, data: c.data as Hex, value: c.value, gas: c.gas }));

  let atomic: boolean | null = null;
  try {
    const caps = await capabilitiesFor(wallet, account, tx.chainId);
    const status = caps?.atomic?.status;
    const can = status === 'supported' || status === 'ready' || caps?.atomicBatch?.supported === true;
    atomic = can ? true : opts.allowNonAtomic && caps !== undefined ? false : null;
  } catch {
    atomic = null; // no EIP-5792 here
  }
  if (atomic !== null) {
    let id: string | null = null;
    try {
      /* EIP-5792 calls carry no gas limit: the wallet estimates the batch. */
      ({ id } = await sendCalls(wallet, { account, chain, calls: calls.map(({ gas: _gas, ...c }) => c), forceAtomic: atomic }));
    } catch (e) {
      if (!isSendCallsUnsupported(e)) throw e;
      id = null; // refused before sending anything: the per-call way below
    }
    if (id !== null) return followBatch(wallet, id, atomic, opts);
  }

  const hashes: Hex[] = [];
  const receiptTimeout = opts.receiptTimeoutMs ?? DEFAULT_RECEIPT_TIMEOUT_MS;
  for (const [i, c] of calls.entries()) {
    const hash = await sendTransaction(wallet, { account, chain, to: c.to, data: c.data, value: c.value, ...(c.gas !== undefined ? { gas: c.gas } : {}) });
    hashes.push(hash);
    opts.onSent?.(hash, i);
    let status: string;
    try {
      ({ status } = await waitForTransactionReceipt(wallet, { hash, timeout: receiptTimeout }));
    } catch (e) {
      throw new FloorEvmError('pending', `call ${i + 1} of ${calls.length} (${hash}) was sent but has no receipt yet; the rest were not sent. Keep waiting on it before sending again`, { cause: e, hashes: [...hashes] });
    }
    if (status !== 'success') throw new FloorEvmError('reverted', `call ${i + 1} of ${calls.length} (${hash}) reverted; the rest were not sent`, { hashes: [...hashes] });
  }
  return { path: 'transactions', hashes };
}

async function followBatch(wallet: Wallet, id: string, atomic: boolean, opts: SendEvmCallsOptions): Promise<SendEvmCallsResult> {
  opts.onSubmitted?.(id);
  if (opts.wait === false) return { path: 'eip5792', id, atomic, hashes: null };
  let res: Awaited<ReturnType<typeof waitForCallsStatus>>;
  try {
    res = await waitForCallsStatus(wallet, { id, timeout: opts.batchTimeoutMs ?? DEFAULT_BATCH_TIMEOUT_MS });
  } catch (e) {
    throw new FloorEvmError('pending', `the batch ${id} was submitted; its outcome is not known yet. Keep waiting on it before sending again`, { cause: e, id });
  }
  const hashes = (res.receipts ?? []).map((r) => r.transactionHash);
  if (res.status === 'success') return { path: 'eip5792', id, atomic, hashes };
  if (res.status === 'failure') throw new FloorEvmError('batch_failed', `the batch ${id} ended failure`, { id, hashes });
  throw new FloorEvmError('pending', `the batch ${id} is still ${res.status ?? 'unknown'}`, { id, hashes });
}
