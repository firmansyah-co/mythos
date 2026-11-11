/**
 * SSE telemetry consumer for the HMI canvas.
 *
 * Implements the listener described in
 * `docs/04-hmi-and-ui/00-frontend-application-architecture.md` §4 (`useSseTelemetry`): one
 * `EventSource` against the telemetry stream, exponential backoff reconnect, and a snapshot
 * handed to the tag store on every `tag_update`.
 *
 * ## One stream, however many panels are open
 *
 * A screen is a dockview tab and an operator routinely has several open at once. The connection
 * is therefore **module-scoped and reference-counted**, not per-hook-instance:
 *
 * - Two panels opening two `EventSource`s would parse the full catalog twice per tick and write
 *   the store twice per tick, and the two feeds would race last-writer-wins on every frame.
 * - Two panels reconnecting on independent backoff clocks would not recover together after a
 *   blip, and the store's single `connection` field would report whichever instance wrote last.
 * - A panel unmounting must not clear the cache a *different*, still-open panel is rendering
 *   from. That would show a healthy panel `BAD_QUALITY` because an unrelated tab closed.
 *
 * So the first subscriber connects, the last one disconnects, and everything in between shares
 * one connection and one backoff clock. The cache is cleared only when the last subscriber goes
 * away — at which point no surface remains that could claim a value is live.
 *
 * ## Backoff bounds are published, not chosen here
 *
 * The architecture doc fixes `minReconnectDelayMs = 100`, `maxReconnectDelayMs = 5000` and a
 * growth factor of `1.5`. Those numbers are reproduced from that contract rather than picked to
 * look reasonable; a reconnect policy is a plant behaviour and inventing one would be the same
 * class of defect as inventing a process value.
 *
 * ## Why the cache is cleared when the stream ends
 *
 * When the stream ends, the values in the cache stop being current. Keeping them would leave a
 * bound faceplate rendering the last frame as a live reading, which is precisely the failure
 * SYS-REQ-096 forbids. Clearing the cache makes every bound component resolve to `BAD_QUALITY`
 * until a new frame arrives.
 *
 * The server's own keep-alive is 15s (`telemetry.rs`), so a connection that has silently died is
 * detected by the browser's `onerror` well inside the window an operator would act on.
 */

import { useEffect } from 'react';
import { useTagStore, type WireTelemetryBatch } from '../stores/useTagStore';

/** Published default: the telemetry stream route (`mythos-core/src/api/mod.rs`). */
export const TELEMETRY_STREAM_ENDPOINT = '/api/v1/telemetry/stream';

/** Published in the architecture doc §4. */
export const MIN_RECONNECT_DELAY_MS = 100;
/** Published in the architecture doc §4. */
export const MAX_RECONNECT_DELAY_MS = 5000;
/** Published in the architecture doc §4. */
export const RECONNECT_BACKOFF_FACTOR = 1.5;

/** The SSE event name carrying a `TelemetryBatch` (`telemetry.rs`). */
export const TAG_UPDATE_EVENT = 'tag_update';

/**
 * The one shared connection. Module-scoped on purpose: this is process-wide state, and holding it
 * in a hook instance is what produced one stream per panel.
 */
interface SharedConnection {
  source: EventSource | null;
  /** How many mounted surfaces are currently subscribed. */
  subscribers: number;
  /** The backoff clock, shared so every panel recovers together. */
  reconnectDelayMs: number;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  endpoint: string;
  minReconnectDelayMs: number;
  maxReconnectDelayMs: number;
  /** Set while the last subscriber is tearing down, so a pending timer cannot reconnect. */
  disposed: boolean;
}

let shared: SharedConnection | null = null;

export interface UseSseTelemetryOptions {
  endpoint?: string;
  minReconnectDelayMs?: number;
  maxReconnectDelayMs?: number;
}

function connect(connection: SharedConnection): void {
  if (connection.disposed) return;

  useTagStore.getState().setConnection('CONNECTING');
  const source = new EventSource(connection.endpoint);
  connection.source = source;

  source.onopen = () => {
    if (connection.disposed) return;
    // Reset the shared backoff clock on a successful connection.
    connection.reconnectDelayMs = connection.minReconnectDelayMs;
    useTagStore.getState().setConnection('OPEN');
  };

  source.addEventListener(TAG_UPDATE_EVENT, (event) => {
    if (connection.disposed) return;
    let batch: WireTelemetryBatch;
    try {
      batch = JSON.parse((event as MessageEvent).data) as WireTelemetryBatch;
    } catch {
      // A malformed frame is not a value. Drop it and keep the last good snapshot rather than
      // clearing: the connection is alive and the next frame is 100ms away.
      return;
    }
    useTagStore.getState().applyBatch(batch);
  });

  source.onerror = () => {
    source.close();
    if (connection.source === source) connection.source = null;
    if (connection.disposed) return;

    // The stream is gone: the cached values are no longer current.
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('CLOSED');

    const delay = connection.reconnectDelayMs;
    connection.reconnectDelayMs = Math.min(
      delay * RECONNECT_BACKOFF_FACTOR,
      connection.maxReconnectDelayMs
    );
    connection.reconnectTimer = setTimeout(() => {
      connection.reconnectTimer = null;
      connect(connection);
    }, delay);
  };
}

function subscribe(options: Required<UseSseTelemetryOptions>): () => void {
  // No EventSource in this environment (SSR, tests without a stub): stay IDLE rather than
  // pretending a stream exists. The canvas then renders honest absence.
  if (typeof EventSource === 'undefined') {
    useTagStore.getState().setConnection('IDLE');
    return () => {};
  }

  if (!shared) {
    shared = {
      source: null,
      subscribers: 0,
      reconnectDelayMs: options.minReconnectDelayMs,
      reconnectTimer: null,
      endpoint: options.endpoint,
      minReconnectDelayMs: options.minReconnectDelayMs,
      maxReconnectDelayMs: options.maxReconnectDelayMs,
      disposed: false,
    };
  }

  const connection = shared;
  connection.subscribers += 1;

  // The first subscriber opens the stream. Later ones join it.
  if (connection.subscribers === 1) connect(connection);

  let released = false;
  return () => {
    // A double release (React strict-mode re-run, a remount) must not over-decrement.
    if (released) return;
    released = true;
    connection.subscribers -= 1;
    if (connection.subscribers > 0) return;

    // Last surface gone: close the stream and drop every value, so nothing can claim liveness.
    connection.disposed = true;
    if (connection.reconnectTimer !== null) {
      clearTimeout(connection.reconnectTimer);
      connection.reconnectTimer = null;
    }
    connection.source?.close();
    connection.source = null;
    if (shared === connection) shared = null;
    useTagStore.getState().clear();
    useTagStore.getState().setConnection('IDLE');
  };
}

/**
 * Subscribe the calling surface to the telemetry stream.
 *
 * Returns nothing: the values land in `useTagStore`, which the canvas reads. Mounting N panels
 * opens one connection, not N. Nothing here invents a value when the stream is silent — a silent
 * stream leaves the cache empty, and an empty cache resolves to `BAD_QUALITY`.
 */
export function useSseTelemetry({
  endpoint = TELEMETRY_STREAM_ENDPOINT,
  minReconnectDelayMs = MIN_RECONNECT_DELAY_MS,
  maxReconnectDelayMs = MAX_RECONNECT_DELAY_MS,
}: UseSseTelemetryOptions = {}): void {
  useEffect(
    () => subscribe({ endpoint, minReconnectDelayMs, maxReconnectDelayMs }),
    [endpoint, minReconnectDelayMs, maxReconnectDelayMs]
  );
}

/**
 * Drop the shared connection and its cache. Test-only: a suite that stubs `EventSource` between
 * cases must not inherit a connection opened by the previous one.
 */
export function __resetTelemetryConnectionForTests(): void {
  if (shared) {
    shared.disposed = true;
    if (shared.reconnectTimer !== null) clearTimeout(shared.reconnectTimer);
    shared.source?.close();
  }
  shared = null;
}
