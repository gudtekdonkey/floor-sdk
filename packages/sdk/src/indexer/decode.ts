

export class ShapeError extends TypeError {
  readonly path: string;
  constructor(path: string, want: string, got: unknown) {
    super(`${path || "body"}: want ${want}, got ${preview(got)}`);
    this.path = path;
  }
}

const preview = (v: unknown): string => {
  if (v === undefined) return "nothing";
  const s = JSON.stringify(v) ?? String(v);
  return s.length > 60 ? `${s.slice(0, 57)}...` : s;
};

export type Decoder<T> = (v: unknown, path: string) => T;

export type Optional<T> = Decoder<T | undefined> & { readonly optional: true };

export type Infer<D> = D extends Decoder<infer T> ? T : never;

type Shape = Record<string, Decoder<unknown>>;
type OptionalKeys<S extends Shape> = { [K in keyof S]: S[K] extends { readonly optional: true } ? K : never }[keyof S];
type Flat<T> = { [K in keyof T]: T[K] };

export type ObjectOf<S extends Shape> = Flat<{ [K in Exclude<keyof S, OptionalKeys<S>>]: Infer<S[K]> } & { [K in OptionalKeys<S>]?: Exclude<Infer<S[K]>, undefined> }>;

export const str: Decoder<string> = (v, p) => {
  if (typeof v !== "string") throw new ShapeError(p, "a string", v);
  return v;
};

export const num: Decoder<number> = (v, p) => {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new ShapeError(p, "a number", v);
  return v;
};

export const int: Decoder<number> = (v, p) => {
  const n = typeof v === "string" && /^-?\d+$/.test(v) ? Number(v) : v;
  if (typeof n !== "number" || !Number.isSafeInteger(n)) throw new ShapeError(p, "a safe integer", v);
  return n;
};

export const bool: Decoder<boolean> = (v, p) => {
  if (typeof v !== "boolean") throw new ShapeError(p, "a boolean", v);
  return v;
};

export const big: Decoder<bigint> = (v, p) => {
  if (typeof v === "string" && /^-?\d+$/.test(v)) return BigInt(v);
  if (typeof v === "number" && Number.isSafeInteger(v)) return BigInt(v);
  throw new ShapeError(p, "an integer string", v);
};

export const decimal: Decoder<string> = (v, p) => {
  if (typeof v !== "string" || !/^-?\d+(\.\d+)?$/.test(v)) throw new ShapeError(p, "a decimal string", v);
  return v;
};

export const unknown: Decoder<unknown> = (v) => v;

export function oneOf<const T extends readonly string[]>(...values: T): Decoder<T[number]> {
  return (v, p) => {
    if (typeof v !== "string" || !values.includes(v)) throw new ShapeError(p, values.map((x) => JSON.stringify(x)).join(" | "), v);
    return v as T[number];
  };
}

export function nullable<T>(d: Decoder<T>): Decoder<T | null> {
  return (v, p) => (v === null ? null : d(v, p));
}

export function optional<T>(d: Decoder<T>): Optional<T> {
  return Object.assign((v: unknown, p: string) => (v === undefined ? undefined : d(v, p)), { optional: true as const });
}

export function array<T>(d: Decoder<T>): Decoder<T[]> {
  return (v, p) => {
    if (!Array.isArray(v)) throw new ShapeError(p, "an array", v);
    return v.map((x, i) => d(x, `${p}[${i}]`));
  };
}

export function record<T>(d: Decoder<T>): Decoder<Record<string, T>> {
  return (v, p) => {
    if (typeof v !== "object" || v === null || Array.isArray(v)) throw new ShapeError(p, "an object", v);
    const out: Record<string, T> = {};
    for (const [k, x] of Object.entries(v)) out[k] = d(x, `${p}.${k}`);
    return out;
  };
}

export function object<S extends Shape>(shape: S): Decoder<ObjectOf<S>> {
  return (v, p) => {
    if (typeof v !== "object" || v === null || Array.isArray(v)) throw new ShapeError(p, "an object", v);
    const o = v as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const [k, d] of Object.entries(shape)) {
      const x = d(o[k], p === "" ? k : `${p}.${k}`);
      if (x !== undefined || !("optional" in d)) out[k] = x;
    }
    return out as ObjectOf<S>;
  };
}

export function decode<T>(d: Decoder<T>, body: unknown): T {
  return d(body, "");
}
