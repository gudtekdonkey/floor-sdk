

import { R, RS, W, WS, instructionDiscriminator } from "../instruction.js";

import { SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, WSOL_MINT, associatedTokenAddress } from "../pda.js";
import {
  FLOOR_AMM_PROGRAM_ID, GENESIS_USDC_MINT, INSTRUCTIONS_SYSVAR_ID, METADATA_PROGRAM_ID, configPda, floorEventAuthorityPda, genesisAllocPda, genesisConvertBandPda, genesisMigrationVaultPda, genesisRaisePda, genesisRaiseTokenVault,
  genesisVaultFloor, genesisVaultUsdc, launchPda, metadataPda, presetPda, quoteEntryPda, teamSlotPda, traderLinkPda,
} from "./program.js";
import { Writer } from "./borsh.js";
import { CLAIM_BITS, CLAIM_KNOWN_MASKS, CLAIM_RESERVED, CLAIM_CREATOR, CLAIM_CREATOR_REF, CLAIM_TEAM_SLOT, CLAIM_TRADER_REF, CLAIM_RESERVED_HOLDER, CLAIM_EXTRA, CLAIM_TEAM_BUY, CLAIM_SHARE_BOUGHT } from "../../generated/index.js";
import { launchMintPda } from "./mint.js";

const pid = (c      )         => c?.programId ?? FLOOR_AMM_PROGRAM_ID;
const evPair = (c      )                => [R(floorEventAuthorityPda(pid(c))[0]), R(pid(c))];
function ix(name        , accounts               , args                       , c      )              {
  const w = new Writer().bytes(instructionDiscriminator(name));
  return { programAddress: pid(c), accounts: [...accounts, ...evPair(c)], data: args(w).done() };
}
const none = (w        )         => w;

const withRest = (i             , rest               )              => (rest.length ? { ...i, accounts: [...i.accounts, ...rest] } : i);

export function initializeConfigInstruction(p                                                                                                                                         )              {
  return ix("initialize_config", [WS(p.upgradeAuthority), W(p.config), R(p.programData), R(SYSTEM_PROGRAM_ID)], (w) => w.pubkey(p.authority).pubkey(p.timelock).pubkey(p.referralAuthority), p);
}
export function transferAuthorityInstruction(p                                                                   )              {
  return ix("transfer_authority", [RS(p.authority), W(p.config)], (w) => w.pubkey(p.newAuthority), p);
}
export function acceptAuthorityInstruction(p                                          )              {
  return ix("accept_authority", [RS(p.signer), W(p.config)], none, p);
}

export function setTreasuryInstruction(p                                                            )              {
  return ix("set_treasury", [RS(p.signer), W(p.config)], (w) => w.pubkey(p.treasury), p);
}

export function setPriceUpdaterInstruction(p                                                                   )              {
  return ix("set_price_updater", [RS(p.authority), W(p.config)], (w) => w.pubkey(p.priceUpdater), p);
}

export function setQuotePriceInstruction(p                                                                                                        )              {
  if (p.usdCentsX64 <= 0n || p.usdCentsX64 >= 1n << 128n) throw new RangeError("usdCentsX64 is in (0, 2^128 - 1] (BadArgument)");
  const quote = p.quote ?? quoteEntryPda(p.quoteMint, pid(p))[0];
  return ix("set_quote_price", [RS(p.priceUpdater), R(p.config), R(p.quoteMint), W(quote)], (w) => w.u128(p.usdCentsX64), p);
}
export function setTimelockInstruction(p                                                            )              {
  return ix("set_timelock", [RS(p.signer), W(p.config)], (w) => w.pubkey(p.timelock), p);
}

export function setCreatePausedInstruction(p                                                              )              {
  return ix("set_create_paused", [RS(p.authority), W(p.config)], (w) => w.bool(p.paused), p);
}
export function setReferralAuthorityInstruction(p                                                                        )              {
  return ix("set_referral_authority", [RS(p.authority), W(p.config)], (w) => w.pubkey(p.referralAuthority), p);
}

export function setHolderDestInstruction(p                                                                 )              {
  return ix("set_holder_dest", [RS(p.authority), W(p.config)], (w) => w.pubkey(p.holderDest), p);
}

export function withdrawHolderInstruction(p                                                                                                                                                                    )              {
  return ix("withdraw_holder", [RS(p.authority), R(p.config), W(p.launch), R(p.quoteMint), W(p.quoteVault), W(p.destination), R(p.quoteTokenProgram)], (w) => w.u64(p.amount), p);
}

export function withdrawHolderManyInstruction(p                                                                                                                                                                             )              {
  if (p.launches.length === 0) throw new RangeError("withdraw_holder_many: no launch");
  const base = ix("withdraw_holder_many", [RS(p.authority), R(p.config), R(p.quoteMint), W(p.destination), R(p.quoteTokenProgram)], (w) => w.u8(p.launches.length), p);
  return { ...base, accounts: [...base.accounts, ...p.launches.flatMap((l) => [W(l.launch), W(l.quoteVault)])] };
}

export function clearTraderRefInstruction(p                                                                                                         )              {
  const i = ix("clear_trader_ref", [RS(p.authority), R(p.config), W(p.traderLink ?? traderLinkPda(p.trader, p.programId)[0])], none, p);
  return withRest(i, (p.traderRefs ?? []).map((r) => W(r)));
}

export function withdrawPlatformInstruction(p                                                                                                                                                                    )              {
  return ix("withdraw_platform", [RS(p.authority), R(p.config), W(p.launch), R(p.quoteMint), W(p.quoteVault), W(p.destination), R(p.quoteTokenProgram)], (w) => w.u64(p.amount), p);
}

export function checkPresetInstruction(p                                                                         )              {
  return ix("check_preset", [RS(p.authority), R(p.config), W(p.preset)], (w) => w.u8(p.id), p);
}

export function setPresetEnabledInstruction(p                                                                                           )              {
  return ix("set_preset_enabled", [RS(p.authority), R(p.config), W(p.preset)], (w) => w.u8(p.id).bool(p.enabled), p);
}
export function deletePresetInstruction(p                                                                         )              {
  return ix("delete_preset", [RS(p.authority), R(p.config), W(p.preset)], (w) => w.u8(p.id), p);
}

export function addQuoteInstruction(p                                                                                                                                         )              {
  return ix("add_quote", [WS(p.authority), R(p.config), R(p.mint), W(p.quote), R(SYSTEM_PROGRAM_ID)], (w) => w.u64(p.createFee).u128(p.usdCentsX64).u32(p.admittedCaps ?? 0), p);
}

export function setQuoteCapsInstruction(p                                                                                  )              {
  return ix("set_quote_caps", [RS(p.authority), R(p.config), W(p.quote)], (w) => w.u32(p.admittedCaps), p);
}
export function updateQuoteInstruction(p                                                                                                                      )              {
  return ix("update_quote", [RS(p.authority), R(p.config), W(p.quote)], (w) => w.bool(p.enabled).u64(p.createFee).u128(p.usdCentsX64), p);
}

export const MAX_NAME_LEN = 32;
export const MAX_SYMBOL_LEN = 10;
export const MAX_URI_LEN = 200;

export const MAX_METADATA_BASE_LEN = 72;
export const METADATA_BASE_SCHEMES = ["https://", "ipfs://", "ar://"]         ;

export function metadataBaseOk(base        )          {
  if (base === "") return true;
  return (
    base.length <= MAX_METADATA_BASE_LEN &&
    METADATA_BASE_SCHEMES.some((s) => base.startsWith(s) && base.length > s.length) &&
    /^[\x21-\x7e]+$/.test(base) &&
    !base.endsWith("/")
  );
}

export const metadataUri = (base        , baseMint        )         => `${base}/${baseMint}.json`;

export function setMetadataBaseInstruction(p                                                           )              {
  if (!metadataBaseOk(p.base)) throw new RangeError("the metadata base is \"\" or https:// · ipfs:// · ar://, printable ASCII, no space, no trailing slash, ≤ 72 bytes");
  return ix("set_metadata_base", [RS(p.authority), W(p.config)], (w) => w.string(p.base), p);
}

export function createMintAccounts(p                                                                                                                                                                                                       )                     {
  const nonce = p.args.mintNonce;
  if (typeof nonce !== "bigint") throw new RangeError("args.mintNonce (a u64 bigint) is required: the mint is the PDA [\"mint\", creator, mintNonce LE]");
  const baseMint = launchMintPda(p.creator, nonce, pid(p))[0];
  if (p.baseMint !== undefined && p.baseMint !== baseMint) throw new RangeError(`baseMint ${p.baseMint} is not the creator's mint PDA for nonce ${nonce} (${baseMint})`);
  const launch = p.launch ?? launchPda(baseMint, pid(p))[0];
  return {
    mintNonce: nonce,
    baseMint,
    launch,
    tokenVault: p.tokenVault ?? associatedTokenAddress(launch, baseMint, TOKEN_2022_PROGRAM_ID),
    quoteVault: p.quoteVault ?? associatedTokenAddress(launch, p.quoteMint, p.quoteTokenProgram),
    metadata: p.metadata ?? metadataPda(baseMint)[0],
  };
}

      

      

export function initTeamSlotInstruction(p                                                                                             )              {
  return ix("init_team_slot", [WS(p.creator), W(p.launch), R(p.quote), W(p.teamSlot), R(SYSTEM_PROGRAM_ID)], (w) => w.u16(p.rateBps), p);
}

                                                                         

const slotPay = (a                 )                => [WS(a.payer), W(a.launch), W(a.teamSlot), R(a.quoteMint), W(a.payerQuote), W(a.quoteVault), R(a.quoteTokenProgram), R(a.baseMint)];
export const TEAM_SLOT_WALLET = 0;
export const TEAM_SLOT_PENDING_HANDLE = 1;
export function teamSlotAddInstruction(p                                                                                        )              {
  return ix("team_slot_add", slotPay(p), (w) => w.pubkey(p.key).u8(p.kind).u64(p.amount).bool(p.payNow), p);
}

export function bindHandleInstruction(p                                                                                                                                          )              {
  if (p.handle.length !== 32) throw new RangeError("a handle is 32 bytes");

  return ix("bind_handle", [RS(p.referralAuthority), R(p.config), R(p.launch), W(p.teamSlot), RS(p.wallet)], (w) => w.u8(p.index).bytes(p.handle).pubkey(p.wallet), p);
}

export function openTraderRefInstruction(p                                                                                                                                                                                                                          )              {
  return withRest(
    ix(
      "open_trader_ref",
      [WS(p.payer), RS(p.referralAuthority), R(p.config), R(p.quoteMint), W(p.traderRef), W(p.escrow), R(p.quoteTokenProgram), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID), RS(p.trader)],
      (w) => w.pubkey(p.trader).pubkey(p.tiers[0]).pubkey(p.tiers[1]).pubkey(p.tiers[2]),
      p,
    ),
    [W(traderLinkPda(p.trader, p.programId)[0])],
  );
}

export function openTraderLinkInstruction(p                                                                                                                                          )              {
  return ix(
    "open_trader_link",
    [WS(p.payer), RS(p.referralAuthority), R(p.config), W(p.traderLink ?? traderLinkPda(p.trader, p.programId)[0]), R(SYSTEM_PROGRAM_ID), RS(p.trader)],
    (w) => w.pubkey(p.trader).pubkey(p.tiers[0]).pubkey(p.tiers[1]).pubkey(p.tiers[2]),
    p,
  );
}

export function openRefOwedInstruction(p                                                                                                                         )              {
  return ix("open_ref_owed", [WS(p.payer), R(p.quoteMint), W(p.refOwed), W(p.vault), R(p.quoteTokenProgram), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID)], (w) => w.pubkey(p.referrer), p);
}

export function rollUpRefsInstruction(p                                                                                                                                                                              )              {
  if (p.traderRefs.length > 255) throw new RangeError("at most 255 links a roll-up");
  const base = ix("roll_up_refs", [R(p.quoteMint), R(p.quoteTokenProgram)], (w) => w.u8(p.traderRefs.length), p);
  const rest                = [];
  for (const t of p.traderRefs) rest.push(W(t.traderRef), W(t.escrow));
  for (const r of p.refOwed) rest.push(W(r.refOwed), W(r.vault));
  return { ...base, accounts: [...base.accounts, ...rest] };
}

export { CLAIM_CREATOR, CLAIM_CREATOR_REF, CLAIM_TEAM_SLOT, CLAIM_TRADER_REF, CLAIM_RESERVED_HOLDER, CLAIM_EXTRA, CLAIM_TEAM_BUY, CLAIM_SHARE_BOUGHT };

export const CLAIM_RESERVED_PAYOUT = CLAIM_RESERVED.solana.payout;

export const CLAIM_KNOWN = CLAIM_KNOWN_MASKS.solana.claim;

export const CLAIM_KNOWN_WIDE = CLAIM_KNOWN_MASKS.solana.claimWideV1;

                                         

export function setNativeCreateFeeInstruction(p                                                               )              {
  return ix("set_native_create_fee", [RS(p.authority), W(p.config)], (w) => w.u64(p.lamports), p);
}

export function withdrawPlatformManyInstruction(p                                                                                                                                                                                                                                    )              {
  if (p.launches.length === 0 && !p.nativeTo) throw new RangeError("withdraw_platform_many: no launch and no native fees");

  if (p.quoteHook?.length && p.launches.length === 0) throw new RangeError("withdraw_platform_many: a quote hook block needs a launch");
  const base = ix("withdraw_platform_many", [RS(p.authority), p.nativeTo ? W(p.config) : R(p.config), R(p.quoteMint), W(p.destination), R(p.quoteTokenProgram)], (w) => w.u8(p.launches.length), p);
  return { ...base, accounts: [...base.accounts, ...p.launches.flatMap((l) => [W(l.launch), W(l.quoteVault)]), ...(p.quoteHook ?? []), ...(p.nativeTo ? [W(p.nativeTo)] : [])] };
}

export const INCINERATOR = "1nc1nerator11111111111111111111111111111111";

export function syncBurnsInstruction(p                                                                                                )              {
  const base = ix("sync_burns", [W(p.launch), R(p.baseMint), R(p.incineratorAta ?? associatedTokenAddress(INCINERATOR, p.baseMint, TOKEN_2022_PROGRAM_ID))], none, p);
  return { ...base, accounts: [...base.accounts, ...(p.extra ?? []).map((a) => R(a))] };
}

export function contributeGenesisInstruction(p                                                                                                                 )              {
  return ix("contribute_genesis", [WS(p.contributor), W(p.genesisRaise), W(p.migrationVault), W(p.allocation), R(SYSTEM_PROGRAM_ID)], (w) => w.u64(p.amount), p);
}

export function contributeGenesisListedInstruction(p                                                                                                                                               )              {
  for (const s of p.proof) if (s.length !== 32) throw new RangeError("contributeGenesisListedInstruction: a proof sibling is 32 bytes");
  return ix(
    "contribute_genesis_listed",
    [WS(p.contributor), W(p.genesisRaise), W(p.migrationVault), W(p.allocation), R(SYSTEM_PROGRAM_ID)],
    (w) => p.proof.reduce((x, s) => x.bytes(s), w.u64(p.amount).u32(p.proof.length)),
    p,
  );
}

export function refundGenesisMigratedInstruction(p                                                                                            )              {
  return ix("refund_genesis_migrated", [WS(p.wallet), W(p.genesisRaise), W(p.migrationVault), W(p.allocation), R(SYSTEM_PROGRAM_ID)], none, p);
}

export function setGenesisCreatorInstruction(p                                                                                    )              {
  return ix("set_genesis_creator", [RS(p.authority), R(p.config), W(p.genesisRaise)], (w) => w.pubkey(p.creator), p);
}

export function setGenesisListRootInstruction(p                                                                                         )              {
  if (p.listRoot.length !== 32) throw new RangeError("setGenesisListRootInstruction: the root is 32 bytes");
  return ix("set_genesis_list_root", [RS(p.authority), R(p.config), W(p.genesisRaise)], (w) => w.bytes(p.listRoot), p);
}

export function setGenesisPresetInstruction(p                                                                                                     )              {
  return ix("set_genesis_preset", [RS(p.authority), R(p.config), W(p.genesisRaise), R(p.preset)], (w) => w.u8(p.presetId), p);
}

export function setGenesisWagmiPresetInstruction(p                                                                                                        )              {
  const g = pid(p);
  const preset = p.preset ?? (p.presetId === 0 ? g : presetPda(p.presetId, g)[0]);
  return ix("set_genesis_wagmi_preset", [RS(p.authority), R(p.config ?? configPda(g)[0]), W(p.genesisRaise ?? genesisRaisePda(g)[0]), R(preset)], (w) => w.u8(p.presetId), p);
}

export function setGenesisWagmiCapInstruction(p                                                                                        )              {
  if (p.capTokens < 0n || p.capTokens >> 64n !== 0n) throw new RangeError("set_genesis_wagmi_cap: cap_tokens is a u64");
  const g = pid(p);
  return ix("set_genesis_wagmi_cap", [RS(p.authority), R(p.config ?? configPda(g)[0]), W(p.genesisRaise ?? genesisRaisePda(g)[0])], (w) => w.u64(p.capTokens), p);
}

export function convertGenesisSolInstruction(p

       

       )              {
  if (p.lamports <= 0n) throw new RangeError("convert_genesis_sol: the lamports must be > 0");
  const g = pid(p);
  const usdc = p.usdcMint ?? GENESIS_USDC_MINT;
  return ix(
    "convert_genesis_sol",
    [
      WS(p.caller), W(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.migrationVault ?? genesisMigrationVaultPda(g)[0]), R(p.solQuote ?? quoteEntryPda(WSOL_MINT, g)[0]), R(p.usdcQuote ?? quoteEntryPda(usdc, g)[0]), R(usdc),
      W(p.callerUsdc ?? associatedTokenAddress(p.caller, usdc, TOKEN_PROGRAM_ID)), W(p.vaultUsdc ?? genesisVaultUsdc(g, usdc)), R(p.preset), R(p.wagmiPreset ?? g),
      R(TOKEN_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID),
      R(typeof p.convertBand === "string" ? p.convertBand : genesisConvertBandPda(g)[0]),
    ],
    (w) => w.u64(p.lamports).u64(p.maxUsdc),
    p,
  );
}

export function openGenesisLaunchInstruction(p

       )              {
  const g = pid(p);
  const wagmiPreset = p.wagmiPreset ?? (p.wagmiPresetId ? presetPda(p.wagmiPresetId, g)[0] : g);
  return ix(
    "open_genesis_launch",
    [
      WS(p.authority), W(p.config), W(p.genesisRaise), W(p.migrationVault), W(p.deployer), R(p.preset), W(p.quote), R(p.quoteMint), WS(p.baseMint), W(p.launch), W(p.tokenVault), W(p.quoteVault),
      W(p.raiseTokenVault), W(p.teamSlot), W(p.metadata), R(p.metadataProgram ?? METADATA_PROGRAM_ID), R(TOKEN_2022_PROGRAM_ID), R(TOKEN_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID), R(INSTRUCTIONS_SYSVAR_ID),
      W(p.vaultUsdc ?? genesisVaultUsdc(g, p.quoteMint)), W(p.vaultFloor ?? genesisVaultFloor(p.baseMint, g)), R(wagmiPreset), R(p.solQuote ?? quoteEntryPda(WSOL_MINT, g)[0]),
    ],
    (w) => w.string(p.args.name).string(p.args.symbol).string(p.args.uri ?? ""),
    p,
  );
}

export function openGenesisLaunchDefaults(p                                                                                                                              )                                                                   {
  const g = pid(p);
  const usdc = p.usdcMint ?? GENESIS_USDC_MINT;
  const launch = launchPda(p.baseMint, g)[0];
  return {
    programId: g, authority: p.authority, config: configPda(g)[0], genesisRaise: genesisRaisePda(g)[0], migrationVault: genesisMigrationVaultPda(g)[0], deployer: p.deployer,
    preset: presetPda(p.presetId, g)[0], quote: quoteEntryPda(usdc, g)[0], quoteMint: usdc, baseMint: p.baseMint, launch,
    tokenVault: associatedTokenAddress(launch, p.baseMint, TOKEN_2022_PROGRAM_ID), quoteVault: associatedTokenAddress(launch, usdc, TOKEN_PROGRAM_ID),
    raiseTokenVault: genesisRaiseTokenVault(p.baseMint, g), teamSlot: teamSlotPda(launch, g)[0], metadata: metadataPda(p.baseMint)[0],
    vaultUsdc: genesisVaultUsdc(g, usdc), vaultFloor: genesisVaultFloor(p.baseMint, g), wagmiPresetId: p.wagmiPresetId ?? 0,
  };
}

export function openGenesisWagmiInstruction(p

       )              {
  const g = pid(p);
  return ix(
    "open_genesis_wagmi",
    [
      WS(p.authority), W(p.config), W(p.genesisRaise), R(p.preset), W(p.quote), R(p.quoteMint), WS(p.baseMint), W(p.launch), W(p.tokenVault), W(p.quoteVault), W(p.raiseTokenVault),
      W(p.teamSlot), R(p.vault), W(p.vaultFloor), W(p.metadata), R(p.metadataProgram ?? METADATA_PROGRAM_ID), R(TOKEN_2022_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID), R(INSTRUCTIONS_SYSVAR_ID),
      R(p.floorLaunch ?? launchPda(p.quoteMint, g)[0]), R(p.usdcQuote ?? quoteEntryPda(GENESIS_USDC_MINT, g)[0]),
    ],
    (w) => w.string(p.args.name).string(p.args.symbol).string(p.args.uri ?? ""),
    p,
  );
}

export function openGenesisWagmiDefaults(p                                                                                                            )                                                                  {
  const g = pid(p);
  const launch = launchPda(p.baseMint, g)[0];
  return {
    programId: g, authority: p.authority, config: configPda(g)[0], genesisRaise: genesisRaisePda(g)[0], preset: presetPda(p.wagmiPresetId, g)[0],
    quote: quoteEntryPda(p.floorMint, g)[0], quoteMint: p.floorMint, baseMint: p.baseMint, launch,
    tokenVault: associatedTokenAddress(launch, p.baseMint, TOKEN_2022_PROGRAM_ID), quoteVault: associatedTokenAddress(launch, p.floorMint, TOKEN_2022_PROGRAM_ID),
    raiseTokenVault: genesisRaiseTokenVault(p.baseMint, g), teamSlot: teamSlotPda(launch, g)[0], vault: genesisMigrationVaultPda(g)[0], vaultFloor: genesisVaultFloor(p.floorMint, g),
    metadata: metadataPda(p.baseMint)[0], floorLaunch: launchPda(p.floorMint, g)[0], usdcQuote: quoteEntryPda(p.usdcMint ?? GENESIS_USDC_MINT, g)[0],
  };
}

export function claimGenesisInstruction(p                                                                                                                                                                                                                                              )              {
  const usdc = p.usdcMint ?? GENESIS_USDC_MINT;
  return ix(
    "claim_genesis",
    [
      WS(p.sender), W(p.wallet), W(p.genesisRaise), W(p.allocation), R(p.baseMint), W(p.raiseTokenVault), W(p.destination), R(TOKEN_2022_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID), W(p.migrationVault ?? genesisMigrationVaultPda(pid(p))[0]),
      R(usdc), W(p.vaultUsdc ?? genesisVaultUsdc(pid(p), usdc)), W(p.usdcDestination ?? associatedTokenAddress(p.wallet, usdc, TOKEN_PROGRAM_ID)), R(TOKEN_PROGRAM_ID),
    ],
    (w) => w.pubkey(p.wallet),
    p,
  );
}

export function claimGenesisRefundInstruction(p                                                                                                                                                                                     )              {
  const g = pid(p);
  const usdc = p.usdcMint ?? GENESIS_USDC_MINT;
  return ix(
    "claim_genesis_refund",
    [
      RS(p.claimer), W(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.allocation ?? genesisAllocPda(p.wallet, g)[0]), W(p.to ?? p.wallet), W(p.migrationVault ?? genesisMigrationVaultPda(g)[0]), R(SYSTEM_PROGRAM_ID),
      W(p.vaultUsdc ?? genesisVaultUsdc(g, usdc)), R(usdc), W(p.usdcTo ?? associatedTokenAddress(p.wallet, usdc, TOKEN_PROGRAM_ID)), R(TOKEN_PROGRAM_ID),
    ],
    (w) => w.pubkey(p.wallet),
    p,
  );
}

export function sweepGenesisDustInstruction(p                                                                                                                                                      )              {
  const g = pid(p);
  const usdc = p.usdcMint ?? GENESIS_USDC_MINT;
  return ix(
    "sweep_genesis_dust",
    [
      W(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.launch), W(p.quoteVault), W(p.migrationVault ?? genesisMigrationVaultPda(g)[0]), R(TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID),
      W(p.config ?? configPda(g)[0]), W(p.vaultUsdc ?? genesisVaultUsdc(g, usdc)), R(usdc),
    ],
    none,
    p,
  );
}

export function claimGenesisWagmiInstruction(p

       )              {
  const g = pid(p);
  return ix(
    "claim_genesis_wagmi",
    [
      WS(p.sender), R(p.wallet), W(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.allocation ?? genesisAllocPda(p.wallet, g)[0]),
      R(p.wagmiMint), W(p.raiseWagmiVault ?? genesisRaiseTokenVault(p.wagmiMint, g)), W(p.wagmiDestination ?? associatedTokenAddress(p.wallet, p.wagmiMint, TOKEN_2022_PROGRAM_ID)),
      R(p.floorMint), R(p.migrationVault ?? genesisMigrationVaultPda(g)[0]), W(p.vaultFloor ?? genesisVaultFloor(p.floorMint, g)), W(p.floorDestination ?? associatedTokenAddress(p.wallet, p.floorMint, TOKEN_2022_PROGRAM_ID)),
      R(TOKEN_2022_PROGRAM_ID), R(ASSOCIATED_TOKEN_PROGRAM_ID), R(SYSTEM_PROGRAM_ID),
    ],
    (w) => w.pubkey(p.wallet),
    p,
  );
}

export function sweepGenesisWagmiDustInstruction(p                                                                                                                              )              {
  const g = pid(p);
  return ix(
    "sweep_genesis_wagmi_dust",
    [
      W(p.genesisRaise ?? genesisRaisePda(g)[0]), W(p.launch), W(p.quoteVault ?? associatedTokenAddress(p.launch, p.floorMint, TOKEN_2022_PROGRAM_ID)), R(p.floorMint),
      R(p.vault ?? genesisMigrationVaultPda(g)[0]), W(p.vaultFloor ?? genesisVaultFloor(p.floorMint, g)), R(TOKEN_2022_PROGRAM_ID),
    ],
    none,
    p,
  );
}

export function acceptGenesisMigrationInstruction(p

       )              {
  const base = ix("accept_genesis_migration", [WS(p.authority), R(p.config), W(p.genesisRaise), W(p.migrationVault), R(p.oldSale), R(SYSTEM_PROGRAM_ID)], (w) => w.u16(p.contributors.length).u64(p.expectedTotal), p);
  return { ...base, accounts: [...base.accounts, ...p.contributors.flatMap((c) => [R(c.contribution), W(c.allocation)])] };
}

export function tokenBurnCheckedInstruction(p                                                                                    )              {
  const data = new Uint8Array(10);
  data[0] = 15;
  new DataView(data.buffer).setBigUint64(1, p.amount, true);
  data[9] = p.decimals;
  return { programAddress: TOKEN_2022_PROGRAM_ID, accounts: [W(p.account), W(p.mint), RS(p.owner)], data };
}
