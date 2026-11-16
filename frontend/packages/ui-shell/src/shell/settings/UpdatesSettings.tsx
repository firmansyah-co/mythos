import { useCallback, useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, Download, RefreshCw, RotateCcw } from 'lucide-react';
import {
  api,
  CHANNELS,
  hasRole,
  type Installed,
  type PublishedRelease,
  type ReleaseChannel,
  type ReleaseList,
  type UpdateStatus,
} from '../../api/client';
import { useSession } from '../../data/session';
import { describeError, formatTime } from './format';

export const CHANNEL_MEANING: Record<ReleaseChannel, string> = {
  release: 'Releases only.',
  rc: 'Release candidates as well as releases.',
  dev: 'Every build, development builds included.',
};

function Running({ status }: { status: UpdateStatus }) {
  const install = status.installation;
  return (
    <dl className="settings__facts" data-testid="update-status">
      <dt>Running</dt>
      <dd className="mono">
        {status.running.installed_version ?? `built as ${status.running.version}`}
      </dd>
      <dt>Platform</dt>
      <dd className="mono">{status.running.target}</dd>
      {install ? (
        <>
          <dt>Installation</dt>
          <dd className="mono">{install.home}</dd>
          <dt>Current</dt>
          <dd className="mono" data-testid="current-version">{install.current_version ?? 'none'}</dd>
          <dt>Previous</dt>
          <dd className="mono">{install.previous_version ?? 'none'}</dd>
          <dt>Installed</dt>
          <dd className="mono">{install.installed.length > 0 ? install.installed.join(', ') : 'none'}</dd>
          <dt>Feed</dt>
          <dd className="mono">{install.feed}</dd>
        </>
      ) : (
        <>
          <dt>Installation</dt>
          <dd data-testid="no-installation">{status.note}</dd>
        </>
      )}
    </dl>
  );
}

function Follow({
  status,
  releases,
  onChanged,
}: {
  status: UpdateStatus;
  releases: PublishedRelease[];
  onChanged: (status: UpdateStatus) => void;
}) {
  const install = status.installation!;
  const [mode, setMode] = useState<'channel' | 'version'>(install.pinned_version ? 'version' : 'channel');
  const [channel, setChannel] = useState<ReleaseChannel>(install.channel);
  const [version, setVersion] = useState(install.pinned_version ?? releases.find((r) => r.has_bundle)?.version ?? '');
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    setError(null);
    try {
      onChanged(await api.setFollow(channel, mode === 'version' ? version : null));
    } catch (cause) {
      setError(describeError(cause));
    }
  };

  return (
    <section className="form settings__block" aria-label="What to follow">
      <h3>What to follow</h3>
      <label className="settings__check">
        <input type="radio" name="follow" checked={mode === 'channel'} onChange={() => setMode('channel')} />
        Follow a channel
      </label>
      <div className="settings__indent">
        <select
          aria-label="Channel"
          value={channel}
          disabled={mode !== 'channel'}
          onChange={(e) => setChannel(e.target.value as ReleaseChannel)}
        >
          {CHANNELS.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <span className="field__hint">{CHANNEL_MEANING[channel]}</span>
      </div>
      <label className="settings__check">
        <input type="radio" name="follow" checked={mode === 'version'} onChange={() => setMode('version')} />
        Stay on one version
      </label>
      <div className="settings__indent">
        <select
          aria-label="Pinned version"
          value={version}
          disabled={mode !== 'version'}
          onChange={(e) => setVersion(e.target.value)}
        >
          {releases.map((r) => (
            <option key={r.version} value={r.version} disabled={!r.has_bundle}>
              {r.version} ({r.channel}){r.has_bundle ? '' : ' — no bundle for this platform'}
            </option>
          ))}
        </select>
      </div>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      <div className="portal__actions">
        <button type="button" className="btn" onClick={() => void save()} disabled={mode === 'version' && !version}>
          Save
        </button>
      </div>
    </section>
  );
}

function ReleaseRow({
  release,
  mayInstall,
  busy,
  onInstall,
}: {
  release: PublishedRelease;
  mayInstall: boolean;
  busy: boolean;
  onInstall: (version: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <li className="settings__release" data-testid="release">
      <div className="settings__release-head">
        <button
          type="button"
          className="icon-btn"
          aria-expanded={open}
          aria-label={`${open ? 'Hide' : 'Show'} what ${release.version} contains`}
          onClick={() => setOpen(!open)}
        >
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span className="mono settings__release-version">{release.version}</span>
        <span className={`settings__badge settings__badge--${release.channel}`}>{release.channel}</span>
        {release.current && <span className="settings__badge settings__badge--current">current</span>}
        {release.installed && !release.current && <span className="settings__badge">installed</span>}
        {!release.has_bundle && <span className="settings__badge settings__badge--muted">no bundle for this platform</span>}
        <span className="muted settings__release-date">
          {release.published_at ? new Date(release.published_at).toLocaleString() : 'date not published'}
        </span>
        {mayInstall && release.has_bundle && !release.current && (
          <button type="button" className="btn btn--flat" disabled={busy} onClick={() => onInstall(release.version)}>
            <Download size={13} aria-hidden /> Install
          </button>
        )}
      </div>
      {open && (
        <div className="settings__release-body" data-testid="release-notes">
          <strong>{release.name ?? release.tag}</strong>
          <pre className="settings__notes">{release.notes ?? 'This release published no notes.'}</pre>
          {release.url && (
            <a href={release.url} target="_blank" rel="noreferrer">
              Read it on the release page
            </a>
          )}
        </div>
      )}
    </li>
  );
}

/** The server's version, what it follows, and every published version (Settings → Updates). */
export function UpdatesSettings() {
  const user = useSession((state) => state.user);
  const mayChange = !user || hasRole(user.role, 'admin');
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [list, setList] = useState<ReleaseList | null>(null);
  const [feedError, setFeedError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Installed | null>(null);

  const loadReleases = useCallback(async (refresh: boolean) => {
    try {
      setList(await api.listReleases(refresh));
      setFeedError(null);
    } catch (cause) {
      setFeedError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    api.getUpdates().then(setStatus, (cause: unknown) => setError(describeError(cause)));
    void loadReleases(false);
  }, [loadReleases]);

  const run = async (action: () => Promise<Installed>) => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const result = await action();
      setDone(result);
      setStatus(result.status);
      await loadReleases(false);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBusy(false);
    }
  };

  const install = status?.installation ?? null;
  const target = list?.target_version ?? null;
  const targetRelease = list?.releases.find((r) => r.version === target);

  return (
    <>
      <h2>Updates</h2>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {status && <Running status={status} />}
      {!mayChange && <p className="muted">Only an administrator changes the version.</p>}

      {done && (
        <div className="notice" data-testid="update-done">
          <div>
            {done.version} is now current.
            {done.reported && <> The installed program reports “{done.reported}”.</>}
            {done.restart_required && install && (
              <>
                {' '}
                This server still runs {status?.running.installed_version ?? status?.running.version}; restart it
                from <span className="mono">{install.home}/current/bin</span> to run {done.version}.
              </>
            )}
          </div>
        </div>
      )}

      {install && mayChange && list && (
        <>
          <Follow key={`${install.channel}:${install.pinned_version}`} status={status!} releases={list.releases} onChanged={setStatus} />
          <div className="portal__actions">
            {target ? (
              <button
                type="button"
                className="btn btn--primary"
                disabled={busy || targetRelease?.current || targetRelease?.has_bundle === false}
                onClick={() => void run(() => api.installUpdate())}
              >
                <Download size={13} aria-hidden />{' '}
                {targetRelease?.current
                  ? `${target} is current`
                  : busy
                  ? 'Installing…'
                  : `Install ${target}`}
              </button>
            ) : (
              <span className="muted">Nothing is published for what this installation follows.</span>
            )}
            {install.previous_version && (
              <button type="button" className="btn" disabled={busy} onClick={() => void run(() => api.rollbackUpdate())}>
                <RotateCcw size={13} aria-hidden /> Roll back to {install.previous_version}
              </button>
            )}
          </div>
        </>
      )}

      <section className="settings__block" aria-label="Published versions">
        <div className="settings__heading">
          <h3>Published versions</h3>
          <button type="button" className="btn btn--flat" onClick={() => void loadReleases(true)}>
            <RefreshCw size={13} aria-hidden /> Check now
          </button>
        </div>
        {feedError ? (
          <p className="form__error" role="alert" data-testid="feed-error">
            The release feed could not be read: {feedError}
          </p>
        ) : !list ? (
          <p className="muted">Asking the release feed…</p>
        ) : list.releases.length === 0 ? (
          <p className="muted">Nothing has been published yet.</p>
        ) : (
          <>
            <p className="muted">
              From {list.feed}, read {formatTime(list.fetched_unix_s)}.
            </p>
            <ul className="settings__releases">
              {list.releases.map((release) => (
                <ReleaseRow
                  key={release.version}
                  release={release}
                  mayInstall={mayChange && install !== null}
                  busy={busy}
                  onInstall={(version) => void run(() => api.installUpdate(version))}
                />
              ))}
            </ul>
          </>
        )}
      </section>
    </>
  );
}
