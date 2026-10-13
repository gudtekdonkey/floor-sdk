/**
 * FLOOR's EVM ABIs, as `as const` arrays so viem infers every argument and return type.
 *
 * FLOOR's own contracts are in `./generated.ts`, written by `scripts/gen-abi.mjs` from the contracts' build artifacts
 * (names, order and components are the contracts' own). This file adds the Uniswap / Permit2 interfaces the SDK calls,
 * the pool-state width constants and two aliases.
 *
 * Traps:
 * - `HOOK_ABI` is the SINGLETON hook: one `SingletonFloorHook` per chain keys every pool by `poolId`. It carries the hook's
 *   own functions, the trader-referral functions its fallback serves, and every event and error its modules emit. The
 *   per-pool functions in `POOL_ABI` (`bidRawQuotePerToken`, `claimCreatorFees`, ...) go INSIDE `execute(poolId, data)`,
 *   never to the hook address directly: a direct call reaches the hook's fallback and reverts `UnknownPool()`. The calldata
 *   is the per-pool call (the hook appends the pool id), and the answer is ABI-encoded twice (an outer `bytes` around the
 *   function's own return). `execute` is `payable`; read through it with `call` / `simulateContract`.
 * - `FLOOR_ERRORS_ABI` is every error the covered contracts declare: a revert that bubbles out of a nested call (a factory
 *   create reaching the hook, a sale reaching the preset registry) decodes against it, not against the called contract.
 * - `poolState` is read through `decodePoolState` (`../reads.ts`), which refuses any width but `POOL_STATE_WORDS`.
 */
import { FLOOR_SWAP_RATCHET_ABI, FLOOR_TRADER_REFERRALS_ABI, POOL_STATE_COMPONENTS } from './generated.js';

export * from './generated.js';
export const POOL_STATE_UNDECODED_TAIL = [
] as const;
export const POOL_STATE_WORDS = POOL_STATE_COMPONENTS.reduce((n, c) => n + ('components' in c ? c.components.length : 1), 0);
export const POOL_STATE_ENGINE_WORDS = POOL_STATE_WORDS + POOL_STATE_UNDECODED_TAIL.length;
/** `FloorSwapRatchet.swapAndRatchet` takes the `sweep` field; the name is kept for callers that picked the sweep build. */
export const FLOOR_SWAP_RATCHET_SWEEP_ABI = FLOOR_SWAP_RATCHET_ABI;
export const TRADER_REFERRALS_ABI = FLOOR_TRADER_REFERRALS_ABI;
/** `SwapAndRatchet.ratchet`: what the ratchet step did. */
export const RATCHET_RESULT = { NotAsked: 0, NothingDue: 1, Ran: 2, Failed: 3 } as const;
export const POOL_MANAGER_ABI = [
  { type: 'event', name: 'Swap', inputs: [
    { name: 'id', type: 'bytes32', indexed: true }, { name: 'sender', type: 'address', indexed: true },
    { name: 'amount0', type: 'int128', indexed: false }, { name: 'amount1', type: 'int128', indexed: false },
    { name: 'sqrtPriceX96', type: 'uint160', indexed: false }, { name: 'liquidity', type: 'uint128', indexed: false },
    { name: 'tick', type: 'int24', indexed: false }, { name: 'fee', type: 'uint24', indexed: false },
  ] },
] as const;
export const PERMIT2_ABI = [
  { type: 'function', name: 'approve', stateMutability: 'nonpayable',
    inputs: [
      { name: 'token', type: 'address' }, { name: 'spender', type: 'address' },
      { name: 'amount', type: 'uint160' }, { name: 'expiration', type: 'uint48' },
    ], outputs: [] },
  { type: 'function', name: 'allowance', stateMutability: 'view',
    inputs: [{ name: 'user', type: 'address' }, { name: 'token', type: 'address' }, { name: 'spender', type: 'address' }],
    outputs: [{ name: 'amount', type: 'uint160' }, { name: 'expiration', type: 'uint48' }, { name: 'nonce', type: 'uint48' }] },
] as const;
export const POOL_STATE_ABI = [
  { type: 'function', name: 'extsload', stateMutability: 'view', inputs: [{ name: 'slot', type: 'bytes32' }], outputs: [{ type: 'bytes32' }] },
] as const;
export const POOL_STATE_BATCH_ABI = [
  { type: 'function', name: 'extsload', stateMutability: 'view', inputs: [{ name: 'slots', type: 'bytes32[]' }], outputs: [{ type: 'bytes32[]' }] },
] as const;
export const SWAP_ROUTER_ABI = [
  { type: 'function', name: 'swap', stateMutability: 'payable',
    inputs: [
      { name: 'key', type: 'tuple', components: [
        { name: 'currency0', type: 'address' }, { name: 'currency1', type: 'address' },
        { name: 'fee', type: 'uint24' }, { name: 'tickSpacing', type: 'int24' }, { name: 'hooks', type: 'address' },
      ] },
      { name: 'params', type: 'tuple', components: [
        { name: 'zeroForOne', type: 'bool' }, { name: 'amountSpecified', type: 'int256' },
        { name: 'sqrtPriceLimitX96', type: 'uint160' },
      ] },
      { name: 'testSettings', type: 'tuple', components: [
        { name: 'takeClaims', type: 'bool' }, { name: 'settleUsingBurn', type: 'bool' },
      ] },
      { name: 'hookData', type: 'bytes' },
    ],
    outputs: [{ type: 'int256' }] },
] as const;
export const UNIVERSAL_ROUTER_ABI = [
  { type: 'function', name: 'execute', stateMutability: 'payable',
    inputs: [
      { name: 'commands', type: 'bytes' }, { name: 'inputs', type: 'bytes[]' },
      { name: 'deadline', type: 'uint256' },
    ],
    outputs: [] },
] as const;
export const V4_QUOTER_ABI = [
  { type: 'function', name: 'quoteExactInputSingle', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'poolKey', type: 'tuple', components: [
        { name: 'currency0', type: 'address' }, { name: 'currency1', type: 'address' },
        { name: 'fee', type: 'uint24' }, { name: 'tickSpacing', type: 'int24' },
        { name: 'hooks', type: 'address' },
      ] },
      { name: 'zeroForOne', type: 'bool' },
      { name: 'exactAmount', type: 'uint128' },
      { name: 'hookData', type: 'bytes' },
    ] }],
    outputs: [{ name: 'amountOut', type: 'uint256' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactOutputSingle', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'poolKey', type: 'tuple', components: [
        { name: 'currency0', type: 'address' }, { name: 'currency1', type: 'address' },
        { name: 'fee', type: 'uint24' }, { name: 'tickSpacing', type: 'int24' },
        { name: 'hooks', type: 'address' },
      ] },
      { name: 'zeroForOne', type: 'bool' },
      { name: 'exactAmount', type: 'uint128' },
      { name: 'hookData', type: 'bytes' },
    ] }],
    outputs: [{ name: 'amountIn', type: 'uint256' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactInput', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'exactCurrency', type: 'address' },
      { name: 'path', type: 'tuple[]', components: [
        { name: 'intermediateCurrency', type: 'address' }, { name: 'fee', type: 'uint24' },
        { name: 'tickSpacing', type: 'int24' }, { name: 'hooks', type: 'address' }, { name: 'hookData', type: 'bytes' },
      ] },
      { name: 'exactAmount', type: 'uint128' },
    ] }],
    outputs: [{ name: 'amountOut', type: 'uint256' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactOutput', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'exactCurrency', type: 'address' },
      { name: 'path', type: 'tuple[]', components: [
        { name: 'intermediateCurrency', type: 'address' }, { name: 'fee', type: 'uint24' },
        { name: 'tickSpacing', type: 'int24' }, { name: 'hooks', type: 'address' }, { name: 'hookData', type: 'bytes' },
      ] },
      { name: 'exactAmount', type: 'uint128' },
    ] }],
    outputs: [{ name: 'amountIn', type: 'uint256' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'poolManager', stateMutability: 'view', inputs: [], outputs: [{ type: 'address' }] },
] as const;
export const V3_QUOTER_V2_ABI = [
  { type: 'function', name: 'quoteExactInputSingle', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'tokenIn', type: 'address' }, { name: 'tokenOut', type: 'address' }, { name: 'amountIn', type: 'uint256' },
      { name: 'fee', type: 'uint24' }, { name: 'sqrtPriceLimitX96', type: 'uint160' },
    ] }],
    outputs: [{ name: 'amountOut', type: 'uint256' }, { name: 'sqrtPriceX96After', type: 'uint160' }, { name: 'initializedTicksCrossed', type: 'uint32' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactOutputSingle', stateMutability: 'nonpayable',
    inputs: [{ name: 'params', type: 'tuple', components: [
      { name: 'tokenIn', type: 'address' }, { name: 'tokenOut', type: 'address' }, { name: 'amount', type: 'uint256' },
      { name: 'fee', type: 'uint24' }, { name: 'sqrtPriceLimitX96', type: 'uint160' },
    ] }],
    outputs: [{ name: 'amountIn', type: 'uint256' }, { name: 'sqrtPriceX96After', type: 'uint160' }, { name: 'initializedTicksCrossed', type: 'uint32' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactInput', stateMutability: 'nonpayable',
    inputs: [{ name: 'path', type: 'bytes' }, { name: 'amountIn', type: 'uint256' }],
    outputs: [{ name: 'amountOut', type: 'uint256' }, { name: 'sqrtPriceX96AfterList', type: 'uint160[]' }, { name: 'initializedTicksCrossedList', type: 'uint32[]' }, { name: 'gasEstimate', type: 'uint256' }] },
  { type: 'function', name: 'quoteExactOutput', stateMutability: 'nonpayable',
    inputs: [{ name: 'path', type: 'bytes' }, { name: 'amountOut', type: 'uint256' }],
    outputs: [{ name: 'amountIn', type: 'uint256' }, { name: 'sqrtPriceX96AfterList', type: 'uint160[]' }, { name: 'initializedTicksCrossedList', type: 'uint32[]' }, { name: 'gasEstimate', type: 'uint256' }] },
] as const;
export const V4_ROUTER_ABI = [
  { type: 'function', name: 'executeActionsAndSweepExcessETH', stateMutability: 'payable',
    inputs: [{ name: 'params', type: 'bytes' }], outputs: [] },
] as const;
export const V4_QUOTER_ERRORS_ABI = [
  { type: 'error', name: 'UnexpectedRevertBytes', inputs: [{ name: 'revertData', type: 'bytes' }] },
  { type: 'error', name: 'NotEnoughLiquidity', inputs: [{ name: 'poolId', type: 'bytes32' }] },
] as const;
