/**
 * `stateView`: FLOOR's reads (`floor_hook`, `floor_launch`, `floor_holders`) beside `@floorsupply/carpenter-sdk`'s — `stateView`
 * here spreads Carpenter's object. Every helper takes an `AccountSource` and returns the decoded value WITH the slot it
 * was read at.
 *
 * ⛔ Inside a hooked flow, pool state comes from instruction data or post-state, never from a stale account: these
 *    helpers are for UIs and for building transactions.
 * ⛔ Every account is checked for its OWNER program as well as its discriminator; a lookalike account owned by another
 *    program is `null`, never decoded.
 */

import {
  stateView as carpenterStateView,
  type AtSlot,
  TOKEN_2022_PROGRAM_ID,
  hasDiscriminator,
  type LayoutDef,
  type AccountSource,
  type RawAccount,
  decodeTokenAccount,
} from '@floorsupply/carpenter-sdk';

import {
  BUYER_POSITION,
  decodeBuyerPosition,
  decodeFloorPool,
  decodeHookConfig,
  decodeLaunch,
  decodeLaunchConfig,
  decodePlatformTreasury,
  decodeQuotePolicy,
  decodeRegistryEntry,
  QUOTE_POLICY,
  REGISTRY_ENTRY,
  type BuyerPosition,
  type FloorPool,
  type HookConfig,
  type Launch,
  type LaunchConfig,
  type PlatformTreasury,
  type QuotePolicy,
  type RegistryEntry,

  

  
  decodeStairState,
  type StairState,
} from './layouts.ts';
import {
  DEFAULT_PROGRAM_IDS,
  findBuyerPosition,
  findFloorPool,
  findHookConfig,
  findPlatformTreasury,
  findTeamTreasury,
  findLaunch,
  findLaunchConfig,
  findQuotePolicy,
  findRegistryEntry,
  type ProgramIds,
  findStairState,
} from './pda.ts';

import type { Address } from '@solana/kit';

function ownedBy(a: RawAccount | null, program: Address): RawAccount | null {
  return a && a.owner === program ? a : null;
}

async function readOwned<T>(src: AccountSource, address: Address, program: Address, decode: (d: Uint8Array) => T): Promise<AtSlot<T | null>> {
  const { slot, account } = await src.getAccount(address);
  const a = ownedBy(account, program);
  return { slot, value: a ? decode(a.data) : null };
}

function decodeAll<T>(rows: { address: Address; account: RawAccount }[], layout: LayoutDef, decode: (d: Uint8Array) => T): [Address, T][] {
  const out: [Address, T][] = [];
  for (const { address, account } of rows) {
    if (!hasDiscriminator(layout, account.data)) continue;
    out.push([address, decode(account.data)]);
  }
  return out;
}
// ---- FLOOR --------------------------------------------------------------------------------------

export async function getHookConfig(src: AccountSource, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<HookConfig | null>> {
  return readOwned(src, (await findHookConfig(ids)).address, ids.floorHook, decodeHookConfig);
}

/**
 *  the program-owned platform treasury (`["platform_treasury"]` under
 * `floor_hook`) — its governance and any pending handover. `null` before `initialize_hook_config` has run.
 */
export async function getPlatformTreasury(src: AccountSource, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<PlatformTreasury | null>> {
  return readOwned(src, (await findPlatformTreasury(ids)).address, ids.floorHook, decodePlatformTreasury);
}

/**
 *  the program-owned TEAM treasury (`["team_treasury"]` under
 * `floor_hook`, `PlatformTreasury`'s layout) — its governance and any pending handover. `null` until
 * `initialize_team_treasury` has run; until then every sweep skips its team leg and the pool keeps `team_owed`.
 */
export async function getTeamTreasury(src: AccountSource, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<PlatformTreasury | null>> {
  return readOwned(src, (await findTeamTreasury(ids)).address, ids.floorHook, decodePlatformTreasury);
}

/**
 *  presence for the FLOOR quote roster entry.
 * `null` means "not on the FLOOR roster" — and wSOL is NEVER on it: it is admitted by construction
 * with compiled-in terms (`quote_policy.rs`: `list_quote` refuses it `QuoteAlreadyListed`), so a
 * `null` for wSOL is the expected answer, not a missing listing. `retired === 1` blocks NEW pools
 * only; there is no un-retire. Owner, length and discriminator are all checked, as `getQuoteMint`
 * does for the AMM's entry: a lamport at the address never means presence.
 */
export async function getQuotePolicy(src: AccountSource, quoteMint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<QuotePolicy | null>> {
  const { slot, account } = await src.getAccount((await findQuotePolicy(quoteMint, ids)).address);
  const a = ownedBy(account, ids.floorHook);
  const present = a && a.data.length === 8 + QUOTE_POLICY.size && hasDiscriminator(QUOTE_POLICY, a.data);
  return { slot, value: present ? decodeQuotePolicy(a.data) : null };
}

export async function getFloorPool(src: AccountSource, ammPool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<FloorPool | null>> {
  return readOwned(src, (await findFloorPool(ammPool, ids)).address, ids.floorHook, decodeFloorPool);
}

/**
 * A staircase pool's `StairState`
 * (`["stair", floor_pool]` under `floor_hook`), or `null` — on every pool without a staircase, and on a staircase launch
 * before `prepare_positions` / its graduating buy created it. `floorPool` is the `FloorPool` ADDRESS (`findFloorPool(amm)`).
 * The ratchet of a graduated staircase pool needs it (`buildRatchet({ stairState })`).
 */
export async function getStairState(src: AccountSource, floorPool: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<StairState | null>> {
  return readOwned(src, (await findStairState(floorPool, ids)).address, ids.floorHook, decodeStairState);
}

export async function getLaunchConfig(src: AccountSource, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<LaunchConfig | null>> {
  return readOwned(src, (await findLaunchConfig(ids)).address, ids.floorLaunch, decodeLaunchConfig);
}

export async function getLaunch(src: AccountSource, mint: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<Launch | null>> {
  return readOwned(src, (await findLaunch(mint, ids)).address, ids.floorLaunch, decodeLaunch);
}

export async function getBuyerPosition(src: AccountSource, launch: Address, buyer: Address, ids: ProgramIds = DEFAULT_PROGRAM_IDS): Promise<AtSlot<BuyerPosition | null>> {
  const { slot, account } = await src.getAccount((await findBuyerPosition(launch, buyer, ids)).address);
  const a = ownedBy(account, ids.floorLaunch);
  return { slot, value: a && hasDiscriminator(BUYER_POSITION, a.data) ? decodeBuyerPosition(a.data) : null };
}

/**
 * One launch as a multi-source reader merges it:
 * keyed `(chainKey, id)`, where `chainKey` is `solana:<genesis32>` and `id` is the MINT address
 * (the one key a Solana launch is known by everywhere: the Launch PDA and the registry entry
 * both derive from or point at it).
 * ⛔ Name and symbol are chain-only; they are not in these accounts (Token-2022 metadata carries
 *    them) and this row does not invent them.
 */
export interface LaunchRow {
  readonly chainKey: string;
  readonly id: Address;
  readonly registryIndex: bigint;
  readonly launch: Address;
  readonly mint: Address;
  readonly ammPool: Address;
  readonly mode: number;
  readonly createdSlot: bigint;
  /** `null` when the Launch account was not readable (it never should be, once registered). */
  readonly state: Launch | null;
}

/**
 * Every launch, newest first, over `RegistryEntry`. Two strategies, both exact:
 * - `mode: 'index'` (default, browser-safe): read `LaunchConfig.registry_count`, then entries
 *   `[count-1 … count-limit]` by PDA with `getMultipleAccounts`; no `getProgramAccounts`.
 * - `mode: 'scan'` (server-side): one `getProgramAccounts` on the discriminator.
 * `nextBefore` is where the next page starts (pass it as `before`), or `null` at the end. In `index` mode it is the
 * lowest registry index this page PROBED, so an entry that is missing or unreadable never ends paging early: a page can
 * hold fewer than `limit` rows and still have a next one.
 * ⛔ The listing is UNFILTERED, with the risk stated: anyone can create a launch, with any name and symbol.
 */
export async function listLaunches(
  src: AccountSource,
  chainKey: string,
  opts: { before?: bigint; limit?: number; mode?: 'index' | 'scan'; withState?: boolean } = {},
  ids: ProgramIds = DEFAULT_PROGRAM_IDS,
): Promise<AtSlot<LaunchRow[]> & { nextBefore: bigint | null }> {
  if (!/^solana:[1-9A-HJ-NP-Za-km-z]{32}$/.test(chainKey)) throw new Error(`listLaunches: chainKey must be solana:<genesis32>, got ${chainKey}`);
  const limit = opts.limit ?? 50;
  let entries: RegistryEntry[];
  let slot: bigint;
  let nextBefore: bigint | null;
  if (opts.mode === 'scan') {
    const r = await src.getProgramAccounts(ids.floorLaunch, [
      { memcmp: { offset: 0, bytes: REGISTRY_ENTRY.discriminator } },
      { dataSize: 8 + REGISTRY_ENTRY.size },
    ]);
    slot = r.slot;
    entries = decodeAll(r.accounts, REGISTRY_ENTRY, decodeRegistryEntry)
      .map(([, e]) => e)
      .filter((e) => opts.before === undefined || e.index < opts.before)
      .sort((a, b) => (a.index < b.index ? 1 : a.index > b.index ? -1 : 0))
      .slice(0, limit);
    const last = entries[entries.length - 1];
    nextBefore = entries.length === limit && last && last.index > 0n ? last.index : null;
  } else {
    const cfg = await getLaunchConfig(src, ids);
    if (!cfg.value) throw new Error('listLaunches: LaunchConfig is not initialized on this cluster');
    const top = opts.before === undefined || opts.before > cfg.value.registryCount ? cfg.value.registryCount : opts.before;
    const indices: bigint[] = [];
    for (let i = top - 1n; i >= 0n && indices.length < limit; i--) indices.push(i);
    const lowest = indices[indices.length - 1];
    nextBefore = lowest !== undefined && lowest > 0n ? lowest : null;
    const addrs = await Promise.all(indices.map(async (i) => (await findRegistryEntry(i, ids)).address));
    const r = await src.getMultipleAccounts(addrs);
    slot = r.slot < cfg.slot ? r.slot : cfg.slot;
    entries = [];
    r.accounts.forEach((a, i) => {
      const owned = ownedBy(a, ids.floorLaunch);
      if (owned && hasDiscriminator(REGISTRY_ENTRY, owned.data)) {
        const e = decodeRegistryEntry(owned.data);
        if (e.index === indices[i]) entries.push(e);
      }
    });
  }
  let states: (Launch | null)[] = entries.map(() => null);
  if (opts.withState !== false && entries.length > 0) {
    const r = await src.getMultipleAccounts(entries.map((e) => e.launch));
    slot = r.slot < slot ? r.slot : slot;
    states = r.accounts.map((a) => {
      const owned = ownedBy(a, ids.floorLaunch);
      return owned ? decodeLaunch(owned.data) : null;
    });
  }
  return {
    slot,
    nextBefore,
    value: entries.map((e, i) => ({
      chainKey,
      id: e.mint,
      registryIndex: e.index,
      launch: e.launch,
      mint: e.mint,
      ammPool: e.ammPool,
      mode: e.mode,
      createdSlot: e.createdSlot,
      state: states[i] ?? null,
    })),
  };
}

/** The named `stateView` object: Carpenter's reads, then FLOOR's. */
export const stateView = {
  ...carpenterStateView,
  getHookConfig,
  getQuotePolicy,
  getFloorPool,
  getLaunchConfig,
  listLaunches,
  getLaunch,
  getBuyerPosition,
} as const;

export async function getHolderPayoutInputs(src: AccountSource, a: {
  owner: Address; mint: Address; floorPool: Address; ids?: ProgramIds;
}) {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const { findHolderBook, findHolderRecord } = await import('./holders.ts');
  const { findAta } = await import('@floorsupply/carpenter-sdk');
  const { decodeHolderBook, decodeHolderRecord } = await import('./layouts.ts');
  const tokenAccount = (await findAta(a.owner, a.mint, TOKEN_2022_PROGRAM_ID)).address;
  const [book, record] = await Promise.all([findHolderBook(a.mint, ids), findHolderRecord(a.mint, tokenAccount, ids)]);
  const { slot, accounts } = await src.getMultipleAccounts([book.address, record.address, a.floorPool, tokenAccount]);
  const b = ownedBy(accounts[0] ?? null, ids.floorHolders);
  const r = ownedBy(accounts[1] ?? null, ids.floorHolders);
  const p = ownedBy(accounts[2] ?? null, ids.floorHook);
  if (!b || !r || !p) return { slot, value: null };
  const bookState = decodeHolderBook(b.data);
  const recordState = decodeHolderRecord(r.data);
  const pool = decodeFloorPool(p.data);
  if (bookState.mint !== a.mint || bookState.floorPool !== a.floorPool || bookState.quoteMint !== pool.quoteMint
    || pool.baseMint !== a.mint || recordState.mint !== a.mint || recordState.tokenAccount !== tokenAccount
    || recordState.owner !== a.owner) return { slot, value: null };
  const t = ownedBy(accounts[3] ?? null, TOKEN_2022_PROGRAM_ID);
  const token = t ? decodeTokenAccount(t.data) : null;
  const liveBalance = token?.mint === a.mint ? token.amount : 0n;
  return { slot, value: { book: b.data, record: r.data, floorPool: p.data, liveBalance,
    ammPool: pool.ammPool, quoteMint: pool.quoteMint, tokenAccount, recordAddress: record.address } };
}
