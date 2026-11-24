

export { log2X64, exp2X64 } from "./fixed.js";

export const BPS = 10_000n;

                                         

export class FloorMathError extends Error {
           code               ;
  constructor(code               ) {
    super(`floor-math: ${code}`);
    this.name = "FloorMathError";
    this.code = code;
  }
}

export function fail(code               )        {
  throw new FloorMathError(code);
}
