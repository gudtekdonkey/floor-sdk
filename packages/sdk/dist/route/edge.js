

import { COMPUTE_BUDGET_PROGRAM_ID } from "../solana/instruction.js";

export const EDGE_UNKNOWN_CU = 400_000;

export function edgeLegInstructions(leg         )                {
  const ixs = [...leg.setup, ...leg.swap, ...leg.cleanup];
  for (const ix of ixs) if (ix.programAddress === COMPUTE_BUDGET_PROGRAM_ID) throw new RangeError("a compute budget instruction outside the budget slot");
  return ixs;
}
