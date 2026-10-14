// stateView over `memorySource`: owner AND discriminator gate every read, the StateView-analogue
// getters, `positionsForOwner`'s memcmp at account offset 48, `listPools` paging, presence,
// `listLaunches` in both modes keyed by chainKey, and the quoter's byte adapter.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import {  type Address } from '@solana/kit';

import { readSwapQuoteAccounts } from '../src/adapters.ts';
import {

  
  DYNAMIC_FEE_FLAG,
  SPL_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  encodeAccount,
  encodeTickArray,

  POOL,

  
  TICK_ARRAY,
  TICK_ARRAY_BITMAP,

  

  findBitmap,

  findTickArray,
  memorySource,
  type RawAccount,

  
  getMintExtensions,

  

  

  

  
  bitmapBit,
} from '@floorlaunchpad/carpenter-sdk';
import { LAUNCH, LAUNCH_CONFIG, QUOTE_POLICY, REGISTRY_ENTRY, FLOOR_POOL } from '../src/layouts.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findFloorPool,
  findLaunch,
  findLaunchConfig,
  findQuotePolicy,
  findRegistryEntry,
} from '../src/pda.ts';
import { getLaunch, getQuotePolicy, listLaunches } from '../src/stateView.ts';
import { addr, fixtureLaunch } from './helpers.ts';

const ids = DEFAULT_PROGRAM_IDS;
const own = (owner: Address, data: Uint8Array): RawAccount => ({ owner, lamports: 1n, executable: false, data });

async function poolFixture() {
  const pool = addr(50);
  const s = 60;
  const words = [0n, 0n, 0n, 0n];
  const mark = (start: number) => {
    const bit = bitmapBit(start, s);
    words[Math.floor(bit / 64)]! |= 1n << BigInt(bit % 64);
  };
  for (const st of [-7200, -3600, 0, 3600, 36_000]) mark(st);
  const accounts = new Map<Address, RawAccount>();
  accounts.set(
    pool,
    own(ids.carpenterAmm, encodeAccount(POOL, { tickSpacing: s, tickCurrent: 10, sqrtPriceX64: 1n << 64n, liquidity: 500n, lpFee: 3000, feeGrowthGlobal0X64: 1000n, feeGrowthGlobal1X64: 2000n, mint0: WSOL_MINT, mint1: addr(51) })),
  );
  accounts.set((await findBitmap(pool, ids)).address, own(ids.carpenterAmm, encodeAccount(TICK_ARRAY_BITMAP, { pool, tickSpacing: s, arraysPerSide: 124 }, words)));
  for (const st of [-7200, -3600, 0, 3600, 36_000]) {
    const ticks = st === 0 ? [{ liquidityGross: 5n, liquidityNet: 5n, feeGrowthOutside0X64: 100n, feeGrowthOutside1X64: 200n }] : [];
    const upper = st === 0 ? { ...ticks[0]!, liquidityNet: -5n, feeGrowthOutside0X64: 300n, feeGrowthOutside1X64: 400n } : undefined;
    const arr = ticks.slice();
    if (upper) arr[1] = upper; // ticks 0 and 60
    accounts.set((await findTickArray(pool, st, ids)).address, own(ids.carpenterAmm, encodeTickArray({ pool, startIndex: st, ticks: arr })));
  }
  return { pool, accounts, s };
}

/**
 * The FLOOR roster entry, read the way the AMM's is: owner, length AND discriminator, or `null`.
 * ⛔ Mutations this kills: the owner check dropped (a lookalike under the AMM's id would read as a
 *    policy); the length check dropped (a truncated account would decode garbage); `retired` not
 *    surfaced (the script would re-list a retired quote).
 */
test('getQuotePolicy is presence under floor_hook: right owner, length and discriminator, or null; retired is surfaced', async () => {
  const mint = addr(52);
  const pda = (await findQuotePolicy(mint, ids)).address;
  const bytes = encodeAccount(QUOTE_POLICY, { version: 1, decimals: 6, retired: 1, quoteMint: mint, tokenProgram: addr(53), minBidFloor: 1_000n, minBidCeil: 5_000_000n, maxBountyCeil: 500_000n, minBid: 50_000n, maxBounty: 40_000n });
  const good = new Map<Address, RawAccount>([[pda, own(ids.floorHook, bytes)]]);
  const v = (await getQuotePolicy(memorySource(good), mint)).value;
  assert.equal(v?.retired, 1);
  assert.equal(v?.decimals, 6);
  assert.equal(v?.quoteMint, mint);
  assert.deepEqual([v?.minBidFloor, v?.minBidCeil, v?.maxBountyCeil, v?.minBid, v?.maxBounty], [1_000n, 5_000_000n, 500_000n, 50_000n, 40_000n]);
  const wrongOwner = new Map<Address, RawAccount>([[pda, own(ids.carpenterAmm, bytes)]]);
  assert.equal((await getQuotePolicy(memorySource(wrongOwner), mint)).value, null, 'owned by another program is null');
  const short = new Map<Address, RawAccount>([[pda, own(ids.floorHook, bytes.subarray(0, 100))]]);
  assert.equal((await getQuotePolicy(memorySource(short), mint)).value, null, 'a short account is null');
  const lamportsOnly = new Map<Address, RawAccount>([[pda, { owner: ids.floorHook, lamports: 1n, executable: false, data: new Uint8Array(0) }]]);
  assert.equal((await getQuotePolicy(memorySource(lamportsOnly), mint)).value, null, 'lamports never mean presence');
  assert.equal((await getQuotePolicy(memorySource(new Map()), mint)).value, null, 'absent is null');
});

test('listLaunches: index mode reads registry_count then entries by PDA; scan mode uses the discriminator; both key by chainKey', async () => {
  const f = await fixtureLaunch();
  const chainKey = 'solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1';
  const accounts = new Map<Address, RawAccount>();
  accounts.set((await findLaunchConfig(ids)).address, own(ids.floorLaunch, encodeAccount(LAUNCH_CONFIG, { registryCount: 3n })));
  for (let i = 0n; i < 3n; i++) {
    accounts.set((await findRegistryEntry(i, ids)).address, own(ids.floorLaunch, encodeAccount(REGISTRY_ENTRY, { index: i, launch: i === 2n ? f.launch : addr(200 + Number(i)), mint: i === 2n ? f.mint : addr(210 + Number(i)), ammPool: addr(220), mode: 1, createdSlot: 10n + i })));
  }
  accounts.set(f.launch, own(ids.floorLaunch, encodeAccount(LAUNCH, { mint: f.mint, state: 0, sold: 5n, raised: 6n, priceX64: f.state.priceX64 })));
  const src = memorySource(accounts, 5n);
  const idx = await listLaunches(src, chainKey);
  assert.equal(idx.slot, 5n);
  assert.deepEqual(idx.value.map((r) => r.registryIndex), [2n, 1n, 0n], 'newest first');
  assert.equal(idx.value[0]?.chainKey, chainKey);
  assert.equal(idx.value[0]?.id, f.mint, 'id is the mint');
  assert.equal(idx.value[0]?.state?.raised, 6n);
  assert.equal(idx.value[1]?.state, null, 'an unreadable Launch is null, never invented');
  const paged = await listLaunches(src, chainKey, { before: 2n, limit: 1 });
  assert.deepEqual(paged.value.map((r) => r.registryIndex), [1n]);
  const scan = await listLaunches(src, chainKey, { mode: 'scan', withState: false });
  assert.deepEqual(scan.value.map((r) => r.registryIndex), [2n, 1n, 0n]);
  assert.equal(scan.value[0]?.state, null);
  await assert.rejects(listLaunches(src, 'solana:devnet'), /genesis32/);
  await assert.rejects(listLaunches(memorySource(new Map()), chainKey), /LaunchConfig/);
  assert.equal((await getLaunch(src, f.mint)).value?.sold, 5n);
});

test('readSwapQuoteAccounts gathers raw bytes for the quoter (plain and FLOOR pools) and tags the oldest slot', async () => {
  const { pool, accounts } = await poolFixture();
  const plain = await readSwapQuoteAccounts(memorySource(accounts, 9n), pool, { radius: 1 });
  assert.equal(plain.slot, 9n);
  assert.equal(plain.value.isFloor, false);
  assert.equal(plain.value.floorPool, null);
  assert.equal(plain.value.supply1, null);
  assert.deepEqual(plain.value.tickArrayStarts, [-3600, 0, 3600]);
  assert.equal(plain.value.tickArrays.length, 3);
  assert.equal(plain.value.tickArrays[0]?.length, 8 + TICK_ARRAY.size);
  assert.equal(plain.value.pool.length, 8 + POOL.size);
  assert.equal(plain.value.poolKey.length, 32);
  const byStarts = await readSwapQuoteAccounts(memorySource(accounts), pool, { starts: [36_000, 0, 0] });
  assert.deepEqual(byStarts.value.tickArrayStarts, [0, 36_000]);
  // a FLOOR pool: floor_pool and the base mint's supply come along
  const floorAccounts = new Map(accounts);
  const poolBytes = encodeAccount(POOL, { tickSpacing: 60, tickCurrent: 10, mint0: WSOL_MINT, mint1: addr(51), hook: ids.floorHook, fee: DYNAMIC_FEE_FLAG });
  floorAccounts.set(pool, own(ids.carpenterAmm, poolBytes));
  floorAccounts.set((await findFloorPool(pool, ids)).address, own(ids.floorHook, encodeAccount(FLOOR_POOL, { ammPool: pool, poolBound: 1 })));
  const mint = new Uint8Array(82);
  mint.set([0, 0xca, 0x9a, 0x3b, 0, 0, 0, 0], 36); // supply 1e9 LE
  floorAccounts.set(addr(51), own(TOKEN_2022_PROGRAM_ID, mint));
  const fl = await readSwapQuoteAccounts(memorySource(floorAccounts), pool, { radius: 0 });
  assert.equal(fl.value.isFloor, true);
  assert.equal(fl.value.floorPool?.length, 8 + FLOOR_POOL.size);
  assert.equal(fl.value.supply1, 1_000_000_000n);
  assert.equal((await getMintExtensions(memorySource(floorAccounts), addr(51))).value?.supply, 1_000_000_000n);
  assert.equal((await getMintExtensions(memorySource(floorAccounts), addr(51))).value?.tokenProgram, TOKEN_2022_PROGRAM_ID);
  assert.equal((await getMintExtensions(memorySource(new Map([[addr(51), own(SPL_TOKEN_PROGRAM_ID, mint)]])), addr(51))).value?.tokenProgram, SPL_TOKEN_PROGRAM_ID);
});

test('findLaunch(mint) is what listLaunches rows point at', async () => {
  const f = await fixtureLaunch();
  assert.equal((await findLaunch(f.mint)).address, f.launch);
});
