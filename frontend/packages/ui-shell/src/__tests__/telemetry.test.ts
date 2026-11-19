import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  goOffline,
  goOnline,
  MAX_RECONNECT_DELAY_MS,
  MIN_RECONNECT_DELAY_MS,
  onStreamEvent,
  useTelemetry,
} from '../data/telemetry';
import { resetStores } from './harness';

/** A stand-in for the browser's EventSource that the test drives by hand. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  private listeners = new Map<string, Array<(event: { data: string }) => void>>();

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }
  addEventListener(name: string, listener: (event: { data: string }) => void) {
    this.listeners.set(name, [...(this.listeners.get(name) ?? []), listener]);
  }
  close() {
    this.closed = true;
  }
  emit(name: string, data: string) {
    for (const listener of this.listeners.get(name) ?? []) listener({ data });
  }
  static last(): FakeEventSource {
    const instance = FakeEventSource.instances[FakeEventSource.instances.length - 1];
    if (!instance) throw new Error('no EventSource was opened');
    return instance;
  }
}

const batch = (updates: unknown[]) => JSON.stringify({ timestamp_ms: 1000, batch_window_ms: 100, updates });
const good = (uri: string, value: number) => ({
  uri,
  type: 'UINT',
  value,
  quality: 'GOOD',
  quality_code: 0xc0,
  quality_name: 'GOOD',
  timestamp_ns: 5,
});

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  vi.stubGlobal('EventSource', FakeEventSource);
  resetStores();
});

afterEach(() => {
  goOffline();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('the tag cache', () => {
  it('replaces the snapshot with each batch, so a tag that stops being reported disappears', () => {
    const { applyBatch } = useTelemetry.getState();
    applyBatch(JSON.parse(batch([good('external.A.B.one', 1), good('external.A.B.two', 2)])));
    expect([...useTelemetry.getState().tags.keys()]).toEqual(['external.A.B.one', 'external.A.B.two']);

    applyBatch(JSON.parse(batch([good('external.A.B.two', 3)])));
    expect([...useTelemetry.getState().tags.keys()]).toEqual(['external.A.B.two']);
    expect(useTelemetry.getState().tags.get('external.A.B.two')?.value).toBe(3);
  });

  it('ignores updates that name no tag and tolerates a batch with no updates', () => {
    const { applyBatch } = useTelemetry.getState();
    applyBatch({ updates: [{ value: 1 }, null, good('external.A.B.one', 1)] });
    expect(useTelemetry.getState().tags.size).toBe(1);
    applyBatch({});
    expect(useTelemetry.getState().tags.size).toBe(0);
  });
});

describe('the connection to the controller', () => {
  it('goes online, receives batches, and forgets everything when the stream is lost', () => {
    goOnline();
    expect(useTelemetry.getState().stream).toBe('CONNECTING');

    const source = FakeEventSource.last();
    source.onopen?.();
    expect(useTelemetry.getState().stream).toBe('OPEN');

    source.emit('tag_update', batch([good('external.A.B.one', 42)]));
    expect(useTelemetry.getState().tags.get('external.A.B.one')?.value).toBe(42);

    source.onerror?.();
    expect(source.closed).toBe(true);
    expect(useTelemetry.getState().stream).toBe('CLOSED');
    expect(useTelemetry.getState().tags.size).toBe(0);
    expect(useTelemetry.getState().lastBatchAtMs).toBeNull();
  });

  it('keeps the previous snapshot when a frame does not parse', () => {
    goOnline();
    const source = FakeEventSource.last();
    source.onopen?.();
    source.emit('tag_update', batch([good('external.A.B.one', 42)]));
    source.emit('tag_update', '{ not json');
    expect(useTelemetry.getState().tags.get('external.A.B.one')?.value).toBe(42);
    expect(useTelemetry.getState().stream).toBe('OPEN');
  });

  it('reconnects with a growing delay that never exceeds the ceiling', () => {
    goOnline();
    let expectedDelay = MIN_RECONNECT_DELAY_MS;
    for (let attempt = 0; attempt < 14; attempt++) {
      const before = FakeEventSource.instances.length;
      FakeEventSource.last().onerror?.();

      vi.advanceTimersByTime(expectedDelay - 1);
      expect(FakeEventSource.instances.length, `attempt ${attempt}: reconnected early`).toBe(before);
      vi.advanceTimersByTime(1);
      expect(FakeEventSource.instances.length, `attempt ${attempt}: did not reconnect`).toBe(before + 1);

      expectedDelay = Math.min(Math.round(expectedDelay * 1.5), MAX_RECONNECT_DELAY_MS);
    }
    expect(expectedDelay).toBe(MAX_RECONNECT_DELAY_MS);
  });

  it('starts again from the shortest delay after a successful connection', () => {
    goOnline();
    FakeEventSource.last().onerror?.();
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS);
    FakeEventSource.last().onerror?.();
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS * 1.5);

    FakeEventSource.last().onopen?.();
    const before = FakeEventSource.instances.length;
    FakeEventSource.last().onerror?.();
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS);
    expect(FakeEventSource.instances.length).toBe(before + 1);
  });

  it('going offline closes the stream, forgets every value, and does not reconnect', () => {
    goOnline();
    const source = FakeEventSource.last();
    source.onopen?.();
    source.emit('tag_update', batch([good('external.A.B.one', 42)]));

    goOffline();
    expect(source.closed).toBe(true);
    expect(useTelemetry.getState().stream).toBe('IDLE');
    expect(useTelemetry.getState().tags.size).toBe(0);

    const opened = FakeEventSource.instances.length;
    vi.advanceTimersByTime(MAX_RECONNECT_DELAY_MS * 3);
    expect(FakeEventSource.instances.length).toBe(opened);
  });

  it('a stream that dies while reconnecting is pending does not resurrect after going offline', () => {
    goOnline();
    FakeEventSource.last().onerror?.();
    goOffline();
    const opened = FakeEventSource.instances.length;
    vi.advanceTimersByTime(MAX_RECONNECT_DELAY_MS);
    expect(FakeEventSource.instances.length).toBe(opened);
    expect(useTelemetry.getState().stream).toBe('IDLE');
  });

  it('ignores a late frame from a stream that has been replaced', () => {
    goOnline();
    const first = FakeEventSource.last();
    first.onopen?.();
    first.onerror?.();
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS);
    const second = FakeEventSource.last();
    expect(second).not.toBe(first);

    first.emit('tag_update', batch([good('external.A.B.ghost', 1)]));
    expect(useTelemetry.getState().tags.size).toBe(0);
  });
});

describe("a module's own events (ADR-011 §2.2)", () => {
  it('reaches a listener asked for before the stream opened, after, and after a reconnect', () => {
    const early: unknown[] = [];
    const late: unknown[] = [];
    const stopEarly = onStreamEvent('NOTES_CHANGED', (data) => early.push(data));
    goOnline();
    const first = FakeEventSource.last();
    first.onopen?.();
    first.emit('NOTES_CHANGED', JSON.stringify({ n: 1 }));

    // A name nobody asked for before the stream opened is still heard on that stream.
    const stopLate = onStreamEvent('TREND_SAVED', (data) => late.push(data));
    first.emit('TREND_SAVED', JSON.stringify({ t: 1 }));

    first.onerror?.();
    vi.advanceTimersByTime(MIN_RECONNECT_DELAY_MS);
    const second = FakeEventSource.last();
    expect(second).not.toBe(first);
    second.onopen?.();
    second.emit('NOTES_CHANGED', JSON.stringify({ n: 2 }));
    second.emit('TREND_SAVED', JSON.stringify({ t: 2 }));
    // The closed stream's listeners are deaf: nothing it still delivers counts.
    first.emit('NOTES_CHANGED', JSON.stringify({ n: 'stale' }));

    expect(early).toEqual([{ n: 1 }, { n: 2 }]);
    expect(late).toEqual([{ t: 1 }, { t: 2 }]);

    stopEarly();
    stopLate();
    second.emit('NOTES_CHANGED', JSON.stringify({ n: 3 }));
    expect(early).toHaveLength(2);
  });

  it('skips a frame that is not JSON', () => {
    const seen: unknown[] = [];
    const stop = onStreamEvent('NOTES_CHANGED', (data) => seen.push(data));
    goOnline();
    FakeEventSource.last().emit('NOTES_CHANGED', '{not json');
    FakeEventSource.last().emit('NOTES_CHANGED', '"ok"');
    expect(seen).toEqual(['ok']);
    stop();
  });
});
