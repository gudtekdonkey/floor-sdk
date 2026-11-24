

import { accountDiscriminator } from "../../solana/instruction.js";
import { Reader } from "../../solana/floor/borsh.js";
import { decodeGenesisAllocation, decodeGenesisRaise } from "../../solana/floor/accounts.js";
import { FLOOR_AMM_PROGRAM_ID, configPda, genesisAllocPda, genesisConvertBandPda, genesisRaisePda } from "../../solana/floor/program.js";
import { genesisWalletClaim } from "../../solana/floor/genesis.js";

export { openGenesisLaunchInstructionV2, openGenesisWagmiInstructionV2, claimGenesisInstructionV2, claimGenesisWagmiInstructionV2, sweepGenesisWagmiDustInstructionV2 } from "../genesis.js";
export { setGenesisConvertBandInstruction } from "../solana.js";
export { contributeGenesisInstruction, contributeGenesisListedInstruction, refundGenesisMigratedInstruction, claimGenesisRefundInstruction, convertGenesisSolInstruction } from "../../solana/floor/instructions.js";
export { decodeGenesisRaise, decodeGenesisAllocation } from "../../solana/floor/accounts.js";
export { genesisWalletClaim } from "../../solana/floor/genesis.js";

export const GENESIS_CONVERT_BAND_SIZE = 8 + 1 + 16 * 4 + 8;

export function decodeGenesisConvertBand(data            )                     {
  if (data.length !== GENESIS_CONVERT_BAND_SIZE) throw new RangeError(`GenesisConvertBand is ${GENESIS_CONVERT_BAND_SIZE} bytes, got ${data.length}`);
  const d = accountDiscriminator("GenesisConvertBand");
  for (let i = 0; i < 8; i++) if (data[i] !== d[i]) throw new RangeError("not a GenesisConvertBand account");
  const r = new Reader(data.subarray(8));
  return { bump: r.u8(), solPxMin: r.u128(), solPxMax: r.u128(), usdcPxMin: r.u128(), usdcPxMax: r.u128(), updatedAt: r.i64() };
}

export const genesisBandHolds = (b                    , solPx        , usdcPx        )          =>
  b.solPxMin > 0n && b.usdcPxMin > 0n && solPx >= b.solPxMin && solPx <= b.solPxMax && usdcPx >= b.usdcPxMin && usdcPx <= b.usdcPxMax;

export function genesisAccountsV2(p                                         )

  {
  const g = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  return {
    config: configPda(g)[0],
    genesisRaise: genesisRaisePda(g)[0],
    convertBand: genesisConvertBandPda(g)[0],
    allocation: p.wallet ? genesisAllocPda(p.wallet, g)[0] : null,
  };
}

export function readGenesisV2(p                                                                                              )                {
  const raise = decodeGenesisRaise(p.raise);
  const allocation = p.allocation ? decodeGenesisAllocation(p.allocation) : null;
  let claim                            = null;
  if (allocation && BigInt(raise.raised_final ?? 0n) > 0n) claim = genesisWalletClaim(raise, allocation);
  const band = p.band ? decodeGenesisConvertBand(p.band) : null;
  return { raise, allocation, claim, band };
}
