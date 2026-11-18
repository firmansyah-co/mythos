// Every test runs without a network (frontend/test-setup.ts): a request nothing stubbed fails at
// once, and so does one made after a test's own stub was taken away.

import { afterEach, describe, expect, it, vi } from 'vitest';

describe('a test has no network', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('refuses a request nothing stubbed, at once', async () => {
    const started = Date.now();
    await expect(fetch('/api/v1/node')).rejects.toThrow('no network in a test');
    expect(Date.now() - started).toBeLessThan(1000);
  });

  it('a test that stubs fetch gets its stub', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"ok":true}')));
    expect(await (await fetch('/api/v1/node')).json()).toEqual({ ok: true });
  });
});
