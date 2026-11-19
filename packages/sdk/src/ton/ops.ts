

import { crc32 } from "./cell.ts";

export const STD_OP = {
  transfer: 0x0f8a7ea5,
  internal_transfer: 0x178d4519,
  transfer_notification: 0x7362d09c,
  excesses: 0xd53276db,
  burn: 0x595f07bc,
  burn_notification: 0x7bdd97de,
  provide_wallet_address: 0x2c76b973,
  take_wallet_address: 0xd1735400,
  provide_wallet_balance: 0x7ac8d559,
  take_wallet_balance: 0xca77fdc2,
} as const;

const memo = new Map<string, number>();

export function op(name: string): number {
  let v = memo.get(name);
  if (v === undefined) {
    v = crc32("floor." + name);
    memo.set(name, v);
  }
  return v;
}

export function logTag(name: string): number {
  const k = "log:" + name;
  let v = memo.get(k);
  if (v === undefined) {
    v = crc32("floor.log." + name);
    memo.set(k, v);
  }
  return v;
}
