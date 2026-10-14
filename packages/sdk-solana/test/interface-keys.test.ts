
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { CARPENTER_ROOT } from './helpers.ts';
import { test } from './node-test.ts';

import * as CARPENTER from '@floorlaunchpad/carpenter-sdk';
import * as FLOOR_LAYOUTS from '../src/layouts.ts';

const layouts = { ...CARPENTER, ...FLOOR_LAYOUTS };
import {
  decodeStairOwners,
  BUYER_POSITION,
  FLOOR_PLATFORM_POOL,
  FLOOR_POOL,
  HOLDER_BOOK,
  HOLDER_RECORD,
  HOOK_CONFIG,
  LAUNCH,
  LAUNCH_CONFIG,
  PLATFORM_TREASURY,
  QUOTE_POLICY,
  REGISTRY_ENTRY,
  STAIR_STATE,
  STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS,
  type BuyerPosition,
  type FloorPlatformPool,
  type FloorPool,
  type TraderLink,
  type TraderBucket,
  TRADER_LINK,
  TRADER_BUCKET,
  type HolderBook,
  type HolderRecord,
  type HookConfig,
  type Launch,
  type LaunchConfig,
  type PlatformTreasury,
  type QuotePolicy,
  type RegistryEntry,
  type StairOwners,
  type StairState,
} from '../src/layouts.ts';
import {
  decodeAccount,
  AMM_CONFIG,
  APPROVED_HOOK,
  MINT_EXCLUSIVITY,
  MINT_POOL_COUNT,
  POOL,
  POOL_INDEX_ENTRY,
  POSITION,
  PROTOCOL_TREASURY,
  QUOTE_MINT,
  TICK_ARRAY,
  TICK_ARRAY_BITMAP,
  type LayoutDef,
  type AmmConfig,
  type ApprovedHook,
  type MintExclusivity,
  type MintPoolCount,
  type Pool,
  type PoolIndexEntry,
  type Position,
  type ProtocolTreasury,
  type QuoteMint,
  type TickArray,
  type TickArrayBitmap,
} from '@floorlaunchpad/carpenter-sdk';

const GUARDED: readonly (readonly [LayoutDef, Record<string, true>])[] = [
  [AMM_CONFIG, {
    version: true, bump: true, authority: true, pendingAuthority: true, protocolFeeController: true,
    pauseGuardian: true, quoteCapsAdmitted: true, protocolFee: true,
  } satisfies Record<keyof Omit<AmmConfig, 'currentProtocolFee'>, true>],
  [APPROVED_HOOK, {
    version: true, bump: true, paused: true, maxRounds: true, callbackAccounts: true, allowPublicPools: true,
    flags: true, hookProgram: true, hookAuthority: true, hookAuthorityBump: true, programdataSlot: true,
    approvedSlot: true, name: true, allowExclusivity: true, baseTransferHook: true, guardianPausedUntil: true, allowHookQuotes: true,
  } satisfies Record<keyof ApprovedHook, true>],
  [MINT_EXCLUSIVITY, {
    mint: true, hookProgram: true, bump: true, hasProtocolFee: true, protocolFee: true,
  } satisfies Record<keyof MintExclusivity, true>],
  [MINT_POOL_COUNT, {
    mint: true, count: true, bump: true,
  } satisfies Record<keyof MintPoolCount, true>],
  [QUOTE_MINT, {
    mint: true, authorityDigest: true, listedSlot: true, retiredSlot: true, state: true, bump: true, decimals: true,
    caps: true, hookProgram: true,
  } satisfies Record<keyof QuoteMint, true>],
  [POOL, {
    version: true, bump: true, hookFlags: true, tickSpacing: true, hookMaxRounds: true, hookCallbackAccounts: true,
    fee: true, lpFee: true, protocolFee: true, tickCurrent: true, mint0: true, mint1: true, hook: true,
    hookAuthority: true, vault0: true, vault1: true, tokenProgram0: true, tokenProgram1: true, sqrtPriceX64: true,
    liquidity: true, feeGrowthGlobal0X64: true, feeGrowthGlobal1X64: true, protocolFeesOwed0: true,
    protocolFeesOwed1: true, hookClaims0: true, hookClaims1: true, volume0: true, volume1: true, lpFees0: true,
    lpFees1: true, hookFees0: true, hookFees1: true, swapCount: true, createdSlot: true, decimals0: true,
    decimals1: true, bitmapBump: true, vault0Bump: true, vault1Bump: true, paused: true, baseTransferHook: true,
    baseHookSide: true, protocolFeeCurrency: true, guardianPausedUntil: true,
  } satisfies Record<keyof Pool, true>],
  [TICK_ARRAY_BITMAP, {
    pool: true, tickSpacing: true, arraysPerSide: true,
  } satisfies Record<keyof Omit<TickArrayBitmap, 'words'>, true>],
  [TICK_ARRAY, {
    pool: true, startIndex: true, initializedTickCount: true, ticks: true,
  } satisfies Record<keyof Omit<TickArray, 'rentOwner'>, true>],
  [POSITION, {
    version: true, bump: true, pool: true, owner: true, salt: true, tickLower: true, tickUpper: true, liquidity: true,
    feeGrowthInside0LastX64: true, feeGrowthInside1LastX64: true, positionId: true,
  } satisfies Record<keyof Position, true>],
  [POOL_INDEX_ENTRY, {
    pool: true, createdSlot: true, bump: true,
  } satisfies Record<keyof PoolIndexEntry, true>],
  [HOOK_CONFIG, {
    version: true, bump: true, maxRounds: true, callbackAccounts: true, flags: true, authorityBump: true,
    governance: true, platformDestination: true, launchProgram: true, launchAuthority: true, hookAuthority: true,
    platformOwner: true, pendingGovernance: true,
  } satisfies Record<keyof HookConfig, true>],
  [FLOOR_POOL, {
    version: true, bump: true, poolBound: true, quoteIsToken1: true, ladderSeeded: true, needsRatchet: true,
    mode: true, quoteSuspended: true, tickSpacing: true, floorTick: true, ammPool: true, poolInitializer: true,
    baseMint: true, quoteMint: true, skimBuyFarBps: true,
    skimSellFarBps: true, creatorBuyBps: true, creatorSellBps: true, holderBuyBps: true, holderSellBps: true,
    wallTicks: true, platformBps: true, minBid: true, minSellable: true, ownerOwed: true, ownerPaid: true,
    vaultEth: true, bidEthDeployed: true, unpaidSkim: true, cumulativeVolumeEth: true, inventoryTokens: true,
    inventoryCostEth: true, askTokensDeployed: true, ladderTokensRemaining: true, bidLower: true, askUpper: true,
    askWidth: true, seedLower: true, seedUpper: true, ladderLower: true, ladderUpper: true, wallSplitTick: true,
    bidLiquidity: true, askLiquidity: true, seedLiq: true, ladderLiq: true, creatorFeesOwed: true, platformOwed: true,
    holderOwed: true, wallTailLiq: true, wallHeldTokens: true, seedFeesSweptEth: true, seedFeesSweptTokens: true,
    ladderProceedsEth: true, platformPaidEth: true, platformSwept: true, creatorFeesPaid: true, holderPaidEth: true,
    lastRatchetSlot: true, bidWidth: true, skimBuyAtFloorBps: true, skimSellAtFloorBps: true, maxBounty: true,
    bountyBps: true, wallBandTicks: true, wallTailBps: true, wallListBps: true, referralOwed: true,
    referralFeesPaid: true, referralShareBps: true, stairSteps: true, stairReady: true, stairGrowthBps: true,
    stairKind: true, stairOwner: true, referrer: true, holdersHooked: true, referrerT2: true, referrerT3: true,
    referralPaidT2: true, referralPaidT3: true, traderRefOwed: true, traderRefBooked: true, teamOwed: true, teamFeesPaid: true, teamFeeBps: true,
  } satisfies Record<keyof Omit<FloorPool, 'twap' | 'payeeSlots' | 'referralDestination' | 'referralTiers' | 'referralTierDues'>, true>],
  [LAUNCH_CONFIG, {
    version: true, bump: true, cluster: true, governance: true, attestor: true, hookProgram: true, ammProgram: true,
    platformBps: true, tickSpacing: true, minBidLamports: true, minSellable: true, registryCount: true,
    maxBountyLamports: true, metadataBaseSetSlot: true, bountyBps: true, metadataBaseLen: true, metadataBase: true,
    pendingGovernance: true, teamSaleBpsRaw: true, teamPoolBpsRaw: true, teamBpsSet: true,
  } satisfies Record<keyof LaunchConfig, true>],
  [REGISTRY_ENTRY, {
    index: true, launch: true, mint: true, ammPool: true, mode: true, createdSlot: true,
  } satisfies Record<keyof RegistryEntry, true>],
  [LAUNCH, {
    version: true, bump: true, saleVaultBump: true, lockBump: true, state: true, mode: true, mintAuthorityBump: true,
    mint: true, creator: true, ammPool: true,
    floorPool: true, tokenVault: true, wsolVault: true, priceX64: true, quoteUsdCents: true, mcapUsd: true,
    createdSlot: true, nonce: true, saleTokens: true, polTokens: true, ladderTokens: true, sold: true, raised: true,
    buyers: true, openTick: true, teamCliffsPaidBySlot: true, traderRefSaleBump: true, skimBuyFar: true, skimSellFar: true, creatorBuy: true, creatorSell: true,
    holderBuy: true, holderSell: true, teamBps: true, registryIndex: true, graduatedSlot: true, quoteMint: true,
    teamOwed: true, teamStepTokens: true, teamWalletTokens: true, teamAnchor: true, teamLockDays: true,
    teamVestDays: true, stairRentBudget: true,
  } satisfies Record<keyof Launch, true>],
  [BUYER_POSITION, {
    bump: true, allocation: true, contributed: true,
  } satisfies Record<keyof BuyerPosition, true>],
  [QUOTE_POLICY, {
    version: true, bump: true, decimals: true, retired: true, quoteMint: true, tokenProgram: true, minBidFloor: true,
    minBidCeil: true, maxBountyCeil: true, minBid: true, maxBounty: true,
  } satisfies Record<keyof QuotePolicy, true>],
  [HOLDER_BOOK, {
    version: true, bump: true, metasBump: true, mint: true, floorPool: true, quoteMint: true, acc: true,
    eligible: true, indexed: true, unallocated: true, paid: true, records: true, createdSlot: true, pending: true,
    lastSlot: true,
  } satisfies Record<keyof HolderBook, true>],
  [HOLDER_RECORD, {
    version: true, bump: true, mint: true, tokenAccount: true, owner: true, shares: true, snapshot: true, owed: true,
    paid: true, openedSlot: true,
  } satisfies Record<keyof HolderRecord, true>],
  [PLATFORM_TREASURY, {
    version: true, bump: true, governance: true, pendingGovernance: true, allowAllQuotes: true,
  } satisfies Record<keyof PlatformTreasury, true>],
  [FLOOR_PLATFORM_POOL, {
    version: true, bump: true, floorPool: true, ammPool: true, baseMint: true, quoteMint: true, received: true,
    credits: true, designatedSlot: true,
  } satisfies Record<keyof FloorPlatformPool, true>],
  [PROTOCOL_TREASURY, {
    version: true, bump: true, governance: true, pendingGovernance: true,
  } satisfies Record<keyof ProtocolTreasury, true>],
  [TRADER_LINK, { trader: true, tier1: true, tier2: true, tier3: true, bump: true } satisfies Record<keyof TraderLink, true>],
  [TRADER_BUCKET, {
    trader: true, quoteMint: true, tier1: true, tier2: true, tier3: true, owed: true, bump: true,
  } satisfies Record<keyof TraderBucket, true>],
  [STAIR_STATE, {
    version: true, bump: true, steps: true, placed: true, firstOpen: true, tailPlaced: true, growthBps: true,
    openEdge: true, tailLower: true, tailUpper: true, cushionLower: true, cushionUpper: true, relistLower: true,
    relistUpper: true, relistCostTick: true, stairTotal: true, tailTotal: true, collectedTotal: true, toPolTotal: true,
    toFloorTotal: true, cushionQuote: true, tailLiq: true, cushionLiq: true, relistLiq: true, stepLower: true,
    stepUpper: true, stepLiq: true, reserveLower: true, reserveUpper: true, reserveLiq: true, vaultListedTokens: true,
    vaultListedCost: true, vaultFedBought: true, cushionBottomBps: true, polToMcapBps: true, cushionTicks: true,
    sweptQuote: true,
  } satisfies Record<keyof StairState, true>],
];

/** The table's decoded keys against its list, by name: `missing` = table fields with no interface key. */
function keyDiff(decoded: readonly string[], list: Record<string, true>): { missing: string[]; extra: string[] } {
  const want = Object.keys(list);
  return { missing: decoded.filter((k) => !want.includes(k)), extra: want.filter((k) => !decoded.includes(k)) };
}

test('every decodeAccount table decodes exactly the keys its interface names', () => {
  for (const [layout, list] of GUARDED) {
    const data = new Uint8Array(8 + layout.size);
    data.set(layout.discriminator, 0);
    const decoded = Object.keys(decodeAccount<Record<string, unknown>>(layout, data));
    assert.deepEqual({ layout: layout.name, ...keyDiff(decoded, list) }, { layout: layout.name, missing: [], extra: [] });
  }
});

test('GUARDED holds one row for every table layouts.ts passes to decodeAccount', () => {
  // Both packages' layouts: `@floorlaunchpad/carpenter-sdk`'s codec and AMM tables, then this package's FLOOR tables.
  const src = [join(CARPENTER_ROOT, 'src/layouts.ts'), fileURLToPath(new URL('../src/layouts.ts', import.meta.url))].map((f) => readFileSync(f, 'utf8')).join('\n');
  const called = [...new Set([...src.matchAll(/decodeAccount<[^(]*>\(\s*([A-Z_]+)\s*,/g)].map((m) => m[1]!))].sort();
  const byLayout = new Map(Object.entries(layouts).filter(([, v]) => GUARDED.some(([l]) => l === v)).map(([k, v]) => [v, k]));
  const guarded = GUARDED.map(([l]) => byLayout.get(l) ?? `<unexported ${l.name}>`).sort();
  assert.deepEqual(guarded, called);
});

const STAIR_OWNERS_KEYS = {
  version: true, kind: true, counted: true, firstStep: true, payeeBps: true, firstUnlockSecs: true, everySecs: true,
  anchor: true, ownerTotal: true, ownerSold: true, ownerWithdrawn: true,
  ownerProceeds: true, owedTotal: true, forfeited: true, feesToFloor: true, tipsPaid: true, tokens: true, cut: true,
  lastTipAt: true,
  held: true,
  reserveFed: true,
  clamped: true,
} satisfies Record<keyof StairOwners, true>;

test('StairOwners decodes exactly the keys its interface names', () => {
  const owners = decodeStairOwners(new Uint8Array(STAIR_STATE_ACCOUNT_LEN_WITH_OWNERS));
  assert.ok(owners);
  assert.deepEqual({ layout: 'StairOwners', ...keyDiff(Object.keys(owners), STAIR_OWNERS_KEYS) }, { layout: 'StairOwners', missing: [], extra: [] });
});
