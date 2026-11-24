

export class ShapeError extends TypeError {
           path        ;
  constructor(path        , want        , got         ) {
    super(`${path || "body"}: want ${want}, got ${preview(got)}`);
    this.path = path;
  }
}

const preview = (v         )         => {
  if (v === undefined) return "nothing";
  const s = JSON.stringify(v) ?? String(v);
  return s.length > 60 ? `${s.slice(0, 57)}...` : s;
};

export const str                  = (v, p) => {
  if (typeof v !== "string") throw new ShapeError(p, "a string", v);
  return v;
};

export const num                  = (v, p) => {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new ShapeError(p, "a number", v);
  return v;
};

export const int                  = (v, p) => {
  const n = typeof v === "string" && /^-?\d+$/.test(v) ? Number(v) : v;
  if (typeof n !== "number" || !Number.isSafeInteger(n)) throw new ShapeError(p, "a safe integer", v);
  return n;
};

export const bool                   = (v, p) => {
  if (typeof v !== "boolean") throw new ShapeError(p, "a boolean", v);
  return v;
};

export const big                  = (v, p) => {
  if (typeof v === "string" && /^-?\d+$/.test(v)) return BigInt(v);
  if (typeof v === "number" && Number.isSafeInteger(v)) return BigInt(v);
  throw new ShapeError(p, "an integer string", v);
};

export const decimal                  = (v, p) => {
  if (typeof v !== "string" || !/^-?\d+(\.\d+)?$/.test(v)) throw new ShapeError(p, "a decimal string", v);
  return v;
};

export const unknown                   = (v) => v;

export function oneOf                                   (...values   )                     {
  return (v, p) => {
    if (typeof v !== "string" || !values.includes(v)) throw new ShapeError(p, values.map((x) => JSON.stringify(x)).join(" | "), v);
    return v             ;
  };
}

export function nullable   (d            )                    {
  return (v, p) => (v === null ? null : d(v, p));
}

export function optional   (d            )              {
  return Object.assign((v         , p        ) => (v === undefined ? undefined : d(v, p)), { optional: true          });
}

export function array   (d            )               {
  return (v, p) => {
    if (!Array.isArray(v)) throw new ShapeError(p, "an array", v);
    return v.map((x, i) => d(x, `${p}[${i}]`));
  };
}

export function record   (d            )                             {
  return (v, p) => {
    if (typeof v !== "object" || v === null || Array.isArray(v)) throw new ShapeError(p, "an object", v);
    const out                    = {};
    for (const [k, x] of Object.entries(v)) out[k] = d(x, `${p}.${k}`);
    return out;
  };
}

export function object                 (shape   )                       {
  return (v, p) => {
    if (typeof v !== "object" || v === null || Array.isArray(v)) throw new ShapeError(p, "an object", v);
    const o = v                           ;
    const out                          = {};
    for (const [k, d] of Object.entries(shape)) {
      const x = d(o[k], p === "" ? k : `${p}.${k}`);
      if (x !== undefined || !("optional" in d)) out[k] = x;
    }
    return out               ;
  };
}

export function decode   (d            , body         )    {
  return d(body, "");
}
