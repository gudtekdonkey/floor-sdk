

import { bytesToHex, decodeAbi, encodeAbi, functionSelector, hexToBytes } from "../vendor/sdk/src/evm/abi.ts";
import type { AbiValue } from "../vendor/sdk/src/evm/abi.ts";
import { FN, OUT } from "../src/abi.ts";

export const AMM = "0x1000000000000000000000000000000000000f10";

export type MockLaunch = {
  id: bigint;
  token: string;
  quote: string;
  quoteUsdCentsX64: bigint;
  createdBlock: number;

  symbol?: string;

  listed?: boolean;
};

export class MockChain {
  readonly launches = new Map<bigint, MockLaunch>();
  block = 0;

  readonly listedQuotes = new Set<string>();

  add(m: MockLaunch): void {
    this.launches.set(m.id, m);
    if (m.listed !== false) this.listedQuotes.add(m.quote.toLowerCase());
  }

  async call(tx: { to: string; data: string }, _blockTag?: number | "latest"): Promise<string> {
    const sel = tx.data.slice(0, 10);
    const args = (types: string[]): AbiValue[] => decodeAbi(types, hexToBytes(`0x${tx.data.slice(10)}`));
    const is = (sig: string): boolean => sel === bytesToHex(functionSelector(sig));
    const ret = (types: readonly string[], v: AbiValue[]): string => bytesToHex(encodeAbi(types, v));
    if (tx.to.toLowerCase() !== AMM.toLowerCase()) {
      const m = [...this.launches.values()].find((x) => x.token.toLowerCase() === tx.to.toLowerCase());
      if (m && is(FN.name)) return ret(OUT.name, [`Launch ${m.id}`]);
      if (m && is(FN.symbol)) return ret(OUT.symbol, [m.symbol ?? `L${m.id}`]);
      throw new Error(`mock: no ${sel} at ${tx.to}`);
    }
    if (is(FN.quoteEntry)) {
      const listed = this.listedQuotes.has((args(["address"])[0] as string).toLowerCase());
      return ret(OUT.quoteEntry, [[listed, listed, 18n, 0n, 0n, 0n, 0n]]);
    }
    throw new Error(`mock: unknown selector ${sel}`);
  }
}
