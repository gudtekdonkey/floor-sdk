/**
 * Trader referrals, as far as a trade needs them.
 * - The LINK is its own transaction, sent FIRST and signed with the trade in ONE wallet prompt:
 *   `[CU limit, Ed25519 verify, link_trader]` — {@link buildLinkTrader}, {@link planLinkThenTrade}. The Ed25519 verify
 *   checks a FLOOR attestor's signature over the link message, which a FLOOR server provides; the SDK cannot make it.
 *   The link opens the trader's wSOL bucket, so the wSOL sale trade that follows it in the same batch books its
 *   referral slices ({@link saleRefPrefix}).
 * - The SALE PREFIX is `buildBuy` / `buildSellBack`'s `traderRef` option (`tx/builders.ts`), built here.
 * - The SWAP slice needs nothing: `buildSwap` always carries the signer's bucket (`floorSwapHookSlice`).
 * ⚠ The sale prefix is safe to send whatever the bucket's state: a bucket not opened yet books nothing and the trade
 *   proceeds. The link opens the wSOL bucket alone, so a token-quoted sale trade books slices only once the trader's
 *   bucket in THAT quote exists.
 */
import type { Address, Instruction } from '@solana/kit';

import {
  addressToBytes,
  WSOL_MINT,
  findEventAuthority,
  compileSendable,
  type LookupTableContents,
  type V0MessageArgs,
} from '@floorlaunchpad/carpenter-sdk';
import {
  findHookConfig,
  findLaunch,
  findLaunchAuthority,
  findLaunchConfig,
  findTraderBucket,
  findTraderLink,
  findTraderRefSale,
  DEFAULT_PROGRAM_IDS,
  type ProgramIds,
} from '../pda.ts';
import { finalize, type BuiltTransaction } from './common.ts';
import { linkTraderIx, type SaleRefPrefix } from './instructions.ts';
import { INSTRUCTIONS_SYSVAR_ID } from '../constants.ts';

/** Floor-types `TRADER_LINK_PREFIX`. */
export const TRADER_LINK_PREFIX = 'FLOOR_TRADER_REF_LINK_V1';
/** Floor-types `TRADER_LINK_MSG_LEN` = 24 + 32 + 1 + 32 × 4 + 8. */
export const TRADER_LINK_MSG_LEN = 193;
/** The Ed25519 precompile. */
export const ED25519_PROGRAM_ID = 'Ed25519SigVerify111111111111111111111111111' as Address;
/** The zero key: an absent tier. */
const ZERO = '11111111111111111111111111111111' as Address;

/** A FLOOR attestor's signed link for one wallet (served by FLOOR's API), decoded. */
export interface TraderLinkAttestation {
  tier1: Address;
  /** `null` = absent (written as the zero key). */
  tier2: Address | null;
  tier3: Address | null;
  deadline: bigint;
  /** The attestor's 64-byte Ed25519 signature over {@link traderLinkMessage}. */
  signature: Uint8Array;
}

/** The 193 bytes the attestor signs — floor-types `trader_link_message`, byte for byte. */
export function traderLinkMessage(a: {
  floorLaunch: Address; cluster: number; trader: Address; tier1: Address; tier2: Address | null; tier3: Address | null; deadline: bigint;
}): Uint8Array {
  const out = new Uint8Array(TRADER_LINK_MSG_LEN);
  let at = 0;
  const put = (b: Uint8Array) => {
    out.set(b, at);
    at += b.length;
  };
  put(new TextEncoder().encode(TRADER_LINK_PREFIX));
  put(addressToBytes(a.floorLaunch));
  put(Uint8Array.of(a.cluster));
  put(addressToBytes(a.trader));
  for (const t of [a.tier1, a.tier2 ?? ZERO, a.tier3 ?? ZERO]) put(addressToBytes(t));
  const d = new Uint8Array(8);
  new DataView(d.buffer).setBigInt64(0, a.deadline, true);
  put(d);
  if (at !== TRADER_LINK_MSG_LEN) throw new Error('traderLinkMessage: length');
  return out;
}

/**
 * The Ed25519 verify instruction `link_trader` reads, at the EXACT offsets the program pins (`link_trader.rs`
 * `EXPECTED_OFFSETS` = [48, 0xffff, 16, 0xffff, 112, 193, 0xffff]): one signature, the public key at 16, the signature at 48,
 * the message at 112, every index "this instruction".
 */
export function traderLinkVerifyIx(a: { attestor: Address; signature: Uint8Array; message: Uint8Array }): Instruction {
  if (a.signature.length !== 64) throw new Error('traderLinkVerifyIx: the signature is 64 bytes');
  if (a.message.length !== TRADER_LINK_MSG_LEN) throw new Error(`traderLinkVerifyIx: the message is ${TRADER_LINK_MSG_LEN} bytes`);
  const data = new Uint8Array(112 + TRADER_LINK_MSG_LEN);
  const v = new DataView(data.buffer);
  data[0] = 1; // one signature
  data[1] = 0; // padding
  [48, 0xffff, 16, 0xffff, 112, TRADER_LINK_MSG_LEN, 0xffff].forEach((x, i) => v.setUint16(2 + 2 * i, x, true));
  data.set(addressToBytes(a.attestor), 16);
  data.set(a.signature, 48);
  data.set(a.message, 112);
  return { programAddress: ED25519_PROGRAM_ID, accounts: [], data };
}

/**
 * The link transaction: `[CU limit, Ed25519 verify, link_trader]`. `attestor` and `cluster` are `LaunchConfig`'s (read
 * them; the program verifies against its own). The trader signs and pays the link's and the wSOL bucket's rent.
 * Refused here by name: a gap (tier 3 without tier 2), a repeat, a self-reference — the program's `BadTraderLinkShape`.
 */
export async function buildLinkTrader(a: {
  trader: Address;
  attestor: Address;
  cluster: number;
  attestation: TraderLinkAttestation;
  ids?: ProgramIds;
}): Promise<BuiltTransaction> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const { tier1, tier2, tier3, deadline, signature } = a.attestation;
  const set = [tier1, tier2, tier3].filter((t): t is Address => t != null && t !== ZERO);
  if (tier1 == null || tier1 === ZERO) throw new Error('buildLinkTrader: tier 1 is required (BadTraderLinkShape)');
  if ((tier2 == null || tier2 === ZERO) && tier3 != null && tier3 !== ZERO) throw new Error('buildLinkTrader: tier 3 without tier 2 (BadTraderLinkShape)');
  if (new Set(set).size !== set.length || set.includes(a.trader)) throw new Error('buildLinkTrader: a repeated tier or the trader itself (BadTraderLinkShape)');
  const [launchConfig, traderLink, wsolBucket, ev] = await Promise.all([
    findLaunchConfig(ids),
    findTraderLink(a.trader, ids),
    findTraderBucket(a.trader, WSOL_MINT, ids),
    findEventAuthority(ids.floorLaunch),
  ]);
  const message = traderLinkMessage({ floorLaunch: ids.floorLaunch, cluster: a.cluster, trader: a.trader, tier1, tier2, tier3, deadline });
  const verify = traderLinkVerifyIx({ attestor: a.attestor, signature, message });
  const main = linkTraderIx(
    {
      program: ids.floorLaunch,
      eventAuthority: ev.address,
      trader: a.trader,
      launchConfig: launchConfig.address,
      traderLink: traderLink.address,
      instructionsSysvar: INSTRUCTIONS_SYSVAR_ID,
      wsolMint: WSOL_MINT,
      wsolBucket: wsolBucket.address,
      floorHook: ids.floorHook,
    },
    { tier1, tier2: tier2 ?? ZERO, tier3: tier3 ?? ZERO, deadline },
  );
  return finalize('buildLinkTrader', 'linkTrader', [verify, main], main);
}

/**
 * The sale prefix's five accounts for `trader` on `launchMint` in `quoteMint` (default wSOL): the trader's bucket, the
 * launch's sale account, `["launch_authority"]`, `["hook_config"]`, `floor_hook`. A bucket (or the launch's sale account)
 * not opened yet books nothing and the trade proceeds.
 */
export async function saleRefPrefix(a: { trader: Address; launchMint: Address; quoteMint?: Address; ids?: ProgramIds }): Promise<SaleRefPrefix> {
  const ids = a.ids ?? DEFAULT_PROGRAM_IDS;
  const launch = (await findLaunch(a.launchMint, ids)).address;
  const [bucket, sale, la, hc] = await Promise.all([
    findTraderBucket(a.trader, a.quoteMint ?? WSOL_MINT, ids),
    findTraderRefSale(launch, ids),
    findLaunchAuthority(ids),
    findHookConfig(ids),
  ]);
  return { traderBucket: bucket.address, saleAccount: sale.address, launchAuthority: la.address, hookConfig: hc.address, floorHook: ids.floorHook };
}

/** One step of {@link planLinkThenTrade}, compiled when a `lifetime` is given (the `PlanStep` shape). */
export interface TraderPlanStep<S extends string> {
  readonly step: S;
  readonly tx: BuiltTransaction;
  readonly compiled?: { transaction: ReturnType<typeof compileSendable>['transaction']; bytes: number };
}

/**
 * `link` first (only while `linked` is false), then the trade's own
 * steps (`planBuy`'s `prepare` / `buy`, a sell-back, a swap), for ONE wallet prompt; the caller sends each in
 * order, CONFIRMS it, and stops at the first failure. A link that fails (expired attestation, already linked) stops the
 * batch before the trade — re-plan with `linked: true` and the trade goes alone. The trade steps are passed through
 * unchanged (compiled again with this `lifetime` / `lookupTables` when given): build them with `traderRef` set when the
 * trade is a wSOL sale trade, since the link opens that bucket.
 * ⛔ The per-launch lookup table is mandatory for a graduating buy: pass the deployment table AND the launch's own.
 */
export function planLinkThenTrade<S extends string>(a: {
  trader: Address;
  linked: boolean;
  link?: BuiltTransaction;
  trade: readonly { step: S; tx: BuiltTransaction }[];
  lookupTables?: readonly LookupTableContents[];
  lifetime?: V0MessageArgs['lifetime'];
}): TraderPlanStep<'link' | S>[] {
  if (!a.linked && !a.link) throw new Error('planLinkThenTrade: an unlinked trader needs `link` (buildLinkTrader)');
  const out: TraderPlanStep<'link' | S>[] = [];
  const push = (step: 'link' | S, tx: BuiltTransaction) => {
    const compiled = a.lifetime
      ? compileSendable(`planLinkThenTrade/${step}`, { feePayer: a.trader, instructions: tx.instructions, lifetime: a.lifetime, ...(a.lookupTables ? { lookupTables: a.lookupTables } : {}) })
      : undefined;
    out.push(compiled ? { step, tx, compiled } : { step, tx });
  };
  if (!a.linked) push('link', a.link!);
  for (const t of a.trade) push(t.step, t.tx);
  return out;
}
