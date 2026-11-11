/**
 * SSE telemetry consumer for the HMI canvas.
 *
 * Implements the listener described in
 * `docs/04-hmi-and-ui/00-frontend-application-architecture.md` §4 (`useSseTelemetry`): one
 * `EventSource` against the telemetry stream, exponential backoff reconnect, and a snapshot
 * handed to the tag store on every `tag_update`.
 *
 * ## Backoff bounds are published, not chosen here
 *
 * The architecture doc fixes `minReconnectDelayMs = 100`, `maxReconnectDelayMs = 5000` and a
 * growth factor of `1.5`. Those numbers are reproduced from that contract rather than picked to
 * look reasonable; a reconnect policy is a plant behaviour and inventing one would be the same
 * class of defect as inventing a process value.
 *
 * ## Why the cache is cleared on close
 *
 * When the stream ends, the values in the cache stop being current. Keeping them would leave a
 * bound faceplate rendering the last frame as a live reading, which is precisely the failure
 * SYS-REQ-096 forbids. Clearing the cache makes every bound component resolve to `BAD_QUALITY`
 * until a new frame arrives.
 *
 * The server's own keep-alive is 15s (`telemetry.rs`), so a connection that has silently died is
 * detected by the browser's `onerror` well inside the window an operator would act on.
 */

import { useEffect, useRef } from 'react';
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

export interface UseSseTelemetryOptions {
  endpoint?: string;
  minReconnectDelayMs?: number;
  maxReconnectDelayMs?: number;
}

/**
 * Subscribe to the telemetry stream for the lifetime of the calling component.
 *
 * Returns nothing: the values land in `useTagStore`, which the canvas reads. Nothing here invents
 * a value when the stream is silent — a silent stream leaves the cache empty, and an empty cache
 * resolves to `BAD_QUALITY`.
 */
export function useSseTelemetry({
  endpoint = TELEMETRY_STREAM_ENDPOINT,
  minReconnectDelayMs = MIN_RECONNECT_DELAY_MS,
  maxReconnectDelayMs = MAX_RECONNECT_DELAY_MS,
}: UseSseTelemetryOptions = {}): void {
  const reconnectDelayRef = useRef(minReconnectDelayMs);
  const eventSourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    // No EventSource in this environment (SSR, tests without a stub): stay IDLE rather than
    // pretending a stream exists. The canvas then renders honest absence.
    if (typeof EventSource === 'undefined') {
      useTagStore.getState().setConnection('IDLE');
      return;
    }

    let isSubscribed = true;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const store = useTagStore.getState();

    function connect() {
      if (!isSubscribed) return;

      store.setConnection('CONNECTING');
      const source = new EventSource(endpoint);
      eventSourceRef.current = source;

      source.onopen = () => {
        if (!isSubscribed) return;
        reconnectDelayRef.current = minReconnectDelayMs;
        useTagStore.getState().setConnection('OPEN');
      };

      source.addEventListener(TAG_UPDATE_EVENT, (event) => {
        if (!isSubscribed) return;
        let batch: WireTelemetryBatch;
        try {
          batch = JSON.parse((event as MessageEvent).data) as WireTelemetryBatch;
        } catch {
          // A malformed frame is not a value. Drop it and keep the last good snapshot rather
          // than clearing: the connection is alive and the next frame is 100ms away.
          return;
        }
        useTagStore.getState().applyBatch(batch);
      });

      source.onerror = () => {
        source.close();
        if (eventSourceRef.current === source) eventSourceRef.current = null;
        if (!isSubscribed) return;

        // The stream is gone: the cached values are no longer current.
        useTagStore.getState().clear();
        useTagStore.getState().setConnection('CLOSED');

        const delay = reconnectDelayRef.current;
        reconnectDelayRef.current = Math.min(delay * RECONNECT_BACKOFF_FACTOR, maxReconnectDelayMs);
        reconnectTimer = setTimeout(connect, delay);
      };
    }

    connect();

    return () => {
      isSubscribed = false;
      if (reconnectTimer !== null) clearTimeout(reconnectTimer);
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      // Leaving the surface ends the claim that its values are live.
      useTagStore.getState().clear();
      useTagStore.getState().setConnection('IDLE');
    };
  }, [endpoint, minReconnectDelayMs, maxReconnectDelayMs]);
}
