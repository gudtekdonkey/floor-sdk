

export class V2MathError extends Error {
           code             ;

           stackIndex                    ;
  constructor(code             , stack         ) {
    super(`floor-math v2: ${code}${stack === undefined ? "" : `(${stack})`}`);
    this.name = "V2MathError";
    this.code = code;
    this.stackIndex = stack;
  }
}

export function v2Fail(code             , stack         )        {
  throw new V2MathError(code, stack);
}
