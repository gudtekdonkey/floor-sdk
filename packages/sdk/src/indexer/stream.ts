

import { decode, int, nullable, object, str, unknown } from "./decode.ts";
import { HEAD, LIVE_LAUNCH, TRADE } from "./bodies.ts";
import type { Head, Launch, Trade } from "./bodies.ts";
import { indexerPath } from "./paths.ts";
import type { StreamQuery } from "./paths.ts";

const HELLO = object({ head: HEAD, window: int, oldest: nullable(str) });
const HEAD_SEEN = object({ block: int, age: int });
const LAUNCH_FRAME = object({ launch: LIVE_LAUNCH });
const STATE = object({ launch: str, mint: str, block: int, ts: nullable(int), seq: int, prev: unknown, state: LIVE_LAUNCH });
const REORG = object({ block: int, rollbacks: int });
const RESET = object({ reason: str, head: HEAD });

export type StreamFrame =
  | { event: "hello"; data: { head: Head; window: number; oldest: string | null } }
  | { event: "head"; data: { block: number; age: number } }
  | { event: "trade"; id: string; data: Trade }
  | { event: "launch"; data: { launch: Launch } }
  | { event: "launchGone"; data: { launch: string; gone: true } }
  | { event: "state"; data: { launch: string; mint: string; block: number; ts: number | null; seq: number; state: Launch } }
  | { event: "reorg"; data: { block: number; rollbacks: number } }
  | { event: "reset"; data: { reason: string; head: Head } }
  | { event: "comment"; text: string };

export function streamPath(q: StreamQuery = {}): string {
  return indexerPath.stream(q);
}

export function decodeStreamFrame(event: string, data: unknown, id?: string): StreamFrame | null {
  switch (event) {
    case "hello":
      return { event, data: decode(HELLO, data) };
    case "head":
      return { event, data: decode(HEAD_SEEN, data) };
    case "trade":
      if (id === undefined) return null;
      return { event, id, data: decode(TRADE, data) };
    case "launch": {
      const d = data as { launch?: unknown; gone?: unknown } | null;
      if (d !== null && typeof d === "object" && d.gone === true) return { event: "launchGone", data: { launch: str(d.launch, "launch"), gone: true } };
      return { event, data: decode(LAUNCH_FRAME, data) };
    }
    case "state": {
      const s = decode(STATE, data);
      return { event, data: { launch: s.launch, mint: s.mint, block: s.block, ts: s.ts, seq: s.seq, state: s.state } };
    }
    case "reorg":
      return { event, data: decode(REORG, data) };
    case "reset":
      return { event, data: decode(RESET, data) };
    default:
      return null;
  }
}

export function parseStreamFrames(text: string): { frames: StreamFrame[]; rest: string } {
  const frames: StreamFrame[] = [];
  let rest = text.replace(/\r\n/g, "\n");
  let cut: number;
  while ((cut = rest.indexOf("\n\n")) >= 0) {
    const block = rest.slice(0, cut);
    rest = rest.slice(cut + 2);
    let event = "message";
    let id: string | undefined;
    const data: string[] = [];
    let comment: string | null = null;
    for (const line of block.split("\n")) {
      if (line.startsWith(":")) comment = line.slice(1).trim();
      else if (line.startsWith("event:")) event = line.slice(6).trim();
      else if (line.startsWith("id:")) id = line.slice(3).trim();
      else if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
    }
    if (data.length === 0) {
      if (comment !== null) frames.push({ event: "comment", text: comment });
      continue;
    }
    const f = decodeStreamFrame(event, JSON.parse(data.join("\n")), id);
    if (f !== null) frames.push(f);
  }
  return { frames, rest };
}
