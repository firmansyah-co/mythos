// The live tag cache and the one connection that feeds it.
//
// "Online" is something the engineer chooses: `goOnline` opens the telemetry stream and
// `goOffline` closes it. While the stream is not open the cache is empty, so no surface can show
// a value from an earlier connection as though it were current.

import { create } from 'zustand';
import { TAG_UPDATE_EVENT, TELEMETRY_STREAM_ENDPOINT } from '../api/client';
import { UNSPECIFIED_TYPE, type StreamState, type TagSample } from './quality';

/** Reconnect policy: first retry after 100 ms, growing by 1.5x to a ceiling of 5 s. */
export const MIN_RECONNECT_DELAY_MS = 100;
export const MAX_RECONNECT_DELAY_MS = 5000;
export const RECONNECT_BACKOFF_FACTOR = 1.5;

interface WireUpdate {
  uri?: unknown;
  type?: unknown;
  value?: unknown;
  quality_code?: unknown;
  quality_name?: unknown;
  timestamp_ns?: unknown;
}

interface WireBatch {
  timestamp_ms?: unknown;
  updates?: unknown;
}

function toSample(raw: WireUpdate): TagSample | null {
  if (typeof raw?.uri !== 'string' || raw.uri.length === 0) return null;
  return {
    uri: raw.uri,
    type: typeof raw.type === 'string' ? raw.type : UNSPECIFIED_TYPE,
    value: raw.value ?? null,
    qualityName: typeof raw.quality_name === 'string' ? raw.quality_name : '',
    qualityCode: typeof raw.quality_code === 'number' ? raw.quality_code : -1,
    timestampNs: typeof raw.timestamp_ns === 'number' ? raw.timestamp_ns : 0,
  };
}

export interface TelemetryState {
  /** Samples from the most recent batch, by tag URI. Empty unless the stream is open. */
  tags: Map<string, TagSample>;
  stream: StreamState;
  /** Controller time of the most recent batch, or `null` before the first. */
  lastBatchAtMs: number | null;
  /** Whether the engineer asked to be online. The stream may still be reconnecting. */
  wantOnline: boolean;

  /**
   * Replace the cache with one batch.
   *
   * Replaced, never merged: the controller reports every tag it could read on each tick, so a
   * tag missing from a batch is a tag it could not read. Merging would carry the previous value
   * across that gap and show it as current.
   */
  applyBatch: (batch: unknown) => void;
  setStream: (stream: StreamState) => void;
  clear: () => void;
}

export const useTelemetry = create<TelemetryState>((set) => ({
  tags: new Map(),
  stream: 'IDLE',
  lastBatchAtMs: null,
  wantOnline: false,

  applyBatch: (batch) => {
    const wire = (batch ?? {}) as WireBatch;
    const updates = Array.isArray(wire.updates) ? (wire.updates as WireUpdate[]) : [];
    const next = new Map<string, TagSample>();
    for (const update of updates) {
      const sample = toSample(update);
      if (sample) next.set(sample.uri, sample);
    }
    set({
      tags: next,
      lastBatchAtMs: typeof wire.timestamp_ms === 'number' ? wire.timestamp_ms : null,
    });
  },

  setStream: (stream) => set({ stream }),
  clear: () => set({ tags: new Map(), lastBatchAtMs: null }),
}));

/** Named events the controller sends on the stream besides tag batches. */
export const HMI_EVENTS = ['SCREENS_RELEASED'] as const;
export type HmiEventName = (typeof HMI_EVENTS)[number];

const hmiHandlers = new Map<HmiEventName, Set<(data: unknown) => void>>();

/** Be told when the controller announces `name`. Returns the function that stops it. */
export function onHmiEvent(name: HmiEventName, handler: (data: unknown) => void): () => void {
  const handlers = hmiHandlers.get(name) ?? new Set();
  hmiHandlers.set(name, handlers);
  handlers.add(handler);
  return () => handlers.delete(handler);
}

// One connection for the whole application. Module state, because a second stream would write
// the cache twice per tick and the two would disagree about whether the workbench is online.
let source: EventSource | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelayMs = MIN_RECONNECT_DELAY_MS;

function connect(): void {
  const store = useTelemetry.getState();
  if (!store.wantOnline) return;

  store.setStream('CONNECTING');
  const current = new EventSource(TELEMETRY_STREAM_ENDPOINT);
  source = current;

  current.onopen = () => {
    if (source !== current) return;
    reconnectDelayMs = MIN_RECONNECT_DELAY_MS;
    useTelemetry.getState().setStream('OPEN');
  };

  current.addEventListener(TAG_UPDATE_EVENT, (event) => {
    if (source !== current) return;
    let batch: unknown;
    try {
      batch = JSON.parse((event as MessageEvent<string>).data);
    } catch {
      // A frame that does not parse is not a batch. The stream is alive and the next frame is
      // one tick away, so the previous snapshot stands until then.
      return;
    }
    useTelemetry.getState().applyBatch(batch);
  });

  for (const name of HMI_EVENTS) {
    current.addEventListener(name, (event) => {
      if (source !== current) return;
      let data: unknown;
      try {
        data = JSON.parse((event as MessageEvent<string>).data);
      } catch {
        return;
      }
      for (const handler of hmiHandlers.get(name) ?? []) handler(data);
    });
  }

  current.onerror = () => {
    // Close it ourselves: the browser's own reconnect would keep the cache as it was while it
    // retried, and the last values would go on being displayed as live.
    current.close();
    if (source !== current) return;
    source = null;

    const state = useTelemetry.getState();
    state.clear();
    state.setStream('CLOSED');
    if (!state.wantOnline) return;

    const delay = reconnectDelayMs;
    // Whole milliseconds: a timer cannot wait a fraction of one.
    reconnectDelayMs = Math.min(
      Math.round(delay * RECONNECT_BACKOFF_FACTOR),
      MAX_RECONNECT_DELAY_MS
    );
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
  };
}

/** Open the telemetry stream and keep it open, reconnecting when it drops. */
export function goOnline(): void {
  if (useTelemetry.getState().wantOnline) return;
  useTelemetry.setState({ wantOnline: true });
  if (typeof EventSource === 'undefined') {
    // No stream can exist here. Report that rather than a connection that never completes.
    useTelemetry.getState().setStream('CLOSED');
    return;
  }
  reconnectDelayMs = MIN_RECONNECT_DELAY_MS;
  connect();
}

/** Close the telemetry stream and forget every value it delivered. */
export function goOffline(): void {
  useTelemetry.setState({ wantOnline: false });
  if (reconnectTimer !== null) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  source?.close();
  source = null;
  const state = useTelemetry.getState();
  state.clear();
  state.setStream('IDLE');
}
