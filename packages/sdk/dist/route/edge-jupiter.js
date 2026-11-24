

import { base58Decode } from "../codec.js";

export function jupiterSwapInstructionsBody(p                                                                           )         {
  return JSON.stringify({ quoteResponse: p.quote, userPublicKey: p.trader, wrapAndUnwrapSol: true, dynamicComputeUnitLimit: true, ...(p.destinationTokenAccount ? { destinationTokenAccount: p.destinationTokenAccount } : {}) });
}

function base64Decode(s        )             {
  const b = atob(s);
  const out = new Uint8Array(b.length);
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i);
  return out;
}

export function fromJupiter(j                    )              {
  base58Decode(j.programId);
  return {
    programAddress: j.programId,
    accounts: j.accounts.map((a) => ({ address: a.pubkey, role: ((a.isSigner ? 2 : 0) | (a.isWritable ? 1 : 0))                  })),
    data: base64Decode(j.data),
  };
}

export function jupiterLeg(j                         )                {
  return [...(j.setupInstructions ?? []), j.swapInstruction, ...(j.cleanupInstruction ? [j.cleanupInstruction] : []), ...(j.otherInstructions ?? [])].map(fromJupiter);
}

export function jupiterEdgeLeg(j                         )          {
  const jb = (j.computeBudgetInstructions ?? []).map(fromJupiter);
  const limit = jb.find((ix) => ix.data[0] === 2 && ix.data.length >= 5);
  const price = jb.find((ix) => ix.data[0] === 3 && ix.data.length >= 9);
  const dv = (ix             )           => new DataView(ix.data.buffer, ix.data.byteOffset);
  return {
    setup: (j.setupInstructions ?? []).map(fromJupiter),
    swap: [fromJupiter(j.swapInstruction)],
    cleanup: [...(j.cleanupInstruction ? [j.cleanupInstruction] : []), ...(j.otherInstructions ?? [])].map(fromJupiter),
    lookupTableAddresses: [...(j.addressLookupTableAddresses ?? [])],
    ...(limit ? { computeUnits: dv(limit).getUint32(1, true) } : {}),
    ...(price ? { computeUnitPrice: dv(price).getBigUint64(1, true) } : {}),
  };
}
