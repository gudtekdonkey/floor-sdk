// `predict.graduationAccounts` == (the frozen fixed list, read out of `graduate.rs`,
// then tick arrays), the rent it sums, `max_graduation_lamports` = rent × 1.1, `ratchetAccounts`
// four positions, and `swapTickArrays` deduplicated union.

import assert from 'node:assert/strict';
// The open tick's admissible window (`floor_launch` `open_tick_in_bounds`), both edges exclusive.
const OPEN_TICK_MIN_EXCLUSIVE = -443_636 + 3 * 6_932 + 60;
const OPEN_TICK_MAX_EXCLUSIVE = 443_636 - 2 * 60;
import { test } from './node-test.ts';

import { AccountRole, address } from '@solana/kit';

import {
  BID_BAND_TICKS,
  FLOOR_TICK_SPACING,
  PLATFORM_BPS,
  SALE_TOKENS,
  SALT_STAIR_RESERVE,
  SALT_STAIR_STEP0,
  SALT_STAIR_TAIL,
  SALT_BID,
  SALT_SEED,
  TEAM_SALE_FEE_BPS_DEFAULT,
} from '../src/constants.ts';
import {
  DYNAMIC_FEE_FLAG,

  MIN_TICK,
  SPL_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  findAmmConfig,
  findApprovedHook,
  findAta,
  findBitmap,
  findEventAuthority,
  findMintExclusivity,
  findMintPools,
  findPool,
  findQuoteMint,
  findPoolIndexEntry,
  findPosition,
  findProgramData,
  findTickArray,
  findVault,
  swapTickArrays,
  TICK_ARRAY_LEN_V2,
  bitmapAccountLen,
  ceilToSpacing,
  floorToSpacing,
  tickArrayStartIndex,
} from '@floorsupply/carpenter-sdk';
import {
  DEFAULT_PROGRAM_IDS,
  findFloorPool,
  findGraduationRent,
  findHookAuthority,
  findHookConfig,
  findLaunch,
  findLaunchConfig,
  findStairState,
} from '../src/pda.ts';
import {
  GRADUATION_ACCOUNT_LENS,
  GRADUATION_ACCOUNT_NAMES,
  GRADUATION_BID_WINDOW_TICKS,
  graduationAccounts,
  graduationFloorBound,
  ladderRange,
  maxGraduationLamports,
  polRange,
  saleGrossFor,
  saleNetOf,
  saleTeamFee,
} from '../src/predict.ts';
import { openTick, stairEdge, tranchePriceX64 } from '../src/tickmath.ts';
import {
  addr,
  FIXTURE_STAIR_STEPS,
  fixtureLaunch,
  parseRustGraduationLens,
  parseRustGraduationList,
  parseRustIntConsts,
  repoFile,
  specRent,
  stripLineComments, SOURCE_ONLY } from './helpers.ts';

const W = AccountRole.WRITABLE;
const R = AccountRole.READONLY;

test('graduationAccounts: the fixed accounts, order and roles, as graduate.rs declares them', SOURCE_ONLY, async () => {
  const f = await fixtureLaunch();
  const g = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  const ids = DEFAULT_PROGRAM_IDS;
  const pool = f.state.ammPool;
  const rust = parseRustGraduationList();
  const hookAuthority = (await findHookAuthority(ids.floorHook)).address;
  const pos = (salt: bigint) => findPosition(pool, hookAuthority, salt, ids).then((p) => p.address);
  const expected: [string, string, AccountRole][] = [
    ['launch_config', (await findLaunchConfig(ids)).address, R],
    ['carpenter_amm', ids.carpenterAmm, R],
    ['amm_config', (await findAmmConfig(ids)).address, R],
    ['approved_hook', (await findApprovedHook(ids.floorHook, ids)).address, R],
    ['hook_programdata', (await findProgramData(ids.floorHook)).address, R],
    ['pool', pool, W],
    ['bitmap', (await findBitmap(pool, ids)).address, W],
    ['vault0', (await findVault(pool, WSOL_MINT, ids)).address, W],
    ['vault1', (await findVault(pool, f.mint, ids)).address, W],
    ['pool_index_entry', (await findPoolIndexEntry(pool, ids)).address, W],
    ['wsol_mint', WSOL_MINT, R],
    ['spl_token', SPL_TOKEN_PROGRAM_ID, R],
    ['mint_exclusivity0', (await findMintExclusivity(WSOL_MINT, ids)).address, R],
    ['mint_exclusivity1', (await findMintExclusivity(f.mint, ids)).address, R],
    ['mint_pools0', (await findMintPools(WSOL_MINT, ids)).address, R],
    ['mint_pools1', (await findMintPools(f.mint, ids)).address, W],
    ['floor_hook', ids.floorHook, R],
    ['hook_config', (await findHookConfig(ids)).address, R],
    ['floor_pool', f.state.floorPool, W],
    ['hook_authority', hookAuthority, R],
    ['position_seed', await pos(SALT_SEED), W],
    ['position_bid', await pos(SALT_BID), W],
    ['system_program', SYSTEM_PROGRAM_ID, R],
    ['amm_event_authority', (await findEventAuthority(ids.carpenterAmm)).address, R],
    ['hook_event_authority', (await findEventAuthority(ids.floorHook)).address, R],
    ['wsol_vault', f.state.wsolVault, W],
    ['graduation_rent', (await findGraduationRent(f.launch, ids)).address, W],
    // The AMM's quote ROSTER: `initialize_pool`'s two REQUIRED trailing slots, so
    // the tick arrays start at 31. The QUOTE's entry (wSOL's on this fixture: the implied quote,
    // never present) then the base mint's, which never exists.
    ['quote_mint0', (await findQuoteMint(WSOL_MINT, ids)).address, R],
    ['quote_mint1', (await findQuoteMint(f.mint, ids)).address, R],
    // Graduation inside the buy: the StairState and its three positions, writable,
    // on every launch; the arrays at 33..
    ['stair_state', (await findStairState(f.state.floorPool, ids)).address, W],
    ['position_stair_step0', await pos(SALT_STAIR_STEP0), W],
    ['position_stair_reserve', await pos(SALT_STAIR_RESERVE), W],
    ['position_stair_tail', await pos(SALT_STAIR_TAIL), W],
  ];
  // The Rust decides the list; `expected` only says which DERIVATION each name is.
  assert.deepEqual([...GRADUATION_ACCOUNT_NAMES], rust.names, 'GRADUATION_ACCOUNT_NAMES == graduate.rs want[]');
  assert.equal(GRADUATION_ACCOUNT_NAMES.length, rust.fixedCount, 'the fixed count is GRADUATION_FIXED_ACCOUNTS');
  assert.deepEqual(expected.map((e) => e[0]), rust.names, 'this test derives every account graduate.rs names');
  rust.names.forEach((name, i) => {
    const want = expected[i]!;
    assert.equal(g.names[i], name, `name ${i}`);
    assert.equal(g.accounts[i]?.address, want[1], `address of ${name}`);
    assert.equal(g.accounts[i]?.role, want[2], `role of ${name}`);
    // WRITABLE_FIXED is `graduate`'s own `ConstraintMut` list: a `w` it wants and the SDK sends
    // read-only fails on chain, and one it does not want is a needless lock.
    assert.equal(g.accounts[i]?.role === W, rust.writable.has(name), `writable of ${name} (graduate.rs WRITABLE_FIXED)`);
  });
  assert.equal(g.accounts.length, rust.fixedCount + g.tickArrayStarts.length);
  for (let i = rust.fixedCount; i < g.accounts.length; i++) {
    const st = g.tickArrayStarts[i - rust.fixedCount]!;
    assert.equal(g.names[i], `tick_array[${st}]`);
    assert.equal(g.accounts[i]?.role, W);
    assert.equal(g.accounts[i]?.address, (await findTickArray(pool, st, ids)).address);
  }
  assert.equal(g.pool, pool);
  assert.equal(g.openTick, f.state.openTick);
});

test('graduationAccounts: the tick arrays cover the POL tail, the staircase and the bid; 4–6 arrays', async () => {
  const f = await fixtureLaunch();
  const g = await graduationAccounts({ launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  const s = FLOOR_TICK_SPACING;
  const open = f.state.openTick;
  const [polLower, polUpper] = polRange(open, s);
  assert.equal(polLower, ceilToSpacing(MIN_TICK, s), 'the POL lower edge is the bottom of the usable grid');
  assert.equal(polLower, -443_580, 'graduate.rs pol_range: −443,580 at spacing 60 (its own unit test)');
  assert.equal(tickArrayStartIndex(polLower, s), -446_400, 'launch_graduation.rs wide_starts: the bottom array is −446,400');
  assert.equal(polUpper, floorToSpacing(open, s));
  // step 7's ladder range IS the POL's (`ladder_range`), so it adds no array.
  assert.deepEqual(ladderRange(open, s), [polLower, polUpper]);
  const bidFloor = ceilToSpacing(open + 1, s);
  const must = [
    polLower, // POL lower = ladder lower: the bottom of the grid, NOT open − 6,932
    polUpper, // POL upper = the open, floored
    bidFloor, // the lowest the bid can land
    stairEdge(polUpper, 1, s), // the staircase's first inner edge
    stairEdge(polUpper, FIXTURE_STAIR_STEPS, s), // and its last
  ].map((t) => tickArrayStartIndex(t, s));
  for (const st of must) assert.ok(g.tickArrayStarts.includes(st), `array ${st}`);
  assert.ok(g.tickArrayStarts.includes(-446_400), 'the bottom array is in the set');
  // ⛔ No 2×..8× band array rides: at the fixture's open, 89,231, such a band's arrays would be 68,400 and 79,200;
  // 79,200 is also the staircase's first inner edge's (one doubling from the open), so it rides for that.
  // The band's edges, 20,794 / 6,932 below the open:
  const retired = [open - 20_794, open - 6_932].map((t) => tickArrayStartIndex(floorToSpacing(t, s), s));
  assert.deepEqual(retired, [68_400, 79_200]);
  assert.ok(!g.tickArrayStarts.includes(68_400), 'no ladder array 68,400');
  assert.equal(tickArrayStartIndex(stairEdge(polUpper, 1, s), s), 79_200);
  assert.deepEqual(g.tickArrayStarts, [...g.tickArrayStarts].sort((a, b) => a - b), 'ascending');
  assert.equal(new Set(g.tickArrayStarts).size, g.tickArrayStarts.length, 'deduplicated');
  assert.ok(g.tickArrayStarts.length >= 4 && g.tickArrayStarts.length <= 6, `${g.tickArrayStarts.length} arrays`);
  // ONE extra array for the POL's tail, never the ~120 between the bottom and the staircase.
  const stairLow = Math.min(...[1, FIXTURE_STAIR_STEPS].map((i) => tickArrayStartIndex(stairEdge(polUpper, i, s), s)));
  assert.equal(g.tickArrayStarts.filter((st) => st < stairLow).length, 1, 'exactly one array below the staircase');
  // A hint pins the bid to the arrays of [lower, lower + W] — the BAND, not
  // [lower, lower + s] — and drops the window.
  const lower = ceilToSpacing(open + 240, s);
  const h = await graduationAccounts({ launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, bidLowerHint: lower });
  assert.ok(h.tickArrayStarts.includes(tickArrayStartIndex(lower, s)));
  assert.ok(h.tickArrayStarts.includes(tickArrayStartIndex(lower + s, s)));
  assert.ok(h.tickArrayStarts.includes(tickArrayStartIndex(lower + ceilToSpacing(BID_BAND_TICKS, s), s)));
  assert.ok(h.tickArrayStarts.length <= g.tickArrayStarts.length);
  // A hint whose BAND crosses an array edge names both arrays. With `lower` two spacings under the
  // edge, `lower + s` is still in the lower array and only the cheap edge `lower + W` reaches the
  // next — which the pre-band `[lower, lower + s]` prediction left out.
  const edge = tickArrayStartIndex(open, s) + 60 * s;
  const straddle = edge - 2 * s; // lower + s = edge − s (this array), lower + W = edge (the next)
  const hs = await graduationAccounts({ launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, bidLowerHint: straddle });
  assert.ok(hs.tickArrayStarts.includes(tickArrayStartIndex(straddle, s)), 'the dear edge’s array');
  assert.ok(hs.tickArrayStarts.includes(edge), 'the cheap edge’s array, one past `lower + s`’s');
  // A hint below the floor of the bid is raised to it.
  const low = await graduationAccounts({ launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, bidLowerHint: open - 10_000 });
  assert.ok(low.tickArrayStarts.includes(tickArrayStartIndex(ceilToSpacing(open + 1, s), s)));
});

test('graduationAccounts: a stock quote’s one-share reserve reaches array 140,400 — the devnet HDBQ… launch, exactly', async () => {
  const s = FLOOR_TICK_SPACING;
  const ids = DEFAULT_PROGRAM_IDS;
  const SPYX = address('CaF1bz1E5KwZfsnm18n9WQU3vu3VrW7pudixqc8BERAV');
  const mint = address('HDBQ6sWmx7NwCEdr2GheCoxM3zp3FTNdhSXU5McHzB1D');
  const quote = { mint: SPYX, tokenProgram: TOKEN_2022_PROGRAM_ID };
  const priceX64 = tranchePriceX64(5_000n, 500n * 100n, 8);
  assert.equal(priceX64, 18_446_744_073_709n, 'Launch.price_x64 on devnet');
  assert.equal(openTick(priceX64), 138_162, 'Launch.open_tick on devnet');
  // ⛔ That launch was created under the 55 / 15 / 30 split: its `Launch.sale_tokens` is 550e12, and its
  // graduation solves on what IT raised. `graduationFloorBound` / `graduationAccounts` read the launch's
  // own `saleTokens`; `SALE_TOKENS` (30 %) is the default for a launch that does not say.
  const saleTokens = 550_000_000_000_000n;
  assert.equal(SALE_TOKENS, 300_000_000_000_000n, 'the 30 / 10 / 60 split');
  const raised = (saleTokens * priceX64) >> 64n;
  const backing = raised - (raised * BigInt(PLATFORM_BPS)) / 10_000n;
  assert.equal(raised, 549_999_999n, 'the 5.5-share raise at sell-out (8 dp)');
  assert.equal(backing, 536_250_000n, 'less the 2.5 % cut: 5.3625 shares into the vault');
  const minBid = 100_000_000n;

  // The bound, and the same floor from first principles: the solve wants the smallest grid tick with
  // SALE / 1.0001^t ≤ backing − min_bid; publish adds the 120-tick band. 140,479.2 → 140,520 → 140,640.
  const bound = graduationFloorBound({ priceX64, minBid, saleTokens });
  const solvedFloat = Math.log(Number(saleTokens) / Number(backing - minBid)) / Math.log(1.0001);
  assert.ok(solvedFloat > 140_479 && solvedFloat < 140_480, `${solvedFloat}`);
  assert.equal(bound, ceilToSpacing(Math.ceil(solvedFloat), s) + ceilToSpacing(BID_BAND_TICKS, s));
  assert.equal(bound, 140_640, 'the bid band [140,520, 140,640]');
  assert.equal(tickArrayStartIndex(bound - 120, s), 140_400);

  const launchAddress = (await findLaunch(mint, ids)).address;
  const ammPool = (await findPool({ mint0: SPYX, mint1: mint, fee: DYNAMIC_FEE_FLAG, tickSpacing: s, hook: ids.floorHook }, ids)).address;
  const launch = {
    mint,
    priceX64,
    saleTokens,
    openTick: 138_162,
    ammPool,
    floorPool: (await findFloorPool(ammPool, ids)).address,
    wsolVault: (await findAta(launchAddress, SPYX, TOKEN_2022_PROGRAM_ID)).address,
  };
  // Every pool is a staircase,
  // whose inner edges' arrays ride instead (the fixture's Bluechip count, 12 steps).
  const steps = FIXTURE_STAIR_STEPS;
  const stairArrays = [1, steps].map((i) => tickArrayStartIndex(stairEdge(floorToSpacing(138_162, s), i, s), s));
  const g = await graduationAccounts({ launch, launchAddress, quote, floorPool: { minBid, stairSteps: steps }, rent: specRent });
  assert.deepEqual(g.tickArrayStarts, [...new Set([-446_400, 136_800, 140_400, ...stairArrays])].sort((a, b) => a - b));
  assert.ok(g.tickArrayStarts.includes(140_400), 'the array the devnet ratchet asked for');
  assert.equal(g.accounts.length, GRADUATION_ACCOUNT_NAMES.length + g.tickArrayStarts.length);

  await assert.rejects(graduationAccounts({ launch, launchAddress, quote, rent: specRent, stairSteps: steps }), /needs `floorPool`.*AccountNotEnoughKeys/);
  const hinted = await graduationAccounts({ launch, launchAddress, quote, rent: specRent, stairSteps: steps, bidLowerHint: 140_520 });
  assert.ok(hinted.tickArrayStarts.includes(140_400));
  for (const st of stairArrays) assert.ok(hinted.tickArrayStarts.includes(st), `hinted: the staircase array ${st}`);
});

test('graduationAccounts: a wSOL launch is unchanged with or without `floorPool`, and the bound is the measured floor', SOURCE_ONLY, async () => {
  const f = await fixtureLaunch();
  const s = FLOOR_TICK_SPACING;
  const minBid = parseRustIntConsts()['MIN_BID_LAMPORTS']!;
  assert.equal(minBid, 50_000_000n, 'floor_types MIN_BID_LAMPORTS, the LaunchConfig default');
  const without = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  const withPool = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, floorPool: { minBid, stairSteps: FIXTURE_STAIR_STEPS } });
  assert.deepEqual(withPool.tickArrayStarts, without.tickArrayStarts, 'the wSOL set does not grow');
  assert.deepEqual(withPool.accounts, without.accounts);
  assert.equal(withPool.rentLamports, without.rentLamports);

  // surfpool: the fixture's price, the default reserve — the floor it measured.
  assert.equal(f.state.openTick, 89_231);
  // Both were 55 %-split launches (the fixture's `saleTokens` is 550e12): the bound on their own sale.
  assert.equal(f.state.saleTokens, 550_000_000_000_000n);
  const surf = graduationFloorBound({ priceX64: f.state.priceX64, minBid, saleTokens: f.state.saleTokens });
  assert.equal(surf, 89_640, 'surfpool-e2e.md floor_tick 89,640');
  assert.ok(surf <= ceilToSpacing(f.state.openTick + 1, s) + GRADUATION_BID_WINDOW_TICKS + s, 'inside the fixed window');
  const seedPrice = 92_233_720_368_547n;
  assert.equal(openTick(seedPrice), 122_066);
  assert.equal(graduationFloorBound({ priceX64: seedPrice, minBid, saleTokens: f.state.saleTokens }), 122_640, 'bid [122,520, 122,640]');
  // The same price under the 30 / 10 / 60 split raises 30 % instead of 55 % of the cap: a smaller vault
  // against the same fixed reserve, so the floor can only land CHEAPER (a higher tick).
  assert.ok(graduationFloorBound({ priceX64: seedPrice, minBid }) >= 122_640);
});

test('graduationFloorBound: monotone in the reserve, the undefined floor at q = 0, and never under the no-reserve floor', () => {
  const s = FLOOR_TICK_SPACING;
  const p = tranchePriceX64(5_000n, 500n * 100n, 8);
  const raised = (SALE_TOKENS * p) >> 64n;
  const backing = raised - (raised * BigInt(PLATFORM_BPS)) / 10_000n;
  let prev = graduationFloorBound({ priceX64: p, minBid: 0n });
  // The reserve can only push the floor CHEAPER (a higher tick), never dearer.

  for (const minBid of [1n, 10_000_000n, 50_000_000n, 100_000_000n, 200_000_000n, 280_000_000n, backing - 1n]) {
    const b = graduationFloorBound({ priceX64: p, minBid });
    assert.ok(b >= prev, `min_bid ${minBid}: ${b} < ${prev}`);
    assert.equal(b % s, 0, 'on the spacing grid');
    prev = b;
  }
  // `backing == min_bid` (create_launch admits it): no solve, the floor stays ceil_to_spacing(MAX_TICK).
  assert.equal(graduationFloorBound({ priceX64: p, minBid: backing }), 443_520);
  assert.equal(graduationFloorBound({ priceX64: p, minBid: backing + 1n }), 443_520);
});

test('polRange is graduate.rs::pol_range, by its text: ceil_to_spacing(MIN_TICK) below, floor_to_spacing(open) above, the :470 defence', SOURCE_ONLY, () => {
  const raw = repoFile('programs/floor_launch/src/graduate.rs');
  const src = stripLineComments(raw);
  const at = src.indexOf('pub fn pol_range');
  assert.ok(at > 0, 'graduate.rs has pol_range');
  const body = src.slice(at, src.indexOf('\n}', at));
  assert.match(body, /let lower = floor_math::spacing::ceil_to_spacing\(MIN_TICK, s\)/, 'the lower edge is the bottom of the usable grid');
  assert.match(body, /let mut upper = floor_math::spacing::floor_to_spacing\(open_tick, s\)/, 'the upper edge is the open, floored');
  assert.match(body, /if upper <= lower \{\s*upper = lower \+ s;/, 'the :470 defence');
  // And step 6 CALLS it — the caller, not just the function (launch_graduation.rs's own guard). The
  // step markers are comments, so this slice is taken on the RAW file.
  const from = raw.indexOf('---- 6. seed_liquidity');
  const to = raw.indexOf('---- 7. seed_inventory');
  assert.ok(from > 0 && to > from, 'graduate.rs carries the step 6 and step 7 markers');
  const step6 = raw.slice(from, to);
  assert.ok(step6.includes('pol_range(open)'), 'step 6 seeds pol_range(open)');
  assert.ok(!step6.includes('TICKS_PER_DOUBLING') && !step6.includes('LADDER_START_TICKS'), 'step 6 is the tail, not a band');
  // The SDK computes the same edges over the whole admissible open-tick window.
  const s = FLOOR_TICK_SPACING;
  for (const open of [OPEN_TICK_MIN_EXCLUSIVE + 1, -100_000, -1, 0, 1, 59, 60, 122_066, OPEN_TICK_MAX_EXCLUSIVE - 1]) {
    const [lo, hi] = polRange(open, s);
    assert.equal(lo, -443_580, `lower at open ${open}`);
    assert.equal(hi, floorToSpacing(open, s), `upper at open ${open}`);
    assert.ok(hi > lo, `upper > lower at open ${open}`);
  }
});

test('graduationAccounts: rent sums the created accounts, honours `existing`, and × 1.1 rounds up', async () => {
  const f = await fixtureLaunch();
  const g = await graduationAccounts({ launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  const n = g.tickArrayStarts.length;
  const expected =
    specRent(GRADUATION_ACCOUNT_LENS.pool) +
    specRent(bitmapAccountLen(FLOOR_TICK_SPACING)) +
    specRent(GRADUATION_ACCOUNT_LENS.vaultSplToken) +
    specRent(GRADUATION_ACCOUNT_LENS.vaultToken2022Pausable) +
    specRent(GRADUATION_ACCOUNT_LENS.poolIndexEntry) +
    specRent(GRADUATION_ACCOUNT_LENS.mintPoolCount) +
    // SEED and BID
    2n * specRent(GRADUATION_ACCOUNT_LENS.position) +
    // the staircase: its StairState and step 0 / the reserve / the open tail
    specRent(GRADUATION_ACCOUNT_LENS.stairState) +
    3n * specRent(GRADUATION_ACCOUNT_LENS.position) +
    BigInt(n) * specRent(GRADUATION_ACCOUNT_LENS.tickArray);
  assert.equal(g.rentLamports, expected);
  assert.equal(g.maxGraduationLamports, (expected * 11n + 9n) / 10n);
  assert.equal(maxGraduationLamports(10n), 11n);
  assert.equal(maxGraduationLamports(1n), 2n, 'ceil, never floor: 1.1 rounds up');
  assert.equal(maxGraduationLamports(0n), 0n);
  // ≈ 0.1 SOL [est.] at rent, as says
  assert.ok(g.rentLamports > 90_000_000n && g.rentLamports < 160_000_000n, `${g.rentLamports}`);
  const gradRent = (await findGraduationRent(f.launch)).address;
  assert.equal(g.accounts[GRADUATION_ACCOUNT_NAMES.indexOf('graduation_rent')]?.address, gradRent);
  const withReserveExisting = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, existing: new Set([gradRent]) });
  assert.equal(withReserveExisting.rentLamports, expected, 'the reserve was never in the sum');
  const firstArray = g.accounts[GRADUATION_ACCOUNT_NAMES.length]!.address;
  const existing = new Set([g.pool, firstArray]);
  const e = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, existing });
  assert.equal(e.rentLamports, expected - specRent(GRADUATION_ACCOUNT_LENS.pool) - specRent(GRADUATION_ACCOUNT_LENS.tickArray));
  // an async rent source works too
  const a = await graduationAccounts({ launch: f.state, stairSteps: FIXTURE_STAIR_STEPS, rent: (len) => Promise.resolve(specRent(len)) });
  assert.equal(a.rentLamports, expected);
});

test('graduationAccounts throws BY NAME when the Launch disagrees with the derivation', async () => {
  const f = await fixtureLaunch();
  await assert.rejects(graduationAccounts({ launch: { ...f.state, ammPool: addr(9) }, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS }), /Launch\.amm_pool/);
  await assert.rejects(graduationAccounts({ launch: { ...f.state, floorPool: addr(9) }, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS }), /Launch\.floor_pool/);
  await assert.rejects(graduationAccounts({ launch: { ...f.state, openTick: f.state.openTick + 1 }, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS }), /Launch\.open_tick/);
});

test('ladderRange is graduate.rs::ladder_range, by its text: the POL’s own edges since the reserve wall', SOURCE_ONLY, () => {
  const src = stripLineComments(repoFile('programs/floor_launch/src/graduate.rs'));
  const at = src.indexOf('pub(crate) fn ladder_range');
  assert.ok(at > 0, 'graduate.rs has ladder_range');
  const body = src.slice(at, src.indexOf('\n}', at));
  assert.match(body, /ceil_to_spacing\(MIN_TICK, FLOOR_TICK_SPACING as i32\)/, 'the lower edge is the bottom of the usable grid');
  assert.match(body, /Ok\(\(lower, spacing_floor\(open\)\?\)\)/, 'the upper edge is the open, floored');
  assert.ok(!body.includes('LADDER_END_TICKS') && !body.includes('LADDER_START_TICKS'), 'not the retired 2×..8× band');
  for (const open of [OPEN_TICK_MIN_EXCLUSIVE + 1, -1, 0, 60, 80_040, 80_099, 122_066, OPEN_TICK_MAX_EXCLUSIVE - 1]) {
    assert.deepEqual(ladderRange(open), polRange(open), `open ${open}`);
  }
  assert.deepEqual(ladderRange(80_040), [-443_580, 80_040], 'graduate.rs’s own unit test');
});

/**
 * every pool is a staircase, so graduation carries the arrays of the
 * staircase's inner edges (`stairEdge(floor(open), 1)` and `stairEdge(floor(open), steps)`) for every step count, and
 * NO wall edge. Custom's count is derived from its cap (`customStairSteps`), so 1 and 16 — the clamp's ends — are walked too.
 */
test('graduationAccounts: the staircase’s inner edges ride for every step count 1..16, and floorPool.stairSteps is the same input', async () => {
  const f = await fixtureLaunch();
  const s = FLOOR_TICK_SPACING;
  const openEdge = floorToSpacing(f.state.openTick, s);
  for (let steps = 1; steps <= 16; steps++) {
    const g = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: steps });
    const viaPool = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, floorPool: { minBid: 50_000_000n, stairSteps: steps } });
    assert.deepEqual(viaPool.tickArrayStarts, g.tickArrayStarts, `steps ${steps}`);
    for (const i of [1, steps]) {
      assert.ok(g.tickArrayStarts.includes(tickArrayStartIndex(stairEdge(openEdge, i, s), s)), `steps ${steps}: the edge of step ${i}`);
    }
  }
});

test('graduationAccounts refuses BY NAME without the pool’s stairSteps, on every quote', async () => {
  const f = await fixtureLaunch();
  await assert.rejects(graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent }), /needs the pool's `stairSteps`.*AccountNotEnoughKeys/);
  await assert.rejects(graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, floorPool: { minBid: 1n } }), /stairSteps/);
  await assert.rejects(graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: 0 }), /stairSteps/);
});

test('swapTickArrays is the deduplicated union of the walk and the actions', async () => {
  const r = await swapTickArrays(addr(5), { arraysTouched: [0, 3600, 7200], actionArrays: [3600, -3600] });
  assert.deepEqual(r.starts, [0, 3600, 7200, -3600]);
  assert.equal(r.addresses.length, 4);
  assert.equal(r.addresses[1], (await findTickArray(addr(5), 3600)).address);
});

test('GRADUATION_ACCOUNT_LENS: every program-owned length is what graduation_rent.rs asserts it creates', SOURCE_ONLY, () => {
  const rust = parseRustGraduationLens();
  const pairs: [keyof typeof GRADUATION_ACCOUNT_LENS, string][] = [
    ['pool', 'POOL_LEN'],
    ['poolIndexEntry', 'POOL_INDEX_LEN'],
    ['mintPoolCount', 'MINT_POOLS_LEN'],
    ['position', 'POSITION_LEN'],
    ['tickArray', 'TICK_ARRAY_LEN'],
  ];
  for (const [sdk, rs] of pairs) {
    const want = rust.get(rs);
    assert.notEqual(want, undefined, `graduation_rent.rs asserts no ${rs}`);
    assert.equal(GRADUATION_ACCOUNT_LENS[sdk], want, `${sdk}: SDK ${GRADUATION_ACCOUNT_LENS[sdk]} vs program ${rs} = ${want}`);
  }
  // The two token accounts are SPL / Token-2022 shapes the program does not assert; they stay as written.
  assert.equal(GRADUATION_ACCOUNT_LENS.vaultSplToken, 165);
  assert.equal(GRADUATION_ACCOUNT_LENS.vaultToken2022Pausable, 170);
});

// the SAME vectors as floor-math sale.rs `the_team_gross_up_and_its_inverse_at_25_bps`.
test('sale fee helpers are floor-math team_net_of / team_fee_of / team_gross_for, vector for vector', () => {
  assert.deepEqual([saleNetOf(1_002_500_000n, 25), saleNetOf(401n, 25), saleNetOf(400n, 25), saleNetOf(1n, 25)], [1_000_000_000n, 400n, 399n, 0n]);
  assert.deepEqual([saleTeamFee(399n, 25), saleTeamFee(400n, 25)], [0n, 1n]);
  assert.deepEqual([saleGrossFor(1_000_000_000n, 25), saleGrossFor(399n, 25), saleGrossFor(1n, 25)], [1_002_500_000n, 400n, 2n]);
  for (const c of [0n, 1n, 399n, 400n, 1_000_000_007n]) {
    assert.equal(saleNetOf(saleGrossFor(c, 25), 25), c);
    assert.equal(saleNetOf(c, 0), c);
  }
});

// the SAME vectors as floor-math sale.rs
// `the_team_gross_up_and_its_inverse_at_the_100_bps_sale_default` — the rate every launch on an unset config
// charges — and a quote follows EACH launch's own rate: the default 100, a lowered 40, and 0 (no fee at all).
// Mutations: a constant read instead of the launch's rate; a zero rate that still floors a fee.
test('sale fee helpers at the 100 bps default, and each launch is quoted at its own rate (100 / 40 / 0)', () => {
  const D = TEAM_SALE_FEE_BPS_DEFAULT;
  assert.equal(D, 100);
  assert.deepEqual([saleNetOf(1_010_000_000n, D), saleNetOf(101n, D), saleNetOf(100n, D), saleNetOf(1n, D)], [1_000_000_000n, 100n, 99n, 0n]);
  assert.deepEqual([saleTeamFee(99n, D), saleTeamFee(100n, D), saleTeamFee(1_000_000_000n, D)], [0n, 1n, 10_000_000n]);
  assert.deepEqual([saleGrossFor(1_000_000_000n, D), saleGrossFor(99n, D), saleGrossFor(1n, D)], [1_010_000_000n, 100n, 2n]);
  for (const c of [0n, 1n, 99n, 100n, 101n, 1_000_000_007n]) {
    const g = saleGrossFor(c, D);
    assert.equal(saleNetOf(g, D), c, 'net_of(gross_for(c)) == c');
    assert.equal(g - c - saleTeamFee(c, D), c % 100n === 0n ? 0n : 1n, 'the slack is 0 iff c % 100 == 0');
  }
  // the same 1.01 SOL gross against three launches: the net walked and the fee on its cost differ by rate
  const gross = 1_010_000_000n;
  const fees = [100, 40, 0].map((bps) => {
    const net = saleNetOf(gross, bps);
    return [net, saleTeamFee(net, bps)];
  });
  assert.deepEqual(fees, [[1_000_000_000n, 10_000_000n], [1_005_976_095n, 4_023_904n], [1_010_000_000n, 0n]]);
  // a whole sell-back of 1 SOL: 1 %, 0.4 % and nothing kept for the team
  assert.deepEqual([100, 40, 0].map((bps) => saleTeamFee(1_000_000_000n, bps)), [10_000_000n, 4_000_000n, 0n]);
});

/**
 * Every tick array the final buy
 * CREATES is V2 from the upgrade on — the body, then its rent owner — so a missing array is priced at
 * 3,920 bytes, never the 3,888-byte body. (The program side is `graduation_rent.rs` `TICK_ARRAY_LEN`;
 * the case above holds the two equal.)
 */
test('graduation rent prices a MISSING tick array at 3,920 (V2): 20,563,840 at the live rate', async () => {
  assert.equal(GRADUATION_ACCOUNT_LENS.tickArray, 3_920);
  assert.equal(GRADUATION_ACCOUNT_LENS.tickArray, TICK_ARRAY_LEN_V2);
  const f = await fixtureLaunch();
  const g = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  assert.ok(g.tickArrayStarts.length >= 1);
  const firstArray = g.accounts[GRADUATION_ACCOUNT_NAMES.length]!.address;
  const e = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, existing: new Set([firstArray]) });
  assert.equal(g.rentLamports - e.rentLamports, specRent(3_920), 'one missing array costs the V2 rent');
  assert.equal(specRent(3_920), 20_563_840n);
  assert.equal(specRent(3_920) - specRent(3_888), 162_560n, 'V2 over V1: 32 bytes of rent');
  // every array, not just the first: n missing arrays cost n × the V2 rent
  const allArrays = new Set(g.accounts.slice(GRADUATION_ACCOUNT_NAMES.length).map((m) => m.address));
  const none = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, existing: allArrays });
  assert.equal(g.rentLamports - none.rentLamports, BigInt(g.tickArrayStarts.length) * 20_563_840n);
});
