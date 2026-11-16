import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { PackagePlus, RefreshCw, Trash2 } from 'lucide-react';
import { api, hasRole, type ModuleList, type ModuleRow } from '../../api/client';
import { useSession } from '../../data/session';
import { describeError } from './format';

function State({ row, list }: { row: ModuleRow; list: ModuleList }) {
  if (!row.enabled) return <span className="settings__badge settings__badge--muted">disabled</span>;
  if (row.kind === 'service') {
    const service = list.services?.find((s) => s.id === row.id);
    if (!service) return <span className="settings__badge">not started</span>;
    return (
      <span title={service.last_error ?? undefined}>
        {service.pid !== null ? (
          <span className="settings__badge settings__badge--current">running · pid {service.pid}</span>
        ) : (
          <span className="settings__badge">{service.last_error ? 'failed' : 'idle'}</span>
        )}{' '}
        {service.starts > 1 && <span className="muted">started {service.starts}×</span>}
      </span>
    );
  }
  return <span className="settings__badge settings__badge--current">in use</span>;
}

function Install({ onInstalled }: { onInstalled: (message: string) => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [url, setUrl] = useState('');
  const [sha, setSha] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<{ installed: string; version: string; enabled: boolean }>) => {
    setBusy(true);
    setError(null);
    try {
      const done = await action();
      onInstalled(
        `${done.installed} ${done.version} is installed${done.enabled ? ' and in use' : ', but disabled'}.`
      );
      setFile(null);
      setUrl('');
      setSha('');
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="form settings__block" aria-label="Install a module">
      <h3>Install a module</h3>
      <p className="muted">
        A module package is a zip with <span className="mono">module.json</span>, a checksum of every file, and an
        executable per platform (<span className="mono">mythos module pack</span> builds one). It is checked file by
        file, and its executable for this platform must run and say it is the module the package names.
      </p>
      <form
        className="settings__inline"
        onSubmit={(e: FormEvent) => {
          e.preventDefault();
          if (file) void run(() => api.installModule(file));
        }}
      >
        <input
          type="file"
          accept=".zip,application/zip"
          aria-label="Module package"
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
        />
        <button type="submit" className="btn" disabled={!file || busy}>
          <PackagePlus size={13} aria-hidden /> Install from file
        </button>
      </form>
      <form
        className="settings__inline"
        onSubmit={(e: FormEvent) => {
          e.preventDefault();
          void run(() => api.installModuleFromUrl(url.trim(), sha.trim()));
        }}
      >
        <input aria-label="Package URL" placeholder="https://…/module.zip" value={url} onChange={(e) => setUrl(e.target.value)} />
        <input aria-label="Package SHA-256" placeholder="SHA-256" className="mono" value={sha} onChange={(e) => setSha(e.target.value)} />
        <button type="submit" className="btn" disabled={!url.trim() || !sha.trim() || busy}>
          Install from URL
        </button>
      </form>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

/** Every module the controller found, switched on or off, installed or removed (Settings → Modules). */
export function ModulesSettings() {
  const user = useSession((state) => state.user);
  const mayChange = !user || hasRole(user.role, 'admin');
  const [list, setList] = useState<ModuleList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setList(await api.listModules());
    } catch (cause) {
      setError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const act = async (action: () => Promise<unknown>, message?: string) => {
    setError(null);
    setNotice(null);
    try {
      await action();
      if (message) setNotice(message);
    } catch (cause) {
      setError(describeError(cause));
    }
    await load();
  };

  const rows = list?.modules ?? [];

  return (
    <>
      <div className="settings__heading">
        <h2>Modules</h2>
        <button type="button" className="btn btn--flat" onClick={() => void act(() => api.rescanModules())}>
          <RefreshCw size={13} aria-hidden /> Look again
        </button>
      </div>
      <p className="muted">
        Drivers read devices; services serve parts of MythOS such as the HMI. A disabled module stays installed but
        is not used: a disabled driver's connections stop and say why.
        {list?.target && (
          <>
            {' '}
            This controller runs on <span className="mono">{list.target}</span>.
          </>
        )}
      </p>
      {!mayChange && <p className="muted">Only an administrator changes modules.</p>}
      {notice && <div className="notice">{notice}</div>}
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {list === null ? (
        <p className="muted">Loading…</p>
      ) : rows.length === 0 ? (
        <p className="muted">No module was found.</p>
      ) : (
        <table className="grid settings__users" aria-label="Modules">
          <thead>
            <tr>
              <th>Module</th>
              <th>Kind</th>
              <th>Provides</th>
              <th>Version</th>
              <th>State</th>
              <th>Enabled</th>
              <th aria-label="Remove" />
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id} data-testid="module-row">
                <td>
                  <strong>{row.name}</strong>
                  <div className="muted mono" title={row.path}>
                    {row.id}
                  </div>
                </td>
                <td>{row.kind}</td>
                <td className="mono">{row.provides}</td>
                <td className="mono">{row.version}</td>
                <td>
                  <State row={row} list={list} />
                </td>
                <td>
                  <label className="settings__check">
                    <input
                      type="checkbox"
                      checked={row.enabled}
                      disabled={!mayChange}
                      aria-label={`${row.id} enabled`}
                      onChange={(e) => void act(() => api.setModuleEnabled(row.id, e.target.checked))}
                    />
                    enabled
                  </label>
                </td>
                <td>
                  {mayChange && row.removable ? (
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={`Remove ${row.id}`}
                      onClick={() => {
                        if (window.confirm(`Remove ${row.name} (${row.id}) from this controller?`)) {
                          void act(() => api.removeModule(row.id), `${row.id} was removed.`);
                        }
                      }}
                    >
                      <Trash2 size={13} />
                    </button>
                  ) : (
                    !row.removable && <span className="muted" title="Part of this MythOS installation; it can be disabled, not removed">built in</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {list && list.problems.length > 0 && (
        <section className="settings__block" aria-label="Module problems">
          <h3>Could not be used</h3>
          <ul>
            {list.problems.map((p) => (
              <li key={p.path}>
                <span className="mono">{p.path}</span>: {p.message}
              </li>
            ))}
          </ul>
        </section>
      )}
      {mayChange && <Install onInstalled={(message) => void act(async () => undefined, message)} />}
    </>
  );
}
