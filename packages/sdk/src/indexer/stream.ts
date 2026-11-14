

import { decode, int, nullable, object, optional, str, unknown } from "./decode.ts";
import { HEAD, LIVE_LAUNCH, TRADE } from "./bodies.ts";
import type { Head, Launch, Trade } from "./bodies.ts";
import { indexerPath } from "./paths.ts";
import type { StreamQuery } from "./paths.ts";

const HELLO = object({ head: HEAD, window: int, oldest: nullable(str), epoch: optional(str) });
const HEAD_SEEN = object({ block: int, age: int });
const LAUNCH_FRAME = object({ launch: LIVE_LAUNCH });
const STATE = object({ launch: str, mint: str, block: int, ts: nullable(int), seq: int, prev: unknown, state: LIVE_LAUNCH });
const REORG = object({ block: int, rollbacks: int });
const RESET = object({ reason: str, head: HEAD });

export type StreamFrame =
  | { event: "hello"; data: { head: Head; window: number; oldest: string | null; epoch?: string } }
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

  const t = text.indexOf("\r") < 0 ? text : text.replace(/\r\n/g, "\n");
  let at = 0;
  let cut: number;
  while ((cut = t.indexOf("\n\n", at)) >= 0) {
    frameOf(t.slice(at, cut), frames);
    at = cut + 2;
  }
  return { frames, rest: at === 0 ? t : t.slice(at) };
}

export interface StreamParser {

  push(chunk: string): StreamFrame[];

  rest(): string;
}

export function createStreamParser(): StreamParser {
  let pieces: string[] = [];
  let cr = false;
  let endsNl = false;
  const parser: StreamParser = {
    push(chunk: string): StreamFrame[] {
      const frames: StreamFrame[] = [];
      let c = cr ? `\r${chunk}` : chunk;
      cr = c.endsWith("\r");
      if (cr) c = c.slice(0, -1);
      if (c.indexOf("\r") >= 0) c = c.replace(/\r\n/g, "\n");
      if (c.length === 0) return frames;
      let at = 0;

      if (endsNl && pieces.length > 0 && c.charCodeAt(0) === 10) {
        const block = pieces.join("");
        pieces = [];
        frameOf(block.slice(0, -1), frames);
        at = 1;
      }
      let cut: number;
      while ((cut = c.indexOf("\n\n", at)) >= 0) {
        const head = c.slice(at, cut);
        frameOf(pieces.length > 0 ? pieces.join("") + head : head, frames);
        pieces = [];
        at = cut + 2;
      }
      if (at < c.length) {
        pieces.push(at === 0 ? c : c.slice(at));
        endsNl = c.charCodeAt(c.length - 1) === 10;
      } else endsNl = false;
      return frames;
    },
    rest(): string {
      const r = pieces.join("");
      pieces = r.length > 0 ? [r] : [];
      return cr ? `${r}\r` : r;
    },
  };
  return parser;
}

function frameOf(block: string, frames: StreamFrame[]): void {
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
    return;
  }
  const f = decodeStreamFrame(event, JSON.parse(data.join("\n")), id);
  if (f !== null) frames.push(f);
}
