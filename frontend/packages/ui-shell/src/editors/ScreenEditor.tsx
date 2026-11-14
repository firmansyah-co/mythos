import { useEffect, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from 'react';
import { RotateCcw, Save, Send, Trash2 } from 'lucide-react';
import { GRID, type KnownElement } from '../data/screenModel';
import {
  anyScreenDirty,
  isScreenDirty,
  releaseState,
  useScreens,
  type ReleaseState,
} from '../data/screens';
import { editorId, useWorkbench } from '../data/workbench';
import { ElementBody } from './ScreenElements';

const RELEASE_WORDS: Record<ReleaseState, { label: string; tone: string }> = {
  never: { label: 'NOT RELEASED', tone: 'neutral' },
  current: { label: 'RELEASED', tone: 'good' },
  differs: { label: 'RELEASED VERSION DIFFERS', tone: 'warn' },
  unknown: { label: 'RELEASE STATE UNKNOWN', tone: 'neutral' },
};

/** Edits one HMI screen: a canvas of elements, saved as a draft and released to operators. */
export function ScreenEditor({ screenId }: { screenId: string }) {
  const doc = useScreens((state) => state.screens.find((screen) => screen.id === screenId));
  const phase = useScreens((state) => state.phase);
  const loadError = useScreens((state) => state.loadError);
  const released = useScreens((state) => state.released);
  const busy = useScreens((state) => state.busy);
  const error = useScreens((state) => state.error);
  const othersDirty = useScreens(anyScreenDirty);
  const save = useScreens((state) => state.save);
  const discard = useScreens((state) => state.discard);
  const release = useScreens((state) => state.release);
  const remove = useScreens((state) => state.remove);
  const updateElement = useScreens((state) => state.updateElement);
  const removeElement = useScreens((state) => state.removeElement);
  const selection = useWorkbench((state) => state.selection);
  const select = useWorkbench((state) => state.select);
  const closeEditor = useWorkbench((state) => state.closeEditor);

  const viewport = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const drag = useRef<{ id: string; startX: number; startY: number; x: number; y: number } | null>(null);

  const canvasWidth = doc?.canvas.width;
  useEffect(() => {
    const element = viewport.current;
    if (!element || canvasWidth === undefined) return;
    const fit = () => {
      const available = element.clientWidth - 32;
      // Shrink to fit, never enlarge: one canvas unit is at most one pixel.
      setScale(available > 0 ? Math.min(1, available / canvasWidth) : 1);
    };
    fit();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [canvasWidth]);

  if (phase === 'idle' || phase === 'loading') {
    return <p className="editor__loading muted">Reading the project's screens…</p>;
  }
  if (phase === 'unreadable') {
    return (
      <div className="notice notice--bad" role="alert" data-testid="screens-unreadable">
        <div>
          <h3>The screens could not be read</h3>
          <p>{loadError}</p>
        </div>
      </div>
    );
  }
  if (!doc) {
    return (
      <div className="notice">
        <div>
          <h3>This screen is no longer in the project</h3>
          <p>It was removed.</p>
        </div>
      </div>
    );
  }

  const dirty = isScreenDirty(doc);
  const state = releaseState(doc, released);
  const words = RELEASE_WORDS[state];
  const selectedId =
    selection?.kind === 'element' && selection.screenId === screenId ? selection.elementId : null;
  const unknown = doc.canvas.elements.filter((element) => element.type === 'unknown');

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>, element: KnownElement) => {
    event.stopPropagation();
    select({ kind: 'element', screenId, elementId: element.id });
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { id: element.id, startX: event.clientX, startY: event.clientY, x: element.x, y: element.y };
  };
  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (!active) return;
    updateElement(screenId, active.id, {
      x: active.x + (event.clientX - active.startX) / scale,
      y: active.y + (event.clientY - active.startY) / scale,
    });
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!selectedId) return;
    const element = doc.canvas.elements.find((candidate) => candidate.id === selectedId);
    if (!element || element.type === 'unknown') return;
    const step = { ArrowLeft: [-GRID, 0], ArrowRight: [GRID, 0], ArrowUp: [0, -GRID], ArrowDown: [0, GRID] }[
      event.key
    ];
    if (step) {
      event.preventDefault();
      updateElement(screenId, selectedId, { x: element.x + step[0]!, y: element.y + step[1]! });
    } else if (event.key === 'Delete') {
      event.preventDefault();
      removeElement(screenId, selectedId);
      select(null);
    }
  };

  return (
    <div className="editor__column" data-testid="screen-editor">
      <div className="editor__tools">
        <button
          type="button"
          className="btn btn--primary"
          disabled={!dirty || busy !== null}
          onClick={() => void save(screenId)}
        >
          <Save size={13} aria-hidden /> {busy === 'saving' ? 'Saving…' : 'Save screen'}
        </button>
        <button type="button" className="btn" disabled={!dirty || busy !== null} onClick={() => discard(screenId)}>
          <RotateCcw size={13} aria-hidden /> Discard changes
        </button>
        <span className="toolbar__sep" aria-hidden />
        <button
          type="button"
          className="btn"
          disabled={othersDirty || busy !== null}
          title={
            othersDirty
              ? 'Save or discard the unsaved screens first: a release publishes what is saved'
              : 'Make the saved screens what operators see'
          }
          onClick={() => void release()}
        >
          <Send size={13} aria-hidden /> {busy === 'releasing' ? 'Releasing…' : 'Release screens'}
        </button>
        {dirty && (
          <span className="badge badge--warn" data-testid="screen-unsaved">
            UNSAVED CHANGES
          </span>
        )}
        <span className={`badge badge--${words.tone}`} data-testid="release-state">
          {words.label}
        </span>
        <span className="editor__tools-end">
          <button
            type="button"
            className="btn btn--flat"
            disabled={busy !== null}
            onClick={() => {
              if (!window.confirm(`Remove the screen “${doc.name}” from the project?`)) return;
              void remove(screenId).then((removed) => {
                if (removed) closeEditor(editorId('screen', screenId));
              });
            }}
          >
            <Trash2 size={13} aria-hidden /> Remove screen
          </button>
        </span>
      </div>

      {error && (
        <div className="notice notice--bad" role="alert" data-testid="screen-error">
          <div>
            <h3>That did not happen</h3>
            <p>{error}</p>
          </div>
        </div>
      )}

      {unknown.length > 0 && (
        <div className="notice notice--warn" data-testid="unknown-elements">
          <div>
            <h3>
              {unknown.length} element{unknown.length === 1 ? '' : 's'} on this screen cannot be shown
            </h3>
            <p>
              {unknown.length === 1 ? 'It was' : 'They were'} not made by this workbench (
              {[...new Set(unknown.map((element) => (element.type === 'unknown' ? element.declaredType : '')))].join(
                ', '
              )}
              ). {unknown.length === 1 ? 'It is' : 'They are'} kept as stored and saved back unchanged.
            </p>
          </div>
        </div>
      )}

      <div className="screen__viewport" ref={viewport} onPointerDown={() => select(null)}>
        <div
          className="screen__sizer"
          style={{ width: doc.canvas.width * scale, height: doc.canvas.height * scale }}
        >
          <div
            className="screen__canvas"
            role="application"
            aria-label={`Screen ${doc.name}`}
            tabIndex={0}
            data-testid="screen-canvas"
            style={{
              width: doc.canvas.width,
              height: doc.canvas.height,
              transform: `scale(${scale})`,
              backgroundSize: `${GRID * 2}px ${GRID * 2}px`,
            }}
            onKeyDown={onKeyDown}
          >
            {doc.canvas.elements.map((element) =>
              element.type === 'unknown' ? null : (
                <div
                  key={element.id}
                  className={`screen__element${element.id === selectedId ? ' is-selected' : ''}`}
                  data-testid="screen-element"
                  data-element-id={element.id}
                  data-element-type={element.type}
                  style={{ left: element.x, top: element.y, width: element.width, height: element.height }}
                  onPointerDown={(event) => onPointerDown(event, element)}
                  onPointerMove={onPointerMove}
                  onPointerUp={onPointerUp}
                  onPointerCancel={onPointerUp}
                >
                  <ElementBody element={element} />
                </div>
              )
            )}
          </div>
        </div>
      </div>
      <p className="editor__footnote muted">
        {doc.canvas.width} × {doc.canvas.height} · shown at {Math.round(scale * 100)} % · drag to
        move, arrow keys to nudge, Delete to remove
      </p>
    </div>
  );
}
