// The deployments.json rules `npm run generate` enforces, as a function the tests can call on synthetic rows.
//
// - The `mainnet` and `local` rows name the program ids this package's IDLs were built for: each must equal the IDL's
//   `address`. A client generated against the wrong program is the one thing generation must stop.
// - Every other row (devnet, ...) may name its own program ids: each is checked to be a base58 32-byte public key.
// - Every derivable account of a row is checked against its derivation from THAT row's program ids.
// - `chainKey` is `solana:<genesis32>` (or `null` on `local`); `alt` is a public key or `null`.

import { getAddressEncoder, getProgramDerivedAddress, isAddress } from '@solana/kit';

/** Rows whose program ids must equal the IDLs' `address`. */
export const IDL_PINNED_ROWS = ['mainnet', 'local'];

const utf8 = (s) => new TextEncoder().encode(s);

/** `[account key, program key, seeds]`: the accounts a row may carry that are PDAs of its own programs. */
export const DERIVED_ACCOUNTS = [
  ['hookConfig', 'floorHook', [utf8('hook_config')]],
  ['launchConfig', 'floorLaunch', [utf8('launch_config')]],
  ['teamTreasury', 'floorHook', [utf8('team_treasury')]],
];
/** Accounts every row must carry. */
export const REQUIRED_ACCOUNTS = ['hookConfig', 'launchConfig'];

function isPubkey(s) {
  if (typeof s !== 'string' || !isAddress(s)) return false;
  return getAddressEncoder().encode(s).length === 32;
}

async function pda(program, seeds) {
  return (await getProgramDerivedAddress({ programAddress: program, seeds }))[0];
}

/**
 * Validate a parsed deployments.json. `idlAddress` maps a `programs` key to its IDL's `address`. Returns the list of
 * problems (empty when valid); never throws on bad data.
 */
export async function validateDeployments(deployments, idlAddress) {
  const problems = [];
  if (deployments?.version !== 1) problems.push('version must be 1');
  for (const [cluster, row] of Object.entries(deployments?.clusters ?? {})) {
    const where = `deployments.json ${cluster}`;
    const pinned = IDL_PINNED_ROWS.includes(row.name);
    for (const [key, address] of Object.entries(idlAddress)) {
      const id = row.programs?.[key];
      if (!isPubkey(id)) {
        problems.push(`${where}.programs.${key} is not a base58 32-byte public key: ${id}`);
      } else if (pinned && id !== address) {
        problems.push(`${where}.programs.${key} = ${id} but the IDL address is ${address} (a ${row.name} row must match the shipped IDL)`);
      }
    }
    for (const k of REQUIRED_ACCOUNTS) if (typeof row.accounts?.[k] !== 'string') problems.push(`${where}.accounts.${k} missing`);
    for (const [k, program, seeds] of DERIVED_ACCOUNTS) {
      const have = row.accounts?.[k];
      if (have === undefined) continue;
      if (!isPubkey(have)) {
        problems.push(`${where}.accounts.${k} is not a base58 32-byte public key: ${have}`);
        continue;
      }
      const programId = row.programs?.[program];
      if (!isPubkey(programId)) continue;
      const want = await pda(programId, seeds);
      if (want !== have) problems.push(`${where}.accounts.${k} = ${have} but its derivation under ${program} ${programId} is ${want}`);
    }
    if (row.alt !== null && !isPubkey(row.alt)) problems.push(`${where}.alt is neither null nor a public key: ${row.alt}`);
    if (cluster === 'local' ? row.chainKey !== null : row.chainKey !== cluster) {
      problems.push(`${where}.chainKey must be ${cluster === 'local' ? 'null' : 'the row key'}: ${row.chainKey}`);
    }
    if (row.chainKey !== null && !/^solana:[1-9A-HJ-NP-Za-km-z]{32}$/.test(row.chainKey)) {
      problems.push(`${where}.chainKey is not solana:<genesis32>: ${row.chainKey}`);
    }
    for (const [mint, q] of Object.entries(row.quotes ?? {})) {
      if (!isPubkey(mint)) {
        problems.push(`${where}.quotes key ${mint} is not a public key`);
        continue;
      }
      const hook = row.programs?.floorHook;
      if (!isPubkey(q?.hook?.policy)) problems.push(`${where}.quotes.${mint}.hook.policy is not a public key`);
      else if (isPubkey(hook)) {
        const want = await pda(hook, [utf8('quote'), getAddressEncoder().encode(mint)]);
        if (want !== q.hook.policy) problems.push(`${where}.quotes.${mint}.hook.policy = ${q.hook.policy} but ["quote", mint] under floorHook is ${want}`);
      }
    }
  }
  return problems;
}
