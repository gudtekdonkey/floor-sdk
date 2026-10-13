// Fork parity: `simulateSwap` against Uniswap's V4Quoter, on a FLOOR pool this script creates and graduates on a fork of Base.
//
// 1. Start a forking node on 127.0.0.1:8545 with the default test key funded: `anvil --fork-url <Base RPC>`, or Hardhat's
//    `hardhat node` forking Base (then mine one block first: Hardhat treats the fork block itself as historical).
// 2. For an ERC-20 quote, fund the test account with it (e.g. `hardhat_setStorageAt` on Base USDC's balance slot, 9).
// 3. Run from this package: `npx tsx scripts/fork-parity.mts`, with
//      QUOTE=<quote token> QUOTE_USD=<its USD price, whole dollars> Q1=<1: quote must sort above the token | 0: below>
//      LABEL=<fixture label> FIXTURE_OUT=test/fixtures/simulate/<name>.json   (to re-record a fixture)
// It launches a Cook launch, buys the sale out, then compares buys and sells of many sizes at six pool states (fresh, after a
// buy, after a ratchet, far above the cushion, after a sell, after a deep sell into the bid), and exits non-zero on any
// difference.
import { encodeFunctionData, createPublicClient, createWalletClient, http, parseEther, type Hex, type Address, maxUint256, encodeAbiParameters } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { base } from 'viem/chains';
import { POOL_ABI, FACTORY_ABI, LAUNCH_ABI, HOOK_ABI, V4_QUOTER_ABI, UNIVERSAL_ROUTER_ABI, TOKEN_ABI } from '../src/abi/index.ts';
import { floorDeploymentFor } from '../src/deployments.ts';
import { readSwapSnapshot, simulateSwap } from '../src/simulate.ts';
import { buildPoolSwap, poolSwapApprovals } from '../src/trade.ts';

const chain = { ...base, rpcUrls: { default: { http: ['http://127.0.0.1:8545'] } } };
const account = privateKeyToAccount('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80');
const pub = createPublicClient({ chain, transport: http('http://127.0.0.1:8545') });
const wallet = createWalletClient({ chain, account, transport: http('http://127.0.0.1:8545') });
const d = floorDeploymentFor(8453);
const MODE = Number(process.env.MODE ?? 2);
const QUOTE = (process.env.QUOTE ?? '0x0000000000000000000000000000000000000000') as Address;
const NATIVE_Q = BigInt(QUOTE) === 0n;
const WANT_Q1 = process.env.Q1; // '1' quote must sort above the token (currency1), '0' below
const ERC20 = [
  { type: 'function', name: 'approve', stateMutability: 'nonpayable', inputs: [{ name: 's', type: 'address' }, { name: 'a', type: 'uint256' }], outputs: [{ type: 'bool' }] },
  { type: 'function', name: 'balanceOf', stateMutability: 'view', inputs: [{ name: 'a', type: 'address' }], outputs: [{ type: 'uint256' }] },
] as const;
async function permitRouter(token: Address) {
  const router = d.routers.find((r) => r.kind === 'universal')!.address;
  await pub.waitForTransactionReceipt({ hash: await wallet.writeContract({ address: token, abi: ERC20, functionName: 'approve', args: [d.permit2!, maxUint256] }) });
  await pub.waitForTransactionReceipt({ hash: await wallet.writeContract({ address: d.permit2!, abi: [{ type: 'function', name: 'approve', stateMutability: 'nonpayable', inputs: [{ name: 'token', type: 'address' }, { name: 'spender', type: 'address' }, { name: 'amount', type: 'uint160' }, { name: 'expiration', type: 'uint48' }], outputs: [] }], functionName: 'approve', args: [token, router, (1n << 160n) - 1n, 2n ** 47n] }) });
}

let checks = 0, mismatches = 0;
const FIXTURE: { chainId: number; block?: string; label: string; quote: string; cases: unknown[] } = { chainId: 8453, label: process.env.LABEL ?? 'native', quote: QUOTE, cases: [] };
const ser = (snap: any) => JSON.parse(JSON.stringify({ ...snap, pool: { ...snap.pool, bitmap: [...snap.pool.bitmap], liquidityNet: [...snap.pool.liquidityNet] } }, (_, v) => (typeof v === 'bigint' ? v.toString() : v)));
async function compare(poolId: Hex, label: string) {
  const snap = await readSwapSnapshot(pub as never, 8453, poolId);
  const st = await pub.readContract({ address: d.singletonHook!, abi: HOOK_ABI, functionName: 'poolState', args: [poolId] });
  const key = st.poolKey;
  const q1 = st.quoteIsCurrency1;
  const block = await pub.getBlockNumber();
  const expected: { side: string; amountIn: string; amountOut: string | null }[] = [];
  const sizes = {
    buy: NATIVE_Q ? [1n, 10n ** 12n, 10n ** 15n, 10n ** 16n, 10n ** 17n, parseEther('1'), parseEther('5'), parseEther('30')] : [1n, 10n ** 3n, 10n ** 6n, 10n ** 8n, 10n ** 9n, 10n ** 10n, 5n * 10n ** 10n, 10n ** 11n],
    sell: [1n, 10n ** 18n, 10n ** 21n, 10n ** 24n, 10n ** 25n, 5n * 10n ** 25n, 10n ** 26n],
  };
  for (const side of ['buy', 'sell'] as const) {
    for (const amt of sizes[side]) {
      const zeroForOne = side === 'buy' ? !q1 : q1;
      let chainOut: bigint | string;
      try {
        const r = await pub.simulateContract({ address: d.quoter!, abi: V4_QUOTER_ABI, functionName: 'quoteExactInputSingle', args: [{ poolKey: key, zeroForOne, exactAmount: amt, hookData: '0x' }] });
        chainOut = (r.result as readonly [bigint, bigint])[0];
      } catch (e: any) { chainOut = 'revert'; }
      let simOut: bigint | string;
      try { simOut = simulateSwap(snap, side, amt).amountOut; } catch (e: any) { simOut = 'revert'; }
      checks++;
      expected.push({ side, amountIn: amt.toString(), amountOut: typeof chainOut === 'bigint' ? chainOut.toString() : null });
      const ok = chainOut === simOut;
      if (!ok) mismatches++;
      console.log(`${ok ? 'ok ' : 'BAD'} [${label}] ${side} ${amt}: quoter ${chainOut} sim ${simOut}${ok || typeof chainOut !== 'bigint' || typeof simOut !== 'bigint' ? '' : ` diff ${simOut - chainOut}`}`);
    }
  }
  FIXTURE.cases.push({ state: label, block: block.toString(), poolId, snapshot: ser(snap), expected });
}

async function swap(poolId: Hex, side: 'buy' | 'sell', amt: bigint) {
  // A real swap, built by the SDK's own trade builder and approvals (the path a wallet signs).
  const st = await pub.readContract({ address: d.singletonHook!, abi: HOOK_ABI, functionName: 'poolState', args: [poolId] });
  const r = d.routers.find((x) => x.kind === 'universal')!;
  const quote = BigInt(st.quoteToken) === 0n ? null : st.quoteToken;
  for (const step of poolSwapApprovals({ side, token: st.token, quote, amountIn: amt, spender: r.address, permit2: d.permit2 ?? null })) {
    await pub.waitForTransactionReceipt({ hash: await wallet.sendTransaction({ to: step.call.to, data: step.call.data, value: step.call.value }) });
  }
  const c = buildPoolSwap({ token: st.token, hook: d.singletonHook!, poolId, quote, quoteIsCurrency1: st.quoteIsCurrency1, side, amountIn: amt, minOut: 1n, deadline: 2n ** 40n, router: { address: r.address, release: r.release! } });
  const rc = await pub.waitForTransactionReceipt({ hash: await wallet.sendTransaction({ to: c.to, data: c.data, value: c.value, gas: 8_000_000n }) });
  if (rc.status !== 'success') throw new Error(`swap ${side} ${amt} reverted`);
}

async function main() {
  const p = {
    mode: MODE, name: 'Parity', symbol: 'PRTY', recipients: [{ wallet: account.address, bps: 10_000, platform: 0, handle: '' }],
    ethUsd: BigInt(process.env.QUOTE_USD ?? 2400), graduationMcapUsd: 0n, skimBuyBps: 0, skimSellBps: 0, creatorBuyBps: 0, creatorSellBps: 0, holderBuyBps: 0,
    holderSellBps: 0, quoteToken: QUOTE, saleBps: 0, polBps: 0,
    skimSellAtFloorMultipleBps: 0, teamBuyBps: 0, teamBuyWalletBps: 0, teamBuyLockDays: 0, teamBuyCliffDays: 0n, teamPayees: [],
  };
  let sim: any;
  for (let i = 0; ; i++) {
    const salt = ('0x' + (BigInt(Date.now()) * 1000n + BigInt(i)).toString(16).padStart(64, '0')) as Hex;
    sim = await pub.simulateContract({ account, address: d.factory, abi: FACTORY_ABI, functionName: 'createLaunch', args: [p as never, salt, '0x' + '00'.repeat(32) as Hex] });
    const tok = BigInt((sim.result as { token: Address }).token);
    if (WANT_Q1 === undefined || NATIVE_Q || (WANT_Q1 === '1') === (BigInt(QUOTE) > tok)) break;
  }
  const a = sim.result as { launch: Address; token: Address; poolId: Hex };
  await pub.waitForTransactionReceipt({ hash: await wallet.writeContract(sim.request) });
  console.log('launch', a.launch, 'token', a.token);
  for (let i = 0; i < 6 && !(await pub.readContract({ address: a.launch, abi: LAUNCH_ABI, functionName: 'graduated' })); i++) {
    const amount = NATIVE_Q ? parseEther('50') : 10n ** 11n;
    if (!NATIVE_Q) await pub.waitForTransactionReceipt({ hash: await wallet.writeContract({ address: QUOTE, abi: ERC20, functionName: 'approve', args: [a.launch, maxUint256] }) });
    const hash = await wallet.writeContract({ address: a.launch, abi: LAUNCH_ABI, functionName: 'buy', args: [amount], value: NATIVE_Q ? amount : 0n });
    console.log('sale buy', (await pub.waitForTransactionReceipt({ hash })).status);
  }
  if (!(await pub.readContract({ address: a.launch, abi: LAUNCH_ABI, functionName: 'graduated' }))) throw new Error('did not graduate');
  const poolId = await pub.readContract({ address: d.singletonHook!, abi: HOOK_ABI, functionName: 'poolForAccount', args: [a.launch] });
  console.log('graduated; pool', poolId);

  await compare(poolId, 'fresh');
  console.log('quoteIsCurrency1', (await pub.readContract({ address: d.singletonHook!, abi: HOOK_ABI, functionName: 'poolState', args: [poolId] })).quoteIsCurrency1);
  await swap(poolId, 'buy', NATIVE_Q ? parseEther('3') : 3n * 10n ** 9n);
  await compare(poolId, 'after 3 ETH buy');
  const ratchet = async () => {
    const hash = await wallet.writeContract({ address: d.singletonHook!, abi: HOOK_ABI, functionName: 'execute', args: [poolId, encodeFunctionData({ abi: POOL_ABI, functionName: 'ratchet' })], gas: 15_000_000n });
    console.log('ratchet', (await pub.waitForTransactionReceipt({ hash })).status);
  };
  await ratchet();
  await compare(poolId, 'after a ratchet');
  await swap(poolId, 'buy', NATIVE_Q ? parseEther('12') : 12n * 10n ** 9n);
  await compare(poolId, 'after a big buy above the cushion');
  const bal = await pub.readContract({ address: a.token, abi: TOKEN_ABI, functionName: 'balanceOf', args: [account.address] });
  await swap(poolId, 'sell', bal / 2n);
  await compare(poolId, 'after selling half the bag');
  await swap(poolId, 'sell', bal / 3n);
  await compare(poolId, 'after a deep sell');
  console.log(`\n${checks} checks, ${mismatches} mismatches`);
  if (process.env.FIXTURE_OUT) (await import('node:fs')).writeFileSync(process.env.FIXTURE_OUT, JSON.stringify(FIXTURE, null, 1) + '\n');
  process.exit(mismatches ? 1 : 0);
}
main().catch((e) => { console.error(e); process.exit(2); });
