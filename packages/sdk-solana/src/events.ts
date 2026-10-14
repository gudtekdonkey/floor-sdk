/**
 * `emit_cpi!` event decoders for all five programs a FLOOR transaction touches — `carpenter_amm`, `floor_hook`,
 * `floor_launch`, `floor_holders` and `floor_vesting`. FLOOR's four programs' schemas live here; `carpenter_amm`'s are
 * `@floorlaunchpad/carpenter-sdk`'s (`EVENT_SCHEMAS.carpenter_amm`), spread into this table so one decoder reads every event.
 *
 * `emit_cpi!` is a self-CPI whose instruction data is `EVENT_IX_TAG (8 B) ‖ sha256("event:<Name>")[..8] ‖ borsh(event)`,
 * signed by the program's `__event_authority` PDA — the hook CALLBACKS' events included (`decodeInnerInstructionEvents`
 * reads them). `emit!` (`sol_log_data`) logs `disc ‖ borsh(event)` as a base64 `Program data:` line; `decodeLogEvents`
 * reads those — only the ratchet's `StairRentReimbursed` and `reclaim_stair_array`'s `StairArrayReclaimed` still use it.
 * The field lists transcribe the programs' `events.rs`; `test/borsh-events.test.ts` holds `EVENT_SCHEMAS` field for
 * field against the shipped IDLs.
 * ⛔ Decode strictly: trailing bytes are an error (they mean the schema here is stale).
 * ⛔ A discriminator proves NOTHING about a field list — it is `sha256("event:<Name>")[..8]`, so it matches whatever
 *    fields the schema carries. Widening `Bought.cost` to u128 shifts `sold`, `raised` and `buyers` without changing the
 *    total length; the IDL comparison is what catches that.
 */

import {
  anchorDiscriminator,
  EVENT_IX_TAG,
  EVENT_SCHEMAS as CARPENTER_EVENT_SCHEMAS,

  type EventSchema,
  type F,
  type Schema,
  BorshReader,
  base64ToBytes,
  equalBytes,
} from '@floorlaunchpad/carpenter-sdk';
import type { Address } from '@solana/kit';

export type { EventFieldKind, EventSchema, F, Schema } from '@floorlaunchpad/carpenter-sdk';

export type ProgramKey = 'carpenter_amm' | 'floor_hook' | 'floor_launch' | 'floor_holders' | 'floor_vesting';

// The field lists are the programs' `events.rs` in order; the shipped IDLs are their documentation.
// ⚠ `floor_holders::execute` emits NOTHING (it runs at depth 5 under a wrapped route and makes no CPI), so a hooked
//   wallet-to-wallet transfer has no event; `floor_hook`'s ratchet reimbursement is an `emit!` log, not `emit_cpi!`.
const SCHEMAS: Readonly<Record<ProgramKey, Readonly<Record<string, Schema>>>> = {
  carpenter_amm: CARPENTER_EVENT_SCHEMAS.carpenter_amm,
  floor_hook: {
    HookConfigInitialized: [
      ['launchProgram', 'pubkey'], ['launchAuthority', 'pubkey'], ['hookAuthority', 'pubkey'], ['platformOwner', 'pubkey'],
      ['platformDestination', 'pubkey'],
    ],
    Skimmed: [['pool', 'pubkey'], ['isBuy', 'bool'], ['bps', 'u16'], ['fee', 'u64'], ['kept', 'u64']],
    CreatorFee: [['pool', 'pubkey'], ['isBuy', 'bool'], ['amount', 'u64']],
    PlatformCut: [['pool', 'pubkey'], ['amount', 'u64']],
    PolRaised: [['pool', 'pubkey'], ['fromEdge', 'i32'], ['toEdge', 'i32'], ['quoteMoved', 'u64'], ['tokensReadded', 'u64']],
    BidReplaced: [['pool', 'pubkey'], ['tokensAbsorbed', 'u64'], ['ethBack', 'u64'], ['fromLower', 'i32'], ['toLower', 'i32']],
    FloorUnbacked: [['pool', 'pubkey'], ['floorTick', 'i32'], ['vaultEth', 'u64'], ['minBid', 'u64']],
    Ratcheted: [['pool', 'pubkey'], ['floorTick', 'i32'], ['askTick', 'i32'], ['vaultEth', 'u64'], ['inventoryTokens', 'u64'], ['bounty', 'u64']],
    StairRentReimbursed: [['reserve', 'pubkey'], ['cranker', 'pubkey'], ['spent', 'u64'], ['lamports', 'u64']],
    StairArrayReclaimed: [['pool', 'pubkey'], ['reserve', 'pubkey'], ['startIndex', 'i32'], ['lamports', 'u64'], ['tip', 'u64']],
    StairRatcheted: [
      ['pool', 'pubkey'], ['collected', 'u64'], ['toPol', 'u64'], ['toFloor', 'u64'], ['cushion', 'u64'], ['polQuote', 'u64'],
      ['shortFloor', 'u128'], ['shortPol', 'u128'], ['firstOpen', 'u8'], ['relisted', 'u64'],
    ],
    StairFeesSwept: [['pool', 'pubkey'], ['quote', 'u64'], ['tokens', 'u64'], ['bounty', 'u64']],
    StairOwnersPlaced: [
      ['pool', 'pubkey'], ['kind', 'u8'], ['counted', 'u8'], ['payeeBps', 'u16'], ['ownerTotal', 'u64'], ['firstStep', 'u8'],
      ['payeeCount', 'u8'], ['anchor', 'i64'], ['firstUnlockSecs', 'u32'], ['everySecs', 'u32'],
    ],
    StairOwnerBooked: [
      ['pool', 'pubkey'], ['ownerSold', 'u64'], ['ownerQuote', 'u64'], ['due', 'u64'], ['paid', 'u64'], ['forfeited', 'u64'],
      ['feesToFloor', 'u64'], ['ownerOwed', 'u64'],
    ],
    StairPayeeSwept: [['pool', 'pubkey'], ['slot', 'u8'], ['payee', 'pubkey'], ['amount', 'u64'], ['tip', 'u64']],
    StairTokensWithdrawn: [['pool', 'pubkey'], ['slot', 'u8'], ['payee', 'pubkey'], ['step', 'u8'], ['fromReserve', 'bool'], ['tokens', 'u64']],
    StairPayeeResolved: [['pool', 'pubkey'], ['slot', 'u8'], ['pendingId', 'bytes32'], ['payee', 'pubkey']],
    VaultTokensFed: [
      ['pool', 'pubkey'], ['step', 'u8'], ['intoReserve', 'bool'], ['tokens', 'u64'], ['bought', 'u64'], ['free', 'u64'],
      ['cost', 'u64'], ['capTick', 'i32'],
    ],
    CreatorClaimed: [['pool', 'pubkey'], ['slot', 'u8'], ['creator', 'pubkey'], ['amount', 'u64']],
    PlatformSwept: [['pool', 'pubkey'], ['destination', 'pubkey'], ['amount', 'u64']],
    RecipientResolved: [['pool', 'pubkey'], ['slot', 'u8'], ['pendingRecipientId', 'bytes32'], ['recipient', 'pubkey']],
    PoolRegistered: [['floorPool', 'pubkey'], ['ammPool', 'pubkey'], ['baseMint', 'pubkey'], ['poolInitializer', 'pubkey'], ['mode', 'u8']],

    ReferralFunded: [['pool', 'pubkey'], ['distributor', 'pubkey'], ['asset', 'pubkey'], ['amount', 'u64'], ['grossCreatorFee', 'u64']],
    QuoteListed: [['quoteMint', 'pubkey'], ['tokenProgram', 'pubkey'], ['decimals', 'u8'], ['minBidFloor', 'u64'], ['minBidCeil', 'u64'], ['maxBountyCeil', 'u64'], ['minBid', 'u64'], ['maxBounty', 'u64']],
    AllowAllQuotesSet: [['treasury', 'pubkey'], ['governance', 'pubkey'], ['previous', 'bool'], ['allowed', 'bool']],
    QuoteAdmitted: [
      ['floorPool', 'pubkey'], ['quoteMint', 'pubkey'], ['how', 'u8'], ['parent', 'pubkey'], ['decimals', 'u8'],
      ['minBid', 'u64'], ['maxBounty', 'u64'], ['capTick', 'i32'],
    ],
    QuoteEconomicsSet: [['quoteMint', 'pubkey'], ['oldMinBid', 'u64'], ['minBid', 'u64'], ['oldMaxBounty', 'u64'], ['maxBounty', 'u64']],
    QuoteRetired: [['quoteMint', 'pubkey']],
    QuoteObserved: [['pool', 'pubkey'], ['frozen', 'bool'], ['paused', 'bool'], ['slot', 'u64']],
    PoolRewardsTerms: [
      ['floorPool', 'pubkey'], ['ammPool', 'pubkey'], ['baseMint', 'pubkey'], ['referrer', 'pubkey'], ['referralShareBps', 'u16'],
      ['holdersHooked', 'bool'],
    ],
    HolderPaid: [['pool', 'pubkey'], ['destination', 'pubkey'], ['amount', 'u64']],
    ReferralPaid: [['pool', 'pubkey'], ['referrer', 'pubkey'], ['destination', 'pubkey'], ['amount', 'u64']],
    PoolReferralTiers: [['floorPool', 'pubkey'], ['ammPool', 'pubkey'], ['referrer', 'pubkey'], ['referrerT2', 'pubkey'], ['referrerT3', 'pubkey']],
    ReferralTierPaid: [['pool', 'pubkey'], ['tier', 'u8'], ['referrer', 'pubkey'], ['amount', 'u64']],
    PlatformWithdrawn: [['mint', 'pubkey'], ['source', 'pubkey'], ['destination', 'pubkey'], ['governance', 'pubkey'], ['amount', 'u64'], ['remaining', 'u64']],
    TreasuryGovernanceChanged: [['treasury', 'pubkey'], ['previous', 'pubkey'], ['governance', 'pubkey'], ['accepted', 'bool']],
    HookGovernanceChanged: [['previous', 'pubkey'], ['governance', 'pubkey'], ['accepted', 'bool']],
    TeamSwept: [['pool', 'pubkey'], ['mint', 'pubkey'], ['amount', 'u64']],
    FloorPoolDesignated: [['floorPool', 'pubkey'], ['ammPool', 'pubkey'], ['baseMint', 'pubkey']],
    PlatformPoolCredit: [['sourcePool', 'pubkey'], ['targetPool', 'pubkey'], ['amount', 'u64']],
    TeamWithdrawn: [['mint', 'pubkey'], ['source', 'pubkey'], ['destination', 'pubkey'], ['governance', 'pubkey'], ['amount', 'u64'], ['remaining', 'u64']],
    InventoryDonated: [['pool', 'pubkey'], ['from', 'pubkey'], ['amount', 'u64']],
    TraderReferralBooked: [['pool', 'pubkey'], ['trader', 'pubkey'], ['s', 'u64x3']],
    TraderReferralSwept: [['pool', 'pubkey'], ['quoteMint', 'pubkey'], ['amount', 'u64']],
    TraderReferralClaimed: [['referrer', 'pubkey'], ['quoteMint', 'pubkey'], ['paid', 'u64']],
  },
  floor_launch: {
    LaunchConfigInitialized: [['launchConfig', 'pubkey'], ['governance', 'pubkey'], ['attestor', 'pubkey'], ['cluster', 'u8']],
    LaunchCreated: [
      ['launch', 'pubkey'], ['mint', 'pubkey'], ['creator', 'pubkey'], ['ammPool', 'pubkey'],
      ['floorPool', 'pubkey'], ['registryIndex', 'u64'], ['mode', 'u8'], ['priceX64', 'u128'], ['openTick', 'i32'],
      ['name', 'string'], ['symbol', 'string'], ['uri', 'string'],
      ['quoteMint', 'pubkey'], ['quoteDecimals', 'u8'], ['quoteUsdCents', 'u64'], ['graduationMcapUsd', 'u64'],
    ],

    PayeeSlotsSet: [['launch', 'pubkey'], ['creator', 'vec_slot_args'], ['team', 'vec_slot_args']],
    Bought: [['launch', 'pubkey'], ['buyer', 'pubkey'], ['tokens', 'u64'], ['cost', 'u64'], ['sold', 'u64'], ['raised', 'u64'], ['buyers', 'u64']],
    SoldBack: [['launch', 'pubkey'], ['buyer', 'pubkey'], ['tokens', 'u64'], ['payout', 'u64'], ['sold', 'u64'], ['raised', 'u64']],
    Graduated: [['launch', 'pubkey'], ['raised', 'u64'], ['seeded', 'u64'], ['openTick', 'i32'], ['ladderTokens', 'u64'], ['pauseAuthorityRevoked', 'bool']],
    RecipientClaimed: [['launch', 'pubkey'], ['slot', 'u8'], ['pendingRecipientId', 'bytes32'], ['recipient', 'pubkey']],
    TeamBought: [
      ['launch', 'pubkey'], ['creator', 'pubkey'], ['tokens', 'u64'], ['stepTokens', 'u64'], ['walletTokens', 'u64'], ['cost', 'u64'],
      ['fee', 'u64'], ['lockDays', 'u16'], ['vestDays', 'u16x4'],
    ],
    TeamVestedClaimed: [['launch', 'pubkey'], ['slot', 'u8'], ['to', 'pubkey'], ['amount', 'u64'], ['cliffsPaid', 'u8']],
    TeamBuySoldBack: [['launch', 'pubkey'], ['creator', 'pubkey'], ['tokens', 'u64'], ['cost', 'u64'], ['fee', 'u64'], ['sold', 'u64'], ['raised', 'u64']],
    StairRentFunded: [['launch', 'pubkey'], ['ammPool', 'pubkey'], ['stairRent', 'pubkey'], ['lamports', 'u64'], ['tip', 'u64']],
    StairPayeeClaimed: [['launch', 'pubkey'], ['slot', 'u8'], ['pendingId', 'bytes32'], ['payee', 'pubkey']],
    GraduationRentReserved: [['launch', 'pubkey'], ['reserve', 'pubkey'], ['lamports', 'u64']],
    GraduationRentClosed: [['launch', 'pubkey'], ['reserve', 'pubkey'], ['creator', 'pubkey'], ['lamports', 'u64']],
    GraduationPrepared: [['launch', 'pubkey'], ['pool', 'pubkey'], ['poolCreated', 'bool'], ['tickArraysCreated', 'u8'], ['reimbursed', 'u64']],
    MetadataBaseSet: [['launchConfig', 'pubkey'], ['base', 'string'], ['setSlot', 'u64'], ['fillableFromSlot', 'u64']],
    MetadataUriFilled: [['launch', 'pubkey'], ['mint', 'pubkey'], ['uri', 'string']],
    LaunchEconomicsUpdated: [
      ['launchConfig', 'pubkey'], ['authority', 'pubkey'], ['oldMinBidLamports', 'u64'], ['minBidLamports', 'u64'],
      ['oldBountyBps', 'u16'], ['bountyBps', 'u16'], ['oldMaxBountyLamports', 'u64'], ['maxBountyLamports', 'u64'],
    ],
    GovernanceChanged: [['launchConfig', 'pubkey'], ['previous', 'pubkey'], ['governance', 'pubkey'], ['accepted', 'bool']],
    AttestorChanged: [['launchConfig', 'pubkey'], ['previous', 'pubkey'], ['attestor', 'pubkey']],
    TeamFeesSet: [
      ['launchConfig', 'pubkey'], ['governance', 'pubkey'], ['oldSaleBps', 'u16'], ['saleBps', 'u16'],
      ['oldPoolBps', 'u16'], ['poolBps', 'u16'],
    ],
    GenesisInitialized: [
      ['genesisSale', 'pubkey'], ['launch', 'pubkey'], ['mint', 'pubkey'], ['authority', 'pubkey'], ['beneficiary', 'pubkey'],
      ['floorShareBps', 'u16'], ['duration', 'u64'],
    ],
    ScheduleUpdated: [['openTime', 'i64'], ['duration', 'u64'], ['saleEnd', 'i64']],
    FloorShareUpdated: [['previous', 'u16'], ['next', 'u16']],
    GenesisAuthorityTransferStarted: [['previousAuthority', 'pubkey'], ['newAuthority', 'pubkey']],
    GenesisAuthorityTransferred: [['previousAuthority', 'pubkey'], ['newAuthority', 'pubkey']],
    Contributed: [['user', 'pubkey'], ['amount', 'u64'], ['userTotal', 'u64'], ['raisedTotal', 'u64'], ['contributorCount', 'u64'], ['phase', 'u8'], ['phaseUserTotal', 'u64'], ['phaseRaisedTotal', 'u64']],
    OpeningTick: [['tick', 'i32'], ['clamped', 'bool'], ['stairSteps', 'u8'], ['stairTopMcap', 'u64']],
    GenesisStairTopUpdated: [['previous', 'u64'], ['next', 'u64']],
    GenesisWhitelistUpdated: [['previous', 'bytes32'], ['next', 'bytes32'], ['phase', 'u8']],
    FloorFeesApplied: [['amount', 'u64'], ['cumulativeApplied', 'u64']],
    GenesisGraduated: [
      ['saleEnd', 'i64'], ['raised', 'u64'], ['externalFees', 'u64'], ['initialFloor', 'u64'], ['offeredPol', 'u64'],
      ['returnedPol', 'u64'], ['ladderTokens', 'u64'], ['openTick', 'i32'],
    ],
    VestingCreated: [['schedules', 'pubkey5']],
    GenesisClaimed: [['user', 'pubkey'], ['amount', 'u64'], ['cumulativeUserClaimed', 'u64'], ['totalClaimed', 'u64']],
    AllocationDustRouted: [['amount', 'u64']],
    GenesisRefunded: [['wallet', 'pubkey'], ['amount', 'u64'], ['refundedTotal', 'u64']],
    QuoteChainPriced: [
      ['launch', 'pubkey'], ['rootQuote', 'pubkey'], ['rootDecimals', 'u8'], ['hops', 'u16'], ['chainTicks', 'i64'],
    ],
    PresetAdded: [
      ['id', 'u8'], ['graduationMcapUsd', 'u64'], ['creatorBps', 'u16'], ['skimBuyFarBps', 'u16'], ['skimSellFarBps', 'u16'],
      ['holderBps', 'u16'], ['saleBps', 'u16'], ['polBps', 'u16'], ['skimSellAtFloorMultipleBps', 'u32'], ['stairSteps', 'u8'],
      ['stairGrowthBps', 'u16'], ['teamBuyMaxBps', 'u16'], ['teamVestDays', 'u16x4'],
    ],
    PresetEnabled: [['id', 'u8'], ['enabled', 'bool']],
    PresetAuthorityTransferStarted: [['authority', 'pubkey'], ['pending', 'pubkey']],
    PresetAuthorityTransferred: [['previous', 'pubkey'], ['authority', 'pubkey']],
    TraderLinked: [['trader', 'pubkey'], ['tier1', 'pubkey'], ['tier2', 'pubkey'], ['tier3', 'pubkey']],
    SaleTraderReferralBooked: [['launch', 'pubkey'], ['trader', 'pubkey'], ['s1', 'u64'], ['s2', 'u64'], ['s3', 'u64']],
    SaleTraderReferralSwept: [['launch', 'pubkey'], ['quoteMint', 'pubkey'], ['amount', 'u64']],
  },
  floor_holders: {
    LaunchHooked: [['mint', 'pubkey'], ['floorPool', 'pubkey'], ['holderBook', 'pubkey'], ['extraAccountMetas', 'pubkey']],
    ClaimsTableRotated: [['table', 'pubkey'], ['tables', 'u64']],
    /** `mint` is the zero key on an unhooked-but-referred launch (its three addresses carry no mint). */
    ClaimsTableExtended: [['table', 'pubkey'], ['mint', 'pubkey'], ['floorPool', 'pubkey'], ['added', 'u8'], ['len', 'u16']],
    RecordOpened: [['mint', 'pubkey'], ['record', 'pubkey'], ['tokenAccount', 'pubkey'], ['owner', 'pubkey'], ['payer', 'pubkey'], ['shares', 'u64']],
    HolderClaimed: [['mint', 'pubkey'], ['record', 'pubkey'], ['owner', 'pubkey'], ['destination', 'pubkey'], ['amount', 'u64']],
    RecordClosed: [['mint', 'pubkey'], ['record', 'pubkey'], ['owner', 'pubkey'], ['refundLamports', 'u64']],
  },
  floor_vesting: {
    ScheduleCreated: [
      ['schedule', 'pubkey'], ['funder', 'pubkey'], ['beneficiary', 'pubkey'], ['mint', 'pubkey'], ['escrow', 'pubkey'],
      ['nonce', 'u64'], ['total', 'u64'], ['start', 'i64'], ['cliff', 'i64'], ['end', 'i64'],
    ],
    Claimed: [['schedule', 'pubkey'], ['beneficiary', 'pubkey'], ['amount', 'u64'], ['claimed', 'u64'], ['total', 'u64']],
    ScheduleClosed: [['schedule', 'pubkey'], ['funder', 'pubkey'], ['rentReturned', 'u64'], ['swept', 'u64']],
    AbsoluteSchedule: [['schedule', 'pubkey'], ['start', 'i64'], ['cliff', 'i64'], ['end', 'i64'], ['linearAfterCliff', 'bool']],
  },
};

/**
 * ⛔ An event NAME must be unique across the five programs: `eventDiscriminator(name)` and an unfiltered
 * `decodeEventPayload` resolve by name / discriminator alone. `floor_vesting.Claimed` is the nearest
 * neighbour (`CreatorClaimed`, `ReferralPaid`, `HolderClaimed` are all distinct names), and this
 * module refuses to load if two programs ever declare the same one.
 */
{
  const seen = new Map<string, string>();
  for (const [program, events] of Object.entries(SCHEMAS)) {
    for (const name of Object.keys(events)) {
      const other = seen.get(name);
      if (other) throw new Error(`events.ts: ${name} is declared by both ${other} and ${program} — an unfiltered decode would be ambiguous`);
      seen.set(name, program);
    }
  }
}

export interface DecodedEvent {
  readonly program: ProgramKey;
  readonly name: string;
  readonly data: Readonly<Record<string, unknown>>;
}

interface Entry {
  program: ProgramKey;
  name: string;
  disc: Uint8Array;
  schema: Schema;
}

const ENTRIES: Entry[] = Object.entries(SCHEMAS).flatMap(([program, events]) =>
  Object.entries(events).map(([name, schema]) => ({ program: program as ProgramKey, name, disc: anchorDiscriminator(`event:${name}`), schema })),
);

/**
 * The decoders’ READ SEQUENCE, in order, per program. `decodeEventPayload` walks exactly this
 * table, so it IS the wire schema this SDK believes in — exported so `test/idl-parity.test.ts`
 * can hold it, field by field and IN ORDER, to the built IDL and to each program’s `events.rs`,
 * and so `test/borsh-events.test.ts` can map each field kind to an IDL type.
 * ⛔ Borsh carries no field names: ORDER is the whole contract.
 * ⛔ Read-only: decode through `decodeEventPayload`, never by walking this.
 */
export const EVENT_SCHEMAS: Readonly<Record<ProgramKey, Readonly<Record<string, EventSchema>>>> = SCHEMAS;

/**
 * ⛔ **EARLIER GENERATIONS OF AN EVENT THAT AN ALREADY-DEPLOYED PROGRAM STILL EMITS**, each
 * as a PREFIX LENGTH of the current schema — so a legacy shape is the current one's first N fields BY
 * CONSTRUCTION and cannot drift from it (fields are only ever APPENDED; Borsh has no names, so an
 * inserted field would break every decoder and is not a generation). Here: `carpenter_amm.Swapped`'s
 * 21-field shape, ending at `feeSplit1`, before the trade's own walk was appended.
 * ⛔ A legacy decode is TRIED ONLY AFTER the current schema refuses, and each shape is still decoded
 * STRICTLY (`finish`): a body that ends INSIDE the appended tail, or runs past it, is neither shape
 * and throws exactly as before. The data of a legacy decode simply LACKS the appended keys —
 * `quoteMint === undefined` means "this event predates the field", never wSOL and never zero.
 */
const LEGACY_PREFIXES: Readonly<Partial<Record<ProgramKey, Readonly<Record<string, readonly number[]>>>>> = {
  carpenter_amm: { Swapped: [21] },
};

/** The legacy shapes `decodeEventPayload` accepts, derived from `LEGACY_PREFIXES` (read-only, for tests). */
export const EVENT_LEGACY_SCHEMAS: Readonly<Partial<Record<ProgramKey, Readonly<Record<string, readonly EventSchema[]>>>>> =
  Object.fromEntries(
    Object.entries(LEGACY_PREFIXES).map(([program, events]) => [
      program,
      Object.fromEntries(
        Object.entries(events ?? {}).map(([name, lens]) => [
          name,
          lens.map((n) => SCHEMAS[program as ProgramKey][name]!.slice(0, n)),
        ]),
      ),
    ]),
  );

export const EVENT_NAMES: Readonly<Record<ProgramKey, readonly string[]>> = {
  carpenter_amm: Object.keys(SCHEMAS.carpenter_amm),
  floor_hook: Object.keys(SCHEMAS.floor_hook),
  floor_launch: Object.keys(SCHEMAS.floor_launch),
  floor_holders: Object.keys(SCHEMAS.floor_holders),
  floor_vesting: Object.keys(SCHEMAS.floor_vesting),
};

export function eventDiscriminator(name: string): Uint8Array {
  const e = ENTRIES.find((x) => x.name === name);
  if (!e) throw new Error(`unknown event ${name}`);
  return e.disc;
}

function readValue(r: BorshReader, f: F): unknown {
  switch (f) {
    case 'pubkey':
      return r.pubkey();
    case 'bool':
      return r.bool();
    case 'u8':
      return r.u8();
    case 'u16':
      return r.u16();
    case 'u32':
      return r.u32();
    case 'i32':
      return r.i32();
    case 'u64':
      return r.u64();
    case 'i64':
      return r.i64();
    case 'u128':
      return r.u128();
    case 'i128':
      return r.i128();
    case 'string':
      return r.string();
    case 'bytes32':
      return r.fixed(32);
    // `[u64; 4]`, the fee split `[skim, creator, holder, platform]`;
    // `[u64; 5]`: `[skim, creator, holder, platform, team]`.
    case 'u64x5':
      return [r.u64(), r.u64(), r.u64(), r.u64(), r.u64()];
    // `TraderReferralBooked.s`, the three tiers' slices.
    case 'u64x3':
      return [r.u64(), r.u64(), r.u64()];
    // `TeamBought.vest_days`.
    case 'u16x4':
      return [r.u16(), r.u16(), r.u16(), r.u16()];
    case 'option_u32':
      return r.option(() => r.u32());
    case 'vec_pubkey':
      return r.vec((): Address => r.pubkey());
    case 'pubkey5':
      return [r.pubkey(), r.pubkey(), r.pubkey(), r.pubkey(), r.pubkey()];
    case 'vec_slot_args':
      return r.vec(() => ({ bps: r.u16(), platform: r.u8(), key: r.fixed(32) }));
  }
}

/** Decode `disc ‖ borsh(event)` (the payload of `emit!` and the tail of `emit_cpi!`). */
export function decodeEventPayload(bytes: Uint8Array, program?: ProgramKey): DecodedEvent | null {
  if (bytes.length < 8) return null;
  const head = bytes.subarray(0, 8);
  const e = ENTRIES.find((x) => (program === undefined || x.program === program) && equalBytes(x.disc, head));
  if (!e) return null;
  const body = bytes.subarray(8);
  try {
    return { program: e.program, name: e.name, data: decodeStrict(body, e.schema) };
  } catch (err) {
    // ⛔ Only an event with a declared earlier generation gets a second try, and only a shape that
    // decodes EXACTLY wins; otherwise the CURRENT schema's error is what the caller sees.
    for (const legacy of EVENT_LEGACY_SCHEMAS[e.program]?.[e.name] ?? []) {
      try {
        return { program: e.program, name: e.name, data: decodeStrict(body, legacy) };
      } catch {
        // not this generation either
      }
    }
    throw err;
  }
}

/** Decode `body` as exactly `schema` — every field, then no trailing byte. */
function decodeStrict(body: Uint8Array, schema: Schema): Record<string, unknown> {
  const r = new BorshReader(body);
  const data: Record<string, unknown> = {};
  for (const [k, f] of schema) data[k] = readValue(r, f);
  r.finish();
  return data;
}

/** Decode one `emit_cpi!` inner-instruction data blob; `null` if it is not an event CPI. */
export function decodeEmitCpiData(ixData: Uint8Array, program?: ProgramKey): DecodedEvent | null {
  if (ixData.length < 16 || !equalBytes(ixData.subarray(0, 8), EVENT_IX_TAG)) return null;
  return decodeEventPayload(ixData.subarray(8), program);
}

/**
 * Decode every FLOOR event in a transaction's inner instructions. `innerInstructions` is the
 * RPC's `meta.innerInstructions` shape, already base58/base64-decoded to bytes by the caller;
 * `programOf(index)` maps an account index to its address. Only CPIs INTO one of our programs,
 * whose data starts with `EVENT_IX_TAG`, are considered.
 */
export function decodeInnerInstructionEvents(
  inner: readonly { readonly instructions: readonly { readonly programIdIndex: number; readonly data: Uint8Array }[] }[],
  accountKeys: readonly Address[],
  // ⚠ PARTIAL: a caller that does not read `floor_holders` / `floor_vesting` events
  //   passes the programs it does; an absent key simply decodes nothing for that program.
  programs: Readonly<Partial<Record<ProgramKey, Address>>>,
): DecodedEvent[] {
  const byAddress = new Map<string, ProgramKey>(Object.entries(programs).filter(([, v]) => v !== undefined).map(([k, v]) => [v as string, k as ProgramKey]));
  const out: DecodedEvent[] = [];
  for (const group of inner) {
    for (const ix of group.instructions) {
      const program = byAddress.get(accountKeys[ix.programIdIndex] ?? '');
      if (!program) continue;
      const ev = decodeEmitCpiData(ix.data, program);
      if (ev) out.push(ev);
    }
  }
  return out;
}

/** `emit!` logs `Program data: <base64>`; decode each such line (the hook callbacks' events are `emit_cpi!`). */
export function decodeLogEvents(logs: readonly string[], program?: ProgramKey): DecodedEvent[] {
  const out: DecodedEvent[] = [];
  for (const line of logs) {
    const m = /^Program data: ([A-Za-z0-9+/=]+)$/.exec(line);
    if (!m || m[1] === undefined) continue;
    const ev = decodeEventPayload(base64ToBytes(m[1]), program);
    if (ev) out.push(ev);
  }
  return out;
}
