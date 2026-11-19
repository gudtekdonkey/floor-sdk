

import { Address, beginCell, bytesOfBase64, bytesOfHex, Cell, fromBoc, hexOf, snakeCell } from "./cell.js";

import { addr } from "./addresses.js";
import { op as opcodeOf } from "./ops.js";
import { decodeRowV2 } from "../math/v2/preset.js";

export class TonGovDecodeError extends Error {
  constructor(m        ) {
    super(m);
    this.name = "TonGovDecodeError";
  }
}

const AUTH = "authority";
const TL = "timelock";
const GOV = "timelock / governance";

export const TON_GOV_SPEC = {
  transfer_authority: { to: "FloorFactory", from: AUTH, fields: [["new_authority", "MsgAddressInt", "addrInt"]] },
  accept_authority: { to: "FloorFactory", from: "pending authority", fields: [] },
  set_timelock: { to: "FloorFactory", from: AUTH, fields: [["timelock", "MsgAddressInt", "addrInt"]] },
  set_create_paused: { to: "FloorFactory", from: "governance multisig (short delay)", fields: [["paused", "Bool", "bool"]] },
  set_treasury: { to: "FloorFactory", from: TL, fields: [["treasury", "MsgAddressInt", "addrInt"]] },
  set_native_create_fee: { to: "FloorFactory", from: AUTH, fields: [["fee", "Coins", "gram"]] },
  set_metadata_base: { to: "FloorFactory", from: AUTH, fields: [["base", "^SnakeString", "snake"]] },
  set_referral_authority: { to: "FloorFactory", from: AUTH, fields: [["pubkey", "uint256", "pubkey"]] },
  set_holder_dest: { to: "FloorFactory", from: AUTH, fields: [["dest", "MsgAddressInt", "addrInt"]] },
  add_preset: { to: "FloorFactory", from: AUTH, fields: [["id", "uint32", "u32"], ["row", "^PresetRow", "presetRow"]] },
  update_preset: { to: "FloorFactory", from: AUTH, fields: [["id", "uint32", "u32"], ["row", "^PresetRow", "presetRow"]] },
  set_preset_enabled: { to: "FloorFactory", from: AUTH, fields: [["id", "uint32", "u32"], ["enabled", "Bool", "bool"]] },
  delete_preset: { to: "FloorFactory", from: AUTH, fields: [["id", "uint32", "u32"]] },
  add_quote: { to: "FloorFactory", from: AUTH, fields: [["row", "^QuoteEntry", "quoteEntry"]] },
  set_quote_cap: { to: "FloorFactory", from: AUTH, fields: [["quote", "MsgAddress", "addr"], ["cap", "Coins", "quoteCoins"]] },
  update_quote: { to: "FloorFactory", from: AUTH, fields: [["row", "^QuoteEntry", "quoteEntry"]] },
  set_quote_creates: {
    to: "FloorFactory", from: "governance multisig (short delay)", fields: [["quote", "MsgAddress", "addr"], ["enabled", "Bool", "bool"]],
  },
  set_price_updater: { to: "FloorFactory", from: AUTH, fields: [["pubkey", "uint256", "pubkey"]] },
  withdraw_platform: { to: "FloorFactory", from: GOV, fields: [["launch_ids", "^Cell", "launchIds"]] },
  withdraw_holder: { to: "FloorFactory", from: GOV, fields: [["launch_ids", "^Cell", "launchIds"]] },
  pull_bucket: {
    to: "FloorLaunch", from: "FloorFactory only", fields: [["bucket", "uint8", "u8"], ["to", "MsgAddressInt", "addrInt"]],
  },
  push_upgrade: { to: "FloorFactory", from: "any", fields: [["from_id", "uint64", "u64"], ["count", "uint16", "u16"]] },
  upgrade_code: {
    to: "FloorLaunch | FloorAccount | FloorTrader",
    from: "FloorLaunch, FloorTrader: FloorFactory only; FloorAccount: its launch only (relay_code); only the scheduled code (code_hash = cell_hash(code), else NotAuthority)",
    fields: [["version", "uint32", "u32"], ["code_hash", "uint256", "hash"], ["code", "^Cell", "code"], ["migrate", "(Maybe ^Cell)", "maybeCell"]],
  },
  schedule: { to: "FloorTimelock", from: "the multisig v2 admin", fields: [["action", "^Cell", "action"], ["eta", "uint32", "u32"]] },
  execute: { to: "FloorTimelock", from: "any after eta", fields: [["action_hash", "uint256", "hash"]] },
  cancel: { to: "FloorTimelock", from: "the multisig v2 admin", fields: [["action_hash", "uint256", "hash"]] },
  set_code: {
    to: "FloorFactory", from: TL, fields: [["family", "uint8", "u8"], ["version", "uint32", "u32"], ["code", "^Cell", "code"]],
  },
  update_delay: { to: "FloorTimelock", from: "the timelock itself (a scheduled self action)", fields: [["delay", "uint32", "u32"]] },
  set_admin: { to: "FloorTimelock", from: "the timelock itself (a scheduled self action)", fields: [["admin", "MsgAddressInt", "addrInt"]] },
  accept_admin: { to: "FloorTimelock", from: "the pending admin (set by a scheduled set_admin)", fields: [] },
  push_child_code: {
    to: "FloorFactory", from: "any",
    fields: [["family", "uint8", "u8"], ["launch_id", "uint64", "u64"], ["owners", "^AddrList", "addrList"]],
  },
  relay_code: {
    to: "FloorLaunch", from: "FloorFactory only",
    fields: [["version", "uint32", "u32"], ["code_hash", "uint256", "hash"], ["code", "^Cell", "code"], ["owners", "^AddrList", "addrList"]],
  },
}                                          ;

export const TON_GOV_OPS = Object.keys(TON_GOV_SPEC)              ;

export const TON_GRAM_DECIMALS = 9;
const FAMILY = ["factory", "launch", "launch-token wallet", "account", "trader"]         ;
const BUCKET = ["platform → treasury", "holder → holder_dest"]         ;

export function tonFormatUnits(v        , decimals        )         {
  if (decimals === 0) return v.toString();
  const s = v.toString().padStart(decimals + 1, "0");
  const int = s.slice(0, -decimals);
  const frac = s.slice(-decimals).replace(/0+$/, "");
  return frac ? `${int}.${frac}` : int;
}
const hex64 = (v        )         => v.toString(16).padStart(64, "0");
const cellHash = (c      )         => hexOf(c.hash());
const addrText = (a                )         => (a ? a.toString({ bounceable: true }) : "addr_none");

const MAX = { u8: 0xffn, u16: 0xffffn, u32: 0xffffffffn, u64: (1n << 64n) - 1n, u256: (1n << 256n) - 1n }         ;
function uint(v                 , bits                        , what        )         {
  const b = BigInt(v);
  const max = { 8: MAX.u8, 16: MAX.u16, 32: MAX.u32, 64: MAX.u64, 256: MAX.u256 }[bits];
  if (b < 0n || b > max) throw new RangeError(`${what} out of uint${bits} range`);
  return b;
}
function coins(v        , what        )         {
  if (v < 0n || v >= 1n << 120n) throw new RangeError(`${what} out of Coins range`);
  return v;
}

export function tonPresetRowCell(bytes            )       {
  let next              = null;
  for (let off = Math.floor(Math.max(bytes.length - 1, 0) / 127) * 127; off >= 0; off -= 127) {
    const b = beginCell().storeBytes(bytes.subarray(off, off + 127));
    if (next) b.storeRef(next);
    next = b.endCell();
  }
  return next ;
}

export function tonQuoteEntryCell(e                  )       {
  if (e.kind !== 0 && e.kind !== 1) throw new RangeError("quote kind is 0 (native) or 1 (jetton)");
  return beginCell()
    .storeUint(e.kind, 1)
    .storeAddress(e.master)
    .storeUint(uint(e.decimals, 8, "decimals"), 8)
    .storeUint(uint(e.walletKind, 8, "wallet_kind"), 8)
    .storeUint(uint(e.walletCodeHash, 256, "wallet_code_hash"), 256)
    .storeMaybeRef(e.walletCode)
    .storeCoins(coins(e.createFee, "create_fee"))
    .storeBit(e.issuerCanFreeze)
    .storeBit(e.createsEnabled)
    .storeCoins(coins(e.cap, "cap"))
    .endCell();
}

export function tonLaunchIdsCell(ids                   )       {
  if (ids.length === 0) throw new RangeError("launch_ids is empty");
  if (ids.length > 250) throw new RangeError("launch_ids holds at most 250 ids (PUSH_BATCH_MAX)");
  let next              = null;
  for (let off = Math.floor((ids.length - 1) / 15) * 15; off >= 0; off -= 15) {
    const b = beginCell();
    for (const id of ids.slice(off, off + 15)) b.storeUint(uint(id, 64, "launch id"), 64);
    if (next) b.storeRef(next);
    next = b.endCell();
  }
  return next ;
}

export function tonAddrListCell(list                        )       {
  if (list.length === 0) throw new RangeError("owners is empty");
  if (list.length > 32) throw new RangeError("owners holds at most 32 addresses");
  let next              = null;
  for (let off = Math.floor((list.length - 1) / 3) * 3; off >= 0; off -= 3) {
    const part = list.slice(off, off + 3).map(addr);
    const b = beginCell().storeUint(part.length, 2).storeAddress(part[0] );
    b.storeMaybeAddress(part[1] ?? null).storeMaybeAddress(part[2] ?? null);
    b.storeMaybeRef(next);
    next = b.endCell();
  }
  return next ;
}

function storeField(b         , k      , v         , name        )       {
  switch (k) {
    case "u8": b.storeUint(uint(v          , 8, name), 8); return;
    case "u16": b.storeUint(uint(v          , 16, name), 16); return;
    case "u32": b.storeUint(uint(v          , 32, name), 32); return;
    case "u64": b.storeUint(uint(v          , 64, name), 64); return;
    case "u256": case "hash": case "pubkey": b.storeUint(uint(v          , 256, name), 256); return;
    case "bool": b.storeBit(v           ); return;
    case "addrInt": b.storeAddress(addr(v               )); return;
    case "addr": b.storeAddress(v == null ? null : addr(v               )); return;
    case "gram": case "quoteCoins": b.storeCoins(coins(v          , name)); return;
    case "snake": b.storeRef(snakeCell(v          )); return;
    case "presetRow": decodeRowV2(v              ); b.storeRef(tonPresetRowCell(v              )); return;
    case "quoteEntry": b.storeRef(tonQuoteEntryCell(v                    )); return;
    case "launchIds": b.storeRef(tonLaunchIdsCell(v            )); return;
    case "addrList": b.storeRef(tonAddrListCell(v                 )); return;
    case "code": case "action": b.storeRef(v        ); return;
    case "maybeCell": b.storeMaybeRef(v               ); return;
  }
}

export function buildTonGovBody                    (name   , fields                 , queryId         = 0n)       {
  const spec                     = (TON_GOV_SPEC                          )[name];
  if (!spec) throw new RangeError(`${name} is not a governance op`);
  const b = beginCell().storeUint(opcodeOf(name), 32).storeUint(uint(queryId, 64, "query_id"), 64);
  const f = fields                           ;
  for (const [n, , k] of spec.fields) {
    if (!(n in f)) throw new RangeError(`${name}: field ${n} missing`);
    if (name === "withdraw_holder" && k === "launchIds" && (f[n]            ).some((x) => x === 0n)) {
      throw new RangeError("withdraw_holder: launch id 0 is the factory's own platform bucket; the factory refuses it for the holder bucket");
    }
    storeField(b, k, f[n], `${name}.${n}`);
  }
  for (const n of Object.keys(f)) if (!spec.fields.some((x) => x[0] === n)) throw new RangeError(`${name}: unknown field ${n}`);
  return b.endCell();
}

export function buildTonGovSchedule                    (inner   , fields                 , eta        , queryId         = 0n, innerQueryId = 0n)       {
  const action = buildTonGovBody(inner, fields, innerQueryId);
  decodeTonGovAction(action, {}, true);
  return buildTonGovBody("schedule", { action, eta }                            , queryId);
}

const BY_OPCODE = new Map                  ();
function byOpcode()                        {
  if (BY_OPCODE.size === 0) for (const n of TON_GOV_OPS) BY_OPCODE.set(opcodeOf(n), n);
  return BY_OPCODE;
}

function guard   (what        , f         )    {
  try {
    return f();
  } catch (e) {
    if (e instanceof TonGovDecodeError) throw e;
    throw new TonGovDecodeError(`${what}: ${(e         ).message}`);
  }
}
function done(s       , what        )       {
  if (s.remainingBits !== 0) throw new TonGovDecodeError(`${what}: ${s.remainingBits} bits left over`);
  if (s.remainingRefs !== 0) throw new TonGovDecodeError(`${what}: ${s.remainingRefs} refs left over`);
}
function addrInt(s       , what        )          {
  const a = guard(what, () => s.loadAddress());
  if (!a) throw new TonGovDecodeError(`${what}: addr_none where MsgAddressInt is required`);
  return a;
}
function maybeAddrInt(s       , what        )                 {
  return guard(what, () => s.loadBit()) ? addrInt(s, what) : null;
}

function readPresetRow(c      , what        )             {
  const parts               = [];
  for (let cur              = c, i = 0; cur; i++) {
    const s = cur.beginParse();
    if (s.remainingBits % 8 !== 0) throw new TonGovDecodeError(`${what}: cell ${i} is not whole bytes`);
    if (s.remainingRefs > 1) throw new TonGovDecodeError(`${what}: cell ${i} has ${s.remainingRefs} refs`);
    parts.push(guard(what, () => s.loadBytes(s.remainingBits / 8)));
    cur = s.remainingRefs ? s.loadRef() : null;
  }
  const all = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) (all.set(p, o), (o += p.length));
  guard(what, () => decodeRowV2(all));
  return all;
}
function readSnakeStrict(c      , what        )         {
  const parts               = [];
  for (let cur                   = c, i = 0; cur; cur = cur.refs[0], i++) {
    if (cur.len % 8 !== 0) throw new TonGovDecodeError(`${what}: cell ${i} is not whole bytes`);
    if (cur.refs.length > 1) throw new TonGovDecodeError(`${what}: cell ${i} has ${cur.refs.length} refs`);
    parts.push(cur.bits.subarray(0, cur.len >> 3));
  }
  const all = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) (all.set(p, o), (o += p.length));
  return guard(what, () => new TextDecoder("utf-8", { fatal: true }).decode(all));
}
function readQuoteEntry(c      , what        )                   {
  const s = c.beginParse();
  const e = guard(what, () => {
    const kind = Number(s.loadUint(1))         ;
    const master = s.loadAddress();
    const decimals = Number(s.loadUint(8));
    const walletKind = Number(s.loadUint(8));
    const walletCodeHash = s.loadUint(256);
    const walletCode = s.loadMaybeRef();
    const createFee = s.loadCoins();
    const issuerCanFreeze = s.loadBit();
    const createsEnabled = s.loadBit();
    const cap = s.loadCoins();
    return { kind, master, decimals, walletKind, walletCodeHash, walletCode, createFee, issuerCanFreeze, createsEnabled, cap };
  });
  done(s, what);
  return e;
}
function readLaunchIds(c      , what        )           {
  const ids           = [];
  for (let cur              = c, i = 0; cur; i++) {
    const s = cur.beginParse();
    if (s.remainingBits % 64 !== 0 || s.remainingBits === 0) throw new TonGovDecodeError(`${what}: cell ${i} is not whole uint64 ids`);
    if (s.remainingRefs > 1) throw new TonGovDecodeError(`${what}: cell ${i} has ${s.remainingRefs} refs`);
    while (s.remainingBits >= 64) ids.push(s.loadUint(64));
    cur = s.remainingRefs ? s.loadRef() : null;
  }
  if (ids.length > 250) throw new TonGovDecodeError(`${what}: ${ids.length} ids, at most 250`);
  return ids;
}
function readAddrList(c      , what        )            {
  const out            = [];
  for (let cur              = c, i = 0; cur; i++) {
    const s = cur.beginParse();
    const w = `${what} cell ${i}`;
    const n = Number(guard(w, () => s.loadUint(2)));
    const got = [addrInt(s, w), maybeAddrInt(s, w), maybeAddrInt(s, w)].filter((a)               => a !== null);
    if (n !== got.length || n === 0) throw new TonGovDecodeError(`${w}: n = ${n} but ${got.length} addresses`);
    out.push(...got);
    cur = guard(w, () => s.loadMaybeRef());
    done(s, w);
  }
  if (out.length > 32) throw new TonGovDecodeError(`${what}: ${out.length} addresses, at most 32`);
  return out;
}

function readField(s       , k      , what        , opts                     )          {
  void opts;
  switch (k) {
    case "u8": return Number(guard(what, () => s.loadUint(8)));
    case "u16": return Number(guard(what, () => s.loadUint(16)));
    case "u32": return Number(guard(what, () => s.loadUint(32)));
    case "u64": return guard(what, () => s.loadUint(64));
    case "u256": case "hash": case "pubkey": return guard(what, () => s.loadUint(256));
    case "bool": return guard(what, () => s.loadBit());
    case "addrInt": return addrInt(s, what);
    case "addr": return guard(what, () => s.loadAddress());
    case "gram": case "quoteCoins": return guard(what, () => s.loadCoins());
    case "snake": return readSnakeStrict(guard(what, () => s.loadRef()), what);
    case "presetRow": return readPresetRow(guard(what, () => s.loadRef()), what);
    case "quoteEntry": return readQuoteEntry(guard(what, () => s.loadRef()), what);
    case "launchIds": return readLaunchIds(guard(what, () => s.loadRef()), what);
    case "addrList": return readAddrList(guard(what, () => s.loadRef()), what);
    case "code": case "action": return guard(what, () => s.loadRef());
    case "maybeCell": return guard(what, () => s.loadMaybeRef());
  }
}

function fieldText(name        , k      , v         , opts                     , quote                            , op = "")         {
  void name;
  switch (k) {
    case "u8": case "u16": case "u32": case "u64": case "bool": return String(v);
    case "hash": return `0x${hex64(v          )}`;
    case "pubkey": return `ed25519 0x${hex64(v          )}`;
    case "u256": return `0x${hex64(v          )}`;
    case "addrInt": case "addr": return addrText(v                  );
    case "gram": return `${tonFormatUnits(v          , TON_GRAM_DECIMALS)} GRAM (${v} nanogram)`;
    case "quoteCoins": {
      const d = quote === null ? TON_GRAM_DECIMALS : quote ? opts.quoteDecimals?.(quote) : undefined;
      return d === undefined ? `${v} raw units (the quote's decimals unknown)` : `${tonFormatUnits(v          , d)} (${v} units, ${d} decimals)`;
    }
    case "snake": return JSON.stringify(v);
    case "presetRow": return `${(v              ).length} bytes, canonical RowV2`;
    case "quoteEntry": {
      const e = v                    ;
      return [
        `kind ${e.kind === 0 ? "native (GRAM)" : "jetton"}`, `master ${addrText(e.master)}`, `decimals ${e.decimals}`,
        `wallet_kind ${e.walletKind}`, `wallet_code_hash 0x${hex64(e.walletCodeHash)}`,
        `wallet_code ${e.walletCode ? `0x${cellHash(e.walletCode)}` : "none"}`,
        `create_fee ${tonFormatUnits(e.createFee, e.decimals)}`, `issuer_can_freeze ${e.issuerCanFreeze}`,
        `creates_enabled ${e.createsEnabled}`, `cap ${tonFormatUnits(e.cap, e.decimals)}`,
      ].join(", ");
    }
    case "launchIds": return (v            ).map((x) => (x === 0n ? (op === "withdraw_holder" ? "0 (REFUSED: the factory's own bucket is platform only)" : "0 (the factory's own platform bucket)") : String(x))).join(", ");
    case "addrList": return (v             ).map(addrText).join(", ");
    case "code": case "action": return `cell 0x${cellHash(v        )}`;
    case "maybeCell": return v ? `cell 0x${cellHash(v        )}` : "none";
  }
}

function toCell(body                            )       {
  if (body instanceof Cell) return body;
  return guard("BoC", () => {
    if (body instanceof Uint8Array) return fromBoc(body);
    const t = body.trim();
    return fromBoc(/^(0x)?[0-9a-fA-F]+$/.test(t) ? bytesOfHex(t.replace(/^0x/, "")) : bytesOfBase64(t));
  });
}

export function decodeTonGovAction(body                            , opts                      = {}, asAction = false)               {
  const c = toCell(body);
  const s = c.beginParse();
  const opcode = Number(guard("op", () => s.loadUint(32)));
  const name = byOpcode().get(opcode);
  if (!name) throw new TonGovDecodeError(`unknown op 0x${opcode.toString(16).padStart(8, "0")}: not a governance op, refused`);
  const spec         = TON_GOV_SPEC[name];
  if (asAction && !(spec.to === "FloorFactory" || name === "update_delay" || name === "set_admin")) {
    throw new TonGovDecodeError(`${name} cannot be a timelock action (the timelock sends its actions to the factory)`);
  }
  const queryId = guard(`${name}.query_id`, () => s.loadUint(64));
  const fields                          = {};
  for (const [n, , k] of spec.fields) fields[n] = readField(s, k, `${name}.${n}`, opts);
  done(s, name);

  const derived                         = {};
  let inner                          ;
  for (const [n, , k] of spec.fields) {
    const v = fields[n];
    if (k === "code") derived[`${n}_hash`] = `0x${cellHash(v        )}`;
    if (k === "presetRow") derived[`${n}_hash`] = `0x${cellHash(tonPresetRowCell(v              ))}`;
    if (k === "action") {
      inner = decodeTonGovAction(v        , opts, true);
      derived.action_hash = `0x${cellHash(v        )}`;
    }
  }
  if ("code_hash" in fields && "code" in fields) {
    const ok = hex64(fields.code_hash          ) === cellHash(fields.code        );
    derived.code_hash_matches = ok ? "yes" : "NO: the contract refuses this code (code_hash ≠ cell_hash(code))";
  }
  if (name === "set_code" || name === "push_child_code") derived.family_name = FAMILY[fields.family          ] ?? "unknown family (refused)";
  if (name === "pull_bucket") derived.bucket_name = BUCKET[fields.bucket          ] ?? "unknown bucket (refused)";
  if (name === "set_create_paused") derived.scope = "new creates only; buys and sells can never be paused";

  const quote = "quote" in fields ? (fields.quote                  ) : undefined;
  const lines = [`${name} → ${spec.to} (query_id ${queryId})`];
  for (const [n, , k] of spec.fields) lines.push(`  ${n}: ${fieldText(n, k, fields[n], opts, quote, name)}`);
  for (const [k, v] of Object.entries(derived)) lines.push(`  [${k}] ${v}`);
  if (inner) lines.push("  action decodes as:", ...inner.lines.map((l) => `    ${l}`));
  return { op: name, opcode, target: spec.to, from: spec.from, queryId, fields: fields                          , derived, inner, lines };
}

export const tonGovPresetRow = (bytes            )        => decodeRowV2(bytes);
