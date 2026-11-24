

import { addressBytes, base58Encode } from "../../codec.js";
import { isOnCurve } from "../../ed25519.js";
import { findProgramAddress } from "../pda.js";

import { FLOOR_AMM_PROGRAM_ID } from "./program.js";

export const MINT_SEED = "mint";

const U64_MAX = (1n << 64n) - 1n;

function nonceBytes(nonce        )             {
  if (typeof nonce !== "bigint" || nonce < 0n || nonce > U64_MAX) throw new RangeError(`a mint nonce is a u64: ${String(nonce)}`);
  const b = new Uint8Array(8);
  let v = nonce;
  for (let i = 0; i < 8; i++) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

export const launchMintPda = (creator        , mintNonce        , p = FLOOR_AMM_PROGRAM_ID)      =>
  findProgramAddress([MINT_SEED, addressBytes(creator), nonceBytes(mintNonce)], p);

export const launchMintAddress = (creator        , mintNonce        , p = FLOOR_AMM_PROGRAM_ID)         => launchMintPda(creator, mintNonce, p)[0];

export function checkMintNonce(p                                                                                             )                 {
  const [mint, bump] = launchMintPda(p.creator, p.nonce, p.programId ?? FLOOR_AMM_PROGRAM_ID);
  return { mint, bump, firstTry: bump === 255, fits: p.fits ? p.fits(mint) : true };
}

const K = new Int32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
const IV = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
const MSG_LEN = 98;

const NONCE_WORD = 9;
const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const M4 = 58 ** 4;

const bswap = (x        )         => ((x << 24) | ((x & 0xff00) << 8) | ((x >>> 8) & 0xff00) | (x >>> 24)) | 0;

function expand(w            )       {
  for (let i = 16; i < 64; i++) {
    const a = w[i - 15], b = w[i - 2];
    const s0 = ((a >>> 7) | (a << 25)) ^ ((a >>> 18) | (a << 14)) ^ (a >>> 3);
    const s1 = ((b >>> 17) | (b << 15)) ^ ((b >>> 19) | (b << 13)) ^ (b >>> 10);
    w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
  }
}

function rounds(s            , kw            , from        )       {
  let a = s[0], b = s[1], c = s[2], d = s[3], e = s[4], f = s[5], g = s[6], h = s[7];
  for (let i = from; i < 64; i++) {
    const t1 = (h + (((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7))) + ((e & f) ^ (~e & g)) + kw[i]) | 0;
    const t2 = ((((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10))) + ((a & b) ^ (a & c) ^ (b & c))) | 0;
    h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
  }
  s[0] = a; s[1] = b; s[2] = c; s[3] = d; s[4] = e; s[5] = f; s[6] = g; s[7] = h;
}

class Kernel {
           w1 = new Int32Array(64);
           kw1 = new Int32Array(64);
           kw2 = new Int32Array(64);
           pre = new Int32Array(8);
           s = new Int32Array(8);
           h1 = new Int32Array(8);
           out = new Int32Array(8);

  constructor(creator        , programId        ) {
    const msg = new Uint8Array(128);
    msg.set(new TextEncoder().encode(MINT_SEED), 0);
    msg.set(addressBytes(creator), 4);
    msg[44] = 255;
    msg.set(addressBytes(programId), 45);
    msg.set(new TextEncoder().encode("ProgramDerivedAddress"), 77);
    msg[MSG_LEN] = 0x80;
    const bits = MSG_LEN * 8;
    msg[126] = bits >>> 8;
    msg[127] = bits & 0xff;
    const word = (o        ) => (msg[o] << 24) | (msg[o + 1] << 16) | (msg[o + 2] << 8) | msg[o + 3];
    for (let i = 0; i < 16; i++) this.w1[i] = word(4 * i);
    const w2 = new Int32Array(64);
    for (let i = 0; i < 16; i++) w2[i] = word(64 + 4 * i);
    expand(w2);
    for (let i = 0; i < 64; i++) this.kw2[i] = (K[i] + w2[i]) | 0;

    for (let i = 0; i < NONCE_WORD; i++) this.kw1[i] = (K[i] + this.w1[i]) | 0;
    const s = this.pre;
    for (let i = 0; i < 8; i++) s[i] = IV[i];
    let a = s[0], b = s[1], c = s[2], d = s[3], e = s[4], f = s[5], g = s[6], h = s[7];
    for (let i = 0; i < NONCE_WORD; i++) {
      const t1 = (h + (((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7))) + ((e & f) ^ (~e & g)) + this.kw1[i]) | 0;
      const t2 = ((((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10))) + ((a & b) ^ (a & c) ^ (b & c))) | 0;
      h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
    }
    s[0] = a; s[1] = b; s[2] = c; s[3] = d; s[4] = e; s[5] = f; s[6] = g; s[7] = h;
  }

  hash(lo        , hi        )             {
    const w = this.w1, kw = this.kw1, s = this.s, h1 = this.h1, out = this.out;
    w[NONCE_WORD] = bswap(lo);
    w[NONCE_WORD + 1] = bswap(hi);
    expand(w);
    for (let i = NONCE_WORD; i < 64; i++) kw[i] = (K[i] + w[i]) | 0;
    s.set(this.pre);
    rounds(s, kw, NONCE_WORD);
    for (let i = 0; i < 8; i++) h1[i] = s[i] = (IV[i] + s[i]) | 0;
    rounds(s, this.kw2, 0);
    for (let i = 0; i < 8; i++) out[i] = (h1[i] + s[i]) | 0;
    return out;
  }
}

function digestBytes(h            )             {
  const b = new Uint8Array(32);
  for (let i = 0; i < 8; i++) {
    b[4 * i] = h[i] >>> 24;
    b[4 * i + 1] = (h[i] >>> 16) & 0xff;
    b[4 * i + 2] = (h[i] >>> 8) & 0xff;
    b[4 * i + 3] = h[i] & 0xff;
  }
  return b;
}

function tail4(h            )         {
  let r = 0;
  for (let i = 0; i < 8; i++) {
    const x = h[i];
    r = (r * 65536 + (x >>> 16)) % M4;
    r = (r * 65536 + (x & 0xffff)) % M4;
  }
  return r;
}

function tailString(r        , n        )         {
  let t = "";
  for (let i = 0; i < n; i++) {
    t = ALPHABET[r % 58] + t;
    r = Math.floor(r / 58);
  }
  return t;
}

function randomNonce()         {
  const b = new Uint8Array(8);
  globalThis.crypto.getRandomValues(b);
  let v = 0n;
  for (let i = 7; i >= 0; i--) v = (v << 8n) | BigInt(b[i]);
  return v;
}

                                                     

const now = ()         => globalThis.performance?.now?.() ?? Date.now();
const macrotask = ()                => new Promise((r) => setTimeout(r, 0));

export async function grindMintNonce(o                       )                               {
  const programId = o.programId ?? FLOOR_AMM_PROGRAM_ID;
  const tail = o.tail;
  if (tail !== undefined && (!Number.isInteger(tail) || tail < 1 || tail > 4)) throw new RangeError(`tail is 1..4 characters: ${tail}`);
  const k = new Kernel(o.creator, programId);
  const sliceMs = o.sliceMs ?? 40;
  const yieldTo = o.yieldTo ?? macrotask;
  const t0 = now();
  const deadline = o.budgetMs === undefined ? Infinity : t0 + o.budgetMs;
  const start = o.start ?? randomNonce();
  nonceBytes(start);
  let lo = Number(start & 0xffffffffn) | 0;
  let hi = Number(start >> 32n) | 0;
  let tried = 0;
  const CHUNK = 2048;
  for (;;) {
    const sliceEnd = Math.min(now() + sliceMs, deadline);
    do {
      for (let n = 0; n < CHUNK; n++) {
        const h = k.hash(lo, hi);
        const nlo = lo, nhi = hi;
        lo = (lo + 1) | 0;
        if (lo === 0) hi = (hi + 1) | 0;
        if (tail !== undefined && !o.fits(tailString(tail4(h), tail))) continue;
        const bytes = digestBytes(h);
        const mint = base58Encode(bytes);
        if (!o.fits(mint) || isOnCurve(bytes)) continue;
        o.onTried?.(tried + n + 1);
        return { nonce: (BigInt(nhi >>> 0) << 32n) | BigInt(nlo >>> 0), mint };
      }
      tried += CHUNK;
    } while (now() < sliceEnd);
    o.onTried?.(tried);
    if (now() >= deadline || o.cancelled?.()) return null;
    await yieldTo();
    if (o.cancelled?.()) return null;
  }
}
