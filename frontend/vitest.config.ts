import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'happy-dom',
    include: ['packages/*/src/**/*.test.{ts,tsx}'],
    // No test reaches the network: see test-setup.ts.
    setupFiles: ['./test-setup.ts'],
  },
});
