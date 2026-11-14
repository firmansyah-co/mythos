import { ChevronDown, ChevronUp } from 'lucide-react';
import type { IoStatus } from '../api/client';
import { useController } from '../data/controller';
import { connectionById, connectionOfTag, currentProblems } from '../data/derive';
import { useLog } from '../data/log';
import { formatClock, formatTimestampNs, viewTag } from '../data/quality';
import { useProjectIo } from '../data/scope';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench, type InspectorTab } from '../data/workbench';
import { ConnectionStateBadge } from '../editors/ConnectionStateBadge';
import { QualityBadge, TagValue } from './QualityBadge';
import { RuntimeGate } from './RuntimeGate';

function TagProperties({ uri }: { uri: string }) {
  const io = useProjectIo();
  const meta = useController((state) => state.tagMeta.get(uri));
  const sample = useTelemetry((state) => state.tags.get(uri));
  const stream = useTelemetry((state) => state.stream);
  const view = viewTag(sample, stream);
  const connection = connectionOfTag(io, uri);
  const online = stream === 'OPEN' && sample !== undefined;

  return (
    <dl className="props" data-testid="tag-properties">
      <dt>Tag</dt>
      <dd className="mono">{uri}</dd>
      <dt>Connection</dt>
      <dd>{connection?.id ?? '—'}</dd>
      <dt>Source</dt>
      <dd>{meta?.description || '—'}</dd>
      <dt>Tag bus slot</dt>
      <dd className="mono">{meta ? meta.slot_id : '—'}</dd>
      <dt>Access</dt>
      <dd>{meta?.access_mode ?? '—'}</dd>
      <dt>Value</dt>
      <dd>
        <TagValue sample={sample} stream={stream} />
      </dd>
      <dt>Data type</dt>
      <dd className="mono">{online && view.kind !== 'never-read' ? sample.type : '—'}</dd>
      <dt>Quality</dt>
      <dd>
        <QualityBadge view={view} />
        {online && sample.qualityCode >= 0 && (
          <span className="mono muted"> 0x{sample.qualityCode.toString(16).padStart(4, '0')}</span>
        )}
      </dd>
      <dt>Last read</dt>
      <dd className="mono">{(online && formatTimestampNs(sample.timestampNs)) || '—'}</dd>
    </dl>
  );
}

function ConnectionProperties({ id }: { id: string }) {
  const io = useProjectIo();
  const connection = connectionById(io, id);
  if (!connection) return <p className="inspector__empty muted">This connection is no longer reported by the controller.</p>;
  return (
    <dl className="props">
      <dt>Connection</dt>
      <dd>{connection.id}</dd>
      <dt>Driver</dt>
      <dd>{connection.driver}</dd>
      <dt>State</dt>
      <dd>
        <ConnectionStateBadge connection={connection} />
      </dd>
      {connection.detail && (
        <>
          <dt>Detail</dt>
          <dd>{connection.detail}</dd>
        </>
      )}
      <dt>Tags</dt>
      <dd className="mono">{connection.tags.length}</dd>
      <dt>Poll interval</dt>
      <dd className="mono">{connection.poll_interval_ms} ms</dd>
      <dt>Cycle deadline</dt>
      <dd className="mono">{connection.cycle_timeout_ms} ms</dd>
    </dl>
  );
}

function ProjectProperties() {
  const project = useWorkbench((state) => state.project);
  if (!project) return null;
  return (
    <dl className="props">
      <dt>Name</dt>
      <dd>{project.name}</dd>
      <dt>Project ID</dt>
      <dd className="mono">{project.id}</dd>
      <dt>Version</dt>
      <dd className="mono">{project.version}</dd>
      <dt>Author</dt>
      <dd>{project.author || '—'}</dd>
    </dl>
  );
}

function PropertiesTab() {
  const selection = useWorkbench((state) => state.selection);
  if (!selection) {
    return <p className="inspector__empty muted">Nothing is selected.</p>;
  }
  if (selection.kind === 'tag') return <TagProperties uri={selection.uri} />;
  if (selection.kind === 'connection') return <ConnectionProperties id={selection.id} />;
  return <ProjectProperties />;
}

function InfoTab() {
  const entries = useLog((state) => state.entries);
  if (entries.length === 0) {
    return <p className="inspector__empty muted">Nothing has been logged in this session.</p>;
  }
  return (
    <table className="grid" aria-label="Session log">
      <tbody>
        {[...entries].reverse().map((entry) => (
          <tr key={entry.id} className={`log log--${entry.level}`}>
            <td className="mono log__time">{formatClock(entry.atMs)}</td>
            <td className="log__level">{entry.level.toUpperCase()}</td>
            <td className="log__text">{entry.text}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function DiagnosticsTab() {
  return <RuntimeGate>{(io) => <Problems io={io} />}</RuntimeGate>;
}

function Problems({ io }: { io: IoStatus }) {
  const select = useWorkbench((state) => state.select);
  const openEditor = useWorkbench((state) => state.openEditor);

  const problems = currentProblems(io);
  if (problems.length === 0) {
    return (
      <p className="inspector__empty muted" data-testid="no-problems">
        {io.connections.length === 0
          ? (io.note ?? 'No field connections are running.')
          : 'The controller reports no I/O problems.'}
      </p>
    );
  }

  return (
    <table className="grid" aria-label="I/O problems">
      <thead>
        <tr>
          <th>Connection</th>
          <th>Tag</th>
          <th>Problem</th>
        </tr>
      </thead>
      <tbody>
        {problems.map((problem) => (
          <tr
            key={problem.id}
            onClick={() =>
              select(
                problem.uri
                  ? { kind: 'tag', uri: problem.uri }
                  : { kind: 'connection', id: problem.connectionId }
              )
            }
            onDoubleClick={() => openEditor('diagnostics', problem.connectionId)}
          >
            <td>{problem.connectionId}</td>
            <td className="mono">{problem.uri ?? '—'}</td>
            <td className="grow" title={problem.text}>
              {problem.text}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const TABS: Array<{ id: InspectorTab; label: string }> = [
  { id: 'properties', label: 'Properties' },
  { id: 'info', label: 'Info' },
  { id: 'diagnostics', label: 'Diagnostics' },
];

/** The inspector window: properties of the selection, the session log, and current problems. */
export function Inspector({ collapsed }: { collapsed: boolean }) {
  const tab = useWorkbench((state) => state.inspectorTab);
  const setTab = useWorkbench((state) => state.setInspectorTab);
  const togglePane = useWorkbench((state) => state.togglePane);
  const problemCount = currentProblems(useProjectIo()).length;

  return (
    <section className="inspector" aria-label="Inspector window">
      <header className="pane__header">
        <span className="pane__title">Inspector</span>
        <div className="inspector__tabs" role="tablist">
          {TABS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className={`inspector__tab${tab === id ? ' is-active' : ''}`}
              onClick={() => {
                setTab(id);
                if (collapsed) togglePane('inspector');
              }}
            >
              {label}
              {id === 'diagnostics' && problemCount > 0 && (
                <span className="inspector__count" data-testid="problem-count">
                  {problemCount}
                </span>
              )}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="icon-btn"
          aria-label={collapsed ? 'Expand inspector' : 'Collapse inspector'}
          onClick={() => togglePane('inspector')}
        >
          {collapsed ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        </button>
      </header>
      {!collapsed && (
        <div className="inspector__body" role="tabpanel">
          {tab === 'properties' && <PropertiesTab />}
          {tab === 'info' && <InfoTab />}
          {tab === 'diagnostics' && <DiagnosticsTab />}
        </div>
      )}
    </section>
  );
}
