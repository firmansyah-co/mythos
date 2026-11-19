import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { Download, Plus, RefreshCw } from 'lucide-react';
import {
  api,
  hasRole,
  type DeviceComparison,
  type PairedNode,
  type PlantDevice,
  type RemoteLink,
} from '../api/client';
import { log } from '../data/log';
import { useSession } from '../data/session';
import { useWorkbench } from '../data/workbench';

/** What a comparison says, in words and tone; unreachable is never "the same". */
const STATES: Record<DeviceComparison['state'], { label: string; tone: string }> = {
  same: { label: 'SAME AS PROJECT', tone: 'good' },
  differs: { label: 'DIFFERS', tone: 'warn' },
  not_running: { label: 'HELD, NOT RUNNING', tone: 'warn' },
  never_downloaded: { label: 'NEVER DOWNLOADED', tone: 'neutral' },
  unreachable: { label: 'UNREACHABLE', tone: 'bad' },
  not_assigned: { label: 'NOT ASSIGNED', tone: 'neutral' },
  unreadable: { label: 'UNREADABLE', tone: 'bad' },
};

/** A link to another device: its tags are current only while it is connected. */
const LINKS: Record<RemoteLink['state'], { label: string; tone: string }> = {
  connected: { label: 'LIVE', tone: 'good' },
  connecting: { label: 'CONNECTING', tone: 'neutral' },
  lost: { label: 'CONNECTION LOST — NOT CURRENT', tone: 'bad' },
};

/** How often the links are read again. */
const LINKS_EVERY_MS = 2000;

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * The plant (ADR-012 §2.4): every MythOS device of this project, the node each is assigned to,
 * whether that node runs what the project says, and downloading one or all.
 */
/**
 * Whether this node reads the device's tags, named `device:tag`. Only a device of the plant this
 * node runs, on another node, is read here; for any other there is nothing to say, and nothing is
 * shown as if it were.
 */
function LinkCell({ link, known }: { link: RemoteLink | undefined; known: boolean }) {
  if (!known) return <span className="muted">unknown</span>;
  if (!link) {
    return (
      <span className="muted" title="Read here only when this node runs this plant and the device is on another node.">
        not read here
      </span>
    );
  }
  const words = LINKS[link.state];
  return (
    <span className={`badge badge--${words.tone}`} data-testid="plant-device-link" title={link.reason ?? undefined}>
      {words.label}
      {link.state === 'connected' && ` · ${link.tags} tag${link.tags === 1 ? '' : 's'}`}
    </span>
  );
}

export function PlantEditor() {
  const project = useWorkbench((state) => state.project);
  const openProject = useWorkbench((state) => state.openProject);
  const user = useSession((state) => state.user);
  const mayChange = !user || hasRole(user.role, 'engineer');
  const [devices, setDevices] = useState<PlantDevice[] | null>(null);
  const [implicit, setImplicit] = useState(false);
  const [peers, setPeers] = useState<PairedNode[]>([]);
  const [thisNode, setThisNode] = useState<{ id: string; name: string } | null>(null);
  const [states, setStates] = useState<Record<string, DeviceComparison>>({});
  const [links, setLinks] = useState<Record<string, RemoteLink> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [adding, setAdding] = useState({ id: '', name: '' });
  const projectId = project?.id ?? null;

  const compareAll = useCallback(async (list: PlantDevice[], id: string) => {
    for (const device of list) {
      try {
        const result = await api.compareDevice(id, device.id);
        setStates((s) => ({ ...s, [device.id]: result }));
      } catch (cause) {
        setStates((s) => ({ ...s, [device.id]: { device: device.id, state: 'unreachable', error: errorText(cause) } }));
      }
    }
  }, []);

  const load = useCallback(async () => {
    if (!projectId) return;
    try {
      const plant = await api.getPlant(projectId);
      setDevices(plant.devices);
      setImplicit(plant.implicit);
      setError(null);
      void compareAll(plant.devices, projectId);
    } catch (cause) {
      setError(errorText(cause));
    }
    try {
      setPeers((await api.listPairings()).peers);
      const node = await api.getNode();
      setThisNode({ id: node.id, name: node.name });
    } catch {
      // Without them the node column offers what is assigned already.
    }
  }, [projectId, compareAll]);

  useEffect(() => {
    void load();
  }, [load]);

  // The tags of other devices read on this node, and whether each link is up.
  useEffect(() => {
    let alive = true;
    const read = async () => {
      try {
        const answer = await api.getRemoteLinks();
        if (alive) setLinks(Object.fromEntries(answer.devices.map((l) => [l.device, l])));
      } catch {
        if (alive) setLinks(null);
      }
    };
    void read();
    const timer = window.setInterval(() => void read(), LINKS_EVERY_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  if (!project) return null;

  const assign = async (device: PlantDevice, node: string) => {
    try {
      await api.setPlantDevice(project.id, device.id, device.name, node === '' ? null : node);
      await load();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  const download = async (device: PlantDevice) => {
    const where = device.node?.name ?? device.node?.id ?? 'its node';
    if (!window.confirm(`Download ${device.name} to ${where}? It changes what that controller reads and shows.`)) return;
    setNotice(null);
    try {
      const result = await api.downloadDevice(project.id, device.id);
      setNotice(`${device.name}: downloaded.`);
      log.info(`Device “${device.name}” downloaded to ${where} (${result.state}).`);
    } catch (cause) {
      setError(`${device.name}: ${errorText(cause)}`);
    }
    if (devices) void compareAll(devices, project.id);
  };

  const downloadAll = async () => {
    if (!window.confirm('Download every assigned device? It changes what each of those controllers reads and shows.')) return;
    setNotice(null);
    try {
      const result = await api.downloadPlant(project.id);
      const words = result.devices.map((d) => `${d.device}: ${d.state}${d.error ? ` (${d.error})` : ''}`).join('; ');
      setNotice(result.complete ? `Every device downloaded. ${words}` : `Not every device downloaded: ${words}`);
      log.info(`Download all: ${words}`);
    } catch (cause) {
      setError(errorText(cause));
    }
    if (devices) void compareAll(devices, project.id);
  };

  const add = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api.setPlantDevice(project.id, adding.id.trim(), adding.name.trim(), null);
      setAdding({ id: '', name: '' });
      await load();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  const openDevice = async (device: PlantDevice) => {
    if (device.project === project.id) return;
    try {
      openProject(await api.getProject(device.project));
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  return (
    <div className="editor" data-testid="plant-editor">
      <div className="editor__toolbar">
        <button type="button" className="btn btn--flat" onClick={() => void load()}>
          <RefreshCw size={13} aria-hidden /> Compare again
        </button>
        {mayChange && (
          <button type="button" className="btn btn--flat" onClick={() => void downloadAll()}>
            <Download size={13} aria-hidden /> Download all
          </button>
        )}
      </div>
      <div className="editor__body">
        <p className="muted">
          Every MythOS device of this plant, the node each runs on, and whether it runs what this project says. A device
          runs on this node or on one paired with it (Settings → Pairing).
          {implicit && ' This project is one device until another is added.'}
        </p>
        {error && (
          <p className="form__error" role="alert">
            {error}
          </p>
        )}
        {notice && (
          <p className="notice" role="status" data-testid="plant-notice">
            {notice}
          </p>
        )}
        {devices === null ? (
          !error && <p className="muted">Reading the plant…</p>
        ) : (
          <table className="grid">
            <thead>
              <tr>
                <th>Device</th>
                <th>Node</th>
                <th>Compared with its node</th>
                <th>Its tags here</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {devices.map((device) => {
                const state = states[device.id];
                const words = state ? STATES[state.state] : null;
                return (
                  <tr key={device.id} data-testid="plant-device">
                    <td>
                      <strong>{device.name}</strong>
                      <div className="mono muted">{device.project}</div>
                    </td>
                    <td>
                      <select
                        aria-label={`Node of ${device.name}`}
                        value={device.node?.id ?? ''}
                        disabled={!mayChange}
                        onChange={(e) => void assign(device, e.target.value)}
                      >
                        <option value="">not assigned</option>
                        {thisNode && <option value={thisNode.id}>this node ({thisNode.name})</option>}
                        {peers.map((peer) => (
                          <option key={peer.id} value={peer.id}>
                            {peer.name} ({peer.role})
                          </option>
                        ))}
                        {device.node && device.node.id !== thisNode?.id && !peers.some((p) => p.id === device.node?.id) && (
                          <option value={device.node.id}>{device.node.name ?? device.node.id} (no longer paired)</option>
                        )}
                      </select>
                    </td>
                    <td>
                      {words ? (
                        <span className={`badge badge--${words.tone}`} data-testid="plant-device-state" title={state?.error}>
                          {words.label}
                        </span>
                      ) : (
                        <span className="muted">comparing…</span>
                      )}
                    </td>
                    <td>
                      <LinkCell link={links?.[device.id]} known={links !== null} />
                    </td>
                    <td className="settings__row-actions">
                      {device.project !== project.id && (
                        <button type="button" className="btn btn--flat" onClick={() => void openDevice(device)}>
                          Open
                        </button>
                      )}
                      {mayChange && device.node && (
                        <button type="button" className="btn btn--flat" onClick={() => void download(device)}>
                          Download
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {mayChange && (
          <form className="settings__inline" onSubmit={add} aria-label="Add a device">
            <input aria-label="Device id" placeholder="edge1" value={adding.id} onChange={(e) => setAdding({ ...adding, id: e.target.value })} />
            <input aria-label="Device name" placeholder="Boiler edge 1" value={adding.name} onChange={(e) => setAdding({ ...adding, name: e.target.value })} />
            <button type="submit" className="btn" disabled={!adding.id.trim() || !adding.name.trim()}>
              <Plus size={13} aria-hidden /> Add device
            </button>
          </form>
        )}
      </div>
    </div>
  );
}
