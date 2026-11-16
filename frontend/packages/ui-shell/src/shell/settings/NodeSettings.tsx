import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { api, hasRole, NODE_ROLES, type NodeInfo, type NodeRole, type NodeSettingChange } from '../../api/client';
import { useSession } from '../../data/session';
import { describeError } from './format';

const ROLE_WORDS: Record<NodeRole, { label: string; summary: string }> = {
  workstation: {
    label: 'Workstation',
    summary: 'An engineer’s computer. Reachable from this computer only.',
  },
  edge: {
    label: 'Edge',
    summary: 'A device in the plant that reads the field. Reachable on its networks.',
  },
  server: {
    label: 'Server',
    summary: 'On premises or in the cloud: plant projects and the fleet. Reachable on its networks.',
  },
};

/** What a role does by default (ADR-012 §2.1); a node's own setting wins. */
const ROLE_DEFAULTS: Record<NodeRole, { field_io: boolean; announce: boolean }> = {
  workstation: { field_io: true, announce: false },
  edge: { field_io: true, announce: true },
  server: { field_io: false, announce: false },
};

type Choice = 'role' | 'on' | 'off';
const choiceOf = (setting: { on: boolean; from_role: boolean }): Choice =>
  setting.from_role ? 'role' : setting.on ? 'on' : 'off';
const changeOf = (choice: Choice): NodeSettingChange => (choice === 'role' ? null : choice === 'on');

function SettingChoice({
  label,
  value,
  roleSays,
  disabled,
  onChange,
}: {
  label: string;
  value: Choice;
  roleSays: boolean;
  disabled: boolean;
  onChange: (choice: Choice) => void;
}) {
  return (
    <label className="form__row">
      <span>{label}</span>
      <select aria-label={label} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value as Choice)}>
        <option value="role">As the role says ({roleSays ? 'on' : 'off'})</option>
        <option value="on">On</option>
        <option value="off">Off</option>
      </select>
    </label>
  );
}

/** This installation as a node: its name, site and role (Settings → Node, ADR-012 §2.1). */
export function NodeSettings() {
  const user = useSession((state) => state.user);
  const mayChange = !user || hasRole(user.role, 'admin');
  const [node, setNode] = useState<NodeInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [draft, setDraft] = useState<{
    name: string;
    site: string;
    role: NodeRole;
    field_io: Choice;
    announce: Choice;
  } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const read = await api.getNode();
      setNode(read);
      setDraft(draftOf(read));
    } catch (cause) {
      setError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const changed =
    node !== null &&
    draft !== null &&
    (draft.name.trim() !== node.name ||
      draft.site.trim() !== node.site ||
      draft.role !== node.role ||
      draft.field_io !== choiceOf(node.field_io) ||
      draft.announce !== choiceOf(node.announce));

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!node || !draft) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await api.updateNode({
        name: draft.name,
        site: draft.site,
        role: draft.role,
        field_io: changeOf(draft.field_io),
        announce: changeOf(draft.announce),
      });
      setNode(saved);
      setDraft(draftOf(saved));
      setNotice('Saved.');
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="settings__heading">
        <h2>Node</h2>
      </div>
      <p className="muted">
        Every MythOS installation is a node. Its id never changes, so renaming or re-addressing a device breaks
        nothing that refers to it. Its role chooses its defaults.
      </p>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {node === null || draft === null ? (
        !error && <p className="muted">Loading…</p>
      ) : (
        <>
          <dl className="settings__facts settings__block" data-testid="node-facts">
            <dt>Id</dt>
            <dd className="mono" data-testid="node-id">
              {node.id}
            </dd>
            <dt>Version</dt>
            <dd className="mono">
              {node.version} · module protocol {node.module_protocol}
            </dd>
            <dt>Listens on</dt>
            <dd className="mono" data-testid="node-listening">
              {node.listening ?? 'not reported'}
            </dd>
          </dl>
          {node.field_io_off && (
            <div className="notice" role="status" data-testid="node-field-io-off">
              This node reads no field devices: {node.field_io_off}.
            </div>
          )}
          {node.restart_required && (
            <div className="notice notice--warn" role="status" data-testid="node-restart">
              The {ROLE_WORDS[node.role].label.toLowerCase()} role listens on{' '}
              <span className="mono">{node.listen_address}</span>. It applies when the server is next started; until
              then it listens on <span className="mono">{node.listening}</span>.
            </div>
          )}
          <form className="form settings__block" onSubmit={save} aria-label="Node">
            <label className="form__row">
              <span>Name</span>
              <input
                aria-label="Node name"
                value={draft.name}
                maxLength={64}
                disabled={!mayChange}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </label>
            <label className="form__row">
              <span>Site</span>
              <input
                aria-label="Site"
                value={draft.site}
                maxLength={128}
                placeholder="Plant or area"
                disabled={!mayChange}
                onChange={(e) => setDraft({ ...draft, site: e.target.value })}
              />
            </label>
            <fieldset className="form__row" disabled={!mayChange}>
              <span>Role</span>
              <div>
                {NODE_ROLES.map((role) => (
                  <label key={role} className="settings__choice">
                    <input
                      type="radio"
                      name="node-role"
                      value={role}
                      checked={draft.role === role}
                      onChange={() => setDraft({ ...draft, role })}
                    />
                    <span>
                      <strong>{ROLE_WORDS[role].label}</strong>
                      <small className="muted"> {ROLE_WORDS[role].summary}</small>
                    </span>
                  </label>
                ))}
              </div>
            </fieldset>
            <SettingChoice
              label="Reads field devices"
              value={draft.field_io}
              roleSays={ROLE_DEFAULTS[draft.role].field_io}
              disabled={!mayChange}
              onChange={(field_io) => setDraft({ ...draft, field_io })}
            />
            <SettingChoice
              label="Announces itself on the network"
              value={draft.announce}
              roleSays={ROLE_DEFAULTS[draft.role].announce}
              disabled={!mayChange}
              onChange={(announce) => setDraft({ ...draft, announce })}
            />
            <p className="muted" data-testid="node-announce-note">
              Nothing announces this node yet: announcing is done by the discovery module (ADR-012 step 3), which
              reads this setting.
            </p>
            {notice && <p className="notice">{notice}</p>}
            {mayChange ? (
              <div className="portal__actions">
                <button type="submit" className="btn btn--primary" disabled={busy || !changed || !draft.name.trim()}>
                  Save
                </button>
              </div>
            ) : (
              <p className="muted">Only an administrator changes the node.</p>
            )}
          </form>
        </>
      )}
    </>
  );
}

function draftOf(node: NodeInfo) {
  return {
    name: node.name,
    site: node.site,
    role: node.role,
    field_io: choiceOf(node.field_io),
    announce: choiceOf(node.announce),
  };
}
