import { useCallback, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

interface Options {
  /** Key under which the size is remembered between sessions. */
  storageKey: string;
  initial: number;
  min: number;
  max: number;
  /** Which way dragging grows the pane. */
  grow: 'right' | 'left' | 'up';
}

function readStored(key: string, fallback: number, min: number, max: number): number {
  try {
    const raw = window.localStorage.getItem(key);
    const value = raw === null ? NaN : Number(raw);
    return Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
  } catch {
    return fallback;
  }
}

/** A pane size the user can drag, remembered in local storage. */
export function useResizable({ storageKey, initial, min, max, grow }: Options) {
  const [size, setSize] = useState(() => readStored(storageKey, initial, min, max));
  const drag = useRef<{ start: number; size: number } | null>(null);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { start: grow === 'up' ? event.clientY : event.clientX, size };
    },
    [grow, size]
  );

  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (!drag.current) return;
      const position = grow === 'up' ? event.clientY : event.clientX;
      const delta = position - drag.current.start;
      const next = drag.current.size + (grow === 'right' ? delta : -delta);
      setSize(Math.min(max, Math.max(min, next)));
    },
    [grow, max, min]
  );

  const onPointerUp = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (!drag.current) return;
      drag.current = null;
      event.currentTarget.releasePointerCapture(event.pointerId);
      try {
        window.localStorage.setItem(storageKey, String(Math.round(size)));
      } catch {
        // Storage is unavailable; the size simply is not remembered.
      }
    },
    [size, storageKey]
  );

  return { size, handle: { onPointerDown, onPointerMove, onPointerUp } };
}
