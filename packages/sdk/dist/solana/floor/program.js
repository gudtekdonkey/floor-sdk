

import { addressBytes } from "../../codec.js";
import { ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, associatedTokenAddress, eventAuthorityPda, findProgramAddress } from "../pda.js";

import { PROGRAM_IDS } from "../../generated/index.js";

export const FLOOR_AMM_PROGRAM_ID         = PROGRAM_IDS.floorAmm;

export const FLOOR_AMM_TIMELOCK         = PROGRAM_IDS.upgradeTimelock;

export const OLD_FLOOR_LAUNCH_PROGRAM_ID = "8vfdPWfaA3b9GtJoFUwc36JKDaMfVCJ4oT9HrCrQ6Tdi";
export const METADATA_PROGRAM_ID = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
export const INSTRUCTIONS_SYSVAR_ID = "Sysvar1nstructions1111111111111111111111111";
export { ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID };

export const FLOOR_AMM_ERRORS = { NotAuthority: 6000, QuoteNotListed: 6006, BadArgument: 6025, TreasuryNotSet: 6043, NotPriceUpdater: 6044, BadMember: 6045, PresetDeleted: 6004, UnknownClaimType: 6017, BadDecimals: 6026, GenesisNotListed: 6046, GenesisListedAllowanceUsed: 6047, QuoteRowCannotOpen: 6048, QuoteTransferShort: 6049, PresetUnchecked: 6050, GenesisRefundUnderRent: 6051, GenesisDustNotYet: 6052, GenesisConvertCapped: 6053, GenesisPriceStale: 6054, GenesisConvertSlippage: 6055, GenesisSolNotConverted: 6056, GenesisWagmiWrongState: 6057, GenesisWagmiNoPreset: 6058, GenesisWagmiQuoteNotListed: 6059, HolderDestNotSet: 6060, NotTeamMember: 6061, BurnTooLarge: 6062 }         ;

const k = addressBytes;

export const configPda = (p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["config"], p);
export const presetPda = (id        , p = FLOOR_AMM_PROGRAM_ID)      => {

  if (!Number.isInteger(id) || id < 0 || id > 255) throw new RangeError(`preset id ${id} is not a u8`);
  return findProgramAddress(["preset", Uint8Array.of(id)], p);
};
export const quoteEntryPda = (quoteMint        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["quote", k(quoteMint)], p);
export const launchPda = (baseMint        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["launch", k(baseMint)], p);
export const teamSlotPda = (launch        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["team_slot", k(launch)], p);
export const traderRefPda = (trader        , quoteMint        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["trader_ref", k(trader), k(quoteMint)], p);

export const traderLinkPda = (wallet        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["trader_link", k(wallet)], p);

export const unwrapPda = (wallet        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["unwrap", k(wallet)], p);
export const refOwedPda = (referrer        , quoteMint        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["ref_owed", k(referrer), k(quoteMint)], p);

export const genesisRaisePda = (p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["genesis_raise"], p);
export const genesisAllocPda = (wallet        , p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["genesis_alloc", k(wallet)], p);

export const genesisConvertBandPda = (p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["genesis_convert_band"], p);
export const genesisMigrationVaultPda = (p = FLOOR_AMM_PROGRAM_ID)      => findProgramAddress(["genesis_migration_vault"], p);

export const GENESIS_USDC_MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

export const GENESIS_CONVERT_DISCOUNT_BPS = 100;

export const GENESIS_PRICE_MAX_AGE_SECS = 900;

export const genesisVaultUsdc = (p = FLOOR_AMM_PROGRAM_ID, usdcMint = GENESIS_USDC_MINT)         => associatedTokenAddress(genesisMigrationVaultPda(p)[0], usdcMint, TOKEN_PROGRAM_ID);

export const genesisVaultFloor = (floorMint        , p = FLOOR_AMM_PROGRAM_ID)         => associatedTokenAddress(genesisMigrationVaultPda(p)[0], floorMint, TOKEN_2022_PROGRAM_ID);

export const genesisRaiseTokenVault = (mint        , p = FLOOR_AMM_PROGRAM_ID)         => associatedTokenAddress(genesisRaisePda(p)[0], mint, TOKEN_2022_PROGRAM_ID);
export const floorEventAuthorityPda = (p = FLOOR_AMM_PROGRAM_ID)      => eventAuthorityPda(p);

export const metadataPda = (mint        )      => findProgramAddress(["metadata", k(METADATA_PROGRAM_ID), k(mint)], METADATA_PROGRAM_ID);

export const tokenVault = (launch        , baseMint        )         => associatedTokenAddress(launch, baseMint, TOKEN_2022_PROGRAM_ID);

export const quoteVault = (launch        , quoteMint        , quoteTokenProgram        )         => associatedTokenAddress(launch, quoteMint, quoteTokenProgram);

export const traderRefEscrow = (traderRef        , quoteMint        , quoteTokenProgram        )         => associatedTokenAddress(traderRef, quoteMint, quoteTokenProgram);

export const refOwedVault = (refOwed        , quoteMint        , quoteTokenProgram        )         => associatedTokenAddress(refOwed, quoteMint, quoteTokenProgram);
