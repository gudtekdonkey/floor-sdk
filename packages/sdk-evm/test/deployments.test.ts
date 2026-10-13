import { describe, expect, it } from 'vitest';
import { getAddress, isAddress } from 'viem';
import {
  ABSENT, CONTRACT_FIELDS, EVM_DEPLOYMENTS, PERMIT2, DeploymentConfigError, checkDeployment, floorDeploymentFor, hasDeployment, isTradable,
  permit2For, setHasTeamFee, swapRatchetOf, swapRouterOf, type Deployment,
  CURRENT_RULES, OLDER_RULES, optionalAddress, rulesOf,
} from '../src/index.js';

const rows = Object.entries(EVM_DEPLOYMENTS).map(([k, d]) => [Number(k), d] as const);
const BRICKED_ROUTER = '0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af';

describe('deployments', () => {
  it('thirteen committed rows; no Anvil row, whose addresses change on every local deploy', () => {
    expect(rows.map(([k]) => k).sort((a, b) => a - b)).toEqual([1, 10, 56, 130, 999, 4663, 5042, 8453, 42161, 46630, 84532, 421614, 11155111]);
    expect(hasDeployment(31337)).toBe(false);
  });

  it('every row\'s key is its chainId, and every row passes its own rules', () => {
    for (const [k, d] of rows) {
      expect(d.chainId).toBe(k);
      expect(() => checkDeployment(k, d)).not.toThrow();
    }
  });

  it('no row carries a zero address, and every address is checksummed as recorded', () => {
    const addrs = (d: Deployment): string[] => ([
      ...CONTRACT_FIELDS.map((f) => d[f]),
      d.recipientClaims, d.holderRewards, d.teamTreasury, d.traderReferrals, d.permit2, d.quoter, d.stateView, d.zap, d.zapAerodrome, d.zapAerodromeRouter, d.swapRatchet, d.presetRegistry,
      ...d.routers.map((r) => r.address), ...(d.v3 ? [d.v3.factory, d.v3.quoter, d.v3.weth] : []),
    ] as (string | undefined)[]).filter((a): a is string => a !== undefined && a !== ABSENT);
    for (const [k, d] of rows) for (const a of addrs(d)) {
      expect(isAddress(a), `${k}: ${a}`).toBe(true);
      expect(BigInt(a), `${k}: a zero address`).not.toBe(0n);
      expect(getAddress(a), `${k}: ${a} is not checksummed`).toBe(a);
    }
  });

  it('Permit2 is the one address on every row that records it, and only beside a UniversalRouter (HyperEVM records it with no venue)', () => {
    expect(PERMIT2).toBe('0x000000000022D473030F116dDEE9F6B43aC78BA3');
    const withPermit2 = rows.filter(([, d]) => d.permit2 !== undefined);
    expect(withPermit2.length).toBe(10);
    for (const [k, d] of withPermit2) {
      expect(d.permit2).toBe(PERMIT2);
      if (k !== 999) expect(d.routers.some((r) => r.kind === 'universal'), `${k}`).toBe(true);
    }
  });

  it('the bricked Universal Router is on no row', () => {
    for (const [, d] of rows) for (const r of d.routers) expect(r.address.toLowerCase()).not.toBe(BRICKED_ROUTER.toLowerCase());
  });

  it('Sepolia carries its test set, with its start block, team vault and FloorSwapRatchet', () => {
    const d = floorDeploymentFor(11155111);
    expect(d.deploymentId).toBe('0x0000000000000000000000000000000000000000000000000000000000000019');
    expect(d.registry).toBe('0x1e816626508Ca63dbb71cD6D1E4794892dbBC989');
    expect(d.factory).toBe('0xeFd78A82F486E37967dc89Bd8b57373703551134');
    expect(d.startBlock).toBe(11895000n);
    expect(d.traderReferrals).toBe('0x5576dcC7CaE2416DAF84d878a3460052BA97Bb0F');
    expect(d.presetRegistry).toBe('0x484dFb3cd0468F9c6F1f0dbf6365F5EBBc138A65');
    expect(d.zap).toBe('0x90944bb31AC6e69ae36853e16040F85BF76Aba20');
    expect(setHasTeamFee(d)).toBe(true);
    const router = swapRouterOf(d);
    expect(router).toEqual({ address: '0x3A9D48AB9751398BbFa63ad67599Bb04e4BdF98b', kind: 'universal', release: '2.0.0' });
    expect(permit2For(d, router)).toBe(PERMIT2);
    expect(swapRatchetOf(d)).toBe('0xeF3B37709384512b1367C031b2801567D952f038');
  });

  it('the five mainnets carry ONE set (deployment 0x…1a, the same addresses on each), with their own start blocks and routers', () => {
    const MAINNET = {
      deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
      registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
      factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
      vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
      recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
      holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
      teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
      traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
      presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
      zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    } as const;
    const RATCHET = '0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371';
    const chains: Record<number, { startBlock: bigint; router: string }> = {
      1: { startBlock: 26178890n, router: '0x4C82D1fBFe28C977cBB58D8C7FF8FCF9F70a2cCA' },
      8453: { startBlock: 52524628n, router: '0xFdf682F51FE81Aa4898F0AE2163d8A55c127fbC7' },
      56: { startBlock: 127278845n, router: '0x8B844f885672f333Bc0042cB669255f93a4C1E6b' },
      4663: { startBlock: 86973488n, router: '0x8876789976dEcBfCbBbe364623C63652db8C0904' },
      5042: { startBlock: 25642396n, router: '0x4fcA4a51Ab4F23A7447b3284fBd7D73289A89Fb1' },
    };
    for (const [id, c] of Object.entries(chains)) {
      const d = floorDeploymentFor(Number(id));
      for (const [f, v] of Object.entries(MAINNET)) expect(d[f as keyof Deployment], `${id}: ${f}`).toBe(v);
      expect(d.startBlock, id).toBe(c.startBlock);
      expect(d.rules, `${id}: runs the current contracts`).toBeUndefined();
      expect(setHasTeamFee(d)).toBe(true);
      const router = swapRouterOf(d);
      expect(router).toEqual({ address: c.router, kind: 'universal', release: '2.1.1' });
      expect(permit2For(d, router)).toBe(PERMIT2);
      expect(swapRatchetOf(d), `${id}: FloorSwapRatchet`).toBe(id === '1' ? null : RATCHET);
      expect(d.zapAerodrome !== undefined, `${id}: zapAerodrome only on Base`).toBe(id === '8453');
    }
    expect(EVM_DEPLOYMENTS[8453]!.zapAerodrome).toBe('0x3b5adDBcEF6785ede8bDf71766Ab1275663129ED');
    expect(EVM_DEPLOYMENTS[8453]!.zapAerodromeRouter).toBe('0xC5b6786D7B64767D775877b0B6A319AD946B11B5');
    expect(EVM_DEPLOYMENTS[1]!.swapRatchet).toBeUndefined();
  });

  it('exactly six chains carry a FLOOR set; every other row is refused', () => {
    const floor = [1, 56, 4663, 5042, 8453, 11155111];
    for (const [k] of rows) {
      if (floor.includes(k)) expect(() => floorDeploymentFor(k), `${k}`).not.toThrow();
      else expect(() => floorDeploymentFor(k), `${k}`).toThrow(DeploymentConfigError);
    }
  });

  it('a chain with no FLOOR set is refused BY NAME, never handed an ABSENT field', () => {
    expect(() => floorDeploymentFor(46630)).toThrow(/Robinhood Testnet \(chain 46630\): registry is not deployed yet/);
    expect(() => floorDeploymentFor(42161)).toThrow(/chain 42161: registry is not deployed yet/);
    expect(() => floorDeploymentFor(999)).toThrow(/HyperEVM \(chain 999\): registry/);
    expect(() => floorDeploymentFor(31337)).toThrow(/No deployment for Anvil \(chain 31337\)/);
    expect(() => floorDeploymentFor(12345)).toThrow(/No deployment for chain 12345/);
    expect(isTradable(4663)).toBe(true);
    expect(isTradable(999)).toBe(false);
    expect(isTradable(46630)).toBe(false);
  });

  it('a caller\'s own row (a local chain) is checked by the same rules', () => {
    const local: Deployment = {
      chainId: 31337, deploymentId: 'anvil-local', poolManager: '0x0000000000000000000000000000000000000001', registry: '0x0000000000000000000000000000000000000002',
      factory: '0x0000000000000000000000000000000000000003', vesting: '0x0000000000000000000000000000000000000004', startBlock: 0n, routers: [],
    };
    expect(floorDeploymentFor(31337, { 31337: checkDeployment(31337, local) }).registry).toBe(local.registry);
    expect(() => checkDeployment(1, local)).toThrow(/chainId 31337/);
    expect(() => checkDeployment(31337, { ...local, registry: '0x0000000000000000000000000000000000000000' })).toThrow(/registry/);
    expect(() => checkDeployment(31337, { ...local, permit2: '0x0000000000000000000000000000000000000009' })).toThrow(/permit2 is not Permit2/);
    expect(() => checkDeployment(31337, { ...local, routers: [{ address: '0x0000000000000000000000000000000000000005', kind: 'universal' }] })).toThrow(/release/);
    expect(() => checkDeployment(31337, { ...local, rules: { saleCost: 'round' as never } })).toThrow(/rules.saleCost/);
    expect(() => checkDeployment(31337, { ...local, rules: { sellBack: 'x' as never } })).toThrow(/rules.sellBack/);
    expect(() => checkDeployment(31337, { ...local, rules: { gapSkim: 1 as never } })).toThrow(/rules.gapSkim/);
  });

  it('contract rules: the Sepolia and mainnet sets run the current contracts; a row that names none gets them; a partial row overrides field by field', () => {
    expect(floorDeploymentFor(11155111).rules).toBeUndefined();
    for (const id of [1, 8453, 56, 4663, 5042]) expect(rulesOf(floorDeploymentFor(id))).toEqual(CURRENT_RULES);
    expect(rulesOf(floorDeploymentFor(11155111))).toEqual({ saleCost: 'ceil', sellBack: 'stateless', gapSkim: true });
    expect(OLDER_RULES).toEqual({ saleCost: 'floor', sellBack: 'proRata', gapSkim: false });
    expect(rulesOf({})).toEqual({ saleCost: 'ceil', sellBack: 'stateless', gapSkim: true });
    expect(rulesOf({ rules: { gapSkim: false } })).toEqual({ ...CURRENT_RULES, gapSkim: false });
  });

  it('optionalAddress: an address, or null for absent and zero', () => {
    expect(optionalAddress(undefined)).toBeNull();
    expect(optionalAddress('0x0000000000000000000000000000000000000000')).toBeNull();
    expect(optionalAddress(ABSENT)).toBeNull();
    expect(optionalAddress(PERMIT2)).toBe(PERMIT2);
  });

  it('the pickers: a v4router is never the swap router, and FloorSwapRatchet needs a UniversalRouter beside it', () => {
    const a = '0x0000000000000000000000000000000000000005' as const, b = '0x0000000000000000000000000000000000000006' as const;
    expect(swapRouterOf({ routers: [{ address: a, kind: 'v4router' }, { address: b, kind: 'poolswaptest' }] })).toEqual({ address: b, kind: 'poolswaptest' });
    expect(swapRatchetOf({ routers: [{ address: b, kind: 'poolswaptest' }], swapRatchet: a })).toBeNull();
    expect(permit2For({ permit2: PERMIT2 }, { kind: 'poolswaptest' })).toBeNull();
  });
});
