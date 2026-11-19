

import { sha256 } from "../sha256.js";

function crcTable(poly        , reflected         )              {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = reflected ? i : i << 24;
    for (let k = 0; k < 8; k++) {
      if (reflected) c = c & 1 ? (c >>> 1) ^ poly : c >>> 1;
      else c = c & 0x80000000 ? (c << 1) ^ poly : c << 1;
    }
    t[i] = c >>> 0;
  }
  return t;
}
let T32                         ;
let T32C                         ;

export function crc32(data                     )         {
  const b = typeof data === "string" ? new TextEncoder().encode(data) : data;
  const t = (T32 ??= crcTable(0xedb88320, true));
  let c = 0xffffffff;
  for (const x of b) c = t[(c ^ x) & 0xff]  ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export function crc32c(data            )         {
  const t = (T32C ??= crcTable(0x82f63b78, true));
  let c = 0xffffffff;
  for (const x of data) c = t[(c ^ x) & 0xff]  ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export function crc16(data                     )         {
  const b = typeof data === "string" ? new TextEncoder().encode(data) : data;
  let c = 0;
  for (const x of b) {
    c ^= x << 8;
    for (let k = 0; k < 8; k++) c = c & 0x8000 ? ((c << 1) ^ 0x1021) & 0xffff : (c << 1) & 0xffff;
  }
  return c;
}

export const methodId = (name        )         => (crc16(name) & 0xffff) | 0x10000;

export function hexOf(b            )         {
  let s = "";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
}
export function bytesOfHex(h        )             {
  const s = h.startsWith("0x") ? h.slice(2) : h;
  if (s.length % 2 !== 0 || /[^0-9a-fA-F]/.test(s)) throw new RangeError("bad hex");
  const o = new Uint8Array(s.length / 2);
  for (let i = 0; i < o.length; i++) o[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return o;
}
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
export function base64Of(b            , urlSafe = false)         {
  let s = "";
  for (let i = 0; i < b.length; i += 3) {
    const n = (b[i]  << 16) | ((b[i + 1] ?? 0) << 8) | (b[i + 2] ?? 0);
    s += B64[(n >> 18) & 63]  + B64[(n >> 12) & 63]  + (i + 1 < b.length ? B64[(n >> 6) & 63]  : "=") + (i + 2 < b.length ? B64[n & 63]  : "=");
  }
  return urlSafe ? s.replace(/\+/g, "-").replace(/\//g, "_") : s;
}
export function bytesOfBase64(s        )             {
  const t = s.replace(/-/g, "+").replace(/_/g, "/").replace(/=+$/, "");
  const out           = [];
  let acc = 0;
  let bits = 0;
  for (const ch of t) {
    const v = B64.indexOf(ch);
    if (v < 0) throw new RangeError("bad base64");
    acc = (acc << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((acc >> bits) & 0xff);
    }
  }
  return Uint8Array.from(out);
}

export class Address {
           workchain        ;
           hash            ;
  constructor(workchain        , hash            ) {
    if (!Number.isInteger(workchain) || workchain < -128 || workchain > 127) throw new RangeError("workchain out of range");
    if (hash.length !== 32) throw new RangeError("address hash must be 32 bytes");
    this.workchain = workchain;
    this.hash = hash.slice();
  }
  equals(o         )          {
    return this.workchain === o.workchain && hexOf(this.hash) === hexOf(o.hash);
  }

  toRaw()         {
    return `${this.workchain}:${hexOf(this.hash)}`;
  }

  toString(o                                                                  = {})         {
    const tag = (o.testOnly ? 0x80 : 0) | (o.bounceable === false ? 0x51 : 0x11);
    const b = new Uint8Array(36);
    b[0] = tag;
    b[1] = this.workchain & 0xff;
    b.set(this.hash, 2);
    const c = crc16(b.subarray(0, 34));
    b[34] = c >> 8;
    b[35] = c & 0xff;
    return base64Of(b, o.urlSafe ?? true);
  }
  static parse(s        )          {
    if (/^-?\d+:[0-9a-fA-F]{64}$/.test(s)) {
      const [w, h] = s.split(":")                    ;
      return new Address(Number(w), bytesOfHex(h));
    }
    if (s.length !== 48) throw new RangeError("bad address length");
    const b = bytesOfBase64(s);
    if (b.length !== 36) throw new RangeError("bad address length");
    const c = crc16(b.subarray(0, 34));
    if (b[34] !== c >> 8 || b[35] !== (c & 0xff)) throw new RangeError("bad address checksum");
    const tag = b[0]  & ~0x80;
    if (tag !== 0x11 && tag !== 0x51) throw new RangeError("bad address tag");
    return new Address(b[1]  > 127 ? b[1]  - 256 : b[1] , b.subarray(2, 34));
  }
}

export const MAX_CELL_BITS = 1023;
export const MAX_CELL_REFS = 4;

export class Cell {
           bits            ;
           len        ;
           refs                 ;

           exotic         ;
  #hash             ;
  #depth         ;
  constructor(bits            , len        , refs                  = [], exotic = false) {
    if (exotic && (len !== 264 || refs.length !== 0 || bits.length !== 33 || bits[0] !== 2)) {
      throw new RangeError("exotic cell: only a library cell (type 2: 0x02 + 32-byte hash, no refs) is allowed");
    }
    if (len < 0 || len > MAX_CELL_BITS) throw new RangeError("cell over 1023 bits");
    if (refs.length > MAX_CELL_REFS) throw new RangeError("cell over 4 refs");
    if (bits.length !== Math.ceil(len / 8)) throw new RangeError("bits length mismatch");
    this.bits = bits;
    this.len = len;
    this.refs = refs;
    this.exotic = exotic;
  }

  #repr()             {
    const dataBytes = Math.ceil(this.len / 8);
    const out = new Uint8Array(2 + dataBytes);
    out[0] = this.refs.length | (this.exotic ? 8 : 0);
    out[1] = Math.ceil(this.len / 8) + Math.floor(this.len / 8);
    out.set(this.bits, 2);
    if (this.len % 8 !== 0) out[2 + dataBytes - 1] = out[2 + dataBytes - 1]  | (0x80 >> this.len % 8);
    return out;
  }
  depth()         {
    if (this.#depth === undefined) this.#depth = this.refs.length === 0 ? 0 : 1 + Math.max(...this.refs.map((r) => r.depth()));
    return this.#depth;
  }
  hash()             {
    if (!this.#hash) {
      const r = this.#repr();
      const buf = new Uint8Array(r.length + this.refs.length * 34);
      buf.set(r, 0);
      let o = r.length;
      for (const c of this.refs) {
        const d = c.depth();
        buf[o++] = d >> 8;
        buf[o++] = d & 0xff;
      }
      for (const c of this.refs) {
        buf.set(c.hash(), o);
        o += 32;
      }
      this.#hash = sha256(buf);
    }
    return this.#hash.slice();
  }
  beginParse()        {
    if (this.exotic) throw new RangeError("a library cell has no readable data here: resolve the library (libraryHashOf gives its hash)");
    return new Slice(this);
  }
  toBoc(o                      = {})             {
    return toBoc(this, o);
  }
}

export const EMPTY_CELL = new Cell(new Uint8Array(0), 0);

export function libraryCell(hash                     )       {
  const h = typeof hash === "string" ? bytesOfHex(hash) : hash;
  if (h.length !== 32) throw new RangeError("library hash must be 32 bytes");
  const bits = new Uint8Array(33);
  bits[0] = 2;
  bits.set(h, 1);
  return new Cell(bits, 264, [], true);
}
export const isLibraryCell = (c      )          => c.exotic;

export function libraryHashOf(c      )             {
  if (!c.exotic) throw new RangeError("not a library cell");
  return c.bits.slice(1, 33);
}

export class Builder {
  #bits           = [];
  #refs         = [];
  get bitLength()         {
    return this.#bits.length;
  }
  get refCount()         {
    return this.#refs.length;
  }
  #push(v        , n        )       {
    if (this.#bits.length + n > MAX_CELL_BITS) throw new RangeError("cell over 1023 bits");
    for (let i = n - 1; i >= 0; i--) this.#bits.push(Number((v >> BigInt(i)) & 1n));
  }
  storeBit(b                           )       {
    this.#push(b ? 1n : 0n, 1);
    return this;
  }
  storeUint(v                 , n        )       {
    const x = BigInt(v);
    if (x < 0n || x >= 1n << BigInt(n)) throw new RangeError(`value ${x} does not fit uint${n}`);
    this.#push(x, n);
    return this;
  }
  storeInt(v                 , n        )       {
    const x = BigInt(v);
    const lim = 1n << BigInt(n - 1);
    if (x < -lim || x >= lim) throw new RangeError(`value ${x} does not fit int${n}`);
    this.#push(BigInt.asUintN(n, x), n);
    return this;
  }
  storeBytes(b            )       {
    for (const x of b) this.storeUint(x, 8);
    return this;
  }

  storeCoins(v                 )       {
    const x = BigInt(v);
    if (x < 0n || x >= 1n << 120n) throw new RangeError("coins out of range");
    if (x === 0n) return this.storeUint(0, 4);
    let n = 0;
    for (let t = x; t > 0n; t >>= 8n) n++;
    this.storeUint(n, 4);
    return this.storeUint(x, n * 8);
  }

  storeAddress(a                            )       {
    if (!a) return this.storeUint(0, 2);
    this.storeUint(2, 2).storeUint(0, 1).storeInt(a.workchain, 8);
    return this.storeBytes(a.hash);
  }
  storeRef(c      )       {
    if (this.#refs.length >= MAX_CELL_REFS) throw new RangeError("cell over 4 refs");
    this.#refs.push(c);
    return this;
  }

  storeMaybeRef(c                         )       {
    if (c) return this.storeBit(1).storeRef(c);
    return this.storeBit(0);
  }

  storeMaybeAddress(a                            )       {
    if (a) return this.storeBit(1).storeAddress(a);
    return this.storeBit(0);
  }
  storeBuilder(b         )       {
    for (const x of b.#bits) this.#push(BigInt(x), 1);
    for (const r of b.#refs) this.storeRef(r);
    return this;
  }
  endCell()       {
    const len = this.#bits.length;
    const out = new Uint8Array(Math.ceil(len / 8));
    for (let i = 0; i < len; i++) if (this.#bits[i]) out[i >> 3] = out[i >> 3]  | (0x80 >> (i & 7));
    return new Cell(out, len, this.#refs.slice());
  }
}

export const beginCell = ()          => new Builder();

export class Slice {
  #cell      ;
  #bit = 0;
  #ref = 0;
  constructor(c      ) {
    this.#cell = c;
  }
  get remainingBits()         {
    return this.#cell.len - this.#bit;
  }
  get remainingRefs()         {
    return this.#cell.refs.length - this.#ref;
  }
  #take(n        )         {
    if (this.#bit + n > this.#cell.len) throw new RangeError("cell underflow");
    let v = 0n;
    for (let i = 0; i < n; i++) {
      const p = this.#bit + i;
      v = (v << 1n) | BigInt((this.#cell.bits[p >> 3]  >> (7 - (p & 7))) & 1);
    }
    this.#bit += n;
    return v;
  }
  loadBit()          {
    return this.#take(1) === 1n;
  }
  loadUint(n        )         {
    return this.#take(n);
  }
  loadInt(n        )         {
    return BigInt.asIntN(n, this.#take(n));
  }
  loadBytes(n        )             {
    const o = new Uint8Array(n);
    for (let i = 0; i < n; i++) o[i] = Number(this.#take(8));
    return o;
  }
  loadCoins()         {
    const n = Number(this.#take(4));
    return n === 0 ? 0n : this.#take(n * 8);
  }

  loadAddress()                 {
    const t = Number(this.#take(2));
    if (t === 0) return null;
    if (t !== 2) throw new RangeError("unsupported address kind");
    if (this.#take(1) === 1n) throw new RangeError("anycast not supported");
    const wc = Number(this.loadInt(8));
    return new Address(wc, this.loadBytes(32));
  }
  loadMaybeAddress()                 {
    return this.loadBit() ? this.loadAddress() : null;
  }
  loadRef()       {
    const c = this.#cell.refs[this.#ref++];
    if (!c) throw new RangeError("ref underflow");
    return c;
  }
  loadMaybeRef()              {
    return this.loadBit() ? this.loadRef() : null;
  }

  endParse()       {
    if (this.remainingBits !== 0 || this.remainingRefs !== 0) throw new RangeError("slice not fully read");
  }
}

export function snakeCell(text        )       {
  const bytes = new TextEncoder().encode(text);
  const chunks               = [];
  for (let i = 0; i < bytes.length; i += 127) chunks.push(bytes.subarray(i, i + 127));
  if (chunks.length === 0) chunks.push(new Uint8Array(0));
  let next              = null;
  for (let i = chunks.length - 1; i >= 0; i--) {
    const b = beginCell().storeBytes(chunks[i] );
    if (next) b.storeRef(next);
    next = b.endCell();
  }
  return next ;
}
export function readSnake(c      )         {
  const parts               = [];
  for (let cur                   = c; cur; cur = cur.refs[0]) parts.push(cur.bits.subarray(0, cur.len >> 3));
  const all = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) {
    all.set(p, o);
    o += p.length;
  }
  return new TextDecoder().decode(all);
}

function sortCells(root      )           {
  let pending         = [root];
  const all = new Map                                        ();
  const notPerm = new Set        ();
  const sorted           = [];
  while (pending.length > 0) {
    const cells = pending;
    pending = [];
    for (const cell of cells) {
      const h = hexOf(cell.hash());
      if (all.has(h)) continue;
      notPerm.add(h);
      all.set(h, { cell, refs: cell.refs.map((r) => hexOf(r.hash())) });
      for (const r of cell.refs) pending.push(r);
    }
  }
  const visit = (h        )       => {
    if (!notPerm.has(h)) return;
    const refs = all.get(h) .refs;
    for (let i = refs.length - 1; i >= 0; i--) visit(refs[i] );
    sorted.push(h);
    notPerm.delete(h);
  };
  while (notPerm.size > 0) visit(notPerm.values().next().value          );
  const idx = new Map                ();
  for (let i = 0; i < sorted.length; i++) idx.set(sorted[sorted.length - i - 1] , i);
  const out           = [];
  for (let i = sorted.length - 1; i >= 0; i--) {
    const e = all.get(sorted[i] ) ;
    out.push({ cell: e.cell, refs: e.refs.map((r) => idx.get(r) ) });
  }
  return out;
}

const bytesFor = (n        )         => Math.max(Math.ceil(n.toString(2).length / 8), 1);

export function toBoc(root      , o                      = {})             {
  const crc = o.crc32 ?? true;
  const cells = sortCells(root);
  const size = bytesFor(cells.length);
  let total = 0;
  for (const c of cells) total += 2 + c.cell.bits.length + c.refs.length * size;
  const off = bytesFor(total);
  const head = 4 + 1 + 1 + 3 * size + off + size;
  const out = new Uint8Array(head + total + (crc ? 4 : 0));
  let p = 0;
  const put = (v        , n        )       => {
    for (let i = n - 1; i >= 0; i--) out[p++] = Math.floor(v / 2 ** (8 * i)) & 0xff;
  };
  put(0xb5ee9c72, 4);
  put((crc ? 0x40 : 0) | size, 1);
  put(off, 1);
  put(cells.length, size);
  put(1, size);
  put(0, size);
  put(total, off);
  put(0, size);
  for (const { cell, refs } of cells) {
    out[p++] = cell.refs.length | (cell.exotic ? 8 : 0);
    out[p++] = Math.ceil(cell.len / 8) + Math.floor(cell.len / 8);
    const data = cell.bits.slice();
    if (cell.len % 8 !== 0) data[data.length - 1] = data[data.length - 1]  | (0x80 >> cell.len % 8);
    out.set(data, p);
    p += data.length;
    for (const r of refs) put(r, size);
  }
  if (crc) {
    const c = crc32c(out.subarray(0, p));

    out[p++] = c & 0xff;
    out[p++] = (c >>> 8) & 0xff;
    out[p++] = (c >>> 16) & 0xff;
    out[p++] = (c >>> 24) & 0xff;
  }
  return out;
}

export function fromBoc(b            )       {
  let p = 0;
  const get = (n        )         => {
    if (p + n > b.length) throw new RangeError("BoC truncated");
    let v = 0;
    for (let i = 0; i < n; i++) v = v * 256 + b[p++] ;
    return v;
  };
  if (get(4) !== 0xb5ee9c72) throw new RangeError("bad BoC magic");
  const flags = get(1);
  const hasIdx = (flags & 0x80) !== 0;
  const hasCrc = (flags & 0x40) !== 0;
  const size = flags & 7;
  if (size < 1 || size > 4) throw new RangeError("BoC bad size");
  const off = get(1);
  if (off < 1 || off > 6) throw new RangeError("BoC bad offset size");
  const n = get(size);
  const roots = get(size);
  get(size);
  const total = get(off);
  if (roots !== 1) throw new RangeError("BoC must have one root");
  if (n < 1 || n > b.length) throw new RangeError("BoC bad cell count");
  const rootIdx = get(size);
  if (rootIdx >= n) throw new RangeError("BoC bad root");
  if (hasIdx) {
    if (p + n * off > b.length) throw new RangeError("BoC truncated");
    p += n * off;
  }
  const end = p + total;
  if (end > b.length) throw new RangeError("BoC truncated");
  const raw                                                                       = [];
  while (p < end) {
    const d1 = get(1);
    const d2 = get(1);
    const exotic = (d1 & 8) !== 0;
    if (exotic && ((d1 & 7) !== 0 || d1 >> 5 !== 0 || d2 !== 66)) {
      throw new RangeError("exotic cell type " + (p < end ? b[p]  : "?") + " is refused: only a library cell (type 2) is allowed");
    }
    if ((d1 & 7) > 4) throw new RangeError("BoC bad ref count");
    const nb = Math.ceil(d2 / 2);
    if (p + nb > end) throw new RangeError("BoC truncated");
    const data = b.slice(p, p + nb);
    p += nb;
    let len = nb * 8;
    if (d2 & 1) {
      const last = data[nb - 1] ;
      if (last === 0) throw new RangeError("BoC bad completion tag");
      let t = 0;
      while (((last >> t) & 1) === 0) t++;
      len = nb * 8 - t - 1;
      data[nb - 1] = last & ~(1 << t) & 0xff;
    }
    const refs           = [];
    for (let i = 0; i < (d1 & 7); i++) refs.push(get(size));
    if (p > end) throw new RangeError("BoC truncated");
    if (exotic && (len !== 264 || data[0] !== 2)) throw new RangeError(`exotic cell type ${data[0]} is refused: only a library cell (type 2) is allowed`);
    raw.push({ bits: data.subarray(0, Math.ceil(len / 8)), len, refs, exotic });
  }
  if (raw.length !== n) throw new RangeError("BoC cell count mismatch");
  if (hasCrc) {
    if (p + 4 > b.length) throw new RangeError("BoC truncated");
    const want = (b[p]  | (b[p + 1]  << 8) | (b[p + 2]  << 16) | (b[p + 3]  << 24)) >>> 0;
    if (crc32c(b.subarray(0, p)) !== want) throw new RangeError("BoC crc32c mismatch");
  }
  const built         = new Array(n);
  for (let i = n - 1; i >= 0; i--) {
    const r = raw[i] ;
    built[i] = new Cell(r.bits.slice(), r.len, r.refs.map((x) => {
      if (x <= i || x >= n) throw new RangeError("BoC refs not forward");
      return built[x] ;
    }), r.exotic);
  }
  return built[rootIdx] ;
}

export function stateInitCell(code      , data      )       {
  return beginCell().storeBit(0).storeBit(0).storeBit(1).storeRef(code).storeBit(1).storeRef(data).storeBit(0).endCell();
}

export function contractAddress(code      , data      , workchain = 0)          {
  return new Address(workchain, stateInitCell(code, data).hash());
}
