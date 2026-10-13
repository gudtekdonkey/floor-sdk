/**
 * FLOOR's accounts (`floor_hook`, `floor_launch`, `floor_holders`), decoded from their bytes on `@carpenter/sdk`'s codec
 * (`decodeAccount`, `encodeAccount`, `FieldKind`, `LayoutDef`).
 *
 * The field tables are transcriptions of the programs' `layout!` lists: STRUCT offsets, discriminator outside.
 * `test/rust-parity.test.ts` requires the same names, offsets and sizes when the program source is present.
 * ⛔ Account offset = struct offset + 8. ⛔ 128-bit fields are two LE u64 limbs (low first).
 * ⛔ Decoders check the discriminator AND the length; a wrong account is an error, never zeros.
 */

import type { Address } from '@solana/kit';

// The `PayeeSlots` tail of a `FloorPool` account.
import { decodePayeeSlots, type PayeeSlots } from './slots.ts';
import {
  TEAM_POOL_FEE_BPS_DEFAULT,
  TEAM_POOL_FEE_BPS_MAX,
  TEAM_SALE_FEE_BPS_DEFAULT,
  TEAM_SALE_FEE_BPS_MAX,
} from './constants.ts';
import {
  SYSTEM_PROGRAM_ID,
  WSOL_MINT,
  LAYOUTS as CARPENTER_LAYOUTS,
  decodeAccount,
  type FieldKind,
  type LayoutDef,
  readField,
  snakeToCamel,
} from '@carpenter/sdk';

/**
 * The all-zero pubkey, base58. ⛔ It is a VALID address, not an error value: the layouts that use
 * it use it as an ABSENT sentinel (`pda.ts` aliases the same value `NO_HOOK` for a hookless pool;
 * `FloorPool.referrer` uses it for a pool that funds no referral).
 */
const ZERO_KEY: Address = SYSTEM_PROGRAM_ID;

/** Padding and reserved bytes are not surfaced. */
const HIDDEN = /^(pad\d*|reserved\d*)$/;

const u8 = { t: 'u8' } as const;
const u16 = { t: 'u16' } as const;
const u32 = { t: 'u32' } as const;
const i32 = { t: 'i32' } as const;
const u64 = { t: 'u64' } as const;
const i64 = { t: 'i64' } as const;
const u128 = { t: 'u128' } as const;
const pk = { t: 'pubkey' } as const;
const bytes = (n: number): FieldKind => ({ t: 'bytes', n });
const d = (...b: number[]): Uint8Array => new Uint8Array(b);

type Row = readonly [string, number, FieldKind];
function def(name: string, size: number, discriminator: Uint8Array, rows: readonly Row[], trailing = false): LayoutDef {
  return { name, size, discriminator, trailing, fields: rows.map(([n, o, kind]) => ({ name: n, offset: o, kind })) };
}

// ---- floor_hook ------------------------------------------------------------------------

export const HOOK_CONFIG = def('HookConfig', 232, d(0x89, 0x9b, 0x65, 0x5f, 0x8a, 0x48, 0x08, 0xb6), [
  ['version', 0, u8], ['bump', 1, u8], ['max_rounds', 2, u8], ['callback_accounts', 3, u8], ['flags', 4, u16],
  ['authority_bump', 6, u8], ['pad0', 7, u8], ['governance', 8, pk], ['platform_destination', 40, pk],
  ['launch_program', 72, pk], ['launch_authority', 104, pk], ['hook_authority', 136, pk],
  ['platform_owner', 168, pk],

  ['pending_governance', 200, pk],
]);

export const FLOOR_POOL = def('FloorPool', 784, d(0x73, 0xeb, 0xd8, 0x17, 0xd1, 0x71, 0x95, 0xc4), [
  ['version', 0, u8], ['bump', 1, u8], ['pool_bound', 2, u8], ['quote_is_token1', 3, u8], ['ladder_seeded', 4, u8],
  ['needs_ratchet', 5, u8], ['mode', 6, u8], ['quote_suspended', 7, u8], ['tick_spacing', 8, i32], ['floor_tick', 12, i32],
  ['amm_pool', 16, pk], ['pool_initializer', 48, pk], ['base_mint', 80, pk], ['quote_mint', 112, pk],
  ['pad9', 144, bytes(32)], ['pad10', 176, bytes(32)],
  ['skim_buy_far_bps', 208, u16], ['skim_sell_far_bps', 210, u16], ['creator_buy_bps', 212, u16],
  ['creator_sell_bps', 214, u16], ['holder_buy_bps', 216, u16], ['holder_sell_bps', 218, u16],
  ['wall_ticks', 220, u16], ['platform_bps', 222, u16], ['min_bid', 224, u64], ['min_sellable', 232, u64],
  ['owner_owed', 240, u64], ['owner_paid', 248, u64], ['vault_eth', 256, u64], ['bid_eth_deployed', 264, u64],
  ['unpaid_skim', 272, u64], ['cumulative_volume_eth', 280, u64], ['inventory_tokens', 288, u64],
  ['inventory_cost_eth', 296, u64], ['ask_tokens_deployed', 304, u64], ['ladder_tokens_remaining', 312, u64],
  ['bid_lower', 320, i32], ['ask_upper', 324, i32], ['ask_width', 328, i32], ['seed_lower', 332, i32],
  ['seed_upper', 336, i32], ['ladder_lower', 340, i32], ['ladder_upper', 344, i32], ['wall_split_tick', 348, i32],
  ['bid_liquidity', 352, u128], ['ask_liquidity', 368, u128], ['seed_liq', 384, u128], ['ladder_liq', 400, u128],
  ['creator_fees_owed', 416, u64], ['platform_owed', 424, u64], ['holder_owed', 432, u64], ['wall_tail_liq', 440, u128],
  ['wall_held_tokens', 456, u64], ['seed_fees_swept_eth', 464, u64],
  ['seed_fees_swept_tokens', 472, u64], ['ladder_proceeds_eth', 480, u64], ['platform_paid_eth', 488, u64],
  ['platform_swept', 496, u64], ['creator_fees_paid', 504, u64], ['holder_paid_eth', 512, u64],
  ['last_ratchet_slot', 520, u64],

  ['bid_width', 528, i32], ['skim_buy_at_floor_bps', 532, u16], ['skim_sell_at_floor_bps', 534, u16],
  ['pad6', 536, bytes(32)], ['max_bounty', 568, u64], ['bounty_bps', 576, u16],
  ['wall_band_ticks', 578, u16], ['wall_tail_bps', 580, u16], ['wall_list_bps', 582, u16],
  ['referral_owed', 584, u64], ['referral_fees_paid', 592, u64],
  ['trader_ref_owed', 600, u64], ['trader_ref_booked', 608, u64], ['pad7', 616, bytes(16)],
  ['referral_share_bps', 632, u16], ['stair_steps', 634, u8], ['stair_ready', 635, u8], ['stair_growth_bps', 636, u16],
  ['stair_kind', 638, u8],
  ['stair_owner', 639, u8],
  ['referrer', 640, pk], ['pad8', 672, u8], ['holders_hooked', 673, u8], ['pad4', 674, bytes(6)],
  ['referrer_t2', 680, pk], ['referrer_t3', 712, pk], ['referral_paid_t2', 744, u64], ['referral_paid_t3', 752, u64],
  ['team_owed', 760, u64], ['team_fees_paid', 768, u64], ['team_fee_bps', 776, u16], ['pad5', 778, bytes(6)],
], true);


export const LAUNCH_CONFIG = def('LaunchConfig', 312, d(0x12, 0xa1, 0x09, 0xe0, 0x66, 0x91, 0x1d, 0x5e), [
  ['version', 0, u8], ['bump', 1, u8], ['cluster', 2, u8], ['pad0', 3, bytes(5)], ['governance', 8, pk],
  ['attestor', 40, pk], ['hook_program', 72, pk], ['amm_program', 104, pk], ['platform_bps', 136, u16],
  ['tick_spacing', 138, u16], ['pad1', 140, bytes(4)], ['min_bid_lamports', 144, u64],
  ['pad3', 152, bytes(16)], ['min_sellable', 168, u64], ['registry_count', 176, u64],
  ['max_bounty_lamports', 184, u64], ['metadata_base_set_slot', 192, u64], ['bounty_bps', 200, u16],
  ['metadata_base_len', 202, u8], ['metadata_base', 203, bytes(64)], ['pad2', 267, bytes(5)],
  ['pending_governance', 272, pk], ['team_sale_bps_raw', 304, u16], ['team_pool_bps_raw', 306, u16],
  ['team_bps_set', 308, u8], ['reserved', 309, bytes(3)],
]);

export const REGISTRY_ENTRY = def('RegistryEntry', 120, d(0x30, 0xc6, 0xf0, 0xfc, 0x9b, 0xba, 0x48, 0x10), [
  ['index', 0, u64], ['launch', 8, pk], ['mint', 40, pk], ['amm_pool', 72, pk], ['mode', 104, u8],
  ['pad0', 105, bytes(7)], ['created_slot', 112, u64],
]);

export const LAUNCH = def('Launch', 512, d(0x90, 0x33, 0x33, 0xa3, 0xce, 0x55, 0xd5, 0x26), [
  ['version', 0, u8], ['bump', 1, u8], ['sale_vault_bump', 2, u8], ['lock_bump', 3, u8], ['state', 4, u8], ['mode', 5, u8],
  ['mint_authority_bump', 6, u8], ['pad0', 7, u8], ['mint', 8, pk], ['creator', 40, pk],
  ['team_cliffs_paid_by_slot', 72, u32], ['pad3', 76, bytes(28)], ['pad4', 104, bytes(32)], ['amm_pool', 136, pk], ['floor_pool', 168, pk],
  ['token_vault', 200, pk], ['wsol_vault', 232, pk], ['price_x64', 264, u128], ['quote_usd_cents', 280, u64],
  ['mcap_usd', 288, u64], ['created_slot', 296, u64], ['nonce', 304, u64], ['sale_tokens', 312, u64],
  ['pol_tokens', 320, u64], ['ladder_tokens', 328, u64], ['sold', 336, u64], ['raised', 344, u64],
  ['buyers', 352, u64], ['open_tick', 360, i32],
  ['trader_ref_sale_bump', 364, u8], ['pad1', 365, bytes(3)], ['skim_buy_far', 368, u16],
  ['skim_sell_far', 370, u16], ['creator_buy', 372, u16], ['creator_sell', 374, u16], ['holder_buy', 376, u16],
  ['holder_sell', 378, u16], ['team_bps', 380, u16], ['pad2', 382, u16], ['registry_index', 384, u64], ['graduated_slot', 392, u64],
  ['quote_mint', 400, pk], ['team_owed', 432, u64],
  ['team_step_tokens', 440, u64], ['team_wallet_tokens', 448, u64], ['team_anchor', 456, i64], ['team_lock_days', 464, u16],
  ['team_vest_days', 466, { t: 'u16s', n: 4 }], ['pad5', 474, u8], ['pad6', 475, u8],
  ['pad7', 476, bytes(32)],
  ['stair_rent_budget', 508, u32],
]);

/**
 * `["quote", quote_mint]` under `floor_hook` — the FLOOR quote roster. The
 * bounds a pool on this quote must sit in AND the VALUES a launch on it is registered with: `min_bid` / `max_bounty` in the quote's own base units,
 * governance-set at `list_quote`, retuned by `set_quote_economics`, read by `create_launch`.
 */
export const QUOTE_POLICY = def('QuotePolicy', 168, d(0x01, 0x2b, 0x1a, 0x4f, 0x56, 0x27, 0xab, 0x43), [
  ['version', 0, u8], ['bump', 1, u8], ['decimals', 2, u8], ['retired', 3, u8], ['pad0', 4, bytes(4)],
  ['quote_mint', 8, pk], ['token_program', 40, pk], ['min_bid_floor', 72, u64], ['min_bid_ceil', 80, u64],
  ['max_bounty_ceil', 88, u64], ['min_bid', 96, u64], ['max_bounty', 104, u64], ['reserved', 112, bytes(56)],
]);

export const BUYER_POSITION = def('BuyerPosition', 40, d(0xe8, 0xa3, 0xa7, 0x5f, 0xaa, 0xd2, 0xd6, 0x53), [
  ['bump', 0, u8], ['pad0', 1, bytes(7)], ['allocation', 8, u64], ['contributed', 16, u64], ['reserved', 24, bytes(16)],
]);

// ---- floor_holders --------------
//
// ⛔ Every one of these lives under `floor_holders` (`FLOOR_HOLDERS_PROGRAM_ID`), and the tally they carry
//    is the program crate's — ONE implementation (the quoter's `holderClaimable` runs it). This SDK DECODES them and
//    never re-implements the math.

/**
 * `["holder_book", mint]` — one launch's tally. Struct 200 B, account 208 B.
 * `acc` is a WRAPPING Q64.64 reward-per-share; `indexed` the lifetime `holder_owed + holder_paid_eth`
 * already folded; `unallocated` what could not be distributed yet (no eligible holder, the rounding
 * remainder, a burn clip) — it rides into the next fold.
 */
export const HOLDER_BOOK = def('HolderBook', 200, d(0x1d, 0x5d, 0xd0, 0x72, 0x4a, 0xee, 0x06, 0xf6), [
  ['version', 0, u8], ['bump', 1, u8], ['metas_bump', 2, u8], ['pad0', 3, bytes(5)],
  ['mint', 8, pk], ['floor_pool', 40, pk], ['quote_mint', 72, pk],
  ['acc', 104, u128], ['eligible', 120, u64], ['indexed', 128, u64], ['unallocated', 136, u64], ['paid', 144, u64],
  ['records', 152, u64], ['created_slot', 160, u64], ['pending', 168, u64], ['last_slot', 176, u64],
  ['reserved', 184, bytes(16)],
]);

/**
 * `["holder", mint, token_account]` — ONE holder's record. Struct 160 B, account 168 B; its rent is the
 * holder's own deposit, refunded to the holder when it closes.
 * ⛔ Keyed by the TOKEN ACCOUNT, never the wallet.
 * `owner` sits at ACCOUNT offset 80 — the `getProgramAccounts` memcmp a wallet's records are listed by.
 */
export const HOLDER_RECORD = def('HolderRecord', 160, d(0xd7, 0x43, 0x1f, 0x29, 0x9b, 0xfb, 0xb2, 0x80), [
  ['version', 0, u8], ['bump', 1, u8], ['pad0', 2, bytes(6)],
  ['mint', 8, pk], ['token_account', 40, pk], ['owner', 72, pk],
  ['shares', 104, u64], ['snapshot', 112, u128], ['owed', 128, u64], ['paid', 136, u64], ['opened_slot', 144, u64],
  ['reserved', 152, bytes(8)],
]);

/**
 * `["platform_treasury"]` under `floor_hook` —. ?").
 * The program-owned platform treasury: its ADDRESS owns every token account the platform cut is
 * swept into, its DATA the governance that alone may withdraw (rotatable in two steps). Created by
 * `initialize_hook_config`.
 */
export const PLATFORM_TREASURY = def('PlatformTreasury', 104, d(0x23, 0xe5, 0x0f, 0xdd, 0xd7, 0xae, 0xa7, 0x5c), [
  ['version', 0, u8], ['bump', 1, u8], ['allow_all_quotes', 2, u8], ['pad0', 3, bytes(5)], ['governance', 8, pk],
  ['pending_governance', 40, pk], ['reserved', 72, bytes(32)],
]);

/**
 * `["floor_platform_pool"]` under `floor_hook` — `$FLOOR` NATIVE TO SOLANA. `$FLOOR`'s
 * pool, recorded ONCE by `designate_floor_pool`, and `received`: the lifetime wSOL cut credited into its backing
 * by `credit_floor_pool` (the EVM's `platformReceivedEth` of `$FLOOR`'s pool). `floor_types::layout::FloorPlatformPool`.
 */
export const FLOOR_PLATFORM_POOL = def('FloorPlatformPool', 224, d(0xa7, 0xd0, 0xa8, 0x21, 0x89, 0x23, 0x05, 0x1f), [
  ['version', 0, u8], ['bump', 1, u8], ['pad0', 2, bytes(6)], ['floor_pool', 8, pk], ['amm_pool', 40, pk],
  ['base_mint', 72, pk], ['quote_mint', 104, pk], ['received', 136, u64], ['credits', 144, u64],
  ['designated_slot', 152, u64], ['reserved', 160, bytes(64)],
]);

/**
 * `["trader_link", trader]` under `floor_launch` — the
 * attestor's link, written ONCE by `link_trader` and never changed. A tier is the zero key when unset (no gaps).
 */
export const TRADER_LINK = def('TraderLink', 136, d(0x47, 0xaa, 0x70, 0x4b, 0xf3, 0x50, 0xc0, 0xad), [
  ['trader', 0, pk], ['tier1', 32, pk], ['tier2', 64, pk], ['tier3', 96, pk], ['bump', 128, u8], ['pad0', 129, bytes(7)],
]);

/**
 * `["trader_ref", trader, quote_mint]` under `floor_hook` — what a trader's trades owe
 * each tier in one quote, `owed[i]` for tier `i + 1`; the tiers are COPIED from the `TraderLink` when the bucket opens.
 */
export const TRADER_BUCKET = def('TraderBucket', 192, d(0xc4, 0xae, 0x45, 0x6d, 0xad, 0xb3, 0x77, 0xbb), [
  ['trader', 0, pk], ['quote_mint', 32, pk], ['tier1', 64, pk], ['tier2', 96, pk], ['tier3', 128, pk],
  ['owed', 160, { t: 'u64s', n: 3 }], ['bump', 184, u8], ['pad0', 185, bytes(7)],
]);

/**
 * `["stair", floor_pool]` under `floor_hook` — a staircase pool's own record. Created EMPTY (`version` 0) ahead of the graduating buy and FILLED by it: every step's planned range, step 0's position, the RESERVE (every step from `placed` on,
 * one position), the open tail, the cushion and the POL re-list, the lifetime meters, and THE FEED's (the vault's
 * bought-back tokens: `vaultListedTokens` / `vaultListedCost`, and the bought tokens they carried, `vaultFedBought`).
 */
export const STAIR_STATE = def('StairState', 584, d(0xb8, 0x99, 0xc1, 0xde, 0x1a, 0xed, 0xf9, 0x72), [
  ['version', 0, u8], ['bump', 1, u8], ['steps', 2, u8], ['placed', 3, u8], ['first_open', 4, u8], ['tail_placed', 5, u8],
  ['growth_bps', 6, u16], ['open_edge', 8, i32], ['tail_lower', 12, i32], ['tail_upper', 16, i32], ['cushion_lower', 20, i32],
  ['cushion_upper', 24, i32], ['relist_lower', 28, i32], ['relist_upper', 32, i32],
  ['relist_cost_tick', 36, i32],
  ['stair_total', 40, u64], ['tail_total', 48, u64], ['collected_total', 56, u64], ['to_pol_total', 64, u64],
  ['to_floor_total', 72, u64], ['cushion_quote', 80, u64], ['tail_liq', 88, u128], ['cushion_liq', 104, u128],
  ['relist_liq', 120, u128], ['step_lower', 136, { t: 'i32s', n: 16 }], ['step_upper', 200, { t: 'i32s', n: 16 }],
  ['step_liq', 264, { t: 'u128s', n: 16 }], ['reserve_lower', 520, i32], ['reserve_upper', 524, i32], ['reserve_liq', 528, u128],
  ['vault_listed_tokens', 544, u64], ['vault_listed_cost', 552, u64], ['vault_fed_bought', 560, u64],
  ['cushion_bottom_bps', 568, u16], ['pol_to_mcap_bps', 570, u16], ['cushion_ticks', 572, i32], ['swept_quote', 576, u64],
]);

/** Every account layout, Carpenter's first (`@carpenter/sdk` `LAYOUTS`), then FLOOR's in the original order. */
export const LAYOUTS: readonly LayoutDef[] = [
  ...CARPENTER_LAYOUTS,
  HOOK_CONFIG, FLOOR_POOL, LAUNCH_CONFIG, REGISTRY_ENTRY, LAUNCH, BUYER_POSITION, QUOTE_POLICY,
  HOLDER_BOOK, HOLDER_RECORD, PLATFORM_TREASURY,

  FLOOR_PLATFORM_POOL,

  TRADER_LINK, TRADER_BUCKET,

  STAIR_STATE,
];

// ---- decoded shapes ---------------------------------------------------------------------------

export interface HookConfig {
  version: number; bump: number; maxRounds: number; callbackAccounts: number; flags: number; authorityBump: number;
  governance: Address; platformDestination: Address; launchProgram: Address; launchAuthority: Address;
  /**
   * ⛔ The `["platform_treasury"]` PDA ({@link findPlatformTreasury}), which `initialize_hook_config` names. A config naming anything else is swept NOTHING (`TreasuryNotMigrated`, 7031).
   */
  hookAuthority: Address; platformOwner: Address;
  /** The candidate `governance` named by `set_hook_governance`; the zero address when none is pending. */
  pendingGovernance: Address;
}
export interface FloorPool {
  /** THE POOL-PRICE TWAP's oracle, the account's 216-byte tail (`decodeFloorPoolTwap`), set by `decodeFloorPool`; `null` on a
   *  792-byte account. Optional: `encodeAccount` and hand-built fixtures never write it. */
  twap?: TwapOracleView | null;
  version: number; bump: number; poolBound: number; quoteIsToken1: number; ladderSeeded: number; needsRatchet: number;
  mode: number; quoteSuspended: number; tickSpacing: number; floorTick: number; ammPool: Address; poolInitializer: Address; baseMint: Address;
  /** The creator-fee recipients and the team-buy payees — the account's `PayeeSlots` tail
   *  ({@link decodePayeeSlots}), set by `decodeFloorPool`; `null` on an account shorter than 1,576 B. Optional: `encodeAccount`
   *  and hand-built fixtures never write it. ⛔ The single `creator` / `pendingRecipientId` fields are GONE (dead padding). */
  payeeSlots?: PayeeSlots | null;
  quoteMint: Address; skimBuyFarBps: number; skimSellFarBps: number;
  creatorBuyBps: number; creatorSellBps: number; holderBuyBps: number; holderSellBps: number;
  /** The wall's offset from the floor in ticks, `wallTicksCenti(k)`. */
  wallTicks: number;
  platformBps: number; minBid: bigint; minSellable: bigint;
  /** @240 / @248: the team steps payee's owed quote and the lifetime
   *  paid. Meaningful only where `stairOwner != 0`. */
  ownerOwed: bigint; ownerPaid: bigint; vaultEth: bigint;
  bidEthDeployed: bigint; unpaidSkim: bigint; cumulativeVolumeEth: bigint; inventoryTokens: bigint;
  inventoryCostEth: bigint; askTokensDeployed: bigint; ladderTokensRemaining: bigint; bidLower: number; askUpper: number;
  askWidth: number; seedLower: number; seedUpper: number; ladderLower: number; ladderUpper: number;
  /** The band's dear edge = the tail's cheap edge. ⛔ Meaningful ONLY while
   *  `wallTailLiq !== 0n` — a real split can be tick 0, so never test this for "is there a tail?". */
  wallSplitTick: number; bidLiquidity: bigint; askLiquidity: bigint; seedLiq: bigint; ladderLiq: bigint;
  creatorFeesOwed: bigint; platformOwed: bigint; holderOwed: bigint;
  /** The tail position's liquidity (salt 101); `0n` = the wall is ONE position. */
  wallTailLiq: bigint;
  /** THE LIST SHARE's HELD tokens: not for sale until the next ratchet, outside circulating supply. The
   *  wall's tokens are `ladderTokensRemaining` (listed) + this. */
  wallHeldTokens: bigint; seedFeesSweptEth: bigint; seedFeesSweptTokens: bigint; ladderProceedsEth: bigint;
  platformPaidEth: bigint; platformSwept: bigint; creatorFeesPaid: bigint; holderPaidEth: bigint; lastRatchetSlot: bigint;
  bidWidth: number; skimBuyAtFloorBps: number; skimSellAtFloorBps: number; maxBounty: bigint;
  bountyBps: number;
  /** THE WALL'S SHAPE, written once by `register_pool`: the band's raw width `wallTicks(W)` (0 = open), the
   *  tail's share of the listed tokens, the list share (⛔ 0 = list everything). */
  wallBandTicks: number; wallTailBps: number; wallListBps: number;
  /**
   * Lamports split off this pool's creator fees and NOT YET MOVED to the distributor — a quote
   * LIABILITY on the pool's claims until `claim_referral_fees` moves it.
   * 0 on an unreferred pool, for ever.
   */
  referralOwed: bigint;
  /**
   * The CUMULATIVE ACCRUED meter: `referralOwed` plus everything `claim_referral_fees` has already
   * moved out. Raised at every split, NEVER by the claim, and it only ever rises. ⚠ Named `_paid`
   * for EVM parity, where the wei really does leave inside the swap; here "paid" == "accrued", and
   * what has actually LEFT the pool is `referralFeesPaid - referralOwed`. 0 on an unreferred pool.
   */
  referralFeesPaid: bigint;
  /**
   * The share of a GROSS creator fee that goes to `referrer`, in bips (1000 = 10 %).
   * ⛔ NOT the gate: it is 0 on an unreferred pool too, but the zero KEY is what `take_fees` and
   * `claim_referral_fees` test; read `referralDestination` for "does this pool fund referrals".
   */
  referralShareBps: number;
  /** @634: the staircase's step count (0 = no staircase), `stairReady` 1 from the graduating buy on, and the step weighting in bps. */
  stairSteps: number;
  stairReady: number;
  stairGrowthBps: number;
  /** The staircase's parameter set — 0 the presets', 1 `$FLOOR`'s. */
  stairKind: number;
  /** 0 none, 1 `$FLOOR`'s team steps, 2 a launch's team steps. */
  stairOwner: number;
  /**
   * Where this pool's share of every creator fee goes, or `null` when it funds none. Derived by
   * {@link decodeFloorPool}; the all-zero key is the absent sentinel.
   * ⚠ `null` does not distinguish "opted out" from "written before referrals existed" -- the
   * bytes are identical and both mean the pool funds nobody and never can (no setter exists).
   */
  referralDestination: Address | null;
  /**
   * ⛔ RAW — the referrer's WALLET, or 32 zero bytes (the System Program's
   * spelling) on an unreferred pool. Read {@link FloorPool.referralDestination}.
   */
  referrer: Address;
  /** 1 iff the base mint carries FLOOR's transfer-hook slot. */
  holdersHooked: number;
  /** ⛔ RAW — tier 2's wallet or 32 zero bytes. Read {@link FloorPool.referralTiers}. */
  referrerT2: Address;
  /** ⛔ RAW — tier 3's wallet or 32 zero bytes. Read {@link FloorPool.referralTiers}. */
  referrerT3: Address;
  /** Lifetime quote paid OUT to tier 2. */
  referralPaidT2: bigint;
  /** Lifetime quote paid OUT to tier 3. */
  referralPaidT3: bigint;
  /** The trader-referral slices booked on this pool and not yet swept into the vault — a
   *  quote liability. */
  traderRefOwed: bigint;
  /** The lifetime trader-referral slices booked on this pool (owed included). */
  traderRefBooked: bigint;
  /**
   * DERIVED by {@link decodeFloorPool}: the three tier wallets `[t1, t2, t3]`,
   * `null` where unset — `t1` is {@link FloorPool.referralDestination}. The chain keeps them pairwise
   * distinct, so a wallet names at most one tier of a pool.
   */
  referralTiers: [Address | null, Address | null, Address | null];
  /**
   * DERIVED by {@link decodeFloorPool}: each tier's due RIGHT NOW, `[due1, due2, due3]` — what
   * `claim_referral_fees(tier)` pays (`referralTierDuesOf`, the port of `floor_math::referral_tier_dues`).
   * ⛔ Show THIS per tier, never `referralOwed` (which overstates tier 1 by up to 10 % on a multi-tier launch).
   */
  referralTierDues: [bigint, bigint, bigint];
  /** FLOOR Platform fees this pool took (and a sale's, handed over at graduation) not yet swept — a quote LIABILITY, never backing. */
  teamOwed: bigint;
  /** The cumulative meter `sweep_platform_cut`'s team leg raises; only ever rises. */
  teamFeesPaid: bigint;
  /** This pool's FLOOR Platform fee rate — the config's pool SETTING at the launch's creation, kept for life; 0 on an older pool, which pays none. Every pool figure uses THIS. */
  teamFeeBps: number;
}
export interface LaunchConfig {
  version: number; bump: number; cluster: number; governance: Address; attestor: Address; hookProgram: Address;
  ammProgram: Address; platformBps: number; tickSpacing: number; minBidLamports: bigint;
  minSellable: bigint; registryCount: bigint; maxBountyLamports: bigint;
  metadataBaseSetSlot: bigint; bountyBps: number; metadataBaseLen: number; metadataBase: Uint8Array;
  /** ⛔ RAW: read {@link launchConfigTeamSaleBps}. Meaningful only when `teamBpsSet` is non-zero. */
  teamSaleBpsRaw: number;
  /** ⛔ RAW: read {@link launchConfigTeamPoolBps}. Meaningful only when `teamBpsSet` is non-zero. */
  teamPoolBpsRaw: number;
  /** 0 = never set (the defaults 100 / 25 apply); non-zero = the two raw rates. */
  teamBpsSet: number;
  /** The two-step handover: the candidate `governance` named by `set_governance`, which it must sign `accept_governance` to become (zeroed there); the zero address when none is pending. */
  pendingGovernance: Address;
}
export interface RegistryEntry { index: bigint; launch: Address; mint: Address; ammPool: Address; mode: number; createdSlot: bigint }
export interface Launch {
  version: number; bump: number; saleVaultBump: number; lockBump: number; state: number; mode: number;
  /** The creator-fee recipients are the POOL's `PayeeSlots` tail ({@link FloorPool.payeeSlots}). */
  mintAuthorityBump: number; mint: Address; creator: Address;
  ammPool: Address; floorPool: Address; tokenVault: Address; wsolVault: Address; priceX64: bigint;
  quoteUsdCents: bigint; mcapUsd: bigint; createdSlot: bigint; nonce: bigint; saleTokens: bigint; polTokens: bigint;
  ladderTokens: bigint; sold: bigint; raised: bigint; buyers: bigint; openTick: number;
  /** The wallet part's cliffs paid PER TEAM SLOT — slot i owns bits 4i..4i+3 (bit k = cliff
   *  k paid to it). Read through {@link teamCliffsPaidForSlot}. */
  teamCliffsPaidBySlot: number;
  /** Trader referrals: the bump of `["trader_ref_sale", launch]`, the launch's sale referral account. */
  traderRefSaleBump: number;
  skimBuyFar: number;
  skimSellFar: number; creatorBuy: number; creatorSell: number; holderBuy: number; holderSell: number;
  registryIndex: bigint; graduatedSlot: bigint;
  /** ⛔ Zeros on a `version < 3` account — read {@link launchQuoteMint}, never this field raw. */
  quoteMint: Address;
  /** This sale's FLOOR Platform fee rate — the config's sale SETTING copied at `create_launch`, kept for the sale's life; 0 on an older launch. Every sale figure uses THIS, never a constant. */
  teamBps: number;
  /** The sale's FLOOR Platform fees not yet handed to its pool (escrowed beside `raised`, never part of it). */
  teamOwed: bigint;
  /** Tokens on team steps (≥ 80 %) and for the deployer's wallet (≤ 20 %). */
  teamStepTokens: bigint; teamWalletTokens: bigint;
  /** The graduation's unix seconds (the wallet's cliffs and the steps' lock count from here); 0 before graduation. */
  teamAnchor: bigint;
  /** The team steps' lock in days (≥ 90) and the wallet's four cliffs in days after `teamAnchor` (25 % each). */
  teamLockDays: number; teamVestDays: number[];
  // The team payees are the pool's `PayeeSlots` tail; the cliffs are per slot (`teamCliffsPaidBySlot`).
  /** LAMPORTS of the staircase budget `create_launch` prepaid into `["grad_rent", launch]`, still to move to the pool's `["stair_rent", amm_pool]`; `fund_stair_rent` moves it after graduation and writes 0. 0 = nothing owed. */
  stairRentBudget: number;
}
export interface BuyerPosition { bump: number; allocation: bigint; contributed: bigint }
export interface QuotePolicy {
  version: number; bump: number; decimals: number; retired: number; quoteMint: Address; tokenProgram: Address;
  minBidFloor: bigint; minBidCeil: bigint; maxBountyCeil: bigint; minBid: bigint; maxBounty: bigint;
}
/** `["holder_book", mint]` under `floor_holders`. */
export interface HolderBook {
  version: number; bump: number; metasBump: number; mint: Address; floorPool: Address; quoteMint: Address;
  acc: bigint; eligible: bigint; indexed: bigint; unallocated: bigint; paid: bigint; records: bigint; createdSlot: bigint;
  /** Holder tax indexed in `lastSlot`, shared at a later slot. */
  pending: bigint; lastSlot: bigint;
}
/** `["holder", mint, token_account]` under `floor_holders`. */
export interface HolderRecord {
  version: number; bump: number; mint: Address; tokenAccount: Address; owner: Address;
  shares: bigint; snapshot: bigint; owed: bigint; paid: bigint; openedSlot: bigint;
}
/**
 * `governance` alone signs `withdraw_platform`; `pendingGovernance` is the
 * key a `set_treasury_governance` proposed, or the zero key — which decodes to
 * `11111111111111111111111111111111` — when nothing is pending.
 */
export interface PlatformTreasury {
  version: number; bump: number;
  /** (R5): 1 = allow-all quotes on (the platform treasury only). */
  allowAllQuotes: number;
  governance: Address; pendingGovernance: Address;
}
/**  `$FLOOR`'s pool record. `received` is lamports credited into its backing, lifetime. */
export interface FloorPlatformPool {
  version: number;
  bump: number;
  floorPool: Address;
  ammPool: Address;
  baseMint: Address;
  quoteMint: Address;
  received: bigint;
  credits: bigint;
  designatedSlot: bigint;
}
/** See {@link STAIR_STATE}. Ticks are numbers, liquidity and amounts bigints. */
export interface StairState {
  version: number; bump: number; steps: number; placed: number; firstOpen: number; tailPlaced: number; growthBps: number;
  openEdge: number; tailLower: number; tailUpper: number; cushionLower: number; cushionUpper: number; relistLower: number;
  relistUpper: number;
  /** ⛔ H1: the re-list's cushion-bought tokens' ORIGINAL cost line, ticks; 0 = none. */
  relistCostTick: number;
  stairTotal: bigint; tailTotal: bigint; collectedTotal: bigint; toPolTotal: bigint; toFloorTotal: bigint;
  cushionQuote: bigint; tailLiq: bigint; cushionLiq: bigint; relistLiq: bigint; stepLower: number[]; stepUpper: number[];
  stepLiq: bigint[]; reserveLower: number; reserveUpper: number; reserveLiq: bigint; vaultListedTokens: bigint;
  vaultListedCost: bigint; vaultFedBought: bigint;
  /** The pool's own numbers; 0 = the presets' (floor_hook's `StairState::params`; `predict.ts` falls back to `CUSHION_TICKS`). */
  cushionBottomBps: number; polToMcapBps: number; cushionTicks: number;
  /** Step money `sweep_stair_fees` collected, for the next ratchet to split. */
  sweptQuote: bigint;
}
/** A trader's link ({@link TRADER_LINK}); an unset tier is the zero key. */
export interface TraderLink {
  trader: Address; tier1: Address; tier2: Address; tier3: Address; bump: number;
}

/** A trader's bucket in one quote ({@link TRADER_BUCKET}); `owed[i]` is tier `i + 1`'s. */
export interface TraderBucket {
  trader: Address; quoteMint: Address; tier1: Address; tier2: Address; tier3: Address; owed: bigint[]; bump: number;
}

/**
 * The mint a launch is quoted in — the TS twin of
 * `floor_types::layout::Launch::quote_mint`, and the same branch: offset 400 is zeros on every
 * account written before `version` 3, and those launches are wSOL-quoted because nothing else could
 * be created then. Every builder that needs the quote (`buildBuy`, `buildSellBack`,
 * `graduationAccounts`) is handed it through this, never the raw field.
 */
export function launchQuoteMint(l: Pick<Launch, 'version' | 'quoteMint'>): Address {
  return l.version >= 3 ? l.quoteMint : WSOL_MINT;
}

export const decodeHookConfig = (data: Uint8Array) => decodeAccount<HookConfig>(HOOK_CONFIG, data);
/**
 * ⛔ The all-zero key is ABSENT, never a destination. `decodeAccount`
 * turns 32 zero bytes into `11111111111111111111111111111111` like any other key -- the assertion
 * in `test/layouts.test.ts` that a zero `creator` "decodes as the default address" is that same
 * behaviour, written down -- so an unreferred pool would otherwise report the System Program as
 * its referral distributor. `referralDestination` is the only honest read.
 */
export const decodeFloorPool = (data: Uint8Array): FloorPool => {
  const p = decodeAccount<FloorPool>(FLOOR_POOL, data);
  return {
    ...p,
    // the oracle in the same account's tail, carried
    // so a ratchet builder names the cushion's target without a second read (`twap.ts ratchetSettledTicks`); null without one.
    twap: decodeFloorPoolTwap(data),
    // the creator / team slots in the account's tail at 1,008; null without one.
    payeeSlots: decodePayeeSlots(data),
    referralDestination: referralDestinationOf(p),
    referralTiers: [referralDestinationOf(p), p.referrerT2 !== ZERO_KEY ? p.referrerT2 : null, p.referrerT3 !== ZERO_KEY ? p.referrerT3 : null],
    referralTierDues: referralTierDuesOf(p),
  };
};

/**
 *  each tier's due, `[due1, due2, due3]` — a BYTE-FOR-BYTE
 * port of `floor_math::referral_tier_dues` (`crates/floor-math/src/referral.rs`), the ONE spelling the
 * program's `claim_referral_fees` runs; `test/referral-tiers.test.ts` pins the two with the same vectors.
 *
 * ```text
 * L = referralFeesPaid // Solana: the lifetime BOOKED meter (owed included)
 * up2 = t2 set ? L / 10: 0; up3 = t2 && t3 set ? up2 / 10: 0
 * earned = [L − up2, up2 − up3, up3] // exact: Σ == L; monotone in L
 * paid1 = L − owed − paidT2 − paidT3 // DERIVED, never stored
 * due_i = max(earned_i − paid_i, 0), each ≤ owed
 * ```
 * ⛔ Computed from the RUNNING TOTAL, never by splitting each accrual event (the sum of floors is not the
 *    floor of the sum). An inconsistent pool (owed + paid2 + paid3 > L) is due nothing, as on chain.
 */
export function referralTierDuesOf(
  p: Pick<FloorPool, 'referralFeesPaid' | 'referralOwed' | 'referralPaidT2' | 'referralPaidT3' | 'referrerT2' | 'referrerT3'>,
): [bigint, bigint, bigint] {
  const L = p.referralFeesPaid;
  const owed = p.referralOwed;
  const taken = owed + p.referralPaidT2 + p.referralPaidT3;
  if (taken > L) return [0n, 0n, 0n];
  const paid = [L - taken, p.referralPaidT2, p.referralPaidT3];
  const t2 = p.referrerT2 !== ZERO_KEY;
  const t3 = t2 && p.referrerT3 !== ZERO_KEY;
  const up2 = t2 ? L / 10n : 0n;
  const up3 = t3 ? up2 / 10n : 0n;
  const earned = [L - up2, up2 - up3, up3];
  const due = (i: number): bigint => {
    const d = earned[i]! - paid[i]!;
    if (d <= 0n) return 0n;
    return d < owed ? d : owed;
  };
  return [due(0), due(1), due(2)];
}

/**
 * The ONE derived field reads the
 * pool's `referrer` WALLET — exactly as
 * `floor_types::layout::FloorPool::referral_destination` does (the zero key is the gate `take_fees`
 * tests). What it MEANS is unchanged ("where this pool's referral share goes, or `null` when it funds
 * none"); what it NAMES moved from a distributor to a wallet, whose quote ATA `claim_referral_fees` pays.
 */
export function referralDestinationOf(p: Pick<FloorPool, 'referrer'>): Address | null {
  return p.referrer !== ZERO_KEY ? p.referrer : null;
}

/**
 * The transfer-hook program a pool (or an approved hook) ADMITS armed on its base mint, or `null` (the
 * stored zero key). ⛔ Read this, never `baseTransferHook` raw: 32 zero bytes decode to the System Program.
 */
export function admittedBaseTransferHook(p: { baseTransferHook: Address }): Address | null {
  return p.baseTransferHook === ZERO_KEY ? null : p.baseTransferHook;
}
export const decodeLaunchConfig = (data: Uint8Array) => decodeAccount<LaunchConfig>(LAUNCH_CONFIG, data);

/**
 * The TS twin of `floor_types::layout::team_rate_setting`, the ONE
 * reading of `LaunchConfig`'s platform-fee setting: `set == 0` is the default; otherwise the raw rate,
 * never above `cap`. `test/layouts.test.ts` holds it to the Rust function's cases.
 */
export function teamRateSetting(set: number, raw: number, dflt: number, cap: number): number {
  if (set === 0) return dflt;
  return raw > cap ? cap : raw;
}
/**
 * The FLOOR Platform fee a sale created NOW charges on every buy and sell-back (what `create_launch` copies into
 * `Launch.teamBps`): the default 100 until `set_team_fees` runs. ⛔ A RUNNING sale charges its own `teamBps`,
 * never this.
 */
export function launchConfigTeamSaleBps(c: Pick<LaunchConfig, 'teamBpsSet' | 'teamSaleBpsRaw'>): number {
  return teamRateSetting(c.teamBpsSet, c.teamSaleBpsRaw, TEAM_SALE_FEE_BPS_DEFAULT, TEAM_SALE_FEE_BPS_MAX);
}
/**
 * The FLOOR Platform fee a pool created NOW charges on every swap after graduation (what `create_launch` hands
 * `register_pool`): the default 25 until `set_team_fees` runs. ⛔ An EXISTING pool charges its own `teamFeeBps`.
 */
export function launchConfigTeamPoolBps(c: Pick<LaunchConfig, 'teamBpsSet' | 'teamPoolBpsRaw'>): number {
  return teamRateSetting(c.teamBpsSet, c.teamPoolBpsRaw, TEAM_POOL_FEE_BPS_DEFAULT, TEAM_POOL_FEE_BPS_MAX);
}
export const decodeRegistryEntry = (data: Uint8Array) => decodeAccount<RegistryEntry>(REGISTRY_ENTRY, data);
export const decodeLaunch = (data: Uint8Array) => decodeAccount<Launch>(LAUNCH, data);
export const decodeQuotePolicy = (data: Uint8Array) => decodeAccount<QuotePolicy>(QUOTE_POLICY, data);
export const decodePlatformTreasury = (data: Uint8Array) => decodeAccount<PlatformTreasury>(PLATFORM_TREASURY, data);
export const decodeFloorPlatformPool = (data: Uint8Array) => decodeAccount<FloorPlatformPool>(FLOOR_PLATFORM_POOL, data);
export const decodeBuyerPosition = (data: Uint8Array) => decodeAccount<BuyerPosition>(BUYER_POSITION, data);
/**  `["floor_platform_pool"]` (exactly 232 B). */
/** `["trader_link", trader]` (exactly 144 B). */
export const decodeTraderLink = (data: Uint8Array) => decodeAccount<TraderLink>(TRADER_LINK, data);
/** `["trader_ref", trader, quote_mint]` (exactly 200 B). */
export const decodeTraderBucket = (data: Uint8Array) => decodeAccount<TraderBucket>(TRADER_BUCKET, data);
/** An owner pool's `StairState` account carries the owner books as a 440-byte
 *  TAIL; the struct itself is unchanged, so both lengths decode. */
export const decodeStairState = (data: Uint8Array) =>
  decodeAccount<StairState>(STAIR_STATE, data.length === STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS ? data.subarray(0, 8 + STAIR_STATE.size) : data);

/** The owner books' ACCOUNT offset in a `StairState` account: 8 + 584. */
export const STAIR_OWNERS_OFFSET = 592;
/** `size_of::<floor_types::owners::StairOwners>`. */
export const STAIR_OWNERS_SIZE = 440;
/** A `StairState` account carrying the owner tail: 592 + 440. */
export const STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS = STAIR_OWNERS_OFFSET + STAIR_OWNERS_SIZE;

/**
 * `floor_types::owners::StairOwners`, the owner books of a
 * staircase pool whose `FloorPool.stairOwner != 0` — `$FLOOR`'s TEAM steps (kind 1) or a launch's TEAM-BUY steps (kind 2).
 * Offsets are within the tail.
 */
export const STAIR_OWNERS_FIELDS: readonly (readonly [string, number, FieldKind])[] = [
  ['version', 0, u8], ['kind', 1, u8], ['counted', 2, u8], ['first_step', 3, u8], ['payee_bps', 4, u16], ['pad0', 6, bytes(2)],
  // `payee` @24 / `payee_pending_id` @56 are DEAD padding — the books pay the pool's TEAM slots
  // (the `FloorPool` account's `PayeeSlots` tail), each its share.
  ['first_unlock_secs', 8, u32], ['every_secs', 12, u32], ['anchor', 16, i64], ['pad1', 24, bytes(32)],
  ['pad2', 56, bytes(32)], ['owner_total', 88, u64], ['owner_sold', 96, u64], ['owner_withdrawn', 104, u64],
  ['owner_proceeds', 112, u64], ['owed_total', 120, u64], ['forfeited', 128, u64], ['fees_to_floor', 136, u64],
  ['tips_paid', 144, u64], ['tokens', 152, { t: 'u64s', n: 16 }], ['cut', 280, { t: 'u64s', n: 16 }],
  ['last_tip_at', 408, i64],
  // `held` — the team's HELD tokens — `reserve_fed`, `clamped`.
  ['held', 416, u64], ['reserve_fed', 424, u64], ['clamped', 432, u64],
];
export interface StairOwners {
  /** 0 while only sized (before graduation), 1 once graduation filled it. */
  version: number;
  /** 1 `$FLOOR`'s team, 2 a launch's deployer. */
  kind: number;
  /** 1: the owner's standing tokens count against the floor — withdrawal is free. */
  counted: number;
  /** The lowest step holding owner tokens at graduation — the first to unlock. */
  firstStep: number;
  /** The payee's share of what the owner's tokens sell for, after FLOOR FIRST (team 4,000; deployer 8,000). */
  payeeBps: number;
  /** Seconds after `anchor` the first step unlocks; then one more step every `everySecs` (0 = every step at once). */
  firstUnlockSecs: number; everySecs: number; anchor: bigint;
  // The payees are the pool's TEAM slots.
  ownerTotal: bigint; ownerSold: bigint; ownerWithdrawn: bigint; ownerProceeds: bigint; owedTotal: bigint; forfeited: bigint;
  /** Lifetime keepers' tips out of `sweep_stair_payee`. */
  feesToFloor: bigint; tipsPaid: bigint;
  /** Owner tokens still standing on step i, and what withdrawals cut out of steps riding the reserve. */
  tokens: bigint[]; cut: bigint[];
  /** Unix seconds of the last `sweep_stair_payee` that PAID a tip; 0 = never. */
  lastTipAt: bigint;
  /** ⛔ The team's HELD tokens: owner tokens whose step
   *  left the staircase unsold, set aside for the payee — counted against the floor, paid by `withdraw_stair_tokens(STAIR_HELD_STEP)`
   *  ({@link buildWithdrawHeldTokens}). */
  held: bigint;
  /** The protocol's (vault-fed) tokens riding the reserve; a close sells them first. */
  reserveFed: bigint;
  /** Lifetime: owner tokens dropped as placement rounding (never paid, never sold). */
  clamped: bigint;
}
/** The owner books of a `StairState` ACCOUNT's bytes, or `null` when the account carries none (shorter than 1,032). */
export function decodeStairOwners(account: Uint8Array): StairOwners | null {
  if (account.length < STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS) return null;
  const out: Record<string, unknown> = {};
  for (const [name, off, kind] of STAIR_OWNERS_FIELDS) {
    if (HIDDEN.test(name)) continue;
    out[snakeToCamel(name)] = readField(account, STAIR_OWNERS_OFFSET + off, kind);
  }
  return out as unknown as StairOwners;
}
export const decodeHolderBook = (data: Uint8Array) => decodeAccount<HolderBook>(HOLDER_BOOK, data);
export const decodeHolderRecord = (data: Uint8Array) => decodeAccount<HolderRecord>(HOLDER_RECORD, data);

// ---- THE POOL-PRICE TWAP's tail ----------------
// `floor_types::twap::TwapOracle`, 216 B at ACCOUNT offset 792 (straight after the discriminator + the 784-byte
// struct) of every `FloorPool`. ⛔ APPEND-ONLY, like the struct.

export const TWAP_ORACLE_OFFSET = 792;
export const TWAP_ORACLE_SIZE = 216;
export const FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP = TWAP_ORACLE_OFFSET + TWAP_ORACLE_SIZE; // 1,008
export const TWAP_CHECKPOINTS = 8;
export const TWAP_CHECKPOINT_SPACING_SECS = 300;
/** `TwapOracle::older_surplus_held`'s offset inside the tail (`floor_types::twap` pins 152). */
export const TWAP_OLDER_SURPLUS_HELD_OFFSET = 152;
/**
 * THE GAP BUY SKIM's cache:
 * `TwapOracle::cushion_top_set` (u8) at tail byte 153 and `cushion_top_tick` (i32) at 156 — the cushion's top in PRICE
 * (its LOWER tick) as the last ratchet placed it. The wasm quoter reads it off the same account bytes itself.
 */
export const TWAP_CUSHION_TOP_SET_OFFSET = 153;
export const TWAP_CUSHION_TOP_TICK_OFFSET = 156;
/** `TwapOracle::flow_a` (i128 LE) at tail byte 160 and
 *  `flow_b` (u64 LE) at 176, written by `after_swap` while the switch is ON (`STAIR_CUSHION_NETFLOW_BPS`). */
export const TWAP_NETFLOW_A_OFFSET = 160;
export const TWAP_NETFLOW_B_OFFSET = 176;

export interface TwapCheckpointView {
  readonly tickCumulative: bigint;
  readonly timestamp: number;
}

export interface TwapOracleView {
  /** 0 = never written; 1 after the first write. */
  readonly version: number;
  /** Ring index of the newest checkpoint. */
  readonly newest: number;
  /** Checkpoints written so far, at most 8. */
  readonly count: number;
  /** The current observation: Σ tick × seconds up to `timestamp`, and the tick that write saw. */
  readonly current: { readonly tickCumulative: bigint; readonly timestamp: number; readonly lastTick: number };
  /** The ring, in slot order (index `newest` is the newest; unwritten slots are zero). */
  readonly checkpoints: readonly TwapCheckpointView[];
  /**
   * 1 when the last ratchet held older surplus back at the settled stand line.
   */
  readonly olderSurplusHeld: number;
  /** 1 when the last ratchet left a cushion standing. */
  readonly cushionTopSet?: number;
  /** Its top in price (the cushion's LOWER tick), byte 156; read only while `cushionTopSet` is 1. */
  readonly cushionTopTick?: number;
  /** The net-flow meter, bytes 160..184: `a` = Σ quote × tick, `b` = Σ quote. */
  readonly netFlow?: { readonly a: bigint; readonly b: bigint };
}

/**
 * The TWAP tail of a `FloorPool` ACCOUNT's bytes (the whole account, discriminator included), or `null` for a
 * pool without one. Reads only; the window
 * arithmetic is the program's (`twap_tick`, simulated), never re-derived here.
 */
export function decodeFloorPoolTwap(account: Uint8Array): TwapOracleView | null {
  if (account.length < FLOOR_POOL_ACCOUNT_LEN_WITH_TWAP) return null;
  const v = new DataView(account.buffer, account.byteOffset + TWAP_ORACLE_OFFSET, TWAP_ORACLE_SIZE);
  const checkpoints: TwapCheckpointView[] = [];
  for (let i = 0; i < TWAP_CHECKPOINTS; i++) {
    const at = 24 + 16 * i;
    checkpoints.push({ tickCumulative: v.getBigInt64(at, true), timestamp: v.getUint32(at + 8, true) });
  }
  return {
    version: v.getUint8(0),
    newest: v.getUint8(1),
    count: v.getUint8(2),
    current: { tickCumulative: v.getBigInt64(8, true), timestamp: v.getUint32(16, true), lastTick: v.getInt32(20, true) },
    checkpoints,
    olderSurplusHeld: v.getUint8(TWAP_OLDER_SURPLUS_HELD_OFFSET),
    cushionTopSet: v.getUint8(TWAP_CUSHION_TOP_SET_OFFSET),
    cushionTopTick: v.getInt32(TWAP_CUSHION_TOP_TICK_OFFSET, true),
    netFlow: {
      a: BigInt.asIntN(128, (v.getBigUint64(TWAP_NETFLOW_A_OFFSET + 8, true) << 64n) | v.getBigUint64(TWAP_NETFLOW_A_OFFSET, true)),
      b: v.getBigUint64(TWAP_NETFLOW_B_OFFSET, true),
    },
  };
}
