

import { EVM_NATIVE, NoRouteError, SOL_NATIVE, coinKey } from "../route/graph.js";
import { FloorQuoteError } from "../quote.js";
import { FloorMathError } from "../math/index.js";
import { COMPUTE_BUDGET_PROGRAM_ID, MAX_COMPUTE_UNIT_LIMIT, computeUnitLimitFor, computeUnitLimitInstruction, computeUnitPriceInstruction } from "../solana/instruction.js";

import { TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, WSOL_MINT, associatedTokenAddress } from "../solana/pda.js";
import { PACKET_DATA_SIZE, closeAccountInstruction, compileV0Message, createAssociatedTokenAccountIdempotentInstruction, syncNativeInstruction, systemTransferInstruction, transactionSize } from "../solana/tx.js";

import { FLOOR_AMM_PROGRAM_ID, launchPda, quoteVault, tokenVault, traderRefEscrow, traderRefPda } from "../solana/floor/program.js";
import { approveCall, buyCall, sellCall } from "../evm/floor/calls.js";

import { checkFloorHops, evmRouteCall } from "../evm/router.js";

import { edgeLegInstructions } from "../route/edge.js";

import { jupiterEdgeLeg } from "../route/edge-jupiter.js";

import { buyFromBalanceInstructionV2, buyInstructionV2, canHoldPosition, openHolderPositionInstruction, sellFromBalanceInstructionV2, sellInstructionV2 } from "./solana.js";

import { quoteReceivedV2, quoteTransferFeeV2 } from "./quoteMint.js";

import { MAX_SLIPPAGE_BPS, minOutAt } from "./press.js";
import { buyLessIn } from "../math/v2/trade.js";

export function isWrapHopV2(h       )          {
  return h.kind === "edge" && ((h.from === SOL_NATIVE && h.to === WSOL_MINT) || (h.from === WSOL_MINT && h.to === SOL_NATIVE));
}

export const MAX_QUOTE_CHAIN_V2 = 8;

function chainOf   (byToken                        , coin        , edge             )                                                        {
  const pools              = [];
  const coins = [coinKey(coin)];
  for (let c = coin; ; ) {
    const pool = edge.has(coinKey(c)) ? undefined : byToken.get(coinKey(c));
    if (!pool) return { pools, coins, base: c };
    if (pools.length >= MAX_QUOTE_CHAIN_V2 || pools.includes(pool)) throw new NoRouteError("QuoteCycle", `${coin}'s quote chain does not end`);
    pools.push(pool);
    c = pool.quote;
    coins.push(coinKey(c));
  }
}

export function findRouteV2   (pools                     , from        , to        , opts                              = {})                 {
  if (coinKey(from) === coinKey(to)) throw new NoRouteError("Same", `${from} → itself`);
  const byToken = new Map                   ();
  for (const p of pools) byToken.set(coinKey(p.token), p);
  const edge = new Set([...(opts.edge ?? [])].map(coinKey));
  const a = chainOf(byToken, from, edge);
  const b = chainOf(byToken, to, edge);
  const all = [...a.pools, ...b.pools];
  if (all.length === 0) throw new NoRouteError("NoFloorHop", `${from} → ${to} crosses no FLOOR launch`);
  const chain = all[0].chain;
  if (all.some((x) => x.chain !== chain)) throw new NoRouteError("Mixed", `${from} → ${to} mixes chains`);
  for (let i = 0; i < a.coins.length; i++) {
    const j = b.coins.indexOf(a.coins[i]);
    if (j < 0) continue;
    const hops             = [...a.pools.slice(0, i).map((pool) => ({ kind: "sell"         , pool })), ...b.pools.slice(0, j).reverse().map((pool) => ({ kind: "buy"         , pool }))];
    return { chain, from, to, hops };
  }
  if (a.pools.length + b.pools.length === 0) throw new NoRouteError("NoFloorHop", `${from} → ${to}: no launch on the way (an outside leg alone is not a FLOOR route)`);
  if (edge.size === 0) throw new NoRouteError("NoFloorHop", `${from} → ${to}: the quote chains never meet (an outside leg needs \`edge\`)`);
  const hops             = [
    ...a.pools.map((pool) => ({ kind: "sell"         , pool })),
    { kind: "edge", from: a.base, to: b.base },
    ...[...b.pools].reverse().map((pool) => ({ kind: "buy"         , pool })),
  ];
  return { chain, from, to, hops };
}

const less = (x        , slippageBps        )         => (x > 0n ? minOutAt(x, slippageBps) : 1n);

const lessIn = (x        , slippageBps        )         => (x > 0n ? buyLessIn(x, slippageBps) : 0n);

export function fillsNothing(e         )          {
  if (e instanceof FloorQuoteError) return e.code === "NothingToBuy" || e.code === "Zero";
  if (e instanceof FloorMathError) return e.code === "NothingToDo" || e.code === "Zero";
  return false;
}

export function minChain                          (f         , min        , isNothing                          = fillsNothing)           {
  if (min <= 0n) return null;
  try {
    const r = f();
    return r.out > 0n ? r : null;
  } catch (e) {
    if (isNothing(e)) return null;
    throw e;
  }
}

function* priceHops   (p                )                                                     {
  if (p.amountIn <= 0n) throw new RangeError("amountIn must be > 0");
  if (!Number.isInteger(p.slippageBps) || p.slippageBps < 0 || p.slippageBps > MAX_SLIPPAGE_BPS) throw new RangeError(`slippageBps in 0..=${MAX_SLIPPAGE_BPS}`);
  if (!p.path.hops.some((h) => h.kind !== "edge")) throw new RangeError("an empty path (no FLOOR hop)");

  if (p.path.chain === "evm") checkFloorHops(p.chainId, p.path.hops.filter((h) => h.kind !== "edge").length);
  const seen = new Set        ();
  for (const h of p.path.hops) {
    if (h.kind === "edge") continue;
    const id = coinKey(h.pool.token);
    if (seen.has(id)) throw new RangeError(`the path trades ${h.pool.token} twice: quote it as one hop`);
    seen.add(id);
  }
  const hops                  = [];
  let expected = p.amountIn;
  let min = p.amountIn;
  let surplus                            ;
  for (const [i, hop] of p.path.hops.entries()) {
    if (hop.kind === "edge") {
      if (isWrapHopV2(hop)) {

        hops.push({ hop, amountIn: expected, amountOut: expected, minOut: min, minReceived: min, quote: { out: expected }, minFloored: false });
        continue;
      }

      const out = p.path.hops.slice(0, i).some((h) => h.kind === "sell");
      const amountIn = out ? min : expected;
      const e = yield { from: hop.from, to: hop.to, amountIn, slippageBps: p.slippageBps };

      if (e.minOut < 1n || e.minOut > e.outAmount) throw new RangeError(`the edge quote for hop ${i} has minOut ${e.minOut}: it must be 1..=outAmount ${e.outAmount}`);
      if (e.inAmount !== amountIn) throw new RangeError(`the edge quote for hop ${i} is for ${e.inAmount}, not ${amountIn}`);
      if (out) surplus = { coin: hop.from, expected: expected - min };
      hops.push({ hop, amountIn, amountOut: e.outAmount, minOut: e.minOut, minReceived: e.minOut, quote: { out: e.outAmount }, minFloored: false, edge: e });
      expected = e.outAmount;
      min = e.minOut;
      continue;
    }

    const tf = hop.pool.quoteTransferFee;
    if (tf?.fee && hop.pool.chain !== "solana") throw new RangeError(`pool ${hop.pool.symbol}: quoteTransferFee is Solana's (Token-2022)`);
    const fee = (x        )         => (tf ? quoteTransferFeeV2(tf.fee, tf.epoch, x) : 0n);
    const arrives = (x        )         => (tf ? quoteReceivedV2(tf.fee, tf.epoch, x) : x);
    let qe                ;
    if (hop.kind === "buy") {
      const w = fee(expected);
      if (w >= expected) throw new RangeError(`hop ${i} (buy ${hop.pool.symbol}): QuoteTransferShort: the quote mint's transfer fee ${w} takes the whole amount ${expected}`);
      qe = p.quote(hop.pool.launch, hop.kind, expected - w);
    } else qe = p.quote(hop.pool.launch, hop.kind, expected);
    if (qe.out <= 0n) throw new RangeError(`hop ${i} (${hop.kind} ${hop.pool.symbol}) fills nothing on ${expected}`);

    let qm                       ;
    let minOut        ;
    let minReceived        ;
    let amountOut        ;
    if (hop.kind === "buy") {
      const mIn = lessIn(min, p.slippageBps);
      const mw = fee(mIn);

      qm = mIn === expected ? qe : mw >= mIn ? null : minChain(() => p.quote(hop.pool.launch, hop.kind, mIn - mw), mIn - mw, p.isNothing);
      minOut = qm ? qm.out : 1n;
      minReceived = minOut;
      amountOut = qe.out;
    } else {
      qm = min === expected ? qe : minChain(() => p.quote(hop.pool.launch, hop.kind, min), min, p.isNothing);
      minOut = qm ? less(qm.out, p.slippageBps) : 1n;
      minReceived = arrives(minOut);
      amountOut = arrives(qe.out);
      if (amountOut <= 0n) throw new RangeError(`hop ${i} (sell ${hop.pool.symbol}): the quote mint's transfer fee takes the whole payout ${qe.out}`);
    }
    hops.push({ hop, amountIn: expected, amountOut, minOut, minReceived, quote: qe, minFloored: qm === null });
    expected = amountOut;
    min = minReceived;
  }
  return { path: p.path, amountIn: p.amountIn, amountOut: expected, minOut: min, hops, slippageBps: p.slippageBps, ...(p.path.chain === "evm" ? { chainId: p.chainId } : {}), ...(surplus ? { surplus } : {}) };
}

export function quoteRouteV2   (p                )                  {
  const r = priceHops(p).next();
  if (!r.done) throw new RangeError(`the outside leg ${r.value.from} → ${r.value.to} needs an EdgeQuoter: use quoteRouteEdgeV2`);
  return r.value;
}

export async function quoteRouteEdgeV2   (p                                        )                           {
  const g = priceHops(p);
  let r = g.next();
  while (!r.done) {
    if (!p.edge) throw new RangeError(`the outside leg ${r.value.from} → ${r.value.to} needs an EdgeQuoter (\`edge\`)`);
    r = g.next(await p.edge(r.value));
  }
  return r.value;
}

function quoteProgramOf(pool        , all          )         {
  if (pool.quoteTokenProgram) return pool.quoteTokenProgram;
  if (all.some((x) => coinKey(x.token) === coinKey(pool.quote))) return TOKEN_2022_PROGRAM_ID;
  throw new RangeError(`pool ${pool.symbol}: quoteTokenProgram is required`);
}

export function routeTradeAccountsV2(trader        , pool        , quoteTokenProgram        , programId = FLOOR_AMM_PROGRAM_ID)                                          {
  const launch = launchPda(pool.token, programId)[0];
  const traderRef = pool.referred || pool.linked ? traderRefPda(trader, pool.quote, programId)[0] : null;
  return {
    programId, trader, launch, baseMint: pool.token, quoteMint: pool.quote,
    tokenVault: tokenVault(launch, pool.token), quoteVault: quoteVault(launch, pool.quote, quoteTokenProgram),
    traderBase: associatedTokenAddress(trader, pool.token, TOKEN_2022_PROGRAM_ID), traderQuote: associatedTokenAddress(trader, pool.quote, quoteTokenProgram),
    quoteTokenProgram, traderRef, escrow: traderRef ? traderRefEscrow(traderRef, pool.quote, quoteTokenProgram) : null,
    user: pool.linked ? trader : null,
  };
}

export function routeInstructionsV2(p                           )                {
  return hopInstructionsV2(p).flat();
}

function hopInstructionsV2(p                           )                  {
  const q = p.quote;
  if (q.path.chain !== "solana") throw new RangeError("routeInstructionsV2 builds Solana routes; EVM routes are `evmRouteCallV2` / `evmHopCallsV2`");
  const programId = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  const pools = q.hops.flatMap((h) => (h.hop.kind === "edge" ? [] : [h.hop.pool]));
  const edges = [...(p.edges ?? [])];
  const legs = q.hops.filter((h) => h.hop.kind === "edge" && !isWrapHopV2(h.hop)).length;
  if (edges.length !== legs) throw new RangeError(`the route has ${legs} edge leg(s): pass ${legs} EdgeLeg(s), not ${edges.length}`);
  const wsolAta = associatedTokenAddress(p.trader, WSOL_MINT, TOKEN_PROGRAM_ID);

  const bal = (mint        )         => {
    const v = p.balances?.[mint];
    if (v === undefined) throw new RangeError(`balances[${mint}] is required: the trader's balance of that coin BEFORE the route (0n if none)`);
    return v;
  };
  const all                  = [];
  q.hops.forEach((hq, i) => {
    const hop = hq.hop;
    const out                = [];
    all.push(out);
    if (hop.kind === "edge") {
      if (!isWrapHopV2(hop)) out.push(...edgeLegInstructions(edges.shift() ));
      else if (hop.from === SOL_NATIVE) out.push(systemTransferInstruction({ from: p.trader, to: wsolAta, lamports: hq.amountIn }), syncNativeInstruction(wsolAta));
      else out.push(closeAccountInstruction({ account: wsolAta, destination: p.trader, owner: p.trader, tokenProgram: TOKEN_PROGRAM_ID }));
      return;
    }
    const pool = hop.pool;
    const a = { ...routeTradeAccountsV2(p.trader, pool, quoteProgramOf(pool, pools), programId), quoteIsLaunch: pool.quoteIsLaunch ?? pools.some((x) => coinKey(x.token) === coinKey(pool.quote)) };
    if (hq.hop.kind === "buy") {
      out.push(i === 0 ? buyInstructionV2({ ...a, amountIn: hq.amountIn, minOut: hq.minOut }) : buyFromBalanceInstructionV2({ ...a, keep: bal(pool.quote), minOut: hq.minOut }));

      const prev = i > 0 ? q.hops[i - 1].hop : null;
      if (prev && isWrapHopV2(prev) && bal(WSOL_MINT) === 0n) out.push(closeAccountInstruction({ account: wsolAta, destination: p.trader, owner: p.trader, tokenProgram: TOKEN_PROGRAM_ID }));
      if (i === q.hops.length - 1 && !p.hasPosition && canHoldPosition(p.trader)) out.push(openHolderPositionInstruction({ owner: p.trader, launch: a.launch, ownerBase: a.traderBase, programId }));
    } else {
      out.push(i === 0 ? sellInstructionV2({ ...a, tokensIn: hq.amountIn, minOut: hq.minOut }) : sellFromBalanceInstructionV2({ ...a, keep: bal(pool.token), minOut: hq.minOut }));
    }
  });
  return all;
}

export const ATA_CREATE_CU = 16_261;

export function solPaidBuyInstructionsV2(p

 )              {
  if ((p.jupiter === undefined) === (p.edge === undefined)) throw new RangeError("pass the edge leg once: `edge` or `jupiter`");
  const e = p.edge ?? jupiterEdgeLeg(p.jupiter );
  if (!p.pool.quoteTokenProgram) throw new RangeError(`pool ${p.pool.symbol}: quoteTokenProgram is required`);
  const { minOut } = p;
  if (minOut < 1n) throw new RangeError("minOut must be ≥ 1");
  const programId = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  const a = { ...routeTradeAccountsV2(p.trader, p.pool, p.pool.quoteTokenProgram, programId), quoteIsLaunch: p.pool.quoteIsLaunch ?? false };
  const limit = p.computeUnitLimit
    ?? (p.pool.snipeSpan || e.computeUnits === undefined || p.buyComputeUnits === undefined ? MAX_COMPUTE_UNIT_LIMIT : computeUnitLimitFor(e.computeUnits + p.buyComputeUnits + ATA_CREATE_CU));
  const budget                = [computeUnitLimitInstruction(limit)];
  const price = p.microLamports ?? e.computeUnitPrice;
  if (price !== undefined) budget.push(computeUnitPriceInstruction(price));
  const rest = [
    createAssociatedTokenAccountIdempotentInstruction({ payer: p.trader, owner: p.trader, mint: p.pool.token, tokenProgram: TOKEN_2022_PROGRAM_ID }),
    ...e.setup,
    ...e.swap,
    buyFromBalanceInstructionV2({ ...a, keep: p.keep, minOut }),
    ...(!p.hasPosition && canHoldPosition(p.trader) ? [openHolderPositionInstruction({ owner: p.trader, launch: a.launch, ownerBase: a.traderBase, programId })] : []),
    ...e.cleanup,
  ];
  for (const ix of rest) if (ix.programAddress === COMPUTE_BUDGET_PROGRAM_ID) throw new RangeError("a compute budget instruction outside the budget slot");
  return { instructions: [...budget, ...rest], lookupTableAddresses: [...e.lookupTableAddresses] };
}

export function routedBuyMessageV2(p                                                                                               )                                                       {
  const message = compileV0Message({ payer: p.trader, instructions: p.routed.instructions, recentBlockhash: p.recentBlockhash, lookupTables: p.lookupTables });
  const size = transactionSize(message);
  return { message, size, fits: size <= PACKET_DATA_SIZE };
}

const DUMMY_BLOCKHASH = "11111111111111111111111111111111";

export function routeTransactionsV2(p                                                                                                                             )               {
  const q = p.quote;
  const per = hopInstructionsV2(p);
  if (p.computeUnits && p.computeUnits.length !== q.hops.length) throw new RangeError(`computeUnits: one entry per path hop (${q.hops.length}), not ${p.computeUnits.length}`);
  for (const u of p.computeUnits ?? []) if (u !== undefined && (!Number.isInteger(u) || u < 0)) throw new RangeError("computeUnits must be whole numbers");
  const pools = q.hops.flatMap((h) => (h.hop.kind === "edge" ? [] : [h.hop.pool]));
  const edges = [...(p.edges ?? [])];

  const segs        = q.hops.map((hq, i)      => {
    const hop = hq.hop;
    const given = p.computeUnits?.[i];
    if (hop.kind === "edge") {
      if (isWrapHopV2(hop)) return { ixs: per[i], atas: hop.from === SOL_NATIVE ? [{ mint: WSOL_MINT, tokenProgram: TOKEN_PROGRAM_ID }] : [], cu: given, luts: [], snipe: false };
      const e = edges.shift() ;
      return { ixs: per[i], atas: [], cu: given ?? e.computeUnits, price: e.computeUnitPrice, luts: [...e.lookupTableAddresses], snipe: false };
    }
    const atas = hop.kind === "buy" ? [{ mint: hop.pool.token, tokenProgram: TOKEN_2022_PROGRAM_ID }] : [{ mint: hop.pool.quote, tokenProgram: quoteProgramOf(hop.pool, pools) }];
    return { ixs: per[i], atas, cu: given, luts: [], snipe: hop.pool.snipeSpan ?? false };
  });
  const atasOf = (ss       )        => {
    const atas        = [];
    for (const s of ss) for (const x of s.atas) if (!atas.some((y) => y.mint === x.mint && y.tokenProgram === x.tokenProgram)) atas.push(x);
    return atas;
  };
  const known = (ss       )         => ss.reduce((n, s) => n + (s.cu ?? 0), 0) + atasOf(ss).length * ATA_CREATE_CU;
  const build = (ss       , start        )             => {
    const limit = ss.some((s) => s.cu === undefined || s.snipe) ? MAX_COMPUTE_UNIT_LIMIT : computeUnitLimitFor(known(ss));
    const budget = [computeUnitLimitInstruction(limit)];
    const price = p.microLamports ?? ss.find((s) => s.price !== undefined)?.price;
    if (price !== undefined) budget.push(computeUnitPriceInstruction(price));
    const creates = atasOf(ss).map((x) => createAssociatedTokenAccountIdempotentInstruction({ payer: p.trader, owner: p.trader, mint: x.mint, tokenProgram: x.tokenProgram }));
    return { instructions: [...budget, ...creates, ...ss.flatMap((s) => s.ixs)], lookupTableAddresses: [...new Set(ss.flatMap((s) => s.luts))], computeUnits: limit, hops: [start, start + ss.length] };
  };
  const packet = p.lookupTables !== undefined || segs.every((s) => s.luts.length === 0);
  const fits = (ss       , start        )          => {
    const units = known(ss);
    if (units + Math.ceil(units / 10) > MAX_COMPUTE_UNIT_LIMIT) return false;
    if (!packet) return true;
    const message = compileV0Message({ payer: p.trader, instructions: build(ss, start).instructions, recentBlockhash: DUMMY_BLOCKHASH, lookupTables: p.lookupTables ?? [] });
    return transactionSize(message) <= PACKET_DATA_SIZE;
  };
  const txs               = [];
  for (let start = 0; start < segs.length; ) {
    let end = start;
    while (end < segs.length && fits(segs.slice(start, end + 1), start)) end++;
    if (end === start) throw new RangeError(`RouteTooLarge: hop ${start} alone does not fit one transaction (1.4M CU and the ${PACKET_DATA_SIZE}-byte packet); pass its lookup tables`);
    txs.push(build(segs.slice(start, end), start));
    start = end;
  }
  return txs;
}

export function routedTxMessageV2(p                                                                                          )                                                       {
  const message = compileV0Message({ payer: p.trader, instructions: p.tx.instructions, recentBlockhash: p.recentBlockhash, lookupTables: p.lookupTables });
  const size = transactionSize(message);
  return { message, size, fits: size <= PACKET_DATA_SIZE };
}

function floorHop   (h          )                {
  if (h.kind === "edge") throw new RangeError("an EVM outside leg goes through the FloorRouter's DEX: use evmRouteCallV2 (direct FloorAmm calls take none)");
  return h;
}

const v1Hop =    (h          )          => (h.kind === "edge" ? { ...h } : { kind: h.kind, pool: { ...h.pool, state: h.pool.launch } });

export function evmRouteCallV2(q              , p                                                                       )                                              {
  if (q.path.chain !== "evm") throw new RangeError("evmRouteCallV2 builds EVM routes; Solana routes are `routeInstructionsV2`");
  if (q.hops.length !== q.path.hops.length || q.hops.some((h, i) => h.hop !== q.path.hops[i])) throw new RangeError("the quote's hops are not its path's: quote the path again");
  checkFloorHops(q.chainId, q.path.hops.filter((h) => h.kind !== "edge").length);

  const v1 = {
    path: { chain: "evm", from: q.path.from, to: q.path.to, hops: q.path.hops.map(v1Hop) },
    amountIn: q.amountIn, amountOut: q.amountOut, minOut: q.minOut, slippageBps: q.slippageBps,
    hops: q.hops.map((h) => ({ hop: v1Hop(h.hop), amountIn: h.amountIn, amountOut: h.amountOut, minOut: h.minOut })),
  }                         ;
  return evmRouteCall(v1, { router: p.router, to: p.to, deadline: p.deadline, chainId: q.chainId          , ...(p.swapData ? { swapData: p.swapData } : {}) });
}

export function evmHopCallsV2(q              , p                                                                        )            {
  if (q.path.chain !== "evm") throw new RangeError("evmHopCallsV2 builds EVM routes");
  for (const h of q.path.hops) floorHop(h);

  if (q.hops.length !== q.path.hops.length || q.hops.some((h, i) => h.hop !== q.path.hops[i])) throw new RangeError("the quote's hops are not its path's: quote the path again");
  checkFloorHops(q.chainId, q.hops.length);
  const calls            = [];
  let spend = q.amountIn;
  for (const hq of q.hops) {
    const hop = floorHop(hq.hop);
    const pool = hop.pool;
    if (pool.launchId === undefined) throw new RangeError(`pool ${pool.symbol}: an EVM pool needs its launchId`);
    if (hop.kind === "buy") {
      if (p.approve !== false && coinKey(pool.quote) !== EVM_NATIVE) calls.push(approveCall(pool.quote, p.floorAmm, spend));
      calls.push(buyCall({ floorAmm: p.floorAmm, quote: pool.quote, launchId: pool.launchId, amountIn: spend, minTokensOut: hq.minOut, to: p.to, deadline: p.deadline }));
    } else {
      if (p.approve !== false) calls.push(approveCall(pool.token, p.floorAmm, spend));
      calls.push(sellCall({ floorAmm: p.floorAmm, launchId: pool.launchId, tokensIn: spend, minQuoteOut: hq.minOut, to: p.to, deadline: p.deadline }));
    }
    spend = hq.minOut;
  }
  return calls;
}
