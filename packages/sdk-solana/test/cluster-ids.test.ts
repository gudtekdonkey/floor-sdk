// A deployment row whose program ids are NOT the shipped IDLs' (another cluster): `npm run generate`'s rules accept it
// with its derivable accounts checked against ITS ids, and every public builder, planner, PDA and read given
// `programIdsFromDeployment(row)` names that row's programs only: no devnet id reaches any instruction.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  DEPLOYMENTS as CARPENTER_DEPLOYMENTS,
  addressToBytes,
  bytesToAddress,
  buildModifyLiquidity,
  buildInitializePool,
  DYNAMIC_FEE_FLAG,
  findAmmConfig,
  findEventAuthority,
  findPool,
  findProtocolTreasury,
  SPL_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  type AccountSource,
  type ClusterDeployment as CarpenterRow,
  CARPENTER_AMM_IDL,
} from '@floorlaunchpad/carpenter-sdk';
import { getProgramDerivedAddress, type Address, type Instruction } from '@solana/kit';

import { validateDeployments } from '../scripts/validate-deployments.mjs';
import { createSolanaAdapter, DEVNET_KEY, MAINNET_KEY, type SolanaAdapterRpc } from '../src/adapter.ts';
import { programIdsFromDeployment, readSwapQuoteAccounts } from '../src/adapters.ts';
import { FLOOR_TICK_SPACING } from '../src/constants.ts';
import { DEPLOYMENTS, deploymentFor, joinDeployment, type ClusterDeployment, type FloorClusterDeployment } from '../src/deployments.ts';
import { FLOOR_HOLDERS_IDL, FLOOR_HOOK_IDL, FLOOR_LAUNCH_IDL, FLOOR_VESTING_IDL } from '../src/generated/idls.ts';
import { findExtraAccountMetas, findHolderBook, findHoldersAuthority, touchIx, transferHookAccounts } from '../src/holders.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findFloorPlatformPool,
  findFloorPool,
  findHookAuthority,
  findHookConfig,
  findLaunch,
  findLaunchAuthority,
  findLaunchConfig,
  findLockAuthority,
  findMintAuthority,
  findPlatformTreasury,
  findTeamTreasury,
  findTraderRefVaultAuthority,
  type ProgramIds,
} from '../src/pda.ts';
import { graduationAccounts } from '../src/predict.ts';
import * as stateView from '../src/stateView.ts';
import {
  buildBuy,
  buildCloseBuyerPosition,
  buildPrepareGraduation,
  buildSellBack,
  buildSellBackWsol,
  buildSwap,
  floorInitializeHookSlice,
  floorSwapHookSlice,
  planBuy,
} from '../src/tx/builders.ts';
import { buildLinkTrader, planLinkThenTrade, saleRefPrefix } from '../src/tx/traderRef.ts';
import { addr, FIXTURE_BUYER, FIXTURE_STAIR_STEPS, fixtureLaunch, SDK_ROOT, specRent } from './helpers.ts';
import { test } from './node-test.ts';

const utf8 = (s: string) => new TextEncoder().encode(s);
const pda = async (program: Address, seeds: Uint8Array[]) => (await getProgramDerivedAddress({ programAddress: program, seeds }))[0];

function randomAddress(): Address {
  const b = new Uint8Array(32);
  crypto.getRandomValues(b);
  return bytesToAddress(b);
}

/** Testnet's CAIP-2 id: a cluster with no shipped row, for synthetic rows. */
const TESTNET_KEY = 'solana:4uhcVJyU9pJkvQyS88uRDiswHXSCkY3z';

/** A joined row (Carpenter's half and FLOOR's) for a cluster this package does not ship, with fresh random program ids and its derived accounts. */
async function syntheticRows(): Promise<{ carpenter: CarpenterRow; floor: FloorClusterDeployment; joined: ClusterDeployment }> {
  const [carpenterAmm, floorHook, floorLaunch, floorHolders, floorVesting] = [0, 1, 2, 3, 4].map(randomAddress) as [Address, Address, Address, Address, Address];
  const carpenter: CarpenterRow = {
    name: 'testnet',
    chainKey: TESTNET_KEY,
    programs: { carpenterAmm },
    accounts: { ammConfig: await pda(carpenterAmm, [utf8('amm_config')]), protocolTreasury: await pda(carpenterAmm, [utf8('protocol_treasury')]) },
    programDataHash: { carpenterAmm: null },
  };
  const floor: FloorClusterDeployment = {
    name: 'testnet',
    chainKey: TESTNET_KEY,
    programs: { floorHook, floorLaunch, floorHolders, floorVesting },
    accounts: {
      hookConfig: await pda(floorHook, [utf8('hook_config')]),
      launchConfig: await pda(floorLaunch, [utf8('launch_config')]),
      teamTreasury: await pda(floorHook, [utf8('team_treasury')]),
    },
    alt: randomAddress(),
    programDataHash: { floorHook: null, floorLaunch: null, floorHolders: null, floorVesting: null },
  };
  return { carpenter, floor, joined: joinDeployment(carpenter, floor) };
}

/** Devnet's program ids, its rows' recorded accounts, and every id-only PDA a builder could fall back to. */
async function devnetKeys(): Promise<Set<string>> {
  const d = DEFAULT_PROGRAM_IDS;
  const pdas = await Promise.all([
    findAmmConfig(d), findProtocolTreasury(d), findHookConfig(d), findLaunchConfig(d), findPlatformTreasury(d), findTeamTreasury(d),
    findFloorPlatformPool(d), findMintAuthority(d), findLockAuthority(d), findLaunchAuthority(d), findTraderRefVaultAuthority(d),
    findHoldersAuthority(d), findHookAuthority(d.floorHook),
    ...[d.carpenterAmm, d.floorHook, d.floorLaunch, d.floorHolders].map((p) => findEventAuthority(p)),
  ]);
  const rows = [...Object.values(DEPLOYMENTS.clusters), ...Object.values(CARPENTER_DEPLOYMENTS.clusters)];
  return new Set<string>([
    ...pdas.map((p) => p.address),
    ...rows.flatMap((r) => [...Object.values(r.programs), ...Object.values(r.accounts as Record<string, string>)]),
    ...Object.values(DEPLOYMENTS.clusters).flatMap((r) => (r.alt ? [r.alt] : [])),
  ]);
}

function addressesOf(ixs: readonly Instruction[]): string[] {
  return ixs.flatMap((ix) => [ix.programAddress, ...(ix.accounts ?? []).map((m) => m.address)]);
}

function assertNoDevnet(what: string, addrs: readonly string[], devnet: ReadonlySet<string>): void {
  for (const a of addrs) assert.ok(!devnet.has(a), `${what}: devnet key ${a} leaked`);
}

/** No devnet key's 32 bytes anywhere in `bytes` (instruction data included). */
function assertBytesClean(what: string, bytes: Uint8Array, devnet: ReadonlySet<string>): void {
  for (const k of devnet) {
    const needle = addressToBytes(k as Address);
    outer: for (let i = 0; i + 32 <= bytes.length; i++) {
      for (let j = 0; j < 32; j++) if (bytes[i + j] !== needle[j]) continue outer;
      assert.fail(`${what}: devnet key ${k} in the bytes`);
    }
  }
}

const IDL_ADDRESS = {
  floorHook: FLOOR_HOOK_IDL.address,
  floorLaunch: FLOOR_LAUNCH_IDL.address,
  floorHolders: FLOOR_HOLDERS_IDL.address,
  floorVesting: FLOOR_VESTING_IDL.address,
};

test('generate accepts a row with its own program ids and checks its derived accounts against THEM', async () => {
  const { floor } = await syntheticRows();
  assert.deepEqual(await validateDeployments({ version: 1, clusters: { ...DEPLOYMENTS.clusters, [TESTNET_KEY]: floor } }, IDL_ADDRESS), []);
  assert.deepEqual(await validateDeployments(DEPLOYMENTS, IDL_ADDRESS), [], 'the shipped file passes');
  const devnetRow = Object.values(DEPLOYMENTS.clusters).find((r) => r.name === 'devnet')!;
  for (const k of ['hookConfig', 'launchConfig', 'teamTreasury'] as const) {
    const wrong = { ...floor, accounts: { ...floor.accounts, [k]: devnetRow.accounts[k] } };
    const p = await validateDeployments({ version: 1, clusters: { [TESTNET_KEY]: wrong } }, IDL_ADDRESS);
    assert.equal(p.length, 1, k);
    assert.match(p[0]!, new RegExp(`${k} .* derivation`));
  }
  const bad = { ...floor, programs: { ...floor.programs, floorLaunch: 'xyz' } };
  assert.ok((await validateDeployments({ version: 1, clusters: { [TESTNET_KEY]: bad } }, IDL_ADDRESS)).some((m) => /floorLaunch is not a base58 32-byte/.test(m)));
  // The mainnet and local rows stay pinned to the IDLs' addresses; devnet names its own programs.
  const drifted = { ...floor, name: 'mainnet' as const, chainKey: MAINNET_KEY };
  assert.ok((await validateDeployments({ version: 1, clusters: { [MAINNET_KEY]: drifted } }, IDL_ADDRESS)).some((m) => /must match the shipped IDL/.test(m)));
  assert.notEqual(devnetRow.programs.floorHook, IDL_ADDRESS.floorHook);
  assert.deepEqual(await validateDeployments({ version: 1, clusters: { [DEVNET_KEY]: devnetRow } }, IDL_ADDRESS), []);
  // A recorded quote policy is ["quote", mint] under that row's floor_hook.
  const mint = randomAddress();
  const withQuote = {
    ...floor,
    quotes: { [mint]: { state: 'listed', decimals: 6, tokenProgram: SPL_TOKEN_PROGRAM_ID, terms: { minBidFloor: '1', minBidCeil: '1', maxBountyCeil: '1', minBid: '1', maxBounty: '1' }, hook: { policy: await pda(floor.programs.floorHook as Address, [utf8('quote'), addressToBytes(mint)]) } } },
  };
  assert.deepEqual(await validateDeployments({ version: 1, clusters: { [TESTNET_KEY]: withQuote } }, IDL_ADDRESS), []);
  withQuote.quotes[mint]!.hook.policy = randomAddress();
  assert.ok((await validateDeployments({ version: 1, clusters: { [TESTNET_KEY]: withQuote } }, IDL_ADDRESS)).some((m) => /hook\.policy/.test(m)));
});

test('the mainnet row is shipped; another key throws by name until a row exists, then is used', async () => {
  const mainnet = deploymentFor(MAINNET_KEY);
  assert.deepEqual(mainnet.programs, { carpenterAmm: CARPENTER_AMM_IDL.address, ...IDL_ADDRESS });
  assert.deepEqual(programIdsFromDeployment(mainnet), DEFAULT_PROGRAM_IDS, 'the compiled default is the mainnet row');
  assert.throws(() => deploymentFor(TESTNET_KEY), new RegExp(TESTNET_KEY));
  const rpc = {} as SolanaAdapterRpc;
  const quoter = {} as never;
  assert.doesNotThrow(() => createSolanaAdapter({ clusters: { mainnet: { rpc } }, quoter }));
  const { carpenter, floor, joined } = await syntheticRows();
  const files = [
    { version: 1 as const, clusters: { ...DEPLOYMENTS.clusters, [TESTNET_KEY]: floor } },
    { version: 1 as const, clusters: { ...CARPENTER_DEPLOYMENTS.clusters, [TESTNET_KEY]: carpenter } },
  ] as const;
  assert.deepEqual(deploymentFor(TESTNET_KEY, files[0], files[1]), joined);
  // A row that does not name floor_holders is refused by name, never filled with a compiled-in id.
  const { floorHolders: _h, ...rest } = joined.programs;
  assert.throws(() => programIdsFromDeployment({ ...joined, programs: rest } as unknown as ClusterDeployment), /names no programs\.floorHolders/);
  // With the row supplied, the adapter builds for it.
  assert.doesNotThrow(() => createSolanaAdapter({ clusters: { mainnet: { rpc, row: joined } }, quoter }));
});

test('every builder, planner and PDA given another row\'s ids names no devnet key', async () => {
  const { joined } = await syntheticRows();
  const ids: ProgramIds = programIdsFromDeployment(joined);
  const devnet = await devnetKeys();
  const f = await fixtureLaunch(ids);
  const L = f.state;
  const hookedPoolState = {
    mint0: WSOL_MINT, mint1: f.mint, vault0: addr(170), vault1: addr(171), tokenProgram0: SPL_TOKEN_PROGRAM_ID, tokenProgram1: TOKEN_2022_PROGRAM_ID,
    hook: ids.floorHook, decimals0: 9, decimals1: 6, tickSpacing: FLOOR_TICK_SPACING,
  };
  const pool = L.ammPool;
  const trader = FIXTURE_BUYER;
  const attestation = { tier1: addr(160), tier2: null, tier3: null, deadline: 1n, signature: new Uint8Array(64).fill(7) };
  const saleRef = await saleRefPrefix({ trader, launchMint: f.mint, ids });
  const launchAlt = { address: addr(78), addresses: [L.ammPool], authority: null, deactivationSlot: 0xffff_ffff_ffff_ffffn };
  const ata = { cpiGuardLocked: true, delegatedAmount: 9n };
  const built = {
    floorBuy: await buildSwap({
      trader, pool, poolState: hookedPoolState, zeroForOne: true, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0], ensureTickArrayStarts: [0],
      transferHook: { floorPool: L.floorPool, openBuyerRecord: true }, actionPositions: [addr(9)], ids,
    }),
    floorSell: await buildSwap({
      trader, pool, poolState: hookedPoolState, zeroForOne: false, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0], sourceHasCpiGuard: true,
      transferHook: { floorPool: L.floorPool }, ids,
    }),
    saleBuy: await buildBuy({ buyer: trader, launch: L, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, touch: {}, traderRef: true, ids }),
    prepare: await buildPrepareGraduation({ payer: trader, launch: L, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, hooked: true, ids }),
    sellBack: await buildSellBack({ buyer: trader, launch: L, tokens: 5n, buyerAta: ata, holder: { floorPool: L.floorPool }, traderRef: true, ids }),
    sellBackWsol: await buildSellBackWsol({ buyer: trader, launch: L, tokens: 5n, buyerAta: ata, holder: { floorPool: L.floorPool }, ids }),
    closeBuyerPosition: await buildCloseBuyerPosition({ buyer: trader, mint: f.mint, ids }),
    link: await buildLinkTrader({ trader, attestor: addr(151), cluster: 1, attestation, ids }),
    floorAddLiquidity: await buildModifyLiquidity({
      owner: trader, pool, poolState: hookedPoolState, salt: 0n, tickLower: 0, tickUpper: 120, liquidityDelta: 1_000n, amount0Max: 10n, amount1Max: 10n,
      hookSlice: [], transferHookAccounts: await transferHookAccounts({ mint: f.mint, floorPool: L.floorPool, source: addr(5), destination: addr(6), ids }), ids,
    }),
    floorInitPool: await buildInitializePool({
      payer: trader, mintA: f.mint, mintB: WSOL_MINT, tokenProgramA: TOKEN_2022_PROGRAM_ID, tokenProgramB: SPL_TOKEN_PROGRAM_ID, fee: DYNAMIC_FEE_FLAG,
      tickSpacing: FLOOR_TICK_SPACING, hook: ids.floorHook, sqrtPriceX64: 1n << 64n, hookSlice: await floorInitializeHookSlice(pool, ids), ids,
    }),
  };
  for (const [name, b] of Object.entries(built)) {
    assertNoDevnet(name, [...addressesOf(b.instructions), ...b.accounts], devnet);
    for (const ix of b.instructions) if (ix.data) assertBytesClean(`${name} data`, new Uint8Array(ix.data), devnet);
  }
  assert.equal(built.saleBuy.main.programAddress, ids.floorLaunch);
  assert.equal(built.floorBuy.main.programAddress, ids.carpenterAmm);
  assert.equal(built.floorInitPool.main.accounts.find((m) => m.address === ids.floorHook)?.address, ids.floorHook);

  const plan = await planBuy({ buyer: trader, launch: L, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, launchAlt, ids });
  for (const s of plan.steps) {
    assertNoDevnet(`planBuy/${s.step}`, [...addressesOf(s.tx.instructions), ...s.tx.accounts], devnet);
  }
  const linked = planLinkThenTrade({ linked: false, link: built.link, trade: plan.steps.map((s) => ({ step: s.step, tx: s.tx })), trader });
  for (const s of linked) assertNoDevnet(`planLinkThenTrade/${s.step}`, addressesOf(s.tx.instructions), devnet);

  const grad = await graduationAccounts({ launch: L, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, ids });
  assertNoDevnet('graduationAccounts', grad.accounts.map((a) => a.address), devnet);
  const slices = [
    ...(await floorSwapHookSlice(pool, trader, WSOL_MINT, ids)),
    ...(await floorInitializeHookSlice(pool, ids)),
    ...(await transferHookAccounts({ mint: f.mint, floorPool: L.floorPool, source: addr(5), destination: addr(6), ids })),
  ];
  assertNoDevnet('hook slices', slices.map((m) => m.address), devnet);
  const touch = await touchIx({ payer: trader, owner: trader, mint: f.mint, floorPool: L.floorPool, ids });
  assertNoDevnet('touch', addressesOf([touch]), devnet);
  assertNoDevnet('saleRefPrefix', Object.values(saleRef), devnet);
  const pdas = await Promise.all([
    findHookConfig(ids), findLaunchConfig(ids), findPlatformTreasury(ids), findTeamTreasury(ids), findFloorPlatformPool(ids), findMintAuthority(ids),
    findLockAuthority(ids), findLaunchAuthority(ids), findTraderRefVaultAuthority(ids), findHoldersAuthority(ids), findHookAuthority(ids.floorHook),
    findHolderBook(f.mint, ids), findExtraAccountMetas(f.mint, ids), findFloorPool(pool, ids), findLaunch(f.mint, ids),
    findPool({ mint0: WSOL_MINT, mint1: f.mint, fee: DYNAMIC_FEE_FLAG, tickSpacing: FLOOR_TICK_SPACING, hook: ids.floorHook }, ids),
  ]);
  assertNoDevnet('PDAs', pdas.map((p) => p.address), devnet);
  assert.equal((await findHookConfig(ids)).address, joined.accounts.hookConfig);
  assert.equal((await findLaunchConfig(ids)).address, joined.accounts.launchConfig);
});

/** An empty source that records every address and program it is asked for. */
function recordingSource(): AccountSource & { asked: string[] } {
  const asked: string[] = [];
  return {
    asked,
    async getAccount(a) {
      asked.push(a);
      return { slot: 1n, account: null };
    },
    async getMultipleAccounts(as) {
      asked.push(...as);
      return { slot: 1n, accounts: as.map(() => null) };
    },
    async getProgramAccounts(program) {
      asked.push(program);
      return { slot: 1n, accounts: [] };
    },
  };
}

test('every read given another row\'s ids asks for that row\'s accounts only', async () => {
  const { joined } = await syntheticRows();
  const ids = programIdsFromDeployment(joined);
  const devnet = await devnetKeys();
  const src = recordingSource();
  const v = stateView;
  const mint = addr(77);
  await v.getHookConfig(src, ids);
  await v.getPlatformTreasury(src, ids);
  await v.getTeamTreasury(src, ids);
  await v.getQuotePolicy(src, WSOL_MINT, ids);
  await v.getFloorPool(src, addr(70), ids);
  await v.getStairState(src, addr(71), ids);
  await v.getLaunchConfig(src, ids);
  await v.getLaunch(src, mint, ids);
  await v.getBuyerPosition(src, addr(72), FIXTURE_BUYER, ids);
  await v.listLaunches(src, MAINNET_KEY, { mode: 'scan' }, ids);
  await assert.rejects(readSwapQuoteAccounts(src, addr(73), { ids }));
  assert.ok(src.asked.includes(ids.floorLaunch));
  assertNoDevnet('reads', src.asked, devnet);
});

test('the generated clients name a shipped program id only as their own default program address', () => {
  const ids = new Set<string>([
    ...Object.values(DEPLOYMENTS.clusters).flatMap((r) => Object.values(r.programs)),
    ...Object.values(CARPENTER_DEPLOYMENTS.clusters).flatMap((r) => Object.values(r.programs)),
  ]);
  const root = join(SDK_ROOT, 'src/generated/client');
  const walk = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
  let files = 0;
  for (const file of walk(root)) {
    files++;
    readFileSync(file, 'utf8').split('\n').forEach((line, i) => {
      for (const id of ids) {
        if (!line.includes(id)) continue;
        const allowed = /PROGRAM_ADDRESS = '/.test(line) || /programAddress = '/.test(line) || /TProgram extends string = '/.test(line);
        assert.ok(allowed, `${file}:${i + 1} names ${id} outside a default program address`);
      }
    });
  }
  assert.ok(files > 50);
});
