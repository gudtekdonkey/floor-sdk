

import { RANGES_MAX, SOL_BUY_WALK_STEPS, cloneBook } from "../math/v2/book.js";

import { buy } from "../math/v2/trade.js";

import { MAX_BANDS, V2_VERSION } from "./layouts.js";

import { quoteBuyNetInV2, quoteReceivedV2 } from "./quoteMint.js";

export const MATH_TOKEN_DECIMALS = 6;

export const SOLANA_TOKEN_DECIMALS = 6;

const DECIMALS_MAX = 24;

function scaleOf(decimals        )         {
  if (!Number.isInteger(decimals) || decimals < MATH_TOKEN_DECIMALS || decimals > DECIMALS_MAX) {
    throw new RangeError(`token decimals ${decimals}: ${MATH_TOKEN_DECIMALS} ..= ${DECIMALS_MAX}`);
  }
  return 10n ** BigInt(decimals - MATH_TOKEN_DECIMALS);
}

export function tokensToMathV2(chain        , decimals        )         {
  if (chain < 0n) throw new RangeError(`tokens ${chain} < 0`);
  return chain / scaleOf(decimals);
}

export function tokensToChainV2(math        , decimals        )         {
  if (math < 0n) throw new RangeError(`tokens ${math} < 0`);
  return math * scaleOf(decimals);
}

const n = (x                 )         => Number(x);

function liveSupplyV2(stored        , mintSupply         )         {
  if (mintSupply === undefined) return stored;
  if (mintSupply < 0n) throw new RangeError(`mint supply ${mintSupply} < 0`);
  return mintSupply < stored ? mintSupply : stored;
}

export function solanaSettleInputsV2(l                 , opts                          = {})                       {
  if (l.version !== V2_VERSION) throw new RangeError(`LaunchV2 version ${l.version}, want ${V2_VERSION}`);
  if (l.decimals !== SOLANA_TOKEN_DECIMALS) throw new RangeError(`LaunchV2 decimals ${l.decimals}: a Solana launch token has ${SOLANA_TOKEN_DECIMALS}`);
  const ranges          = Array.from({ length: RANGES_MAX }, (_, i) => ({
    sqrtA: l.rngSqrtA[i], sqrtB: l.rngSqrtB[i], tokens: l.rngTokens[i], askLeft: l.rngAskLeft[i], back: l.rngBack[i],
    place: l.rngPlace[i], thetaBps: n(l.rngThetaBps[i]), psiBps: n(l.rngPsiBps[i]),
    kind: n(l.rngKind[i]), class: n(l.rngClass[i]), msCapPpm: n(l.rngMsCapPpm[i]),
  }));
  const book       = {
    sqrtP: l.sqrtP, sqrtF: l.sqrtF, sqrtLMax: l.sqrtLMax, floor: l.floor, floorTokens: l.floorTokens,
    nBands: l.nBands,
    bandEdge: [...l.bandEdge], bandMoney: [...l.bandMoney], bandTokens: [...l.bandTokens],
    nC: l.nC, bandF: [...l.bandF], bandFClaim: [...l.bandFClaim], ckAtF: l.ckAtF, ckFloorTokens: l.ckFloorTokens,
    feePpm: l.feePpm.map(n), feeStartPpm: l.feeStartPpm.map(n), sqrtFeeA: l.sqrtFeeA, sqrtFeeB: l.sqrtFeeB,
    graduated: l.graduated !== 0, nRanges: l.nRanges, ranges,

    ...(l.ownFees !== 0 && { ownFees: true }),
  };

  const row           = {
    phiBps: l.phiBps, ckBps: l.ckBps, wcBps: l.wcBps, cShareBps: 5_000,
    msClass: l.msClass, platformMsBps: (l.flags & 1) !== 0 ? l.platformMsBps : 0, floorMsBps: (l.flags & 2) !== 0 ? l.floorMsBps : 0,
  };
  return {
    book, row, sup: { live: liveSupplyV2(l.supply, opts.mintSupply), dead: l.deadTokens },
    fee: { feePpm: [...book.feePpm], feeStartPpm: [...book.feeStartPpm], sqrtFeeA: book.sqrtFeeA, sqrtFeeB: book.sqrtFeeB },
    msCapPpm: ranges.map((r) => r.msCapPpm),
  };
}

export const bookFromLaunchV2 = (l                 )       => solanaSettleInputsV2(l).book;

export function withFeeCurveV2                          (inputs   , curve            )    {
  if (curve.feePpm.length !== 5 || curve.feeStartPpm.length !== 5) throw new RangeError("a fee curve has 5 legs");
  const book = cloneBook(inputs.book);
  book.feePpm = [...curve.feePpm];
  book.feeStartPpm = [...curve.feeStartPpm];
  book.sqrtFeeA = curve.sqrtFeeA;
  book.sqrtFeeB = curve.sqrtFeeB;
  return { ...inputs, book };
}

export function quoteBuySolanaV2(
  l                 ,
  gross        ,
  opts                                                                                                                           = {},
)                                                                          {
  const s = solanaSettleInputsV2(l, { mintSupply: opts.mintSupply });
  const after = cloneBook(s.book);

  const tf = opts.quoteTransferFee;
  const net = tf ? quoteBuyNetInV2(tf.fee, tf.epoch, gross) : gross;
  const r = buy(after, s.row, s.sup, net, opts.minTokens ?? 0n, { walkSteps: SOL_BUY_WALK_STEPS });
  const refundReceived = tf ? quoteReceivedV2(tf.fee, tf.epoch, r.refund) : r.refund;
  return { buy: r, after, withheld: gross - net, refundReceived };
}
