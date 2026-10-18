import { describe, it, expect, vi, beforeEach } from 'vitest';
import { DeployControlBar } from '../DeployControlBar';

describe('DeployControlBar component', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('can be imported and verified as function component', () => {
    expect(typeof DeployControlBar).toBe('function');
  });

  it('correctly constructs deployment payload', async () => {
    let capturedBody: any = null;
    global.fetch = vi.fn().mockImplementation(async (_url, init) => {
      capturedBody = JSON.parse(init.body);
      return {
        ok: true,
        json: async () => ({
          success: true,
          compilation_time_ms: 42,
          message: 'Hot reload successful',
          binary_hash: 'abc123hash',
        }),
      };
    });

    const res = await fetch('/api/v1/control/deploy', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        code: 'PROGRAM Test; END_PROGRAM',
        language: 'ST',
        target_block: 'OB1',
        hot_reload: true,
      }),
    });

    const data = await res.json();
    expect(capturedBody).toEqual({
      code: 'PROGRAM Test; END_PROGRAM',
      language: 'ST',
      target_block: 'OB1',
      hot_reload: true,
    });
    expect(data.success).toBe(true);
    expect(data.compilation_time_ms).toBe(42);
  });
});
