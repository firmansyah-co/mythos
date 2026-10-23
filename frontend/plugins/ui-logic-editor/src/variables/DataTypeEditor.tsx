import React, { useState } from 'react';
import { DataTypeDefinition, DataTypeEditorProps, ELEMENTARY_DATA_TYPES, StructMember } from './types';
import { Plus, Trash2, Layers, Check, Copy } from 'lucide-react';

export const DEFAULT_SAMPLE_DATA_TYPES: DataTypeDefinition[] = [
  {
    id: 'dt-bottle-spec',
    name: 'DUT_BottleSpec',
    kind: 'STRUCT',
    comment: 'Bottling line container dimensional profile',
    members: [
      { id: 'm-1', name: 'Height_mm', dataType: 'REAL', initialValue: '250.0', comment: 'Bottle height' },
      { id: 'm-2', name: 'Diameter_mm', dataType: 'REAL', initialValue: '75.0', comment: 'Outer diameter' },
      { id: 'm-3', name: 'Volume_ml', dataType: 'DINT', initialValue: '500', comment: 'Nominal volume in ml' },
      { id: 'm-4', name: 'IsGlass', dataType: 'BOOL', initialValue: 'TRUE', comment: 'Glass vs PET flag' },
    ],
  },
  {
    id: 'dt-machine-state',
    name: 'DUT_MachineState',
    kind: 'ENUM',
    comment: 'PackML ISA-TR88 Operational Machine States',
    enumConstants: ['STOPPED', 'STARTING', 'IDLE', 'RUNNING', 'HOLDING', 'HELD', 'ABORTED'],
  },
];

export const DataTypeEditor: React.FC<DataTypeEditorProps> = ({
  dataTypes = DEFAULT_SAMPLE_DATA_TYPES,
  onChange,
  readOnly = false,
  className = '',
}) => {
  const [types, setTypes] = useState<DataTypeDefinition[]>(dataTypes);
  const [selectedTypeId, setSelectedTypeId] = useState<string>(types[0]?.id || '');
  const [copiedNotification, setCopiedNotification] = useState(false);

  const activeType = types.find((t) => t.id === selectedTypeId) || types[0];

  const handleAddType = (kind: 'STRUCT' | 'ENUM') => {
    const newType: DataTypeDefinition = {
      id: `dt-${Date.now()}`,
      name: kind === 'STRUCT' ? `DUT_Struct_${types.length + 1}` : `DUT_Enum_${types.length + 1}`,
      kind,
      comment: 'User defined IEC type',
      members:
        kind === 'STRUCT'
          ? [
              { id: `m-${Date.now()}-1`, name: 'Field1', dataType: 'BOOL', initialValue: 'FALSE' },
              { id: `m-${Date.now()}-2`, name: 'Field2', dataType: 'INT', initialValue: '0' },
            ]
          : undefined,
      enumConstants: kind === 'ENUM' ? ['STATE_A', 'STATE_B', 'STATE_C'] : undefined,
    };
    const next = [...types, newType];
    setTypes(next);
    setSelectedTypeId(newType.id);
    onChange?.(next);
  };

  const handleDeleteType = (id: string) => {
    const next = types.filter((t) => t.id !== id);
    setTypes(next);
    if (selectedTypeId === id && next.length > 0) {
      setSelectedTypeId(next[0].id);
    }
    onChange?.(next);
  };

  const handleUpdateActiveType = (updated: DataTypeDefinition) => {
    const next = types.map((t) => (t.id === updated.id ? updated : t));
    setTypes(next);
    onChange?.(next);
  };

  const handleAddMember = () => {
    if (!activeType || activeType.kind !== 'STRUCT') return;
    const newMember: StructMember = {
      id: `m-${Date.now()}`,
      name: `Element_${(activeType.members?.length || 0) + 1}`,
      dataType: 'BOOL',
      initialValue: 'FALSE',
      comment: '',
    };
    handleUpdateActiveType({
      ...activeType,
      members: [...(activeType.members || []), newMember],
    });
  };

  const handleDeleteMember = (memberId: string) => {
    if (!activeType || activeType.kind !== 'STRUCT') return;
    handleUpdateActiveType({
      ...activeType,
      members: (activeType.members || []).filter((m) => m.id !== memberId),
    });
  };

  const generateIecDeclaration = (dt: DataTypeDefinition): string => {
    if (dt.kind === 'STRUCT') {
      const membersText = (dt.members || [])
        .map(
          (m) =>
            `    ${m.name} : ${m.dataType}${m.initialValue ? ` := ${m.initialValue}` : ''};${
              m.comment ? ` (* ${m.comment} *)` : ''
            }`
        )
        .join('\n');
      return `TYPE ${dt.name} :\n  STRUCT\n${membersText}\n  END_STRUCT\nEND_TYPE`;
    } else if (dt.kind === 'ENUM') {
      const enumVals = (dt.enumConstants || []).join(', ');
      return `TYPE ${dt.name} :\n  (${enumVals})\nEND_TYPE;`;
    }
    return '';
  };

  const handleCopyStCode = () => {
    if (!activeType) return;
    const code = generateIecDeclaration(activeType);
    navigator.clipboard?.writeText(code);
    setCopiedNotification(true);
    setTimeout(() => setCopiedNotification(false), 2000);
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-base)] text-neutral-200 p-4 select-none overflow-hidden ${className}`}
      data-testid="datatype-editor"
    >
      {/* Top Header */}
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-4">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-xs font-semibold text-neutral-200 tracking-wide uppercase">
              IEC 61131-3 Data Type Editor (DUT / UDT)
            </h3>
            <span className="px-1.5 py-0.2 rounded bg-neutral-800 text-[10px] text-cyan-400 font-mono">
              OpenPLC Parity
            </span>
          </div>
          <p className="text-[11px] text-neutral-500">
            User Defined Data Types • Structs & Enumerations • Standard ST Export
          </p>
        </div>

        {!readOnly && (
          <div className="flex items-center gap-2 text-xs">
            <button
              onClick={() => handleAddType('STRUCT')}
              className="flex items-center gap-1.5 px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-cyan-300 border border-neutral-700 rounded transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>+ New STRUCT</span>
            </button>
            <button
              onClick={() => handleAddType('ENUM')}
              className="flex items-center gap-1.5 px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-amber-300 border border-neutral-700 rounded transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>+ New ENUM</span>
            </button>
          </div>
        )}
      </div>

      <div className="flex-1 flex gap-4 min-h-0 overflow-hidden">
        {/* Left Side: Type List */}
        <div className="w-64 border border-neutral-800 rounded bg-[var(--bg-base)] flex flex-col overflow-hidden">
          <div className="p-2.5 bg-[var(--bg-surface)] border-b border-neutral-800 text-xs font-semibold text-neutral-300 flex items-center gap-2">
            <Layers className="w-4 h-4 text-cyan-400" />
            <span>Defined Data Types ({types.length})</span>
          </div>

          <div className="flex-1 overflow-y-auto divide-y divide-neutral-800/60 p-1">
            {types.map((dt) => (
              <div
                key={dt.id}
                onClick={() => setSelectedTypeId(dt.id)}
                className={`p-2 rounded flex items-center justify-between cursor-pointer font-mono text-xs transition-colors ${
                  activeType?.id === dt.id
                    ? 'bg-blue-600/20 border border-blue-500/50 text-white'
                    : 'hover:bg-neutral-800/40 text-neutral-300'
                }`}
              >
                <div className="flex items-center gap-2 truncate">
                  <span
                    className={`px-1 rounded text-[9px] font-bold ${
                      dt.kind === 'STRUCT' ? 'bg-cyan-950 text-cyan-400' : 'bg-amber-950 text-amber-400'
                    }`}
                  >
                    {dt.kind}
                  </span>
                  <span className="truncate">{dt.name}</span>
                </div>

                {!readOnly && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteType(dt.id);
                    }}
                    className="p-1 hover:text-rose-400 text-neutral-500 transition-colors"
                    title="Delete type"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Right Side: Type Detail & Members */}
        {activeType ? (
          <div className="flex-1 border border-neutral-800 rounded bg-[var(--bg-base)] flex flex-col overflow-hidden p-4">
            <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-3 text-xs">
              <div className="flex items-center gap-3">
                <span className="text-neutral-400 font-mono">Type Name:</span>
                {readOnly ? (
                  <span className="text-white font-mono font-bold text-sm">{activeType.name}</span>
                ) : (
                  <input
                    type="text"
                    value={activeType.name}
                    onChange={(e) => handleUpdateActiveType({ ...activeType, name: e.target.value })}
                    className="px-2 py-1 bg-neutral-900 border border-neutral-700 rounded text-cyan-300 font-mono text-xs font-bold focus:outline-none focus:border-cyan-500"
                  />
                )}
                <span className="px-1.5 py-0.5 rounded bg-neutral-800 text-[10px] text-neutral-400 font-mono">
                  {activeType.kind}
                </span>
              </div>

              <div className="flex items-center gap-2">
                <button
                  onClick={handleCopyStCode}
                  className="flex items-center gap-1.5 px-2.5 py-1 bg-neutral-900 hover:bg-neutral-800 text-neutral-300 border border-neutral-700 rounded text-xs transition-colors"
                >
                  {copiedNotification ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                  <span>{copiedNotification ? 'Copied ST Code' : 'Copy IEC ST'}</span>
                </button>
              </div>
            </div>

            {/* Struct Members Table */}
            {activeType.kind === 'STRUCT' && (
              <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-semibold text-neutral-300">Struct Members</span>
                  {!readOnly && (
                    <button
                      onClick={handleAddMember}
                      className="px-2 py-0.5 bg-blue-600 hover:bg-blue-500 text-white rounded text-xs font-medium"
                    >
                      + Add Member
                    </button>
                  )}
                </div>

                <div className="flex-1 overflow-auto border border-neutral-800 rounded">
                  <table className="w-full text-left text-xs font-mono border-collapse">
                    <thead className="bg-[var(--bg-surface)] text-neutral-400 text-[11px] sticky top-0">
                      <tr>
                        <th className="py-2 px-3 border-r border-neutral-800">Member Name</th>
                        <th className="py-2 px-3 border-r border-neutral-800">Data Type</th>
                        <th className="py-2 px-3 border-r border-neutral-800">Initial Value</th>
                        <th className="py-2 px-3 border-r border-neutral-800">Comment</th>
                        {!readOnly && <th className="py-2 px-2 w-10 text-center">Act</th>}
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-neutral-800/60">
                      {activeType.members?.map((m) => (
                        <tr key={m.id} className="hover:bg-neutral-800/40">
                          <td className="py-1.5 px-3 border-r border-neutral-800/60">
                            {readOnly ? (
                              <span className="text-cyan-300">{m.name}</span>
                            ) : (
                              <input
                                type="text"
                                value={m.name}
                                onChange={(e) => {
                                  const updatedMembers = activeType.members?.map((mem) =>
                                    mem.id === m.id ? { ...mem, name: e.target.value } : mem
                                  );
                                  handleUpdateActiveType({ ...activeType, members: updatedMembers });
                                }}
                                className="w-full bg-transparent text-cyan-300 focus:outline-none"
                              />
                            )}
                          </td>
                          <td className="py-1.5 px-3 border-r border-neutral-800/60">
                            {readOnly ? (
                              <span className="text-amber-300">{m.dataType}</span>
                            ) : (
                              <select
                                value={m.dataType}
                                onChange={(e) => {
                                  const updatedMembers = activeType.members?.map((mem) =>
                                    mem.id === m.id ? { ...mem, dataType: e.target.value } : mem
                                  );
                                  handleUpdateActiveType({ ...activeType, members: updatedMembers });
                                }}
                                className="bg-neutral-900 border border-neutral-700 text-amber-300 rounded px-1 text-xs"
                              >
                                {ELEMENTARY_DATA_TYPES.map((et) => (
                                  <option key={et} value={et}>
                                    {et}
                                  </option>
                                ))}
                              </select>
                            )}
                          </td>
                          <td className="py-1.5 px-3 border-r border-neutral-800/60">
                            {readOnly ? (
                              <span className="text-neutral-300">{m.initialValue || '-'}</span>
                            ) : (
                              <input
                                type="text"
                                value={m.initialValue || ''}
                                onChange={(e) => {
                                  const updatedMembers = activeType.members?.map((mem) =>
                                    mem.id === m.id ? { ...mem, initialValue: e.target.value } : mem
                                  );
                                  handleUpdateActiveType({ ...activeType, members: updatedMembers });
                                }}
                                className="w-full bg-transparent text-neutral-300 focus:outline-none"
                              />
                            )}
                          </td>
                          <td className="py-1.5 px-3 border-r border-neutral-800/60">
                            {readOnly ? (
                              <span className="text-neutral-500">{m.comment || ''}</span>
                            ) : (
                              <input
                                type="text"
                                value={m.comment || ''}
                                onChange={(e) => {
                                  const updatedMembers = activeType.members?.map((mem) =>
                                    mem.id === m.id ? { ...mem, comment: e.target.value } : mem
                                  );
                                  handleUpdateActiveType({ ...activeType, members: updatedMembers });
                                }}
                                className="w-full bg-transparent text-neutral-500 focus:outline-none"
                              />
                            )}
                          </td>
                          {!readOnly && (
                            <td className="py-1.5 px-2 text-center">
                              <button
                                onClick={() => handleDeleteMember(m.id)}
                                className="p-1 text-neutral-500 hover:text-rose-400"
                              >
                                <Trash2 className="w-3.5 h-3.5" />
                              </button>
                            </td>
                          )}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {/* Enum Constants View */}
            {activeType.kind === 'ENUM' && (
              <div className="flex-1 flex flex-col min-h-0">
                <span className="text-xs font-semibold text-neutral-300 mb-2">Enumeration Values</span>
                <div className="p-3 border border-neutral-800 rounded bg-[var(--bg-base)] font-mono text-xs text-amber-300 space-y-1">
                  {activeType.enumConstants?.map((val, idx) => (
                    <div key={idx} className="flex items-center gap-2">
                      <span className="text-neutral-500">{idx}:</span>
                      <span>{val}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Preview Generated IEC 61131-3 ST Code */}
            <div className="mt-3 pt-3 border-t border-neutral-800">
              <span className="text-[10px] font-mono text-neutral-500 block mb-1">
                Generated IEC 61131-3 Structured Text Representation:
              </span>
              <pre className="p-2.5 rounded bg-neutral-950 border border-neutral-800 font-mono text-[11px] text-cyan-300 overflow-x-auto">
                {generateIecDeclaration(activeType)}
              </pre>
            </div>
          </div>
        ) : (
          <div className="flex-1 flex items-center justify-center text-neutral-500 font-mono text-xs">
            Select or create a data type to view definition
          </div>
        )}
      </div>
    </div>
  );
};
