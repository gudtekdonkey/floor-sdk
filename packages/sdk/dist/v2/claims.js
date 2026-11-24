

import { addressBytes, base58Encode } from "../codec.js";
import { computeUnitLimitInstruction, computeUnitPriceInstruction } from "../solana/instruction.js";

import { PACKET_DATA_SIZE, compileV0Message, transactionSize } from "../solana/tx.js";

import { decodeTeamSlot } from "../solana/floor/accounts.js";
import { CLAIM_CREATOR, CLAIM_CREATOR_REF, CLAIM_SHARE_BOUGHT, CLAIM_TEAM_BUY, CLAIM_TEAM_SLOT, CLAIM_TRADER_REF } from "../solana/floor/instructions.js";
import { FLOOR_AMM_PROGRAM_ID, configPda, refOwedPda, refOwedVault } from "../solana/floor/program.js";
import { TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, WSOL_MINT, associatedTokenAddress } from "../solana/pda.js";
import { CREATOR_BUY_LOCK_SECS, MAX_MEMBERS, splitTeamBuyV2 } from "../math/v2/team.js";
import { holderSync, holderTake } from "../math/v2/holders.js";
import { FloorMathError } from "../math/index.js";
import { decodeHolderBook, decodeHolderPosition, decodeLaunchV2 } from "./layouts.js";
import { CLAIM_HOLDER, claimInstructionV2 } from "./solana.js";

export const MAX_TX_ACCOUNT_LOCKS_V2 = 64;

export const CLAIM_COMPUTE_UNIT_LIMIT_V2 = 1_400_000;

const MEMBER_IN = 2;
const KIND_WALLET = 0;

export function teamBuyDueV2(createdAt        , now        )          {
  const c = createdAt < 0n ? 0n : createdAt;
  const due = c + CREATOR_BUY_LOCK_SECS > 0xffff_ffff_ffff_ffffn ? 0xffff_ffff_ffff_ffffn : c + CREATOR_BUY_LOCK_SECS;
  return now >= due;
}

export function holderOwedNowV2(p                                                                            )         {
  if (!p.positionData) return 0n;
  const pos = decodeHolderPosition(p.positionData)       ;
  const book = decodeHolderBook(p.bookData)       ;
  if (pos.launch !== book.launch) throw new RangeError("this HolderPosition is not the book's launch");
  const b = { index: book.index          , w: book.w          , carry: book.carry          , bookedTotal: book.bookedTotal          , paidTotal: book.paidTotal           };
  const q = { amount: pos.amount          , owed: pos.owed          , indexSnap: pos.indexSnap          , optedOut: false };
  holderSync(b, q, p.balance);
  return holderTake(b, q);
}

export const claimBitsV2 = (o        )         =>
  (o.creator > 0n ? CLAIM_CREATOR : 0) | (o.creatorRef > 0n ? CLAIM_CREATOR_REF : 0) | (o.teamSlot > 0n ? CLAIM_TEAM_SLOT : 0) |
  (o.teamBuy > 0n ? CLAIM_TEAM_BUY : 0) | (o.shareBought > 0n ? CLAIM_SHARE_BOUGHT : 0) | (o.holder > 0n ? CLAIM_HOLDER : 0);
const quoteOf = (o        )         => o.creator + o.creatorRef + o.teamSlot + o.holder;

export function launchOwedV2(p

 )               {
  const a = decodeLaunchV2(p.launchData);
  const owed         = { creator: 0n, creatorRef: 0n, teamSlot: 0n, teamBuy: 0n, shareBought: 0n, holder: p.holderOwed ?? 0n };
  const places            = Array(MAX_MEMBERS).fill(false);
  if (a.creator === p.wallet) {
    owed.creator = a.creatorOwed;
    places[0] = true;
  }
  (a.creatorRef            ).forEach((r, i) => {
    if (r === p.wallet) owed.creatorRef += a.creatorRefOwed[i];
  });
  if (p.teamSlot) {
    const t = decodeTeamSlot(p.teamSlot.data);
    if (t.launch !== p.launch) throw new RangeError("this TeamSlot is not the launch's");
    const k = addressBytes(p.wallet);
    const i = (t.members         ).findIndex((m) => m.state === MEMBER_IN && m.kind === KIND_WALLET && (m.key              ).every((x, j) => x === k[j]));
    if (i >= 0) {
      owed.teamSlot = a.slotOwed[i];
      places[i] = true;
    }
  }
  const createdAt = a.createdAt < 0n ? 0n : (a.createdAt          );
  let buy           = a.teamBuyOwed.slice();
  const due = teamBuyDueV2(a.createdAt, p.now);
  if (a.creatorLocked > 0n) {
    try {
      const split = splitTeamBuyV2(a.creatorLocked, a.slotWeightPpb, createdAt, p.now);
      buy = buy.map((x, i) => x + split[i]);
    } catch (e) {
      if (!(e instanceof FloorMathError && e.code === "NotDue")) throw e;
    }
  }
  for (let i = 0; i < MAX_MEMBERS; i++) {
    if (!places[i]) continue;
    if (due) owed.teamBuy += buy[i];
    owed.shareBought += a.slotBoughtOwed[i];
  }
  return {
    launch: p.launch, quoteMint: a.quoteMint, quoteTokenProgram: a.quoteTokenProgram, quoteVault: a.quoteVault, baseMint: a.baseMint,
    tokenVault: a.tokenVault, teamSlot: p.teamSlot?.address ?? null, owed,
  };
}

function locksOf(payer        , ixs               )         {
  const s = new Set        ([payer]);
  for (const ix of ixs) {
    s.add(ix.programAddress);
    for (const a of ix.accounts) s.add(a.address);
  }
  return s.size;
}

export function planClaimAllV2(p

       

       )              {
  const sender = p.sender ?? p.wallet;
  const skipped           = [];

  const byQuote = new Map               ();
  const group = (q        , tp        )        => {
    let g = byQuote.get(q);
    if (!g) byQuote.set(q, (g = { tp, classes: new Map(), refs: 0n }));
    return g;
  };
  const classOf = (o        )         => (o.teamBuy > 0n || o.shareBought > 0n ? 1 : 0) | (o.holder > 0n ? 2 : 0);
  for (const l of p.launches) {
    if (claimBitsV2(l.owed) === 0) {
      skipped.push(l.launch);
      continue;
    }
    const g = group(l.quoteMint, l.quoteTokenProgram);
    const c = classOf(l.owed);
    if (!g.classes.has(c)) g.classes.set(c, []);
    g.classes.get(c) .push(l);
  }
  for (const r of p.refOwed ?? []) {
    if (r.owed === 0n) {
      skipped.push(`ref_owed:${r.quoteMint}`);
      continue;
    }
    const known = byQuote.get(r.quoteMint);
    if (!known && !r.quoteTokenProgram) throw new RangeError(`the RefOwed in ${r.quoteMint} needs its quote's token program`);
    group(r.quoteMint, known?.tp ?? r.quoteTokenProgram ).refs = r.owed;
  }
  const pid = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  const budget = ()                => [computeUnitLimitInstruction(p.computeUnitLimit ?? CLAIM_COMPUTE_UNIT_LIMIT_V2), ...(p.microLamports !== undefined ? [computeUnitPriceInstruction(p.microLamports)] : [])];
  const fits = (t           )          => t.size <= PACKET_DATA_SIZE && t.locks <= MAX_TX_ACCOUNT_LOCKS_V2;
  const txs              = [];
  for (const [quoteMint, g] of byQuote) {
    const unwrap = (p.unwrapSol ?? true) && quoteMint === WSOL_MINT && g.tp === TOKEN_PROGRAM_ID;
    const build = (ls                , refs        )            => {
      const types = ls.reduce((a, l) => a | claimBitsV2(l.owed), 0) | (refs > 0n ? CLAIM_TRADER_REF : 0);
      const tok = (types & (CLAIM_TEAM_BUY | CLAIM_SHARE_BOUGHT)) !== 0;
      const ro = refOwedPda(p.wallet, quoteMint, pid)[0];
      const claim = claimInstructionV2({
        sender, wallet: p.wallet, quoteMint, quoteTokenProgram: g.tp, destination: associatedTokenAddress(p.wallet, quoteMint, g.tp),
        config: configPda(pid)[0], refOwed: refs > 0n ? ro : null, refOwedVault: refs > 0n ? refOwedVault(ro, quoteMint, g.tp) : null, types,
        launches: ls.map((l)                => ({
          launch: l.launch, quoteVault: l.quoteVault, teamSlot: l.teamSlot, baseMint: l.baseMint,
          ...(tok ? { teamBuy: { baseMint: l.baseMint, tokenVault: l.tokenVault, walletBase: associatedTokenAddress(p.wallet, l.baseMint, TOKEN_2022_PROGRAM_ID) } } : {}),
        })),
        unwrap, quoteIsLaunch: p.launchQuotes?.includes(quoteMint) ?? false, programId: p.programId, hookProgramId: p.hookProgramId,
      });
      const instructions = [...budget(), claim];
      const message = compileV0Message({ payer: sender, instructions, recentBlockhash: p.recentBlockhash, lookupTables: p.lookupTables });
      return {
        quoteMint, instructions, message, size: transactionSize(message), locks: locksOf(sender, instructions), launches: ls.map((l) => l.launch),
        refOwed: refs > 0n, types, unwrap, amount: ls.reduce((a, l) => a + quoteOf(l.owed), 0n) + refs, tokens: ls.reduce((a, l) => a + l.owed.teamBuy + l.owed.shareBought, 0n),
      };
    };
    const alone = (ls                , refs        , what        )            => {
      const t = build(ls, refs);
      if (!fits(t)) throw new RangeError(`${what} alone does not fit one transaction`);
      return t;
    };
    let refs = g.refs;
    for (const c of [...g.classes.keys()].sort()) {
      let cur                 = [];
      let last                   = null;
      for (const l of g.classes.get(c) ) {
        const t = build([...cur, l], refs);
        if (fits(t)) {
          cur.push(l);
          last = t;
          continue;
        }
        if (cur.length === 0 && refs > 0n) {

          txs.push(alone([], refs, "the referral claim"));
          refs = 0n;
        } else if (cur.length === 0) throw new RangeError(`launch ${l.launch} alone does not fit one transaction`);
        else {
          txs.push(last );
          refs = 0n;
        }
        cur = [l];
        last = alone(cur, 0n, `launch ${l.launch}`);
      }
      if (last) {
        txs.push(last);
        refs = 0n;
      }
    }
    if (refs > 0n) txs.push(alone([], refs, "the referral claim"));
  }
  return { transactions: txs, skipped };
}

export function claimLaunchesPerTxV2(p                                                                                                                 = {})                       {
  const key = (i        )         => {
    const b = new Uint8Array(32);
    b[0] = 0xf1;
    b[1] = i & 0xff;
    b[2] = i >> 8;
    return base58Encode(b);
  };
  const quoteMint = p.quoteMint ?? key(0xfff0);
  const quoteTokenProgram = p.quoteTokenProgram ?? TOKEN_PROGRAM_ID;
  const measure = (o                 )         => {
    const launches = Array.from({ length: 64 }, (_, i)               => ({
      launch: key(i), quoteMint, quoteTokenProgram, quoteVault: key(0x100 + i), baseMint: key(0x200 + i), tokenVault: key(0x300 + i), teamSlot: key(0x400 + i),
      owed: { creator: 0n, creatorRef: 0n, teamSlot: 0n, teamBuy: 0n, shareBought: 0n, holder: 0n, ...o },
    }));
    const plan = planClaimAllV2({ wallet: key(0xfff1), launches, recentBlockhash: key(0xfff2), lookupTables: p.lookupTables, microLamports: p.microLamports, programId: p.programId, hookProgramId: p.hookProgramId });
    return plan.transactions[0].launches.length;
  };
  return { plain: measure({ creator: 1n, teamSlot: 1n }), tokens: measure({ creator: 1n, teamBuy: 1n }), holder: measure({ holder: 1n }), both: measure({ teamBuy: 1n, holder: 1n }) };
}
