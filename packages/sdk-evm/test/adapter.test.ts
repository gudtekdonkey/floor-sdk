import { describe, expect, it } from 'vitest';
import { createPublicClient, custom, decodeFunctionData, encodeAbiParameters, encodeFunctionResult, getAddress, type Abi, type Address, type Hex } from 'viem';
import {
  FACTORY_ABI, FLOOR_SWAP_RATCHET_ABI, HOOK_ABI, LAUNCH_ABI, PERMIT2_ABI, PERMIT2, POOL_ABI, POOL_STATE_ABI, POOL_STATE_COMPONENTS, REGISTRY_ABI, TOKEN_ABI, UNIVERSAL_ROUTER_ABI, V4_QUOTER_ABI, WALLET_SCAN_ABI,
  EVM_DEPLOYMENTS, FloorEvmError, assertWholeTake, createEvmAdapter, decodePoolState, poolIdOf, poolStateSlot, priceX18AtTick, saleBuyQuote,
  BOND_BUY_GAS_FLOOR, PERMIT2_MAX_EXPIRATION, isRevert, launchOfToken, readQuoteAsset, readSale, readSalePosition,
  OLDER_RULES, STATE_VIEW_ABI, gapSkimBps, hookTakes, marketTick, simulateSwap, skimBpsAt, sqrtPriceAtTick, type Deployment, type PoolStateTuple, type SwapSnapshot,
} from '../src/index.js';

const CHAIN = 11155111;
const D = EVM_DEPLOYMENTS[CHAIN]!;
const HOOK = getAddress('0x483eff3913cc30771faa9e42826d6083775260cc');
const NATIVE = '0x0000000000000000000000000000000000000000' as Address;
const E18 = 10n ** 18n;
const A = (n: number) => getAddress(`0x${n.toString(16).padStart(40, '0')}`);
const BOND = { token: A(0xb0), launch: A(0xb1) };
const GRAD = { token: A(0xc0), launch: A(0xc1) };
const STRANGER = A(0xd0);
const QB = { token: A(0xf0), launch: A(0xf1) }; // a bonding launch quoted in an ERC-20
const STOCK = A(0x5c);
const UNREGISTERED = { token: A(0x70), launch: A(0x71) };
const BROKEN = A(0x99); // an address whose calls fail at the transport (not a revert)
const CREATOR = A(0xee);
const SELLER = A(0xaa);

/** A poolState answer with every field zero but those given. */
function poolState(over: Partial<Record<string, unknown>>): PoolStateTuple {
  const zero = (t: string): unknown => (t === 'address' ? NATIVE : t === 'bool' ? false : t === 'int24' ? 0 : 0n);
  const out: Record<string, unknown> = {};
  for (const c of POOL_STATE_COMPONENTS) {
    out[c.name] = 'components' in c ? Object.fromEntries(c.components.map((x) => [x.name, x.type === 'uint24' || x.type === 'int24' ? 0 : zero(x.type)])) : zero(c.type);
  }
  return { ...out, ...over } as PoolStateTuple;
}
const RATES = { skimBuyFarBps: 100n, skimSellFarBps: 100n, skimBuyAtFloorBps: 0n, skimSellAtFloorBps: 500n, creatorBuyBps: 50n, creatorSellBps: 75n, holderBuyBps: 0n, holderSellBps: 0n, teamFeeBps: 25n, tickSpacing: 60 };
const STATES: Record<string, PoolStateTuple> = {
  [poolIdOf(BOND.token, HOOK)]: poolState({ token: BOND.token, ...RATES }),
  [poolIdOf(GRAD.token, HOOK)]: poolState({ token: GRAD.token, ...RATES, floorTick: 92100, bidWidth: 120 }),
  [poolIdOf(QB.token, HOOK, STOCK)]: poolState({ token: QB.token, ...RATES, quoteToken: STOCK, quoteIsCurrency1: false }),
  // GRAD's state under an id its key does not hash to
  [poolIdOf(GRAD.token, STRANGER)]: poolState({ token: GRAD.token, ...RATES, floorTick: 92100, bidWidth: 120 }),
};
/** `stairs()`'s answer: every field zero but the cushion's edge and liquidity. */
function stairsAnswer(cushionLower: number, cushionLiq: bigint) {
  const out = (POOL_ABI.find((f) => f.type === 'function' && f.name === 'stairs') as { outputs: readonly { components: readonly { name: string; type: string; components?: readonly { name: string; type: string }[] }[] }[] }).outputs[0]!;
  const z = (t: string): unknown => (t === 'bool' ? false : /^u?int(8|16|24)$/.test(t) ? 0 : 0n);
  const v: Record<string, unknown> = {};
  for (const c of out.components) {
    v[c.name] = c.type === 'tuple[16]' ? Array.from({ length: 16 }, () => Object.fromEntries(c.components!.map((x) => [x.name, z(x.type)]))) : z(c.type);
  }
  return { ...v, cushionLower, cushionLiq };
}
const SALE = { funded: true, saleTokens: 550_000_000n * E18, sold: 0n, raised: 0n, p0: 225_000_000_000n, p1: 225_000_000_000n, tranches: 10, teamFeeBps: 100 };
const ENTRY = (t: { token: Address; launch: Address }, at: bigint) => ({ launch: t.launch, token: t.token, hook: HOOK, rewards: NATIVE, poolId: '0x' + '00'.repeat(32) as Hex, mode: 2, at, registrar: D.factory });
const ENTRIES = [ENTRY(GRAD, 100n), ENTRY(BOND, 200n)];
const ENTRIES_WITH_QB = [ENTRY(QB, 50n), ...ENTRIES];
const QUOTED_OUT = 123_456_789n;
const TICK = 92100 - 120 - 8047; // half way up the ramp from the dear edge
/** GRAD's pool as StateView answers it: one full-range position of `POOL_LIQ` and one narrow position just above the price. */
const POOL_LIQ = 10n ** 24n;
const NARROW = { lower: 84_000, upper: 84_600, liq: 3n * 10n ** 23n };
const NETS = new Map<number, bigint>([[-887_220, POOL_LIQ], [887_220, -POOL_LIQ], [NARROW.lower, NARROW.liq], [NARROW.upper, -NARROW.liq]]);
const BITMAP = (() => {
  const m = new Map<number, bigint>();
  for (const t of NETS.keys()) { const c = Math.floor(t / 60); m.set(c >> 8, (m.get(c >> 8) ?? 0n) | (1n << BigInt(c & 0xff))); }
  return m;
})();
/** GRAD's pool as `simulateSwap` reads it whole, at `cushion`. */
const gradSnapshot = (cushion = { cushionLower: 0, cushionLiq: 0n }, gapSkim = true): SwapSnapshot => ({
  pool: { sqrtPriceX96: sqrtPriceAtTick(TICK), tick: TICK, liquidity: POOL_LIQ, tickSpacing: 60, protocolFee: 0, bitmap: BITMAP, liquidityNet: NETS },
  quoteIsCurrency1: false, floorTick: 92100,
  rates: { skimBuyFarBps: 100, skimSellFarBps: 100, skimBuyAtFloorBps: 0, skimSellAtFloorBps: 500, creatorBuyBps: 50, creatorSellBps: 75, holderBuyBps: 0, holderSellBps: 0, teamFeeBps: 25 },
  cushion, gapSkim,
});

type Handler = { abi: Abi; fns: Record<string, (args: readonly unknown[]) => unknown> };
function chain(chainId = CHAIN, allowance = { erc20: 0n, permit2: 0n }, o: { funded?: boolean; creator?: Address; cushion?: { lower: number; liq: bigint }; badPoolId?: boolean; stateView?: boolean; quoter?: 'fixed' | 'pool' } = {}) {
  const entries = ENTRIES_WITH_QB;
  const tokenFns = (t: { token: Address; launch: Address }, sym: string): Handler => ({
    abi: [...TOKEN_ABI, ...WALLET_SCAN_ABI] as Abi,
    fns: { allowance: () => allowance.erc20, launch: () => t.launch, name: () => `${sym} token`, symbol: () => sym, decimals: () => 18, totalSupply: () => 1_000_000_000n * E18 },
  });
  const launchFns = (graduated: boolean): Handler => ({
    abi: LAUNCH_ABI as Abi,
    fns: { graduated: () => graduated, funded: () => o.funded ?? SALE.funded, saleTokens: () => SALE.saleTokens, sold: () => SALE.sold, raised: () => SALE.raised, p0: () => SALE.p0, p1: () => SALE.p1, tranches: () => SALE.tranches, teamFeeBps: () => SALE.teamFeeBps, allocation: () => 1000n * E18, contributed: () => 225n * 10n ** 12n + 7n },
  });
  const slot0 = (BigInt(TICK) & 0xffffffn) << 160n | 1n;
  const handlers: Record<string, Handler> = {
    [D.registry.toLowerCase()]: { abi: REGISTRY_ABI as Abi, fns: { count: () => BigInt(entries.length), isRegistered: ([l]) => [BOND.launch, GRAD.launch, QB.launch].includes(l as Address), page: ([s, n]) => entries.slice(Number(s), Number(s as bigint) + Number(n)) } },
    [D.factory.toLowerCase()]: { abi: FACTORY_ABI as Abi, fns: { creatorOf: () => o.creator ?? CREATOR } },
    [HOOK.toLowerCase()]: { abi: HOOK_ABI as Abi, fns: {
      poolForAccount: ([l]) => (l === BOND.launch ? poolIdOf(BOND.token, HOOK) : l === QB.launch ? poolIdOf(QB.token, HOOK, STOCK) : o.badPoolId ? poolIdOf(GRAD.token, STRANGER) : poolIdOf(GRAD.token, HOOK)),
      poolState: ([id]) => STATES[id as string],
      execute: ([, data]) => {
        calls.push('hook.execute');
        if (decodeFunctionData({ abi: POOL_ABI, data: data as Hex }).functionName !== 'stairs') throw new Error('only stairs');
        const c = o.cushion ?? { lower: 0, liq: 0n };
        return encodeFunctionResult({ abi: POOL_ABI, functionName: 'stairs', result: stairsAnswer(c.lower, c.liq) } as never);
      },
    } },
    [D.poolManager.toLowerCase()]: { abi: POOL_STATE_ABI as Abi, fns: { extsload: ([slot]) => (slot === poolStateSlot(poolIdOf(GRAD.token, HOOK)) ? `0x${slot0.toString(16).padStart(64, '0')}` : '0x' + '00'.repeat(32)) } },
    /* 'pool': the quoter answers what GRAD's pool gives (the whole-map simulation); 'fixed' (default): QUOTED_OUT. */
    [D.quoter!.toLowerCase()]: { abi: V4_QUOTER_ABI as Abi, fns: { quoteExactInputSingle: ([p]) => {
      if (o.quoter !== 'pool') return [QUOTED_OUT, 100_000n];
      const { zeroForOne, exactAmount } = p as { zeroForOne: boolean; exactAmount: bigint };
      const c = o.cushion ?? { lower: 0, liq: 0n };
      return [simulateSwap(gradSnapshot({ cushionLower: c.lower, cushionLiq: c.liq }), zeroForOne ? 'buy' : 'sell', exactAmount).amountOut, 100_000n];
    } } },
    ...(o.stateView ? { [D.stateView!.toLowerCase()]: { abi: STATE_VIEW_ABI as Abi, fns: {
      getSlot0: () => [sqrtPriceAtTick(TICK), TICK, 0, 0],
      getLiquidity: () => POOL_LIQ,
      getTickBitmap: ([, w]) => BITMAP.get(Number(w)) ?? 0n,
      getTickLiquidity: ([, t]) => [0n, NETS.get(Number(t)) ?? 0n],
    } } } : {}),
    [PERMIT2.toLowerCase()]: { abi: PERMIT2_ABI as Abi, fns: { allowance: () => [allowance.permit2, 2 ** 40, 0] } },
    [BOND.token.toLowerCase()]: tokenFns(BOND, 'BOND'),
    [GRAD.token.toLowerCase()]: tokenFns(GRAD, 'GRAD'),
    [QB.token.toLowerCase()]: tokenFns(QB, 'QB'),
    [UNREGISTERED.token.toLowerCase()]: tokenFns(UNREGISTERED, 'UNREG'),
    [STOCK.toLowerCase()]: { abi: TOKEN_ABI as Abi, fns: { allowance: () => allowance.erc20, symbol: () => 'STK', decimals: () => 6 } },
    [QB.launch.toLowerCase()]: launchFns(false),
    [BOND.launch.toLowerCase()]: launchFns(false),
    [GRAD.launch.toLowerCase()]: launchFns(true),
  };
  const calls: string[] = [];
  const transport = custom({
    async request({ method, params }: { method: string; params?: unknown }) {
      if (method === 'eth_chainId') return `0x${chainId.toString(16)}`;
      if (method === 'eth_blockNumber') return '0x10';
      if (method !== 'eth_call') throw new Error(`unexpected ${method}`);
      const { to, data } = (params as [{ to: Address; data: Hex }])[0];
      if (to.toLowerCase() === BROKEN.toLowerCase()) throw new Error('fetch failed');
      const h = handlers[to.toLowerCase()];
      if (!h) throw Object.assign(new Error('execution reverted'), { code: 3, data: '0x' });
      const { functionName, args } = decodeFunctionData({ abi: h.abi, data });
      calls.push(`${to.slice(0, 6)}.${functionName}`);
      const fn = h.fns[functionName];
      if (!fn) throw new Error(`no fake for ${functionName}`);
      if (functionName === 'poolState') return encodeAbiParameters([{ type: 'tuple', components: POOL_STATE_COMPONENTS }] as never, [fn(args ?? [])] as never);
      return encodeFunctionResult({ abi: h.abi, functionName, result: fn(args ?? []) } as never);
    },
  }, { retryCount: 0 });
  return { transport, calls };
}

const adapterOn = (c = chain()) => createEvmAdapter({ chains: [CHAIN], transports: { [CHAIN]: c.transport }, now: () => 1_000_000_000_000 });
/** The same contracts as a caller's row on a local chain that names the older contracts' rules. */
const LOCAL = 31337;
const OLDER_ROW: Deployment = { ...D, chainId: LOCAL, rules: OLDER_RULES };
const olderOn = (c = chain(LOCAL)) => createEvmAdapter({ chains: [LOCAL], transports: { [LOCAL]: c.transport }, deployments: { [LOCAL]: OLDER_ROW }, now: () => 1_000_000_000_000 });
const localRef = (token: Address) => ({ chain: { family: 'evm' as const, chainId: LOCAL }, token });
const ref = (token: Address) => ({ chain: { family: 'evm' as const, chainId: CHAIN }, token });

describe('createEvmAdapter over a mocked transport', () => {
  it('refuses a chain with no FLOOR set, and a chain with no transport, by name', () => {
    expect(() => createEvmAdapter({ chains: [42161], rpcUrls: { 42161: 'http://x' } })).toThrow(/registry is not deployed yet/);
    expect(() => createEvmAdapter({ chains: [CHAIN] })).toThrow(/no client, transport or rpc URL/);
  });

  it('lists ONE registry page per call, newest first, with a cursor', async () => {
    const a = adapterOn();
    const first = await a.listLaunches({ family: 'evm', chainId: CHAIN }, { limit: 1 });
    expect(first.launches.map((l) => l.symbol)).toEqual(['BOND']);
    expect(first.cursor).toBe('2');
    const second = await a.listLaunches({ family: 'evm', chainId: CHAIN }, { limit: 1, cursor: first.cursor! });
    expect(second.launches.map((l) => l.symbol)).toEqual(['GRAD']);
    expect(second.cursor).toBe('1');
    const third = await a.listLaunches({ family: 'evm', chainId: CHAIN }, { limit: 5, cursor: second.cursor! });
    expect(third.launches.map((l) => l.symbol)).toEqual(['QB']);
    expect(third.cursor).toBeNull();
    await expect(a.listLaunches({ family: 'evm', chainId: CHAIN }, { cursor: '9' })).rejects.toThrow(/cursor/);
    await expect(a.listLaunches({ family: 'evm', chainId: CHAIN }, { limit: 101 })).rejects.toThrow(/limit/);
  });

  it('reads a bonding launch: the sale book, its own FLOOR Platform fee, no published floor yet', async () => {
    const L = await adapterOn().getLaunch(ref(BOND.token));
    expect(L).toMatchObject({ phase: 'bonding', name: 'BOND token', symbol: 'BOND', decimals: 18, mode: 2, creator: CREATOR, floorPriceX18: null, quote: { address: null, symbol: 'ETH', decimals: 18 } });
    expect(L.raw.sale).toMatchObject({ tranches: 10, teamFeeBps: 100, graduated: false });
    expect(L.raw.poolRates.teamFeeBps).toBe(25);
    expect(L.raw.poolId).toBe(poolIdOf(BOND.token, HOOK));
  });

  it('reads a graduated launch: the published floor off floorTick, the band width', async () => {
    const L = await adapterOn().getLaunch(ref(GRAD.token));
    expect(L.phase).toBe('graduated');
    expect(L.floorPriceX18).toBe(priceX18AtTick(92100, 18, 18));
    expect(L.raw.sale).toBeNull();
    expect(L.raw.bidWidth).toBe(120);
  });

  it('an unfunded sale is pending, and a factory that names nobody is a null creator', async () => {
    const L = await adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { funded: false, creator: NATIVE })).getLaunch(ref(BOND.token));
    expect(L.phase).toBe('pending');
    expect(L.creator).toBeNull();
    await expect(adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { funded: false })).quote(ref(BOND.token), 'buy', E18)).rejects.toMatchObject({ code: 'no_quote' });
  });

  it('refuses a token that is not a launch here, and a client on another chain', async () => {
    await expect(adapterOn().getLaunch(ref(STRANGER))).rejects.toMatchObject({ code: 'not_a_launch' });
    await expect(adapterOn(chain(1)).getLaunch(ref(BOND.token))).rejects.toThrow(/connected to chain 1/);
  });

  it('quotes a bonding buy off the walk, with the whole take', async () => {
    const q = await adapterOn().quote(ref(BOND.token), 'buy', 101n * 10n ** 16n);
    const w = saleBuyQuote({ ...SALE }, 101n * 10n ** 16n, 18, 'ceil'); // the current contracts round costs up
    expect(q).toMatchObject({ venue: 'sale', feesExact: true, amountOut: w.got, amountIn: w.spent + w.fee });
    assertWholeTake(q.fees);
    expect(q.fees.find((f) => f.kind === 'floorPlatform')!.amount).toBe(w.fee);
    await expect(adapterOn().quote(ref(BOND.token), 'sell', 1n)).rejects.toThrow(/opts.trader/);
    const viaQuote = await adapterOn().quote(ref(BOND.token), 'sell', 500n * E18, { trader: SELLER });
    const sb = await adapterOn().quoteSellBack(ref(BOND.token), SELLER, 500n * E18);
    // the current contracts (the Sepolia row): stateless, the tokens at p0
    expect(sb.amountOut).toBe(225n * 10n ** 12n / 2n - (225n * 10n ** 12n / 2n) / 100n);
    expect(viaQuote).toEqual(sb);
    // older contracts pay the position's pro-rata share: half of 225e12 + 7 wei, rounded down, carries 3 wei of the residue
    const old = await olderOn().quoteSellBack(localRef(BOND.token), SELLER, 500n * E18);
    const share = (225n * 10n ** 12n + 7n) / 2n;
    expect(old.amountOut).toBe(share - share / 100n);
  });

  it('the sale walk follows the row: the current contracts round costs up, older ones down', async () => {
    // 1 wei buys floor(1e18 / 225e9) base units: 0 wei rounded down (the sale sells it for nothing), 1 rounded up
    const cur = await adapterOn().quote(ref(BOND.token), 'buy', 2n);
    const old = await olderOn().quote(localRef(BOND.token), 'buy', 2n);
    expect(old.amountOut).toBe(saleBuyQuote(SALE, 2n, 18, 'floor').got);
    expect(cur.amountOut).toBe(saleBuyQuote(SALE, 2n, 18, 'ceil').got);
    expect(saleBuyQuote(SALE, 2n, 18, 'floor').spent).not.toBe(saleBuyQuote(SALE, 2n, 18, 'ceil').spent);
    expect(cur.amountIn).toBe(1n);
    expect(old.amountIn).toBe(0n);
  });

  it('quotes a pool trade off the V4Quoter, and lists the five parts at the pool\'s own rates', async () => {
    const q = await adapterOn().quote(ref(GRAD.token), 'buy', E18);
    expect(q).toMatchObject({ venue: 'pool', feesExact: false, amountOut: QUOTED_OUT, amountIn: E18 });
    assertWholeTake(q.fees);
    const by = Object.fromEntries(q.fees.map((f) => [f.kind, f.amount]));
    expect(by.floorSkim).toBe(E18 * 50n / 10_000n); // half way up the buy ramp: 50 of 100 bps
    expect(by.creator).toBe(E18 * 50n / 10_000n);
    expect(by.floorPlatform).toBe(E18 * 25n / 10_000n);
    expect(by.holder).toBe(0n);
  });

  it('the gap buy skim: on a row whose contracts carry it, a buy\'s skim is scaled by the cushion gap read through execute(stairs)', async () => {
    // 5,109 ticks above the cushion's top: the ramp's 50 bps × 1.5 = 75
    const far = chain(CHAIN, { erc20: 0n, permit2: 0n }, { cushion: { lower: TICK + 5_109, liq: 1n } });
    const q = await adapterOn(far).quote(ref(GRAD.token), 'buy', E18);
    expect(q.fees.find((f) => f.kind === 'floorSkim')!.amount).toBe(E18 * 75n / 10_000n);
    expect(far.calls).toContain('hook.execute');
    // 2,232 ticks: not scaled
    const near = await adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { cushion: { lower: TICK + 2_232, liq: 1n } })).quote(ref(GRAD.token), 'buy', E18);
    expect(near.fees.find((f) => f.kind === 'floorSkim')!.amount).toBe(E18 * 50n / 10_000n);
    // a sell is never scaled, and never reads the cushion
    const sellSide = chain(CHAIN, { erc20: 0n, permit2: 0n }, { cushion: { lower: TICK + 5_109, liq: 1n } });
    await adapterOn(sellSide).quote(ref(GRAD.token), 'sell', E18);
    expect(sellSide.calls).not.toContain('hook.execute');
    // older contracts have no gap skim: not read, not scaled
    const old = chain(LOCAL, { erc20: 0n, permit2: 0n }, { cushion: { lower: TICK + 5_109, liq: 1n } });
    const oq = await olderOn(old).quote(localRef(GRAD.token), 'buy', E18);
    expect(oq.fees.find((f) => f.kind === 'floorSkim')!.amount).toBe(E18 * 50n / 10_000n);
    expect(old.calls).not.toContain('hook.execute');
  });

  it('a pool SELL\'s parts are charged at the POST-swap tick, read off the SDK\'s own walk at the quoter\'s block', async () => {
    const amountIn = 2n * 10n ** 23n; // walks up past the narrow position, toward the floor
    const c = chain(CHAIN, { erc20: 0n, permit2: 0n }, { stateView: true, quoter: 'pool' });
    const q = await adapterOn(c).quote(ref(GRAD.token), 'sell', amountIn);
    const sim = simulateSwap(gradSnapshot(), 'sell', amountIn);
    expect(q.amountOut).toBe(sim.amountOut);
    const pre = skimBpsAt('sell', 92100 - 120 - TICK, gradSnapshot().rates);
    const post = skimBpsAt('sell', 92100 - 120 - marketTick(sim.tickAfter, false), gradSnapshot().rates);
    expect(pre).toBe(300);
    expect(post).toBeGreaterThan(pre);
    const by = Object.fromEntries(q.fees.map((f) => [f.kind, f.amount]));
    const leg = q.amountOut + by.floorSkim! + by.creator! + by.holder! + by.floorPlatform!;
    expect(hookTakes('sell', leg, post, gradSnapshot().rates)).toMatchObject({ floorSkim: by.floorSkim, creator: by.creator, holder: by.holder, floorPlatform: by.floorPlatform });
    // the LP fee was charged in the token: stated in quote at the walk's average price, so the breakdown is not exact
    expect(by.lp).toBe((sim.fees.lp * leg) / (amountIn - sim.fees.lp));
    expect(q.feesExact).toBe(false);
    // the walk read only the bitmap words it crossed (and a few ahead), never the whole map
    expect(c.calls.filter((x) => x.endsWith('.getTickBitmap')).length).toBeLessThan(10);
    expect(c.calls).not.toContain('hook.execute');
  });

  it('a pool BUY whose walk gives the quoter\'s amount is exact: the hook\'s four at the pre-swap tick, the walk\'s own LP fee', async () => {
    const cushion = { lower: TICK + 5_109, liq: 1n };
    const q = await adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { stateView: true, quoter: 'pool', cushion })).quote(ref(GRAD.token), 'buy', E18);
    const sim = simulateSwap(gradSnapshot({ cushionLower: cushion.lower, cushionLiq: cushion.liq }), 'buy', E18);
    expect(q).toMatchObject({ venue: 'pool', feesExact: true, amountOut: sim.amountOut });
    const by = Object.fromEntries(q.fees.map((f) => [f.kind, f.amount]));
    expect(sim.skimBps).toBe(gapSkimBps(50, { cushionLower: cushion.lower, cushionLiq: 1n }, TICK));
    expect(by).toEqual({ lp: sim.fees.lp, floorSkim: E18 * 75n / 10_000n, creator: E18 * 50n / 10_000n, holder: 0n, floorPlatform: E18 * 25n / 10_000n });
  });

  it('a walk that disagrees with the quoter falls back to the rates; a sell then takes the walk\'s post-swap skim', async () => {
    const amountIn = 2n * 10n ** 23n;
    const q = await adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { stateView: true })).quote(ref(GRAD.token), 'sell', amountIn);
    expect(q).toMatchObject({ feesExact: false, amountOut: QUOTED_OUT });
    const sim = simulateSwap(gradSnapshot(), 'sell', amountIn);
    const by = Object.fromEntries(q.fees.map((f) => [f.kind, f.amount]));
    const leg = QUOTED_OUT + by.floorSkim! + by.creator! + by.holder! + by.floorPlatform!;
    expect(by.floorSkim).toBe(leg * BigInt(sim.skimBps) / 10_000n);
    expect(sim.skimBps).toBeGreaterThan(300);
  });

  it('refuses a launch whose pool key does not hash to the hook\'s id', async () => {
    await expect(adapterOn(chain(CHAIN, { erc20: 0n, permit2: 0n }, { badPoolId: true })).getLaunch(ref(GRAD.token))).rejects.toMatchObject({ code: 'contract_mismatch', message: expect.stringMatching(/hashes to/) });
  });

  it('builds a pool trade through FloorSwapRatchet: the contract is the spender Permit2 approves, then swapAndRatchet; exact approvals on request', async () => {
    const params = { launch: ref(GRAD.token), side: 'sell' as const, amountIn: 3n * E18, minAmountOut: 77n, trader: SELLER };
    const tx = await adapterOn().buildTrade(params, { swapRatchet: { ratchet: true }, approvalAmount: 'exact' });
    const calls = (tx as { calls: readonly { to: string; data: string; value: bigint }[] }).calls;
    expect(calls.map((c) => c.to)).toEqual([GRAD.token, PERMIT2, D.swapRatchet]);
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: calls[0]!.data as Hex }).args).toEqual([PERMIT2, 3n * E18]);
    expect(decodeFunctionData({ abi: PERMIT2_ABI, data: calls[1]!.data as Hex }).args).toEqual([GRAD.token, D.swapRatchet, 3n * E18, Number(PERMIT2_MAX_EXPIRATION)]);
    const { functionName, args } = decodeFunctionData({ abi: FLOOR_SWAP_RATCHET_ABI, data: calls[2]!.data as Hex });
    expect(functionName).toBe('swapAndRatchet');
    expect(args[0]).toMatchObject({ poolId: poolIdOf(GRAD.token, HOOK), amountIn: 3n * E18, minOut: 77n, ratchet: true, sweep: false });
    // a native buy through it carries the value
    const buy = await adapterOn().buildTrade({ ...params, side: 'buy', amountIn: E18 }, { swapRatchet: { ratchet: false, sweep: true } });
    expect((buy as { calls: readonly { to: string; value: bigint }[] }).calls).toEqual([expect.objectContaining({ to: D.swapRatchet, value: E18 })]);
  });

  it('a token-quoted bonding buy approves the EXACT amount to the launch, resetting a short non-zero allowance first', async () => {
    const amount = 5n * 10n ** 6n;
    const fresh = await adapterOn().buildTrade({ launch: ref(QB.token), side: 'buy', amountIn: amount, minAmountOut: 1n, trader: SELLER });
    const fc = (fresh as { calls: readonly { to: string; data: string; value: bigint; gas?: bigint }[] }).calls;
    expect(fc.map((c) => c.to)).toEqual([STOCK, QB.launch]);
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: fc[0]!.data as Hex }).args).toEqual([QB.launch, amount]);
    expect(decodeFunctionData({ abi: LAUNCH_ABI, data: fc[1]!.data as Hex })).toEqual({ functionName: 'buy', args: [amount] });
    expect(fc[1]!.value).toBe(0n);
    const short = await adapterOn(chain(CHAIN, { erc20: 1n, permit2: 0n })).buildTrade({ launch: ref(QB.token), side: 'buy', amountIn: amount, minAmountOut: 1n, trader: SELLER });
    const sc = (short as { calls: readonly { to: string; data: string }[] }).calls;
    expect(sc.map((c) => c.to)).toEqual([STOCK, STOCK, QB.launch]);
    expect(sc.slice(0, 2).map((c) => decodeFunctionData({ abi: TOKEN_ABI, data: c.data as Hex }).args)).toEqual([[QB.launch, 0n], [QB.launch, amount]]);
    const unlimited = await adapterOn().buildTrade({ launch: ref(QB.token), side: 'buy', amountIn: amount, minAmountOut: 1n, trader: SELLER }, { approvalAmount: 'unlimited' });
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: (unlimited as { calls: readonly { data: string }[] }).calls[0]!.data as Hex }).args).toEqual([QB.launch, 2n ** 255n]);
  });

  it('builds a pool sell as ONE batch: the missing ERC-20 → Permit2 and Permit2 → router approvals ON CHAIN, then the swap', async () => {
    const params = { launch: ref(GRAD.token), side: 'sell' as const, amountIn: E18, minAmountOut: 77n, trader: SELLER };
    const tx = await adapterOn().buildTrade(params);
    expect(tx.family).toBe('evm');
    const calls = (tx as { calls: readonly { to: string; data: string; value: bigint }[] }).calls;
    expect(calls.map((c) => c.to)).toEqual([GRAD.token, PERMIT2, D.routers[0]!.address]);
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: calls[0]!.data as Hex }).functionName).toBe('approve');
    expect(decodeFunctionData({ abi: PERMIT2_ABI, data: calls[1]!.data as Hex }).args?.slice(0, 2)).toEqual([GRAD.token, D.routers[0]!.address]);
    const { args } = decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: calls[2]!.data as Hex });
    expect(args[0]).toBe('0x10');
    expect(args[2]).toBe(1_000_000_000n + 600n);
    // Both allowances already granted (and the Permit2 grant unexpired): the swap alone.
    const granted = await adapterOn(chain(CHAIN, { erc20: 2n ** 255n, permit2: 2n ** 159n })).buildTrade({ ...params, deadline: 1_234n });
    expect((granted as { calls: readonly unknown[] }).calls.length).toBe(1);
    const only = (granted as { calls: readonly { data: string }[] }).calls[0]!;
    expect(decodeFunctionData({ abi: UNIVERSAL_ROUTER_ABI, data: only.data as Hex }).args[2]).toBe(1_234n);
  });

  it('builds a bonding buy and refuses one the sale would not fill to the minimum', async () => {
    const a = adapterOn();
    // Every bonding buy carries the gas floor: any of them can become the one that sells the sale out and graduates it.
    const tx = await a.buildTrade({ launch: ref(BOND.token), side: 'buy', amountIn: E18, minAmountOut: 1n, trader: SELLER });
    expect(tx).toEqual({ family: 'evm', chainId: CHAIN, calls: [{ to: BOND.launch, data: '0xa6f2ae3a', value: E18, gas: BOND_BUY_GAS_FLOOR }] });
    const out = await a.buildTrade({ launch: ref(BOND.token), side: 'buy', amountIn: 200n * E18, minAmountOut: 1n, trader: SELLER });
    expect((out as { calls: readonly { gas?: bigint }[] }).calls[0]!.gas).toBe(5_100_000n);
    await expect(a.buildTrade({ launch: ref(BOND.token), side: 'buy', amountIn: E18, minAmountOut: 10n ** 40n, trader: SELLER })).rejects.toMatchObject({ code: 'min_out_unmet' });
    await expect(a.buildTrade({ launch: ref(BOND.token), side: 'buy', amountIn: E18, minAmountOut: 0n, trader: SELLER })).rejects.toThrow(FloorEvmError);
    const sb = await a.buildTrade({ launch: ref(BOND.token), side: 'sell', amountIn: 10n * E18, minAmountOut: 1n, trader: SELLER });
    expect(sb).toEqual({ family: 'evm', chainId: CHAIN, calls: [expect.objectContaining({ to: BOND.token, value: 0n })] });
    const sbCall = (sb as { calls: readonly { data: string; gas?: bigint }[] }).calls[0]!;
    expect(decodeFunctionData({ abi: TOKEN_ABI, data: sbCall.data as Hex })).toEqual({ functionName: 'sellBack', args: [10n * E18] });
    expect(sbCall.gas).toBeUndefined();
    // a sell-back past the position, or under the minimum, is refused
    await expect(a.buildTrade({ launch: ref(BOND.token), side: 'sell', amountIn: 1001n * E18, minAmountOut: 1n, trader: SELLER })).rejects.toMatchObject({ code: 'bad_params' });
    await expect(a.buildTrade({ launch: ref(BOND.token), side: 'sell', amountIn: 10n * E18, minAmountOut: 10n ** 30n, trader: SELLER })).rejects.toMatchObject({ code: 'min_out_unmet' });
  });
});

describe('the readers', () => {
  const client = (c = chain()) => createPublicClient({ transport: c.transport });
  it('readSale reads the book in one batch and asks the fee only of a set that carries it', async () => {
    const r = await readSale(client(), D, BOND.launch);
    expect(r).toEqual({ funded: true, saleTokens: SALE.saleTokens, sold: 0n, raised: 0n, p0: SALE.p0, p1: SALE.p1, tranches: 10, graduated: false, teamFeeBps: 100 });
    const c = chain();
    const { teamTreasury: _t, ...noFee } = D;
    expect((await readSale(client(c), noFee as Deployment, BOND.launch)).teamFeeBps).toBe(0);
    expect(c.calls.some((x) => x.endsWith('.teamFeeBps'))).toBe(false);
  });
  it('readSalePosition reads allocation and contributed', async () => {
    expect(await readSalePosition(client(), BOND.launch, SELLER)).toEqual({ allocation: 1000n * E18, contributed: 225n * 10n ** 12n + 7n });
  });
  it('readQuoteAsset: native is the chain\'s symbol and 18; a token reads its own', async () => {
    expect(await readQuoteAsset(client(), CHAIN, null)).toEqual({ address: null, symbol: 'ETH', decimals: 18 });
    expect(await readQuoteAsset(client(), 999_999, null)).toEqual({ address: null, symbol: 'native', decimals: 18 });
    expect(await readQuoteAsset(client(), 999_999, null, 'XYZ')).toEqual({ address: null, symbol: 'XYZ', decimals: 18 });
    expect(await readQuoteAsset(client(), CHAIN, STOCK)).toEqual({ address: STOCK, symbol: 'STK', decimals: 6 });
  });
  it('launchOfToken: the token\'s launch, registered here; a revert or an unregistered launch is not_a_launch; a transport failure is not an answer', async () => {
    expect(await launchOfToken(client(), D, BOND.token)).toBe(BOND.launch);
    await expect(launchOfToken(client(), D, STRANGER)).rejects.toMatchObject({ code: 'not_a_launch' });
    await expect(launchOfToken(client(), D, UNREGISTERED.token)).rejects.toMatchObject({ code: 'not_a_launch', message: expect.stringMatching(/not registered/) });
    const err = await launchOfToken(client(), D, BROKEN).catch((e: unknown) => e);
    expect(err).not.toBeInstanceOf(FloorEvmError);
  });
  it('isRevert tells a contract revert from a transport failure', async () => {
    const c = client();
    const revert = await c.readContract({ address: STRANGER, abi: WALLET_SCAN_ABI, functionName: 'launch' }).catch((e: unknown) => e);
    const broken = await c.readContract({ address: BROKEN, abi: WALLET_SCAN_ABI, functionName: 'launch' }).catch((e: unknown) => e);
    expect(isRevert(revert)).toBe(true);
    expect(isRevert(broken)).toBe(false);
    expect(isRevert(new Error('execution reverted'))).toBe(false);
  });
});

describe('decodePoolState', () => {
  it('refuses any width but the engine\'s', () => {
    expect(() => decodePoolState(('0x' + '00'.repeat(32 * 56)) as Hex)).toThrow(/56 words; the engine's layout has 57/);
  });
});
