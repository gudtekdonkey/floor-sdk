

import { bytesToHex, checksumAddress, encodeAbi, hexToBytes } from "./abi.js";
import { encodeFunctionData } from "./items.js";

export const UR_V3_SWAP_EXACT_IN = 0x00;
export const UR_V2_SWAP_EXACT_IN = 0x08;
export const UR_WRAP_ETH = 0x0b;
export const UR_UNWRAP_WETH = 0x0c;
export const UR_SWEEP = 0x04;
export const UR_MSG_SENDER = "0x0000000000000000000000000000000000000001";
export const UR_ADDRESS_THIS = "0x0000000000000000000000000000000000000002";

const fn = (name        , inputs                                                             ) => ({ type: "function", name, inputs, outputs: [], stateMutability: "payable" })         ;
const tup = (fields                    ) => fields.map(([name, type]) => ({ name, type }));

export function v3Path(tokens                   , fees                   )         {
  if (tokens.length < 2 || fees.length !== tokens.length - 1) throw new RangeError("a V3 path is n tokens and n − 1 fees");
  let out = "0x";
  tokens.forEach((t, i) => {
    out += bytesToHex(hexToBytes(t)).slice(2);
    if (i < fees.length) {
      const f = fees[i];
      if (!Number.isInteger(f) || f < 0 || f >= 1 << 24) throw new RangeError(`fee ${f} is not a uint24`);
      out += f.toString(16).padStart(6, "0");
    }
  });
  return out;
}

export function algebraPath(tokens                   )         {
  if (tokens.length < 2) throw new RangeError("a path is at least two tokens");
  return "0x" + tokens.map((t) => bytesToHex(hexToBytes(t)).slice(2)).join("");
}

const checkRoute = (p               )       => {
  if (p.amountIn <= 0n) throw new RangeError("amountIn must be > 0");
  if (p.minQuoteOut <= 0n) throw new RangeError("minQuoteOut must be > 0 (the NO_MINIMUM rule)");
  if (p.route.tokens.length < 2) throw new RangeError("a route is at least two tokens");
};

function universalRouter(p               )         {
  const wrap = bytesToHex(encodeAbi(["address", "uint256"], [UR_ADDRESS_THIS, p.amountIn]));
  let cmd        ;
  let swap        ;
  if (p.route.fees) {
    cmd = UR_V3_SWAP_EXACT_IN;
    swap = bytesToHex(encodeAbi(["address", "uint256", "uint256", "bytes", "bool", "uint256[]"], [UR_MSG_SENDER, p.amountIn, p.minQuoteOut, v3Path(p.route.tokens, p.route.fees), false, []]));
  } else {
    cmd = UR_V2_SWAP_EXACT_IN;
    swap = bytesToHex(encodeAbi(["address", "uint256", "uint256", "address[]", "bool", "uint256[]"], [UR_MSG_SENDER, p.amountIn, p.minQuoteOut, [...p.route.tokens], false, []]));
  }
  const commands = "0x" + [UR_WRAP_ETH, cmd].map((c) => c.toString(16).padStart(2, "0")).join("");
  const abi = [fn("execute", [{ name: "commands", type: "bytes" }, { name: "inputs", type: "bytes[]" }, { name: "deadline", type: "uint256" }])];
  return encodeFunctionData(abi, "execute", { commands, inputs: [wrap, swap], deadline: p.deadline });
}

const REFUND_ETH = [fn("refundETH", [])];
const REFUND_NATIVE = [fn("refundNativeToken", [])];

function v3Router(p               , v02         )         {
  const fees = p.route.fees;
  if (!fees) throw new RangeError("a V3 router route needs its fees");
  const [tokenIn, tokenOut] = [p.route.tokens[0], p.route.tokens[p.route.tokens.length - 1]];
  let swap        ;
  if (p.route.tokens.length === 2) {
    const fields                     = v02
      ? [["tokenIn", "address"], ["tokenOut", "address"], ["fee", "uint24"], ["recipient", "address"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"], ["sqrtPriceLimitX96", "uint160"]]
      : [["tokenIn", "address"], ["tokenOut", "address"], ["fee", "uint24"], ["recipient", "address"], ["deadline", "uint256"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"], ["sqrtPriceLimitX96", "uint160"]];
    const abi = [fn("exactInputSingle", [{ name: "params", type: "tuple", components: tup(fields) }])];
    swap = encodeFunctionData(abi, "exactInputSingle", { params: { tokenIn, tokenOut, fee: BigInt(fees[0]), recipient: p.zap, deadline: p.deadline, amountIn: p.amountIn, amountOutMinimum: p.minQuoteOut, sqrtPriceLimitX96: 0n } });
  } else {
    const fields                     = v02
      ? [["path", "bytes"], ["recipient", "address"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"]]
      : [["path", "bytes"], ["recipient", "address"], ["deadline", "uint256"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"]];
    const abi = [fn("exactInput", [{ name: "params", type: "tuple", components: tup(fields) }])];
    swap = encodeFunctionData(abi, "exactInput", { params: { path: v3Path(p.route.tokens, fees), recipient: p.zap, deadline: p.deadline, amountIn: p.amountIn, amountOutMinimum: p.minQuoteOut } });
  }
  const refund = encodeFunctionData(REFUND_ETH, "refundETH");
  if (v02) return encodeFunctionData([fn("multicall", [{ name: "deadline", type: "uint256" }, { name: "data", type: "bytes[]" }])], "multicall", { deadline: p.deadline, data: [swap, refund] });
  return encodeFunctionData([fn("multicall", [{ name: "data", type: "bytes[]" }])], "multicall", { data: [swap, refund] });
}

function algebra(p               )         {
  const [tokenIn, tokenOut] = [p.route.tokens[0], p.route.tokens[p.route.tokens.length - 1]];
  let swap        ;
  if (p.route.tokens.length === 2) {
    const fields                     = [["tokenIn", "address"], ["tokenOut", "address"], ["recipient", "address"], ["deadline", "uint256"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"], ["limitSqrtPrice", "uint160"]];
    swap = encodeFunctionData([fn("exactInputSingle", [{ name: "params", type: "tuple", components: tup(fields) }])], "exactInputSingle", { params: { tokenIn, tokenOut, recipient: p.zap, deadline: p.deadline, amountIn: p.amountIn, amountOutMinimum: p.minQuoteOut, limitSqrtPrice: 0n } });
  } else {
    const fields                     = [["path", "bytes"], ["recipient", "address"], ["deadline", "uint256"], ["amountIn", "uint256"], ["amountOutMinimum", "uint256"]];
    swap = encodeFunctionData([fn("exactInput", [{ name: "params", type: "tuple", components: tup(fields) }])], "exactInput", { params: { path: algebraPath(p.route.tokens), recipient: p.zap, deadline: p.deadline, amountIn: p.amountIn, amountOutMinimum: p.minQuoteOut } });
  }
  return encodeFunctionData([fn("multicall", [{ name: "data", type: "bytes[]" }])], "multicall", { data: [swap, encodeFunctionData(REFUND_NATIVE, "refundNativeToken")] });
}

function v2Router(p               )         {
  const abi = [fn("swapExactETHForTokens", tup([["amountOutMin", "uint256"], ["path", "address[]"], ["to", "address"], ["deadline", "uint256"]]))];
  return encodeFunctionData(abi, "swapExactETHForTokens", { amountOutMin: p.minQuoteOut, path: [...p.route.tokens], to: p.zap, deadline: p.deadline });
}

function aerodromeV2(p               )         {
  const { tokens, stable, factory } = p.route;
  if (!stable || stable.length !== tokens.length - 1 || !factory) throw new RangeError("an Aerodrome route needs a stable flag per hop and the factory");
  const route = { name: "routes", type: "tuple[]", components: tup([["from", "address"], ["to", "address"], ["stable", "bool"], ["factory", "address"]]) };
  const abi = [fn("swapExactETHForTokens", [{ name: "amountOutMin", type: "uint256" }, route, { name: "to", type: "address" }, { name: "deadline", type: "uint256" }])];
  const routes = stable.map((s, i) => ({ from: tokens[i], to: tokens[i + 1], stable: s, factory }));
  return encodeFunctionData(abi, "swapExactETHForTokens", { amountOutMin: p.minQuoteOut, routes, to: p.zap, deadline: p.deadline });
}

export function zapSwapData(p               )         {
  checkRoute(p);
  checksumAddress(p.zap);
  switch (p.family) {
    case "uniswap-ur":
      return universalRouter(p);
    case "v3-router":
      return v3Router(p, false);
    case "v3-router02":
      return v3Router(p, true);
    case "algebra":
      return algebra(p);
    case "v2-router":
      return v2Router(p);
    case "aerodrome-v2":
      return aerodromeV2(p);
  }
}
