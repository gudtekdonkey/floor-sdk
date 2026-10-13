/**
 * Where FLOOR's contracts and Uniswap's v4 contracts are, per EVM chain id — plain data, no environment reads.
 *
 * Not included:
 * - chain 31337 (Anvil): its addresses change on every local deploy. A caller with a local chain passes its own row
 *   (`createEvmAdapter({ deployments: { 31337: {...} } })`), checked by the same rules as these rows (`checkDeployment`).
 * - RPC endpoints: the caller names its own transport or RPC URL.
 *
 * Two kinds of row share this table: a chain with a FLOOR set (Ethereum, Base, BSC, Robinhood Chain and Arc mainnets, which
 * share one set of addresses, and Sepolia), and chains that carry only Uniswap's v4 contracts (FLOOR is not deployed there). `floorDeploymentFor` REFUSES the second kind by name —
 * it never hands back an `ABSENT` field as if it were an address.
 */
import { isAddress, type Address } from 'viem';
import { DeploymentConfigError } from './errors.js';

/** How a swap reaches the pool. `universal` = Uniswap's UniversalRouter (pulls a token through Permit2); `poolswaptest` =
 *  v4-core's TEST harness (no minimum-out, no deadline; testnets only); `v4router` = a dev `MockV4Router` (local only). */
export type RouterKind = 'poolswaptest' | 'universal' | 'v4router';
/** The UniversalRouter release a `universal` row runs: 2.1.1 adds `minHopPriceX36` inside every v4 swap struct; 2.0.0 does not. */
export type UrRelease = '2.0.0' | '2.1.1';
export type SwapRouter = { address: Address; kind: 'poolswaptest' } | { address: Address; kind: 'universal'; release: UrRelease };

export interface Deployment {
  /** Equal to this row's key in `EVM_DEPLOYMENTS` (`checkDeployment` enforces it). */
  chainId: number;
  /** An input to every CREATE2 salt of the set. Empty where FLOOR is not deployed. */
  deploymentId: string;
  explorer?: string;
  /** Uniswap's canonical v4 PoolManager on this chain — the one FLOOR's hook is (or will be) deployed against. */
  poolManager: Address;
  /** FLOOR's `FloorRegistry`. `ABSENT` where FLOOR is not deployed. */
  registry: Address;
  /** FLOOR's `FloorSingletonFactory`. `ABSENT` where FLOOR is not deployed. */
  factory: Address;
  /** FLOOR's `FloorVesting`. `ABSENT` where FLOOR is not deployed. */
  vesting: Address;
  /** The chain's one singleton FLOOR hook: every FLOOR pool on the chain is a v4 pool keyed by this hook (an indexer finds
   *  them by it in the PoolManager's `Initialize`). Optional. */
  singletonHook?: Address;
  /** The timelock that owns the set's upgrades. Optional. */
  upgradeTimelock?: Address;
  /** The platform treasury. Optional. */
  platformTreasury?: Address;
  /** `FloorRecipientClaims`. Optional: missing means "not part of this deployment". */
  recipientClaims?: Address;
  /** The chain's one `FloorHolderRewards` proxy. Optional. */
  holderRewards?: Address;
  /** The team vault `sweepTeamFees` pays the FLOOR Platform fee into. ITS PRESENCE is the signal that this set's contracts
   *  carry the FLOOR Platform fee ABI (`setHasTeamFee`); a set without it is never asked a rate and charges 0. */
  teamTreasury?: Address;
  /** `FloorTraderReferrals`. Optional. */
  traderReferrals?: Address;
  /** The block the set was deployed at — where a log scan starts. `0n` where FLOOR is not deployed. */
  startBlock: bigint;
  /** Every router, in preference order. Read through `swapRouterOf`, never by index. Empty = no swap router. */
  routers: readonly { address: Address; kind: RouterKind; release?: UrRelease }[];
  /** Only for a row whose router pulls through Permit2. Omitted = this router needs none. */
  permit2?: Address;
  /** Uniswap's V4Quoter — the minimum-out source for a hooked pool. */
  quoter?: Address;
  stateView?: Address;
  /** Uniswap v3 as this row's UniversalRouter reaches it (route hops). */
  v3?: { factory: Address; quoter: Address; weth: Address };
  /** `FloorZap` (ETH → a stock quote → a sale buy). */
  zap?: Address;
  zapAerodrome?: Address;
  zapAerodromeRouter?: Address;
  /** `FloorPresetRegistry`: the launch presets' rows. Optional. */
  presetRegistry?: Address;
  /** `FloorSwapRatchet`: a pool trade through it can run a due ratchet in the same transaction. Only beside a `universal` router. */
  swapRatchet?: Address;
  /** Which contract behaviours this set's FLOOR contracts have. Omitted fields default to `CURRENT_RULES`. */
  rules?: Partial<ContractRules>;
  note?: string;
}

/**
 * The contract behaviours that differ between FLOOR contract versions, which a quote must model to match the set it reads.
 */
export interface ContractRules {
  /** How `FloorLaunch._walk` rounds a tranche's cost: `ceil` = `(tokens·price + 1e18 − 1) / 1e18`, `floor` = `tokens·price / 1e18`. */
  saleCost: 'floor' | 'ceil';
  /** What `FloorLaunch._sell` pays: `stateless` = `min(tokens·p0 / 1e18, contributed)`; `proRata` = `contributed·tokens / allocation`. */
  sellBack: 'stateless' | 'proRata';
  /** Whether the engine scales a BUY's floor skim by the cushion gap (`_gapSkim`, the linear ramp; `gapSkimBps`). */
  gapSkim: boolean;
}

/** The current FLOOR contracts: costs rounded up, the stateless sell-back, the gap buy skim. */
export const CURRENT_RULES: Readonly<ContractRules> = { saleCost: 'ceil', sellBack: 'stateless', gapSkim: true };
/** Older FLOOR contracts: costs rounded down, the pro-rata sell-back, no gap buy skim. For a caller's row of such a set. */
export const OLDER_RULES: Readonly<ContractRules> = { saleCost: 'floor', sellBack: 'proRata', gapSkim: false };

/** A row's contract rules: its own `rules` over `CURRENT_RULES`. */
export function rulesOf(d: Pick<Deployment, 'rules'>): ContractRules {
  return { ...CURRENT_RULES, ...(d.rules ?? {}) };
}

/**
 * NOT an address: a REQUIRED FLOOR contract that is not deployed on the chain. `''`, never a plausible hex string — a fake
 * address does not throw, it reads address zero and lists nothing. `floorDeploymentFor` refuses a row still carrying one.
 * Only the `CONTRACT_FIELDS` use it; every optional field uses `undefined`.
 */
export const ABSENT = '' as Address;

/** The fields `floorDeploymentFor` insists on, in the order they are needed. */
export const CONTRACT_FIELDS = ['registry', 'factory', 'poolManager', 'vesting'] as const satisfies readonly (keyof Deployment)[];

/** Permit2 — one address on every chain that has it (a CREATE2 deploy); each row that names it was checked against the chain. */
export const PERMIT2 = '0x000000000022D473030F116dDEE9F6B43aC78BA3' as Address;

/** Labels and native symbols for the EVM chain ids FLOOR names. */
export const EVM_NETWORKS: Readonly<Record<number, { label: string; nativeSymbol: string; testnet: boolean }>> = {
  4663: { label: 'Robinhood Chain', nativeSymbol: 'ETH', testnet: false },
  8453: { label: 'Base', nativeSymbol: 'ETH', testnet: false },
  56: { label: 'BSC', nativeSymbol: 'BNB', testnet: false },
  999: { label: 'HyperEVM', nativeSymbol: 'HYPE', testnet: false },
  1: { label: 'Ethereum', nativeSymbol: 'ETH', testnet: false },
  11155111: { label: 'Sepolia', nativeSymbol: 'ETH', testnet: true },
  31337: { label: 'Anvil', nativeSymbol: 'ETH', testnet: true },
  46630: { label: 'Robinhood Testnet', nativeSymbol: 'ETH', testnet: true },
  5042: { label: 'Arc', nativeSymbol: 'USDC', testnet: false },
};

/**
 * The rows, keyed by chain id. Six carry a FLOOR set: the five mainnets (1, 8453, 56, 4663, 5042; one set of addresses on
 * all five, except Base's `zapAerodrome` and Ethereum, which has no `swapRatchet`) and Sepolia (11155111). The others carry
 * Uniswap's v4 contracts only.
 */
export const EVM_DEPLOYMENTS: Readonly<Record<number, Deployment>> = {
  11155111: {
    chainId: 11155111,
    deploymentId: '0x0000000000000000000000000000000000000000000000000000000000000019',
    explorer: 'https://sepolia.etherscan.io',
    poolManager: '0xE03A1074c86CFeDd5C142C4F04F1a1536e203543',
    registry: '0x1e816626508Ca63dbb71cD6D1E4794892dbBC989',
    factory: '0xeFd78A82F486E37967dc89Bd8b57373703551134',
    vesting: '0x112d89ae8649DB6e9e4a13F6A9f87be9c89E512C',
    recipientClaims: '0x9789D099378BC19bF539189792843eCd972DCAbb',
    holderRewards: '0x16D24e5F3d93F5e53ea4e81DdCe0D7FEb39C5021',
    teamTreasury: '0xDe11874af6a3Aa96b2d09d5039a30c43537713E4',
    traderReferrals: '0x5576dcC7CaE2416DAF84d878a3460052BA97Bb0F',
    startBlock: 11895000n,
    swapRatchet: '0xeF3B37709384512b1367C031b2801567D952f038',
    zap: '0x90944bb31AC6e69ae36853e16040F85BF76Aba20',
    presetRegistry: '0x484dFb3cd0468F9c6F1f0dbf6365F5EBBc138A65',
    routers: [
      { address: '0x3A9D48AB9751398BbFa63ad67599Bb04e4BdF98b', kind: 'universal', release: '2.0.0' },
      { address: '0x9B6b46e2c869aa39918Db7f52f5557FE577B6eEe', kind: 'poolswaptest' },
    ],
    permit2: PERMIT2,
    quoter: '0x61B3f2011A92d183C7dbaDBdA940a7555Ccf9227',
    stateView: '0xE1Dd9c3fA50EDB962E442f60DfBc432e24537E4C',
    note: 'Sepolia test set. Swaps go through the UniversalRouter (2.0.0); PoolSwapTest is recorded second and unused. FloorSwapRatchet and FloorZap wrap the same UniversalRouter. Quote allowlist empty (native quote only).',
  },
  84532: {
    chainId: 84532, deploymentId: '',
    poolManager: '0x05E73354cFDd6745C338b50BcFDfA3Aa6fA03408',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [{ address: '0x8B5bcC363ddE2614281aD875bad385E0A785D3B9', kind: 'poolswaptest' }],
    quoter: '0x4A6513c898fe1B2d0E78d3b0e0A4a151589B1cBa',
    stateView: '0x571291b572ed32ce6751a2Cb2486EbEe8DEfB9B4',
    note: 'Base Sepolia. Only the older Universal Router generation is deployed here; PoolSwapTest only.',
  },
  421614: {
    chainId: 421614, deploymentId: '',
    poolManager: '0xFB3e0C6F74eB1a21CC1Da29aeC80D2Dfe6C9a317',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [{ address: '0xf3A39C86dbd13C45365E57FB90fe413371F65AF8', kind: 'poolswaptest' }],
    quoter: '0x7dE51022d70A725b508085468052E25e22b5c4c9',
    stateView: '0x9D467FA9062b6e9B1a46E26007aD82db116c67cB',
    note: 'Arbitrum Sepolia. PoolSwapTest only.',
  },
  46630: {
    chainId: 46630, deploymentId: '',
    explorer: 'https://explorer.testnet.chain.robinhood.com',
    poolManager: '0x8366a39CC670B4001A1121B8F6A443A643e40951',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [],
    note: 'Robinhood Testnet. Uniswap v4 PoolManager only; no swap router; no FLOOR set.',
  },
  4663: {
    chainId: 4663,
    poolManager: '0x8366a39CC670B4001A1121B8F6A443A643e40951',
    deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
    registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
    factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
    vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
    singletonHook: '0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc',
    upgradeTimelock: '0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E',
    platformTreasury: '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c',
    recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
    holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
    teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
    traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
    presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
    startBlock: 86973488n,
    swapRatchet: '0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371',
    zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    routers: [{ address: '0x8876789976dEcBfCbBbe364623C63652db8C0904', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x8Dc178eFB8111BB0973Dd9d722ebeFF267c98F94',
    stateView: '0xF3334192D15450CdD385c8B70e03f9A6bD9E673b',
    v3: { factory: '0x1f7d7550B1b028f7571E69A784071F0205FD2EfA', quoter: '0x33e885eD0Ec9bF04EcfB19341582aADCb4c8A9E7', weth: '0x0Bd7D308f8E1639FAb988df18A8011f41EAcAD73' },
    note: 'Robinhood Chain mainnet set. Swaps go through the UniversalRouter (2.1.1); FloorSwapRatchet and FloorZap wrap the same router.',
  },
  5042: {
    chainId: 5042,
    poolManager: '0x8366a39CC670B4001A1121B8F6A443A643e40951',
    deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
    registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
    factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
    vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
    singletonHook: '0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc',
    upgradeTimelock: '0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E',
    platformTreasury: '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c',
    recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
    holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
    teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
    traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
    presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
    startBlock: 25642396n,
    swapRatchet: '0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371',
    zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    routers: [{ address: '0x4fcA4a51Ab4F23A7447b3284fBd7D73289A89Fb1', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x8Dc178eFB8111BB0973Dd9d722ebeFF267c98F94',
    stateView: '0xF3334192D15450CdD385c8B70e03f9A6bD9E673b',
    explorer: 'https://explorer.arc.io',
    note: 'Arc mainnet set. Native gas and quote is USDC. Uniswap v4 is deployed at the Robinhood Chain addresses (CREATE2).',
  },
  8453: {
    chainId: 8453,
    poolManager: '0x498581fF718922c3f8e6A244956aF099B2652b2b',
    deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
    registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
    factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
    vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
    singletonHook: '0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc',
    upgradeTimelock: '0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E',
    platformTreasury: '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c',
    recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
    holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
    teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
    traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
    presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
    startBlock: 52524628n,
    swapRatchet: '0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371',
    zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    zapAerodrome: '0x3b5adDBcEF6785ede8bDf71766Ab1275663129ED',
    zapAerodromeRouter: '0xC5b6786D7B64767D775877b0B6A319AD946B11B5',
    explorer: 'https://basescan.org',
    routers: [{ address: '0xFdf682F51FE81Aa4898F0AE2163d8A55c127fbC7', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x0d5e0F971ED27FBfF6c2837bf31316121532048D',
    stateView: '0xA3c0c9b65baD0b08107Aa264b0f3dB444b867A71',
    v3: { factory: '0x33128a8fC17869897dcE68Ed026d694621f6FDfD', quoter: '0x3d4e44Eb1374240CE5F1B871ab261CD16335B76a', weth: '0x4200000000000000000000000000000000000006' },
    note: 'Base mainnet set. FloorZap and FloorSwapRatchet wrap the UniversalRouter; a second FloorZap (zapAerodrome) wraps Aerodrome\'s UniversalRouter (zapAerodromeRouter).',
  },
  56: {
    chainId: 56,
    poolManager: '0x28e2Ea090877bF75740558f6BFB36A5ffeE9e9dF',
    deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
    registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
    factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
    vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
    singletonHook: '0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc',
    upgradeTimelock: '0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E',
    platformTreasury: '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c',
    recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
    holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
    teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
    traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
    presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
    startBlock: 127278845n,
    swapRatchet: '0xcBEC6E0a19Ef373f3bDA83F125572Aa7A2C0f371',
    zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    explorer: 'https://bscscan.com',
    routers: [{ address: '0x8B844f885672f333Bc0042cB669255f93a4C1E6b', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x9F75dD27D6664c475B90e105573E550ff69437B0',
    stateView: '0xd13Dd3D6E93f276FAfc9Db9E6BB47C1180aeE0c4',
    v3: { factory: '0xdB1d10011AD0Ff90774D0C6Bb92e5C5c8b4461F7', quoter: '0x78D78E420Da98ad378D7799bE8f4AF69033EB077', weth: '0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c' },
    note: 'BSC mainnet set. This 2.1.1 UniversalRouter address is shared with Arbitrum, Optimism and Polygon.',
  },
  1: {
    chainId: 1,
    poolManager: '0x000000000004444c5dc75cB358380D2e3dE08A90',
    deploymentId: '0x000000000000000000000000000000000000000000000000000000000000001a',
    registry: '0x623c349ac8cBBD796f1E9540f902b9BfA9cD5804',
    factory: '0x594A1A3D81b16c52e57e12975E32aF66D641Ed5F',
    vesting: '0xa826B75Ca94206C51De45c8112c5ce8541e357A3',
    singletonHook: '0xe44886ffe0a894c6C5900d77A51c7d49732C60Cc',
    upgradeTimelock: '0xf1dBe7312CF4E0E33f26074f64dEc1844B3D536E',
    platformTreasury: '0x6134E81DB1C853354E5D6B0cd70C5d04a4A6508c',
    recipientClaims: '0x830E0853d08aBD7Ef2181b6e171Fa673E0B51d40',
    holderRewards: '0xc6257aF57c2A92812D87AD43834eb24e1Cd711dD',
    teamTreasury: '0x062B6c421E37579728877337B47Cf528AE4d49bb',
    traderReferrals: '0x905e9965FC52CDE4b133e4711b6EbdD784b0BeFE',
    presetRegistry: '0x2d5b6582C0EA925be1e356F7ECE585238d911505',
    startBlock: 26178890n,
    zap: '0x0117F2686F195eb7b85C27C96EEFDC8040079d9D',
    explorer: 'https://etherscan.io',
    routers: [{ address: '0x4C82D1fBFe28C977cBB58D8C7FF8FCF9F70a2cCA', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x52F0E24D1c21C8A0cB1e5a5dD6198556BD9E1203',
    stateView: '0x7fFE42C4a5DEeA5b0feC41C94C136Cf115597227',
    v3: { factory: '0x1F98431c8aD98523631AE4a59f267346ea31F984', quoter: '0x61fFE014bA17989E743c5F6cB21bF9697530B21e', weth: '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2' },
    note: "Ethereum mainnet set. No FloorSwapRatchet here: a ratchet costs about 1.4M gas on Ethereum, so pool trades go straight through the UniversalRouter and a keeper ratchets. 0x66a9893c…A8Af is the older UniversalRouter; byte-identical copies sit at that address on other chains, where they point at Ethereum's PoolManager and cannot swap. Never use it.",
  },
  42161: {
    chainId: 42161, deploymentId: '',
    poolManager: '0x360E68faCcca8cA495c1B759Fd9EEe466db9FB32',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [{ address: '0x8B844f885672f333Bc0042cB669255f93a4C1E6b', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x3972C00f7ed4885e145823eb7C655375d275A1C5',
    stateView: '0x76Fd297e2D437cd7f76d50F01AfE6160f86e9990',
  },
  10: {
    chainId: 10, deploymentId: '',
    poolManager: '0x9a13F98Cb987694C9F086b1F5eB990EeA8264Ec3',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [{ address: '0x8B844f885672f333Bc0042cB669255f93a4C1E6b', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x1f3131A13296FB91C90870043742C3CDBFF1A8d7',
    stateView: '0xc18a3169788F4F75A170290584ECA6395C75Ecdb',
  },
  130: {
    chainId: 130, deploymentId: '',
    poolManager: '0x1F98400000000000000000000000000000000004',
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [{ address: '0xFdf682F51FE81Aa4898F0AE2163d8A55c127fbC7', kind: 'universal', release: '2.1.1' }],
    permit2: PERMIT2,
    quoter: '0x333E3C607B141b18fF6de9f258db6e77fE7491E0',
    stateView: '0x86e8631A016F9068C3f085fAF484Ee3F5fDee8f2',
  },
  999: {
    chainId: 999, deploymentId: '',
    poolManager: ABSENT,
    registry: ABSENT, factory: ABSENT, vesting: ABSENT, startBlock: 0n,
    routers: [],
    permit2: PERMIT2,
    note: 'HyperEVM. v4-core exists only as third-party deployments with almost no liquidity; left without a PoolManager on purpose.',
  },
};

const ZERO = /^0x0{40}$/i;
const present = (a: string | undefined): a is Address => !!a && isAddress(a, { strict: false }) && !ZERO.test(a);
const who = (chainId: number): string => {
  const n = EVM_NETWORKS[chainId];
  return n ? `${n.label} (chain ${chainId})` : `chain ${chainId}`;
};

/**
 * A row's own consistency: the key and `chainId` agree; every set address is a
 * well-formed, non-zero address; a REQUIRED field is either an address or `ABSENT`; Permit2 is the one Permit2 address.
 * Throws `DeploymentConfigError` naming the field.
 */
export function checkDeployment(key: number, d: Deployment): Deployment {
  const w = who(key);
  if (d.chainId !== key) throw new DeploymentConfigError(`${w}: the row says chainId ${d.chainId}`);
  for (const f of CONTRACT_FIELDS) {
    const v = d[f];
    if (v !== ABSENT && !present(v)) throw new DeploymentConfigError(`${w}: ${f} is neither an address nor ABSENT`);
  }
  const optional = ['recipientClaims', 'holderRewards', 'teamTreasury', 'traderReferrals', 'permit2', 'quoter', 'stateView', 'zap', 'zapAerodrome', 'zapAerodromeRouter', 'presetRegistry', 'swapRatchet'] as const;
  for (const f of optional) {
    const v = d[f];
    if (v !== undefined && !present(v)) throw new DeploymentConfigError(`${w}: ${f} is not a non-zero address`);
  }
  for (const r of d.routers) {
    if (!present(r.address)) throw new DeploymentConfigError(`${w}: a router is not a non-zero address`);
    if (r.kind === 'universal' && !r.release) throw new DeploymentConfigError(`${w}: the UniversalRouter ${r.address} has no recorded release`);
  }
  if (d.permit2 !== undefined && d.permit2.toLowerCase() !== PERMIT2.toLowerCase()) throw new DeploymentConfigError(`${w}: permit2 is not Permit2`);
  const r = d.rules ?? {};
  if (r.saleCost !== undefined && r.saleCost !== 'floor' && r.saleCost !== 'ceil') throw new DeploymentConfigError(`${w}: rules.saleCost is neither 'floor' nor 'ceil'`);
  if (r.sellBack !== undefined && r.sellBack !== 'stateless' && r.sellBack !== 'proRata') throw new DeploymentConfigError(`${w}: rules.sellBack is neither 'stateless' nor 'proRata'`);
  if (r.gapSkim !== undefined && typeof r.gapSkim !== 'boolean') throw new DeploymentConfigError(`${w}: rules.gapSkim is not a boolean`);
  if (d.v3) for (const [f, v] of Object.entries(d.v3)) if (!present(v)) throw new DeploymentConfigError(`${w}: v3.${f} is not a non-zero address`);
  return d;
}

/** True when a row exists for the chain — of either kind. */
export function hasDeployment(chainId: number, table: Readonly<Record<number, Deployment>> = EVM_DEPLOYMENTS): boolean {
  return Object.prototype.hasOwnProperty.call(table, chainId);
}

/**
 * The chain's row, ONLY if it carries a FLOOR set: every `CONTRACT_FIELDS` address real. Otherwise a `DeploymentConfigError`
 * naming the chain and the first missing contract — never a row with an `ABSENT` field handed on.
 */
export function floorDeploymentFor(chainId: number, table: Readonly<Record<number, Deployment>> = EVM_DEPLOYMENTS): Deployment {
  const d = hasDeployment(chainId, table) ? table[chainId] : undefined;
  if (!d) throw new DeploymentConfigError(`No deployment for ${who(chainId)}. Configured chains: ${Object.keys(table).join(', ')}.`);
  for (const f of CONTRACT_FIELDS) {
    if (!present(d[f])) throw new DeploymentConfigError(`${who(chainId)}: ${f} is not deployed yet.`);
  }
  return d;
}

/** Whether a trade can settle on the chain at all: a PoolManager and at least one router. Orthogonal to having a FLOOR set. */
export function isTradable(chainId: number, table: Readonly<Record<number, Deployment>> = EVM_DEPLOYMENTS): boolean {
  const d = hasDeployment(chainId, table) ? table[chainId] : undefined;
  return !!d && d.poolManager !== ABSENT && d.routers.length > 0;
}

/** The router a single-hop pool swap goes through: the first non-`v4router` entry. A `universal` one carries its release. */
export function swapRouterOf(d: Pick<Deployment, 'routers'>): SwapRouter | null {
  const r = d.routers.find((x) => x.kind !== 'v4router');
  if (!r) return null;
  if (r.kind === 'universal') {
    if (!r.release) throw new DeploymentConfigError(`the UniversalRouter ${r.address} has no recorded release`);
    return { address: r.address, kind: 'universal', release: r.release };
  }
  return { address: r.address, kind: 'poolswaptest' };
}

/** Permit2, only for a router that pulls through it (`universal`). */
export function permit2For(d: Pick<Deployment, 'permit2'>, router: { kind: RouterKind } | null): Address | null {
  return router?.kind === 'universal' ? d.permit2 ?? null : null;
}

/** `FloorSwapRatchet`, only beside a `universal` swap router (it wraps one UniversalRouter). */
export function swapRatchetOf(d: Pick<Deployment, 'routers' | 'swapRatchet'>): Address | null {
  const a = d.swapRatchet;
  if (!present(a)) return null;
  return swapRouterOf(d)?.kind === 'universal' ? a : null;
}

/** The FLOOR Platform fee ABI is on this set exactly when its row names a team vault. */
export function setHasTeamFee(d: Pick<Deployment, 'teamTreasury'>): boolean {
  return present(d.teamTreasury);
}

/** A row's optional address, or `null` where it is absent or zero. */
export function optionalAddress(a: Address | undefined): Address | null {
  return present(a) ? a : null;
}
