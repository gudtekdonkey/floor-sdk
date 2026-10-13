import { describe, expect, it } from 'vitest';
import { encodeAbiParameters, encodeEventTopics, toEventSelector, type AbiEvent, type Hex } from 'viem';
import * as abi from '../src/abi/index.js';
import { FLOOR_EVENTS, decodeFloorLog, decodeFloorLogs, floorEventTopics } from '../src/events.js';

const events = (Object.entries(abi) as [string, unknown][])
  .filter(([, v]) => Array.isArray(v))
  .flatMap(([name, v]) => (v as readonly { type: string }[]).filter((i) => i.type === 'event').map((e) => [name, e as AbiEvent] as const));

const find = (list: readonly unknown[], name: string) =>
  (list as readonly { type: string; name?: string }[]).find((x) => x.type === 'event' && x.name === name) as AbiEvent;

const BUYER = '0x1111111111111111111111111111111111111111';
const TOKEN = '0x2222222222222222222222222222222222222222';
const HOOK = '0x3333333333333333333333333333333333333333';
const LAUNCH = '0x4444444444444444444444444444444444444444';

function logOf(event: AbiEvent, values: Record<string, unknown>): { topics: Hex[]; data: Hex } {
  const indexed = event.inputs.filter((i) => i.indexed);
  const plain = event.inputs.filter((i) => !i.indexed);
  const topics = encodeEventTopics({ abi: [event], eventName: event.name, args: Object.fromEntries(indexed.map((i) => [i.name, values[i.name!]])) } as never) as Hex[];
  const data = encodeAbiParameters(plain, plain.map((i) => values[i.name!]));
  return { topics, data };
}

describe('FLOOR event decoders', () => {
  it('every event in every ABI is in the table, under its own selector', () => {
    for (const [name, e] of events) {
      const info = FLOOR_EVENTS.get(toEventSelector(e));
      expect(info, `${name}.${e.name}`).toBeDefined();
      expect(info!.abis).toContain(name);
    }
  });

  it('no selector names two different layouts: one table can decode any log', () => {
    const layout = (e: AbiEvent) => e.inputs.map((i) => `${i.type}${i.indexed ? '!' : ''}`).join(',');
    for (const [, e] of events) expect(layout(FLOOR_EVENTS.get(toEventSelector(e))!.event)).toBe(layout(e));
  });

  it('decodes a sale buy with its log metadata, and LaunchCreated by its indexed and plain fields', () => {
    const bought = find(abi.LAUNCH_ABI, 'Bought');
    const log = { ...logOf(bought, { who: BUYER, tokens: 5n, ethSpent: 777n, soldTotal: 10n, raisedTotal: 20n }), address: LAUNCH as Hex, logIndex: 3 };
    const d = decodeFloorLog(log)!;
    expect(d.name).toBe('Bought');
    expect(d.args).toMatchObject({ who: BUYER, tokens: 5n, ethSpent: 777n, soldTotal: 10n, raisedTotal: 20n });
    expect(d.abis).toContain('LAUNCH_ABI');
    expect(d.address).toBe(LAUNCH);
    expect(d.logIndex).toBe(3);

    const created = find(abi.FACTORY_ABI, 'LaunchCreated');
    const c = decodeFloorLog(logOf(created, {
      token: TOKEN, hook: HOOK, launch: LAUNCH, rewards: BUYER, poolId: `0x${'ab'.repeat(32)}`, mode: 2,
      creator: BUYER, graduationMcapUsd: 69_000n, ethUsd: 2400n,
    }))!;
    expect(c.name).toBe('LaunchCreated');
    expect((c.args.token as string).toLowerCase()).toBe(TOKEN);
    expect(c.args.mode).toBe(2);
  });

  it('refuses a log it cannot read: an unknown selector, no topics, or data that does not fit the layout', () => {
    const bought = find(abi.LAUNCH_ABI, 'Bought');
    const good = logOf(bought, { who: BUYER, tokens: 1n, ethSpent: 1n, soldTotal: 1n, raisedTotal: 1n });
    expect(decodeFloorLog({ topics: [`0x${'00'.repeat(32)}`], data: '0x' })).toBeNull();
    expect(decodeFloorLog({ topics: [], data: '0x' })).toBeNull();
    expect(decodeFloorLog({ topics: good.topics, data: good.data.slice(0, 66) as Hex })).toBeNull();
    expect(decodeFloorLog({ topics: good.topics.slice(0, 1), data: good.data })).toBeNull();
  });

  it('decodes a receipt in order and skips logs that are not FLOOR events', () => {
    const bought = find(abi.LAUNCH_ABI, 'Bought');
    const a = logOf(bought, { who: BUYER, tokens: 1n, ethSpent: 2n, soldTotal: 3n, raisedTotal: 4n });
    const out = decodeFloorLogs([a, { topics: [`0x${'11'.repeat(32)}`], data: '0x' }, a]);
    expect(out.map((e) => e.name)).toEqual(['Bought', 'Bought']);
  });

  it('floorEventTopics finds an event by name, both selectors of a name declared twice, and nothing for an unknown name', () => {
    expect(floorEventTopics('Bought')).toEqual([toEventSelector(find(abi.LAUNCH_ABI, 'Bought'))]);
    expect(floorEventTopics('Claimed').length).toBe(2);
    expect(floorEventTopics('NoSuchEvent')).toEqual([]);
  });
});
