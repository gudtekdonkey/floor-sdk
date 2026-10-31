

import { decodeAbi, encodeAbi, encodeCall, eventTopic, functionSelector, hexToBytes, bytesToHex } from "../evm/abi.js";
import { keccak256 } from "../evm/keccak.js";

import { claimCall } from "../evm/floor/calls.js";

import { BIDS_MAX, RANGES_MAX, cloneBook, emptyRange } from "../math/v2/book.js";

import { buy } from "../math/v2/trade.js";

import { evmSignature } from "../generated/index.js";
import { CLAIM_BITS, CLAIM_KNOWN_MASKS } from "../generated/index.js";
import { EVM_EVENT_SCHEMAS } from "../abi/evm.gen.js";
import { V2_EVENT_FIELD_NAMES } from "../abi/evm-event-names.js";

export const CLAIM_HOLDER_EVM = CLAIM_BITS.evm.holder;

export const CLAIM_KNOWN_V2_EVM = CLAIM_KNOWN_MASKS.evm.v2;

const BOOK_RANGE = "(uint128,uint128,uint64,uint64,uint64,uint128,uint16,uint16,uint8,uint8)";

const LAUNCH_BOOK = `(uint128,uint128,uint128,uint128[],uint8,${BOOK_RANGE}[48],uint128,uint64,uint64[],uint128[],uint8,uint8,uint128[],uint128[],uint128,uint64,bool,uint32[5])`;

const BUY_OUT = "(uint256,uint256,uint256,uint128,uint8,bool,bool)";

export const V2_VIEWS = {
  launchBook: { sig: evmSignature("launchBook"), ret: [LAUNCH_BOOK] },
  launchMilestones: { sig: evmSignature("launchMilestones"), ret: ["(uint128[4],uint128[4])"] },

  simBuy: { sig: evmSignature("simBuy"), ret: [BUY_OUT] },
  holderPositionOf: { sig: "holderPositionOf(uint256,address)", ret: ["(bool,uint64,uint64,uint128,uint128,bool)"] },
  holderBook: { sig: evmSignature("holderBook"), ret: ["(uint128,uint64,uint64,uint128,uint128)"] },
  circulatingOf: { sig: evmSignature("circulatingOf"), ret: ["uint256", "uint256", "uint256"] },

  launchState: { sig: evmSignature("launchState"), ret: ["(address,address,address,uint8,uint32,bytes32,bool,uint64,uint64,uint64,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint16)"] },

  bidMoney: { sig: evmSignature("bidMoney"), ret: ["uint256"] },

  teamShareOf: { sig: evmSignature("teamShareOf"), ret: ["uint256", "uint256", "uint64"] },

  launchFeeCurve: { sig: evmSignature("launchFeeCurve"), ret: ["(uint32[5],uint128,uint128)"] },

  teamSlotOf: { sig: evmSignature("teamSlotOf"), ret: ["(uint64[10],uint32[10],uint64[10],address[10],bytes32[10],uint128[10],uint64[10],address[10])"] },

  pairIdOf: { sig: evmSignature("pairIdOf"), ret: ["bytes32"] },

  launchOfPair: { sig: evmSignature("launchOfPair"), ret: ["uint256"] },
}         ;

export function evmPairId(floorAmm        , launchId        )         {
  return bytesToHex(keccak256(encodeAbi(["address", "uint256"], [floorAmm, launchId])));
}

export const V2_TOKEN_VIEWS = {

  positionOf: { sig: evmSignature("positionOf"), ret: ["uint64", "uint64", "uint128", "bool", "uint128"] },

  bookState: { sig: evmSignature("bookState"), ret: ["uint128", "uint64", "uint64", "uint128", "uint128"] },
  bookTotals: { sig: evmSignature("bookTotals"), ret: ["uint128", "uint128"] },

  optedIn: { sig: evmSignature("optedIn"), ret: ["bool"] },
}         ;

export const V2_CALLS = {
  creditBacking: evmSignature("creditBacking"),
}         ;

export const V2_TOKEN_CALLS = {
  optIn: evmSignature("optIn"),
  releaseSkipped: evmSignature("releaseSkipped"),
}         ;

export const V2_DROPPED_EVM = ["setTraderListBps", "TraderListSet", "listBps"]         ;

export const V2_PENDING_EVM                         = {
  holderPositionOf: "not built: read the token's `positionOf` + `optedIn` (`readHolderV2`)",
  RowBands: "row error, not decoded yet", RowShares: "row error, not decoded yet", RowLength: "row error, not decoded yet",
  RowVersion: "row error, not decoded yet", RowEdges: "row error, not decoded yet", RowTail: "row error, not decoded yet",
  RowBounds: "row error, not decoded yet", RowFees: "row error, not decoded yet", RowTeam: "row error, not decoded yet",
  RowFlags: "row error, not decoded yet", RowMsClass: "row error, not decoded yet", RowNonCanonical: "row error, not decoded yet",
};

const call = (to        , sig        , args            , value = 0n)          => ({ to, data: bytesToHex(encodeCall(sig, args)), value });

const NATIVE = "0x0000000000000000000000000000000000000000";

export function creditBackingCall(amm        , launchId        , quote        , amount        )          {
  if (amount <= 0n) throw new RangeError("creditBacking: amount must be > 0 (NothingToDo)");
  return call(amm, V2_CALLS.creditBacking, [launchId, amount], quote.toLowerCase() === NATIVE ? amount : 0n);
}

export function claimV2Call(p                                                                                                               )          {
  if (!Number.isInteger(p.types) || p.types <= 0 || (p.types & ~CLAIM_KNOWN_V2_EVM) !== 0) throw new RangeError("UnknownClaimType: the v2 claim takes bits 1, 2, 4, 8, 64, 128, 256, 512 (16 and 32 are reserved)");
  if ((p.types & CLAIM_HOLDER_EVM) === 0) return claimCall(p);
  if ((p.launchIds ?? []).length === 0) throw new RangeError("bit 512 (holder) claims per launch: name the launchIds");
  if (p.quotes.length === 0) throw new RangeError("bit 512 (holder) pays in each launch's quote: list them in quotes (an unlisted one is skipped)");
  const rest = p.types & ~CLAIM_HOLDER_EVM;
  if (rest !== 0) claimCall({ ...p, types: rest });
  return call(p.floorAmm, evmSignature("claim"), [p.wallet, BigInt(p.types), [...(p.launchIds ?? [])], [...p.quotes]]);
}
export const optInCall = (token        )          => call(token, V2_TOKEN_CALLS.optIn, []);
export const releaseSkippedCall = (token        , a        )          => call(token, V2_TOKEN_CALLS.releaseSkipped, [a]);

export function viewCall(name                       , args            )         {
  return bytesToHex(encodeCall(V2_VIEWS[name].sig, args));
}
export function tokenViewCall(name                             , args            )         {
  return bytesToHex(encodeCall(V2_TOKEN_VIEWS[name].sig, args));
}

const n = (v          )         => Number(v          );
const big = (v          )         => v          ;
const bigs = (v          )           => (v              ).map(big);
const nums = (v          )           => (v              ).map(n);

function one(name                       , data                     )             {
  return decodeAbi(V2_VIEWS[name].ret, data);
}

export function decodeLaunchBook(data                     )               {
  const [t] = one("launchBook", data)                ;
  const nR = n(t[4]);
  const ranges = (t[5]                ).slice(0, nR).map((r) => ({
    sqrtA: big(r[0]), sqrtB: big(r[1]), tokens: big(r[2]), askLeft: big(r[3]), back: big(r[4]), place: big(r[5]),
    thetaBps: n(r[6]), psiBps: n(r[7]), kind: n(r[8]), class: n(r[9]),
  }));
  return {
    sqrtP: big(t[0]), sqrtF: big(t[1]), f: big(t[2]), bandMoney: bigs(t[3]), nRanges: nR, ranges,
    sqrtLMax: big(t[6]), floorTokens: big(t[7]), bandTokens: bigs(t[8]), bandEdge: bigs(t[9]), nBands: n(t[10]), nC: n(t[11]),
    bandF: bigs(t[12]), bandFClaim: bigs(t[13]), ckAtF: big(t[14]), ckFloorTokens: big(t[15]), graduated: t[16]           , feePpm: nums(t[17]),
  };
}
export function decodeSimBuy(data                     )           {
  const [t] = one("simBuy", data)                ;
  return { tokensOut: big(t[0]), spent: big(t[1]), refund: big(t[2]), sqrtAfter: big(t[3]), steps: n(t[4]), budgetHit: t[5]           , graduatedNow: t[6]            };
}

export function decodeLaunchStateSupply(data                     )                                                       {
  const [t] = one("launchState", data)                ;
  return { graduated: t[6]           , supply: big(t[10]), sOut: big(t[11]) };
}
export const decodeBidMoney = (data                     )         => big(one("bidMoney", data)[0]);
export const decodeOptedIn = (data                     )          => decodeAbi(V2_TOKEN_VIEWS.optedIn.ret, data)[0]           ;
export function decodeLaunchMilestones(data                     )             {
  const [t] = one("launchMilestones", data)                ;
  return { booked: bigs(t[0]), paid: bigs(t[1]) };
}

export function decodeLaunchFeeCurve(data                     )                 {
  const [t] = one("launchFeeCurve", data)                ;
  return { feeStartPpm: nums(t[0]), sqrtFeeA: big(t[1]), sqrtFeeB: big(t[2]) };
}

export function decodeTeamSlotOf(data                     )             {
  const [t] = one("teamSlotOf", data)                ;
  const strs = (v          )           => (v              ).map((x) => x          );
  return { teamBuyPart: bigs(t[0]), weightPpb: nums(t[1]), share: bigs(t[2]), members: strs(t[3]), handles: strs(t[4]), slotOwed: bigs(t[5]), slotBoughtOwed: bigs(t[6]), wallets: strs(t[7]) };
}
export function decodeTeamShareOf(data                     )              {
  const [claimable, boughtOwed, lastDay] = one("teamShareOf", data);
  return { claimable: big(claimable), boughtOwed: big(boughtOwed), lastDay: big(lastDay) };
}
export function decodeHolderPositionOf(data                     )              {
  const [t] = one("holderPositionOf", data)                ;
  return { matured: t[0]           , amount: big(t[1]), avgTs: big(t[2]), owed: big(t[3]), indexSnap: big(t[4]), skipped: t[5]            };
}
export function decodeHolderBook(data                     )               {
  const [t] = one("holderBook", data)                ;
  return { index: big(t[0]), wMatured: big(t[1]), wPending: big(t[2]), carry: big(t[3]), dust: big(t[4]) };
}
export function decodeCirculatingOf(data                     )                                                  {
  const [a, b, c] = one("circulatingOf", data);
  return { sCirc: big(a), sBelow: big(b), dead: big(c) };
}
export function decodeTokenPosition(data                     )                               {
  const [amount, avgTs, owed, matured, indexSnap] = decodeAbi(V2_TOKEN_VIEWS.positionOf.ret, data);
  return { amount: big(amount), avgTs: big(avgTs), owed: big(owed), matured: matured           , indexSnap: big(indexSnap) };
}
export function decodeTokenBookState(data                     )               {
  const [index, wMatured, wPending, carry, dust] = decodeAbi(V2_TOKEN_VIEWS.bookState.ret, data);
  return { index: big(index), wMatured: big(wMatured), wPending: big(wPending), carry: big(carry), dust: big(dust) };
}

export const V2_EVENTS = Object.fromEntries(
  (Object.keys(V2_EVENT_FIELD_NAMES)                 ).map((n) => {
    const e = EVM_EVENT_SCHEMAS.find((x) => x.name === n);
    if (!e) throw new Error(`no generated schema for ${n}`);
    return [n, { sig: e.signature, indexed: e.params.filter((p) => p.indexed).length, names: V2_EVENT_FIELD_NAMES[n] }];
  }),
)                                                                                                                                         ;

export const V2_EVENTS_ELSEWHERE = [
  "Initialized", "LaunchCreated", "PayoutOwed", "TeamSlotOpened", "Upgraded", "Approval", "Transfer", "EIP712DomainChanged", "Claimed", "ClaimSkipped",
  "HolderWithdrawn", "PlatformWithdrawn", "PayoutClaimed", "PotsSwept", "TeamBuyClaimed", "TeamShareBoughtClaimed", "CreatorBuyUnlocked",
  "TeamSlotMember",
]         ;

let topics                                                 ;
const TOPICS = ()                                      =>
  (topics ??= new Map((Object.keys(V2_EVENTS)                              ).map((k) => [eventTopic(V2_EVENTS[k].sig), k])));

export const v2EventTopic = (name                        )         => eventTopic(V2_EVENTS[name].sig);

export function decodeLogV2(log                                             )                                                            {
  const name = TOPICS().get((log.topics[0] ?? "").toLowerCase());
  if (!name) return null;
  const ev = V2_EVENTS[name];
  const types = ev.sig.slice(ev.sig.indexOf("(") + 1, -1).split(/,(?![^[]*\])/);
  if (log.topics.length !== 1 + ev.indexed) throw new RangeError(`${name}: ${log.topics.length - 1} indexed topics, want ${ev.indexed}`);
  const fields                           = {};
  for (let i = 0; i < ev.indexed; i++) fields[ev.names[i]] = decodeAbi([types[i]], hexToBytes(log.topics[1 + i]))[0];
  const body = decodeAbi(types.slice(ev.indexed), log.data);
  body.forEach((v, i) => (fields[ev.names[ev.indexed + i]] = v));
  return { name, fields };
}

export const V2_ERRORS_EVM = [
  "RowTooManyRanges", "RowNotFeasible", "NotToken",

  "NotProgram",

  "BadMember",
]         ;

export const V2_ERRORS_ARGS_EVM = ["UnknownClaimType(uint256)"]         ;
let errs                                 ;

export function decodeErrorV2(data        )                {
  errs ??= new Map([...V2_ERRORS_EVM.map((n) => `${n}()`), ...V2_ERRORS_ARGS_EVM].map((sig) => [bytesToHex(functionSelector(sig)), sig.slice(0, sig.indexOf("("))]));
  return errs.get(data.slice(0, 10).toLowerCase()) ?? null;
}

export function bookFromEvmLaunchBook(view              , opts                                                           = {})       {
  const noRow           = { phiBps: 0, ckBps: 0, wcBps: 0, cShareBps: 0, msClass: 0, platformMsBps: 0, floorMsBps: 0 };
  return evmSettleInputsV2(view, noRow, { live: 0n, dead: 0n }, opts.msCapPpm, opts.curve).book;
}

export function evmSettleInputsV2(b              , row          , sup        , msCapPpm                    = [], curve                 )                                             {
  if (curve && curve.feeStartPpm.length !== 5) throw new RangeError("a fee curve has 5 legs");
  const fits = (xs                              , what        )       => {
    for (let i = BIDS_MAX; i < xs.length; i++) if (BigInt(xs[i]) !== 0n) throw new RangeError(`${what}[${i}] is set: the SDK's v2 math carries ${BIDS_MAX} bid segments`);
  };
  if (b.nBands > BIDS_MAX) throw new RangeError(`${b.nBands} bid segments: the SDK's v2 math carries ${BIDS_MAX}`);
  fits(b.bandMoney, "bandMoney");
  fits(b.bandTokens, "bandTokens");
  fits(b.bandF, "bandF");
  fits(b.bandFClaim, "bandFClaim");
  const pad =    (xs              , len        , z   )      => Array.from({ length: len }, (_, i) => xs[i] ?? z);
  const ranges          = Array.from({ length: RANGES_MAX }, (_, i) => (b.ranges[i] ? { ...b.ranges[i], msCapPpm: msCapPpm[i] ?? 0 } : emptyRange()));
  const book       = {
    sqrtP: b.sqrtP, sqrtF: b.sqrtF, sqrtLMax: b.sqrtLMax, floor: b.f, floorTokens: b.floorTokens, nBands: b.nBands, nC: b.nC,
    bandEdge: pad(b.bandEdge, BIDS_MAX + 1, 0n).slice(0, BIDS_MAX + 1), bandMoney: pad(b.bandMoney, BIDS_MAX, 0n).slice(0, BIDS_MAX),
    bandTokens: pad(b.bandTokens, BIDS_MAX, 0n).slice(0, BIDS_MAX), bandF: pad(b.bandF, BIDS_MAX, 0n).slice(0, BIDS_MAX),
    bandFClaim: pad(b.bandFClaim, BIDS_MAX, 0n).slice(0, BIDS_MAX), ckAtF: b.ckAtF, ckFloorTokens: b.ckFloorTokens,
    feePpm: [...b.feePpm], graduated: b.graduated,

    feeStartPpm: curve ? [...curve.feeStartPpm] : [0, 0, 0, 0, 0], sqrtFeeA: curve?.sqrtFeeA ?? 0n, sqrtFeeB: curve?.sqrtFeeB ?? 0n,
    nRanges: b.nRanges, ranges,
  };
  return { book, row: { ...row }, sup: { ...sup } };
}

export function quoteBuyEvmV2(
  v                                                    ,
  gross        ,
  opts                                                                                         = {},
)                                                                 {
  const { book, row, sup } = evmSettleInputsV2(v.book, v.row, v.sup, opts.msCapPpm, opts.curve);
  const after = cloneBook(book);
  const r = buy(after, row, sup, gross, 0n);
  return { buy: r, after, holder: opts.recipient ?? null };
}

const NEVER_COUNTED = new Set(["0x0000000000000000000000000000000000000000", "0x0000000000000000000000000000000000000001", "0x000000000000000000000000000000000000dead", "0xdead000000000000000042069420694206942069"]);

export function holderStandingV2(p                                                                      )                   {
  const a = p.address.toLowerCase();
  if (NEVER_COUNTED.has(a) || a === p.program.toLowerCase()) return { weight: "never-counted", earns: false, note: "This address never holds a holder position." };
  const hasCode = p.code !== "" && p.code !== "0x" && !/^0x0*$/.test(p.code);
  if (hasCode && !p.optedIn) {
    return { weight: "contract-not-opted-in", earns: false, note: "A contract (or a delegated wallet) earns no Holder Fee and no Holder Milestones until it calls the token's optIn() once." };
  }
  return { weight: "counted", earns: true, note: null };
}

export async function readHolderV2(
  rpc                                                                                                           ,
  p                                                     ,
)                                                                                                                  {
  const [pos, opted, code] = await Promise.all([
    rpc.ethCall(p.token, tokenViewCall("positionOf", [p.address])),
    rpc.ethCall(p.token, tokenViewCall("optedIn", [p.address])),
    rpc.getCode(p.address),
  ]);
  const x = decodeTokenPosition(pos);
  const standing = holderStandingV2({ address: p.address, code, optedIn: decodeOptedIn(opted), program: p.program });
  return { amount: x.amount, owed: x.owed, indexSnap: x.indexSnap, standing, optIn: standing.weight === "contract-not-opted-in" ? optInCall(p.token) : null };
}
