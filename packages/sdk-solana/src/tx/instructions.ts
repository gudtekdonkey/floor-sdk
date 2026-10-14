/**
 * Raw encoders for FLOOR's programs' instructions a trade needs: `discriminator ‖ borsh(args)` and the account list in
 * the program's `#[derive(Accounts)]` order, on `@floorlaunchpad/carpenter-sdk`'s encoder machinery. `DISC` here is the union (`amm` is
 * Carpenter's table) and `FloorInstruction` is Carpenter's `ProgramInstruction`.
 *
 * ⛔ Account order is the wire format. Each instruction also carries `accountNames`, the Rust field names in order;
 *    `test/idl-parity.test.ts` requires them to equal the shipped IDL's account list.
 * ⛔ `#[event_cpi]` appends `event_authority` then `program` AFTER the named accounts and BEFORE the remaining accounts.
 * ⛔ Anchor `Option<Account>`: `None` is the executing PROGRAM's own id in that slot, read-only.
 */

import {  type AccountMeta, type Address,  } from '@solana/kit';

import {
  DISC as CARPENTER_DISC,
  makeInstruction,

  remainingRo,
  remainingRw,
  ro,

  rw,
  ws,
  type ProgramInstruction,
  type Slot,
  BorshWriter,
  SYSTEM_PROGRAM_ID,
} from '@floorlaunchpad/carpenter-sdk';

/** Every encoder's return shape — `@floorlaunchpad/carpenter-sdk`'s `ProgramInstruction`. */
export type FloorInstruction = ProgramInstruction;

const d = (...b: number[]): Uint8Array => new Uint8Array(b);

/** Every instruction's Anchor discriminator, `sha256("global:<name>")[..8]`, per program (`amm` is `@floorlaunchpad/carpenter-sdk`'s). */
export const DISC = {
  amm: CARPENTER_DISC.amm,
  hook: {
    seed_floor: d(0x65, 0x7e, 0xc0, 0xc3, 0xf8, 0x09, 0x8e, 0x4a),
    ratchet: d(0xdd, 0xd6, 0xcd, 0x6e, 0x0c, 0xaa, 0x18, 0xad),
    reclaim_stair_array: d(0x1f, 0x29, 0x02, 0xa6, 0x5f, 0x63, 0xa2, 0xce),
    sweep_stair_fees: d(0x26, 0x17, 0xbc, 0xc2, 0x2f, 0x25, 0x71, 0xf8),
    sweep_stair_payee: d(0xa3, 0x0b, 0x7e, 0x2b, 0x8c, 0xad, 0xd9, 0x50),
    withdraw_stair_tokens: d(0x8a, 0xd5, 0x50, 0xd0, 0xc1, 0x62, 0xca, 0x29),
    claim_creator_fees: d(0x00, 0x17, 0x7d, 0xea, 0x9c, 0x76, 0x86, 0x59),
    sweep_platform_cut: d(0x03, 0xdf, 0x23, 0xc9, 0x4a, 0xf3, 0x13, 0x84),
    claim_referral_fees: d(0xd0, 0xd8, 0x89, 0x4e, 0x24, 0x67, 0xa2, 0x31),
    observe_quote: d(0x47, 0xf1, 0x56, 0x74, 0xbc, 0x47, 0x86, 0x1e),
    open_trader_bucket: d(0x2a, 0x6c, 0xb2, 0xe5, 0x6e, 0x99, 0x12, 0xa9),
    open_trader_ref_vault: d(0xf3, 0x9b, 0x65, 0xd2, 0x74, 0x3c, 0xfb, 0x2a),
    sweep_trader_referral: d(0x33, 0x70, 0x18, 0xfd, 0xc4, 0x7a, 0xa4, 0xa7),
    claim_trader_referral: d(0x5e, 0x42, 0x25, 0xf9, 0x05, 0xb7, 0xb6, 0x9f),
    twap_tick: d(0x6c, 0x53, 0xf6, 0x14, 0x0e, 0x51, 0xf1, 0xbd),
    designate_floor_pool: d(0x8a, 0x52, 0x72, 0xb7, 0x44, 0xae, 0xa5, 0xfc),
    credit_floor_pool: d(0xa1, 0x3a, 0x0a, 0x82, 0x10, 0x74, 0xc9, 0xac),
    set_genesis_stair: d(0xc2, 0x07, 0xdb, 0x84, 0xf5, 0x65, 0xfd, 0x97),
    list_quote: d(0x71, 0x05, 0xb8, 0xc4, 0xcc, 0xb9, 0x69, 0x89),
    set_quote_economics: d(0x54, 0x54, 0x8d, 0x84, 0x7b, 0xb4, 0x20, 0xcf),
    retire_quote: d(0xae, 0x9e, 0xa8, 0x70, 0x8e, 0xdc, 0x60, 0x64),
    set_allow_all_quotes: d(0x66, 0x7f, 0x80, 0x48, 0x18, 0xb0, 0xd2, 0x21),
    withdraw_platform: d(0x57, 0xed, 0x1f, 0xd4, 0xf1, 0xac, 0x63, 0x21),
    set_treasury_governance: d(0x8e, 0x90, 0xa1, 0xe6, 0x6b, 0x0d, 0x86, 0x32),
    accept_treasury_governance: d(0x6c, 0xf8, 0xa8, 0x36, 0x28, 0xfb, 0x99, 0xe6),
    initialize_team_treasury: d(0xb5, 0x26, 0x6b, 0x95, 0xac, 0x7b, 0xf6, 0xa1),
    withdraw_team: d(0x84, 0x9c, 0x8a, 0xfa, 0x20, 0x0f, 0x65, 0x23),
    set_team_treasury_governance: d(0x04, 0x39, 0x12, 0x1d, 0x42, 0x31, 0xec, 0xc4),
    accept_team_treasury_governance: d(0x33, 0x41, 0xaf, 0xb5, 0xb8, 0x3b, 0xd3, 0xa0),
    set_hook_governance: d(0x24, 0xb4, 0xe0, 0xc0, 0x8c, 0xde, 0x3e, 0x29),
    accept_hook_governance: d(0x44, 0x27, 0x63, 0x6a, 0x6d, 0xa6, 0x43, 0x90),
    donate_inventory: d(0x01, 0xa3, 0xb6, 0x51, 0x71, 0x01, 0xb5, 0x34),
  },
  launch: {
    create_launch: d(0xef, 0xdf, 0xff, 0x86, 0x27, 0x79, 0x7f, 0x3e),
    buy: d(0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea),
    sell_back: d(0xe6, 0x32, 0x08, 0x06, 0xd2, 0x0f, 0x98, 0x85),
    sell_back_wsol: d(0x8c, 0xc1, 0x20, 0x8a, 0x4c, 0x2d, 0xdc, 0xbe),
    close_buyer_position: d(0xef, 0x79, 0xdd, 0xe1, 0xcc, 0x60, 0xab, 0xd4),
    claim_recipient: d(0xdb, 0xb5, 0xf3, 0xf6, 0x36, 0x55, 0x39, 0xc3),
    graduate: d(0x2d, 0xeb, 0xe1, 0xb5, 0x11, 0xda, 0x40, 0x82),
    close_graduation_rent: d(0x01, 0x5a, 0x0c, 0x7b, 0x94, 0xdf, 0x84, 0x3f),
    fund_stair_rent: d(0x9f, 0x87, 0xc9, 0xd7, 0x7a, 0xc4, 0xd9, 0x34),
    close_prepared_tick_array: d(0x73, 0x65, 0xd7, 0x5b, 0x35, 0x3e, 0x00, 0x24),
    prepare_graduation: d(0x45, 0xbc, 0xed, 0x6d, 0x58, 0xda, 0x9f, 0x73),
    claim_team_vested: d(0xe4, 0x1b, 0xf0, 0x7e, 0x26, 0x49, 0x0e, 0x67),
    team_buy_sell_back: d(0x09, 0xf1, 0x5b, 0x33, 0xd1, 0x93, 0x28, 0x3d),
    claim_team_payee: d(0x6d, 0xeb, 0x4b, 0x84, 0x35, 0x3f, 0x7e, 0x37),
    claim_genesis_payee: d(0xb0, 0x04, 0x57, 0xe7, 0xa1, 0xd8, 0x58, 0xaa),
    set_team_fees: d(0xb4, 0xf9, 0x40, 0x08, 0xc3, 0x06, 0xde, 0xb3),
    init_genesis: d(0x35, 0x99, 0x1b, 0x88, 0x26, 0xa7, 0x49, 0x19),
    set_genesis_schedule: d(0x21, 0x03, 0xc1, 0xb8, 0x97, 0xe4, 0xe2, 0xbd),
    set_genesis_floor_share: d(0x8c, 0x8c, 0x5f, 0xdc, 0x04, 0x98, 0x13, 0xc4),
    set_genesis_stair_top: d(0xe7, 0x25, 0x81, 0x07, 0xdb, 0x13, 0x6a, 0xf5),
    set_genesis_whitelist: d(0xab, 0xba, 0x7f, 0xa2, 0x9f, 0xd8, 0x80, 0xba),
    transfer_genesis_authority: d(0x91, 0xa7, 0xa0, 0x3f, 0x42, 0x6c, 0xeb, 0x5e),
    accept_genesis_authority: d(0xcc, 0xa0, 0x75, 0xfe, 0x7b, 0xf1, 0xec, 0xb4),
    contribute: d(0x52, 0x21, 0x44, 0x83, 0x20, 0x00, 0xcd, 0x5f),
    end_genesis_sale: d(0x23, 0x45, 0xe6, 0x3f, 0xd1, 0xc7, 0xf2, 0xc9),
    graduate_genesis: d(0x1a, 0x9b, 0x54, 0xf9, 0x5f, 0xbd, 0x49, 0xd8),
    lock_genesis: d(0x24, 0x89, 0x4e, 0xae, 0x8e, 0x29, 0xf6, 0xc4),
    claim_genesis: d(0x28, 0x51, 0xb2, 0x63, 0xbe, 0x9a, 0xd0, 0xfe),
    refund_genesis: d(0xaf, 0xfb, 0xb1, 0x91, 0xdc, 0x0b, 0xde, 0x43),
    walk_quote_chain: d(0x94, 0x30, 0xf0, 0x33, 0xeb, 0x2b, 0x52, 0x12),
    close_quote_walk: d(0x90, 0xda, 0xe2, 0xa5, 0x86, 0xc4, 0x75, 0x73),
    link_trader: d(0xfb, 0x3f, 0x3d, 0x38, 0xa7, 0xc7, 0x3a, 0xc1),
    sweep_sale_trader_referral: d(0x42, 0x23, 0x40, 0x83, 0x0f, 0x21, 0x8a, 0x5c),
    open_sale_trader_referral: d(0x40, 0xb4, 0x6b, 0x37, 0xe9, 0x6c, 0x6a, 0x4b),
    flush_floor_fees: d(0x99, 0xb8, 0xcb, 0x3a, 0x7c, 0x36, 0xa0, 0x23),
    flush_allocation_dust: d(0x3d, 0xb9, 0xe5, 0x2e, 0x93, 0xbc, 0x95, 0x20),
    init_preset_registry: d(0x41, 0x17, 0xf7, 0x6f, 0x72, 0x31, 0x86, 0x6b),
    add_preset: d(0x97, 0x01, 0x51, 0xbb, 0x50, 0x24, 0x04, 0xcb),
    set_preset_enabled: d(0xa7, 0x7a, 0xee, 0xb9, 0x5c, 0x3b, 0x55, 0x68),
    set_preset_authority: d(0x55, 0xe4, 0x47, 0xf0, 0xe2, 0xc0, 0x8f, 0x76),
    accept_preset_authority: d(0x66, 0xfe, 0xbc, 0x55, 0x1e, 0xbd, 0xc0, 0xcf),
  },
  /**
   * `floor_holders`. Anchor
   * `sha256("global:<name>")[..8]`; `rust-parity.test.ts` holds each to the hash (the crate names no
   * instruction discriminator for this program) and `idl-parity.test.ts` to the built IDL.
   * ⛔ `execute` is NOT here: it is the transfer-hook INTERFACE's
   *    `sha256("spl-transfer-hook-interface:execute")[..8]` and Token-2022 alone sends it.
   */
  holders: {
    initialize_claims_registry: d(0x64, 0x95, 0x78, 0x79, 0xae, 0xc4, 0x1f, 0x1f),
    rotate_claims_table: d(0x4e, 0xfe, 0x77, 0xce, 0x26, 0x15, 0x3f, 0xde),
    open_record: d(0x09, 0xf5, 0xf4, 0x74, 0x6e, 0xd8, 0x36, 0x9e),
    touch: d(0x50, 0x9d, 0x6a, 0xdc, 0xb5, 0x48, 0x65, 0x51),
    sync: d(0x04, 0xdb, 0x28, 0xa4, 0x15, 0x9d, 0xbd, 0x58),
    claim: d(0x3e, 0xc6, 0xd6, 0xc1, 0xd5, 0x9f, 0x6c, 0xd2),
    close_record: d(0x6f, 0xc0, 0x7a, 0xbc, 0x26, 0xea, 0xf2, 0xf9),
  },
  /**
   * `floor_vesting`. Anchor `sha256("global:<name>")[..8]`, held as `holders` is.
   */
  vesting: {
    create_schedule: d(0xc8, 0xb0, 0xd5, 0xd6, 0xd2, 0x79, 0x23, 0xe1),
    claim: d(0x3e, 0xc6, 0xd6, 0xc1, 0xd5, 0x9f, 0x6c, 0xd2),
    close_schedule: d(0x3d, 0xcf, 0xa8, 0x8b, 0x6a, 0xac, 0xe1, 0x0c),
    create_schedule_at: d(0xa2, 0x16, 0xe3, 0x6b, 0x4a, 0x90, 0x07, 0x05),
  },
} as const;

export interface SaleAccounts {
  program: Address;
  eventAuthority: Address;
  buyer: Address;
  launch: Address;
  saleVault: Address;
  tokenVault: Address;
  buyerPosition: Address;
  buyerAta: Address;
  mint: Address;
  lockAuthority: Address;
  token2022: Address;
}

/**
 * `graduationAccounts` are ALWAYS passed, because finality is decided at execution.
 * ⛔ On a launch quoted in something other than wSOL the
 * buyer's QUOTE ATA is the FIRST remaining account, in front of the graduation list — the program
 * reads the quote mint, its token program and the escrow from the list's own slots 10 / 11 / 27.
 * `buyerQuoteAta` is omitted on a wSOL launch and the wire is unchanged.
 */
export function buyIx(
  a: SaleAccounts & { ataProgram: Address; graduationAccounts: readonly AccountMeta[]; buyerQuoteAta?: Address; saleRef?: SaleRefPrefix },
  args: { lamportsIn: bigint; minTokens: bigint; maxGraduationLamports: bigint },
): FloorInstruction {
  const prefix = saleRefMetas(a.saleRef);
  const rest = a.buyerQuoteAta ? [remainingRw(a.buyerQuoteAta), ...a.graduationAccounts] : a.graduationAccounts;
  return makeInstruction(
    'buy',
    a.program,
    DISC.launch.buy,
    new BorshWriter().u64(args.lamportsIn).u64(args.minTokens).u64(args.maxGraduationLamports),
    [
      ws('buyer', a.buyer),
      rw('launch', a.launch),
      rw('sale_vault', a.saleVault),
      rw('token_vault', a.tokenVault),
      rw('buyer_position', a.buyerPosition),
      rw('buyer_ata', a.buyerAta),
      rw('mint', a.mint),
      ro('lock_authority', a.lockAuthority),
      ro('ata_program', a.ataProgram),
      ro('token_2022', a.token2022),
      ro('system_program', SYSTEM_PROGRAM_ID),
    ],
    {
      eventAuthority: a.eventAuthority,
      remaining: [...prefix, ...rest],
      argNames: ['lamports_in', 'min_tokens', 'max_graduation_lamports'],
    },
  );
}

/**
 * `prepare_graduation`: make the AMM pool and plant the accounts would
 * otherwise create inside the final buy, whose instruction trace does not fit Agave's 64-entry cap
 * without them.
 *
 * ⛔ **THE REMAINING ACCOUNTS ARE 's LIST** — byte for byte what `buildBuy` passes, from the
 * same `graduationAccounts`. That is the design: a client that can build a graduating buy can build
 * this with no new derivation, and the two cannot disagree about which accounts a graduation needs.
 * ⛔ Permissionless and idempotent: `payer` is whoever presses it, and is reimbursed out of the
 * launch's `["grad_rent", launch]` reserve, so it costs a transaction fee and nothing else.
 */
export function prepareGraduationIx(
  a: {
    program: Address;
    payer: Address;
    launch: Address;
    mint: Address;
    token2022: Address;
    eventAuthority: Address;
    graduationAccounts: readonly AccountMeta[];
  },
): FloorInstruction {
  return makeInstruction(
    'prepare_graduation',
    a.program,
    DISC.launch.prepare_graduation,
    null,
    [
      ws('payer', a.payer),
      ro('launch', a.launch),
      ro('mint', a.mint),
      ro('token_2022', a.token2022),
      ro('system_program', SYSTEM_PROGRAM_ID),
    ],
    { eventAuthority: a.eventAuthority, remaining: a.graduationAccounts },
  );
}

/**
 * The four REMAINING accounts of `sell_back`'s quote leg
 * on a launch quoted in something other than wSOL — in this order, each pinned by the program to
 * the launch's stored keys. None on a wSOL launch.
 */
export interface SellBackQuoteLeg {
  quoteMint: Address;
  quoteTokenProgram: Address;
  /** The launch's quote ATA (`Launch.wsol_vault`), the escrow the payout leaves. */
  quoteVault: Address;
  buyerQuoteAta: Address;
}

/**
 * ⛔ the 4-account HOLDER
 * TAIL of `sell_back` on a HOOKED launch, AFTER the quote leg (0 or 4): `floor_holders`, the launch's
 * `holder_book` (w), its `floor_pool` (ro) and the seller's `["holder", mint, buyer_ata]` record (w).
 * The program CPIs `floor_holders::sync` for the record when it exists (the hook is not armed before
 * graduation, so a sell-back is the one thing that can lower a balance then) and skips it when it
 * does not. ⛔ REQUIRED on a hooked launch — a wrong count or address is `HolderRecordMismatch`; ABSENT
 * on an unhooked one (the counts are unchanged there).
 */
export interface SellBackHolderTail {
  floorHolders: Address;
  holderBook: Address;
  floorPool: Address;
  holderRecord: Address;
}

/**
 * The OPTIONAL 5-account
 * prefix at the FRONT of `buy` / `sell_back`'s remaining accounts — `[trader_bucket w, sale_account w, launch_authority,
 * hook_config, floor_hook]`. The program recognises it by its shape (slots 3–5 are `launch_authority`, `hook_config` and
 * `floor_hook`) or by a `floor_hook`-owned `TraderBucket` first. The trader's bucket not opened yet (at its own PDA), or the
 * launch's sale account not opened yet or frozen, books nothing and the trade proceeds. Present but wrong (another
 * trader's or quote's bucket, another launch's sale account) is `TraderRefMismatch` (6095), never ignored.
 */
export interface SaleRefPrefix {
  traderBucket: Address;
  saleAccount: Address;
  launchAuthority: Address;
  hookConfig: Address;
  floorHook: Address;
}
/** The prefix's metas, in the program's order (`SALE_REF_ACCOUNTS` = 5); empty when absent. */
export const saleRefMetas = (p?: SaleRefPrefix): AccountMeta[] =>
  p ? [remainingRw(p.traderBucket), remainingRw(p.saleAccount), remainingRo(p.launchAuthority), remainingRo(p.hookConfig), remainingRo(p.floorHook)] : [];

export function sellBackIx(a: SaleAccounts & { quoteLeg?: SellBackQuoteLeg; holderTail?: SellBackHolderTail; saleRef?: SaleRefPrefix }, tokens: bigint): FloorInstruction {
  const quoteLeg: AccountMeta[] = a.quoteLeg
    ? [remainingRo(a.quoteLeg.quoteMint), remainingRo(a.quoteLeg.quoteTokenProgram), remainingRw(a.quoteLeg.quoteVault), remainingRw(a.quoteLeg.buyerQuoteAta)]
    : [];
  const tail: AccountMeta[] = a.holderTail
    ? [remainingRo(a.holderTail.floorHolders), remainingRw(a.holderTail.holderBook), remainingRo(a.holderTail.floorPool), remainingRw(a.holderTail.holderRecord)]
    : [];
  const remaining = [...saleRefMetas(a.saleRef), ...quoteLeg, ...tail];
  return makeInstruction(
    'sell_back',
    a.program,
    DISC.launch.sell_back,
    new BorshWriter().u64(tokens),
    [
      ws('buyer', a.buyer),
      rw('launch', a.launch),
      rw('sale_vault', a.saleVault),
      rw('buyer_position', a.buyerPosition),
      rw('buyer_ata', a.buyerAta),
      rw('token_vault', a.tokenVault),
      rw('mint', a.mint),
      ro('lock_authority', a.lockAuthority),
      ro('token_2022', a.token2022),
    ],
    { eventAuthority: a.eventAuthority, argNames: ['tokens'], ...(remaining.length ? { remaining } : {}) },
  );
}

/**
 * `sell_back_wsol(tokens)`: `sell_back`'s named accounts and payout (`floor(tokens · price_x64 / 2^64)` less the FLOOR
 * Platform fee), delivered as wSOL into the seller's SPL Token NATIVE account (credited, then `SyncNative`), so a route
 * can continue from it. A wSOL launch only (`WsolPayoutNotNative`); `sellerWsol` must be an initialized native account
 * owned by the signer (`BadWsolDestination`). Remaining accounts: the optional trader-referral prefix, then
 * `[sellerWsol w, splToken]` (in the token quote leg's place), then a hooked launch's holder tail.
 */
export function sellBackWsolIx(
  a: SaleAccounts & { sellerWsol: Address; splToken: Address; holderTail?: SellBackHolderTail; saleRef?: SaleRefPrefix },
  tokens: bigint,
): FloorInstruction {
  const tail: AccountMeta[] = a.holderTail
    ? [remainingRo(a.holderTail.floorHolders), remainingRw(a.holderTail.holderBook), remainingRo(a.holderTail.floorPool), remainingRw(a.holderTail.holderRecord)]
    : [];
  const remaining = [...saleRefMetas(a.saleRef), remainingRw(a.sellerWsol), remainingRo(a.splToken), ...tail];
  return makeInstruction(
    'sell_back_wsol',
    a.program,
    DISC.launch.sell_back_wsol,
    new BorshWriter().u64(tokens),
    [
      ws('buyer', a.buyer),
      rw('launch', a.launch),
      rw('sale_vault', a.saleVault),
      rw('buyer_position', a.buyerPosition),
      rw('buyer_ata', a.buyerAta),
      rw('token_vault', a.tokenVault),
      rw('mint', a.mint),
      ro('lock_authority', a.lockAuthority),
      ro('token_2022', a.token2022),
    ],
    { eventAuthority: a.eventAuthority, argNames: ['tokens'], remaining },
  );
}

export function closeBuyerPositionIx(p: { program: Address; buyer: Address; launch: Address; buyerPosition: Address }): FloorInstruction {
  return makeInstruction('close_buyer_position', p.program, DISC.launch.close_buyer_position, null, [
    ws('buyer', p.buyer),
    ro('launch', p.launch),
    rw('buyer_position', p.buyerPosition),
  ]);
}

/**
 * `floor_launch::link_trader(tier1, tier2, tier3, deadline)` —
 * the attestor-signed link, written ONCE (`["trader_link", trader]`), which also opens the trader's wSOL bucket (CPI
 * `floor_hook::open_trader_bucket`). ⛔ Top level only, and the instruction IMMEDIATELY after its Ed25519 verification
 * (`traderLinkVerifyIx`), exact offsets. An absent tier is the zero key. Errors `BadAttestation` 6015, `Expired`,
 * `BadTraderLinkShape` 6092, `AlreadyLinked` 6093 (a second link), `TraderTierOffCurve` 6094.
 */
export function linkTraderIx(
  a: {
    program: Address;
    eventAuthority: Address;
    trader: Address;
    launchConfig: Address;
    traderLink: Address;
    instructionsSysvar: Address;
    wsolMint: Address;
    wsolBucket: Address;
    floorHook: Address;
  },
  args: { tier1: Address; tier2: Address; tier3: Address; deadline: bigint },
): FloorInstruction {
  return makeInstruction(
    'link_trader',
    a.program,
    DISC.launch.link_trader,
    new BorshWriter().pubkey(args.tier1).pubkey(args.tier2).pubkey(args.tier3).i64(args.deadline),
    [
      ws('trader', a.trader),
      ro('launch_config', a.launchConfig),
      rw('trader_link', a.traderLink),
      ro('instructions', a.instructionsSysvar),
      ro('wsol_mint', a.wsolMint),
      rw('wsol_bucket', a.wsolBucket),
      ro('floor_hook', a.floorHook),
      ro('system_program', SYSTEM_PROGRAM_ID),
    ],
    { eventAuthority: a.eventAuthority, argNames: ['tier1', 'tier2', 'tier3', 'deadline'] },
  );
}

/** `open_record` / `touch`'s shared accounts. */
export interface HolderRecordOpenAccounts {
  program: Address;
  eventAuthority: Address;
  /** ANY payer — the buyer, the Start-earning holder, or a Withdraw's SENDER. Funds the record's rent. */
  payer: Address;
  /** The token account's OWNER: must be ON-CURVE; need not sign. */
  owner: Address;
  mint: Address;
  holderBook: Address;
  floorPool: Address;
  /** ⛔ `ATA` under Token-2022, and it must EXIST (`NotAssociatedTokenAccount`). */
  tokenAccount: Address;
  /** `["holder", mint, token_account]`. */
  record: Address;
}

function holderOpenSlots(p: HolderRecordOpenAccounts): Slot[] {
  return [
    ws('payer', p.payer),
    ro('owner', p.owner),
    ro('mint', p.mint),
    rw('holder_book', p.holderBook),
    ro('floor_pool', p.floorPool),
    ro('token_account', p.tokenAccount),
    rw('record', p.record),
    ro('system_program', SYSTEM_PROGRAM_ID),
  ];
}

/** `touch()` — `open_record` if the record is missing, else `sync`: the idempotent one a client appends. */
export function holdersTouchIx(p: HolderRecordOpenAccounts): FloorInstruction {
  return makeInstruction('touch', p.program, DISC.holders.touch, null, holderOpenSlots(p), { eventAuthority: p.eventAuthority });
}
