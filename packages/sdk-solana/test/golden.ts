
import { address, type Address } from '@solana/kit';

import {
  addressToBytes,
  bytesToHex,
  WSOL_MINT,
  findAmmConfig,
  findApprovedHook,
  findBitmap,
  findEventAuthority,
  findMintExclusivity,
  findMintPools,
  findPoolIndexEntry,
  findPosition,
  findProgramData,
  findQuoteMint,
  findTickArray,
  findVault,
  TOKEN_2022_PROGRAM_ID,
  findAta,
  getSqrtPriceAtTick,
  getTickAtSqrtPrice,
} from '@carpenter/sdk';
import { SALT_BID, SALT_SEED } from '../src/constants.ts';
import { EVENT_NAMES, eventDiscriminator } from '../src/events.ts';
import { LAYOUTS } from '../src/layouts.ts';
import {
  type ProgramIds,
  findBuyerPosition,
  findFloorPool,
  findHookAuthority,
  findHookConfig,
  findLaunchAuthority,
  findLaunchConfig,
  findLockAuthority,
  findMintAuthority,
  findQuotePolicy,
  findRegistryEntry,
  findSaleVault,
} from '../src/pda.ts';
import { graduationAccounts } from '../src/predict.ts';
import {
  findExtraAccountMetas,
  findHolderBook,
  findHolderRecord,
  findHoldersAuthority,
  transferHookAccounts,
} from '../src/holders.ts';
import { openTick, tranchePriceX64 } from '../src/tickmath.ts';
import { everyInstruction, FIXTURE_BUYER, FIXTURE_STAIR_STEPS, fixtureLaunch, repoFile, specRent } from './helpers.ts';

/** JSON-safe: bigint → `"<n>n"`, Uint8Array → hex, else as is. */
export function toJson(v: unknown): unknown {
  if (typeof v === 'bigint') return `${v}n`;
  if (v instanceof Uint8Array) return bytesToHex(v);
  if (Array.isArray(v)) return v.map(toJson);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, toJson(x)]));
  return v;
}

export const GOLDEN_VERSION = 1;

/**
 * `fixtures/stock-mints/mints.json` → the `backed-xstock-8dp` row's mint (the Backed xStocks shape,
 * 8 dp) — READ from the record, never typed here: the fixture's keys are deterministic
 * (`sha256("<label>:mint")`) and `mints.json` is what the pinned Token-2022 accepted.
 */
export function stockFixtureMint(label = 'backed-xstock-8dp'): Address {
  const file = JSON.parse(repoFile('fixtures/stock-mints/mints.json')) as { mints: { label: string; mint: string }[] };
  const row = file.mints.find((m) => m.label === label);
  if (!row) throw new Error(`fixtures/stock-mints/mints.json has no ${label}`);
  return row.mint as Address;
}

/** The vector was recorded on devnet: every address is derived under devnet's program ids, never the default. */
export const GOLDEN_PROGRAM_IDS: ProgramIds = {
  carpenterAmm: address('Arv1M4LifBrWMywvxpeAGvh37aVHPjpXPCg7sacbe3Hy'),
  floorHook: address('6foted69N6wB1oJzsxccbXUxefBt3Hkn7tN2yHifXnTS'),
  floorLaunch: address('93VBXddbRj7py1PxGGCy1cAhPJQYNseca2bajaeqdnAG'),
  floorHolders: address('DShmCjpc34oRb3mqjLvp1fs1rYfY6ZLvj1G38d1s6XPZ'),
};

export async function computeGolden(): Promise<Record<string, unknown>> {
  const ids = GOLDEN_PROGRAM_IDS;
  const f = await fixtureLaunch(ids);
  const pool = f.state.ammPool;
  const hookAuthority = (await findHookAuthority(ids.floorHook)).address;
  const pdas: Record<string, { address: string; bump: number }> = {};
  const put = async (name: string, p: Promise<{ address: string; bump: number }>) => {
    const r = await p;
    pdas[name] = { address: r.address, bump: r.bump };
  };
  await put('amm_config', findAmmConfig(ids));
  await put('approved_hook[floor_hook]', findApprovedHook(ids.floorHook, ids));
  await put('hook_config', findHookConfig(ids));
  await put('launch_config', findLaunchConfig(ids));
  await put('hook_authority', findHookAuthority(ids.floorHook));
  await put('launch_authority', findLaunchAuthority(ids));
  await put('mint_authority', findMintAuthority(ids));
  await put('lock_authority', findLockAuthority(ids));
  await put('programdata[floor_hook]', findProgramData(ids.floorHook));
  await put('event_authority[carpenter_amm]', findEventAuthority(ids.carpenterAmm));
  await put('event_authority[floor_hook]', findEventAuthority(ids.floorHook));
  await put('event_authority[floor_launch]', findEventAuthority(ids.floorLaunch));
  await put('mint_pools[wsol]', findMintPools(WSOL_MINT, ids));
  await put('mint_excl[wsol]', findMintExclusivity(WSOL_MINT, ids));
  // The two quote ROSTER entries for the stock fixture `fixtures/stock-mints` names first
  // (`backed-xstock-8dp`, the Backed xStocks shape) — what `scripts/quote-admit.mjs` lists.
  const stock = stockFixtureMint();
  await put('quote_mint[backed-xstock-8dp]', findQuoteMint(stock, ids));
  await put('quote_policy[backed-xstock-8dp]', findQuotePolicy(stock, ids));
  await put('bitmap[fixture]', findBitmap(pool, ids));
  await put('vault0[fixture]', findVault(pool, WSOL_MINT, ids));
  await put('vault1[fixture]', findVault(pool, f.mint, ids));
  await put('pool_index[fixture]', findPoolIndexEntry(pool, ids));
  await put('floor_pool[fixture]', findFloorPool(pool, ids));
  await put('sale_vault[fixture]', findSaleVault(f.launch, ids));
  await put('buyer[fixture]', findBuyerPosition(f.launch, FIXTURE_BUYER, ids));
  await put('registry_entry[0]', findRegistryEntry(0n, ids));
  await put('registry_entry[258]', findRegistryEntry(258n, ids));
  await put('tick_array[fixture,-3600]', findTickArray(pool, -3600, ids));
  await put('tick_array[fixture,0]', findTickArray(pool, 0, ids));
  // No position is opened at these salts; the PDAs are pinned only.
  for (const [name, salt] of [['bid', SALT_BID], ['seed', SALT_SEED]] as const) {
    await put(`position[fixture,${name}]`, findPosition(pool, hookAuthority, salt, ids));
  }
  // under the floor_holders program id (`58nLaD9b…M7oE9`; regenerate the fixture with `npm run golden`).
  await put('holders_authority', findHoldersAuthority(ids));
  await put('event_authority[floor_holders]', findEventAuthority(ids.floorHolders));
  await put('holder_book[fixture]', findHolderBook(f.mint, ids));
  await put('extra_account_metas[fixture]', findExtraAccountMetas(f.mint, ids));
  const buyerAta = (await findAta(FIXTURE_BUYER, f.mint, TOKEN_2022_PROGRAM_ID)).address;
  await put('holder_record[fixture,buyer]', findHolderRecord(f.mint, buyerAta, ids));

  const g = await graduationAccounts({ launch: f.state, launchAddress: f.launch, rent: specRent, stairSteps: FIXTURE_STAIR_STEPS, ids });
  const instructions: Record<string, unknown> = {};
  for (const [key, [, ix]] of Object.entries(everyInstruction())) {
    instructions[key] = {
      program: ix.programAddress,
      data: bytesToHex(ix.data),
      accounts: ix.accounts.map((a) => ({ address: a.address, role: a.role })),
      names: ix.accountNames,
      args: ix.argNames,
    };
  }
  const p = tranchePriceX64(20_000n, 150n * 100n, 9);
  return toJson({
    version: GOLDEN_VERSION,
    programIds: ids,
    fixtureLaunch: { creator: f.state.creator, nonce: f.nonce, mint: f.mint, mintBytes: addressToBytes(f.mint), launch: f.launch, ammPool: pool, floorPool: f.state.floorPool, tokenVault: f.state.tokenVault, wsolVault: f.state.wsolVault, priceX64: p, openTick: openTick(p) },
    pdas,
    graduation: { names: g.names, accounts: g.accounts, tickArrayStarts: g.tickArrayStarts, rentLamports: g.rentLamports, maxGraduationLamports: g.maxGraduationLamports, rentModel: '(128 + len) × 5,080' },
    instructions,
    events: Object.fromEntries(Object.entries(EVENT_NAMES).map(([program, names]) => [program, Object.fromEntries(names.map((n) => [n, bytesToHex(eventDiscriminator(n))]))])),
    layouts: Object.fromEntries(LAYOUTS.map((l) => [l.name, { size: l.size, discriminator: bytesToHex(l.discriminator) }])),
    // the hook's six extra accounts for the fixture launch, the buyer's ATA ← the base vault.
    transferHookAccounts: await transferHookAccounts({ mint: f.mint, floorPool: f.state.floorPool, source: (await findVault(pool, f.mint, ids)).address, destination: buyerAta, ids }),
    tickMath: {
      sqrtAt: Object.fromEntries([-443_636, -69_081, -1, 0, 1, 6932, 89_231, 443_636].map((t) => [t, getSqrtPriceAtTick(t)])),
      tickAt: Object.fromEntries([4_295_048_016n, 1n << 64n, (1n << 64n) + 1n, 79_226_673_521_066_979_257_578_248_090n].map((s) => [String(s), getTickAtSqrtPrice(s)])),
    },
  }) as Record<string, unknown>;
}
