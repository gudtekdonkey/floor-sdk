

import { keccak256 } from "./evm/keccak.js";
import { hex } from "./codec.js";

export function handleKeyPreimage(platform        , handle        )         {
  const p = platform.trim().toLowerCase();
  if (!/^[a-z0-9]+$/.test(p)) throw new RangeError(`a platform is its lowercase short name (e.g. "x"): ${JSON.stringify(platform)}`);
  let h = handle.trim();
  if (h.startsWith("@")) h = h.slice(1);
  h = h.toLowerCase();
  if (h.length === 0 || /[\s@:]/u.test(h)) throw new RangeError(`a handle has no spaces, no ':' and no '@' but one leading: ${JSON.stringify(handle)}`);
  return `${p}:${h}`;
}

export function teamSlotHandleKey(platform        , handle        )             {
  return keccak256(new TextEncoder().encode(handleKeyPreimage(platform, handle)));
}

export function teamSlotHandleId(platform        , handle        )         {
  return "0x" + hex(teamSlotHandleKey(platform, handle));
}
