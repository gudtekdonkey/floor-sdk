// Live parity: `simulateSwap` against Uniswap's V4Quoter on every graduated FLOOR pool the chains list today, no fork and no
// transaction. Each chain's pools come from its registry (`count` / `page` → `poolForAccount`); every read and every quote is
// pinned to one block, so a trade landing mid-run cannot split the comparison.
//
// Run from this package: `npx tsx scripts/live-parity.mts`. Options (env):
//   CHAINS=1,8453            the chains to check (default: every chain in the book with a singleton hook, a quoter and a node)
//   RPC_<chainId>=<url>      a node for that chain (defaults below are public endpoints)
//   FIXTURE_DIR=test/fixtures/simulate   write one fixture per pool (`live-<chainId>-<token>.json`) for the unit tests
// Exits non-zero on any difference.
import { createPublicClient, http, type Address, type Hex, type PublicClient } from 'viem';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { HOOK_ABI, LAUNCH_ABI, V4_QUOTER_ABI } from '../src/abi/index.ts';
import { EVM_DEPLOYMENTS, EVM_NETWORKS, hasDeployment, floorDeploymentFor } from '../src/deployments.ts';
import { readPoolId, readRegistryCount, readRegistryPage } from '../src/reads.ts';
import { readSwapSnapshot, simulateSwap, type SwapSnapshot } from '../src/simulate.ts';

const PUBLIC_RPC: Record<number, string> = {
  1: 'https://ethereum-rpc.publicnode.com',
  56: 'https://bsc-dataseed.bnbchain.org',
  4663: 'https://rpc.mainnet.chain.robinhood.com',
  5042: 'https://rpc.mainnet.arc.io',
  8453: 'https://mainnet.base.org',
};
const rpcOf = (id: number) => process.env[`RPC_${id}`] ?? PUBLIC_RPC[id];
const chains = process.env.CHAINS
  ? process.env.CHAINS.split(',').map(Number)
  : Object.values(EVM_DEPLOYMENTS).filter((d) => hasDeployment(d.chainId) && d.singletonHook && d.quoter && d.stateView && rpcOf(d.chainId)).map((d) => d.chainId);

// Every read below goes to one block: the client's reads take `blockNumber` from here.
function pinned(c: PublicClient, blockNumber: bigint): PublicClient {
  return new Proxy(c, {
    get(t, k) {
      if (k === 'readContract' || k === 'call' || k === 'simulateContract') return (a: object) => (t as any)[k]({ ...a, blockNumber });
      return (t as any)[k];
    },
  });
}

// Amounts in the input asset's smallest unit, from dust to past the pool's depth (a quoter refusal is compared too).
const BUY = [1n, 10n ** 9n, 10n ** 12n, 10n ** 15n, 10n ** 16n, 10n ** 17n, 10n ** 18n, 10n ** 19n, 10n ** 20n, 10n ** 21n];
const SELL = [1n, 10n ** 15n, 10n ** 18n, 10n ** 21n, 10n ** 23n, 10n ** 24n, 10n ** 25n, 5n * 10n ** 25n, 10n ** 26n, 3n * 10n ** 26n];

const ser = (snap: SwapSnapshot) =>
  JSON.parse(JSON.stringify({ ...snap, pool: { ...snap.pool, bitmap: [...snap.pool.bitmap], liquidityNet: [...snap.pool.liquidityNet] } }, (_, v) => (typeof v === 'bigint' ? v.toString() : v)));

let checks = 0, mismatches = 0, pools = 0;
for (const chainId of chains) {
  const d = floorDeploymentFor(chainId);
  const live = createPublicClient({ transport: http(rpcOf(chainId), { retryCount: 3 }) }) as PublicClient;
  const block = await live.getBlockNumber();
  const c = pinned(live, block);
  const n = await readRegistryCount(c, d);
  const entries = n === 0n ? [] : await readRegistryPage(c, d, 0n, n);
  for (const e of entries) {
    if (!(await c.readContract({ address: e.launch, abi: LAUNCH_ABI, functionName: 'graduated' }))) continue;
    const poolId: Hex = await readPoolId(c, d.singletonHook as Address, e.launch);
    if (BigInt(poolId) === 0n) continue;
    pools++;
    const st = await c.readContract({ address: d.singletonHook as Address, abi: HOOK_ABI, functionName: 'poolState', args: [poolId] });
    const snap = await readSwapSnapshot(c, chainId, poolId);
    const label = `${chainId} ${e.token}`;
    console.log(`\n${label} @${block}: tick ${snap.pool.tick} floor ${snap.floorTick} protocolFee ${snap.pool.protocolFee} rates ${JSON.stringify(snap.rates)}`);
    const expected: { side: 'buy' | 'sell'; amountIn: string; amountOut: string | null }[] = [];
    for (const side of ['buy', 'sell'] as const) {
      for (const amt of side === 'buy' ? BUY : SELL) {
        const zeroForOne = side === 'buy' ? !st.quoteIsCurrency1 : st.quoteIsCurrency1;
        let chainOut: bigint | 'revert';
        try {
          const r = await c.simulateContract({ address: d.quoter as Address, abi: V4_QUOTER_ABI, functionName: 'quoteExactInputSingle', args: [{ poolKey: st.poolKey, zeroForOne, exactAmount: amt, hookData: '0x' }] });
          chainOut = (r.result as readonly [bigint, bigint])[0];
        } catch { chainOut = 'revert'; }
        let simOut: bigint | 'revert';
        try { simOut = simulateSwap(snap, side, amt).amountOut; } catch { simOut = 'revert'; }
        checks++;
        const ok = chainOut === simOut;
        if (!ok) mismatches++;
        expected.push({ side, amountIn: amt.toString(), amountOut: typeof chainOut === 'bigint' ? chainOut.toString() : null });
        console.log(`${ok ? 'ok ' : 'BAD'} ${side} ${amt}: quoter ${chainOut} sim ${simOut}`);
      }
    }
    if (process.env.FIXTURE_DIR) {
      const fx = { chainId, label: `live ${EVM_NETWORKS[chainId]?.label ?? chainId} ${e.token}`, quote: st.quoteToken, cases: [{ state: 'live', block: block.toString(), poolId, snapshot: ser(snap), expected }] };
      writeFileSync(join(process.env.FIXTURE_DIR, `live-${chainId}-${e.token.toLowerCase()}.json`), JSON.stringify(fx, null, 1) + '\n');
    }
  }
}
console.log(`\n${pools} pools, ${checks} checks, ${mismatches} mismatches`);
process.exit(mismatches || pools === 0 ? 1 : 0);
