/**
 * FLOOR's four program IDLs this package ships, and their sha256.
 * ⛔ `IDL_SHA256` hashes the IDL FILE BYTES (`idl/<program>.json`) exactly as `anchor idl build` wrote them; an app's
 *    IDL check compares it with the installed client.
 * ⛔ `./generated/` is written by `npm run generate` (`scripts/sync-idls.mjs`) from `idl/` and `deployments.json`.
 */

import { FLOOR_HOLDERS_IDL, FLOOR_HOOK_IDL, FLOOR_LAUNCH_IDL, FLOOR_VESTING_IDL, IDL_SHA256 } from './generated/idls.ts';
import type { AnchorIdl, ProgramName } from './deployments.ts';

export { FLOOR_HOOK_IDL, FLOOR_LAUNCH_IDL, FLOOR_HOLDERS_IDL, FLOOR_VESTING_IDL, IDL_SHA256 };
/** Alias of `IDL_SHA256`: `{ floor_hook, floor_launch, floor_holders, floor_vesting }` → hex. */
export const idlSha256: Readonly<Record<ProgramName, string>> = IDL_SHA256;

export const IDLS: Readonly<Record<ProgramName, AnchorIdl>> = {
  floor_hook: FLOOR_HOOK_IDL,
  floor_launch: FLOOR_LAUNCH_IDL,
  floor_holders: FLOOR_HOLDERS_IDL,
  floor_vesting: FLOOR_VESTING_IDL,
};
