

import { bytesToHex, checksumAddress, decodeAbi, encodeCall } from "../evm/abi.js";
import { planEvmClaimAllV2 } from "./claims-evm.js";
import { EVM_TOKEN_SCALE } from "./market-evm.js";
import { evmSignature } from "../generated/index.js";

export const MAX_LAUNCHES_PER_OWED_READ_V2 = 100;

                            

const OWED_V2_SIG = evmSignature("owedV2");

const OWED_V2_LAUNCH_ROWS = "(uint256,address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint64,uint256)[]";
const OWED_V2_QUOTE_ROWS = "(address,uint256,uint256,uint256,uint256,uint256)[]";

function owedCall(wallet        , launchIds                   , quotes                   )         {
  return bytesToHex(encodeCall(OWED_V2_SIG, [checksumAddress(wallet), [...launchIds], quotes.map((q) => checksumAddress(q))]));
}

function toMath(chain        , what        )         {
  if (chain % EVM_TOKEN_SCALE !== 0n) throw new RangeError(`owedV2.${what} ${chain} is not a whole math unit`);
  return chain / EVM_TOKEN_SCALE;
}

function decodeOwed(data                     )                {
  const [ls, qs] = decodeAbi([OWED_V2_LAUNCH_ROWS, OWED_V2_QUOTE_ROWS], data)                              ;
  const n = (v         )         => v          ;
  return {
    launches: ls.map((r) => ({
      launchId: n(r[0]), quote: checksumAddress(r[1]          ), token: checksumAddress(r[2]          ), creator: n(r[3]), creatorRef: n(r[4]),
      teamSlot: n(r[5]), holder: n(r[6]), teamBuy: toMath(n(r[7]), "teamBuy"), teamBuyLocked: toMath(n(r[8]), "teamBuyLocked"),
      shareBought: toMath(n(r[10]), "shareBought"), unlockAt: n(r[9]),
    })),
    quotes: qs.map((r) => ({ quote: checksumAddress(r[0]          ), creator: n(r[1]), creatorRef: n(r[2]), teamSlot: n(r[3]), traderRef: n(r[4]), payout: n(r[5]) })),
  };
}

export function evmOwedReaderOfClaimsViewV2(p

 )                        {
  const to = checksumAddress(p.floorAmm);
  const per = p.maxLaunchesPerRead ?? MAX_LAUNCHES_PER_OWED_READ_V2;
  if (!Number.isInteger(per) || per < 1) throw new RangeError("maxLaunchesPerRead must be ≥ 1");
  return {
    async owed(wallet, launchIds, quotes) {
      const out                = { launches: [], quotes: [] };
      for (let i = 0; i === 0 || i < launchIds.length; i += per) {
        const ids = launchIds.slice(i, i + per);
        const qs = i === 0 ? quotes : [];
        const r = decodeOwed(await p.ethCall(to, owedCall(wallet, ids, qs)));
        if (r.launches.length !== ids.length || r.quotes.length !== qs.length) throw new RangeError("owedV2: one row per launch and per quote");
        out.launches.push(...r.launches);
        out.quotes.push(...r.quotes);
      }
      return out;
    },
  };
}

export async function planEvmClaimAll(p

 )                         {
  const reader = evmOwedReaderOfClaimsViewV2({ floorAmm: p.floorAmm, ethCall: p.ethCall, maxLaunchesPerRead: p.maxLaunchesPerRead });
  const owed = await reader.owed(p.wallet, p.launchIds, p.quotes);
  const plan = planEvmClaimAllV2({
    floorAmm: p.floorAmm, wallet: p.wallet, owed, payoutTo: p.payoutTo, maxQuotesPerCall: p.maxQuotesPerCall, maxLaunchesPerCall: p.maxLaunchesPerCall,
  });
  return { ...plan, owed };
}
