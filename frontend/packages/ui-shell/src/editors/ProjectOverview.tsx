import { useState } from 'react';
import { api } from '../api/client';
import { useController } from '../data/controller';
import { log } from '../data/log';
import { useWorkbench } from '../data/workbench';

/** The open project's properties, and whether the controller is running it. */
export function ProjectOverview() {
  const project = useWorkbench((state) => state.project);
  const activeProjectId = useController((state) => state.activeProjectId);
  const io = useController((state) => state.io);
  const error = useController((state) => state.error);
  const refresh = useController((state) => state.refresh);
  const [busy, setBusy] = useState(false);
  const [activateError, setActivateError] = useState<string | null>(null);
  const [activatedNow, setActivatedNow] = useState(false);

  if (!project) return null;

  const isActive = activeProjectId === project.id;

  const activate = async () => {
    setBusy(true);
    setActivateError(null);
    try {
      await api.activateProject(project.id);
      setActivatedNow(true);
      log.info(`Project “${project.name}” set as the controller's active project.`);
      await refresh();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setActivateError(message);
      log.error(`Activating “${project.name}” failed: ${message}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="editor__scroll">
      <dl className="props props--wide">
        <dt>Name</dt>
        <dd>{project.name}</dd>
        <dt>Project ID</dt>
        <dd className="mono">{project.id}</dd>
        <dt>Version</dt>
        <dd className="mono">{project.version}</dd>
        <dt>Author</dt>
        <dd>{project.author || '—'}</dd>
        <dt>Description</dt>
        <dd>{project.description || '—'}</dd>
        <dt>On the controller</dt>
        <dd data-testid="project-run-state">
          {error ? (
            <span className="badge badge--bad">CONTROLLER UNREACHABLE</span>
          ) : activeProjectId === undefined ? (
            <span className="badge badge--neutral">UNKNOWN</span>
          ) : isActive ? (
            <span className="badge badge--good">ACTIVE PROJECT</span>
          ) : (
            <span className="badge badge--neutral">NOT ACTIVE</span>
          )}
        </dd>
        {isActive && io && (
          <>
            <dt>Field I/O</dt>
            <dd>
              {io.connections.length > 0
                ? `${io.connections.length} connection(s)`
                : (io.note ?? 'No connections')}
            </dd>
            {io.source && (
              <>
                <dt>I/O configuration</dt>
                <dd className="mono">{io.source}</dd>
              </>
            )}
          </>
        )}
      </dl>

      {!error && activeProjectId !== undefined && !isActive && (
        <div className="notice notice--warn" data-testid="not-active-notice">
          <div>
            <h3>This project is not running on the controller</h3>
            <p>
              {activeProjectId
                ? `The controller's active project is “${activeProjectId}”. Live tags belong to that project, so none are shown here.`
                : 'The controller has no active project, so it reads no field devices.'}
            </p>
            <p>
              <button type="button" className="btn btn--primary" disabled={busy} onClick={activate}>
                {busy ? 'Activating…' : 'Make this the active project'}
              </button>
            </p>
            {activateError && <p role="alert">Activation failed: {activateError}</p>}
          </div>
        </div>
      )}

      {activatedNow && isActive && (
        <div className="notice notice--warn" data-testid="restart-notice">
          <div>
            <h3>Restart the server to load this project's field I/O</h3>
            <p>
              The project is now the active one, but the I/O configuration is read only when the
              server starts. Until then the controller keeps running what it loaded before.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
