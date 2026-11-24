

import { FloorError } from "../errors.js";
import { DEFAULT_SLIPPAGE_BPS } from "../client/types.js";
import { keccak256 } from "../evm/keccak.js";
import { bytesToHex, checksumAddress, decodeAbi, encodeAbi, hexToBytes } from "../evm/abi.js";
import { deadlineArg, floorAmmCall, floorTokenCall } from "../evm/floor/calls.js";
import { EVM_TOKEN_SCALE } from "./market-evm.js";

export { DEFAULT_SLIPPAGE_BPS };

export const MAX_SLIPPAGE_BPS = 10_000;
const BPS = 10_000n;

const MAX_UNIX_SECONDS = 100_000_000_000n;

const bad = (name        , message        )             => new FloorError("BadArgument", message, { name });

export function minOutAt(quoted        , slippageBps        )         {
  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > MAX_SLIPPAGE_BPS) {
    throw bad("BadSlippage", `slippageBps ${slippageBps} is not a whole number of bps in 0..=${MAX_SLIPPAGE_BPS}`);
  }
  if (quoted <= 0n) throw bad("BadQuote", `a quote of ${quoted} has no minimum (the NO_MINIMUM rule)`);
  const m = (quoted * (BPS - BigInt(slippageBps)) + BPS - 1n) / BPS;
  return m > 0n ? m : 1n;
}

const seconds = (v                 , what        )         => {
  if (typeof v === "number" && !Number.isFinite(v)) throw bad("BadDeadline", `${what} ${v} is not a time`);
  const s = typeof v === "bigint" ? v : BigInt(Math.floor(v));
  if (s < 0n || s > MAX_UNIX_SECONDS) throw bad("BadDeadline", `${what} ${v} is not unix seconds (a millisecond clock reads as no deadline)`);
  return s;
};

export function deadlineFrom(p                                                                                 )         {
  const now = seconds(p.nowSec, "nowSec");
  const head = p.headTs === undefined || p.headTs === null ? 0n : seconds(p.headTs, "headTs");
  if (!Number.isInteger(p.windowSec) || p.windowSec <= 0) throw bad("BadDeadline", `windowSec ${p.windowSec} is not whole seconds > 0`);
  return deadlineArg((now > head ? now : head) + BigInt(p.windowSec));
}

const utf8 = (s        )             => new TextEncoder().encode(s);
const PERMIT_TYPE = "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)";
const DOMAIN_TYPE = "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";

export const PERMIT_TYPEHASH = bytesToHex(keccak256(utf8(PERMIT_TYPE)));

export const EIP712_DOMAIN_TYPEHASH = bytesToHex(keccak256(utf8(DOMAIN_TYPE)));

export const permitDomain = (p                                                                         )               =>
  ({ name: p.tokenName, version: p.version ?? "1", chainId: p.chainId, verifyingContract: checksumAddress(p.token) });

function sellTokens(tokensIn        )         {
  const t = tokensIn - (tokensIn % EVM_TOKEN_SCALE);
  if (t <= 0n) throw new RangeError("tokensIn must be at least one TOKEN_SCALE (10^12)");
  return t;
}

export function sellPermitTypedData(p

 )                      {
  return {
    domain: permitDomain(p),
    types: {
      EIP712Domain: [{ name: "name", type: "string" }, { name: "version", type: "string" }, { name: "chainId", type: "uint256" }, { name: "verifyingContract", type: "address" }],
      Permit: [{ name: "owner", type: "address" }, { name: "spender", type: "address" }, { name: "value", type: "uint256" }, { name: "nonce", type: "uint256" }, { name: "deadline", type: "uint256" }],
    },
    primaryType: "Permit",
    message: { owner: checksumAddress(p.owner), spender: checksumAddress(p.floorAmm), value: sellTokens(p.tokensIn), nonce: p.nonce, deadline: deadlineArg(p.permitDeadline) },
  };
}

export const permitDomainSeparator = (d              )         =>
  bytesToHex(keccak256(encodeAbi(["bytes32", "bytes32", "bytes32", "uint256", "address"], [hexToBytes(EIP712_DOMAIN_TYPEHASH), keccak256(utf8(d.name)), keccak256(utf8(d.version)), BigInt(d.chainId), d.verifyingContract])));

export function permitDigest(td                     )         {
  const m = td.message;
  const struct = keccak256(encodeAbi(["bytes32", "address", "address", "uint256", "uint256", "uint256"], [hexToBytes(PERMIT_TYPEHASH), m.owner, m.spender, m.value, m.nonce, m.deadline]));
  return bytesToHex(keccak256(new Uint8Array([0x19, 0x01, ...hexToBytes(permitDomainSeparator(td.domain)), ...struct])));
}

export function splitSignature(sig        )                                      {
  const b = hexToBytes(sig);
  if (b.length !== 65) throw new RangeError(`a signature is 65 bytes, not ${b.length}`);
  const v = b[64] < 27 ? b[64] + 27 : b[64];
  if (v !== 27 && v !== 28) throw new RangeError(`signature v ${b[64]} is not 27 / 28 (or 0 / 1)`);
  return { r: bytesToHex(b.slice(0, 32)), s: bytesToHex(b.slice(32, 64)), v };
}

export function sellWithPermitCall(p

 )          {
  if (p.minQuoteOut <= 0n) throw new RangeError("minQuoteOut must be > 0 (the NO_MINIMUM rule)");
  const { r, s, v } = splitSignature(p.signature);
  return floorAmmCall(p.floorAmm, "sellWithPermit", {
    launchId: p.launchId, tokensIn: sellTokens(p.tokensIn), minQuoteOut: p.minQuoteOut, to: p.to, deadline: deadlineArg(p.deadline),
    permitDeadline: deadlineArg(p.permitDeadline), v: BigInt(v), r, s,
  });
}

export function sellPlanOf(p                                         )                    {
  return p.allowance >= sellTokens(p.tokensIn) ? "sell" : "permit";
}

export const permitNonceCall = (token        , owner        )          => floorTokenCall(token, "nonces", { owner });
export const decodePermitNonce = (data        )         => decodeAbi(["uint256"], data)[0]          ;
