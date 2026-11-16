import { useController } from '../data/controller';
import { allTagUris } from '../data/derive';
import { formatClock, viewTag } from '../data/quality';
import { useProjectIo, useRuntimeScope } from '../data/scope';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { ModuleMount } from '../modules/ModuleMount';
import { useModuleUi } from '../modules/registry';

const STREAM_WORDS = {
  IDLE: { label: 'OFFLINE', tone: 'neutral' },
  CONNECTING: { label: 'CONNECTING', tone: 'warn' },
  OPEN: { label: 'ONLINE', tone: 'online' },
  CLOSED: { label: 'CONNECTION LOST', tone: 'bad' },
} as const;

/** The "Online tools" task card: the state of the link to the controller and what it carries. */
export function OnlineTools() {
  const stream = useTelemetry((state) => state.stream);
  const tags = useTelemetry((state) => state.tags);
  const lastBatchAtMs = useTelemetry((state) => state.lastBatchAtMs);
  const io = useProjectIo();
  const scope = useRuntimeScope();
  const connectionCount = useController((state) => state.io?.connections.length);
  const activeProjectId = useController((state) => state.activeProjectId);
  const error = useController((state) => state.error);

  const uris = allTagUris(io);
  const counts = { live: 0, stale: 0, unread: 0, missing: 0 };
  if (stream === 'OPEN') {
    for (const uri of uris) {
      const view = viewTag(tags.get(uri), stream);
      if (view.kind === 'live') counts.live += 1;
      else if (view.kind === 'stale') counts.stale += 1;
      else if (view.kind === 'never-read') counts.unread += 1;
      else counts.missing += 1;
    }
  }
  const words = STREAM_WORDS[stream];

  return (
    <div className="card">
      <h3 className="card__title">Telemetry stream</h3>
      <dl className="props">
        <dt>State</dt>
        <dd>
          <span className={`badge badge--${words.tone}`} data-testid="stream-state">
            {words.label}
          </span>
        </dd>
        <dt>Last batch</dt>
        <dd className="mono">{lastBatchAtMs === null ? '—' : formatClock(lastBatchAtMs)}</dd>
      </dl>

      <h3 className="card__title">Tags of this project</h3>
      {scope.kind === 'not-active' ? (
        <p className="card__note" data-testid="card-not-active">
          This project is not running on the controller, so it has no live tags.
        </p>
      ) : (
        <dl className="props">
          <dt>Configured</dt>
          <dd className="mono">{io ? uris.length : '—'}</dd>
          <dt>Live</dt>
          <dd className="mono">{io && stream === 'OPEN' ? counts.live : '—'}</dd>
          <dt>Stale</dt>
          <dd className="mono">{io && stream === 'OPEN' ? counts.stale : '—'}</dd>
          <dt>Never read</dt>
          <dd className="mono">{io && stream === 'OPEN' ? counts.unread : '—'}</dd>
          <dt>No data</dt>
          <dd className="mono">{io && stream === 'OPEN' ? counts.missing : '—'}</dd>
        </dl>
      )}

      <h3 className="card__title">Controller</h3>
      {error ? (
        <p className="card__text" role="alert">
          Unreachable: {error}
        </p>
      ) : (
        <dl className="props">
          <dt>Active project</dt>
          <dd className="mono">
            {activeProjectId === undefined ? '—' : (activeProjectId ?? 'none')}
          </dd>
          <dt>Connections</dt>
          <dd className="mono">{connectionCount ?? '—'}</dd>
        </dl>
      )}
    </div>
  );
}


/** The task cards modules set, in the order they were set. */
export function ModuleTaskCards() {
  const cards = useModuleUi((state) => state.taskCards);
  const project = useWorkbench((state) => state.project);
  if (!project) return null;
  return (
    <>
      {Object.entries(cards).map(([owner, card]) => (
        <div key={owner} className="card" data-testid="module-task-card">
          <h3 className="card__title">{card.item.title}</h3>
          <ModuleMount
            mount={card.item.mount}
            context={{ project: { id: project.id, name: project.name } }}
            deps={[project.id, owner]}
          />
        </div>
      ))}
    </>
  );
}
