/**
 * FLOOR's PDAs (`floor_hook`, `floor_launch`), derived exactly as the programs derive them. `ProgramIds` here EXTENDS
 * `@carpenter/sdk`'s with FLOOR's program ids.
 * ⛔ Integers in seeds are LITTLE-endian. ⛔ A pool PDA takes the mints already sorted; `sortMints` is the one place
 *    that sorts.
 */

import {   type Address } from '@solana/kit';

import {
  findHookAuthority as findHookAuthorityOf,
  findAta,
  findPda,
  type ProgramIds as CarpenterProgramIds,
  addressToBytes,
  u64le,
  CARPENTER_AMM_PROGRAM_ID,
} from '@carpenter/sdk';
import { FLOOR_HOLDERS_PROGRAM_ID, FLOOR_HOOK_PROGRAM_ID, FLOOR_LAUNCH_PROGRAM_ID, SEEDS } from './constants.ts';

/** Program ids, overridable per deployment (tests, a redeployed cluster). */
export interface ProgramIds extends CarpenterProgramIds {
  readonly floorHook: Address;
  readonly floorLaunch: Address;
  /**
   * `floor_holders`, the transfer-hook holder tally. ⛔ REQUIRED, not optional: every hooked transfer, every `touch` / `claim`, `create_launch`'s
   * appended accounts and `sell_back`'s holder tail name it, and an optional field would let a caller
   * that forgot it fall through to a default program id in silence. A cluster that has not deployed it
   * cannot trade a HOOKED launch; its unhooked launches never name it.
   */
  readonly floorHolders: Address;
}

export const DEFAULT_PROGRAM_IDS: ProgramIds = {
  carpenterAmm: CARPENTER_AMM_PROGRAM_ID,
  floorHook: FLOOR_HOOK_PROGRAM_ID,
  floorLaunch: FLOOR_LAUNCH_PROGRAM_ID,
  floorHolders: FLOOR_HOLDERS_PROGRAM_ID,
};

const k = addressToBytes;

// ---- floor_hook ---------------------------------------------------------------------

export const findHookConfig = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.hookConfig]);
export const findFloorPool = (ammPool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.floorPool, k(ammPool)]);
/**
 * A trader's `TraderBucket` in one
 * quote, `["trader_ref", trader, quote_mint]` under `floor_hook`. It is the SECOND account of every FLOOR swap's hook slice
 * (`FLOOR_CALLBACK_ACCOUNTS` = 4: `[floor_pool w, trader_bucket w, hook event_authority r, floor_hook r]`), where `trader` is the swap's SIGNER and `quoteMint` the
 * pool's `mint0`. Always pass the derived PDA, writable: the hook books
 * nothing for an absent / uninitialised bucket and never fails the swap over it.
 */
export const findTraderBucket = (trader: Address, quoteMint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorHook, [SEEDS.traderRef, k(trader), k(quoteMint)]);
/** `["trader_ref_vault"]` under `floor_hook` — the WALLET of every quote's
 *  trader-referral vault (off-curve); it signs only `claim_trader_referral`'s payouts. */
export const findTraderRefVaultAuthority = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.traderRefVault]);
/** One quote's trader-referral vault, `ATA(["trader_ref_vault"], quoteMint, tokenProgram)`. */
export const findTraderRefVault = async (quoteMint: Address, tokenProgram: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findAta((await findTraderRefVaultAuthority(ids)).address, quoteMint, tokenProgram);
/** `["stair", floor_pool]` under `floor_hook`. */
export const findStairState = (floorPool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.stair, k(floorPool)]);
/**
 * `["stair_rent", amm_pool]` under floor_hook — a zero-data, SYSTEM-owned PDA
 * holding the launch's prepaid SOL for its staircase's tick arrays. `fund_stair_rent` fills it once after graduation; a
 * ratchet that passes it (writable, among the remaining accounts) with a zero `rent_owner` is adopted; and
 * `reclaim_stair_array` returns a closed array's rent to it.
 */
export const findStairRent = (ammPool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.stairRent, k(ammPool)]);
/** One per hook program. */
export const findHookAuthority = (hookProgram: Address = DEFAULT_PROGRAM_IDS.floorHook) => findHookAuthorityOf(hookProgram);
/**
 *  the program-owned platform treasury, `["platform_treasury"]` — the owner
 * of every token account the platform cut lands in (`findAta(treasury, quote, quoteTokenProgram)`,
 * off-curve), and the `PlatformTreasury` account holding its governance.
 */
export const findPlatformTreasury = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.platformTreasury]);
/**
 *  the program-owned TEAM treasury, `["team_treasury"]` — the owner
 * of every token account the platform fee is swept into (`findAta(team, quote, quoteTokenProgram)`, off-curve),
 * and a `PlatformTreasury`-layout account holding ITS governance. Never the platform treasury.
 */
export const findTeamTreasury = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.teamTreasury]);
/**
 * `["floor_platform_pool"]` — `$FLOOR`'s pool record
 * (decode with `decodeFloorPlatformPool`; absent until the genesis launch designates it) and the owner of the
 * wSOL transit account (`findAta(record, WSOL_MINT, SPL_TOKEN_PROGRAM_ID)`, off-curve).
 */
export const findFloorPlatformPool = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorHook, [SEEDS.floorPlatformPool]);

// ---- floor_launch -------------------------------------------------------------------

export const findLaunchConfig = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.launchConfig]);
export const findRegistryEntry = (index: bigint, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorLaunch, [SEEDS.registryEntry, u64le(index)]);
export const findLaunch = (mint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.launch, k(mint)]);
export const findSaleVault = (launch: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.saleVault, k(launch)]);
/**
 * `["grad_rent", launch]`, the launch's graduation rent reserve. The CREATOR funds it at `create_launch`, graduation spends
 * it, and `close_graduation_rent` reclaims what is left — so the last buy of a sale carries no step change of permanent
 * rent. Zero data, program-owned, lamports only; ⚠ anyone may top it up with a plain SOL transfer.
 */
export const findGraduationRent = (launch: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorLaunch, [SEEDS.gradRent, k(launch)]);
/** `["quote", quote_mint]` under `floor_hook`, the FLOOR quote roster entry. */
export const findQuotePolicy = (quoteMint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorHook, [SEEDS.quotePolicy, k(quoteMint)]);
export const findBuyerPosition = (launch: Address, buyer: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorLaunch, [SEEDS.buyer, k(launch), k(buyer)]);
/**  B1D: `["mint", creator, nonce u64 LE]`. */
export const findLaunchMint = (creator: Address, nonce: bigint, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorLaunch, [SEEDS.mint, k(creator), u64le(nonce)]);
export const findMintAuthority = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.mintAuthority]);
export const findLockAuthority = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.lockAuthority]);
export const findLaunchAuthority = (ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.launchAuthority]);
/** `["trader_link", trader]` under `floor_launch` — the trader's three tiers. */
export const findTraderLink = (trader: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) => findPda(ids.floorLaunch, [SEEDS.traderLink, k(trader)]);
/** Trader referrals: `["trader_ref_sale", launch]` — the launch's SALE referral token account. */
export const findTraderRefSale = (launch: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS) =>
  findPda(ids.floorLaunch, [SEEDS.traderRefSale, k(launch)]);

