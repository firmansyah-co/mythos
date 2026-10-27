import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Boxes, Plus, Trash2, Save, Rocket, Snowflake, AlertTriangle, RotateCcw } from 'lucide-react';
import { STANDARD_UDT_MAPPINGS } from '@mythos/ui-faceplate-lib';
import { useHmiStore } from '../../stores/useHmiStore';
import { useFaceplateStore } from '../../stores/useFaceplateStore';
import {
  BindingAccess,
  FaceplateType,
  FaceplateTypeSummary,
  FaceplateView,
  FaceplateViewKind,
  ResolvedInstance,
  UdtDefinition,
  UdtKind,
  versionLabel,
} from '../../types/faceplate';

export interface FaceplateTypeDesignerProps {
  /** Master type id from the Dockview panel params; `new` opens the create form. */
  typeId?: string;
  className?: string;
}

const VIEW_KINDS: Array<{ kind: FaceplateViewKind; label: string }> = [
  { kind: 'PID_SYMBOL', label: 'P&ID Symbol (Tier 1)' },
  { kind: 'SUMMARY_CARD', label: 'Summary Card (Tier 2)' },
  { kind: 'DIAGNOSTIC_POPUP', label: 'Diagnostic Faceplate (Tier 3)' },
  { kind: 'ALARM', label: 'Alarm Archetype' },
];

const UDT_KINDS: UdtKind[] = ['STRUCT', 'ENUM', 'ALIAS'];

/**
 * The keys a faceplate component can consume come from the faceplate library's own
 * UDT signature tables, so the designer never invents a binding target.
 */
const FACEPLATE_KEYS: string[] = Array.from(
  new Set(Object.values(STANDARD_UDT_MAPPINGS).flat().map((mapping) => mapping.faceplateKey))
).sort();

const emptyUdt = (): UdtDefinition => ({
  name: '',
  kind: 'STRUCT',
  members: [{ name: '', dataType: 'BOOL' }],
  enumConstants: [],
  baseType: null,
});

/** The elementary IEC types an inline UDT member may use. */
const UDT_DATA_TYPES = [
  'BOOL',
  'INT',
  'DINT',
  'REAL',
  'TIME',
  'STRING',
  'WORD',
  'DWORD',
  'BYTE',
  'LREAL',
  'UDINT',
];

/**
 * Renders one instance purely from the payload the daemon resolved against the master at read
 * time. An instance holds no views of its own, which is what lets a release propagate without
 * touching instances.
 */
export const FaceplateInstanceCard: React.FC<{ instance: ResolvedInstance }> = ({ instance }) => {
  const masterMissing = instance.views === null || instance.udt === null;

  return (
    <div
      className="space-y-1.5 rounded border border-neutral-800 bg-neutral-900/80 p-2.5"
      data-testid="faceplate-instance-card"
      data-instance-id={instance.id}
      data-resolved-version={instance.resolvedVersion ?? 'unavailable'}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-mono text-[11px] text-cyan-400">{instance.name}</span>
        <span className="flex items-center gap-1">
          {instance.frozen && (
            <span
              data-testid="instance-frozen-badge"
              className="flex items-center gap-0.5 rounded bg-sky-950 px-1 py-0.5 text-[9px] font-semibold text-sky-300"
              title={`Frozen at ${versionLabel(instance.typeVersion)}; follows that version until unfrozen`}
            >
              <Snowflake className="h-2.5 w-2.5" /> FROZEN
            </span>
          )}
          <span className="font-mono text-[10px] text-neutral-500">
            {versionLabel(instance.resolvedVersion)}
          </span>
        </span>
      </div>

      {masterMissing ? (
        <div className="text-[11px] text-rose-400" data-testid="instance-master-unavailable">
          Master faceplate data is unavailable for this instance. Nothing is rendered rather than
          substituting a replacement faceplate.
        </div>
      ) : (
        <div className="space-y-1">
          {instance.views!.map((view) => (
            <div key={view.kind} className="text-[10px] text-neutral-400">
              <span className="text-neutral-500">{view.kind}</span>
              {view.bindings.length === 0 ? (
                <span className="ml-1 italic text-neutral-600">no bindings</span>
              ) : (
                <span className="ml-1 font-mono text-neutral-300">
                  {view.bindings.map((binding) => `${binding.faceplateKey}←${binding.udtProperty}`).join(', ')}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[10px]">
        {instance.bindings.length === 0 ? (
          <span className="italic text-neutral-600">No tags assigned</span>
        ) : (
          instance.bindings.map((binding) => (
            <span key={binding.faceplateKey} className="font-mono text-neutral-400">
              {binding.faceplateKey}: {binding.tagUri || '(unassigned)'}
            </span>
          ))
        )}
      </div>

      {/* The daemon's own warnings, verbatim: a frozen version missing from history, a draft
          fallback, and so on. An empty array renders nothing rather than a reassuring guess. */}
      {instance.warnings.map((warning) => (
        <div key={warning} className="flex items-start gap-1 text-[10px] text-amber-400">
          <AlertTriangle className="mt-0.5 h-2.5 w-2.5 shrink-0" />
          <span>{warning}</span>
        </div>
      ))}
    </div>
  );
};

const Field: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <label className="block space-y-0.5">
    <span className="text-[10px] uppercase tracking-wider text-neutral-500">{label}</span>
    {children}
  </label>
);

const inputClass =
  'w-full rounded border border-neutral-700 bg-neutral-900 px-1.5 py-1 text-[11px] text-neutral-200 focus:border-cyan-600 focus:outline-none';

const TypeEditor: React.FC<{ type: FaceplateType }> = ({ type }) => {
  const instances = useFaceplateStore((s) => s.instances);
  const lastRelease = useFaceplateStore((s) => s.lastRelease);
  const saveDraft = useFaceplateStore((s) => s.saveDraft);
  const release = useFaceplateStore((s) => s.release);
  const createInstance = useFaceplateStore((s) => s.createInstance);
  const setInstancePin = useFaceplateStore((s) => s.setInstancePin);
  const dismissRelease = useFaceplateStore((s) => s.dismissRelease);

  const [draft, setDraft] = useState<FaceplateType>(type);
  const [instanceName, setInstanceName] = useState('');
  const errorMessage = useFaceplateStore((s) => s.errorMessage);
  const clearError = useFaceplateStore((s) => s.clearError);
  // The last server copy this editor adopted. Dirty is judged against it rather than against the
  // current prop, so a background refresh cannot make unsaved edits look saved.
  const serverRef = useRef<FaceplateType>(type);

  useEffect(() => {
    // A refresh (another station's release, an SSE FACEPLATE_PROPAGATED) must not drop edits that
    // were never saved. Follow the server only while the operator has nothing unsaved.
    setDraft((current) =>
      JSON.stringify(current) === JSON.stringify(serverRef.current) ? type : current
    );
    serverRef.current = type;
  }, [type]);

  const memberNames = draft.udt.members.map((member) => member.name).filter(Boolean);
  const boundInstances = instances.filter((instance) => instance.typeId === type.id);
  const dirty = JSON.stringify(draft) !== JSON.stringify(serverRef.current);

  const patchUdt = (patch: Partial<UdtDefinition>) =>
    setDraft((current) => ({ ...current, udt: { ...current.udt, ...patch } }));

  const patchView = (index: number, view: FaceplateView) =>
    setDraft((current) => ({
      ...current,
      views: current.views.map((existing, i) => (i === index ? view : existing)),
    }));

  const addView = (kind: FaceplateViewKind) =>
    setDraft((current) => ({ ...current, views: [...current.views, { kind, bindings: [] }] }));

  return (
    <div className="flex h-full flex-col overflow-hidden bg-[var(--bg-base)] text-neutral-300">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-neutral-800 bg-[var(--bg-surface)] px-3 py-1.5">
        <div className="flex items-center gap-2">
          <Boxes className="h-4 w-4 text-teal-400" />
          <span className="text-xs font-semibold text-white">{type.name}</span>
          <span className="font-mono text-[10px] text-neutral-500">{versionLabel(type.version)}</span>
          {dirty && (
            <span className="rounded bg-amber-950 px-1.5 py-0.5 text-[9px] font-semibold text-amber-300">
              UNSAVED
            </span>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            data-testid="save-faceplate-draft"
            disabled={!dirty}
            onClick={() => void saveDraft(draft)}
            className="flex items-center gap-1 rounded border border-neutral-700 px-2 py-1 text-[11px] text-neutral-200 hover:border-cyan-600 disabled:opacity-40"
          >
            <Save className="h-3 w-3" /> Save draft
          </button>
          <button
            type="button"
            data-testid="release-faceplate-type"
            disabled={dirty}
            title={
              dirty
                ? 'Save the draft before releasing: release publishes the stored draft, not your unsaved edits.'
                : undefined
            }
            onClick={() => void release(type.id)}
            className="flex items-center gap-1 rounded bg-blue-600 px-2 py-1 text-[11px] font-semibold text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:bg-neutral-700 disabled:text-neutral-400"
          >
            <Rocket className="h-3 w-3" /> Release
          </button>
        </div>
      </div>

      <div className="flex-1 space-y-4 overflow-y-auto p-3">
        {dirty && (
          <div
            data-testid="release-blocked-unsaved"
            className="rounded border border-amber-800 bg-amber-950/40 p-2 text-[11px] text-amber-300"
          >
            Release is blocked while this type has unsaved edits. Saving first means the version you
            release is the one on screen; releasing without saving would publish the stored draft and
            silently drop these changes.
          </div>
        )}
        {errorMessage && (
          <div
            data-testid="faceplate-write-error"
            className="flex items-start justify-between gap-2 rounded border border-rose-800 bg-rose-950/40 p-2 text-[11px] text-rose-300"
          >
            <span>{errorMessage}</span>
            <button type="button" onClick={clearError} className="text-rose-400 hover:text-white">
              Dismiss
            </button>
          </div>
        )}
        {lastRelease && lastRelease.id === type.id && (
          <div
            data-testid="faceplate-release-result"
            className="rounded border border-neutral-700 bg-neutral-900 p-2 text-[11px]"
          >
            <div className="flex items-center justify-between">
              <span className="text-emerald-400">
                Released {versionLabel(lastRelease.version)}. {lastRelease.instancesAffected}{' '}
                instance{lastRelease.instancesAffected === 1 ? '' : 's'} now resolve against it.
              </span>
              <button type="button" onClick={dismissRelease} className="text-neutral-500 hover:text-white">
                Dismiss
              </button>
            </div>
            {lastRelease.strandedInstances.length > 0 && (
              <div className="mt-1 text-amber-400" data-testid="stranded-instances">
                {lastRelease.strandedInstances.length} instance
                {lastRelease.strandedInstances.length === 1 ? '' : 's'} stayed frozen and were not
                upgraded:{' '}
                {lastRelease.strandedInstances
                  .map((instance) => `${instance.name} (${versionLabel(instance.typeVersion)})`)
                  .join(', ')}
              </div>
            )}
          </div>
        )}

        <section className="space-y-2">
          <h3 className="text-[11px] font-semibold uppercase tracking-wider text-neutral-400">
            Master Type Identity
          </h3>
          <div className="grid grid-cols-2 gap-2">
            <Field label="Type name">
              <input
                className={inputClass}
                data-testid="faceplate-type-name"
                value={draft.name}
                onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
              />
            </Field>
            <Field label="Version (assigned by release)">
              <input className={inputClass} value={versionLabel(draft.version)} readOnly />
            </Field>
          </div>
        </section>

        <section className="space-y-2" data-testid="udt-binding-section">
          <h3 className="text-[11px] font-semibold uppercase tracking-wider text-neutral-400">
            UDT Binding
          </h3>
          <div className="grid grid-cols-2 gap-2">
            <Field label="UDT name">
              <input
                className={inputClass}
                data-testid="udt-name"
                value={draft.udt.name}
                onChange={(event) => patchUdt({ name: event.target.value })}
              />
            </Field>
            <Field label="Kind">
              <select
                className={inputClass}
                value={draft.udt.kind}
                onChange={(event) => patchUdt({ kind: event.target.value as UdtKind })}
              >
                {UDT_KINDS.map((kind) => (
                  <option key={kind} value={kind}>
                    {kind}
                  </option>
                ))}
              </select>
            </Field>
          </div>

          {draft.udt.kind === 'STRUCT' ? (
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-[10px] uppercase tracking-wider text-neutral-500">Members</span>
                <button
                  type="button"
                  onClick={() =>
                    patchUdt({ members: [...draft.udt.members, { name: '', dataType: 'BOOL' }] })
                  }
                  className="flex items-center gap-0.5 text-[10px] text-cyan-400 hover:text-cyan-200"
                >
                  <Plus className="h-3 w-3" /> Add member
                </button>
              </div>
              {draft.udt.members.length === 0 ? (
                <div className="text-[11px] italic text-neutral-600">
                  No members yet. Views can only bind to declared UDT members.
                </div>
              ) : (
                draft.udt.members.map((member, index) => (
                  <div key={index} className="flex items-center gap-1">
                    <input
                      className={inputClass}
                      placeholder="memberName"
                      value={member.name}
                      onChange={(event) =>
                        patchUdt({
                          members: draft.udt.members.map((existing, i) =>
                            i === index ? { ...existing, name: event.target.value } : existing
                          ),
                        })
                      }
                    />
                    <select
                      className={`${inputClass} w-32`}
                      value={member.dataType}
                      onChange={(event) =>
                        patchUdt({
                          members: draft.udt.members.map((existing, i) =>
                            i === index ? { ...existing, dataType: event.target.value } : existing
                          ),
                        })
                      }
                    >
                      {UDT_DATA_TYPES.map((dataType) => (
                        <option key={dataType} value={dataType}>
                          {dataType}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      title="Remove member"
                      onClick={() =>
                        patchUdt({ members: draft.udt.members.filter((_, i) => i !== index) })
                      }
                      className="p-1 text-neutral-500 hover:text-rose-400"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                ))
              )}
            </div>
          ) : (
            <Field label="Enum constants (comma separated)">
              <input
                className={inputClass}
                value={draft.udt.enumConstants.join(', ')}
                onChange={(event) =>
                  patchUdt({
                    enumConstants: event.target.value
                      .split(',')
                      .map((value) => value.trim())
                      .filter(Boolean),
                  })
                }
              />
            </Field>
          )}
        </section>

        <section className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-[11px] font-semibold uppercase tracking-wider text-neutral-400">
              Archetype Views
            </h3>
            <div className="flex flex-wrap gap-1">
              {VIEW_KINDS.map(({ kind, label }) => (
                <button
                  key={kind}
                  type="button"
                  onClick={() => addView(kind)}
                  className="rounded border border-neutral-700 px-1.5 py-0.5 text-[10px] text-neutral-300 hover:border-cyan-600"
                  title={`Add ${label}`}
                >
                  + {kind}
                </button>
              ))}
            </div>
          </div>

          {draft.views.length === 0 ? (
            <div className="text-[11px] italic text-neutral-600">
              No views defined yet. A released type with no views renders nothing.
            </div>
          ) : (
            draft.views.map((view, viewIndex) => (
              <div
                key={`${view.kind}-${viewIndex}`}
                className="space-y-1 rounded border border-neutral-800 bg-neutral-900/60 p-2"
              >
                <div className="flex items-center justify-between">
                  <span className="text-[11px] font-semibold text-neutral-200">{view.kind}</span>
                  <button
                    type="button"
                    title="Remove view"
                    onClick={() =>
                      setDraft((current) => ({
                        ...current,
                        views: current.views.filter((_, i) => i !== viewIndex),
                      }))
                    }
                    className="p-0.5 text-neutral-500 hover:text-rose-400"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>

                {view.bindings.map((binding, bindingIndex) => (
                  <div key={bindingIndex} className="flex items-center gap-1">
                    <select
                      className={inputClass}
                      value={binding.faceplateKey}
                      onChange={(event) =>
                        patchView(viewIndex, {
                          ...view,
                          bindings: view.bindings.map((existing, i) =>
                            i === bindingIndex
                              ? { ...existing, faceplateKey: event.target.value }
                              : existing
                          ),
                        })
                      }
                    >
                      <option value="">faceplate property…</option>
                      {FACEPLATE_KEYS.map((key) => (
                        <option key={key} value={key}>
                          {key}
                        </option>
                      ))}
                    </select>
                    <select
                      className={inputClass}
                      value={binding.udtProperty}
                      onChange={(event) =>
                        patchView(viewIndex, {
                          ...view,
                          bindings: view.bindings.map((existing, i) =>
                            i === bindingIndex
                              ? { ...existing, udtProperty: event.target.value }
                              : existing
                          ),
                        })
                      }
                    >
                      <option value="">UDT member…</option>
                      {memberNames.map((name) => (
                        <option key={name} value={name}>
                          {name}
                        </option>
                      ))}
                    </select>
                    <select
                      className={`${inputClass} w-28`}
                      value={binding.access}
                      onChange={(event) =>
                        patchView(viewIndex, {
                          ...view,
                          bindings: view.bindings.map((existing, i) =>
                            i === bindingIndex
                              ? { ...existing, access: event.target.value as BindingAccess }
                              : existing
                          ),
                        })
                      }
                    >
                      <option value="READ_ONLY">READ_ONLY</option>
                      <option value="READ_WRITE">READ_WRITE</option>
                    </select>
                    <button
                      type="button"
                      title="Remove binding"
                      onClick={() =>
                        patchView(viewIndex, {
                          ...view,
                          bindings: view.bindings.filter((_, i) => i !== bindingIndex),
                        })
                      }
                      className="p-1 text-neutral-500 hover:text-rose-400"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                ))}

                <button
                  type="button"
                  data-testid={`add-binding-${view.kind}`}
                  disabled={memberNames.length === 0}
                  onClick={() =>
                    patchView(viewIndex, {
                      ...view,
                      bindings: [
                        ...view.bindings,
                        { faceplateKey: '', udtProperty: '', access: 'READ_ONLY', unit: null },
                      ],
                    })
                  }
                  className="text-[10px] text-cyan-400 hover:text-cyan-200 disabled:opacity-40"
                >
                  + binding
                </button>
                {memberNames.length === 0 && (
                  <div className="text-[10px] italic text-neutral-600">
                    Declare UDT members before binding a view.
                  </div>
                )}
              </div>
            ))
          )}
        </section>

        <section className="space-y-2" data-testid="faceplate-instances-section">
          <h3 className="text-[11px] font-semibold uppercase tracking-wider text-neutral-400">
            Instances
          </h3>
          <div className="flex items-center gap-1">
            <input
              className={inputClass}
              placeholder="Instance name"
              value={instanceName}
              onChange={(event) => setInstanceName(event.target.value)}
            />
            <button
              type="button"
              data-testid="create-faceplate-instance"
              disabled={!instanceName.trim()}
              onClick={async () => {
                const created = await createInstance({ name: instanceName, typeId: type.id });
                if (created) setInstanceName('');
              }}
              className="shrink-0 rounded bg-neutral-800 px-2 py-1 text-[11px] text-neutral-200 hover:bg-neutral-700 disabled:opacity-40"
            >
              Derive instance
            </button>
          </div>

          {boundInstances.length === 0 ? (
            <div className="text-[11px] italic text-neutral-600" data-testid="empty-instances">
              No instances derive from this type yet.
            </div>
          ) : (
            <div className="space-y-1.5">
              {boundInstances.map((instance) => (
                <div key={instance.id} className="space-y-1">
                  <FaceplateInstanceCard instance={instance} />
                  <button
                    type="button"
                    onClick={() =>
                      void setInstancePin(
                        instance.id,
                        instance.frozen ? null : instance.resolvedVersion
                      )
                    }
                    className="flex items-center gap-1 text-[10px] text-neutral-500 hover:text-sky-300"
                  >
                    <RotateCcw className="h-2.5 w-2.5" />
                    {instance.frozen ? 'Unfreeze (follow active version)' : 'Freeze at current version'}
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
};

const NewTypeForm: React.FC = () => {
  const createType = useFaceplateStore((s) => s.createType);
  const errorMessage = useFaceplateStore((s) => s.errorMessage);
  const clearError = useFaceplateStore((s) => s.clearError);
  const [name, setName] = useState('');
  const [udt, setUdt] = useState<UdtDefinition>(emptyUdt());

  return (
    <div className="h-full overflow-y-auto bg-[var(--bg-base)] p-4 text-neutral-300">
      <h2 className="mb-1 text-sm font-semibold text-white">New Master Faceplate Type</h2>
      <p className="mb-3 text-[11px] text-neutral-400">
        The type binds to a UDT by declaring it here. Views bind to its declared members; the daemon
        rejects a binding that names a member you did not declare.
      </p>

      <div className="max-w-xl space-y-3">
        {errorMessage && (
          <div
            data-testid="faceplate-write-error"
            className="flex items-start justify-between gap-2 rounded border border-rose-800 bg-rose-950/40 p-2 text-[11px] text-rose-300"
          >
            <span>{errorMessage}</span>
            <button type="button" onClick={clearError} className="text-rose-400 hover:text-white">
              Dismiss
            </button>
          </div>
        )}
        <Field label="Type name">
          <input
            className={inputClass}
            data-testid="new-faceplate-type-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="e.g. Motor_VFD"
          />
        </Field>

        <Field label="UDT name">
          <input
            className={inputClass}
            data-testid="new-faceplate-udt-name"
            value={udt.name}
            onChange={(event) => setUdt({ ...udt, name: event.target.value })}
            placeholder="e.g. UDT_Motor_VFD"
          />
        </Field>

        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <span className="text-[10px] uppercase tracking-wider text-neutral-500">UDT members</span>
            <button
              type="button"
              onClick={() => setUdt({ ...udt, members: [...udt.members, { name: '', dataType: 'BOOL' }] })}
              className="flex items-center gap-0.5 text-[10px] text-cyan-400 hover:text-cyan-200"
            >
              <Plus className="h-3 w-3" /> Add member
            </button>
          </div>
          {udt.members.map((member, index) => (
            <div key={index} className="flex items-center gap-1">
              <input
                className={inputClass}
                placeholder="memberName"
                value={member.name}
                onChange={(event) =>
                  setUdt({
                    ...udt,
                    members: udt.members.map((existing, i) =>
                      i === index ? { ...existing, name: event.target.value } : existing
                    ),
                  })
                }
              />
              <select
                className={`${inputClass} w-32`}
                value={member.dataType}
                onChange={(event) =>
                  setUdt({
                    ...udt,
                    members: udt.members.map((existing, i) =>
                      i === index ? { ...existing, dataType: event.target.value } : existing
                    ),
                  })
                }
              >
                {UDT_DATA_TYPES.map((dataType) => (
                  <option key={dataType} value={dataType}>
                    {dataType}
                  </option>
                ))}
              </select>
            </div>
          ))}
        </div>

        <button
          type="button"
          data-testid="create-faceplate-type"
          disabled={!name.trim() || !udt.name.trim()}
          onClick={() => void createType({ name, udt })}
          className="rounded bg-blue-600 px-3 py-1.5 text-[11px] font-semibold text-white hover:bg-blue-500 disabled:opacity-40"
        >
          Create master type
        </button>
      </div>
    </div>
  );
};

const LibraryOverview: React.FC<{ typeId?: string }> = ({ typeId }) => {
  const types = useFaceplateStore((s) => s.types);
  const status = useFaceplateStore((s) => s.status);
  const errorMessage = useFaceplateStore((s) => s.errorMessage);
  const load = useFaceplateStore((s) => s.load);
  const projectId = useFaceplateStore((s) => s.projectId);

  if (status === 'LOADING' || status === 'IDLE') {
    return <div className="p-4 text-[11px] text-neutral-400">Loading faceplate types…</div>;
  }

  if (status === 'ERROR') {
    return (
      <div className="space-y-2 p-4 text-[11px]">
        <div className="text-rose-400" data-testid="faceplate-load-error">
          {errorMessage}
        </div>
        <button
          type="button"
          onClick={() => projectId && void load(projectId)}
          className="rounded border border-neutral-700 px-2 py-1 text-neutral-200 hover:border-cyan-600"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto bg-[var(--bg-base)] p-4 text-neutral-300">
      <h2 className="mb-3 text-sm font-semibold text-white">Master Faceplate Types</h2>
      {types.length === 0 ? (
        <div className="text-[11px] text-neutral-500" data-testid="empty-faceplate-library">
          This project has no faceplate types yet. Create one from the Project Tree and release it to
          make its views available to instances.
        </div>
      ) : (
        <ul className="space-y-1">
          {types.map((type: FaceplateTypeSummary) => (
            <li
              key={type.id}
              className={`rounded border px-2 py-1 text-[11px] ${
                type.id === typeId ? 'border-cyan-600 bg-neutral-900' : 'border-neutral-800'
              }`}
            >
              <span className="font-mono text-cyan-400">{type.name}</span>{' '}
              <span className="text-neutral-500">{versionLabel(type.activeVersion)}</span>{' '}
              <span className="text-neutral-500">· UDT {type.udtName}</span>
              {type.hasDraft && <span className="ml-1 text-amber-400">· draft</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

/**
 * Zone 3 master faceplate type designer (TSK-309 part A).
 *
 * Every value shown comes from the project-scoped faceplate routes and the ts-rs generated
 * shapes. With no types defined the panel shows an empty state; it never presents a sample
 * type, and it never renders an instance from anything but the daemon-resolved master.
 */
export const FaceplateTypeDesigner: React.FC<FaceplateTypeDesignerProps> = ({
  typeId,
  className = '',
}) => {
  const types = useFaceplateStore((s) => s.types);
  const status = useFaceplateStore((s) => s.status);
  const loadedProjectId = useFaceplateStore((s) => s.projectId);
  const openType = useFaceplateStore((s) => s.openType);
  const openTypeStatus = useFaceplateStore((s) => s.openTypeStatus);
  const openTypeError = useFaceplateStore((s) => s.openTypeError);
  const load = useFaceplateStore((s) => s.load);
  const loadType = useFaceplateStore((s) => s.loadType);
  const currentProject = useHmiStore((s) => s.currentProject);
  const currentProjectId = currentProject?.id ?? null;

  // The panel may be opened without the tree mounted, so it loads its own data.
  useEffect(() => {
    if (currentProjectId) {
      if (loadedProjectId !== currentProjectId) void load(currentProjectId);
    } else {
      useFaceplateStore.getState().reset();
    }
  }, [currentProjectId, loadedProjectId, load]);

  // The tree carries summaries; the designer edits the full authored definition.
  useEffect(() => {
    if (currentProjectId && typeId && typeId !== 'new') void loadType(typeId);
  }, [currentProjectId, typeId, loadType]);

  // A release elsewhere (another station, a script) arrives on the telemetry stream; re-read
  // rather than guessing which instances moved.
  useEffect(() => {
    if (!currentProjectId) return undefined;
    return useFaceplateStore.getState().watchPropagation();
  }, [currentProjectId]);

  const summary = useMemo(
    () => (typeId ? types.find((type) => type.id === typeId || type.name === typeId) : undefined),
    [types, typeId]
  );

  let body: React.ReactNode;
  if (!currentProjectId) {
    body = (
      <div className="p-4 text-[11px] text-neutral-500" data-testid="designer-no-project">
        Open a project before authoring faceplate types.
      </div>
    );
  } else if (typeId === 'new') {
    body = <NewTypeForm />;
  } else if (!typeId) {
    body = <LibraryOverview typeId={typeId} />;
  } else if (openTypeStatus === 'ERROR') {
    body = (
      <div className="space-y-2 p-4 text-[11px]">
        <div className="text-rose-400" data-testid="faceplate-type-error">
          {openTypeError}
        </div>
        <button
          type="button"
          onClick={() => void loadType(typeId)}
          className="rounded border border-neutral-700 px-2 py-1 text-neutral-200 hover:border-cyan-600"
        >
          Retry
        </button>
      </div>
    );
  } else if (!summary) {
    // The id came from a panel param but the project does not list it: say so, do not invent it.
    body = (
      <div className="p-4 text-[11px] text-neutral-500" data-testid="faceplate-type-missing">
        {status === 'LOADING' || status === 'IDLE'
          ? 'Loading faceplate types…'
          : `This project has no faceplate type "${typeId}".`}
      </div>
    );
  } else if (!openType || openTypeStatus !== 'READY') {
    body = <div className="p-4 text-[11px] text-neutral-400">Loading faceplate type…</div>;
  } else {
    // Keyed by id only: remounting on a views change would reset the editor's local draft and
    // drop unsaved edits on the next background refresh.
    body = <TypeEditor key={openType.id} type={openType} />;
  }

  return (
    <div
      className={`h-full w-full overflow-hidden bg-[var(--bg-base)] ${className}`}
      data-testid="faceplate-type-designer"
    >
      {body}
    </div>
  );
};