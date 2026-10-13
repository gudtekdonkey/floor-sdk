/**
 * FLOOR's deployments on Solana: `floor_hook` / `floor_launch` / `floor_holders` / `floor_vesting` per cluster, keyed by
 * CAIP-2 id, with FLOOR's singleton accounts, the deployment address lookup table and the hook roster's quote terms.
 * `carpenter_amm`'s rows are `@carpenter/sdk`'s `deployments.json`.
 *
 * ⛔ `deploymentFor` returns the JOINED row — Carpenter's row and FLOOR's for the same cluster (`programs.carpenterAmm`
 *    beside FLOOR's ids) — because every FLOOR builder names the AMM it is hooked on. It throws BY NAME when EITHER file
 *    lacks the cluster: a missing row must never read as a cluster with no launches.
 * ⛔ Program ids come from these files, never a constant in an app. A row may name program ids other than the shipped
 *    IDLs' `address` (another cluster's deployment); every builder, PDA and read takes the ids it is given
 *    (`programIdsFromDeployment(row)`). The devnet and local rows are the ones generation pins to the IDLs.
 * ⛔ `alt` is FLOOR's deployment lookup table (FLOOR's programs, configs and PDAs beside the AMM's); read its contents
 *    from the chain.
 * ⛔ A `quotes` entry is a RECORD of a listing, never an authority: the rosters on chain are.
 */

import {
  DEPLOYMENTS as CARPENTER_DEPLOYMENTS,
  type ClusterDeployment as CarpenterClusterDeployment,
  type DeploymentsFile as CarpenterDeploymentsFile,
} from '@carpenter/sdk';

import { DEPLOYMENTS } from './generated/deployments.ts';

export type { AnchorIdl, AnchorIdlAccountItem, AnchorIdlInstruction } from '@carpenter/sdk';
export { DEPLOYMENTS };

/**
 * FLOOR's four deployable programs, by their Anchor crate name (the IDL file stem).
 */
export type ProgramName = 'floor_hook' | 'floor_launch' | 'floor_holders' | 'floor_vesting';

export const PROGRAM_NAMES: readonly ProgramName[] = ['floor_hook', 'floor_launch', 'floor_holders', 'floor_vesting'];

/** One cluster's FLOOR row in this package's `deployments.json`. */
export interface FloorClusterDeployment {
  readonly name: 'mainnet' | 'devnet' | 'testnet' | 'local';
  readonly chainKey: string | null;
  readonly programs: {
    readonly floorHook: string;
    readonly floorLaunch: string;
    readonly floorHolders: string;
    readonly floorVesting: string;
  };
  readonly accounts: {
    readonly hookConfig: string;
    readonly launchConfig: string;
    /** The team treasury PDA, where a row records it. */
    readonly teamTreasury?: string;
  };
  /** The per-deployment address lookup table; `null` until created. */
  readonly alt: string | null;
  /** Each deployed ProgramData's EXECUTABLE hash; `null` until recorded. */
  readonly programDataHash: {
    readonly floorHook: string | null;
    readonly floorLaunch: string | null;
    readonly floorHolders: string | null;
    readonly floorVesting: string | null;
  };
  /** The quote mints listed on the HOOK's roster (`["quote", mint]`), with their terms (raw units, decimal strings). */
  readonly quotes?: Readonly<Record<string, HookQuoteListingRecord>>;
}

/** One quote on the hook's roster: the listing state, the terms and the hook's `QuotePolicy` account. */
export interface HookQuoteListingRecord {
  /** `listed` on both rosters, `retired` on both, or the split the script could only half-finish. */
  readonly state: 'listed' | 'retired' | 'amm-only' | 'hook-only';
  readonly decimals: number;
  readonly tokenProgram: string;
  readonly terms: { readonly minBidFloor: string; readonly minBidCeil: string; readonly maxBountyCeil: string; readonly minBid: string; readonly maxBounty: string };
  readonly hook: { readonly policy: string };
}

export interface DeploymentsFile {
  readonly version: 1;
  readonly clusters: Readonly<Record<string, FloorClusterDeployment>>;
}

/** One listed quote, both rosters' records joined: the AMM's `QuoteMint` entry and caps beside the hook's terms. */
export interface QuoteListingRecord extends HookQuoteListingRecord {
  readonly caps: number;
  readonly capNames: readonly string[];
  readonly amm: { readonly entry: string; readonly listedSlot: string | null };
}

/** The JOINED row: Carpenter's and FLOOR's for one cluster. */
export interface ClusterDeployment {
  readonly name: 'mainnet' | 'devnet' | 'testnet' | 'local';
  readonly chainKey: string | null;
  readonly programs: { readonly carpenterAmm: string } & FloorClusterDeployment['programs'];
  readonly accounts: CarpenterClusterDeployment['accounts'] & FloorClusterDeployment['accounts'];
  readonly alt: string | null;
  readonly programDataHash: { readonly carpenterAmm: string | null } & FloorClusterDeployment['programDataHash'];
  readonly quotes?: Readonly<Record<string, QuoteListingRecord>>;
}

/**
 * Join one cluster's Carpenter row and FLOOR row. ⛔ Refuses rows of two different clusters by name (a `chainKey` or
 * `name` mismatch), and a quote the hook's roster records that the AMM's does not (the hook refuses a listing the AMM
 * has not made, so such a record is a half-finished script run the joined shape cannot express — it keeps the hook's
 * half and reads `caps` 0, `capNames` [] and an empty `amm`).
 */
export function joinDeployment(carpenter: CarpenterClusterDeployment, floor: FloorClusterDeployment): ClusterDeployment {
  if (carpenter.chainKey !== floor.chainKey || carpenter.name !== floor.name) {
    throw new Error(`joinDeployment: the Carpenter row is ${carpenter.name} (${carpenter.chainKey}) but the FLOOR row is ${floor.name} (${floor.chainKey})`);
  }
  let quotes: Record<string, QuoteListingRecord> | undefined;
  if (floor.quotes) {
    quotes = {};
    for (const [mint, q] of Object.entries(floor.quotes)) {
      const a = carpenter.quotes?.[mint];
      quotes[mint] = {
        state: q.state, decimals: q.decimals, tokenProgram: q.tokenProgram,
        caps: a?.caps ?? 0, capNames: a?.capNames ?? [], terms: q.terms,
        amm: { entry: a?.amm.entry ?? '', listedSlot: a?.amm.listedSlot ?? null }, hook: { policy: q.hook.policy },
      };
    }
  }
  return {
    name: floor.name,
    chainKey: floor.chainKey,
    programs: { carpenterAmm: carpenter.programs.carpenterAmm, ...floor.programs },
    accounts: { ...carpenter.accounts, ...floor.accounts },
    alt: floor.alt,
    programDataHash: { carpenterAmm: carpenter.programDataHash.carpenterAmm, ...floor.programDataHash },
    ...(quotes ? { quotes } : {}),
  };
}

/**
 * The JOINED deployment for a cluster key, over both shipped files by default. Throws BY NAME on a key either file
 * lacks: a missing row must never read as a cluster with no launches.
 */
export function deploymentFor(clusterKey: string, file: DeploymentsFile = DEPLOYMENTS, carpenterFile: CarpenterDeploymentsFile = CARPENTER_DEPLOYMENTS): ClusterDeployment {
  const row = file.clusters[clusterKey];
  if (!row) {
    throw new Error(`@floorsupply/sdk-solana: no deployment for cluster "${clusterKey}" (known: ${Object.keys(file.clusters).join(', ')})`);
  }
  const amm = carpenterFile.clusters[clusterKey];
  if (!amm) {
    throw new Error(`@floorsupply/sdk-solana: @carpenter/sdk has no deployment for cluster "${clusterKey}" (known: ${Object.keys(carpenterFile.clusters).join(', ')})`);
  }
  return joinDeployment(amm, row);
}
