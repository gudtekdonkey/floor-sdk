// The holder tally, client side.
//
// What this file holds, and against what:
//   · the account ORDER of every floor_holders instruction and of `claim_referral_fees`, against the
//     reference lists written out literally here (`idl-parity.test.ts` holds the same encoders
//     to the Rust structs and the built IDL — this is the reference half, so a table that agrees with a
//     wrong struct still fails one of the two);
//   · `transferHookAccounts`' derivations against an INDEPENDENT `create_program_address` (node:crypto
//     sha256 over the Rust seed spellings, bump walked down from 255) — never kit's `getProgramDerivedAddress`
//     and never the SDK's own seed table;
//   · `planClaimAll`'s packing: never over 1,232 B, 64 locks or 1.4 M CU, every target placed, the
//     create-idempotent once per quote PER transaction, and the launches-per-transaction figure on record;
//   · the CreateLaunchParams byte layout (the referrer IN PLACE of `fund_referrals`), the sell_back holder
//     tail, the swap / buy / liquidity / vesting hook options, the burn and the wallet send.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from './node-test.ts';

import { AccountRole, isOffCurveAddress,  type Address, type Instruction } from '@solana/kit';

import {
  addressToBytes,
  bytesToAddress,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SPL_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  encodeAccount,
  findAta,
  findVault,
} from '@floorlaunchpad/carpenter-sdk';
import { CU_LIMITS, FLOOR_HOLDERS_PROGRAM_ID } from '../src/constants.ts';
import {
  withoutAccountsAlreadyIn,
  findExtraAccountMetas,
  findHolderBook,
  findHolderRecord,
  findHoldersAuthority,
  transferHookAccounts,
} from '../src/holders.ts';
import {
  decodeHolderBook,
  decodeHolderRecord,
  HOLDER_BOOK,
  HOLDER_RECORD,
} from '../src/layouts.ts';
import { DEFAULT_PROGRAM_IDS,  } from '../src/pda.ts';
import { buildModifyLiquidity as carpenterBuildModifyLiquidity } from '@floorlaunchpad/carpenter-sdk';
import { buildBuy, buildSellBack, buildSwap } from '../src/tx/builders.ts';
import type { QuoteSpec } from '../src/tx/common.ts';
import type { FloorInstruction } from '../src/tx/instructions.ts';
import { addr, FIXTURE_STAIR_STEPS, fixtureLaunch, parseRustHolders, specRent, SOURCE_ONLY } from './helpers.ts';

const ids = DEFAULT_PROGRAM_IDS;
const WSOL: QuoteSpec = { mint: WSOL_MINT, tokenProgram: SPL_TOKEN_PROGRAM_ID, decimals: 9 };

/** The first `addr(n)` from `from` that is ON the curve — a wallet; records exist only for those. */
function onCurve(from: number): Address {
  for (let n = from; ; n++) if (!isOffCurveAddress(addr(n))) return addr(n);
}
/** The first `addr(n)` from `from` that is OFF the curve — a program-owned account, which never holds a record. */
function offCurve(from: number): Address {
  for (let n = from; ; n++) if (isOffCurveAddress(addr(n))) return addr(n);
}

const at = (ix: FloorInstruction, name: string) => ix.accounts[ix.accountNames.indexOf(name)];

// =================================================================================================
// an INDEPENDENT create_program_address — the Rust vectors' own algorithm, not kit's
// =================================================================================================

/**
 * `Pubkey::find_program_address`: for bump 255 → 0, `sha256(seeds ‖ [bump] ‖ program ‖ "ProgramDerivedAddress")`,
 * the first result that is OFF the curve. The seeds are spelled HERE, from the Rust source's strings.
 */
function findPdaIndependently(program: Address, seeds: readonly Uint8Array[]): Address {
  for (let bump = 255; bump >= 0; bump--) {
    const h = createHash('sha256');
    for (const s of seeds) h.update(s);
    h.update(new Uint8Array([bump]));
    h.update(addressToBytes(program));
    h.update('ProgramDerivedAddress');
    const candidate = bytesToAddress(new Uint8Array(h.digest()));
    if (isOffCurveAddress(candidate)) return candidate;
  }
  throw new Error('no bump');
}
const utf8 = (s: string) => new TextEncoder().encode(s);

test('the decoders read the floor_holders book and record', SOURCE_ONLY, () => {
  const book = decodeHolderBook(encodeAccount(HOLDER_BOOK, { version: 1, mint: addr(1), floorPool: addr(2), quoteMint: addr(3), acc: (1n << 70n) + 5n, eligible: 9n, indexed: 10n, unallocated: 11n, paid: 12n, records: 2n, createdSlot: 99n }));
  assert.equal(book.acc, (1n << 70n) + 5n, 'the Q64.64 accumulator is a full u128');
  assert.deepEqual([book.eligible, book.indexed, book.unallocated, book.paid, book.records], [9n, 10n, 11n, 12n, 2n]);
  const rec = decodeHolderRecord(encodeAccount(HOLDER_RECORD, { version: 1, mint: addr(1), tokenAccount: addr(4), owner: addr(5), shares: 7n, snapshot: 3n << 64n, owed: 6n, paid: 1n, openedSlot: 8n }));
  assert.equal(rec.owner, addr(5));
  assert.equal(rec.snapshot, 3n << 64n);
  // `owner` at ACCOUNT offset 80 — the memcmp a wallet's records are listed by
  const bytes = encodeAccount(HOLDER_RECORD, { owner: addr(5) });
  assert.equal(bytesToAddress(bytes.slice(80, 112)), addr(5));
  assert.equal(parseRustHolders().ints['HOLDER_RECORD_ACCT_OWNER'], 80n);
});

// =================================================================================================
// the hook's extra accounts
// =================================================================================================

test('transferHookAccounts: the six accounts, in order, with their roles — every PDA equal to an independent derivation', async () => {
  const f = await fixtureLaunch();
  const source = addr(40);
  const destination = addr(41);
  const metas = await transferHookAccounts({ mint: f.mint, floorPool: f.state.floorPool, source, destination });
  const P = FLOOR_HOLDERS_PROGRAM_ID;
  const expect: [Address, AccountRole][] = [
    [P, AccountRole.READONLY],
    [findPdaIndependently(P, [utf8('extra-account-metas'), addressToBytes(f.mint)]), AccountRole.READONLY],
    [findPdaIndependently(P, [utf8('holder_book'), addressToBytes(f.mint)]), AccountRole.WRITABLE],
    [f.state.floorPool, AccountRole.READONLY],
    [findPdaIndependently(P, [utf8('holder'), addressToBytes(f.mint), addressToBytes(source)]), AccountRole.WRITABLE],
    [findPdaIndependently(P, [utf8('holder'), addressToBytes(f.mint), addressToBytes(destination)]), AccountRole.WRITABLE],
  ];
  assert.deepEqual(metas.map((m) => [m.address, m.role]), expect, '[floor_holders, extra_account_metas, holder_book (w), floor_pool, sender record (w), receiver record (w)]');
  // the SDK's own finders agree with the independent derivation
  assert.equal((await findHolderBook(f.mint)).address, expect[2]![0]);
  assert.equal((await findExtraAccountMetas(f.mint)).address, expect[1]![0]);
  assert.equal((await findHolderRecord(f.mint, source)).address, expect[4]![0]);
  // ⛔ a record is keyed by the TOKEN ACCOUNT: two token accounts of one wallet are two records
  assert.notEqual((await findHolderRecord(f.mint, addr(42))).address, expect[4]![0]);
  // the deployment-wide three, independently
  assert.equal((await findHoldersAuthority()).address, findPdaIndependently(P, [utf8('holders_authority')]));
  // a self-transfer names one record twice — the runtime takes the duplicate
  const self = await transferHookAccounts({ mint: f.mint, floorPool: f.state.floorPool, source, destination: source });
  assert.equal(self[4]!.address, self[5]!.address);
});

// =================================================================================================
// the mechanism's builders on a hooked launch
// =================================================================================================

test('buildSwap on a HOOKED FLOOR pool: the hook accounts appended (floor_pool not twice), the buyer\'s record opened BEFORE the swap', async () => {
  const f = await fixtureLaunch();
  const trader = onCurve(140);
  const pool = f.state.ammPool;
  const floorPool = f.state.floorPool;
  const poolState = {
    mint0: WSOL_MINT, mint1: f.mint, vault0: addr(141), vault1: addr(142), tokenProgram0: SPL_TOKEN_PROGRAM_ID, tokenProgram1: TOKEN_2022_PROGRAM_ID,
    hook: ids.floorHook, decimals0: 9, decimals1: 6,
  };
  const plain = await buildSwap({ trader, pool, poolState, zeroForOne: true, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0] });
  const buy = await buildSwap({ trader, pool, poolState, zeroForOne: true, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0], transferHook: { floorPool, openBuyerRecord: true } });
  const traderBase = (await findAta(trader, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  const hook = await transferHookAccounts({ mint: f.mint, floorPool, source: poolState.vault1, destination: traderBase });
  const tail = buy.main.accounts.slice(plain.main.accounts.length);
  assert.deepEqual(tail, hook.filter((m) => m.address !== floorPool), 'the six minus floor_pool (already the hook slice), appended last');
  assert.equal(buy.main.accounts.filter((m) => m.address === floorPool).length, 1, 'floor_pool once');
  const kinds = buy.instructions.map((ix) => (ix as FloorInstruction).name ?? ix.programAddress);
  assert.equal(buy.instructions[1]?.programAddress, ASSOCIATED_TOKEN_PROGRAM_ID, "the buyer's base ATA first");
  assert.equal((buy.instructions[2] as FloorInstruction).name, 'touch', 'then touch — BEFORE the swap, so execute settles this buy into the record');
  assert.ok(kinds.indexOf('touch') < kinds.indexOf('swap'));
  assert.equal(buy.cuLimit, CU_LIMITS.swapFloorBuy + CU_LIMITS.holdersTouch);
  // a sell carries the accounts in its direction, and refuses openBuyerRecord
  const sell = await buildSwap({ trader, pool, poolState, zeroForOne: false, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0], transferHook: { floorPool } });
  const sellHook = await transferHookAccounts({ mint: f.mint, floorPool, source: traderBase, destination: poolState.vault1 });
  assert.deepEqual(new Set(sell.main.accounts.slice(-5).map((m) => m.address)), new Set(sellHook.filter((m) => m.address !== floorPool).map((m) => m.address)));
  assert.equal(sell.cuLimit, CU_LIMITS.swapFloorSell);
  await assert.rejects(buildSwap({ trader, pool, poolState, zeroForOne: false, amountSpecified: -1n, amountLimit: 1n, tickArrayStarts: [0], transferHook: { floorPool, openBuyerRecord: true } }), /BUY option/);
  await assert.rejects(buildSwap({ trader, pool, poolState: { ...poolState, hook: SYSTEM_PROGRAM_ID as Address }, zeroForOne: true, amountSpecified: -1n, amountLimit: 1n, tickArrayStarts: [0], transferHook: { floorPool } }), /FLOOR pool/);
  // ⛔ a record keys the buyer's ATA alone: an output routed elsewhere cannot open one — refused by name
  await assert.rejects(
    buildSwap({ trader, pool, poolState, zeroForOne: true, amountSpecified: -1n, amountLimit: 1n, tickArrayStarts: [0], traderTokenOut: addr(143), transferHook: { floorPool, openBuyerRecord: true } }),
    /not the buyer's Token-2022 ATA.*NotAssociatedTokenAccount/,
  );
});

test('an OFF-CURVE trader (a PDA wallet) buys on a hooked launch WITHOUT touch — no record can exist for it', async () => {
  const f = await fixtureLaunch();
  const trader = offCurve(145);
  const floorPool = f.state.floorPool;
  const poolState = {
    mint0: WSOL_MINT, mint1: f.mint, vault0: addr(146), vault1: addr(147), tokenProgram0: SPL_TOKEN_PROGRAM_ID, tokenProgram1: TOKEN_2022_PROGRAM_ID,
    hook: ids.floorHook, decimals0: 9, decimals1: 6,
  };
  const kinds = (b: { instructions: readonly Instruction[] }) => b.instructions.map((ix) => (ix as FloorInstruction).name ?? ix.programAddress);
  const buy = await buildSwap({ trader, pool: f.state.ammPool, poolState, zeroForOne: true, amountSpecified: -1_000n, amountLimit: 1n, tickArrayStarts: [0], transferHook: { floorPool, openBuyerRecord: true } });
  assert.ok(!kinds(buy).includes('touch'), 'buildSwap: no touch for an off-curve buyer (it would fail the whole buy, OwnerOffCurve)');
  assert.equal(buy.cuLimit, CU_LIMITS.swapFloorBuy, 'and no touch row in the limit');
  const hook = await transferHookAccounts({ mint: f.mint, floorPool, source: poolState.vault1, destination: (await findAta(trader, f.mint, TOKEN_2022_PROGRAM_ID)).address });
  assert.deepEqual(buy.main.accounts.slice(-5), hook.filter((m) => m.address !== floorPool), 'the hook accounts still ride the base leg');
  const sale = await buildBuy({ buyer: trader, launch: f.state, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, touch: {} });
  assert.ok(!kinds(sale).includes('touch'), 'buildBuy: likewise');
  const onBuy = await buildBuy({ buyer: onCurve(148), launch: f.state, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, touch: {} });
  assert.ok(kinds(onBuy).includes('touch'));
});

test('liquidity on a hooked FLOOR pool: @floorlaunchpad/carpenter-sdk\'s buildModifyLiquidity carries transferHookAccounts\' extra accounts', async () => {
  const f = await fixtureLaunch();
  const owner = onCurve(150);
  const poolState = {
    mint0: WSOL_MINT, mint1: f.mint, vault0: addr(151), vault1: addr(152), tokenProgram0: SPL_TOKEN_PROGRAM_ID, tokenProgram1: TOKEN_2022_PROGRAM_ID,
    hook: ids.floorHook, decimals0: 9, decimals1: 6, tickSpacing: 60,
  };
  const t1 = (await findAta(owner, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  const hook = await transferHookAccounts({ mint: f.mint, floorPool: f.state.floorPool, source: t1, destination: poolState.vault1 });
  const extra = withoutAccountsAlreadyIn(hook, [{ address: f.state.floorPool, role: 1 }]);
  const args = {
    owner, pool: f.state.ammPool, poolState, salt: 1n, tickLower: -60, tickUpper: 60, liquidityDelta: 1n, amount0Max: 1n, amount1Max: 1n,
    hookSlice: [], transferHookAccounts: extra, allowNoMinimum: true,
  };
  const b = await carpenterBuildModifyLiquidity(args as unknown as Parameters<typeof carpenterBuildModifyLiquidity>[0]);
  assert.deepEqual(b.main.accounts.slice(-5), extra);
});

test('buildBuy { touch } appends touch AFTER the buy; buildSellBack { holder } carries the 4-account tail after the quote leg', async () => {
  const f = await fixtureLaunch();
  const buyer = onCurve(160);
  const buy = await buildBuy({ buyer, launch: f.state, lamportsIn: 1_000_000n, minTokens: 1n, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, touch: {} });
  const last = buy.instructions[buy.instructions.length - 1] as FloorInstruction;
  assert.equal(last.name, 'touch', 'after the buy: the buy CREATES the ATA touch needs');
  assert.equal(at(last, 'floor_pool')?.address, f.state.floorPool, "floorPool defaults to the launch's");
  assert.equal(at(last, 'payer')?.address, buyer, 'the buyer pays their own record deposit, in their own buy');
  assert.equal(buy.cuLimit, CU_LIMITS.buyFinal, 'still the 1.4 M cap');
  const sell = await buildSellBack({ buyer, launch: f.state, tokens: 5n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n }, holder: { floorPool: f.state.floorPool } });
  const buyerAta = (await findAta(buyer, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  const tail = sell.main.accounts.slice(-4);
  assert.deepEqual(
    tail.map((m) => [m.address, m.role]),
    [
      [FLOOR_HOLDERS_PROGRAM_ID, AccountRole.READONLY],
      [(await findHolderBook(f.mint)).address, AccountRole.WRITABLE],
      [f.state.floorPool, AccountRole.READONLY],
      [(await findHolderRecord(f.mint, buyerAta)).address, AccountRole.WRITABLE],
    ],
    '[floor_holders, holder_book (w), floor_pool (ro), the seller\'s record (w)]',
  );
  assert.equal(sell.cuLimit, CU_LIMITS.sellBack + CU_LIMITS.holdersSync);
  const plain = await buildSellBack({ buyer, launch: f.state, tokens: 5n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n } });
  assert.equal(sell.main.accounts.length - plain.main.accounts.length, 4, 'an unhooked launch: counts unchanged');
  // a token-quoted launch: the tail FOLLOWS the 4-account quote leg
  const stock: QuoteSpec = { mint: addr(3), tokenProgram: TOKEN_2022_PROGRAM_ID, decimals: 8 };
  const stockSell = await buildSellBack({ buyer, launch: f.state, quote: stock, tokens: 5n, buyerAta: { cpiGuardLocked: false, delegatedAmount: 0n }, holder: { floorPool: f.state.floorPool } });
  assert.equal(stockSell.main.accounts.at(-5)?.address, (await findAta(buyer, stock.mint, stock.tokenProgram)).address, 'the quote leg ends with the buyer\'s quote ATA, then the tail');
});
