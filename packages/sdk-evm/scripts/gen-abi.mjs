#!/usr/bin/env node
/**
 * Writes `src/abi/generated.ts` from the FLOOR contracts' Foundry build artifacts.
 *
 *   node packages/sdk-evm/scripts/gen-abi.mjs <foundry out/ directory>
 *
 * Each export is a contract's ABI (or a named slice of one), taken verbatim from `out/<Name>.sol/<Name>.json` with
 * `internalType` dropped and the constructor / fallback / receive left out. Entries keep the artifact's order; an entry
 * reached through several contracts (the singleton hook's modules) is kept once, from the first contract listed; two
 * entries with one signature that differ in more than parameter names (indexing, outputs, mutability) stop the run.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const outDir = process.argv[2];
if (!outDir) {
  console.error('usage: gen-abi.mjs <foundry out/ directory>');
  process.exit(2);
}
const target = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'abi', 'generated.ts');

const strip = (p) => {
  const q = { name: p.name ?? '', type: p.type };
  if (p.indexed !== undefined) q.indexed = p.indexed;
  if (p.components) q.components = p.components.map(strip);
  return q;
};
const clean = (e) => {
  const o = { type: e.type, name: e.name };
  if (e.type === 'function') o.stateMutability = e.stateMutability;
  if (e.type === 'event') o.anonymous = !!e.anonymous;
  o.inputs = (e.inputs ?? []).map(strip);
  if (e.type === 'function') o.outputs = (e.outputs ?? []).map(strip);
  return o;
};
const typeSig = (p) => (p.type.startsWith('tuple') ? `(${p.components.map(typeSig).join(',')})${p.type.slice(5)}` : p.type);
const sig = (e) => `${e.type} ${e.name}(${e.inputs.map(typeSig).join(',')})`;

const cache = new Map();
const load = (name) => {
  if (!cache.has(name)) {
    const art = JSON.parse(readFileSync(join(outDir, `${name}.sol`, `${name}.json`), 'utf8'));
    cache.set(name, art.abi.filter((e) => ['function', 'event', 'error'].includes(e.type)).map(clean));
  }
  return cache.get(name);
};
const unnamedP = (p) => ({ ...p, name: '', ...(p.components ? { components: p.components.map(unnamedP) } : {}) });
const unnamed = (e) => ({ ...e, inputs: e.inputs.map(unnamedP), ...(e.outputs ? { outputs: e.outputs.map(unnamedP) } : {}) });
const renamed = new Set();
/** The union of `entries`, once per signature; a signature seen twice must have one shape. */
const union = (entries, label) => {
  const seen = new Map();
  const out = [];
  for (const e of entries) {
    const s = sig(e);
    const prev = seen.get(s);
    if (prev) {
      if (JSON.stringify(unnamed(prev)) !== JSON.stringify(unnamed(e))) throw new Error(`${label}: ${s} has two shapes`);
      if (JSON.stringify(prev) !== JSON.stringify(e)) renamed.add(`${label}: ${s} (parameter names differ; kept the first)`);
      continue;
    }
    seen.set(s, e);
    out.push(e);
  }
  return out;
};
const of = (contracts, pick = () => true) => contracts.flatMap(load).filter(pick);
const named = (names) => (e) => names.includes(e.name);
const kind = (t) => (e) => e.type === t;
const both = (a, b) => (e) => a(e) && b(e);

// The singleton hook: one address, its behaviour delegated to modules. Their events and errors surface at the hook.
const HOOK = 'SingletonFloorHook';
const MODULES = [
  'SingletonFloorEngine', 'SingletonFloorViews', 'SingletonFloorSetup', 'SingletonFloorReferrals', 'SingletonFloorStairs',
  'SingletonFloorStairSteps', 'SingletonFloorProceeds', 'SingletonFloorSeed', 'SingletonFloorTeamSlots', 'SingletonEngineBase',
];
const HOOK_ALL = [HOOK, ...MODULES];
/** The trader-referral selectors the hook's fallback delegates to the referrals module. */
const REFERRAL_FALLBACK = [
  'creditSaleReferral', 'sweepReferralFees', 'takeReferralOwed', 'restoreReferralOwed', 'setTraderUpline', 'configureReferralVault',
  'traderUplineOf', 'referralOwedOf', 'referralUnsweptOf', 'referralVault', 'saleReferralSum',
];
/** The setup module's per-pool entries routed by `execute` (`SingletonFloorSetup.hookEntries`). */
const SETUP_ENTRIES = ['claimCreatorFees', 'resolveRecipient', 'registerTeamSteps', 'resolveTeamPayee', 'withdrawTeamTokens'];
/** Engine functions only the hook itself reaches (v4 callbacks, the registration-only initializer). */
const ENGINE_INTERNAL = ['beforeInitialize', 'beforeSwap', 'afterSwap', 'unlockCallback', 'getHookPermissions', 'poolManager', 'initialize'];

const hookErrors = union(of(HOOK_ALL, kind('error')), 'hook errors');
const hookEvents = union(of(HOOK_ALL, kind('event')), 'hook events');
const teamHookEvents = hookEvents.filter((e) => e.name.startsWith('Team') && !e.name.startsWith('TeamFee'));
const launchTeam = (e) => /team/i.test(e.name);

const exports = {
  HOOK_ABI: union([
    ...of([HOOK], kind('function')),
    ...of(['SingletonFloorReferrals'], both(kind('function'), named(REFERRAL_FALLBACK))),
    ...hookEvents, ...hookErrors,
  ], 'HOOK_ABI'),
  HOOK_WRITE_ABI: union([...of([HOOK], both(kind('function'), named(['execute']))), ...hookErrors], 'HOOK_WRITE_ABI'),
  POOL_ABI: union([
    ...of(['SingletonFloorViews'], (e) => e.type === 'function' && e.name !== 'selectors'),
    ...of(['SingletonFloorSetup'], both(kind('function'), named(SETUP_ENTRIES))),
    ...of(['SingletonFloorEngine'], (e) => e.type === 'function' && !ENGINE_INTERNAL.includes(e.name)),
    ...hookErrors,
  ], 'POOL_ABI'),
  TEAM_STEPS_HOOK_EVENTS: teamHookEvents,
  TEAM_STEPS_HOOK_ABI: union([
    ...of([HOOK], both(kind('function'), named(['bookTeamSteps', 'teamSteps', 'sweepTeamProceeds', 'claimTeamProceeds', 'sweepStairFees']))),
    ...hookErrors.filter((e) => e.name === 'BadSlot' || /Team/.test(e.name)),
    ...teamHookEvents,
  ], 'TEAM_STEPS_HOOK_ABI'),
  STAIR_SWEEP_HOOK_ABI: of([HOOK], both(kind('function'), named(['sweepStairFees']))),
  HOOK_TRADER_REFERRALS_ABI: union([
    ...of(['SingletonFloorReferrals'], (e) => e.type !== 'function' || REFERRAL_FALLBACK.includes(e.name)),
  ], 'HOOK_TRADER_REFERRALS_ABI'),
  TEAM_FEE_SETTINGS_ABI: of([HOOK], (e) => (e.type === 'function' && e.name === 'teamFeeRates') || (e.type === 'event' && e.name === 'TeamFeeRatesSet')),

  TOKEN_ABI: of(['FloorToken']),
  LAUNCH_ABI: of(['FloorLaunch']),
  TEAM_BUY_LAUNCH_EVENTS: of(['FloorLaunch'], both(kind('event'), launchTeam)),
  TEAM_BUY_LAUNCH_ABI: of(['FloorLaunch'], launchTeam),
  TEAM_PAYEE_READ_ABI: [
    ...of(['FloorToken'], both(kind('function'), named(['launch']))),
    ...of(['FloorLaunch'], both(kind('function'), named(['teamStepTokens']))),
  ],
  HOLDER_REWARDS_ABI: of(['FloorHolderRewards']),
  CLAIMS_ABI: of(['FloorRecipientClaims']),
  TEAM_PAYEE_CLAIMS_ABI: of(['FloorRecipientClaims'], named(['claimPoolPayee', 'PoolPayeeClaimed'])),
  GOVERNANCE_ABI: of(['FloorGovernance']),
  CLAIMS_ROTATION_ABI: union([
    ...of(['FloorRecipientClaims'], named(['setAttestor', 'AttestorChanged', 'ZeroAttestor', 'AttestorIsGovernance', 'GovernanceIsAttestor'])),
    ...of(['FloorGovernance']),
  ], 'CLAIMS_ROTATION_ABI'),
  WALLET_SCAN_ABI: union([
    ...of(['FloorToken'], both(kind('function'), named(['launch']))),
    ...of(['FloorSingletonFactory'], both(kind('event'), named(['LaunchRole']))),
    ...of(['FloorRecipientClaims'], both(kind('event'), named(['PoolRoleClaimed']))),
  ], 'WALLET_SCAN_ABI'),
  REGISTRY_ABI: of(['FloorRegistry']),
  FACTORY_ABI: of(['FloorSingletonFactory']),
  PLATFORM_TREASURY_ABI: of(['FloorPlatformTreasury']),
  PRESET_REGISTRY_ABI: of(['FloorPresetRegistry']),
  VESTING_ABI: of(['FloorVesting']),
  VESTING_LOCKER_ABI: of(['FloorVesting'], (e) => (e.type === 'function' && e.name === 'locks') || (e.type === 'event' && e.name === 'Locked')),
  FLOOR_ZAP_ABI: of(['FloorZap']),
  FLOOR_SWAP_RATCHET_ABI: of(['FloorSwapRatchet']),
  FLOOR_TRADER_REFERRALS_ABI: of(['FloorTraderReferrals']),
};
exports.FACTORY_TEAM_ABI = union([...exports.FACTORY_ABI, ...exports.TEAM_BUY_LAUNCH_EVENTS, ...exports.TEAM_STEPS_HOOK_EVENTS], 'FACTORY_TEAM_ABI');
/** Every error any covered FLOOR contract can revert with, for decoding a revert that bubbled out of a nested call. */
const COVERED = [
  ...HOOK_ALL, 'FloorToken', 'FloorLaunch', 'FloorHolderRewards', 'FloorRecipientClaims', 'FloorGovernance', 'FloorRegistry',
  'FloorSingletonFactory', 'FloorPlatformTreasury', 'FloorPresetRegistry', 'FloorVesting', 'FloorZap', 'FloorSwapRatchet',
  'FloorTraderReferrals',
];
exports.FLOOR_ERRORS_ABI = union(of(COVERED, kind('error')), 'FLOOR_ERRORS_ABI');

// The structs the package decodes by hand.
const fnOf = (abi, name) => abi.find((e) => e.type === 'function' && e.name === name);
const poolStateOut = fnOf(exports.HOOK_ABI, 'poolState').outputs[0];
const components = {
  POOL_STATE_COMPONENTS: poolStateOut.components,
  POOL_KEY_COMPONENTS: poolStateOut.components.find((c) => c.name === 'poolKey').components,
  SLOT_VIEW_COMPONENTS: fnOf(exports.POOL_ABI, 'creatorSlots').outputs[0].components,
  SLOT_COMPONENTS: fnOf(exports.FACTORY_ABI, 'createLaunch').inputs[0].components.find((c) => c.name === 'recipients').components,
};

const lit = (v) => JSON.stringify(v).replace(/"([A-Za-z_][A-Za-z0-9_]*)":/g, '$1:').replace(/"/g, "'");
let ts = `/**\n * GENERATED by \`scripts/gen-abi.mjs\` from the FLOOR contracts' Foundry build artifacts. Do not edit by hand: change the\n * contracts, rebuild, and run the script again.\n */\n`;
for (const [name, v] of Object.entries(components)) ts += `export const ${name} = ${lit(v)} as const;\n`;
for (const [name, abi] of Object.entries(exports)) {
  ts += `export const ${name} = [\n${abi.map((e) => `  ${lit(e)},`).join('\n')}\n] as const;\n`;
}
writeFileSync(target, ts);
for (const [name, abi] of Object.entries(exports)) console.log(`${name.padEnd(28)} ${abi.length}`);
for (const r of renamed) console.log(`note: ${r}`);
