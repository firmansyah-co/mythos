/**
 * S-004 — one real TagBus value reaches one HMI faceplate, and a dropped stream never leaves a
 * stale value presented as live (SYS-REQ-096).
 *
 * These tests are written against the REAL render path: a screen is put into `useHmiStore` and
 * `HmiScreenCanvasPanel` is rendered, exactly as the operator sees it. Nothing here asserts
 * against a stand-in for the faceplate.
 *
 * The defect this suite exists for, proven by execution against the resolver on origin/dev: the
 * wire's `quality` CLASS string ("BAD", "UNCERTAIN") and even the specific name
 * ("BAD_COMM_FAILURE") are not members of the faceplate enum, so passing one through as a quality
 * declaration makes `resolveComponentQuality` fall through to its final branch and return GOOD
 * whenever the payload carries any value. A comm failure with a stale value renders as a live
 * reading, and a naive "drop the stream" test still passes because no payload reaches the
 * `if (!live) return BAD_QUALITY` line at all.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';

import { useHmiStore, type ScreenDescriptor } from '../stores/useHmiStore';
import {
  useTagStore,
  mapWireQualityToFaceplateQuality,
  resolveComponentLive,
  readComponentBindings,
  type WireTelemetryBatch,
} from '../stores/useTagStore';
import {
  useSseTelemetry,
  MIN_RECONNECT_DELAY_MS,
  MAX_RECONNECT_DELAY_MS,
  __resetTelemetryConnectionForTests,
} from '../hooks/useSseTelemetry';
import { HmiScreenCanvasPanel } from '../components/workbench/DockviewWorkspace';

// ---------------------------------------------------------------------------------------------
// A controllable EventSource, so the stream can be driven and dropped from a test.
// ---------------------------------------------------------------------------------------------

type Listener = (event: { data: string }) => void;

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;

  readonly url: string;
  readyState = FakeEventSource.CONNECTING;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, Listener[]>();
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  addEventListener(name: string, fn: Listener) {
    const list = this.listeners.get(name) ?? [];
    list.push(fn);
    this.listeners.set(name, list);
  }

  emit(name: string, data: unknown) {
    for (const fn of this.listeners.get(name) ?? []) fn({ data: JSON.stringify(data) });
  }

  /** Simulate the browser opening the connection. */
  open() {
    this.readyState = FakeEventSource.OPEN;
    this.onopen?.();
  }

  /** Simulate the connection dropping (server gone, network cut). */
  fail() {
    this.readyState = FakeEventSource.CLOSED;
    this.onerror?.();
  }

  close() {
    this.closed = true;
    this.readyState = FakeEventSource.CLOSED;
  }
}

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

const TAG_URI = 'external.plant.MTR-1.rSpeedFeedback';

const MOTOR_SCREEN: ScreenDescriptor = {
  id: 'screen_live_probe',
  name: 'Live probe',
  is_root: true,
  version: '1.0.0',
  subscreens: [],
  canvasData: {
    title: 'Live probe',
    components: [
      {
        id: 'mtr_1',
        type: 'MOTOR',
        tag: 'MTR-1',
        label: 'Feed Pump',
        bindings: [{ property: 'actualSpeed', tag: TAG_URI }],
      },
    ],
  },
} as ScreenDescriptor;

let container: HTMLDivElement;
let root: Root;

function renderCanvas(): string {
  flushSync(() => {
    root.render(
      React.createElement(HmiScreenCanvasPanel, {
        params: { screenId: MOTOR_SCREEN.id },
      } as never)
    );
  });
  return container.innerHTML;
}

/** The rendered faceplate text, extracted by testid so an SVG path cannot satisfy the assertion. */
function faceplateText(): string {
  const el = container.querySelector('[data-testid^="faceplate-tier-"]');
  if (!el) throw new Error('motor faceplate was not rendered');
  return el.textContent ?? '';
}

function mountScreen() {
  useHmiStore.setState({
    screens: [MOTOR_SCREEN],
    draftScreens: new Map([[MOTOR_SCREEN.id, MOTOR_SCREEN.canvasData]]),
    activeScreens: new Map(),
    activeScreenId: MOTOR_SCREEN.id,
    activeSubscreenId: null,
  } as never);
}

/** One wire update, shaped exactly as `TagValueUpdate` serialises in `telemetry.rs`. */
function wireUpdate(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    uri: TAG_URI,
    slot_id: 7,
    type: 'REAL',
    value: 1450,
    quality: 'GOOD',
    quality_code: 0x00c0,
    quality_name: 'GOOD',
    timestamp_ns: 1_700_000_000_000_000_000,
    ...overrides,
  };
}

function batch(updates: Array<Partial<Record<string, unknown>>>): WireTelemetryBatch {
  return { timestamp_ms: 1_700_000_000_000, batch_window_ms: 100, updates: updates as never };
}

/**
 * Render TWO canvas panels on one screen, which is the operator's normal case: a screen is a
 * dockview tab and more than one can be open.
 *
 * A single-panel test passes against both multi-panel defects, so only a two-panel test can fail
 * for them. `FakeEventSource.instances` assertions elsewhere in this file are reconnect-backoff
 * assertions on a single panel — green, correct, and not coverage for this.
 */
const SECOND_SCREEN: ScreenDescriptor = {
  ...MOTOR_SCREEN,
  id: 'screen_live_probe_2',
} as ScreenDescriptor;

function mountTwoScreens() {
  useHmiStore.setState({
    screens: [MOTOR_SCREEN, SECOND_SCREEN],
    draftScreens: new Map([
      [MOTOR_SCREEN.id, MOTOR_SCREEN.canvasData],
      [SECOND_SCREEN.id, SECOND_SCREEN.canvasData],
    ]),
    activeScreens: new Map(),
    activeScreenId: MOTOR_SCREEN.id,
    activeSubscreenId: null,
  } as never);
}

/**
 * Render N panels inside a STABLE wrapper.
 *
 * The wrapper matters: changing the root's element type between renders (a div, then a bare
 * panel) makes React unmount the whole subtree and remount it, which tears the connection down
 * and back up. That is a harness artefact, not the property under test — it would fail this suite
 * against correct code. Keeping one wrapper means a change in child count unmounts exactly the
 * panel that went away.
 */
function renderPanels(screenIds: string[]): void {
  flushSync(() => {
    root.render(
      React.createElement(
        'div',
        null,
        ...screenIds.map((screenId) =>
          React.createElement(HmiScreenCanvasPanel, { key: screenId, params: { screenId } } as never)
        )
      )
    );
  });
}

const renderTwoPanels = () => renderPanels([MOTOR_SCREEN.id, SECOND_SCREEN.id]);
const renderOnlyFirstPanel = () => renderPanels([MOTOR_SCREEN.id]);

describe('S-004: a real TagBus value reaches a rendered faceplate', () => {
  beforeEach(() => {
    __resetTelemetryConnectionForTests();
    FakeEventSource.instances = [];
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('IDLE');
    mountScreen();
  });

  afterEach(() => {
    flushSync(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('renders the published value on the faceplate after a GOOD tag_update', () => {
    renderCanvas();
    const source = FakeEventSource.instances[0];
    expect(source, 'the canvas must open exactly one telemetry stream').toBeDefined();

    source.open();
    source.emit('tag_update', batch([wireUpdate()]));
    const html = renderCanvas();

    expect(html).toContain('1450');
    // A GOOD faceplate announces no data-quality badge.
    expect(container.querySelector('[data-testid="faceplate-no-data"]')).toBeNull();
  });

  it('renders honest absence before any frame arrives', () => {
    renderCanvas();
    FakeEventSource.instances[0].open();
    renderCanvas();

    // Bound but not yet reporting: BAD_QUALITY, values suppressed, badge shown.
    const badge = container.querySelector('[data-testid="faceplate-no-data"]');
    expect(badge).not.toBeNull();
    expect(badge?.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(faceplateText()).not.toContain('1450');
  });
});

describe('S-004: many panels share ONE stream, and one panel closing does not disturb another', () => {
  beforeEach(() => {
    __resetTelemetryConnectionForTests();
    FakeEventSource.instances = [];
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('IDLE');
    mountTwoScreens();
  });

  afterEach(() => {
    flushSync(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  // Property (b): connection count. Two panels at 10 Hz each would parse the full catalog twice
  // and replace the whole Map twice per tick, racing last-writer-wins on every frame.
  it('opens exactly ONE EventSource for two mounted panels', () => {
    renderTwoPanels();
    expect(
      FakeEventSource.instances.length,
      'two panels must share one telemetry stream, not open one each'
    ).toBe(1);
  });

  it('still opens exactly one stream for three panels', () => {
    const third = { ...MOTOR_SCREEN, id: 'screen_live_probe_3' } as ScreenDescriptor;
    useHmiStore.setState({
      screens: [MOTOR_SCREEN, SECOND_SCREEN, third],
      draftScreens: new Map([
        [MOTOR_SCREEN.id, MOTOR_SCREEN.canvasData],
        [SECOND_SCREEN.id, SECOND_SCREEN.canvasData],
        [third.id, MOTOR_SCREEN.canvasData],
      ]),
      activeScreens: new Map(),
      activeScreenId: MOTOR_SCREEN.id,
      activeSubscreenId: null,
    } as never);
    renderPanels([MOTOR_SCREEN.id, SECOND_SCREEN.id, third.id]);
    expect(FakeEventSource.instances.length, 'connection count must not scale with panel count').toBe(1);
  });

  // Property (a): teardown isolation. A healthy panel must not go BAD_QUALITY because a
  // different panel closed, and must not depend on waiting for the next 100ms frame to recover.
  it('keeps the surviving panel live when another panel unmounts', () => {
    renderTwoPanels();
    FakeEventSource.instances[0].open();
    FakeEventSource.instances[0].emit('tag_update', batch([wireUpdate()]));
    renderTwoPanels();
    expect(container.innerHTML).toContain('1450');

    // Close the second panel. The first is still open and still healthy.
    renderOnlyFirstPanel();

    // No new frame is delivered after the unmount: the survivor must still be GOOD.
    const badge = container.querySelector('[data-testid="faceplate-no-data"]');
    expect(
      badge,
      'a healthy panel must not degrade because a DIFFERENT panel closed'
    ).toBeNull();
    expect(faceplateText()).toContain('1450');
    expect(useTagStore.getState().tags.has(TAG_URI), 'the cache must survive one panel unmounting').toBe(
      true
    );
  });

  it('closes the shared stream only when the LAST panel unmounts', () => {
    renderTwoPanels();
    const source = FakeEventSource.instances[0];
    source.open();

    renderOnlyFirstPanel();
    expect(source.closed, 'one of two panels unmounting must not close the shared stream').toBe(false);

    // The last panel goes away: now the stream should be closed and the cache dropped.
    flushSync(() => root.render(React.createElement('div', null)));
    expect(source.closed, 'the last panel unmounting must close the stream').toBe(true);
    expect(useTagStore.getState().tags.size, 'no panel remains, so no value may be claimed').toBe(0);
  });
});

describe('S-004: the stream dropping never leaves a stale value presented as live', () => {
  beforeEach(() => {
    __resetTelemetryConnectionForTests();
    FakeEventSource.instances = [];
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('IDLE');
    mountScreen();
  });

  afterEach(() => {
    flushSync(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('degrades to BAD_QUALITY when the stream drops mid-render, and drops the stale value', () => {
    renderCanvas();
    const source = FakeEventSource.instances[0];
    source.open();
    source.emit('tag_update', batch([wireUpdate()]));
    renderCanvas();
    expect(container.innerHTML).toContain('1450');

    // The connection dies with a value still on screen.
    source.fail();
    renderCanvas();

    const badge = container.querySelector('[data-testid="faceplate-no-data"]');
    expect(badge, 'a dropped stream must announce abnormal quality').not.toBeNull();
    expect(badge?.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(faceplateText(), 'the stale value must not survive the drop').not.toContain('1450');
  });

  /**
   * The defect that the naive drop-the-stream test cannot see.
   *
   * A comm failure arrives WITH a stale value still in the slot. If the wire quality is passed
   * through unmapped, the resolver's inference returns GOOD and the faceplate renders 1450 as a
   * live reading while the field link is dead. This asserts on the RENDERED surface, so it fails
   * against that shape and passes only when the quality is mapped explicitly.
   */
  it('renders a comm failure as BAD_QUALITY even though the frame carries a stale value', () => {
    renderCanvas();
    const source = FakeEventSource.instances[0];
    source.open();
    source.emit(
      'tag_update',
      batch([
        wireUpdate({
          value: 1450,
          quality: 'BAD',
          quality_code: 0x0018,
          quality_name: 'BAD_COMM_FAILURE',
        }),
      ])
    );
    renderCanvas();

    const badge = container.querySelector('[data-testid="faceplate-no-data"]');
    expect(badge, 'a comm failure must announce abnormal quality').not.toBeNull();
    expect(badge?.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(faceplateText(), 'a stale slot value must not be rendered as live').not.toContain('1450');
  });

  it('reconnects with bounded exponential backoff and recovers on the next frame', () => {
    vi.useFakeTimers();
    renderCanvas();
    const first = FakeEventSource.instances[0];
    first.open();
    first.fail();

    // First retry waits the published minimum.
    expect(FakeEventSource.instances).toHaveLength(1);
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS);
    expect(FakeEventSource.instances).toHaveLength(2);

    // Each subsequent failure backs off further, never past the published maximum.
    const second = FakeEventSource.instances[1];
    second.fail();
    vi.advanceTimersByTime(Math.round(MIN_RECONNECT_DELAY_MS * 1.5));
    expect(FakeEventSource.instances).toHaveLength(3);

    for (let i = 0; i < 20; i += 1) {
      const current = FakeEventSource.instances[FakeEventSource.instances.length - 1];
      current.fail();
      vi.advanceTimersByTime(MAX_RECONNECT_DELAY_MS);
    }
    // The delay is capped, so a retry still happens after the maximum wait.
    const before = FakeEventSource.instances.length;
    FakeEventSource.instances[FakeEventSource.instances.length - 1].fail();
    vi.advanceTimersByTime(MAX_RECONNECT_DELAY_MS);
    expect(FakeEventSource.instances.length).toBe(before + 1);

    // And the surface recovers when a frame arrives again.
    const latest = FakeEventSource.instances[FakeEventSource.instances.length - 1];
    latest.open();
    latest.emit('tag_update', batch([wireUpdate()]));
    vi.useRealTimers();
    renderCanvas();
    expect(container.innerHTML).toContain('1450');
  });
});

describe('S-004: quality is mapped, never inferred from a value being present', () => {
  // The three cases that fail OPEN if a wire quality string is passed straight through.
  it.each([
    ['BAD class with a stale value', { quality: 'BAD', quality_code: 0x0000, quality_name: 'BAD_NON_SPECIFIC' }],
    ['UNCERTAIN class with a value', { quality: 'UNCERTAIN', quality_code: 0x0044, quality_name: 'UNCERTAIN_SUBSTITUTE' }],
    ['comm failure with a stale value', { quality: 'BAD', quality_code: 0x0018, quality_name: 'BAD_COMM_FAILURE' }],
  ])('resolves %s to BAD_QUALITY', (_label, wire) => {
    const sample = resolveComponentLive(
      { bindings: [{ property: 'actualSpeed', tag: TAG_URI }] },
      new Map([
        [
          TAG_URI,
          {
            uri: TAG_URI,
            value: 1450,
            type: 'REAL',
            qualityCode: (wire as { quality_code: number }).quality_code,
            qualityName: (wire as { quality_name: string }).quality_name,
            quality: mapWireQualityToFaceplateQuality(
              (wire as { quality_name: string }).quality_name,
              (wire as { quality_code: number }).quality_code,
              'REAL'
            ),
          },
        ],
      ])
    );

    expect(sample?.quality).toBe('BAD_QUALITY');
    expect(sample?.live, 'a non-GOOD binding must yield no process value').toEqual({});
  });

  it('maps only the GOOD name to GOOD', () => {
    expect(mapWireQualityToFaceplateQuality('GOOD', 0x00c0, 'REAL')).toBe('GOOD');
  });

  it('never promotes an unrecognised quality word to GOOD', () => {
    expect(mapWireQualityToFaceplateQuality('UNRECOGNIZED', 0x7fff, 'REAL')).toBe('BAD_QUALITY');
    expect(mapWireQualityToFaceplateQuality(undefined, 0x7fff, 'REAL')).toBe('BAD_QUALITY');
    expect(mapWireQualityToFaceplateQuality(undefined, undefined, 'REAL')).toBe('BAD_QUALITY');
  });

  it('treats a slot with no declared type as unusable even when its quality says GOOD', () => {
    expect(mapWireQualityToFaceplateQuality('GOOD', 0x00c0, 'UNSPECIFIED')).toBe('BAD_QUALITY');
  });

  it('suppresses the whole faceplate when any one binding is not GOOD', () => {
    const resolved = resolveComponentLive(
      {
        bindings: [
          { property: 'actualSpeed', tag: TAG_URI },
          { property: 'currentAmps', tag: 'external.plant.MTR-1.rCurrentAmps' },
        ],
      },
      new Map([
        [
          TAG_URI,
          { uri: TAG_URI, value: 1450, type: 'REAL', qualityCode: 0x00c0, qualityName: 'GOOD', quality: 'GOOD' as const },
        ],
      ])
    );
    expect(resolved?.quality).toBe('BAD_QUALITY');
    expect(resolved?.live).toEqual({});
  });

  it('leaves an unbound component alone so it resolves NOT_BOUND', () => {
    expect(resolveComponentLive({ id: 'mtr_2', type: 'MOTOR' }, new Map())).toBeNull();
    expect(readComponentBindings({ id: 'mtr_2' })).toEqual([]);
  });
});

describe('S-004: the cache is a snapshot, not an accumulator', () => {
  beforeEach(() => {
    useTagStore.getState().clear();
  });

  it('drops a tag that is absent from the next batch instead of retaining it', () => {
    useTagStore.getState().applyBatch(batch([wireUpdate()]));
    expect(useTagStore.getState().tags.has(TAG_URI)).toBe(true);

    // The server emits an update only for tags whose slot it could read. An absent tag means
    // "could not read", not "unchanged" — retaining it would present a stale value as current.
    useTagStore.getState().applyBatch(batch([]));
    expect(useTagStore.getState().tags.has(TAG_URI)).toBe(false);
  });

  it('records the batch timestamp it is reporting', () => {
    useTagStore.getState().applyBatch(batch([wireUpdate()]));
    expect(useTagStore.getState().lastBatchAtMs).toBe(1_700_000_000_000);
  });
});

describe('S-004: no EventSource in this environment', () => {
  it('stays IDLE rather than pretending a stream exists', () => {
    vi.stubGlobal('EventSource', undefined);
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('OPEN');

    function Probe() {
      useSseTelemetry();
      return null;
    }
    const el = document.createElement('div');
    const probeRoot = createRoot(el);
    flushSync(() => probeRoot.render(React.createElement(Probe)));

    expect(useTagStore.getState().connection).toBe('IDLE');
    flushSync(() => probeRoot.unmount());
    vi.unstubAllGlobals();
  });
});
