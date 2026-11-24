

import { sha256 } from "../sha256.js";
import { hexOf, readSnake } from "./cell.js";

import { addr } from "./addresses.js";
import { StackError, parseRefMap } from "./reader.js";

export const TON_LAUNCH_DECIMALS = 6;

export const TON_CONTENT_KEYS = ["name", "symbol", "decimals", "image", "uri", "description", "image_data"]         ;

export class TonContentError extends Error {}

const keyOf = (k        )         => BigInt("0x" + hexOf(sha256(new TextEncoder().encode(k))));

function valueText(v      , key        )         {
  if (v.len < 8 || v.bits[0] !== 0) throw new TonContentError(`content key "${key}": not a snake value (prefix 0x00)`);
  const head = new TextDecoder().decode(v.bits.subarray(1, v.len >> 3));
  return v.refs[0] ? head + readSnake(v.refs[0]) : head;
}

export function readTonJettonContent(content      )                   {
  const out                   = { layout: "onchain", name: null, symbol: null, decimals: null, image: null, uri: null, description: null };
  if (content.len < 8) throw new TonContentError("content: no layout byte");
  const tag = content.bits[0];
  if (tag === 1) {
    out.layout = "offchain";
    const rest = new TextDecoder().decode(content.bits.subarray(1, content.len >> 3));
    out.uri = content.refs[0] ? rest + readSnake(content.refs[0]) : rest;
    return out;
  }
  if (tag !== 0) throw new TonContentError(`content: unknown layout byte ${tag}`);
  if (content.len < 9) throw new TonContentError("content: no dictionary bit");
  const hasDict = (content.bits[1]  & 0x80) !== 0;
  if (!hasDict) return out;
  const map = parseRefMap(content.refs[0] ?? null, 256);
  const text = (k               )                => {
    const v = map.get(keyOf(k));
    return v ? valueText(v, k) : null;
  };
  out.name = text("name");
  out.symbol = text("symbol");
  out.image = text("image");
  out.uri = text("uri");
  out.description = text("description");
  const d = text("decimals");
  if (d !== null) {
    if (!/^\d{1,3}$/.test(d) || Number(d) > 255) throw new TonContentError(`content: decimals "${d.slice(0, 40)}" is not an integer`);
    out.decimals = Number(d);
  }
  return out;
}

function under(base        , launch             , ext        )         {
  if (base === "") throw new RangeError("metadata_base is unset: a launch created now publishes no uri or image");
  return `${base}${base.endsWith("/") ? "" : "/"}${hexOf(addr(launch).hash)}${ext}`;
}

export const tonLaunchMetadataUri = (base        , launch             )         => under(base, launch, ".json");

export const tonLaunchImageUri = (base        , launch             )         => under(base, launch, ".png");

export function tonLaunchMetadataJson(m                                                                                     )

  {
  const out                                                                                           = {
    name: m.name, symbol: m.symbol, decimals: String(TON_LAUNCH_DECIMALS),
  };
  if (m.image) out.image = m.image;
  if (m.description) out.description = m.description;
  return out;
}

export async function getTonLaunchContent(rpc        , launch             )                            {
  const s = await rpc.runGetMethod(addr(launch), "get_jetton_data", []);
  if (s.length < 5) throw new StackError(`get_jetton_data: ${s.length} stack items, expected 5`);
  const mintable = s[1];
  if (!mintable || mintable.type !== "int") throw new StackError("mintable: expected an int at 1 (TEP-74 order)");
  const c = s[3];
  if (!c || (c.type !== "cell" && c.type !== "slice")) throw new StackError("content: expected a cell at 3");
  return readTonJettonContent(c.cell);
}
