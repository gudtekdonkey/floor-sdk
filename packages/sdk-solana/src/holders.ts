/**
 * `floor_holders` — the holder tally a launch with a holder rate carries as a Token-2022 TRANSFER HOOK
 * (`floor_holders::execute`), client side: its PDAs, the extra accounts a hooked transfer needs, the record-opening
 * `touch` a buy carries.
 *
 * ⛔ EVERY ADDRESS IS DERIVED OFFLINE. A hooked transfer's six extra accounts, a record, the book, the metas list — all
 *    PDAs of the mint, the token account and the program id; nothing here reads the chain.
 * ⛔ RECORDS EXIST ONLY FOR THE Token-2022 ATA OF AN ON-CURVE OWNER: the program refuses any other token account
 *    (`NotAssociatedTokenAccount`) and any program-owned owner. The builders derive the ATA themselves and refuse an
 *    off-curve owner BY NAME before the wallet is asked.
 * ⛔ What a record is owed is the program's arithmetic (the quoter's `holderClaimable` runs it); this module never
 *    computes it.
 */

import { AccountRole, isOffCurveAddress, type AccountMeta, type Address,   } from '@solana/kit';

import {
  addressToBytes,
  DYNAMIC_FEE_FLAG,
  TOKEN_2022_PROGRAM_ID,
  WSOL_MINT,
  findAta,
  findBitmap,
  findEventAuthority,
  findMintExclusivity,
  findMintPools,
  findPda,
  findPool,
  findPoolIndexEntry,
  findPosition,
  findQuoteMint,
  findVault,
  type Pda,
} from '@floorlaunchpad/carpenter-sdk';
import { FLOOR_TICK_SPACING, HOLDERS_SEEDS, SALT_BID, SALT_SEED } from './constants.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findFloorPool,
  findGraduationRent,
  findHookAuthority,
  findLaunch,
  findQuotePolicy,
  findSaleVault,
  type ProgramIds,
  findTraderRefSale,
} from './pda.ts';
import type { QuoteSpec } from './tx/common.ts';
import { holdersTouchIx, type FloorInstruction } from './tx/instructions.ts';

// The decoded shapes and their decoders live in `layouts.ts` beside every other account; they are
// re-exported here so a consumer of this module finds the whole surface in one place.
export { decodeHolderBook, decodeHolderRecord } from './layouts.ts';
export type { HolderBook, HolderRecord } from './layouts.ts';

// =================================================================================================
// PDAs
// =================================================================================================

const k = addressToBytes;

/**
 * ⚠ **These return the SDK's `Pda` (`{ address, bump }`), NOT kit's `ProgramDerivedAddressWithBump` tuple**, like
 * every other `find*` in this SDK. Read `.address`.
 */
export const findHolderBook = (mint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<Pda> =>
  findPda(ids.floorHolders, [HOLDERS_SEEDS.holderBook, k(mint)]);
/** `["holder", mint, token_account]` — ⛔ keyed by the TOKEN ACCOUNT, never the wallet. */
export const findHolderRecord = (mint: Address, tokenAccount: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<Pda> =>
  findPda(ids.floorHolders, [HOLDERS_SEEDS.holderRecord, k(mint), k(tokenAccount)]);
/** The transfer-hook interface's validation account, `["extra-account-metas", mint]` under the hook program. */
export const findExtraAccountMetas = (mint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<Pda> =>
  findPda(ids.floorHolders, [HOLDERS_SEEDS.extraAccountMetas, k(mint)]);
/** `["holders_authority"]` — signs `floor_hook::pay_holder`. */
export const findHoldersAuthority = (ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<Pda> => findPda(ids.floorHolders, [HOLDERS_SEEDS.holdersAuthority]);

/** A holder's record is keyed by THEIR ATA of the mint: the pair every builder below derives. */
async function recordOf(owner: Address, mint: Address, ids: ProgramIds): Promise<{ tokenAccount: Address; record: Address }> {
  const tokenAccount = (await findAta(owner, mint, TOKEN_2022_PROGRAM_ID)).address;
  const record = (await findHolderRecord(mint, tokenAccount, ids)).address;
  return { tokenAccount, record };
}

// =================================================================================================
// the hook's extra accounts — appended to EVERY hooked transfer
// =================================================================================================

/**
 * The accounts Token-2022 needs to CPI `floor_holders::execute` on a transfer of a hooked mint, in the
 * program's order: `[floor_holders (ro), extra_account_metas (ro), holder_book (w), floor_pool (ro),
 * sender record (w), receiver record (w)]`. Offline — no reads.
 *
 * ⛔ The records are the SOURCE and DESTINATION token accounts' — `["holder", mint, source]` and
 *    `["holder", mint, destination]` — whether or not either exists: `execute` SKIPS a missing record
 *    (a pool vault, an escrow, a plain-send receiver who has not started earning) and never fails on
 *    bookkeeping. A self-transfer names one record twice; Token-2022 and the runtime take duplicates.
 * Token-2022 (`invoke_execute`) and the AMM's hook-aware legs
 *    find every one BY KEY. It is fixed here so a golden can hold it and a reader can check it against
 *    the metas list (entries [0]..[3] = the book, the pool, the sender's record, the receiver's).
 * ⚠ The two direction-dependent entries are the SET {source record, destination record}: an AMM leg that
 *   moves either way between the same two token accounts (a modify_liquidity, a fee collect) names the
 *   same six accounts.
 */
export async function transferHookAccounts(a: { mint: Address; floorPool: Address; source: Address; destination: Address; ids?: ProgramIds }): Promise<AccountMeta[]> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const [metas, book, sender, receiver] = await Promise.all([
    findExtraAccountMetas(a.mint, ids),
    findHolderBook(a.mint, ids),
    findHolderRecord(a.mint, a.source, ids),
    findHolderRecord(a.mint, a.destination, ids),
  ]);
  return [
    { address: ids.floorHolders, role: AccountRole.READONLY },
    { address: metas.address, role: AccountRole.READONLY },
    { address: book.address, role: AccountRole.WRITABLE },
    { address: a.floorPool, role: AccountRole.READONLY },
    { address: sender.address, role: AccountRole.WRITABLE },
    { address: receiver.address, role: AccountRole.WRITABLE },
  ];
}

/**
 * `extra` minus every address `already` carries — for an AMM instruction, which finds the hook's accounts
 * BY KEY in its remaining set and already names `floor_pool` (its hook slice). ⛔ Never used on a
 * top-level `transfer_checked`, whose own account list is the four standard slots.
 */
export function withoutAccountsAlreadyIn(extra: readonly AccountMeta[], already: readonly AccountMeta[]): AccountMeta[] {
  const seen = new Set(already.map((m) => m.address));
  const out: AccountMeta[] = [];
  for (const m of extra) {
    if (seen.has(m.address)) continue;
    seen.add(m.address);
    out.push(m);
  }
  return out;
}

// =================================================================================================
// instructions
// =================================================================================================

/** ⛔ A record can never exist for a program-owned (off-curve) owner — refused here, before the wallet. */
function refuseOffCurve(fn: string, owner: Address): void {
  if (isOffCurveAddress(owner)) {
    throw new Error(`${fn}: ${owner} is OFF-CURVE (a program-owned account) — floor_holders never opens a record for one (OwnerOffCurve); it earns nothing, and its share goes to holders with records`);
  }
}

async function openAccounts(fn: string, a: { payer: Address; owner: Address; mint: Address; floorPool: Address; ids?: ProgramIds }) {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  refuseOffCurve(fn, a.owner);
  const [{ tokenAccount, record }, book, ev] = await Promise.all([recordOf(a.owner, a.mint, ids), findHolderBook(a.mint, ids), findEventAuthority(ids.floorHolders)]);
  return {
    program: ids.floorHolders,
    eventAuthority: ev.address,
    payer: a.payer,
    owner: a.owner,
    mint: a.mint,
    holderBook: book.address,
    floorPool: a.floorPool,
    tokenAccount,
    record,
  };
}

/** `touch`: open `owner`'s record when missing, else `sync` it. What a buy appends. */
export async function touchIx(a: { payer: Address; owner: Address; mint: Address; floorPool: Address; ids?: ProgramIds }): Promise<FloorInstruction> {
  return holdersTouchIx(await openAccounts('touchIx', a));
}
