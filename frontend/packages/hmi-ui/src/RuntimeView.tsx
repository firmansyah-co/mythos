import { useEffect, useRef, useState } from 'react';
import { LogOut } from 'lucide-react';
import { formatClock } from '@mythos/workbench-sdk/quality';
import { screenToShow, useRuntime } from './runtime';
import { useRuntimeScope } from './scope';
import { goOnline, onStreamEvent, useTelemetry } from './bridge';
import { useWorkbench } from './bridge';
import { ElementBody } from './ScreenElements';

const STREAM_WORDS = {
  IDLE: { label: 'OFFLINE — VALUES ARE NOT BEING READ', tone: 'neutral' },
  CONNECTING: { label: 'CONNECTING…', tone: 'warn' },
  OPEN: { label: 'ONLINE', tone: 'online' },
  CLOSED: { label: 'CONNECTION LOST — VALUES ARE NOT CURRENT', tone: 'bad' },
} as const;

/**
 * The operator runtime: the released screens of the open project, full window, read-only.
 *
 * It shows what was released and nothing else. Whatever stands between the operator and a
 * current value is stated across the top: the link is down, the controller is running another
 * project, the controller cannot be reached, or part of the screen cannot be drawn.
 */
export function RuntimeView() {
  const project = useWorkbench((state) => state.project);
  const wanted = useWorkbench((state) => state.runtimeScreenId);
  const startRuntime = useWorkbench((state) => state.startRuntime);
  const exitRuntime = useWorkbench((state) => state.exitRuntime);
  const phase = useRuntime((state) => state.phase);
  const loadError = useRuntime((state) => state.loadError);
  const screens = useRuntime((state) => state.screens);
  const load = useRuntime((state) => state.load);
  const scope = useRuntimeScope();
  const stream = useTelemetry((state) => state.stream);
  const lastBatchAtMs = useTelemetry((state) => state.lastBatchAtMs);

  const projectId = project?.id ?? null;
  const running = scope.kind === 'active';

  // Read the released screens when the runtime opens.
  useEffect(() => {
    if (projectId !== null) void load(projectId);
  }, [projectId, load]);

  // A running project is read online. Nothing is gained by an operator view that is offline.
  useEffect(() => {
    if (running) goOnline();
  }, [running]);

  // Take up a new release as soon as the controller announces it.
  useEffect(() => {
    if (projectId === null) return;
    return onStreamEvent('SCREENS_RELEASED', (data) => {
      const announced = (data as { project_id?: unknown } | null)?.project_id;
      if (announced === projectId) void load(projectId);
    });
  }, [projectId, load]);

  // An announcement made while the stream was down was missed, so read again on reconnect.
  const wasOpen = useRef(stream === 'OPEN');
  useEffect(() => {
    if (stream === 'OPEN' && !wasOpen.current && projectId !== null) void load(projectId);
    wasOpen.current = stream === 'OPEN';
  }, [stream, projectId, load]);

  const screen = screenToShow(screens, wanted);
  const stage = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const width = screen?.canvas.width;
  const height = screen?.canvas.height;
  useEffect(() => {
    const element = stage.current;
    if (!element || width === undefined || height === undefined) return;
    const fit = () => {
      const availableWidth = element.clientWidth - 24;
      const availableHeight = element.clientHeight - 24;
      setScale(
        availableWidth > 0 && availableHeight > 0
          ? Math.min(availableWidth / width, availableHeight / height)
          : 1
      );
    };
    fit();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [width, height]);

  if (!project) return null;

  const words = STREAM_WORDS[stream];
  const unknown = screen?.canvas.elements.filter((element) => element.type === 'unknown') ?? [];

  return (
    <div className="rt" data-testid="runtime-view">
      <header className="rt__bar">
        <img className="rt__logo" src="/favicon.svg" alt="" />
        <span className="rt__project">{project.name}</span>
        <span className="rt__mode">RUNTIME</span>
        <span className={`badge badge--${words.tone}`} data-testid="runtime-stream">
          {words.label}
        </span>
        <span className="rt__updated mono" data-testid="runtime-updated">
          {stream === 'OPEN' && lastBatchAtMs !== null ? `updated ${formatClock(lastBatchAtMs)}` : ''}
        </span>
        <button type="button" className="rt__exit" onClick={exitRuntime}>
          <LogOut size={14} aria-hidden /> Exit runtime
        </button>
      </header>

      {screens.length > 1 && (
        <nav className="rt__screens" aria-label="Screens">
          {screens.map((candidate) => (
            <button
              key={candidate.id}
              type="button"
              aria-current={candidate.id === screen?.id}
              className={`rt__screen${candidate.id === screen?.id ? ' is-active' : ''}`}
              onClick={() => startRuntime(candidate.id)}
            >
              {candidate.name}
            </button>
          ))}
        </nav>
      )}

      {scope.kind === 'unreachable' && (
        <div className="rt__alert rt__alert--bad" role="alert" data-testid="runtime-unreachable">
          The controller cannot be reached. Nothing on this screen is current. ({scope.error})
        </div>
      )}
      {scope.kind === 'not-active' && (
        <div className="rt__alert rt__alert--warn" role="alert" data-testid="runtime-not-active">
          {scope.activeProjectId
            ? `The controller is running “${scope.activeProjectId}”, not this project. No values are shown.`
            : 'The controller is running no project. No values are shown.'}
        </div>
      )}
      {unknown.length > 0 && (
        <div className="rt__alert rt__alert--warn" role="alert" data-testid="runtime-incomplete">
          This screen is incomplete: {unknown.length} element{unknown.length === 1 ? '' : 's'} cannot
          be shown.
        </div>
      )}

      <main className="rt__stage" ref={stage}>
        {phase === 'unreadable' ? (
          <div className="rt__message rt__message--bad" role="alert" data-testid="runtime-unreadable">
            <h2>The released screens could not be read</h2>
            <p>{loadError}</p>
          </div>
        ) : phase !== 'ready' ? (
          <div className="rt__message">Reading the released screens…</div>
        ) : !screen ? (
          <div className="rt__message" data-testid="runtime-nothing-released">
            <h2>No screens have been released for this project</h2>
            <p>Screens appear here after an engineer releases them.</p>
          </div>
        ) : (
          <div className="rt__sizer" style={{ width: screen.canvas.width * scale, height: screen.canvas.height * scale }}>
            <div
              className="rt__canvas"
              data-testid="runtime-canvas"
              data-screen-id={screen.id}
              style={{
                width: screen.canvas.width,
                height: screen.canvas.height,
                transform: `scale(${scale})`,
              }}
            >
              {screen.canvas.elements.map((element) =>
                element.type === 'unknown' ? null : (
                  <div
                    key={element.id}
                    className="rt__element"
                    data-testid="runtime-element"
                    style={{ left: element.x, top: element.y, width: element.width, height: element.height }}
                  >
                    <ElementBody element={element} />
                  </div>
                )
              )}
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
