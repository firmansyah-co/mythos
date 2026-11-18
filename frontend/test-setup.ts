// What every frontend test runs in: no network. A test stubs `fetch` with the controller it
// talks to; outside that stub there is none. Without this, a test that timed out goes on running
// after it is reported, its requests reach happy-dom's own origin (localhost:3000) for real, and
// on a loaded CI runner an IPv6 connection there can hang for minutes and fail every later test
// in the file with it.

import { afterEach, beforeEach, vi } from 'vitest';

const refusing = () =>
  vi.fn(async (input: RequestInfo | URL) => {
    throw new Error(`no network in a test: nothing stubbed fetch for ${String(input)}`);
  });

// Before a test's own hooks run, and again after them (setup hooks are registered first, so
// with the default stacked order this `afterEach` runs last).
beforeEach(() => {
  vi.stubGlobal('fetch', refusing());
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.stubGlobal('fetch', refusing());
});
