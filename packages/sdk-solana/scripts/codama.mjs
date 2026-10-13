// `npm run generate`, step 2: idl/floor_*.json → Codama → src/generated/client/<program>.
//
// Options:
// `syncPackageJson: false` (or the renderer rewrites package.json with its own @solana/kit range),
// `kitImportStrategy: 'rootOnly'` (everything from `@solana/kit` and its `program-client-core` subpath),
// `formatCode: false` (prettier is not a dependency), `importExtension: 'js'`, because this package
// compiles under NodeNext, where an extensionless relative import does not resolve.
// Accounts the IDL fixes to another cluster-specific program (its id, or a PDA under it) are made caller-supplied
// (`dropClusterFixedAccounts`), so the client carries no devnet address beyond each program's own default
// `programAddress`.
// ⛔ Never edit `src/generated/`: regenerate. ⚠ Codama renames `floor_launch`'s `contribution` PDA helper (different seeds in two
//    instructions) to `contributeContribution`, and `floor_hook.register_pool`'s `baseMint` ARG, and says so on stdout — expected, never "fixed" in the IDL.

import { createFromRoot } from 'codama';
import { rootNodeFromAnchor } from '@codama/nodes-from-anchor';
import { renderVisitor } from '@codama/renderers-js';
import { getAddressDecoder } from '@solana/kit';
import { readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PROGRAMS = ['floor_hook', 'floor_launch', 'floor_holders', 'floor_vesting'];

/**
 * Drop the `pda` of any account whose own seeds name ITSELF, and print which. Anchor's IDL generator degrades a
 * seed read out of another account's DATA to that account's ADDRESS, which Codama reports as a cycle and refuses to
 * render; dropping it makes the account caller-supplied, the only correct client behaviour. `floor_launch` (`close_graduation_rent.launch` and four more) and `floor_hook` (`retire_quote` / `set_quote_economics`'s `quote_policy`) have them.
 */
function dropSelfReferentialPdas(idl, program) {
  const dropped = [];
  for (const ix of idl.instructions ?? []) {
    for (const acc of ix.accounts ?? []) {
      if (acc.pda?.seeds?.some((s) => s.kind === 'account' && s.path === acc.name)) {
        delete acc.pda;
        dropped.push(`${ix.name}.${acc.name}`);
      }
    }
  }
  if (dropped.length) console.log(`codama: ${program} — self-referential pda dropped, caller supplies it: ${dropped.join(', ')}`);
}

/**
 * The program ids that differ per cluster: every program of every row of this package's and `@floorsupply/carpenter-sdk`'s
 * deployments.json. An IDL account fixed to one of them (another program's id, or a PDA under another program) would
 * render as a devnet constant inside the generated client; those accounts are made caller-supplied instead, so a client
 * on any cluster passes that cluster's addresses.
 */
function clusterProgramIds() {
  const require = createRequire(import.meta.url);
  const files = [join(pkg, 'deployments.json'), require.resolve('@floorsupply/carpenter-sdk/deployments.json')];
  const ids = new Set();
  for (const f of files) {
    for (const row of Object.values(JSON.parse(readFileSync(f, 'utf8')).clusters)) for (const id of Object.values(row.programs)) ids.add(id);
  }
  return ids;
}

/** Drop every fixed `address` naming another program in `ids`, and every `pda` under a const program in `ids`. */
function dropClusterFixedAccounts(idl, program, ids) {
  const dropped = [];
  const decode = getAddressDecoder();
  const walk = (ixName, accounts) => {
    for (const acc of accounts ?? []) {
      if (acc.accounts) {
        walk(ixName, acc.accounts);
        continue;
      }
      if (acc.address && ids.has(acc.address) && acc.address !== idl.address) {
        delete acc.address;
        dropped.push(`${ixName}.${acc.name}`);
      }
      const p = acc.pda?.program;
      if (p?.kind === 'const' && ids.has(decode.decode(Uint8Array.from(p.value)))) {
        delete acc.pda;
        dropped.push(`${ixName}.${acc.name} (pda)`);
      }
    }
  };
  for (const ix of idl.instructions ?? []) walk(ix.name, ix.accounts);
  if (dropped.length) console.log(`codama: ${program} — ${dropped.length} cluster-fixed accounts made caller-supplied`);
}

const CLUSTER_PROGRAM_IDS = clusterProgramIds();

rmSync(join(pkg, 'src/generated/client'), { recursive: true, force: true });
for (const name of PROGRAMS) {
  const idl = JSON.parse(readFileSync(join(pkg, 'idl', `${name}.json`), 'utf8'));
  dropSelfReferentialPdas(idl, name);
  dropClusterFixedAccounts(idl, name, CLUSTER_PROGRAM_IDS);
  await createFromRoot(rootNodeFromAnchor(idl)).accept(
    renderVisitor(pkg, {
      generatedFolder: `src/generated/client/${name}`,
      syncPackageJson: false,
      kitImportStrategy: 'rootOnly',
      formatCode: false,
      importExtension: 'js',
    }),
  );
  console.log(`codama: ${name} → src/generated/client/${name} (IDL spec ${idl.metadata?.spec ?? '?'})`);
}
