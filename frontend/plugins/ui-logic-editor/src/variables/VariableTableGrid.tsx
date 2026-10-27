import React, { useState } from 'react';
import { VariableRow, VariableScope, ELEMENTARY_DATA_TYPES, VariableTableGridProps } from './types';
import { Plus, Trash2, Search, Filter, Lock, Unlock } from 'lucide-react';
import { EmptySurface, EMPTY_SURFACE } from '@mythos/ui-empty-state';


const SCOPES: VariableScope[] = [
  'VAR',
  'VAR_INPUT',
  'VAR_OUTPUT',
  'VAR_IN_OUT',
  'VAR_TEMP',
  'VAR_GLOBAL',
  'VAR_EXTERNAL',
];

export const VariableTableGrid: React.FC<VariableTableGridProps> = ({
  variables,
  availableDataTypes = [...ELEMENTARY_DATA_TYPES],
  onChange,
  readOnly = false,
  className = '',
  debugActive = false,
  onForceVariable,
  onUnforceVariable,
}) => {
  const [rows, setRows] = useState<VariableRow[]>(variables ?? []);
  const [filterText, setFilterText] = useState('');
  const [scopeFilter, setScopeFilter] = useState<string>('ALL');

  const handleRowChange = (id: string, field: keyof VariableRow, val: any) => {
    const updated = rows.map((r) => (r.id === id ? { ...r, [field]: val } : r));
    setRows(updated);
    onChange?.(updated);
  };

  const handleAddRow = () => {
    const newRow: VariableRow = {
      id: `var-${Date.now()}`,
      name: `Var_${rows.length + 1}`,
      scope: 'VAR',
      dataType: 'BOOL',
      address: '',
      initialValue: 'FALSE',
      retain: false,
      constant: false,
      comment: '',
    };
    const next = [...rows, newRow];
    setRows(next);
    onChange?.(next);
  };

  const handleDeleteRow = (id: string) => {
    const next = rows.filter((r) => r.id !== id);
    setRows(next);
    onChange?.(next);
  };

  const filteredRows = rows.filter((r) => {
    const matchText =
      r.name.toLowerCase().includes(filterText.toLowerCase()) ||
      (r.address && r.address.toLowerCase().includes(filterText.toLowerCase())) ||
      (r.comment && r.comment.toLowerCase().includes(filterText.toLowerCase()));
    const matchScope = scopeFilter === 'ALL' || r.scope === scopeFilter;
    return matchText && matchScope;
  });

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-base)] text-neutral-200 p-4 select-none overflow-hidden ${className}`}
      data-testid="variable-table-grid"
    >
      {/* Top Filter and Controls Bar */}
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-3 text-xs">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5 px-2.5 py-1 bg-neutral-900 border border-neutral-700 rounded text-neutral-300">
            <Search className="w-3.5 h-3.5 text-neutral-400" />
            <input
              type="text"
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              placeholder="Filter variables by name / address..."
              className="bg-transparent border-none text-xs text-neutral-200 focus:outline-none w-48 font-mono"
            />
          </div>

          <div className="flex items-center gap-1.5 text-neutral-400 font-mono text-[11px]">
            <Filter className="w-3 h-3 text-neutral-500" />
            <span>Scope:</span>
            <select
              value={scopeFilter}
              onChange={(e) => setScopeFilter(e.target.value)}
              className="px-2 py-1 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none"
            >
              <option value="ALL">ALL SCOPES</option>
              {SCOPES.map((sc) => (
                <option key={sc} value={sc}>
                  {sc}
                </option>
              ))}
            </select>
          </div>
        </div>

        {!readOnly && (
          <button
            onClick={handleAddRow}
            data-testid="add-variable-btn"
            className="flex items-center gap-1.5 px-3 py-1 bg-blue-600 hover:bg-blue-500 text-white rounded font-medium shadow text-xs transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Add Variable</span>
          </button>
        )}
      </div>

      {/* Spreadsheet Grid Table */}
      {rows.length === 0 ? (
        <div className="flex-1 flex items-center justify-center p-6">
          <EmptySurface copy={EMPTY_SURFACE.variables} />
        </div>
      ) : (
      <div className="flex-1 overflow-auto border border-neutral-800 rounded bg-[var(--bg-base)]">
        <table className="w-full text-left text-xs font-mono border-collapse">
          <thead className="bg-[var(--bg-surface)] text-neutral-400 border-b border-neutral-800 sticky top-0 z-10 select-none text-[11px]">
            <tr>
              <th className="py-2 px-3 border-r border-neutral-800 w-10 text-center">#</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[140px]">Name</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[110px]">Scope</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[110px]">Data Type</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[110px]">IEC Address</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[100px]">Initial Value</th>
              <th className="py-2 px-3 border-r border-neutral-800 w-16 text-center">Retain</th>
              <th className="py-2 px-3 border-r border-neutral-800 w-16 text-center">Constant</th>
              <th className="py-2 px-3 border-r border-neutral-800 min-w-[180px]">Comment</th>
              {debugActive && (
                <th className="py-2 px-3 border-r border-neutral-800 min-w-[100px] text-emerald-400">
                  Live Value
                </th>
              )}
              {!readOnly && <th className="py-2 px-2 w-10 text-center">Act</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-neutral-800/60">
            {filteredRows.map((r, idx) => (
              <tr key={r.id} className="hover:bg-neutral-800/40 transition-colors">
                <td className="py-1.5 px-3 border-r border-neutral-800/60 text-center text-neutral-500 text-[10px]">
                  {idx + 1}
                </td>

                {/* Variable Name */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-cyan-300 font-medium">{r.name}</span>
                  ) : (
                    <input
                      type="text"
                      value={r.name}
                      onChange={(e) => handleRowChange(r.id, 'name', e.target.value)}
                      className="w-full bg-transparent border-none text-cyan-300 font-medium focus:outline-none focus:ring-1 focus:ring-cyan-500 rounded px-1"
                    />
                  )}
                </td>

                {/* Scope */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-neutral-400">{r.scope}</span>
                  ) : (
                    <select
                      value={r.scope}
                      onChange={(e) => handleRowChange(r.id, 'scope', e.target.value as VariableScope)}
                      className="w-full bg-neutral-900 border border-neutral-700 text-neutral-300 rounded px-1.5 py-0.5 text-[11px] focus:outline-none"
                    >
                      {SCOPES.map((sc) => (
                        <option key={sc} value={sc}>
                          {sc}
                        </option>
                      ))}
                    </select>
                  )}
                </td>

                {/* Data Type */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-amber-400">{r.dataType}</span>
                  ) : (
                    <select
                      value={r.dataType}
                      onChange={(e) => handleRowChange(r.id, 'dataType', e.target.value)}
                      className="w-full bg-neutral-900 border border-neutral-700 text-amber-300 rounded px-1.5 py-0.5 text-[11px] focus:outline-none"
                    >
                      {availableDataTypes.map((dt) => (
                        <option key={dt} value={dt}>
                          {dt}
                        </option>
                      ))}
                    </select>
                  )}
                </td>

                {/* Address */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-purple-400">{r.address || '-'}</span>
                  ) : (
                    <input
                      type="text"
                      value={r.address || ''}
                      placeholder="%IX0.0"
                      onChange={(e) => handleRowChange(r.id, 'address', e.target.value)}
                      className="w-full bg-transparent border-none text-purple-300 placeholder-neutral-600 focus:outline-none focus:ring-1 focus:ring-purple-500 rounded px-1"
                    />
                  )}
                </td>

                {/* Initial Value */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-neutral-300">{r.initialValue || '-'}</span>
                  ) : (
                    <input
                      type="text"
                      value={r.initialValue || ''}
                      onChange={(e) => handleRowChange(r.id, 'initialValue', e.target.value)}
                      className="w-full bg-transparent border-none text-neutral-300 focus:outline-none focus:ring-1 focus:ring-neutral-500 rounded px-1"
                    />
                  )}
                </td>

                {/* Retain */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60 text-center">
                  <input
                    type="checkbox"
                    checked={Boolean(r.retain)}
                    disabled={readOnly}
                    onChange={(e) => handleRowChange(r.id, 'retain', e.target.checked)}
                    className="rounded bg-neutral-900 border-neutral-700 text-blue-600 focus:ring-0"
                  />
                </td>

                {/* Constant */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60 text-center">
                  <input
                    type="checkbox"
                    checked={Boolean(r.constant)}
                    disabled={readOnly}
                    onChange={(e) => handleRowChange(r.id, 'constant', e.target.checked)}
                    className="rounded bg-neutral-900 border-neutral-700 text-blue-600 focus:ring-0"
                  />
                </td>

                {/* Comment */}
                <td className="py-1.5 px-3 border-r border-neutral-800/60">
                  {readOnly ? (
                    <span className="text-neutral-500">{r.comment || ''}</span>
                  ) : (
                    <input
                      type="text"
                      value={r.comment || ''}
                      placeholder="Comment..."
                      onChange={(e) => handleRowChange(r.id, 'comment', e.target.value)}
                      className="w-full bg-transparent border-none text-neutral-400 placeholder-neutral-700 focus:outline-none focus:ring-1 focus:ring-neutral-600 rounded px-1"
                    />
                  )}
                </td>

                {/* Live Value in Debug Mode */}
                {debugActive && (
                  <td className="py-1.5 px-3 border-r border-neutral-800/60">
                    <div className="flex items-center justify-between">
                      <span className="text-emerald-400 font-bold">
                        {r.liveValue !== undefined ? String(r.liveValue) : 'TRUE'}
                      </span>
                      {r.isForced ? (
                        <button
                          onClick={() => onUnforceVariable?.(r.name)}
                          className="p-0.5 text-amber-400 hover:text-amber-300 ml-1"
                          title="Unforce"
                        >
                          <Lock className="w-3 h-3" />
                        </button>
                      ) : (
                        <button
                          onClick={() => onForceVariable?.(r.name, true)}
                          className="p-0.5 text-neutral-500 hover:text-cyan-400 ml-1"
                          title="Force"
                        >
                          <Unlock className="w-3 h-3" />
                        </button>
                      )}
                    </div>
                  </td>
                )}

                {/* Actions */}
                {!readOnly && (
                  <td className="py-1.5 px-2 text-center">
                    <button
                      onClick={() => handleDeleteRow(r.id)}
                      className="p-1 hover:bg-neutral-800 text-neutral-500 hover:text-rose-400 rounded transition-colors"
                      title="Delete variable"
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
      )}
    </div>
  );
};
