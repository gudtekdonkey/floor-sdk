

import { Address, beginCell, bytesOfBase64, fromBoc } from "./cell.js";

import { addr, accountAddress as deriveAccount } from "./addresses.js";

export const intItem = (value                           )            => ({ type: "int", value: BigInt(value) });

export const addressItem = (a                    )            => ({ type: "slice", cell: beginCell().storeAddress(a === null ? null : addr(a)).endCell() });

export class StackError extends Error {}

const int = (s             , i        , name        )         => {
  const x = s[i];
  if (!x || x.type !== "int") throw new StackError(`${name}: expected an int at ${i}`);
  return x.value;
};
const bool = (s             , i        , name        )          => int(s, i, name) !== 0n;
const cell = (s             , i        , name        )       => {
  const x = s[i];
  if (!x || (x.type !== "cell" && x.type !== "slice")) throw new StackError(`${name}: expected a cell at ${i}`);
  return x.cell;
};
const address = (s             , i        , name        )                 => {
  const x = s[i];
  if (!x) throw new StackError(`${name}: missing item ${i}`);
  if (x.type === "null") return null;
  if (x.type !== "slice" && x.type !== "cell") throw new StackError(`${name}: expected an address at ${i}`);
  return x.cell.beginParse().loadAddress();
};
const need = (s             , n        , name        )       => {
  if (s.length < n) throw new StackError(`${name}: ${s.length} stack items, expected ${n}`);
};

const parseNum = (v         )         => {
  const t = String(v);
  return t.startsWith("-") ? -BigInt(t.slice(1)) : BigInt(t);
};

export function parseToncenterStack(raw                    )              {
  return raw.map((e)            => {
    let type        ;
    let v     ;
    if (Array.isArray(e)) [type, v] = e                 ;
    else if (e && typeof e === "object" && typeof (e       )["@type"] === "string") {

      const t = (e       )["@type"]          ;
      if (t === "tvm.stackEntryNumber") { type = "num"; v = (e       ).number?.number ?? (e       ).number; }
      else if (t === "tvm.stackEntryCell") { type = "cell"; v = (e       ).cell; }
      else if (t === "tvm.stackEntrySlice") { type = "slice"; v = (e       ).slice; }
      else if (t === "tvm.stackEntryTuple") { type = "tuple"; v = (e       ).tuple; }
      else if (t === "tvm.stackEntryList") { type = "list"; v = (e       ).list; }
      else if (t === "tvm.stackEntryUnsupported") { type = "null"; v = null; }
      else throw new StackError(`unknown stack item type ${t}`);
    } else {
      type = (e       ).type;
      v = (e       ).value;
    }
    switch (type) {
      case "num":
      case "int":
        return { type: "int", value: parseNum(v) };
      case "cell":
        return { type: "cell", cell: fromBoc(bytesOfBase64(typeof v === "string" ? v : v.bytes)) };
      case "slice":
      case "cell_slice":
        return { type: "slice", cell: fromBoc(bytesOfBase64(typeof v === "string" ? v : v.bytes)) };
      case "null":
        return { type: "null" };
      case "tuple":
      case "list": {
        const els                     = v?.elements ?? v ?? [];

        if (els.length === 0) return { type: "null" };
        return { type: "tuple", items: parseToncenterStack(els) };
      }
      default:
        throw new StackError(`unknown stack item type ${type}`);
    }
  });
}

export function parseCoinsMap(root             , keyBits        )                      {
  const out = new Map                ();
  if (!root) return out;
  const walk = (c      , prefix        , prefixLen        )       => {
    const s = c.beginParse();
    const m = keyBits - prefixLen;

    let lbl = 0n;
    let len = 0;
    const lenBits = (n        )         => Math.ceil(Math.log2(n + 1));
    if (!s.loadBit()) {

      while (s.loadBit()) len++;
      lbl = len > 0 ? s.loadUint(len) : 0n;
    } else if (!s.loadBit()) {
      len = Number(s.loadUint(lenBits(m)));
      lbl = len > 0 ? s.loadUint(len) : 0n;
    } else {
      const bit = s.loadBit();
      len = Number(s.loadUint(lenBits(m)));
      lbl = bit ? (1n << BigInt(len)) - 1n : 0n;
    }
    const pfx = (prefix << BigInt(len)) | lbl;
    const plen = prefixLen + len;
    if (plen === keyBits) {
      out.set(pfx, s.loadCoins());
      return;
    }

    const l = s.loadRef();
    const r = s.loadRef();
    walk(l, pfx << 1n, plen + 1);
    walk(r, (pfx << 1n) | 1n, plen + 1);
  };
  walk(root, 0n, 0);
  return out;
}

function feesOf(s             , i        , name        )          {
  const x = s[i];
  if (!x || x.type !== "tuple" || x.items.length < 5) throw new StackError(`${name}: expected a 5-int tuple at ${i}`);
  const [backing, team, platform, floor, holder] = [0, 1, 2, 3, 4].map((k) => int(x.items, k, `${name}[${k}]`));
  return { backing: backing , team: team , platform: platform , floor: floor , holder: holder  };
}

async function run(rpc        , a             , method        , stack              = [])                       {
  return rpc.runGetMethod(addr(a), method, stack);
}

export async function getLaunchState(rpc        , launch             )                          {
  const s = await run(rpc, launch, "get_launch_state");
  need(s, 14, "get_launch_state");
  return {
    id: int(s, 0, "id"), factory: address(s, 1, "factory"), creator: address(s, 2, "creator"), quote: address(s, 3, "quote"),
    presetId: int(s, 4, "preset_id"), graduated: bool(s, 5, "phase"), supply: int(s, 6, "supply"), circulating: int(s, 7, "circulating"),
    dead: int(s, 8, "dead"), f: int(s, 9, "f"), c: int(s, 10, "c"), k: int(s, 11, "k"), moneyInPlace: int(s, 12, "money_in_place"),
    reserve: int(s, 13, "reserve"),
  };
}

export async function getLaunchBook(rpc        , launch             )                {
  const s = await run(rpc, launch, "get_launch_book");
  need(s, 1, "get_launch_book");
  return cell(s, 0, "book");
}
export async function getLaunchMilestones(rpc        , launch             )                {
  return cell(await run(rpc, launch, "get_launch_milestones"), 0, "milestones");
}
export async function getFeeCurve(rpc        , launch             )                {
  return cell(await run(rpc, launch, "get_fee_curve"), 0, "curve");
}
export async function getCirculating(rpc        , launch             )                  {
  return int(await run(rpc, launch, "get_circulating"), 0, "s_circ");
}
export async function getBidMoney(rpc        , launch             )                                                                     {
  const s = await run(rpc, launch, "get_bid_money");
  need(s, 4, "get_bid_money");
  return { f: int(s, 0, "f"), c: int(s, 1, "c"), k: int(s, 2, "k"), moneyInPlace: int(s, 3, "money_in_place") };
}
export async function getHolderBook(rpc        , launch             )                                                           {
  const s = await run(rpc, launch, "get_holder_book");
  need(s, 3, "get_holder_book");
  return { indexX64: int(s, 0, "index_x64"), w: int(s, 1, "w"), bucket: int(s, 2, "bucket") };
}
export async function getTeamShare(rpc        , launch             , place        )                                            {
  const s = await run(rpc, launch, "get_team_share", [intItem(place)]);
  need(s, 2, "get_team_share");
  return { owed: int(s, 0, "owed"), bought: int(s, 1, "bought") };
}
export async function getGasParams(rpc        , launch             )                        {
  const s = await run(rpc, launch, "get_gas_params");
  need(s, 4, "get_gas_params");
  return { buyFwdMin: int(s, 0, "buy_fwd_min"), sellFwdMin: int(s, 1, "sell_fwd_min"), refundFwdMin: int(s, 2, "refund_fwd_min"), reclaimAfter: int(s, 3, "reclaim_after") };
}
export async function getQuoteWallet(rpc        , launch             )                          {
  return address(await run(rpc, launch, "get_quote_wallet"), 0, "wallet");
}
export async function getQuoteBooks(rpc        , launch             )                                                                                                     {
  const s = await run(rpc, launch, "get_quote_books");
  need(s, 5, "get_quote_books");
  return { entry: cell(s, 0, "entry"), booked: int(s, 1, "booked"), syncedBalance: int(s, 2, "synced_balance"), syncedAt: int(s, 3, "synced_at"), surplus: int(s, 4, "surplus") };
}
export async function getWalletAddress(rpc        , master             , owner             )                          {
  return address(await run(rpc, master, "get_wallet_address", [addressItem(owner)]), 0, "wallet");
}
export async function getLaunchAddress(rpc        , factory             , id        )                          {
  return address(await run(rpc, factory, "get_launch_address", [intItem(id)]), 0, "launch");
}
export async function getAccountAddress(rpc        , launch             , owner             )                          {
  return address(await run(rpc, launch, "get_account_address", [addressItem(owner)]), 0, "account");
}
export async function getPosition(rpc        , wallet             )                       {
  const s = await run(rpc, wallet, "get_position");
  need(s, 5, "get_position");
  return { amount: int(s, 0, "amount"), owed: int(s, 1, "owed"), indexSnapX64: int(s, 2, "index_snap_x64"), optedIn: bool(s, 3, "opted_in"), dead: bool(s, 4, "dead") };
}
export async function getWalletData(rpc        , wallet             )                                                                              {
  const s = await run(rpc, wallet, "get_wallet_data");
  need(s, 3, "get_wallet_data");
  return { balance: int(s, 0, "balance"), owner: address(s, 1, "owner"), master: address(s, 2, "master") };
}

export async function simBuy(rpc        , launch             , money        , now        )                     {
  const s = await run(rpc, launch, "sim_buy", [intItem(money), intItem(now)]);
  need(s, 6, "sim_buy");
  return { tokens: int(s, 0, "tokens"), refund: int(s, 1, "refund"), fees: feesOf(s, 2, "fees"), steps: Number(int(s, 3, "steps")), budgetHit: bool(s, 4, "budget_hit"), errorClass: Number(int(s, 5, "class")) };
}
export async function simSell(rpc        , launch             , tokens        , now        )                      {
  const s = await run(rpc, launch, "sim_sell", [intItem(tokens), intItem(now)]);
  need(s, 3, "sim_sell");
  return { out: int(s, 0, "out"), fees: feesOf(s, 1, "fees"), errorClass: Number(int(s, 2, "class")) };
}

export async function quoteBuyMoney(rpc        , launch             , tokens        )                  {
  return int(await run(rpc, launch, "quote_buy", [intItem(tokens)]), 0, "money");
}
export async function quoteSellTokens(rpc        , launch             , out        )                  {
  return int(await run(rpc, launch, "quote_sell", [intItem(out)]), 0, "tokens");
}

export async function getOwed(rpc        , account             )                       {
  const s = await run(rpc, account, "get_owed");
  need(s, 3, "get_owed");
  const m = s[0];
  return {
    owed: parseCoinsMap(m && m.type !== "null" && m.type !== "int" ? (m                  ).cell : null, 16),
    teamBuyLockUntil: int(s, 1, "team_buy_lock_until"), teamBuyTokens: int(s, 2, "team_buy_tokens"),
  };
}

export async function getOwedOf(rpc        , chain                         , launch             , owner             )                       {
  return getOwed(rpc, deriveAccount(chain, launch, owner));
}
export async function getInFlight(rpc        , account             , payoutId        )                    {
  const s = await run(rpc, account, "get_in_flight", [intItem(payoutId)]);
  need(s, 4, "get_in_flight");
  return { quote: address(s, 0, "quote"), amount: int(s, 1, "amount"), bit: Number(int(s, 2, "bit")), sentAt: int(s, 3, "sent_at") };
}
export async function getConfig(rpc        , factory             )

   {
  const s = await run(rpc, factory, "get_config");
  need(s, 13, "get_config");
  return {
    authority: address(s, 0, "authority"), pendingAuthority: address(s, 1, "pending_authority"), timelock: address(s, 2, "timelock"), treasury: address(s, 3, "treasury"),
    createPaused: bool(s, 4, "create_paused"), nativeCreateFee: int(s, 5, "native_create_fee"), referralAuthority: int(s, 6, "referral_authority"),
    priceUpdater: int(s, 7, "price_updater"), holderDest: address(s, 8, "holder_dest"), metadataBase: s[9]?.type === "null" ? null : cell(s, 9, "metadata_base"),
    launchCount: int(s, 10, "launch_count"), nextPresetId: int(s, 11, "next_preset_id"), codeVersion: int(s, 12, "code_version"),
  };
}
export async function getQuote(rpc        , factory             , master                    )                                                                    {
  const s = await run(rpc, factory, "get_quote", [addressItem(master)]);
  need(s, 3, "get_quote");
  return { entry: cell(s, 0, "entry"), usdCentsX64: int(s, 1, "usd_cents_x64"), validUntil: int(s, 2, "valid_until") };
}

export async function getTonQuotePrice(rpc        , factory             , quote                    )                         {
  const s = await run(rpc, factory, "get_quote", [addressItem(quote)]);
  need(s, 3, "get_quote");
  const usdCentsX64 = int(s, 1, "usd_cents_x64");
  const validUntil = int(s, 2, "valid_until");
  if (s[0] .type === "null") return { listed: false, entry: null, usdCentsX64, validUntil };
  return { listed: true, entry: decodeTonQuoteEntry(cell(s, 0, "entry")), usdCentsX64, validUntil };
}

export function decodeTonQuoteEntry(c      )             {
  const s = c.beginParse();
  const kind = Number(s.loadUint(1))         ;
  const master = s.loadAddress();
  const decimals = Number(s.loadUint(8));
  const walletKind = Number(s.loadUint(8));
  const walletCodeHash = s.loadUint(256);
  const walletCode = s.loadMaybeRef();
  const createFee = s.loadCoins();
  const issuerCanFreeze = s.loadBit();
  const createsEnabled = s.loadBit();
  const cap = s.loadCoins();
  s.endParse();
  return { kind, master, decimals, walletKind, walletCodeHash, walletCode, createFee, issuerCanFreeze, createsEnabled, cap };
}

export function parseRefMap(root             , keyBits        )                    {
  const out = new Map              ();
  if (!root) return out;
  const lenBits = (n        )         => Math.ceil(Math.log2(n + 1));
  const walk = (c      , prefix        , prefixLen        )       => {
    const s = c.beginParse();
    const m = keyBits - prefixLen;
    let lbl = 0n;
    let len = 0;
    if (!s.loadBit()) {
      while (s.loadBit()) len++;
      lbl = len > 0 ? s.loadUint(len) : 0n;
    } else if (!s.loadBit()) {
      len = Number(s.loadUint(lenBits(m)));
      lbl = len > 0 ? s.loadUint(len) : 0n;
    } else {
      const bit = s.loadBit();
      len = Number(s.loadUint(lenBits(m)));
      lbl = bit ? (1n << BigInt(len)) - 1n : 0n;
    }
    const pfx = (prefix << BigInt(len)) | lbl;
    const plen = prefixLen + len;
    if (plen === keyBits) {
      out.set(pfx, s.loadRef());
      return;
    }
    const l = s.loadRef();
    const r = s.loadRef();
    walk(l, pfx << 1n, plen + 1);
    walk(r, (pfx << 1n) | 1n, plen + 1);
  };
  walk(root, 0n, 0);
  return out;
}

export function decodeTonSlotPlace(c      , place        )               {
  const s = c.beginParse();
  const member = s.loadAddress();
  const handle = s.loadBit() ? s.loadUint(256) : null;
  const shares = s.loadUint(64);
  const weightPpb = s.loadUint(32);
  const slotOwed = s.loadCoins();
  const boughtOwed = s.loadCoins();
  const teamBuyPart = s.loadCoins();
  s.endParse();
  return { place, member, handle, shares, weightPpb, slotOwed, boughtOwed, teamBuyPart };
}

export async function getTeamSlot(rpc        , launch             )                       {
  const s = await run(rpc, launch, "get_team_slot");
  need(s, 5, "get_team_slot");
  const d = s[4];
  const dict = d && d.type !== "null" && d.type !== "int" ? (d                  ).cell : null;
  const places = [...parseRefMap(dict, 4)].sort((a, b) => (a[0] < b[0] ? -1 : 1)).map(([k, c]) => decodeTonSlotPlace(c, Number(k)));
  return { slotBps: int(s, 0, "slot_bps"), count: Number(int(s, 1, "count")), shareTotal: int(s, 2, "share_total"), shareBuyDay: int(s, 3, "share_buy_day"), places };
}

export async function getPresetRow(rpc        , factory             , id                 )                            {
  const s = await run(rpc, factory, "get_preset_row", [intItem(id)]);
  need(s, 6, "get_preset_row");
  const r = s[0];
  const row = r && r.type !== "null" && r.type !== "int" ? (r                  ).cell : null;
  let rowBytes                    = null;
  if (row) {
    const parts               = [];
    for (let cur              = row; cur; ) {
      const sl = cur.beginParse();
      parts.push(sl.loadBytes(sl.remainingBits / 8));
      cur = sl.remainingRefs ? sl.loadRef() : null;
    }
    rowBytes = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
    let o = 0;
    for (const p of parts) (rowBytes.set(p, o), (o += p.length));
  }
  return {
    found: row !== null, row, rowBytes, enabled: bool(s, 1, "enabled"), deleted: bool(s, 2, "deleted"), checked: bool(s, 3, "checked"),
    version: int(s, 4, "version"), hash: int(s, 5, "hash"),
  };
}

export async function getTraderAddress(rpc        , factory             , trader             )                          {
  return address(await run(rpc, factory, "get_trader_address", [addressItem(trader)]), 0, "trader");
}

export async function getFamily(rpc        , factory             , family        )                                                                   {
  if (!Number.isInteger(family) || family < 0 || family > 4) throw new RangeError("family must be 0 to 4");
  const s = await run(rpc, factory, "get_family", [intItem(family)]);
  need(s, 3, "get_family");
  return { version: int(s, 0, "version"), initHash: int(s, 1, "init_hash"), codeHash: int(s, 2, "code_hash") };
}
export async function getTimelockInfo(rpc        , timelock             )                                                                             {
  const s = await run(rpc, timelock, "get_timelock");
  need(s, 3, "get_timelock");
  return { admin: address(s, 0, "admin"), factory: address(s, 1, "factory"), delay: int(s, 2, "delay") };
}
