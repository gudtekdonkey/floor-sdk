

export { SOLANA_ERRORS, SOLANA_EVENT_SCHEMAS } from "./solana.gen.js";

export { EVM_ERRORS, EVM_EVENT_SCHEMAS, EVM_UNNAMED_EVENT_SIGNATURES } from "./evm.gen.js";

export { ERC20_ABI, ERC20_DECIMALS_SELECTOR, FLOOR_AMM_ABI, FLOOR_AMM_V2_FUNCTIONS } from "./functions.gen.js";

import { SOLANA_ERRORS } from "./solana.gen.js";
import { EVM_ERRORS } from "./evm.gen.js";

let byCode                                          ;
let bySel                                          ;

export const solanaErrorByCode = (code        )                              => (byCode ??= new Map(SOLANA_ERRORS.map((e) => [e.code , e]))).get(code);

export const evmErrorBySelector = (selector        )                              => (bySel ??= new Map(EVM_ERRORS.map((e) => [e.selector , e]))).get(selector.slice(0, 10).toLowerCase());
