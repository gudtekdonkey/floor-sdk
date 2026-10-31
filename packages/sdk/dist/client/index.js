

import { FloorError } from "../errors.js";
import { createEvmClient } from "./evm/index.js";
import { createSolanaClient } from "./solana/index.js";

export { DEFAULT_SLIPPAGE_BPS } from "./types.js";

export { createEvmClient } from "./evm/index.js";

export { createSolanaClient } from "./solana/index.js";

export function createFloorClient                       (opts                       )                 {
  const family = opts.chain?.family;
  if (!opts.transport?.rpc) throw new FloorError("BadArgument", "createFloorClient: transport.rpc is required", { source: "sdk" });
  if (family === "solana") return createSolanaClient(opts                                )                             ;
  if (family === "evm") return createEvmClient(opts                             )                             ;
  throw new FloorError("BadArgument", `createFloorClient: chain.family ${String(family)}`, { source: "sdk" });
}
