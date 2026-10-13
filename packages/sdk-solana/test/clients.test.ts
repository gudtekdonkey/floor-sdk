
import assert from 'node:assert/strict';

import { deploymentFor, floorHolders, floorHook, floorLaunch, floorVesting, IDLS } from '../src/index.ts';
import { test } from './node-test.ts';

test('every generated client names its IDL\'s program, which is the mainnet row\'s', () => {
  const row = deploymentFor('solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp');
  assert.equal(floorHook.FLOOR_HOOK_PROGRAM_ADDRESS, IDLS.floor_hook.address);
  assert.equal(floorLaunch.FLOOR_LAUNCH_PROGRAM_ADDRESS, IDLS.floor_launch.address);
  assert.equal(floorHolders.FLOOR_HOLDERS_PROGRAM_ADDRESS, IDLS.floor_holders.address);
  assert.equal(floorVesting.FLOOR_VESTING_PROGRAM_ADDRESS, IDLS.floor_vesting.address);
  assert.equal(floorHook.FLOOR_HOOK_PROGRAM_ADDRESS, row.programs.floorHook);
  assert.equal(floorLaunch.FLOOR_LAUNCH_PROGRAM_ADDRESS, row.programs.floorLaunch);
  assert.equal(floorHolders.FLOOR_HOLDERS_PROGRAM_ADDRESS, row.programs.floorHolders);
  assert.equal(floorVesting.FLOOR_VESTING_PROGRAM_ADDRESS, row.programs.floorVesting);
});

test('the one-prompt sender is @floorsupply/carpenter-sdk\'s, re-exported — not a second copy', async () => {
  const floor = await import('../src/index.ts');
  const carpenter = await import('@floorsupply/carpenter-sdk');
  assert.equal(floor.sendSolanaTransactions, carpenter.sendSolanaTransactions);
  assert.equal(floor.SolanaSendError, carpenter.SolanaSendError);
});
