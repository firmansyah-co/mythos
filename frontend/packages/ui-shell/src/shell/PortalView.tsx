import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { ArrowRight, FilePlus2, FolderOpen } from 'lucide-react';
import { api, type ProjectConfig } from '../api/client';
import { log } from '../data/log';
import { APP_CHANNEL, APP_VERSION } from '../lib/appVersion';
import { useWorkbench } from '../data/workbench';

type Action = 'open' | 'create';

interface ProjectRow {
  id: string;
  /** `null` when the project's descriptor could not be read. */
  config: ProjectConfig | null;
  problem: string | null;
}

interface Listing {
  rows: ProjectRow[];
  /** `undefined` when the controller did not say which project it runs. */
  activeProjectId: string | null | undefined;
}

/** A project id the storage layer can use as a directory name. */
export function toProjectId(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 48);
}

const PROJECT_ID_PATTERN = /^[a-z0-9][a-z0-9_-]{0,47}$/;

function OpenProject() {
  const openProject = useWorkbench((state) => state.openProject);
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const ids = await api.listProjectIds();
      const [configs, active] = await Promise.all([
        Promise.allSettled(ids.map((id) => api.getProject(id))),
        api.getActiveProjectId().catch(() => undefined),
      ]);
      setListing({
        activeProjectId: active,
        rows: ids.map((id, index) => {
          const result = configs[index];
          return result?.status === 'fulfilled'
            ? { id, config: result.value, problem: null }
            : {
                id,
                config: null,
                problem:
                  result?.status === 'rejected' && result.reason instanceof Error
                    ? result.reason.message
                    : 'could not be read',
              };
        }),
      });
    } catch (cause) {
      setListing(null);
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const chosen = listing?.rows.find((row) => row.id === selected)?.config ?? null;
  const open = (config: ProjectConfig) => {
    log.info(`Project “${config.name}” opened.`);
    openProject(config);
  };

  return (
    <section className="portal__panel" aria-label="Open existing project">
      <h2>Open existing project</h2>

      {error && (
        <div className="notice notice--bad" role="alert">
          <div>
            <h3>The controller cannot be reached</h3>
            <p>{error}</p>
            <p>
              <button type="button" className="btn" onClick={() => void load()}>
                Try again
              </button>
            </p>
          </div>
        </div>
      )}

      {!error && !listing && <p className="muted">Reading projects from the controller…</p>}

      {listing && listing.rows.length === 0 && (
        <div className="notice" data-testid="no-projects">
          <div>
            <h3>There are no projects on this controller</h3>
            <p>Create one to begin.</p>
          </div>
        </div>
      )}

      {listing && listing.rows.length > 0 && (
        <>
          <div className="portal__table">
            <table className="grid" aria-label="Projects">
              <thead>
                <tr>
                  <th>Project</th>
                  <th>ID</th>
                  <th>Author</th>
                  <th>Description</th>
                  <th>Controller</th>
                </tr>
              </thead>
              <tbody>
                {listing.rows.map((row) => (
                  <tr
                    key={row.id}
                    data-testid="project-row"
                    aria-selected={selected === row.id}
                    onClick={() => setSelected(row.id)}
                    onDoubleClick={() => row.config && open(row.config)}
                  >
                    <td>{row.config?.name ?? <span className="muted">unreadable</span>}</td>
                    <td className="mono">{row.id}</td>
                    <td>{row.config?.author || '—'}</td>
                    <td title={row.config?.description ?? row.problem ?? ''}>
                      {row.config ? row.config.description || '—' : row.problem}
                    </td>
                    <td>
                      {listing.activeProjectId === row.id && (
                        <span className="badge badge--good">ACTIVE</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="portal__actions">
            <button
              type="button"
              className="btn btn--primary"
              disabled={!chosen}
              onClick={() => chosen && open(chosen)}
            >
              Open <ArrowRight size={14} aria-hidden />
            </button>
            {!selected && <span className="muted">Select a project, or double-click it.</span>}
            {selected && !chosen && (
              <span className="muted">This project's descriptor could not be read.</span>
            )}
          </div>
        </>
      )}
    </section>
  );
}

function CreateProject() {
  const openProject = useWorkbench((state) => state.openProject);
  const [name, setName] = useState('');
  const [id, setId] = useState('');
  const [idEdited, setIdEdited] = useState(false);
  const [author, setAuthor] = useState('');
  const [description, setDescription] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const effectiveId = idEdited ? id : toProjectId(name);
  const idValid = PROJECT_ID_PATTERN.test(effectiveId);
  const canSubmit = name.trim().length > 0 && idValid && !busy;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      const created = await api.createProject({
        id: effectiveId,
        name: name.trim(),
        author: author.trim(),
        description: description.trim(),
      });
      log.info(`Project “${created.name}” created.`);
      openProject(created);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="portal__panel" aria-label="Create new project">
      <h2>Create new project</h2>
      <form className="form" onSubmit={submit}>
        <label className="form__row">
          <span>Project name</span>
          <input value={name} onChange={(event) => setName(event.target.value)} autoFocus required />
        </label>
        <label className="form__row">
          <span>Project ID</span>
          <input
            className="mono"
            value={effectiveId}
            onChange={(event) => {
              setIdEdited(true);
              setId(event.target.value);
            }}
            aria-invalid={effectiveId.length > 0 && !idValid}
          />
          <small className="muted">
            The directory name on the controller. Lower-case letters, digits, “_” and “-”.
          </small>
        </label>
        <label className="form__row">
          <span>Author</span>
          <input value={author} onChange={(event) => setAuthor(event.target.value)} />
        </label>
        <label className="form__row">
          <span>Description</span>
          <textarea
            rows={3}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
        {error && (
          <p className="form__error" role="alert">
            The project was not created: {error}
          </p>
        )}
        <div className="portal__actions">
          <button type="submit" className="btn btn--primary" disabled={!canSubmit}>
            {busy ? 'Creating…' : 'Create'}
          </button>
        </div>
      </form>
    </section>
  );
}

/** The portal view: the start page from which a project is opened or created. */
export function PortalView() {
  const [action, setAction] = useState<Action>('open');
  const project = useWorkbench((state) => state.project);
  const showProject = useWorkbench((state) => state.showProject);

  return (
    <div className="portal" data-testid="portal-view">
      <aside className="portal__side">
        <div className="portal__brand">
          <img src="/favicon.svg" alt="" />
          <div>
            <div className="portal__product">MythOS Workbench</div>
            <div className="portal__version mono">
              v{APP_VERSION} · channel {APP_CHANNEL}
            </div>
          </div>
        </div>

        <nav className="portal__nav" aria-label="Start">
          <div className="portal__nav-title">Start</div>
          <button
            type="button"
            className={`portal__nav-item${action === 'open' ? ' is-active' : ''}`}
            aria-current={action === 'open'}
            onClick={() => setAction('open')}
          >
            <FolderOpen size={16} aria-hidden /> Open existing project
          </button>
          <button
            type="button"
            className={`portal__nav-item${action === 'create' ? ' is-active' : ''}`}
            aria-current={action === 'create'}
            onClick={() => setAction('create')}
          >
            <FilePlus2 size={16} aria-hidden /> Create new project
          </button>
        </nav>

        {project && (
          <button type="button" className="portal__back" onClick={showProject}>
            <ArrowRight size={14} aria-hidden /> Project view — {project.name}
          </button>
        )}
      </aside>

      <main className="portal__main">{action === 'open' ? <OpenProject /> : <CreateProject />}</main>
    </div>
  );
}
