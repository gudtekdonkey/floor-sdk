

import { addressBytes } from "../codec.js";
import { isOnCurve } from "../ed25519.js";
import { R, RS, W, WS, instructionDiscriminator } from "../solana/instruction.js";

import { ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, WSOL_MINT, associatedTokenAddress, findProgramAddress } from "../solana/pda.js";

import { FLOOR_AMM_ERRORS, FLOOR_AMM_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, METADATA_PROGRAM_ID, configPda, floorEventAuthorityPda, genesisConvertBandPda, genesisRaisePda, launchPda, teamSlotPda, traderLinkPda, traderRefPda, unwrapPda } from "../solana/floor/program.js";
import { Writer } from "../solana/floor/borsh.js";
import { CLAIM_CREATOR, CLAIM_CREATOR_REF, CLAIM_EXTRA, CLAIM_KNOWN_WIDE, CLAIM_TEAM_BUY, CLAIM_TEAM_SLOT, CLAIM_TRADER_REF, CLAIM_SHARE_BOUGHT, MAX_NAME_LEN, MAX_SYMBOL_LEN, createMintAccounts, withdrawPlatformManyInstruction } from "../solana/floor/instructions.js";

import { MEMBERS_MAX, ROW_LEN } from "./layouts.js";
import { freshRegistryPrice } from "../registryPrice.js";

import { CLAIM_BITS, CLAIM_KNOWN_MASKS, PROGRAM_IDS } from "../generated/index.js";

export const FLOOR_HOOK_PROGRAM_ID         = PROGRAM_IDS.floorHook;

export const FLOOR_HOOK_PLACEHOLDER_ID = "824TastyTFiKGJ8HteV5NHDkqcAcGVAn4L9JT8ZSdAE4";
export function isFloorHookPlaceholder(id        )          {
  return id === FLOOR_HOOK_PLACEHOLDER_ID;
}

const pid = (c      )         => c?.programId ?? FLOOR_AMM_PROGRAM_ID;
const hid = (c      )         => c?.hookProgramId ?? FLOOR_HOOK_PROGRAM_ID;
const k = addressBytes;

export const holderBookPda = (launch        , hook = FLOOR_HOOK_PROGRAM_ID)      => findProgramAddress(["holder_book", k(launch)], hook);
export const holderPositionPda = (launch        , owner        , hook = FLOOR_HOOK_PROGRAM_ID)      => findProgramAddress(["holder", k(launch), k(owner)], hook);
export const deadMarkPda = (launch        , tokenAccount        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["dead", k(launch), k(tokenAccount)], p);

export const extraAccountMetasPda = (baseMint        , hook = FLOOR_HOOK_PROGRAM_ID)      => findProgramAddress(["extra-account-metas", k(baseMint)], hook);

const V2_ERROR_NAMES = [
  "NothingToSettle", "HolderPositionExists", "HolderPositionNotEmpty", "HolderBookMismatch", "DeadAccountNotDead",
  "HookAccountsMissing", "RowTooManyRanges", "RowNotFeasible",
]         ;

export const V2_ERROR_BASE = FLOOR_AMM_ERRORS.BurnTooLarge + 1;
export const FLOOR_AMM_ERRORS_V2                                                            = Object.freeze(
  Object.fromEntries(V2_ERROR_NAMES.map((n, i) => [n, V2_ERROR_BASE + i]))                                                   ,
);

export const FLOOR_HOOK_ERRORS = Object.freeze({
  NotTransferring: 0, WrongMint: 1, WrongProgram: 2, WrongAccount: 3, PositionNotWritable: 4, AlreadyOpen: 5, NotEmpty: 6, MetasExist: 7,
  NotLaunchSigner: 8, HookAuthoritySet: 9, Math: 10, OwnerNotImmutable: 11,

  NotTheOwnersAta: 12,

  OffCurveOwner: 13,
}         );

export const canHoldPosition = (owner        )          => isOnCurve(k(owner));

export const TRANSFER_HOOK_EXECUTE_DISCRIMINATOR = Uint8Array.from([105, 37, 101, 197, 75, 251, 102, 26]);

export function hookAccountsFor(p                                                                )                {
  const hook = hid(p);
  const launch = launchPda(p.baseMint, pid(p))[0];
  return [
    R(extraAccountMetasPda(p.baseMint, hook)[0]),
    R(launch),
    W(holderBookPda(launch, hook)[0]),
    W(holderPositionPda(launch, p.srcOwner, hook)[0]),
    W(holderPositionPda(launch, p.dstOwner, hook)[0]),
    R(hook),
  ];
}

export function transferCheckedWithHookInstruction(p                                                                                                                                                           )              {
  return {
    programAddress: TOKEN_2022_PROGRAM_ID,
    accounts: [W(p.source), R(p.baseMint), W(p.destination), RS(p.authority), ...hookAccountsFor({ baseMint: p.baseMint, srcOwner: p.srcOwner ?? p.authority, dstOwner: p.dstOwner, programId: p.programId, hookProgramId: p.hookProgramId })],
    data: new Writer().u8(12).u64(p.amount).u8(p.decimals).done(),
  };
}

export const HOOK_IX_RESYNC = new TextEncoder().encode("resync__");

export function resyncInstruction(p                                                               )              {
  const hook = hid(p);
  const launch = launchPda(p.baseMint, pid(p))[0];
  return {
    programAddress: hook,
    accounts: [W(holderBookPda(launch, hook)[0]), W(holderPositionPda(launch, p.owner, hook)[0]), R(p.ownerBase ?? associatedTokenAddress(p.owner, p.baseMint, TOKEN_2022_PROGRAM_ID))],
    data: HOOK_IX_RESYNC.slice(),
  };
}

export function burnCheckedInstruction(p                                                                                        )              {
  if (p.amount <= 0n) throw new RangeError("burn: the amount must be > 0");
  return { programAddress: TOKEN_2022_PROGRAM_ID, accounts: [W(p.account), W(p.baseMint), RS(p.owner)], data: new Writer().u8(15).u64(p.amount).u8(p.decimals).done() };
}

const evPair = (c      )                => [R(floorEventAuthorityPda(pid(c))[0]), R(pid(c))];

const hookProg = (c      )              => R(hid(c));
const opt = (a                           , c                 , role                            )              => (a ? role(a) : R(pid(c)));
function ix(name        , accounts               , args                       , c      , events = true)              {
  return { programAddress: pid(c), accounts: [...accounts, ...(events ? evPair(c) : [])], data: args(new Writer().bytes(instructionDiscriminator(name))).done() };
}

export function creditBackingInstructionV2(p                                                                                                                                                                                          )              {
  if (p.amount <= 0n) throw new RangeError("credit_backing: the amount must be > 0");
  const base = ix("credit_backing", [RS(p.donor), W(p.launch), R(p.baseMint), R(p.quoteMint), W(p.donorQuote), W(p.quoteVault), R(p.quoteTokenProgram)], (w) => w.u64(p.amount), p);
  if (!p.quoteIsLaunch) return base;
  return { ...base, accounts: [...base.accounts, ...hookAccountsFor({ baseMint: p.quoteMint, srcOwner: p.donor, dstOwner: p.launch, programId: p.programId, hookProgramId: p.hookProgramId })] };
}

export const SYNC_DEAD_MAX_PAIRS = 8;

export function syncDeadInstruction(p                                                                                                 )              {
  const n = p.deadTokenAccounts.length;
  if (n < 1 || n > SYNC_DEAD_MAX_PAIRS) throw new RangeError(`sync_dead takes 1..=${SYNC_DEAD_MAX_PAIRS} dead token accounts`);
  const base = ix("sync_dead", [WS(p.payer), W(p.launch), R(p.baseMint), R(SYSTEM_PROGRAM_ID)], (w) => w, p);
  const pairs = p.deadTokenAccounts.flatMap((a) => [R(a), W(deadMarkPda(p.launch, a, pid(p))[0])]);
  return { ...base, accounts: [...base.accounts, ...pairs] };
}

export function openHolderPositionInstruction(p                                                            )              {
  if (!canHoldPosition(p.owner)) throw new RangeError("open_holder_position: the owner is off the ed25519 curve (OffCurveOwner 13): a PDA or vault holds no position");
  const hook = hid(p);
  return ix("open_holder_position", [WS(p.owner), R(p.launch), W(holderBookPda(p.launch, hook)[0]), W(holderPositionPda(p.launch, p.owner, hook)[0]), R(p.ownerBase), R(SYSTEM_PROGRAM_ID), hookProg(p)], (w) => w, p);
}

export function holderAccountWarning(p                                                                                               )                                                                                              {
  const ata = associatedTokenAddress(p.owner, p.baseMint, TOKEN_2022_PROGRAM_ID);
  const idle = p.accounts.filter((a) => a.address !== ata && a.amount > 0n);
  const idleAmount = idle.reduce((s, a) => s + a.amount, 0n);
  return {
    idle,
    idleAmount,
    message: idle.length
      ? "Only your main token account earns Holder Fees and Holder Milestones. Tokens in another account of this wallet earn nothing until they move back."
      : null,
  };
}

export function closeHolderPositionInstruction(p                                                                                                                                                                           )              {
  const hook = hid(p);

  const base = ix(
    "close_holder_position",
    [WS(p.owner), W(p.launch), W(holderBookPda(p.launch, hook)[0]), W(holderPositionPda(p.launch, p.owner, hook)[0]), R(p.ownerBase), W(p.quoteVault), W(p.ownerQuote), R(p.quoteMint), R(p.quoteTokenProgram), hookProg(p)],
    (w) => w,
    p,
  );
  if (!p.quoteIsLaunch) return base;
  return { ...base, accounts: [...base.accounts, ...hookAccountsFor({ baseMint: p.quoteMint, srcOwner: p.launch, dstOwner: p.owner, programId: p.programId, hookProgramId: p.hookProgramId })] };
}

export const PRESET_CHUNK_MAX = 900;

export function writePresetChunkInstruction(p                                                                                                           )              {
  if (p.offset < 0 || p.data.length === 0 || p.offset + p.data.length > ROW_LEN) throw new RangeError(`a chunk lies inside the ${ROW_LEN}-byte row`);
  return ix("write_preset_chunk", [WS(p.authority), R(p.config), W(p.preset), R(SYSTEM_PROGRAM_ID)], (w) => w.u8(p.id).u16(p.offset).u32(p.data.length).bytes(p.data), p, false);
}

export function presetV2Instruction(p                                                                                                                                      )              {

  const accts = p.name === "update_preset" ? [RS(p.authority), R(p.config), W(p.preset)] : [WS(p.authority), W(p.config), W(p.preset), R(SYSTEM_PROGRAM_ID)];
  return ix(p.name, accts, (w) => w.u8(p.id), p);
}

export function presetV2Instructions(p                                                                                                                                                                       )                {
  if (p.row.length !== ROW_LEN) throw new RangeError(`a v2 row is ${ROW_LEN} bytes`);
  const n = p.chunk ?? PRESET_CHUNK_MAX;
  if (n < 1) throw new RangeError("chunk must be > 0");
  const out                = [];
  for (let o = 0; o < ROW_LEN; o += n) out.push(writePresetChunkInstruction({ ...p, offset: o, data: p.row.slice(o, Math.min(o + n, ROW_LEN)) }));
  out.push(presetV2Instruction(p));
  return out;
}

export function teamShareBuyInstructionV2(p                                                           )              {
  const base = ix(
    "team_share_buy",
    [WS(p.member), W(p.launch), R(p.baseMint), R(p.quoteMint), W(p.tokenVault), R(p.quoteVault), R(p.teamSlot ?? teamSlotPda(p.launch, pid(p))[0]), W(p.memberBase), R(TOKEN_2022_PROGRAM_ID)],
    (w) => w.u64(p.amountIn).u64(minOut(p.minOut, "minOut")),
    p,
  );
  return { ...base, accounts: [...base.accounts, ...hookAccountsFor({ baseMint: p.baseMint, srcOwner: p.launch, dstOwner: p.member, programId: p.programId, hookProgramId: p.hookProgramId })] };
}

const REM_HOOK_AT = 5;

export const REM_QUOTE_HOOK_AT = 11;

export const REM_REF_POSITION_AT = 17;

function tradeMetas(a                 , c      )                {
  return [
    WS(a.trader), W(a.launch), R(a.baseMint), R(a.quoteMint), W(a.tokenVault), W(a.quoteVault), W(a.traderBase), W(a.traderQuote), R(TOKEN_2022_PROGRAM_ID), R(a.quoteTokenProgram),
    opt(a.traderRef, c, W), opt(a.escrow, c, W),

  ];
}

function remMetas(a                 , isBuy         , c      )                {
  const rem                = [];
  if (a.user) {
    const wallet = a.linkWallet ?? a.user;
    rem.push(WS(a.user), R(traderLinkPda(wallet, c?.programId)[0]), R(SYSTEM_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID));
    if (a.recipient && a.recipient !== wallet) rem.push(R(traderLinkPda(a.recipient, c?.programId)[0]));
  }
  while (rem.length < REM_HOOK_AT) rem.push(R(pid(c)));
  const holder = a.baseOwner ?? a.trader;

  const [srcOwner, dstOwner] = isBuy ? [a.launch, holder] : [holder, a.launch];
  const out = [...rem, ...hookAccountsFor({ baseMint: a.baseMint, srcOwner, dstOwner, programId: c?.programId, hookProgramId: c?.hookProgramId })];
  if (!a.quoteIsLaunch) return out;

  const payer = a.quoteOwner ?? a.trader;
  const [qSrc, qDst] = isBuy ? [payer, a.launch] : [a.launch, payer];
  const q = [...out, ...hookAccountsFor({ baseMint: a.quoteMint, srcOwner: qSrc, dstOwner: qDst, programId: c?.programId, hookProgramId: c?.hookProgramId })];

  if (!a.traderRef || !a.escrow) return q;
  return [...q, W(holderPositionPda(launchPda(a.quoteMint, pid(c))[0], a.traderRef, hid(c))[0])];
}

const minOut = (v        , what        )         => {
  if (v <= 0n) throw new RangeError(`${what} must be > 0 (the NO_MINIMUM rule)`);
  return v;
};
function trade(name        , a                       , isBuy         , x        , m        , what        )              {
  const base = ix(name, tradeMetas(a, a), (w) => w.u64(x).u64(minOut(m, what)), a);
  return { ...base, accounts: [...base.accounts, ...remMetas(a, isBuy, a)] };
}

export const buyInstructionV2 = (p           )              => trade("buy", p, true, p.amountIn, p.minOut, "minOut");
export const buyFromBalanceInstructionV2 = (p                                                          )              => trade("buy_from_balance", p, true, p.keep, p.minOut, "minOut");

export const sellInstructionV2 = (p            )              => trade("sell", p, false, p.tokensIn, p.minOut, "minOut");
export const sellFromBalanceInstructionV2 = (p                                                          )              => trade("sell_from_balance", p, false, p.keep, p.minOut, "minOut");

export function buyWithPositionInstructions(p                                      )                {
  const buy = buyInstructionV2(p);
  if (p.hasPosition) return [buy];
  const owner = p.baseOwner ?? p.trader;
  if (owner !== p.trader) return [buy];
  if (!canHoldPosition(owner)) return [buy];
  return [buy, openHolderPositionInstruction({ owner, launch: p.launch, ownerBase: p.traderBase, programId: p.programId, hookProgramId: p.hookProgramId })];
}

export function createLaunchInstructionV2(p

                                                                                        

       

       )              {
  const enc = new TextEncoder();
  if (p.registry && p.args.quoteUsdCentsX64 === 0n) freshRegistryPrice(p.registry.entry, p.registry.now);
  if (enc.encode(p.args.name).length > MAX_NAME_LEN || enc.encode(p.args.symbol).length > MAX_SYMBOL_LEN) throw new RangeError("name ≤ 32, symbol ≤ 10 bytes");
  const buy = p.args.creatorBuy ?? 0n;
  const minTok = p.args.creatorBuyMinTokens ?? 0n;
  if (buy < 0n || minTok < 0n) throw new RangeError("the Team Buy and its minimum are ≥ 0");
  if (buy === 0n && minTok > 0n) throw new RangeError("a minimum without a Team Buy");
  if (p.teamBuyMax !== undefined && buy > p.teamBuyMax) throw new RangeError(`the Team Buy ${buy} is over the row's share ${p.teamBuyMax} (CreatorBuyTooLarge)`);
  const rate = p.teamSlotMaxBps;
  if (!Number.isInteger(rate) || rate < 0 || rate > 0xffff) throw new RangeError(`the row's Team Slot max is a u16: ${rate}`);
  if (p.args.teamSlotRateBps !== undefined && p.args.teamSlotRateBps !== rate) {
    throw new RangeError(`the Team Slot opens at the row's max ${rate}, never ${p.args.teamSlotRateBps} (TeamSlotRateOverMax)`);
  }
  const prog = pid(p);
  const m = createMintAccounts({ ...p, programId: prog });
  const base = ix(
    "create_launch",
    [
      WS(p.creator), W(p.config), R(p.preset), W(p.quote), R(p.quoteMint), W(m.baseMint), W(m.launch), W(m.tokenVault), W(m.quoteVault), W(p.creatorQuote),
      W(m.metadata), R(p.metadataProgram ?? METADATA_PROGRAM_ID), R(p.creatorRef ?? traderRefPda(p.creator, p.quoteMint, prog)[0]),
      R(p.creatorLink ?? traderLinkPda(p.creator, prog)[0]), R(TOKEN_2022_PROGRAM_ID), R(p.quoteTokenProgram), R(ASSOCIATED_TOKEN_PROGRAM_ID),
      R(SYSTEM_PROGRAM_ID), R(INSTRUCTIONS_SYSVAR_ID), W(teamSlotPda(m.launch, prog)[0]),
      W(holderBookPda(m.launch, hid(p))[0]), W(extraAccountMetasPda(m.baseMint, hid(p))[0]), hookProg(p),
    ],
    (w) => w.string(p.args.name).string(p.args.symbol).u64(buy).u64(minTok).u128(p.args.quoteUsdCentsX64).u16(rate).u64(m.mintNonce),
    p,
  );
  if (!p.quoteIsLaunch) return base;
  const quoteHook = hookAccountsFor({ baseMint: p.quoteMint, srcOwner: p.creator, dstOwner: m.launch, programId: p.programId, hookProgramId: p.hookProgramId });
  return { ...base, accounts: [...base.accounts, ...quoteHook] };
}

const vaultPayoutHook = (baseMint        , launch        , wallet        , c      )                =>
  hookAccountsFor({ baseMint, srcOwner: launch, dstOwner: wallet, programId: c?.programId, hookProgramId: c?.hookProgramId });

export function unlockCreatorBuyInstructionV2(p                                                                                                                       )              {
  const dest = p.destination ?? associatedTokenAddress(p.creator, p.baseMint, TOKEN_2022_PROGRAM_ID);
  const base = ix(
    "unlock_creator_buy",
    [WS(p.sender), W(p.launch), R(p.creator), R(p.baseMint), W(p.tokenVault), W(dest), R(TOKEN_2022_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID)],
    (w) => w,
    p,
  );
  return { ...base, accounts: [...base.accounts, ...vaultPayoutHook(p.baseMint, p.launch, p.creator, p)] };
}

export function burnTokensInstructionV2(p                                                                                                                                                                   )              {
  if (!Number.isInteger(p.place) || p.place < 0 || p.place >= MEMBERS_MAX) throw new RangeError(`burn_tokens: place is 0..${MEMBERS_MAX}`);
  if (p.teamAmount < 0n || p.walletAmount < 0n || (p.teamAmount === 0n && p.walletAmount === 0n)) throw new RangeError("burn_tokens: nothing to burn");
  const hook = hid(p);
  return ix(
    "burn_tokens",
    [
      RS(p.holder), W(p.launch), R(p.teamSlot ?? p.launch), W(p.baseMint), W(p.tokenVault), W(associatedTokenAddress(p.holder, p.baseMint, TOKEN_2022_PROGRAM_ID)), R(TOKEN_2022_PROGRAM_ID),
      W(holderBookPda(p.launch, hook)[0]), W(holderPositionPda(p.launch, p.holder, hook)[0]), hookProg(p),
    ],
    (w) => w.u8(p.place).u64(p.teamAmount).u64(p.walletAmount),
    p,
  );
}

export function quoteHookBlock(quoteMint        , receiver        , payers                   , p      = {})                {
  const hook = hid(p);
  const ql = launchPda(quoteMint, pid(p))[0];
  return [
    R(extraAccountMetasPda(quoteMint, hook)[0]),
    R(ql),
    W(holderBookPda(ql, hook)[0]),
    W(holderPositionPda(ql, receiver, hook)[0]),
    R(hook),
    ...payers.map((x) => W(holderPositionPda(ql, x, hook)[0])),
  ];
}

export function withdrawPlatformManyInstructionV2(p                                                                                                        )              {
  if (!p.quoteIsLaunch || p.launches.length === 0) return withdrawPlatformManyInstruction(p);
  if (!p.treasury) throw new RangeError("withdraw_platform_many: a launch-token quote needs the treasury (its position receives)");
  return withdrawPlatformManyInstruction({ ...p, quoteHook: quoteHookBlock(p.quoteMint, p.treasury, p.launches.map((l) => l.launch), p) });
}

export const CLAIM_HOLDER = CLAIM_BITS.solana.holder;

const CLAIM_QUOTE_BITS = CLAIM_CREATOR | CLAIM_CREATOR_REF | CLAIM_TEAM_SLOT | CLAIM_TRADER_REF;

export const CLAIM_KNOWN_WIDE_V2 = CLAIM_KNOWN_MASKS.solana.v2;

export function claimInstructionV2(p

       )              {
  if (p.unwrap && p.quoteIsLaunch) throw new RangeError("unwrap: a launch-token quote is never wSOL");
  if (p.unwrap && (p.quoteMint !== WSOL_MINT || p.quoteTokenProgram !== TOKEN_PROGRAM_ID)) throw new RangeError("unwrap: only a wSOL claim");
  if (!Number.isInteger(p.types) || p.types <= 0 || (p.types & ~CLAIM_KNOWN_WIDE_V2) !== 0) throw new RangeError("UnknownClaimType: bits 1, 2, 4, 8, 128, 256, 512 (16, 32 and 64 are reserved)");
  if (p.launches.length > 255) throw new RangeError("at most 255 launches a claim");
  const tok = (p.types & (CLAIM_TEAM_BUY | CLAIM_SHARE_BOUGHT)) !== 0;
  if (tok && p.launches.some((l) => !l.teamBuy)) throw new RangeError("bits 128 / 256: every launch needs its teamBuy accounts");
  const holder = (p.types & CLAIM_HOLDER) !== 0;
  const wide = p.types > 0xff;
  const prog = pid(p);
  const base = ix(
    wide ? "claim_wide" : "claim",
    [WS(p.sender), p.unwrap ? W(p.wallet) : R(p.wallet), R(p.quoteMint), R(p.quoteTokenProgram), W(p.destination), R(p.config), opt(p.refOwed, p, W), opt(p.refOwedVault, p, W), R(SYSTEM_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID)],
    (w) => (wide ? w.pubkey(p.wallet).u16(p.types) : w.pubkey(p.wallet).u8(p.types)).u8(p.launches.length),
    p,
  );
  const rest                = [];
  for (const l of p.launches) {
    rest.push(W(l.launch));
    if (holder) {
      const owner = l.holderOwner ?? p.wallet;
      const mint = l.baseMint ?? l.teamBuy?.baseMint;
      if (!l.holderBase && !mint) throw new RangeError("bit 512: every launch needs its holderBase or baseMint (Take's owner_base)");
      rest.push(W(holderBookPda(l.launch, hid(p))[0]), W(holderPositionPda(l.launch, owner, hid(p))[0]), R(l.holderBase ?? associatedTokenAddress(owner, mint , TOKEN_2022_PROGRAM_ID)));
    }
    rest.push(W(l.quoteVault), W(l.teamSlot ?? l.launch));
    if (tok) rest.push(R(l.teamBuy .baseMint), W(l.teamBuy .tokenVault), W(l.teamBuy .walletBase), ...vaultPayoutHook(l.teamBuy .baseMint, l.launch, p.wallet, p));
  }
  if (tok) rest.push(R(TOKEN_2022_PROGRAM_ID));
  if (holder) rest.push(hookProg(p));
  if (p.unwrap) rest.push(W(unwrapPda(p.wallet, prog)[0]));
  if (p.quoteIsLaunch && (p.types & (CLAIM_QUOTE_BITS | CLAIM_HOLDER)) !== 0) {
    const payers = p.launches.map((l) => l.launch);
    if ((p.types & CLAIM_TRADER_REF) !== 0 && p.refOwed) payers.push(p.refOwed);
    rest.push(...quoteHookBlock(p.quoteMint, p.wallet, payers, p));
  }
  return { ...base, accounts: [...base.accounts, ...rest] };
}

export function setGenesisConvertBandInstruction(p                                                                                                                                                                     )              {
  for (const v of [p.solPxMin, p.solPxMax, p.usdcPxMin, p.usdcPxMax]) if (v <= 0n || v >> 128n !== 0n) throw new RangeError("set_genesis_convert_band: each price is a u128 > 0");
  if (p.solPxMin > p.solPxMax || p.usdcPxMin > p.usdcPxMax) throw new RangeError("set_genesis_convert_band: min ≤ max");
  const g = pid(p);
  return ix(
    "set_genesis_convert_band",
    [WS(p.authority), R(p.config ?? configPda(g)[0]), R(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.convertBand ?? genesisConvertBandPda(g)[0]), R(SYSTEM_PROGRAM_ID)],
    (w) => w.u128(p.solPxMin).u128(p.solPxMax).u128(p.usdcPxMin).u128(p.usdcPxMax),
    p,
    false,
  );
}
