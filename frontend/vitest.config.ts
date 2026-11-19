import { defineConfig } from 'vitest/config';

// How much longer a test may take before it counts as hung. GitHub Actions sets CI. There the
// Rust and the frontend jobs run at once on one host's cores, with no limit between them: the
// same suite took 160 s beside the Rust job and 27 s alone (PR #156, 2026-11-19), and the first
// test to pass 20 s failed. Only the limits that catch a hang are stretched, never what a test
// asserts. MYTHOS_TEST_TIME_SCALE sets the factor by hand.
const timeScale = Number(process.env.MYTHOS_TEST_TIME_SCALE ?? (process.env.CI ? 6 : 1));

export default defineConfig({
  test: {
    testTimeout: 5_000 * timeScale,
    hookTimeout: 10_000 * timeScale,
    // Read by the tests that set a longer limit of their own (logic-ui's harness).
    env: { MYTHOS_TEST_TIME_SCALE: String(timeScale) },
    environment: 'happy-dom',
    include: ['packages/*/src/**/*.test.{ts,tsx}'],
    // No test reaches the network: see test-setup.ts.
    setupFiles: ['./test-setup.ts'],
  },
});
