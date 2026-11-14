import { describe, expect, it } from 'vitest';
import pkg from '../../package.json';
import { APP_VERSION, normaliseChannel } from '../lib/appVersion';

describe('build identity', () => {
  it('takes the version from the package manifest', () => {
    expect(APP_VERSION).toBe(pkg.version);
  });

  it.each(['release', 'rc', 'dev'] as const)('recognises the %s channel', (channel) => {
    expect(normaliseChannel(channel)).toBe(channel);
  });

  it.each([undefined, '', 'DEV', 'nightly', 'unknown'])(
    'reports %o as unknown instead of guessing a channel',
    (raw) => {
      expect(normaliseChannel(raw)).toBe('unknown');
    }
  );
});
