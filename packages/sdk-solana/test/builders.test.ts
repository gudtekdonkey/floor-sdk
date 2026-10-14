// Transaction builders: every one prepends SetComputeUnitLimit from CU_LIMITS and refuses > 64
// accounts; `buildBuy` always carries the accounts, the final-buy limit and
// max_graduation_lamports; `buildSellBack` and `buildSwap` follow /  to the letter
// (ApproveChecked immediately before, Revoke only when a residual can remain); the v0 + ALT
// message compiles and fits.

import assert from 'node:assert/strict';
import { test } from './node-test.ts';

import { AccountRole, createAddressWithSeed, getProgramDerivedAddress,  type Address, type Blockhash, type Instruction } from '@solana/kit';

import {
  BorshReader,
  addressToBytes,
  U64_MAX,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  COMPUTE_BUDGET_PROGRAM_ID,
  DYNAMIC_FEE_FLAG,
  MAX_ACCOUNT_LOCKS,
  MIN_SQRT_PRICE_X64,
  SPL_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_ACCOUNT_RENT_LAMPORTS,
  WSOL_MINT,
  findAta,
  findBitmap,
  findEventAuthority,
  findMintPools,
  findTickArray,
  NO_HOOK,
  buildClosePosition,
  buildCloseTickArrays,
  buildInitializeTickArray,
  MAX_CLOSE_TICK_ARRAYS_ONE_POOL,
  buildV0Message,
  messageSize,
  buildInitializePool,
  findPool,
  sortMints,
} from '@floorlaunchpad/carpenter-sdk';
import { CU_LIMITS } from '../src/constants.ts';
import { DEFAULT_PROGRAM_IDS, findFloorPool, findHookConfig, findTraderBucket } from '../src/pda.ts';
// slot lists on the create wire and the pool's PayeeSlots tail.
import {
  buildBuy,
  buildPrepareGraduation,
  graduationPrepared,
  planBuy,
  isBenignLinkFailure,
  PREPARED_SLOTS,
  buildSellBack,
  buildSellBackWsol,
  buildSwap,
  TooManyAccountsError,
  uniqueAccounts,
  floorInitializeHookSlice,
} from '../src/tx/builders.ts';
import type { BuiltTransaction } from '../src/tx/common.ts';
import { DISC } from '../src/tx/instructions.ts';
import { addr, FIXTURE_BUYER, FIXTURE_STAIR_STEPS, fixtureLaunch, specRent } from './helpers.ts';

const isCuLimit = (ix: Instruction, units: number) => ix.programAddress === COMPUTE_BUDGET_PROGRAM_ID && ix.data?.[0] === 2 && new DataView(ix.data.buffer, ix.data.byteOffset).getUint32(1, true) === units;
const isFloor = (ix: Instruction, disc: Uint8Array) => ix.data !== undefined && ix.data.length >= 8 && disc.every((b, i) => ix.data![i] === b);
const dataOf = (ix: Instruction) => new BorshReader(ix.data!.subarray(8));

function plainPool(over: Partial<Parameters<typeof buildSwap>[0]['poolState']> = {}) {
  return {
    mint0: addr(20),
    mint1: addr(21),
    vault0: addr(22),
    vault1: addr(23),
    tokenProgram0: SPL_TOKEN_PROGRAM_ID,
    tokenProgram1: TOKEN_2022_PROGRAM_ID,
    hook: NO_HOOK,
    decimals0: 9,
    decimals1: 6,
    tickSpacing: 60,
    ...over,
  };
}

function checkCommon(b: BuiltTransaction, cu: keyof typeof CU_LIMITS, disc: Uint8Array): void {
  assert.equal(b.cuLimit, CU_LIMITS[cu]);
  assert.ok(isCuLimit(b.instructions[0]!, CU_LIMITS[cu]), 'SetComputeUnitLimit first');
  assert.ok(b.instructions.includes(b.main), 'main is in the body');
  assert.ok(isFloor(b.main, disc), 'main carries its discriminator');
  assert.equal(b.addressCount, b.accounts.length);
  assert.ok(b.addressCount <= MAX_ACCOUNT_LOCKS);
  assert.deepEqual(b.accounts, uniqueAccounts(b.instructions));
}

/**
 * A FLOOR pool's `initialize_pool` hook slice, as graduation's CPI passes it. `@floorlaunchpad/carpenter-sdk`'s `buildInitializePool`
 * is hook-agnostic (a hooked pool takes its slice from the caller), so these cases hand it the FLOOR slice.
 */
async function floorInitSlice(base: Address) {
  const [mint0, mint1] = sortMints(base, WSOL_MINT);
  const pool = (await findPool({ mint0, mint1, fee: DYNAMIC_FEE_FLAG, tickSpacing: 60, hook: DEFAULT_PROGRAM_IDS.floorHook })).address;
  return floorInitializeHookSlice(pool);
}

test('buildBuy: every sale buy carries the graduation accounts, the final-buy limit and max_graduation_lamports', async () => {
  const f = await fixtureLaunch();
  const b = await buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000_000_000n, minTokens: 7n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  checkCommon(b, 'buyFinal', DISC.launch.buy);
  assert.equal(b.cuLimit, 1_400_000);
  assert.equal(b.instructions.length, 2);
  const named = b.main.accountNames.length;
  assert.equal(named, 11 + 2, 'buy has 11 named accounts + event_cpi');
  assert.deepEqual(b.main.accounts.slice(named), b.graduation.accounts, 'remaining == the graduation list');
  assert.equal(b.main.accounts[0]?.address, FIXTURE_BUYER);
  assert.equal(b.main.accounts[0]?.role, AccountRole.WRITABLE_SIGNER);
  assert.equal(b.main.accounts[1]?.address, f.launch);
  assert.equal(b.main.accounts[5]?.address, (await findAta(FIXTURE_BUYER, f.mint, TOKEN_2022_PROGRAM_ID)).address, 'buyer_ata is the signer\'s Token-2022 ATA');
  assert.equal(b.main.accounts[8]?.address, ASSOCIATED_TOKEN_PROGRAM_ID);
  const r = dataOf(b.main);
  assert.equal(r.u64(), 1_000_000_000n);
  assert.equal(r.u64(), 7n);
  assert.equal(r.u64(), b.graduation.maxGraduationLamports);
  r.finish();
  assert.ok(b.addressCount <= 64, `${b.addressCount} locks`);
  await assert.rejects(buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 0n, minTokens: 0n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS }), /Nothing/);
  await assert.rejects(
    buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, buyerAtaRequiresMemo: true }),
    /MemoTransfer, TokenError::NoMemo/,
  );
});

test('planBuy: [prepare, buy] while the launch is unprepared — on ANY buy, not only one that looks final; [buy] once prepared', async () => {
  const f = await fixtureLaunch();
  const launchAlt = { address: addr(78), addresses: [f.state.ammPool], authority: null, deactivationSlot: 0xffff_ffff_ffff_ffffn };
  const args = { buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000_000_000n, minTokens: 7n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, launchAlt };
  // the per-launch table is mandatory, and must be THIS launch's.
  await assert.rejects(planBuy({ ...args, launchAlt: undefined as never }), /per-launch lookup table is mandatory/);
  await assert.rejects(planBuy({ ...args, launchAlt: { ...launchAlt, addresses: [addr(79)] } }), /does not hold the launch's pool/);
  await assert.rejects(planBuy({ ...args, launchAlt: { ...launchAlt, authority: f.state.creator } }), (e: Error) => e.name === 'LaunchTableNotFrozenError' && /is not frozen/.test(e.message));
  await assert.rejects(planBuy({ ...args, launchAlt: { ...launchAlt, deactivationSlot: 9n } }), (e: Error) => e.name === 'LaunchTableDeactivatedError' && /deactivated in slot 9/.test(e.message));
  await assert.rejects(planBuy({ ...args, launchAlt: { address: addr(78), addresses: [f.state.ammPool] } as never }), /READ BACK from chain/);
  // No read: the plan cannot tell, so it prepares (a fee at worst, idempotent on chain).
  const blind = await planBuy(args);
  assert.deepEqual(blind.steps.map((s) => s.step), ['prepare', 'buy']);
  const [prep, buy] = blind.steps.map((s) => s.tx);
  checkCommon(prep!, 'prepareGraduation', DISC.launch.prepare_graduation);
  checkCommon(buy!, 'buyFinal', DISC.launch.buy);
  assert.equal(prep!.main.accounts[0]?.address, FIXTURE_BUYER, 'the buyer presses prepare (the reserve reimburses it)');
  assert.equal(prep!.main.accounts[0]?.role, AccountRole.WRITABLE_SIGNER);
  // The prepare's list is the buy's fixed slots, and carries every array preparation must plant.
  const named = prep!.main.accountNames.length;
  const prepList = prep!.main.accounts.slice(named);
  const buyList = buy!.main.accounts.slice(buy!.main.accountNames.length);
  const fixed = 33;
  assert.deepEqual(prepList.slice(0, fixed), buyList.slice(0, fixed), 'one graduation list for both instructions');
  const minimal = await buildPrepareGraduation({ payer: FIXTURE_BUYER, launch: f.state, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, bidLowerHint: f.state.openTick + 1 });
  assert.deepEqual(prepList, minimal.main.accounts.slice(named), 'the minimal list (bid at the open)');
  for (const a of prepList.slice(fixed)) assert.ok(buyList.some((b) => b.address === a.address), 'every prepared array is one the buy carries');
  // Unprepared by the read: one slot missing is enough.
  const g = blind.graduation;
  const all = new Set(g.accounts.map((a) => a.address));
  assert.equal(graduationPrepared(g, all), true);
  for (const name of PREPARED_SLOTS) {
    const missing = new Set(all);
    missing.delete(g.accounts[g.names.indexOf(name)]!.address);
    assert.equal(graduationPrepared(g, missing), false, name);
    assert.deepEqual((await planBuy({ ...args, existing: missing })).steps.map((s) => s.step), ['prepare', 'buy'], name);
  }
  // Prepared: the buy alone, the same buy `buildBuy` makes.
  const ready = await planBuy({ ...args, existing: all });
  assert.deepEqual(ready.steps.map((s) => s.step), ['buy']);
  const { launchAlt: _alt, ...buildArgs } = args;
  assert.deepEqual(ready.steps[0]!.tx.main.accounts, (await buildBuy({ ...buildArgs, existing: all })).main.accounts);
  assert.deepEqual((await planBuy({ ...args, prepared: true })).steps.map((s) => s.step), ['buy']);
});

test('buildSellBackWsol: the wSOL ATA created first, then sell_back_wsol with [seller_wsol w, spl_token] as its remaining accounts', async () => {
  const f = await fixtureLaunch();
  const wsol = (await findAta(FIXTURE_BUYER, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)).address;
  const b = await buildSellBackWsol({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 7n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } });
  assert.equal(b.cuLimit, CU_LIMITS.sellBack + CU_LIMITS.ataCreate);
  assert.ok(isCuLimit(b.instructions[0]!, CU_LIMITS.sellBack + CU_LIMITS.ataCreate));
  assert.ok(isFloor(b.main, DISC.launch.sell_back_wsol));
  assert.equal(b.instructions.length, 3);
  const create = b.instructions[1]!;
  assert.equal(create.programAddress, ASSOCIATED_TOKEN_PROGRAM_ID);
  assert.equal(create.data?.[0], 1, 'CreateIdempotent');
  assert.equal(create.accounts?.[1]?.address, wsol);
  assert.equal(create.accounts?.[3]?.address, WSOL_MINT);
  assert.equal(create.accounts?.[5]?.address, SPL_TOKEN_PROGRAM_ID);
  assert.equal(b.instructions[2], b.main);
  const plainNative = await buildSellBack({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 7n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } });
  // the same named accounts as sell_back, then the two remaining accounts
  const named = plainNative.main.accounts!;
  assert.deepEqual(b.main.accounts!.slice(0, named.length), named);
  const rest = b.main.accounts!.slice(named.length);
  assert.deepEqual(rest.map((m) => [m.address, m.role]), [[wsol, AccountRole.WRITABLE], [SPL_TOKEN_PROGRAM_ID, AccountRole.READONLY]]);
  const r = dataOf(b.main);
  assert.equal(r.u64(), 7n);
  r.finish();
  // a caller-named account: no create, the limit is sell_back's
  const own = await buildSellBackWsol({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 7n, sellerWsol: wsol, buyerAta: { cpiGuardLocked: true, delegatedAmount: 0n } });
  checkCommon(own, 'sellBack', DISC.launch.sell_back_wsol);
  assert.equal(own.instructions.length, 3);
  assert.equal(own.instructions[1]?.data?.[0], 13, 'ApproveChecked');
  assert.equal(own.instructions[2], own.main, 'immediately before sell_back_wsol');
  await assert.rejects(buildSellBackWsol({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 0n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } }), /Nothing/);
});

test('buildSellBack: ApproveChecked(delegate = launch, amount = tokens) immediately before sell_back; Revoke only for a residual', async () => {
  const f = await fixtureLaunch();
  const ata = (await findAta(FIXTURE_BUYER, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  const guarded = await buildSellBack({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 1_000_000n, buyerAta: { cpiGuardLocked: true, delegatedAmount: 0n } });
  checkCommon(guarded, 'sellBack', DISC.launch.sell_back);
  assert.equal(guarded.instructions.length, 3);
  const approve = guarded.instructions[1]!;
  assert.equal(approve.programAddress, TOKEN_2022_PROGRAM_ID);
  assert.equal(approve.data?.[0], 13);
  assert.equal(new DataView(approve.data!.buffer, approve.data!.byteOffset).getBigUint64(1, true), 1_000_000n, 'never more than tokens');
  assert.equal(approve.data?.[9], 6, 'decimals');
  assert.equal(approve.accounts?.[0]?.address, ata);
  assert.equal(approve.accounts?.[2]?.address, f.launch, 'delegate = the Launch PDA');
  assert.equal(guarded.instructions[2], guarded.main, 'immediately before sell_back');
  const residual = await buildSellBack({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 5n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 9n } });
  assert.equal(residual.instructions.length, 3);
  assert.equal(residual.instructions[1], residual.main);
  assert.equal(residual.instructions[2]?.data?.[0], 5, 'Revoke appended after sell_back');
  const plain = await buildSellBack({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 5n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } });
  assert.equal(plain.instructions.length, 2);
  const r = dataOf(plain.main);
  assert.equal(r.u64(), 5n);
  r.finish();
  await assert.rejects(buildSellBack({ buyer: FIXTURE_BUYER, launch: f.state, tokens: 0n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } }), /Nothing/);
});

test('buildSwap: plain pool with a wSOL input goes through a temporary wSOL account (never the trader`s ATA); CPI Guard adds Approve/Revoke around the swap', async () => {
  const pool = addr(30);
  const p = plainPool({ mint0: WSOL_MINT });
  const b = await buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: true, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0, 3600], wsolSeed: 'wsolfixed' });
  checkCommon(b, 'swapPlain', DISC.amm.swap);
  const kindOf = (x: BuiltTransaction) => x.instructions.map((ix) => (ix === x.main ? 'swap' : ix.programAddress === COMPUTE_BUDGET_PROGRAM_ID ? 'cu' : ix.programAddress === ASSOCIATED_TOKEN_PROGRAM_ID ? 'ata' : ix.programAddress === SYSTEM_PROGRAM_ID ? 'sys' : `tok${ix.data?.[0]}`));
  assert.deepEqual(kindOf(b), ['cu', 'ata', 'sys', 'tok18', 'swap', 'tok9']);
  const temp = await createAddressWithSeed({ baseAddress: addr(3), programAddress: SPL_TOKEN_PROGRAM_ID, seed: 'wsolfixed' });
  const create = b.instructions[2]!;
  assert.equal(create.accounts?.[1]?.address, temp, 'the temporary account is created with the seed');
  // CreateAccountWithSeed: u32 3 | base (32) | seed (u64 len + bytes) | lamports u64 | space u64 | owner (32)
  const lamportsAt = 4 + 32 + 8 + 'wsolfixed'.length;
  assert.equal(new DataView(create.data!.buffer, create.data!.byteOffset).getBigUint64(lamportsAt, true), TOKEN_ACCOUNT_RENT_LAMPORTS + 1_000n, 'funded with exactly the input, plus rent');
  assert.ok(b.main.accounts.some((m) => m.address === temp), 'the swap spends from the temporary account');
  const close = b.instructions[5]!;
  assert.equal(close.accounts?.[0]?.address, temp, 'only the temporary account is closed');
  const traderWsol = (await findAta(addr(3), WSOL_MINT, SPL_TOKEN_PROGRAM_ID)).address;
  assert.ok(!b.instructions.some((ix) => ix.accounts?.some((m) => m.address === traderWsol)), 'the trader`s wSOL ATA is not touched');
  const sell = await buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: false, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [] });
  assert.deepEqual(kindOf(sell), ['cu', 'sys', 'tok18', 'swap', 'tok9'], 'a wSOL output: a temporary account closed to SOL');
  assert.ok(!sell.instructions.some((ix) => ix.accounts?.some((m) => m.address === traderWsol)));
  assert.equal(b.main.accounts.length, 12 + 2 + 2, 'named + event_cpi + two tick arrays, no hook slice');
  assert.equal(b.main.accounts[11]?.address, DEFAULT_PROGRAM_IDS.carpenterAmm, 'hook_program None → the program id');
  const g = await buildSwap({ trader: addr(3), pool, poolState: plainPool(), zeroForOne: false, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [], sourceHasCpiGuard: true, wrapSol: false });
  const gk = g.instructions.map((ix) => (ix === g.main ? 'swap' : ix.programAddress === COMPUTE_BUDGET_PROGRAM_ID ? 'cu' : ix.programAddress === ASSOCIATED_TOKEN_PROGRAM_ID ? 'ata' : `tok${ix.data?.[0]}`));
  assert.deepEqual(gk, ['cu', 'ata', 'tok13', 'swap', 'tok5']);
  assert.equal(g.instructions[2]?.accounts?.[2]?.address, pool, 'delegate = pool');
  const r = dataOf(g.main);
  assert.equal(r.bool(), false);
  assert.equal(r.i128(), -5n);
  r.u128();
  assert.equal(r.u64(), 1n);
  assert.deepEqual([...r.vecU8()], []);
  r.finish();
});

test('buildSwap on a FLOOR pool: exact-out refused, [floor_pool, trader_bucket, hook event_authority, floor_hook] is the hook slice, CU by side', async () => {
  const pool = addr(31);
  const p = plainPool({ mint0: WSOL_MINT, hook: DEFAULT_PROGRAM_IDS.floorHook });
  await assert.rejects(buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: true, amountSpecified: 5n, amountLimit: 1n, tickArrayStarts: [] }), /ExactOutNotSupported/);
  await assert.rejects(buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: true, amountSpecified: 0n, amountLimit: 1n, tickArrayStarts: [] }), /AmountZero/);
  await assert.rejects(
    buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [0], destinationRequiresMemo: true }),
    /MemoTransfer, TokenError::NoMemo/,
  );
  const buy = await buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [0], wrapSol: false });
  checkCommon(buy, 'swapFloorBuy', DISC.amm.swap);
  const named = buy.main.accountNames.length;
  assert.equal(buy.main.accounts[11]?.address, DEFAULT_PROGRAM_IDS.floorHook);
  assert.equal(buy.main.accounts[named]?.address, (await findFloorPool(pool)).address, 'hook slice first');
  assert.equal(buy.main.accounts[named]?.role, AccountRole.WRITABLE);
  // the slice's second slot is the signer's bucket in the pool's quote (mint0)
  assert.equal(buy.main.accounts[named + 1]?.address, (await findTraderBucket(addr(3), WSOL_MINT)).address, 'trader_bucket second');
  assert.equal(buy.main.accounts[named + 1]?.role, AccountRole.WRITABLE);
  // slots 3 and 4 are the callbacks' `#[event_cpi]` pair, read-only
  assert.equal(buy.main.accounts[named + 2]?.address, (await findEventAuthority(DEFAULT_PROGRAM_IDS.floorHook)).address, 'the hook event authority third');
  assert.equal(buy.main.accounts[named + 2]?.role, AccountRole.READONLY);
  assert.equal(buy.main.accounts[named + 3]?.address, DEFAULT_PROGRAM_IDS.floorHook, 'floor_hook fourth');
  assert.equal(buy.main.accounts[named + 3]?.role, AccountRole.READONLY);
  assert.equal(buy.main.accounts[named + 4]?.address, (await findTickArray(pool, 0)).address, 'the tick-array set after the 4-account slice');
  const sell = await buildSwap({ trader: addr(3), pool, poolState: p, zeroForOne: false, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [0], wrapSol: false });
  assert.equal(sell.cuLimit, CU_LIMITS.swapFloorSell);
  assert.equal(sell.main.accounts[named + 1]?.address, (await findTraderBucket(addr(3), WSOL_MINT)).address, 'a sell books the same bucket');
  await assert.rejects(buildSwap({ trader: addr(3), pool, poolState: plainPool({ hook: addr(99) }), zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [] }), /hookSlice/);
});

// `swap_from_balance`, the graduated route's second leg.
test('too many accounts throws TooManyAccountsError by count', async () => {
  const many = Array.from({ length: 70 }, (_, i) => i * 3600);
  await assert.rejects(
    buildSwap({ trader: addr(3), pool: addr(30), poolState: plainPool(), zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: many }),
    (e: unknown) => e instanceof TooManyAccountsError && e.count > MAX_ACCOUNT_LOCKS,
  );
});

test('FLOOR hook slice: a swap`s slot 2 is the SIGNER`s ["trader_ref", trader, mint0] bucket, writable; the initialize_pool slice passes hook_config read-only', async () => {
  const pool = addr(31);
  const quote = addr(20); // a non-wSOL quote: the bucket keys the pool's mint0, whatever it is
  const p = plainPool({ mint0: quote, hook: DEFAULT_PROGRAM_IDS.floorHook });
  const trader = addr(7);
  // derived independently of findTraderBucket: kit's own PDA over the Rust seed spelling
  const [expected] = await getProgramDerivedAddress({
    programAddress: DEFAULT_PROGRAM_IDS.floorHook,
    seeds: [new TextEncoder().encode('trader_ref'), addressToBytes(trader), addressToBytes(quote)],
  });
  assert.equal((await findTraderBucket(trader, quote)).address, expected);
  for (const zeroForOne of [true, false]) {
    const swap = await buildSwap({ trader, pool, poolState: p, zeroForOne, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [0], wrapSol: false });
    for (const b of [swap]) {
      const named = b.main.accountNames.length;
      assert.equal(b.main.accounts[named]?.address, (await findFloorPool(pool)).address);
      assert.equal(b.main.accounts[named + 1]?.address, expected, 'slot 2 = the signer`s bucket');
      assert.equal(b.main.accounts[named + 1]?.role, AccountRole.WRITABLE, 'the bucket is writable');
    }
  }
  // another signer, another bucket; the same signer on another quote, another bucket
  const other = await buildSwap({ trader: addr(8), pool, poolState: p, zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [], wrapSol: false });
  assert.equal(other.main.accounts[other.main.accountNames.length + 1]?.address, (await findTraderBucket(addr(8), quote)).address);
  assert.notEqual((await findTraderBucket(addr(8), quote)).address, expected);
  // an explicit hookSlice still wins
  const explicit = await buildSwap({ trader, pool, poolState: p, zeroForOne: true, amountSpecified: -5n, amountLimit: 1n, tickArrayStarts: [], wrapSol: false, hookSlice: [{ address: addr(99), role: AccountRole.WRITABLE }] });
  assert.equal(explicit.main.accounts[explicit.main.accountNames.length]?.address, addr(99));
  assert.equal(explicit.main.accounts.length, explicit.main.accountNames.length + 1);

  // initialize_pool's slice: [floor_pool w, hook_config r, hook event_authority r, floor_hook r] — graduation's CPI shape
  const init = await floorInitializeHookSlice(pool);
  assert.deepEqual(init.map((m) => [m.address, m.role]), [
    [(await findFloorPool(pool)).address, AccountRole.WRITABLE],
    [(await findHookConfig()).address, AccountRole.READONLY],
    [(await findEventAuthority(DEFAULT_PROGRAM_IDS.floorHook)).address, AccountRole.READONLY],
    [DEFAULT_PROGRAM_IDS.floorHook, AccountRole.READONLY],
  ]);
});

// A graduating buy does not fit the 1,232-byte packet through the deployment table alone; it fits through the
// deployment table + the launch's own table (the launch's accounts and graduation's list).
test('the v0 message compiles, and the deployment + per-launch ALTs shrink a final buy under 1,232 bytes', async (t) => {
  const f = await fixtureLaunch();
  const b = await buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS });
  const lifetime = { blockhash: '11111111111111111111111111111111' as Blockhash, lastValidBlockHeight: 1n };
  const ids = DEFAULT_PROGRAM_IDS;
  // A deployment table: the programs, configs, event authorities and the wSOL PDAs every launch shares.
  const { findAmmConfig, findApprovedHook, findMintExclusivity, findProgramData } = await import('@floorlaunchpad/carpenter-sdk');
  const { findHookAuthority, findLaunchAuthority, findLaunchConfig, findLockAuthority, findMintAuthority } = await import('../src/pda.ts');
  const { findHoldersAuthority } = await import('../src/holders.ts');
  const pdas = await Promise.all([
    findProgramData(ids.floorHook), ...[ids.carpenterAmm, ids.floorHook, ids.floorLaunch, ids.floorHolders].map((p) => findEventAuthority(p)),
    findAmmConfig(ids), findLaunchConfig(ids), findHookConfig(ids), findApprovedHook(ids.floorHook, ids), findHookAuthority(ids.floorHook),
    findLaunchAuthority(ids), findMintAuthority(ids), findLockAuthority(ids), findMintPools(WSOL_MINT, ids), findMintExclusivity(WSOL_MINT, ids),
    findHoldersAuthority(ids),
  ]);
  const alt = [ids.carpenterAmm, ids.floorHook, ids.floorLaunch, ids.floorHolders, ...pdas.map((p) => p.address), WSOL_MINT, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, COMPUTE_BUDGET_PROGRAM_ID];
  const launchAlt = [...new Set([f.launch, f.state.mint, ...b.graduation.accounts.map((m) => m.address)])].filter((a) => !alt.includes(a));
  const raw = messageSize(buildV0Message({ feePayer: FIXTURE_BUYER, instructions: b.instructions, lifetime }));
  const deploymentOnly = messageSize(buildV0Message({ feePayer: FIXTURE_BUYER, instructions: b.instructions, lifetime, lookupTable: { address: addr(77), addresses: alt } }));
  const both = messageSize(
    buildV0Message({ feePayer: FIXTURE_BUYER, instructions: b.instructions, lifetime, lookupTables: [{ address: addr(77), addresses: alt }, { address: addr(78), addresses: launchAlt }] }),
  );
  t.diagnostic(`final buy: ${raw.bytes} B bare, ${deploymentOnly.bytes} B through the deployment table, ${both.bytes} B with the per-launch table (${launchAlt.length} addresses)`);
  assert.ok(deploymentOnly.bytes < raw.bytes, `${deploymentOnly.bytes} < ${raw.bytes}`);
  assert.ok(!deploymentOnly.fitsLegacyPacket, `a graduating buy needs the launch's table: ${deploymentOnly.bytes} B through the deployment table alone`);
  assert.ok(both.fitsLegacyPacket, `final buy is ${both.bytes} B with both tables (gate 1,232)`);
  const prep = (await planBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, launchAlt: { address: addr(78), addresses: launchAlt, authority: null, deactivationSlot: 0xffff_ffff_ffff_ffffn } })).steps[0]!;
  assert.equal(prep.step, 'prepare');
  const prepSize = messageSize(buildV0Message({ feePayer: FIXTURE_BUYER, instructions: prep.tx.instructions, lifetime, lookupTable: { address: addr(77), addresses: alt } }));
  t.diagnostic(`prepare: ${prepSize.bytes} B through the deployment table alone (${prep.tx.addressCount} accounts)`);
  assert.ok(prepSize.fitsLegacyPacket, `the prepare is ${prepSize.bytes} B through the deployment table (gate 1,232)`);
});

// =================================================================================================

// =================================================================================================

const ZERO32 = Array.from({ length: 32 }, () => 0);

test('buildBuy: a Custom final buy (steps derived from its cap) carries every tick array prepare_graduation creates', async () => {
  const { floorToSpacing, stairEdge, tickArrayStartIndex } = ({ ...(await import('@floorlaunchpad/carpenter-sdk')), ...(await import('../src/index.ts')) });
  const { ladderRange, polRange } = ({ ...(await import('@floorlaunchpad/carpenter-sdk')), ...(await import('../src/index.ts')) });
  const f = await fixtureLaunch();
  const s = 60;
  // Custom staircases at $25k, $200k and $1M caps: 15, 12 and 10 steps.
  for (const [cap, steps] of [[25_000n, 15], [200_000n, 12], [1_000_000n, 10]] as const) {
    const b = await buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000_000_000n, minTokens: 1n, rent: specRent, floorPool: { minBid: 50_000_000n, stairSteps: steps } });
    const open = b.graduation.openTick;
    const [polLo, polHi] = polRange(open, s);
    const [ladLo, ladHi] = ladderRange(open, s);
    const openEdge = floorToSpacing(open, s);
    const planted = new Set([polLo, polHi, ladLo, ladHi, stairEdge(openEdge, 1, s), stairEdge(openEdge, steps, s)].map((t) => tickArrayStartIndex(t, s)));
    const carried = new Set(b.graduation.tickArrayStarts);
    for (const st of planted) assert.ok(carried.has(st), `$${cap} (${steps} steps): prepare_graduation plants ${st}; the final buy carries ${[...carried].join(', ')}`);
    assert.deepEqual(b.main.accounts.slice(b.main.accountNames.length), b.graduation.accounts, 'remaining == the graduation list');
    assert.ok(b.addressCount <= 64, `${b.addressCount} locks`);
  }
  // ⛔ every pool is a staircase: a prediction without the pool's steps is refused by name, never a wall's list
  await assert.rejects(buildBuy({ buyer: FIXTURE_BUYER, launch: f.state, lamportsIn: 1_000n, minTokens: 1n, rent: specRent }), /stairSteps/);
});

test('a link step is benign on failure (AlreadyLinked 6093 / Expired)', () => {
  assert.equal(isBenignLinkFailure(6093), true);
  assert.equal(isBenignLinkFailure(6014), true, 'Expired');
  assert.equal(isBenignLinkFailure(null, ['Program log: AnchorError … Error Code: AlreadyLinked. Error Number: 6093.']), true);
  assert.equal(isBenignLinkFailure(6092), false, 'a badly shaped link is a real error');
  assert.equal(isBenignLinkFailure(null), false);
});
