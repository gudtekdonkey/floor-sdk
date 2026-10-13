import { describe, expect, it } from 'vitest';
import { createWalletClient, custom, type Hex } from 'viem';
import { sendEvmCalls, FloorEvmError } from '../src/index.js';
import type { UnsignedTx } from '@floor/sdk';

const CHAIN = 11155111;
const ME = '0x9999999999999999999999999999999999999999' as const;
const TX: UnsignedTx = {
  family: 'evm', chainId: CHAIN,
  calls: [
    { to: '0x1111111111111111111111111111111111111111', data: '0x095ea7b3', value: 0n, gas: 5_100_000n },
    { to: '0x2222222222222222222222222222222222222222', data: '0x3593564c', value: 5n },
  ],
};
const hashOf = (i: number) => (`0x${(i + 1).toString(16).padStart(64, '0')}`) as Hex;

function wallet(o: { caps?: unknown; capsFail?: boolean; chainId?: number; revertAt?: number; batchFails?: boolean; batchPending?: boolean; noReceipt?: boolean; sendCallsError?: { code: number; message: string } }) {
  const seen: { method: string; params: unknown }[] = [];
  let sent = 0;
  const client = createWalletClient({
    account: ME,
    pollingInterval: 5,
    transport: custom({
      async request({ method, params }: { method: string; params?: unknown }) {
        seen.push({ method, params });
        switch (method) {
          case 'eth_chainId': return `0x${(o.chainId ?? CHAIN).toString(16)}`;
          case 'wallet_getCapabilities':
            if (o.capsFail) throw Object.assign(new Error('Method not found'), { code: -32601 });
            return o.caps;
          case 'wallet_sendCalls':
            if (o.sendCallsError) throw Object.assign(new Error(o.sendCallsError.message), o.sendCallsError);
            return { id: 'batch-1' };
          case 'wallet_getCallsStatus': return {
            version: '2.0.0', id: 'batch-1', chainId: `0x${CHAIN.toString(16)}`, atomic: true, status: o.batchPending ? 100 : o.batchFails ? 500 : 200,
            receipts: [{ transactionHash: hashOf(7), blockHash: `0x${'ab'.repeat(32)}`, blockNumber: '0x10', gasUsed: '0x1', logs: [], status: o.batchFails ? '0x0' : '0x1' }],
          };
          case 'eth_sendTransaction': return hashOf(sent++);
          case 'eth_blockNumber': return '0x10';
          case 'eth_getTransactionByHash': return null;
          case 'eth_getTransactionReceipt': {
            if (o.noReceipt) return null;
            const h = (params as [Hex])[0];
            const i = Number(BigInt(h)) - 1;
            return {
              transactionHash: h, blockHash: `0x${'ab'.repeat(32)}`, blockNumber: '0x10', transactionIndex: '0x0', from: ME, to: TX.family === 'evm' ? TX.calls[i]!.to : null,
              cumulativeGasUsed: '0x1', gasUsed: '0x1', effectiveGasPrice: '0x1', logs: [], logsBloom: `0x${'00'.repeat(256)}`, contractAddress: null,
              status: o.revertAt === i ? '0x0' : '0x1', type: '0x2',
            };
          }
          default: throw new Error(`unexpected ${method}`);
        }
      },
    }),
  });
  return { client, seen };
}

describe('sendEvmCalls', () => {
  it('EIP-5792 with an atomic batch: ONE wallet_sendCalls carrying every call, atomic required', async () => {
    const w = wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'supported' } } } });
    const r = await sendEvmCalls(w.client, TX);
    expect(r).toEqual({ path: 'eip5792', id: 'batch-1', atomic: true, hashes: [hashOf(7)] });
    const sc = w.seen.filter((s) => s.method === 'wallet_sendCalls');
    expect(sc.length).toBe(1);
    const p = (sc[0]!.params as [{ atomicRequired: boolean; chainId: string; calls: { to: string; value?: string }[] }])[0];
    expect(p.atomicRequired).toBe(true);
    expect(p.chainId).toBe(`0x${CHAIN.toString(16)}`);
    expect(p.calls.map((c) => c.to)).toEqual(TX.family === 'evm' ? TX.calls.map((c) => c.to) : []);
    expect(w.seen.some((s) => s.method === 'eth_sendTransaction')).toBe(false);
  });

  it('an older wallet\'s atomicBatch.supported takes the batch too; a failed batch rejects; wait: false returns at the id', async () => {
    const legacy = wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomicBatch: { supported: true } } } });
    expect(await sendEvmCalls(legacy.client, TX, { wait: false })).toEqual({ path: 'eip5792', id: 'batch-1', atomic: true, hashes: null });
    const failing = wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'ready' } } }, batchFails: true });
    await expect(sendEvmCalls(failing.client, TX)).rejects.toMatchObject({ code: 'batch_failed', id: 'batch-1', hashes: [hashOf(7)] });
  });

  it('no EIP-5792 (a wallet that throws on getCapabilities, as Privy\'s embedded wallet did) or no atomic batch on this chain: one transaction per call, in order, each receipt awaited', async () => {
    for (const w of [wallet({ capsFail: true }), wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'unsupported' } } } })]) {
      const r = await sendEvmCalls(w.client, TX);
      expect(r).toEqual({ path: 'transactions', hashes: [hashOf(0), hashOf(1)] });
      const order = w.seen.map((s) => s.method).filter((m) => m === 'eth_sendTransaction' || m === 'eth_getTransactionReceipt');
      expect(order).toEqual(['eth_sendTransaction', 'eth_getTransactionReceipt', 'eth_sendTransaction', 'eth_getTransactionReceipt']);
      expect(w.seen.some((s) => s.method === 'wallet_sendCalls')).toBe(false);
      expect((w.seen.find((s) => s.method === 'eth_sendTransaction')!.params as [{ gas?: string }])[0].gas).toBe('0x4dd1e0');
    }
  });

  it('stops at a reverted call, and refuses a wallet on another chain', async () => {
    const w = wallet({ capsFail: true, revertAt: 0 });
    await expect(sendEvmCalls(w.client, TX)).rejects.toThrow(/call 1 of 2 .* reverted/);
    await expect(sendEvmCalls(wallet({ capsFail: true, revertAt: 0 }).client, TX)).rejects.toMatchObject({ code: 'reverted', hashes: [hashOf(0)] });
    expect(w.seen.filter((s) => s.method === 'eth_sendTransaction').length).toBe(1);
    const other = wallet({ chainId: 1 });
    await expect(sendEvmCalls(other.client, TX)).rejects.toThrow(FloorEvmError);
    await expect(sendEvmCalls(other.client, TX)).rejects.toMatchObject({ code: 'wrong_chain' });
    expect(other.seen.some((s) => s.method === 'wallet_sendCalls' || s.method === 'eth_sendTransaction')).toBe(false);
  });

  it('hands the batch id to onSubmitted BEFORE waiting, and a wait that times out throws pending with the id (never "nothing sent")', async () => {
    const order: string[] = [];
    const w = wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'supported' } } }, batchPending: true });
    const err = await sendEvmCalls(w.client, TX, { batchTimeoutMs: 40, onSubmitted: (id) => order.push(`submitted ${id}`) }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(FloorEvmError);
    expect(err).toMatchObject({ code: 'pending', id: 'batch-1' });
    expect(order).toEqual(['submitted batch-1']);
    expect(w.seen.filter((s) => s.method === 'wallet_sendCalls').length).toBe(1);
    expect(w.seen.some((s) => s.method === 'eth_sendTransaction')).toBe(false); // never re-sent the per-call way
    // wait: false still reports the id
    const ids: string[] = [];
    await sendEvmCalls(wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'ready' } } } }).client, TX, { wait: false, onSubmitted: (id) => ids.push(id) });
    expect(ids).toEqual(['batch-1']);
  });

  it('a receipt that does not arrive in time throws pending with every hash sent, and sends nothing more', async () => {
    const w = wallet({ capsFail: true, noReceipt: true });
    const sentHashes: Hex[] = [];
    const err = await sendEvmCalls(w.client, TX, { receiptTimeoutMs: 40, onSent: (h) => sentHashes.push(h) }).catch((e: unknown) => e);
    expect(err).toMatchObject({ code: 'pending', hashes: [hashOf(0)] });
    expect(sentHashes).toEqual([hashOf(0)]);
    expect(w.seen.filter((s) => s.method === 'eth_sendTransaction').length).toBe(1);
  });

  it('atomic.status ready takes the batch; the chain-agnostic "0x0" entry counts; the chain\'s own entry wins over it', async () => {
    const ready = wallet({ caps: { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'ready' } } } });
    expect(await sendEvmCalls(ready.client, TX)).toMatchObject({ path: 'eip5792', atomic: true });
    const any = wallet({ caps: { '0x0': { atomic: { status: 'supported' } } } });
    expect(await sendEvmCalls(any.client, TX)).toMatchObject({ path: 'eip5792', atomic: true });
    const own = wallet({ caps: { '0x0': { atomic: { status: 'supported' } }, [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'unsupported' } } } });
    expect(await sendEvmCalls(own.client, TX)).toMatchObject({ path: 'transactions' });
  });

  it('a non-atomic batch only with allowNonAtomic', async () => {
    const caps = { [`0x${CHAIN.toString(16)}`]: { atomic: { status: 'unsupported' } } };
    expect(await sendEvmCalls(wallet({ caps }).client, TX)).toMatchObject({ path: 'transactions' });
    const w = wallet({ caps });
    expect(await sendEvmCalls(w.client, TX, { allowNonAtomic: true })).toMatchObject({ path: 'eip5792', atomic: false });
    const p = (w.seen.find((s) => s.method === 'wallet_sendCalls')!.params as [{ atomicRequired: boolean }])[0];
    expect(p.atomicRequired).toBe(false);
  });

  it('a wallet that refuses wallet_sendCalls as UNSUPPORTED (4200 / -32601 / 5700 / 5710 / 5760) is sent the calls one by one; any other refusal is thrown', async () => {
    const caps = { [`0x${CHAIN.toString(16)}`]: { atomicBatch: { supported: true } } };
    for (const code of [4200, -32601, 5700, 5710, 5760]) {
      const w = wallet({ caps, sendCallsError: { code, message: 'unsupported' } });
      expect(await sendEvmCalls(w.client, TX)).toEqual({ path: 'transactions', hashes: [hashOf(0), hashOf(1)] });
    }
    for (const code of [4001, -32603, 5740]) {
      const w = wallet({ caps, sendCallsError: { code, message: 'no' } });
      await expect(sendEvmCalls(w.client, TX)).rejects.toThrow();
      expect(w.seen.some((s) => s.method === 'eth_sendTransaction')).toBe(false);
    }
  });
});
