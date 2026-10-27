// Identity of the running build: which version it is, and which channel it belongs to.
//
// VERSION comes from this package's manifest, which scripts/version.sh stamps from the release
// tag. It is imported rather than written as a literal: a hardcoded version is how this product
// ended up with no version anywhere, and an injected global that goes missing reports a
// plausible number instead of an absent one.
//
// CHANNEL is injected at build time by CI (VITE_APP_CHANNEL, from scripts/version.sh --channel)
// because it is a property of the event that produced the bundle, not of the source tree. When it
// is absent this reports `unknown` rather than guessing `dev`: a defaulted channel is the same
// fabrication shape as a defaulted sensor value - it looks like a measurement and is not one.

import pkg from '../../package.json';

export const APP_VERSION: string = pkg.version;

export type AppChannel = 'release' | 'rc' | 'dev' | 'unknown';

/**
 * Map an injected channel value to a known channel, or `unknown`.
 *
 * Pure and exported so it can be tested: `import.meta.env.VITE_*` is substituted at transform
 * time, so a test cannot inject a new value afterwards. The wiring is exercised in CI, where
 * VITE_APP_CHANNEL is actually set and the build logs the resolved channel.
 */
export function normaliseChannel(raw: string | undefined): AppChannel {
  return raw === 'release' || raw === 'rc' || raw === 'dev' ? raw : 'unknown';
}

const injected = (import.meta as ImportMeta & { env?: Record<string, string | undefined> }).env
  ?.VITE_APP_CHANNEL;

export const APP_CHANNEL: AppChannel = normaliseChannel(injected);
