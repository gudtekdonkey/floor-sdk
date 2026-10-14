import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vitest/config';

// During development `@floorlaunchpad/carpenter-sdk` is a `file:` link to the sibling checkout and `@floorlaunchpad/sdk` a workspace sibling:
// both resolve to their SOURCE here, so a test never runs against a stale `dist/`.
export default defineConfig({
  resolve: {
    alias: {
      '@floorlaunchpad/carpenter-sdk': fileURLToPath(new URL('../../../carpenter-sdk/packages/sdk/src/index.ts', import.meta.url)),
      '@floorlaunchpad/sdk': fileURLToPath(new URL('../sdk/src/index.ts', import.meta.url)),
    },
  },
  test: {
    include: ['test/**/*.test.ts'],
    testTimeout: 120_000,
  },
});
