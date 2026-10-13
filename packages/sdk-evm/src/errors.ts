/**
 * The errors this package throws. Each is its own class so a caller can tell a misconfiguration from an RPC failure, and
 * every message names what stopped it.
 */

/** A chain with no row, or a row without a FLOOR set, or a row that breaks its own rules. */
export class DeploymentConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DeploymentConfigError';
  }
}

/** A rate a chain answered above the cap its contract enforces. Refused, never clamped and never quoted. */
export class RateAboveCap extends RangeError {
  constructor(what: string, value: number, cap: number) {
    super(`${what}: ${value} is above its cap ${cap} — refused, never quoted`);
    this.name = 'RateAboveCap';
  }
}

/** A sale whose getters answered something the walk cannot run on. */
export class MalformedSale extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MalformedSale';
  }
}

export type FloorEvmErrorCode =
  /** The token is not a launch on this chain's registry. */
  | 'not_a_launch'
  /** The registry scan ended without the token's entry. */
  | 'not_found'
  /** The chain answered something this package refuses to read (a wrong width, a mismatched pool). */
  | 'contract_mismatch'
  /** The request cannot be built: an amount, a side, a phase, a router. */
  | 'bad_params'
  /** The quote does not reach the caller's minimum. */
  | 'min_out_unmet'
  /** The pool cannot be priced (no quoter, or a quote of nothing). */
  | 'no_quote'
  /** The wallet is on another chain than the calls are for. Nothing was sent. */
  | 'wrong_chain'
  /**
   * Something WAS submitted (`id` and / or `hashes` say what), but its outcome is not known yet: the wait timed out or the
   * status read failed. Keep waiting on `id` / `hashes`; do not send the action again.
   */
  | 'pending'
  /** An EIP-5792 batch the wallet reports as failed (`id` names it). */
  | 'batch_failed'
  /** A transaction on the per-call path reverted (`hashes` lists every one sent); the calls after it were not sent. */
  | 'reverted';

export interface FloorEvmErrorOptions {
  cause?: unknown;
  /** The EIP-5792 batch id, where a batch was submitted. */
  id?: string;
  /** Every transaction hash known to have been submitted, in order. */
  hashes?: readonly `0x${string}`[];
}

export class FloorEvmError extends Error {
  /** The EIP-5792 batch id, where a batch was submitted. */
  readonly id?: string;
  /** Every transaction hash known to have been submitted, in order. */
  readonly hashes?: readonly `0x${string}`[];
  constructor(readonly code: FloorEvmErrorCode, message: string, options: FloorEvmErrorOptions = {}) {
    super(message, options.cause !== undefined ? { cause: options.cause } : undefined);
    this.name = 'FloorEvmError';
    if (options.id !== undefined) this.id = options.id;
    if (options.hashes !== undefined) this.hashes = options.hashes;
  }
}
