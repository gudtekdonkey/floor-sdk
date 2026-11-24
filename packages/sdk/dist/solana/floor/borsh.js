

import { addressBytes, base58Encode } from "../../codec.js";

export class Writer {
          parts           = [];
  uint(v                 , n        )       {
    let x = BigInt(v);
    if (x < 0n || x >= 1n << BigInt(8 * n)) throw new RangeError(`a ${8 * n}-bit value out of range: ${v}`);
    for (let i = 0; i < n; i++) {
      this.parts.push(Number(x & 0xffn));
      x >>= 8n;
    }
    return this;
  }
  u8 = (v        )       => this.uint(v, 1);
  u16 = (v        )       => this.uint(v, 2);
  u32 = (v        )       => this.uint(v, 4);
  u64 = (v        )       => this.uint(v, 8);
  u128 = (v        )       => this.uint(v, 16);
  bool = (v         )       => this.u8(v ? 1 : 0);
  pubkey = (a        )       => this.bytes(addressBytes(a));
  bytes(b            )       {
    for (const x of b) this.parts.push(x);
    return this;
  }
  string(s        )       {
    const b = new TextEncoder().encode(s);
    return this.u32(b.length).bytes(b);
  }
  done()             {
    return Uint8Array.from(this.parts);
  }
}

export class Reader {
  o = 0;
           b            ;
  constructor(b            ) {
    this.b = b;
  }
  uint(n        )         {
    if (this.o + n > this.b.length) throw new RangeError("borsh: data too short");
    let v = 0n;
    for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(this.b[this.o + i]);
    this.o += n;
    return v;
  }
  u8 = ()         => Number(this.uint(1));
  u16 = ()         => Number(this.uint(2));
  u32 = ()         => Number(this.uint(4));
  u64 = ()         => this.uint(8);
  i64 = ()         => {
    const v = this.uint(8);
    return v >= 1n << 63n ? v - (1n << 64n) : v;
  };
  u128 = ()         => this.uint(16);
  bool = ()          => this.u8() !== 0;
  bytes(n        )             {
    if (this.o + n > this.b.length) throw new RangeError("borsh: data too short");
    const r = this.b.slice(this.o, this.o + n);
    this.o += n;
    return r;
  }
  pubkey = ()         => base58Encode(this.bytes(32));
}
