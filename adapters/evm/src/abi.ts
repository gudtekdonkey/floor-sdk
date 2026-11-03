

import { decodeAbi, encodeCall, hexToBytes, bytesToHex } from '../vendor/sdk/src/evm/abi.ts';
import { ERC20_ABI, FLOOR_AMM_ABI } from '../vendor/sdk/src/abi/functions.gen.ts';
import { EVM_EVENT_SCHEMAS } from '../vendor/sdk/src/abi/evm.gen.ts';
import { evmSignature } from '../vendor/sdk/src/generated/index.ts';
import type { AbiValue } from '../vendor/sdk/src/evm/abi.ts';

const FN_NAMES = [

  'buy',
  'buyFromBalance',
  'sell',
  'sellFromBalance',

  'quoteBuy',
  'quoteSell',

  'launchState',
  'launchOf',
  'launchCount',

  'quoteEntry',

  'totalSupply',
  'balanceOf',
  'name',
  'symbol',
  'decimals',
] as const;
type FnName = (typeof FN_NAMES)[number];

export const FN = Object.fromEntries(FN_NAMES.map((n) => [n, evmSignature(n)])) as Record<FnName, string>;

type AbiParam = { readonly type: string; readonly name?: string; readonly components?: readonly AbiParam[] };
const ptype = (p: AbiParam): string =>
  p.type.startsWith('tuple') ? `(${(p.components ?? []).map(ptype).join(',')})${p.type.slice(5)}` : p.type;
const FN_ITEMS: readonly { readonly name: string; readonly outputs: readonly AbiParam[] }[] = [...FLOOR_AMM_ABI, ...ERC20_ABI];
const item = (n: string) => {
  const i = FN_ITEMS.find((x) => x.name === n);
  if (!i) throw new Error(`no frozen ABI item ${n}`);
  return i;
};

type OutName = Exclude<FnName, 'buyFromBalance' | 'sellFromBalance'>;
export const OUT: Record<OutName, readonly string[]> = Object.fromEntries(
  FN_NAMES.filter((n) => n !== 'buyFromBalance' && n !== 'sellFromBalance').map((n) => [n, item(n).outputs.map(ptype)]),
) as Record<string, readonly string[]> as Record<OutName, readonly string[]>;

export const LV = Object.fromEntries((item('launchState').outputs[0].components ?? []).map((c, i) => [c.name, i])) as Record<string, number>;

const EV_NAMES = [
  'LaunchCreated',
  'PotsSwept',

  'Graduated',

  'TeamSlotOpened',
  'TeamSlotMember',

  'CreatorBuyUnlocked',

  'TeamBuyClaimed',

  'Transfer',
] as const;
type EvName = (typeof EV_NAMES)[number];
const schema = (n: string) => {
  const e = EVM_EVENT_SCHEMAS.find((x) => x.name === n);
  if (!e) throw new Error(`no generated event schema ${n}`);
  return e;
};

export const EV = Object.fromEntries(EV_NAMES.map((n) => [n, schema(n).signature])) as Record<EvName, string>;

export const EV_DATA: Record<EvName, readonly string[]> = Object.fromEntries(
  EV_NAMES.map((n) => [n, schema(n).params.filter((p) => !p.indexed).map((p) => p.type)]),
) as Record<string, readonly string[]> as Record<EvName, readonly string[]>;

export const LAUNCH_CREATED_LISTED = schema('LaunchCreated').params.filter((p) => !p.indexed).findIndex((p) => p.name === 'listed');

export const TOPIC = Object.fromEntries(EV_NAMES.map((n) => [n, schema(n).topic])) as Record<EvName, string>;

export const NO_DEADLINE = (1n << 256n) - 1n;

export const DEAD = '0x000000000000000000000000000000000000dead';

export const calldata = (sig: string, values: readonly AbiValue[]): string =>
  bytesToHex(encodeCall(sig, values));
export const decode = (types: readonly string[], hex: string): AbiValue[] =>
  decodeAbi(types, hexToBytes(hex));

export const topicUint = (t: string): bigint => BigInt(t);
export const topicAddress = (t: string): string =>
  `0x${t.slice(-40)}`.toLowerCase();
