// createSolanaAdapter over an in-memory RPC: the shared Launch shape, the phase, the floor price, sale quotes with the
// whole six-part take (and a graduating buy's extra lamports), token-quoted launches, the program-version gate, the
// graduated swap path, the trader link's deadline margin, listLaunches paging, and the refusals by name.

import assert from 'node:assert/strict';
import {
  addressToBytes,
  APPROVED_HOOK,
  bytesToAddress,
  DYNAMIC_FEE_FLAG,
  encodeAccount,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  COMPUTE_BUDGET_PROGRAM_ID,
  findApprovedHook,
  findEventAuthority,
  SYSTEM_PROGRAM_ID,
  findAta,
  findBitmap,
  findPool,
  findPosition,
  findTickArray,
  LOOKUP_TABLE_META_SIZE,
  POOL,
  SPL_TOKEN_PROGRAM_ID,
  TICK_ARRAY_BITMAP,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
} from '@floorsupply/carpenter-sdk';
import { getBase64Encoder, getCompiledTransactionMessageDecoder, getTransactionDecoder } from '@solana/kit';
import { solanaRef, type LaunchRef } from '@floorsupply/sdk';
import type { Address } from '@solana/kit';

import { createSolanaAdapter, SolanaAdapterError, swapToQuote, TRADER_LINK_DEADLINE_MARGIN_SECONDS, type SolanaAdapterRpc } from '../src/adapter.ts';
import { FLOOR_CALLBACK_ACCOUNTS, FLOOR_TICK_SPACING, LAUNCH_STATE_BONDING, LAUNCH_STATE_GRADUATED } from '../src/constants.ts';
import { findHolderBook } from '../src/holders.ts';
import { FLOOR_POOL, LAUNCH, LAUNCH_CONFIG, REGISTRY_ENTRY } from '../src/layouts.ts';
import { programIdsFromDeployment } from '../src/adapters.ts';
import { DEFAULT_PROGRAM_IDS, type ProgramIds, findBuyerPosition, findFloorPool, findHookAuthority, findLaunch, findLaunchConfig, findRegistryEntry } from '../src/pda.ts';
import { graduationAccounts } from '../src/predict.ts';
import { buildBuy } from '../src/tx/builders.ts';
import { deploymentFor, type ClusterDeployment } from '../src/deployments.ts';
import { ED25519_PROGRAM_ID } from '../src/tx/traderRef.ts';
import type { Quoter, SwapQuote } from '../src/quote/quoter.ts';
import { addr, FIXTURE_STAIR_STEPS, fixtureLaunch, specRent } from './helpers.ts';
import { test } from './node-test.ts';

const ids = DEFAULT_PROGRAM_IDS;
const DEFAULT_IDS = DEFAULT_PROGRAM_IDS;
const SIX = ['lp', 'floorSkim', 'creator', 'holder', 'floorPlatform', 'carpenterPlatform'];
const TRADER = addr(2);
const NOW = 1_800_000_000;

type Acc = { owner: Address; data: Uint8Array };

/** The account reads `fromKitRpc` makes, over a map, counting each method's calls. */
function memRpc(accounts: Map<Address, Acc>, calls: Record<string, number>): SolanaAdapterRpc {
  const b64 = (d: Uint8Array) => Buffer.from(d).toString('base64');
  const view = (a: Address) => {
    const v = accounts.get(a);
    return v ? { owner: v.owner, lamports: 1n, executable: false, data: [b64(v.data), 'base64'] } : null;
  };
  const count = (m: string) => (calls[m] = (calls[m] ?? 0) + 1);
  return {
    getAccountInfo: (a: Address) => ({ send: async () => (count('getAccountInfo'), { context: { slot: 7n }, value: view(a) }) }),
    getMultipleAccounts: (as: readonly Address[]) => ({ send: async () => (count('getMultipleAccounts'), { context: { slot: 7n }, value: as.map(view) }) }),
    getLatestBlockhash: () => ({ send: async () => ({ context: { slot: 7n }, value: { blockhash: 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1Cr7xE9bbG1vv', lastValidBlockHeight: 100n } }) }),
    getMinimumBalanceForRentExemption: (len: bigint) => ({ send: async () => specRent(Number(len)) }),
  } as unknown as SolanaAdapterRpc;
}

/** A deployment table: the programs and deployment-wide accounts every trade names. */
async function deploymentTable(ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<Address[]> {
  const pdas = await Promise.all([ids.carpenterAmm, ids.floorHook, ids.floorLaunch, ids.floorHolders].map(async (p) => (await findEventAuthority(p)).address));
  return [
    ids.carpenterAmm, ids.floorHook, ids.floorLaunch, ids.floorHolders, ...pdas, (await findHookAuthority(ids.floorHook)).address,
    WSOL_MINT, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, COMPUTE_BUDGET_PROGRAM_ID,
  ];
}

/** A frozen lookup table account holding `addresses`. */
function lookupTable(addresses: readonly Address[]): Uint8Array {
  const out = new Uint8Array(LOOKUP_TABLE_META_SIZE + 32 * addresses.length);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, 1, true);
  dv.setBigUint64(4, 0xffff_ffff_ffff_ffffn, true);
  addresses.forEach((a, i) => out.set(addressToBytes(a), LOOKUP_TABLE_META_SIZE + 32 * i));
  return out;
}

/** An SPL / Token-2022 mint account (82 bytes, no extensions). */
function mintAccount(decimals: number, supply: bigint): Uint8Array {
  const out = new Uint8Array(82);
  new DataView(out.buffer).setBigUint64(36, supply, true);
  out[44] = decimals;
  out[45] = 1;
  return out;
}

/** A Token-2022 token account with the CPI Guard extension locked. */
function cpiGuardedAccount(mint: Address, owner: Address): Uint8Array {
  const out = new Uint8Array(165 + 1 + 4 + 1);
  out.set(addressToBytes(mint), 0);
  out.set(addressToBytes(owner), 32);
  out[108] = 1; // initialized
  out[165] = 2; // AccountType::Account
  const dv = new DataView(out.buffer);
  dv.setUint16(166, 11, true); // CpiGuard
  dv.setUint16(168, 1, true);
  out[170] = 1; // lock_cpi
  return out;
}

/** An address that sorts before every launch mint (a token quote is currency 0). */
function lowAddr(n: number): Address {
  const b = new Uint8Array(32);
  b[0] = 0x01;
  b[31] = n;
  return bytesToAddress(b);
}
const QUOTE = lowAddr(9);

interface WorldOpts {
  state?: number;
  floorTick?: number;
  traderLink?: { deadline: bigint } | true;
  sellBackPayout?: 'native' | 'wsol';
  /** Quoted in the SPL token `QUOTE` (6 decimals) instead of wSOL. */
  token?: boolean;
  /** Leave the token quote's mint account out of the cluster. */
  dropQuoteMint?: boolean;
  /** A holder rate, so the launch's mint carries the holders transfer hook. */
  hooked?: boolean;
  /** `ApprovedHook.callback_accounts` (default 4); `null` leaves the record out. */
  approvedCallbacks?: number | null;
  /** The graduated pool's `hook_callback_accounts` (default 4). */
  poolCallbacks?: number;
  /** The trader's base ATA carries a locked CPI Guard. */
  cpiGuard?: boolean;
  /** The trader holds a sale position. */
  position?: boolean;
  quoter?: Partial<Quoter>;
  /** Another cluster: its joined deployment row (its ids and lookup table) and the adapter's cluster name. */
  cluster?: { name: 'mainnet' | 'devnet'; row: ClusterDeployment };
}

const SWAP_QUOTE: SwapQuote = {
  amountIn: 1_000n, amountOut: 900n,
  fees: { lp: 3n, skim: 4n, creator: 5n, holder: 6n, platformCut: 1n, team: 7n, protocol: 8n, protocolCurrency: 0 },
  sqrtAfter: 1n << 64n, tickAfter: 0, sqrtAfterTrade: 1n << 64n, tickAfterTrade: 0,
  arraysTouched: [0], actionArrays: [-3600], allArrays: [0, -3600],
  replaceActions: [
    { kind: 'modify_position', salt: 2n, tickLower: -3600, tickUpper: -3480, liquidityDelta: -5n },
    { kind: 'swap', zeroForOne: true, amountSpecified: -1n, sqrtPriceLimitX64: 1n },
    { kind: 'modify_position', salt: 2n, tickLower: -3720, tickUpper: -3600, liquidityDelta: 5n },
  ],
  replace: null, signals: 0, hookDelta0: 0n, hookDelta1: 0n,
};

async function world(o: WorldOpts = {}) {
  const ids = o.cluster ? programIdsFromDeployment(o.cluster.row) : DEFAULT_IDS;
  const deploymentAlt = (o.cluster ? o.cluster.row.alt : DEPLOYMENT_ALT) as Address;
  const clusterName = o.cluster?.name ?? 'devnet';
  const state = o.state ?? LAUNCH_STATE_BONDING;
  const f = await fixtureLaunch(ids);
  const launchAddr = (await findLaunch(f.mint, ids)).address;
  let launchState: Record<string, unknown> = { ...f.state, state, ...(o.hooked ? { holderBuy: 100, holderSell: 100 } : {}) };
  if (o.token) {
    const ammPool = (await findPool({ mint0: QUOTE, mint1: f.mint, fee: DYNAMIC_FEE_FLAG, tickSpacing: FLOOR_TICK_SPACING, hook: ids.floorHook }, ids)).address;
    launchState = {
      ...launchState, version: 3, quoteMint: QUOTE, ammPool,
      floorPool: (await findFloorPool(ammPool, ids)).address,
      wsolVault: (await findAta(launchAddr, QUOTE, SPL_TOKEN_PROGRAM_ID)).address,
    };
  }
  const L = launchState as typeof f.state & { quoteMint?: Address };
  const accounts = new Map<Address, Acc>();
  accounts.set(launchAddr, { owner: ids.floorLaunch, data: encodeAccount(LAUNCH, launchState) });
  accounts.set(f.mint, { owner: TOKEN_2022_PROGRAM_ID, data: mintAccount(6, 1_000_000_000_000_000n) });
  if (o.token && !o.dropQuoteMint) accounts.set(QUOTE, { owner: SPL_TOKEN_PROGRAM_ID, data: mintAccount(6, 10n ** 15n) });
  accounts.set((await findFloorPool(L.ammPool, ids)).address, {
    owner: ids.floorHook,
    data: encodeAccount(FLOOR_POOL, { ammPool: L.ammPool, floorTick: o.floorTick ?? 0, stairSteps: FIXTURE_STAIR_STEPS, minBid: 50_000_000n }),
  });
  accounts.set((await findLaunchConfig(ids)).address, { owner: ids.floorLaunch, data: encodeAccount(LAUNCH_CONFIG, { attestor: ATTESTOR, cluster: 1 }) });
  if (o.approvedCallbacks !== null) {
    accounts.set((await findApprovedHook(ids.floorHook, ids)).address, {
      owner: ids.carpenterAmm,
      data: encodeAccount(APPROVED_HOOK, { version: 1, callbackAccounts: o.approvedCallbacks ?? FLOOR_CALLBACK_ACCOUNTS, hookProgram: ids.floorHook }),
    });
  }
  // A graduated launch's pool and bitmap.
  const quoteMint = o.token ? QUOTE : WSOL_MINT;
  accounts.set(L.ammPool, {
    owner: ids.carpenterAmm,
    data: encodeAccount(POOL, {
      version: 1, tickSpacing: FLOOR_TICK_SPACING, hookCallbackAccounts: o.poolCallbacks ?? FLOOR_CALLBACK_ACCOUNTS, hookFlags: 0x60cc,
      mint0: quoteMint, mint1: f.mint, hook: ids.floorHook, vault0: addr(170), vault1: addr(171),
      tokenProgram0: SPL_TOKEN_PROGRAM_ID, tokenProgram1: TOKEN_2022_PROGRAM_ID, sqrtPriceX64: 1n << 64n, decimals0: o.token ? 6 : 9, decimals1: 6,
    }),
  });
  accounts.set((await findBitmap(L.ammPool, ids)).address, {
    owner: ids.carpenterAmm,
    data: encodeAccount(TICK_ARRAY_BITMAP, { pool: L.ammPool, tickSpacing: FLOOR_TICK_SPACING, arraysPerSide: 124 }, [0n, 0n, 0n, 0n]),
  });
  const baseAta = (await findAta(TRADER, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  if (o.cpiGuard) accounts.set(baseAta, { owner: TOKEN_2022_PROGRAM_ID, data: cpiGuardedAccount(f.mint, TRADER) });
  if (o.position) accounts.set((await findBuyerPosition(launchAddr, TRADER, ids)).address, { owner: ids.floorLaunch, data: new Uint8Array(64) });
  // The cluster's deployment table, and the launch's own table holding every account a buy and its prepare name (built
  // for another buyer, so the trader's own accounts stay static keys).
  accounts.set(deploymentAlt, { owner: addr(99), data: lookupTable(await deploymentTable(ids)) });
  const probe = await buildBuy({
    buyer: addr(3), launch: L, lamportsIn: 1n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, ids,
    ...(o.token ? { quote: { mint: QUOTE, tokenProgram: SPL_TOKEN_PROGRAM_ID }, floorPool: { minBid: 50_000_000n, stairSteps: FIXTURE_STAIR_STEPS } } : {}),
  });
  accounts.set(TABLE, { owner: addr(99), data: lookupTable(probe.accounts.filter((a) => a !== addr(3))) });
  TABLES.clear();
  for (const t of [TABLE, deploymentAlt]) TABLES.set(t, decodeTableAddresses(accounts.get(t)!.data));
  const calls: Record<string, number> = {};
  const quoter = {
    saleBuy: (_l: Uint8Array, lamportsIn: bigint) => ({ tokens: 1_000n, cost: lamportsIn - 14n, spent: lamportsIn - 5n, teamFee: 9n, graduates: false }),
    saleSellBack: () => ({ payout: 2n, received: 1n, teamFee: 1n }),
    quoteSwap: () => SWAP_QUOTE,
    ...o.quoter,
  } as unknown as Quoter;
  const link = o.traderLink === true ? { deadline: BigInt(NOW + 3_600) } : o.traderLink;
  const adapter = createSolanaAdapter({
    clusters: { [clusterName]: { rpc: memRpc(accounts, calls), ids, ...(o.cluster ? { row: o.cluster.row } : {}) } },
    quoter,
    launchTable: async () => TABLE,
    now: () => NOW,
    quoteSymbol: (m) => (m === QUOTE ? 'USDQ' : undefined),
    ...(o.sellBackPayout ? { sellBackPayout: o.sellBackPayout } : {}),
    ...(link ? { traderLink: async () => ({ tier1: addr(160), tier2: null, tier3: null, deadline: link.deadline, signature: new Uint8Array(64).fill(7) }) } : {}),
  });
  const ref: LaunchRef = { chain: solanaRef(clusterName), token: f.mint };
  return { adapter, ref, f, L, accounts, calls, baseAta };
}

const TABLE = addr(150);
const ATTESTOR = addr(151);
const DEPLOYMENT_ALT = deploymentFor('solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1').alt as Address;
/** The current world's lookup tables, by address (for resolving a v0 message's keys). */
const TABLES = new Map<Address, Address[]>();

function decodeTableAddresses(data: Uint8Array): Address[] {
  const out: Address[] = [];
  for (let at = LOOKUP_TABLE_META_SIZE; at + 32 <= data.length; at += 32) out.push(bytesToAddress(data.subarray(at, at + 32)));
  return out;
}

/** A base64 wire transaction's keys (static, then its lookup tables' writable, then readonly entries) and instructions. */
function decode(tx: string) {
  const { messageBytes } = getTransactionDecoder().decode(new Uint8Array(getBase64Encoder().encode(tx)));
  const m = getCompiledTransactionMessageDecoder().decode(messageBytes) as unknown as {
    staticAccounts: Address[];
    instructions: { programAddressIndex: number; accountIndices?: number[]; data?: Uint8Array }[];
    addressTableLookups?: { lookupTableAddress: Address; writableIndexes: number[]; readonlyIndexes: number[] }[];
  };
  const lookups = m.addressTableLookups ?? [];
  const writable = lookups.flatMap((l) => l.writableIndexes.map((i) => TABLES.get(l.lookupTableAddress)![i]!));
  const readonly = lookups.flatMap((l) => l.readonlyIndexes.map((i) => TABLES.get(l.lookupTableAddress)![i]!));
  const keys = [...m.staticAccounts, ...writable, ...readonly];
  return {
    keys,
    ixs: m.instructions.map((ix) => ({ program: keys[ix.programAddressIndex]!, accounts: (ix.accountIndices ?? []).map((i) => keys[i]!), data: ix.data ?? new Uint8Array() })),
  };
}

/** Whether a base64 wire transaction names `a` as an account. */
function names(tx: string, a: Address): boolean {
  return decode(tx).keys.includes(a);
}

/** Whether `a`'s 32 bytes appear anywhere in the transaction (an instruction's data included). */
function carries(tx: string, a: Address): boolean {
  const bytes = new Uint8Array(getBase64Encoder().encode(tx));
  const key = addressToBytes(a);
  outer: for (let i = 0; i + 32 <= bytes.length; i++) {
    for (let j = 0; j < 32; j++) if (bytes[i + j] !== key[j]) continue outer;
    return true;
  }
  return false;
}

const solana = (tx: Awaited<ReturnType<ReturnType<typeof createSolanaAdapter>['buildTrade']>>) => {
  if (tx.family !== 'solana') throw new Error('not solana');
  return tx;
};

test('getLaunch: phase from the state byte, creator, quote SOL, no floor price while bonding', async () => {
  const { adapter, ref, f } = await world();
  const l = await adapter.getLaunch(ref);
  assert.equal(l.phase, 'bonding');
  assert.equal(l.creator, f.state.creator);
  assert.equal(l.floorPriceX18, null);
  assert.equal(l.totalSupply, 1_000_000_000_000_000n);
  assert.deepEqual(l.quote, { address: null, symbol: 'SOL', decimals: 9 });
  assert.equal((await adapter.getLaunch({ ...ref })).ref.token, f.mint, 'the mint is returned as given, never case-folded');
  const pending = await world({ state: 2 }); // any state but bonding / graduated
  assert.equal((await pending.adapter.getLaunch(pending.ref)).phase, 'pending');
});

test('getLaunch on a graduated launch: floorPriceX18 is whole quote per whole token × 1e18, from FloorPool.floorTick', async () => {
  // Tick 0 is a raw price of 1 base unit per quote unit: 1 token (1e6 units) costs 1e6 lamports = 0.001 SOL.
  const { adapter, ref } = await world({ state: LAUNCH_STATE_GRADUATED });
  const l = await adapter.getLaunch(ref);
  assert.equal(l.phase, 'graduated');
  assert.equal(l.floorPriceX18, 10n ** 15n);
});

test('a token-quoted launch: the quote is the mint with its own decimals and symbol, and the floor is priced in it', async () => {
  const { adapter, ref } = await world({ state: LAUNCH_STATE_GRADUATED, token: true });
  const l = await adapter.getLaunch(ref);
  assert.deepEqual(l.quote, { address: QUOTE, symbol: 'USDQ', decimals: 6 });
  // Tick 0 with a 6-decimal quote: 1 token (1e6 units) costs 1e6 quote units = 1 whole quote.
  assert.equal(l.floorPriceX18, 10n ** 18n);
});

test('a token-quoted launch whose quote mint cannot be read is refused not_found (never wSOL, never decimals 0)', async () => {
  const { adapter, ref } = await world({ state: LAUNCH_STATE_GRADUATED, token: true, dropQuoteMint: true });
  await assert.rejects(adapter.getLaunch(ref), (e: unknown) => e instanceof SolanaAdapterError && e.code === 'not_found' && e.message.includes(QUOTE));
  const bonding = await world({ token: true, dropQuoteMint: true });
  await assert.rejects(
    bonding.adapter.buildTrade({ launch: bonding.ref, side: 'buy', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER }),
    (e: unknown) => e instanceof SolanaAdapterError && e.code === 'not_found' && e.message.includes(QUOTE),
  );
});

test('a sale buy quote lists the whole six-part take, is exact, and carries the unspent refund', async () => {
  const { adapter, ref } = await world();
  const q = await adapter.quote(ref, 'buy', 1_000_000n);
  assert.equal(q.amountOut, 1_000n);
  assert.deepEqual(q.fees.map((x) => x.kind), SIX);
  assert.equal(q.fees.find((x) => x.kind === 'floorPlatform')!.amount, 9n);
  assert.equal(q.feesExact, true);
  assert.equal(q.refund, 5n);
  assert.equal(q.graduates, false);
  assert.equal(q.maxExtraNative, undefined, 'a buy that does not graduate pays no graduation rent');
});

test('a graduating sale buy quote says so and carries the most lamports graduation can take on top (max_graduation_lamports)', async () => {
  const { adapter, ref, L, accounts } = await world({ quoter: { saleBuy: () => ({ tokens: 9n, cost: 9n, teamFee: 0n, spent: 9n, graduates: true }) } });
  const q = await adapter.quote(ref, 'buy', 9n);
  assert.equal(q.graduates, true);
  const args = { launch: L, rent: specRent, floorPool: { minBid: 50_000_000n, stairSteps: FIXTURE_STAIR_STEPS }, ids };
  // The accounts that already exist (here the pool and its bitmap) are credited.
  const existing = new Set((await graduationAccounts(args)).accounts.map((m) => m.address).filter((a) => accounts.has(a)));
  assert.ok(existing.size > 0);
  const want = await graduationAccounts({ ...args, existing });
  assert.ok(want.maxGraduationLamports > 0n);
  assert.equal(q.maxExtraNative, want.maxGraduationLamports);
});

test('a sell-back quote is checked against the trader\'s position; refusals by name', async () => {
  const { adapter, ref } = await world({ position: true });
  const q = await adapter.quote(ref, 'sell', 5n, { trader: TRADER });
  assert.equal(q.amountOut, 1n);
  assert.deepEqual(q.fees.map((x) => x.kind), SIX);
  await assert.rejects(adapter.quote(ref, 'sell', 1n), (e: unknown) => e instanceof SolanaAdapterError && e.code === 'needs_trader');
  await assert.rejects(adapter.quote(ref, 'sell', 1n, { trader: addr(9) }), (e: unknown) => e instanceof SolanaAdapterError && e.code === 'not_found');
  await assert.rejects(adapter.getLaunch({ ...ref, token: addr(77) }), (e: unknown) => e instanceof SolanaAdapterError && e.code === 'not_found');
  await assert.rejects(adapter.getLaunch({ ...ref, chain: solanaRef('mainnet') }), (e: unknown) => e instanceof SolanaAdapterError && e.code === 'no_cluster');
});

test('swapToQuote: six parts; a sell converts the token-side LP fee to quote and is marked inexact; a missing part throws', () => {
  const ref: LaunchRef = { chain: solanaRef('devnet'), token: addr(1) };
  const fees = { lp: 30n, skim: 4n, creator: 3n, holder: 2n, platformCut: 1n, team: 5n, protocol: 6n, protocolCurrency: 0 as const };
  const buy = swapToQuote(ref, 'buy', { ...SWAP_QUOTE, amountIn: 1_000n, amountOut: 500n, fees });
  assert.deepEqual(buy.fees.map((x) => [x.kind, x.amount]), [['lp', 30n], ['floorSkim', 4n], ['creator', 3n], ['holder', 2n], ['floorPlatform', 5n], ['carpenterPlatform', 6n]]);
  assert.equal(buy.feesExact, true);
  const sell = swapToQuote(ref, 'sell', { ...SWAP_QUOTE, amountIn: 1_000n, amountOut: 500n, fees });
  assert.equal(sell.fees[0]!.amount, 15n);
  assert.equal(sell.feesExact, false);
  const { holder: _h, ...partial } = fees;
  assert.throws(() => swapToQuote(ref, 'buy', { ...SWAP_QUOTE, fees: partial } as unknown as SwapQuote), SolanaAdapterError);
});

test('program version: an ApprovedHook record with another callback count, a missing record, or an older pool is refused program_version', async () => {
  for (const o of [{ approvedCallbacks: 2 }, { approvedCallbacks: null }] as const) {
    const w = await world(o);
    await assert.rejects(
      w.adapter.buildTrade({ launch: w.ref, side: 'buy', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER }),
      (e: unknown) => e instanceof SolanaAdapterError && e.code === 'program_version',
    );
  }
  const old = await world({ state: LAUNCH_STATE_GRADUATED, poolCallbacks: 2 });
  await assert.rejects(
    old.adapter.buildTrade({ launch: old.ref, side: 'buy', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER }),
    (e: unknown) => e instanceof SolanaAdapterError && e.code === 'program_version' && e.message.includes('pool'),
  );
  // the gate passes on the current set
  const ok = await world({ state: LAUNCH_STATE_GRADUATED });
  assert.equal(solana(await ok.adapter.buildTrade({ launch: ok.ref, side: 'buy', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER })).transactions.length, 1);
});

test('buildTrade, a sale buy on an unprepared launch: [prepare, buy] in one UnsignedTx, each a v0 transaction for the trader', async () => {
  const { adapter, ref } = await world();
  const tx = solana(await adapter.buildTrade({ launch: ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(tx.cluster, 'devnet');
  assert.equal(tx.transactions.length, 2, 'prepare, then buy');
  for (const t of tx.transactions) assert.ok(names(t, TRADER), 'the trader pays and signs');
});

test('buildTrade, a token-quoted sale buy pays from the trader\'s quote ATA', async () => {
  const { adapter, ref } = await world({ token: true });
  const tx = solana(await adapter.buildTrade({ launch: ref, side: 'buy', amountIn: 1_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(tx.transactions.length, 2);
  const quoteAta = (await findAta(TRADER, QUOTE, SPL_TOKEN_PROGRAM_ID)).address;
  assert.ok(names(tx.transactions[1]!, quoteAta), 'the buyer\'s quote ATA rides in the buy');
});

test('buildTrade with traderLink: an unlinked trader gets the link transaction FIRST (the Ed25519 attestor check), then the trade', async () => {
  const { adapter, ref } = await world({ traderLink: true });
  const tx = solana(await adapter.buildTrade({ launch: ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(tx.transactions.length, 3, 'link, prepare, buy');
  assert.ok(names(tx.transactions[0]!, ED25519_PROGRAM_ID), 'the link carries the Ed25519 verification');
  assert.ok(carries(tx.transactions[0]!, ATTESTOR), 'against the LaunchConfig attestor');
  assert.ok(!names(tx.transactions[2]!, ED25519_PROGRAM_ID));
});

test('buildTrade with traderLink: an attestation that expires within the margin is skipped, and the trade goes alone', async () => {
  const soon = await world({ traderLink: { deadline: BigInt(NOW + TRADER_LINK_DEADLINE_MARGIN_SECONDS - 1) } });
  const a = solana(await soon.adapter.buildTrade({ launch: soon.ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(a.transactions.length, 2, 'prepare, buy — no link');
  assert.ok(!a.transactions.some((t) => names(t, ED25519_PROGRAM_ID)));
  const later = await world({ traderLink: { deadline: BigInt(NOW + TRADER_LINK_DEADLINE_MARGIN_SECONDS) } });
  const b = solana(await later.adapter.buildTrade({ launch: later.ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(b.transactions.length, 3, 'at the margin the link still rides');
});

test('buildTrade, a sale sell-back: lamports by default (sell_back); sellBackPayout "wsol" pays into the wSOL ATA (sell_back_wsol)', async () => {
  const wsolAta = (await findAta(TRADER, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)).address;
  const native = await world();
  const n = solana(await native.adapter.buildTrade({ launch: native.ref, side: 'sell', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(n.transactions.length, 1);
  assert.ok(!names(n.transactions[0]!, wsolAta), 'the native payout names no wSOL account');
  const wrapped = await world({ sellBackPayout: 'wsol' });
  const w = solana(await wrapped.adapter.buildTrade({ launch: wrapped.ref, side: 'sell', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(w.transactions.length, 1);
  assert.ok(names(w.transactions[0]!, wsolAta), 'the wSOL payout lands in the seller\'s wSOL ATA');
});

test('buildTrade, a hooked sell-back carries the holder tail (the launch\'s holder book)', async () => {
  const { adapter, ref, f } = await world({ hooked: true });
  const book = (await findHolderBook(f.mint, ids)).address;
  const t = solana(await adapter.buildTrade({ launch: ref, side: 'sell', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER })).transactions[0]!;
  assert.ok(names(t, book));
  const plain = await world();
  assert.ok(!names(solana(await plain.adapter.buildTrade({ launch: plain.ref, side: 'sell', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER })).transactions[0]!, book));
});

test('buildTrade on a graduated pool: one swap carrying the walk\'s and the actions\' arrays, the re-place positions and the hook slice', async () => {
  const { adapter, ref, L } = await world({ state: LAUNCH_STATE_GRADUATED });
  const tx = solana(await adapter.buildTrade({ launch: ref, side: 'sell', amountIn: 1_000n, minAmountOut: 900n, trader: TRADER }));
  assert.equal(tx.transactions.length, 1);
  const { keys, ixs } = decode(tx.transactions[0]!);
  const hookAuthority = (await findHookAuthority(ids.floorHook)).address;
  const bidPosition = (await findPosition(L.ammPool, hookAuthority, 2n, ids)).address;
  assert.ok(keys.includes(bidPosition), 'the re-place\'s position (from replaceActions)');
  for (const st of [0, -3600]) assert.ok(keys.includes((await findTickArray(L.ammPool, st, ids)).address), `tick array ${st}`);
  assert.ok(keys.includes((await findFloorPool(L.ammPool, ids)).address), 'the FloorPool (hook slice)');
  // the action's array is created first if missing (initialize_tick_array), then the swap
  const amm = ixs.filter((i) => i.program === ids.carpenterAmm);
  assert.equal(amm.length, 2, 'initialize_tick_array, swap');
  assert.ok(amm[0]!.accounts.includes((await findTickArray(L.ammPool, -3600, ids)).address));
  // no holder hook on an unhooked launch, and no approve on an unguarded source
  assert.ok(!keys.includes((await findHolderBook(L.mint, ids)).address));
  assert.ok(!ixs.some((i) => i.program === TOKEN_2022_PROGRAM_ID && i.data[0] === 13));
});

test('buildTrade on a graduated pool: a hooked buy carries the holders transfer hook and opens the buyer\'s record', async () => {
  const { adapter, ref, f } = await world({ state: LAUNCH_STATE_GRADUATED, hooked: true });
  const { keys, ixs } = decode(solana(await adapter.buildTrade({ launch: ref, side: 'buy', amountIn: 1_000n, minAmountOut: 900n, trader: TRADER })).transactions[0]!);
  assert.ok(keys.includes((await findHolderBook(f.mint, ids)).address), 'the holder book (transfer-hook accounts)');
  assert.ok(keys.includes(ids.floorHolders));
  const touch = ixs.findIndex((i) => i.program === ids.floorHolders);
  const swap = ixs.findIndex((i, n) => i.program === ids.carpenterAmm && n > touch);
  assert.ok(touch >= 0 && swap > touch, 'touch (the buyer\'s record) before the swap');
});

test('buildTrade on a graduated pool: a sell from a CPI-Guard account approves the pool right before the swap and revokes after', async () => {
  const { adapter, ref, L, baseAta } = await world({ state: LAUNCH_STATE_GRADUATED, cpiGuard: true });
  const { ixs } = decode(solana(await adapter.buildTrade({ launch: ref, side: 'sell', amountIn: 1_000n, minAmountOut: 900n, trader: TRADER })).transactions[0]!);
  const swap = ixs.findIndex((i) => i.program === ids.carpenterAmm && i.accounts.includes(baseAta));
  const approve = ixs[swap - 1]!;
  assert.equal(approve.program, TOKEN_2022_PROGRAM_ID);
  assert.equal(approve.data[0], 13, 'ApproveChecked');
  assert.equal(approve.accounts[0], baseAta);
  assert.equal(approve.accounts[2], L.ammPool, 'delegate = the pool');
  assert.equal(ixs[swap + 1]!.data[0], 5, 'Revoke');
});

/** A cluster with `n` registry slots; `missing` slots have no RegistryEntry. Slot 0's launch is graduated. */
async function registryWorld(n: number, missing: ReadonlySet<number>) {
  const f = await fixtureLaunch(ids);
  const accounts = new Map<Address, Acc>();
  accounts.set((await findLaunchConfig(ids)).address, { owner: ids.floorLaunch, data: encodeAccount(LAUNCH_CONFIG, { registryCount: BigInt(n) }) });
  const mints: Address[] = [];
  for (let i = 0; i < n; i++) {
    const mint = addr(300 + i);
    mints.push(mint);
    const launch = (await findLaunch(mint, ids)).address;
    const ammPool = addr(400 + i);
    const state = i === 0 ? LAUNCH_STATE_GRADUATED : LAUNCH_STATE_BONDING;
    accounts.set(launch, { owner: ids.floorLaunch, data: encodeAccount(LAUNCH, { ...f.state, mint, ammPool, state }) });
    accounts.set(mint, { owner: TOKEN_2022_PROGRAM_ID, data: mintAccount(6, 1n) });
    accounts.set((await findFloorPool(ammPool, ids)).address, { owner: ids.floorHook, data: encodeAccount(FLOOR_POOL, { ammPool, floorTick: 0 }) });
    if (!missing.has(i)) {
      accounts.set((await findRegistryEntry(BigInt(i), ids)).address, {
        owner: ids.floorLaunch,
        data: encodeAccount(REGISTRY_ENTRY, { index: BigInt(i), launch, mint, ammPool, mode: 1, createdSlot: BigInt(i) }),
      });
    }
  }
  const calls: Record<string, number> = {};
  const adapter = createSolanaAdapter({ clusters: { devnet: { rpc: memRpc(accounts, calls), ids } }, quoter: {} as Quoter });
  return { adapter, mints, calls };
}

test('listLaunches pages newest first, and a missing registry entry does not end paging early', async () => {
  const { adapter, mints } = await registryWorld(5, new Set([2]));
  const chain = solanaRef('devnet');
  const seen: string[] = [];
  const cursors: (string | null)[] = [];
  let cursor: string | undefined;
  for (let pages = 0; pages < 10; pages++) {
    const page = await adapter.listLaunches(chain, { limit: 2, ...(cursor ? { cursor } : {}) });
    seen.push(...page.launches.map((l) => l.ref.token));
    cursors.push(page.cursor);
    if (!page.cursor) break;
    cursor = page.cursor;
  }
  assert.deepEqual(seen, [mints[4], mints[3], mints[1], mints[0]], 'every listed launch, newest first, slot 2 skipped');
  assert.deepEqual(cursors, ['3', '1', null]);
  const last = await adapter.listLaunches(chain, { limit: 10 });
  assert.equal(last.cursor, null);
  assert.equal(last.launches.find((l) => l.ref.token === mints[0])!.floorPriceX18, 10n ** 15n, 'a graduated row reads its FloorPool');
});

test('listLaunches batches its reads: three getMultipleAccounts calls for a page, however many rows', async () => {
  for (const n of [2, 6]) {
    const { adapter, calls } = await registryWorld(n, new Set());
    const page = await adapter.listLaunches(solanaRef('devnet'), { limit: 10 });
    assert.equal(page.launches.length, n);
    assert.equal(calls['getMultipleAccounts'], 3, `${n} rows: entries, launches, then mints and FloorPools together`);
    assert.equal(calls['getAccountInfo'], 1, 'the LaunchConfig');
  }
});

/** A joined row for a cluster other than devnet: fresh random program ids, its own lookup table. */
async function otherClusterRow(): Promise<ClusterDeployment> {
  const rnd = (): Address => {
    const b = new Uint8Array(32);
    crypto.getRandomValues(b);
    return bytesToAddress(b);
  };
  const [carpenterAmm, floorHook, floorLaunch, floorHolders, floorVesting] = [rnd(), rnd(), rnd(), rnd(), rnd()];
  const o = { carpenterAmm, floorHook, floorLaunch, floorHolders } as ProgramIds;
  return {
    name: 'mainnet',
    chainKey: 'solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp',
    programs: { carpenterAmm, floorHook, floorLaunch, floorHolders, floorVesting },
    accounts: { ammConfig: rnd(), hookConfig: (await findLaunchConfig(o)).address, launchConfig: (await findLaunchConfig(o)).address },
    alt: rnd(),
    programDataHash: { carpenterAmm: null, floorHook: null, floorLaunch: null, floorHolders: null, floorVesting: null },
  };
}

test('another cluster\'s row: every trade the adapter builds names that cluster\'s programs and tables, never a devnet key', async () => {
  const row = await otherClusterRow();
  const other = programIdsFromDeployment(row);
  const d = DEFAULT_PROGRAM_IDS;
  const devnet = [
    d.carpenterAmm, d.floorHook, d.floorLaunch, d.floorHolders, DEPLOYMENT_ALT,
    ...(await Promise.all([d.carpenterAmm, d.floorHook, d.floorLaunch, d.floorHolders].map(async (p) => (await findEventAuthority(p)).address))),
    (await findHookAuthority(d.floorHook)).address, (await findLaunchConfig(d)).address,
  ];
  const check = (what: string, txs: readonly string[]) => {
    for (const t of txs) for (const k of devnet) assert.ok(!carries(t, k), `${what}: devnet key ${k}`);
  };
  const sale = await world({ cluster: { name: 'mainnet', row }, hooked: true, position: true, traderLink: true });
  assert.equal(sale.ref.chain.family === 'solana' && sale.ref.chain.cluster, 'mainnet');
  const launch = await sale.adapter.getLaunch(sale.ref);
  assert.equal(launch.ref.token, sale.f.mint);
  const buy = solana(await sale.adapter.buildTrade({ launch: sale.ref, side: 'buy', amountIn: 1_000_000_000n, minAmountOut: 1n, trader: TRADER }));
  assert.equal(buy.cluster, 'mainnet');
  assert.equal(buy.transactions.length, 3, 'link, prepare, buy');
  assert.ok(buy.transactions.every((t) => names(t, other.floorLaunch)), 'the other cluster\'s floor_launch');
  check('sale buy', buy.transactions);
  check('sell-back', solana(await sale.adapter.buildTrade({ launch: sale.ref, side: 'sell', amountIn: 1_000n, minAmountOut: 1n, trader: TRADER })).transactions);
  const pool = await world({ cluster: { name: 'mainnet', row }, state: LAUNCH_STATE_GRADUATED, hooked: true, cpiGuard: true });
  for (const side of ['buy', 'sell'] as const) {
    const tx = solana(await pool.adapter.buildTrade({ launch: pool.ref, side, amountIn: 1_000n, minAmountOut: 900n, trader: TRADER }));
    assert.ok(names(tx.transactions[0]!, other.carpenterAmm), `${side}: the other cluster's carpenter_amm`);
    assert.ok(carries(tx.transactions[0]!, row.alt as Address), `${side}: through the other cluster's lookup table`);
    check(`graduated ${side}`, tx.transactions);
  }
});
