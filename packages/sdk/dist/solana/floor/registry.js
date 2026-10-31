

import { addressBytes, base58Encode, readUintLE, hex } from "../../codec.js";
import { R, RS, W, WS, accountDiscriminator, instructionDiscriminator } from "../instruction.js";

import { SYSTEM_PROGRAM_ID, findProgramAddress } from "../pda.js";

import { Writer } from "./borsh.js";
import { FLOOR_AMM_PROGRAM_ID, floorEventAuthorityPda } from "./program.js";

export const CLAIMS_PAGE_HEADER = 80;

export const CLAIMS_PAGE                                         = [
  ["wallet", "pubkey"],
  ["page", "u16"],
  ["count", "u16"],
  ["bump", "u8"],
  ["pad0", "bytes:3"],
  ["lookup_table", "pubkey"],
  ["reserved", "bytes:8"],
];
export const CLAIMS_ENTRY_SIZE = 40;
export const CLAIMS_PAGE_ENTRIES = 100;

export const REGISTRY_ROLES = 1 | 2 | 4 | 32;
const DEFAULT_KEY = "11111111111111111111111111111111";

export const claimsPagePda = (wallet        , page        , p = FLOOR_AMM_PROGRAM_ID)      => {
  if (!Number.isInteger(page) || page < 0 || page > 0xffff) throw new RangeError("a page is a u16");
  return findProgramAddress(["claims", addressBytes(wallet), Uint8Array.of(page & 0xff, page >> 8)], p);
};

export function decodeClaimsPage(data            )             {
  const head = 8 + CLAIMS_PAGE_HEADER;
  if (data.length < head || hex(data.slice(0, 8)) !== hex(accountDiscriminator("ClaimsPage"))) throw new RangeError("not a ClaimsPage");
  const count = Number(readUintLE(data, 8 + 34, 2));
  if (count > CLAIMS_PAGE_ENTRIES || data.length !== head + count * CLAIMS_ENTRY_SIZE) throw new RangeError(`ClaimsPage: ${data.length} bytes for ${count} entries`);
  const lut = base58Encode(data.slice(8 + 40, 8 + 72));
  const entries                = [];
  for (let i = 0; i < count; i++) {
    const o = head + i * CLAIMS_ENTRY_SIZE;
    entries.push({ launch: base58Encode(data.slice(o, o + 32)), roles: data[o + 32] });
  }
  return { wallet: base58Encode(data.slice(8, 40)), page: Number(readUintLE(data, 8 + 32, 2)), count, lookupTable: lut === DEFAULT_KEY ? null : lut, entries };
}

export function launchesFromRegistry(pages                       )                                                          {
  const launches = new Map                ();
  let lookupTable                = null;
  for (const p of pages) {
    if (p.page === 0) lookupTable = p.lookupTable;
    for (const e of p.entries) launches.set(e.launch, (launches.get(e.launch) ?? 0) | (e.roles & REGISTRY_ROLES));
  }
  return { launches: [...launches].map(([launch, roles]) => ({ launch, roles })), lookupTable };
}

export function nextClaimsPage(pages                       )         {
  if (pages.length === 0) return 0;
  const last = pages[pages.length - 1];
  return last.count < CLAIMS_PAGE_ENTRIES ? last.page : last.page + 1;
}

function ix(name        , accounts               , args                       , programId = FLOOR_AMM_PROGRAM_ID)              {
  const ev = floorEventAuthorityPda(programId)[0];
  return { programAddress: programId, accounts: [...accounts, R(ev), R(programId)], data: args(new Writer().bytes(instructionDiscriminator(name))).done() };
}

export function registerRoleInstruction(p                                                                                                    )              {
  if (![1, 2, 4, 32].includes(p.role)) throw new RangeError("one role bit: 1, 2, 4 or 32");
  const pid = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  const prev = p.page === 0 ? pid : claimsPagePda(p.wallet, p.page - 1, pid)[0];
  return ix("register_role", [WS(p.payer), W(claimsPagePda(p.wallet, p.page, pid)[0]), R(prev), R(p.subject), R(SYSTEM_PROGRAM_ID)], (w) => w.pubkey(p.wallet).u16(p.page).u8(p.role), pid);
}

export function setClaimsLutInstruction(p                                                              )              {
  const pid = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  return ix("set_claims_lut", [RS(p.wallet), W(claimsPagePda(p.wallet, 0, pid)[0])], (w) => w.pubkey(p.lookupTable ?? "11111111111111111111111111111111"), pid);
}

export function registryHookAccounts(p                                                                                           )                {
  const pid = p.programId ?? FLOOR_AMM_PROGRAM_ID;
  return [WS(p.payer), R(SYSTEM_PROGRAM_ID), ...p.pages.map((x) => W(claimsPagePda(x.wallet, x.page, pid)[0]))];
}
