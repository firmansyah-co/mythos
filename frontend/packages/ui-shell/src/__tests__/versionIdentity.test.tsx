import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { HmiScreenCanvasPanel } from '../components/workbench/DockviewWorkspace';
import { APP_CHANNEL, APP_VERSION, normaliseChannel } from '../lib/appVersion';
import { useHmiStore } from '../stores/useHmiStore';

// The expectation is the manifest that scripts/version.sh stamps from the release tag — never a
// literal. A test asserting "0.1.0" passes forever and proves nothing, and that is precisely how
// this product reached a state with no version anywhere.
const pkg = JSON.parse(
  readFileSync(path.resolve(process.cwd(), 'packages/ui-shell/package.json'), 'utf-8')
) as { version: string };

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('Version identity is visible and sourced, not hardcoded', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('reports a version rather than a sentinel or a literal', () => {
    // APP_VERSION must be the manifest value. This fails if someone replaces the manifest import
    // with a literal (the drift this pair of tests exists to stop) or drops it for a sentinel.
    expect(APP_VERSION).toBe(pkg.version);
    expect(APP_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('renders the version on a surface the operator can read', () => {
    const html = renderClient(<HmiScreenCanvasPanel params={{}} /> as any);
    const el = container.querySelector('[data-testid="app-version"]');

    expect(el).not.toBeNull();
    expect(el?.textContent?.replace(/\s+/g, '')).toContain(`v${pkg.version}`);
    expect(html).toContain(pkg.version);
  });

  it('renders the channel beside the version', () => {
    const html = renderClient(<HmiScreenCanvasPanel params={{}} /> as any);
    const el = container.querySelector('[data-testid="app-version"]');

    expect(el?.textContent).toContain(APP_CHANNEL);
    expect(html).toContain(APP_CHANNEL);
  });

  it('reports the injected channel, and invents none when nothing is injected', () => {
    // This test must hold in EVERY environment the job runs in. CI exports VITE_APP_CHANNEL
    // before this step on dev/rc/release pushes, so an unconditional `expect('unknown')` passed
    // only on pull requests and reddened every push to dev - the failing path was never
    // exercised because both runs at the time were event:pull_request.
    const injected = (import.meta as ImportMeta & { env?: Record<string, string | undefined> })
      .env?.VITE_APP_CHANNEL;

    // The contract: whatever was injected is mapped, never replaced by a convenience default.
    expect(APP_CHANNEL).toBe(normaliseChannel(injected));

    if (injected === 'release' || injected === 'rc' || injected === 'dev') {
      expect(APP_CHANNEL).toBe(injected);
    } else {
      // No channel injected: unknown, never a silent 'dev'.
      expect(APP_CHANNEL).toBe('unknown');
    }
  });

  it('maps injected channel values and refuses an unrecognised one', () => {
    // `import.meta.env.VITE_*` is substituted at transform time, so a test cannot inject a value
    // after the module loads; the mapping is tested directly and the wiring is exercised in CI,
    // which sets VITE_APP_CHANNEL and logs the resolved channel.
    expect(normaliseChannel('release')).toBe('release');
    expect(normaliseChannel('rc')).toBe('rc');
    expect(normaliseChannel('dev')).toBe('dev');
    expect(normaliseChannel(undefined)).toBe('unknown');
    expect(normaliseChannel('')).toBe('unknown');
    expect(normaliseChannel('staging')).toBe('unknown');
  });

  it('has no hardcoded version literal in the rendering surface', () => {
    // Second layer, same shape as the TagBus guard: a DOM test can stay green while a literal
    // creeps back into the component. This fails on a reintroduced "v1.2.3" string.
    const source = readFileSync(
      path.resolve(process.cwd(), 'packages/ui-shell/src/components/workbench/DockviewWorkspace.tsx'),
      'utf-8'
    );
    expect(source).not.toMatch(/\bv\d+\.\d+\.\d+/);
    expect(source).toContain('{APP_VERSION}');
  });
});
