import { describe, expect, it } from 'vitest';
import { toEventSelector, toFunctionSelector, type AbiEvent, type AbiFunction } from 'viem';
import * as abi from '../src/abi/index.js';

type Item = { type: string; name?: string; inputs?: readonly Param[]; outputs?: readonly Param[] };
type Param = { name?: string; type: string; components?: readonly Param[] };

const ABIS = (Object.entries(abi) as [string, unknown][]).filter((e) => e[0].endsWith('_ABI') || e[0].endsWith('_EVENTS')) as [string, readonly Item[]][];
const find = <T>(list: readonly unknown[], type: string, name: string) => (list as readonly Item[]).find((x) => x.type === type && x.name === name) as T;
const fn = (list: readonly unknown[], name: string) => toFunctionSelector(find<AbiFunction>(list, 'function', name));
const ev = (list: readonly unknown[], name: string) => toEventSelector(find<AbiEvent>(list, 'event', name));
const err = (list: readonly unknown[], name: string) => {
  const e = find<Item>(list, 'error', name);
  return toFunctionSelector(`${e.name}(${(e.inputs ?? []).map((i) => i.type).join(',')})`);
};

describe('ABI shape', () => {
  it('exports every ABI an integrator needs, and leaves out the Aerodrome, Slipstream and v3-pool interfaces no builder here calls', () => {
    const names = ABIS.map(([n]) => n);
    for (const n of ['HOOK_ABI', 'HOOK_WRITE_ABI', 'POOL_ABI', 'TOKEN_ABI', 'LAUNCH_ABI', 'FACTORY_ABI', 'REGISTRY_ABI', 'POOL_STATE_ABI',
      'UNIVERSAL_ROUTER_ABI', 'V4_ROUTER_ABI', 'V4_QUOTER_ABI', 'V3_QUOTER_V2_ABI', 'PERMIT2_ABI', 'HOLDER_REWARDS_ABI', 'CLAIMS_ABI',
      'VESTING_ABI', 'PRESET_REGISTRY_ABI', 'PLATFORM_TREASURY_ABI', 'FLOOR_TRADER_REFERRALS_ABI', 'HOOK_TRADER_REFERRALS_ABI',
      'FLOOR_SWAP_RATCHET_ABI', 'FLOOR_SWAP_RATCHET_SWEEP_ABI', 'FLOOR_ZAP_ABI', 'TEAM_FEE_SETTINGS_ABI']) expect(names).toContain(n);
    for (const n of ['AERODROME_MIXED_QUOTER_ABI', 'SLIPSTREAM_POOL_ABI', 'V3_POOL_ABI']) expect(names).not.toContain(n);
  });

  it('every fragment is a function, event or error, and every tuple component is NAMED (one anonymous one makes viem decode positionally)', () => {
    const named = (ps: readonly Param[] | undefined, where: string): void => {
      for (const p of ps ?? []) {
        if (p.components) {
          for (const c of p.components) expect(c.name, `${where}: an anonymous tuple component`).toBeTruthy();
          named(p.components, where);
        }
      }
    };
    for (const [n, list] of ABIS) {
      for (const f of list) {
        expect(['function', 'event', 'error'], `${n}.${f.name}`).toContain(f.type);
        expect(f.name, n).toBeTruthy();
        named(f.inputs, `${n}.${f.name}`);
        named(f.outputs, `${n}.${f.name}`);
      }
    }
  });

  it('poolState is the fresh deploy\'s 53 fields / 57 words', () => {
    expect(abi.POOL_STATE_COMPONENTS.length).toBe(53);
    expect(abi.POOL_STATE_WORDS).toBe(57);
    expect(abi.POOL_STATE_ENGINE_WORDS).toBe(57);
  });

  it('selectors and topics are the contracts\' own', () => {
    expect(fn(abi.FLOOR_ZAP_ABI, 'buyWithEth')).toBe('0x1e3cdd26');
    expect(ev(abi.FLOOR_ZAP_ABI, 'ZapBuy')).toBe('0x5550d5076d326329b5d39a746b505c42dcfc52f7415de3c641ea01e9a3025cdb');
    expect(ev(abi.LAUNCH_ABI, 'BoughtFor')).toBe('0x52206f5c49b1ddb4039db225aacda3ff4781df2ad35b86d7f17c370a4899ab76');
    expect(fn(abi.V4_ROUTER_ABI, 'executeActionsAndSweepExcessETH')).toBe('0xa1bfb5ff');
    expect(ev(abi.FACTORY_ABI, 'LaunchCreated')).toBe('0x8321cdc78b126d52df07d4091ee27afee4b1d4b90d231fb5c517ed7d19b8f3f9');
    expect(ev(abi.HOOK_ABI, 'CreatorSlotSet')).toBe('0xa98dcd8a9837aa71182120055c39a2068d9b28af3b87b1a5f326dff24ad2b09d');
    expect(toFunctionSelector('claimCreatorFees(uint8)')).toBe('0xbc8e2237');
    expect(fn(abi.POOL_ABI, 'claimCreatorFees')).toBe('0xbc8e2237');
    expect(err(abi.HOOK_ABI, 'UnknownPool')).toBe('0xf7139e33');
    expect(err(abi.HOOK_ABI, 'RecipientPendingClaim')).toBe('0x5ca477ce');
    expect(fn(abi.CLAIMS_ABI, 'claimPool')).toBe('0xcbb0d95c');
    expect(err(abi.FACTORY_ABI, 'QuoteUnitMismatch')).toBe('0x47f80e73');
    expect(err(abi.FLOOR_ERRORS_ABI, 'PlatformNotConfigured')).toBe('0x1af1398d');
    expect(err(abi.FLOOR_ERRORS_ABI, 'PlatformCutRequired')).toBe('0x224d4ed2');
    expect(fn(abi.TEAM_FEE_SETTINGS_ABI, 'teamFeeRates')).toBe('0x62f99e97');
    expect(fn(abi.FLOOR_SWAP_RATCHET_ABI, 'swapAndRatchet')).toBe('0x02758f5e');
    expect(fn(abi.FLOOR_SWAP_RATCHET_ABI, 'swapAndRatchetWithPermit')).toBe('0x1f9cf753');
    expect(abi.FLOOR_SWAP_RATCHET_SWEEP_ABI).toBe(abi.FLOOR_SWAP_RATCHET_ABI);
    expect(fn(abi.TEAM_STEPS_HOOK_ABI, 'claimTeamProceeds')).toBe('0x35fc69d2');
    expect(fn(abi.HOOK_ABI, 'execute')).toBe('0xe9ae5c53');
    expect(fn(abi.HOOK_ABI, 'poolState')).toBe('0xe0b01bac');
  });

  it('the trade calls match their Solidity signatures', () => {
    const buys = (abi.LAUNCH_ABI as readonly Item[]).filter((x) => x.type === 'function' && x.name === 'buy') as unknown as AbiFunction[];
    expect(buys.map((b) => toFunctionSelector(b)).sort()).toEqual([toFunctionSelector('buy()'), toFunctionSelector('buy(uint256)')].sort());
    expect(fn(abi.TOKEN_ABI, 'sellBack')).toBe(toFunctionSelector('sellBack(uint256)'));
    expect(fn(abi.UNIVERSAL_ROUTER_ABI, 'execute')).toBe(toFunctionSelector('execute(bytes,bytes[],uint256)'));
    expect(fn(abi.PERMIT2_ABI, 'approve')).toBe(toFunctionSelector('approve(address,address,uint160,uint48)'));
    expect(fn(abi.V4_QUOTER_ABI, 'quoteExactInputSingle')).toBe(toFunctionSelector('quoteExactInputSingle(((address,address,uint24,int24,address),bool,uint128,bytes))'));
    expect(fn(abi.REGISTRY_ABI, 'page')).toBe(toFunctionSelector('page(uint256,uint256)'));
    expect(fn(abi.HOOK_ABI, 'poolForAccount')).toBe(toFunctionSelector('poolForAccount(address)'));
  });

  it('the hook surface carries its modules\' events and errors', () => {
    expect(ev(abi.HOOK_ABI, 'HookFee')).toBe(toEventSelector('HookFee(bytes32,address,uint128,uint128)'));
    expect(ev(abi.HOOK_ABI, 'TeamProceedsPaid')).toBe(toEventSelector('TeamProceedsPaid(bytes32,uint8,address,uint256,uint256)'));
    expect(ev(abi.TEAM_STEPS_HOOK_EVENTS, 'TeamProceedsSkipped')).toBe(toEventSelector('TeamProceedsSkipped(bytes32,uint8,address,uint256)'));
    for (const e of ['BadNativeToken', 'NotTeamPayee', 'BadTeamSteps', 'BadSlot']) expect(find(abi.HOOK_ABI, 'error', e), e).toBeTruthy();
    expect(find(abi.HOOK_ABI, 'event', 'LadderFilled')).toBeUndefined();
    expect(fn(abi.HOOK_ABI, 'traderUplineOf')).toBe(toFunctionSelector('traderUplineOf(address)'));
  });

  it('FLOOR_ERRORS_ABI holds every error of every FLOOR export, once per selector', () => {
    const sel = (e: Item) => err([e], e.name!);
    const all = new Set((abi.FLOOR_ERRORS_ABI as readonly Item[]).map(sel));
    expect(all.size).toBe(abi.FLOOR_ERRORS_ABI.length);
    for (const [n, list] of ABIS) {
      if (/^(V4_|V3_|POOL_MANAGER|PERMIT2|POOL_STATE|SWAP_ROUTER|UNIVERSAL_ROUTER)/.test(n)) continue;
      for (const e of list) if (e.type === 'error') expect(all.has(sel(e)), `${n}.${e.name}`).toBe(true);
    }
  });
});
