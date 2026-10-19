import React from 'react';
import {
  LadderRung,
  LadderElement,
  LadderParallelBranch,
  LadderContactType,
  LadderCoilType,
} from './types';
import {
  ContactElementView,
  CoilElementView,
  FunctionBlockElementView,
} from './LadderElements';
import { Trash2, Split } from 'lucide-react';

export interface RungRendererProps {
  rung: LadderRung;
  index: number;
  readOnly?: boolean;
  onUpdateRung: (rung: LadderRung) => void;
  onDeleteRung: (id: string) => void;
  onAddContact: (rungId: string, type: LadderContactType) => void;
  onAddCoil: (rungId: string, type: LadderCoilType) => void;
  onAddBranch: (rungId: string) => void;
  onAddBlock: (rungId: string, blockType: string) => void;
}

export const RungRenderer: React.FC<RungRendererProps> = ({
  rung,
  index,
  readOnly = false,
  onUpdateRung,
  onDeleteRung,
  onAddContact,
  onAddCoil,
  onAddBranch,
  onAddBlock,
}) => {
  const isRungEnergized = Boolean(rung.isEnergized);

  const handleTagChange = (elementId: string, newTag: string) => {
    const updateElements = (elements: LadderElement[]): LadderElement[] => {
      return elements.map((el) => {
        if (el.id === elementId) {
          return { ...el, tag: newTag } as LadderElement;
        }
        if (el.type === 'branch') {
          return {
            ...el,
            branches: el.branches.map((b) => updateElements(b)),
          };
        }
        return el;
      });
    };

    onUpdateRung({
      ...rung,
      elements: updateElements(rung.elements),
    });
  };

  const handleDeleteElement = (elementId: string) => {
    const filterElements = (elements: LadderElement[]): LadderElement[] => {
      return elements
        .filter((el) => el.id !== elementId)
        .map((el) => {
          if (el.type === 'branch') {
            return {
              ...el,
              branches: el.branches.map((b) => filterElements(b)),
            };
          }
          return el;
        });
    };

    onUpdateRung({
      ...rung,
      elements: filterElements(rung.elements),
    });
  };

  const renderSingleElement = (el: LadderElement) => {
    if (el.type === 'branch') {
      return renderParallelBranch(el);
    }
    if (el.type === 'function_block') {
      return (
        <FunctionBlockElementView
          key={el.id}
          element={el}
          onDelete={handleDeleteElement}
          readOnly={readOnly}
        />
      );
    }
    if (
      el.type === 'contact_no' ||
      el.type === 'contact_nc' ||
      el.type === 'contact_p' ||
      el.type === 'contact_n'
    ) {
      return (
        <ContactElementView
          key={el.id}
          element={el}
          onTagChange={handleTagChange}
          onDelete={handleDeleteElement}
          readOnly={readOnly}
        />
      );
    }
    if (
      el.type === 'coil' ||
      el.type === 'coil_normal' ||
      el.type === 'coil_negated' ||
      el.type === 'coil_set' ||
      el.type === 'coil_reset'
    ) {
      return (
        <CoilElementView
          key={el.id}
          element={el}
          onTagChange={handleTagChange}
          onDelete={handleDeleteElement}
          readOnly={readOnly}
        />
      );
    }
    return null;
  };

  const renderParallelBranch = (branch: LadderParallelBranch) => {
    const isBranchActive = Boolean(branch.isEnergized);
    return (
      <div
        key={branch.id}
        className={`my-1 mx-2 p-2 border-l-2 border-r-2 rounded relative flex flex-col gap-2 ${
          isBranchActive
            ? 'border-[#22C55E] bg-[#22C55E]/5'
            : 'border-cyan-600/70 bg-neutral-900/50'
        }`}
        data-testid={`ladder-branch-${branch.id}`}
      >
        <span className="text-[9px] font-mono text-cyan-400 absolute -top-2 left-2 px-1 bg-neutral-900 rounded">
          OR Branch
        </span>
        {branch.branches.map((subBranch, branchIdx) => (
          <div
            key={branchIdx}
            className="flex items-center gap-1 border-b border-neutral-800/60 pb-1 last:border-b-0 last:pb-0"
          >
            {subBranch.map((subEl) => renderSingleElement(subEl))}
          </div>
        ))}
      </div>
    );
  };

  // Split elements into contacts/blocks/branches (left-to-middle) and coils (right aligned)
  const leftElements = rung.elements.filter(
    (el) =>
      el.type !== 'coil' &&
      el.type !== 'coil_normal' &&
      el.type !== 'coil_negated' &&
      el.type !== 'coil_set' &&
      el.type !== 'coil_reset'
  );
  const coilElements = rung.elements.filter(
    (el) =>
      el.type === 'coil' ||
      el.type === 'coil_normal' ||
      el.type === 'coil_negated' ||
      el.type === 'coil_set' ||
      el.type === 'coil_reset'
  );

  return (
    <div
      className={`border rounded bg-[#0F141C] p-3 shadow-inner transition-colors duration-200 ${
        isRungEnergized ? 'border-[#22C55E]/80 shadow-[0_0_15px_rgba(34,197,94,0.15)]' : 'border-neutral-800/80'
      }`}
      data-testid={`rung-container-${rung.id}`}
    >
      {/* Rung Header */}
      <div className="flex items-center justify-between text-[11px] font-mono text-neutral-400 mb-2">
        <div className="flex items-center gap-2">
          <span
            className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${
              isRungEnergized ? 'bg-[#22C55E] text-black font-extrabold' : 'bg-neutral-800 text-cyan-400'
            }`}
          >
            RUNG {index + 1}
          </span>
          {readOnly ? (
            <span className="text-neutral-300">{rung.comment}</span>
          ) : (
            <input
              type="text"
              value={rung.comment}
              onChange={(e) => onUpdateRung({ ...rung, comment: e.target.value })}
              className="bg-transparent border-b border-transparent hover:border-neutral-700 focus:border-cyan-500 text-neutral-300 w-80 px-1 focus:outline-none"
              placeholder="Rung network description..."
            />
          )}
        </div>

        {!readOnly && (
          <div className="flex items-center gap-1">
            <button
              onClick={() => onDeleteRung(rung.id)}
              className="p-1 hover:bg-neutral-800 text-neutral-400 hover:text-rose-400 rounded transition-colors"
              title="Delete Rung"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        )}
      </div>

      {/* Rung Power Rails & Ladder Network */}
      <div
        className={`relative flex items-center py-4 px-3 bg-neutral-950/70 rounded border-l-4 border-r-4 transition-colors ${
          isRungEnergized
            ? 'border-l-[#22C55E] border-r-neutral-600'
            : 'border-l-blue-600 border-r-neutral-700'
        }`}
      >
        {/* Left Power Rail Indicator (Hot Rail / 24VDC) */}
        <div className="absolute left-0 top-0 bottom-0 flex flex-col justify-between py-1 pl-1 text-[8px] font-mono text-blue-400 pointer-events-none select-none">
          <span>+24V</span>
          <span>HOT</span>
        </div>

        {/* Content Flow */}
        <div className="flex items-center w-full min-h-[48px] pl-6 pr-6">
          <div className="flex items-center flex-wrap gap-1 flex-1">
            {leftElements.map((el) => renderSingleElement(el))}
          </div>

          <div className="flex items-center ml-auto">
            {coilElements.map((el) => renderSingleElement(el))}
          </div>
        </div>

        {/* Right Power Rail Indicator (Neutral / 0VDC Rail) */}
        <div className="absolute right-0 top-0 bottom-0 flex flex-col justify-between py-1 pr-1 text-[8px] font-mono text-neutral-500 pointer-events-none select-none">
          <span>0V</span>
          <span>RET</span>
        </div>
      </div>

      {/* Rung Palette Insertion Bar */}
      {!readOnly && (
        <div className="mt-2 pt-2 border-t border-neutral-800/60 flex items-center justify-between text-[10px] font-mono">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="text-neutral-500 mr-1">Insert:</span>
            <button
              onClick={() => onAddContact(rung.id, 'contact_no')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-cyan-400 text-cyan-300"
              title="Normally Open Contact"
            >
              + ┤ ├ (NO)
            </button>
            <button
              onClick={() => onAddContact(rung.id, 'contact_nc')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-amber-400 text-amber-300"
              title="Normally Closed Contact"
            >
              + ┤/├ (NC)
            </button>
            <button
              onClick={() => onAddContact(rung.id, 'contact_p')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-purple-400 text-purple-300"
              title="Positive Transition Contact (Rising Edge)"
            >
              + ┤P├ (Edge)
            </button>
            <button
              onClick={() => onAddBranch(rung.id)}
              className="flex items-center gap-1 px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-blue-400 text-blue-300"
              title="Parallel OR Branch"
            >
              <Split className="w-3 h-3" />
              <span>OR Branch</span>
            </button>
            <button
              onClick={() => onAddBlock(rung.id, 'TON')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
              title="Insert TON Timer Block"
            >
              + TON
            </button>
            <button
              onClick={() => onAddBlock(rung.id, 'CTU')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
              title="Insert CTU Counter Block"
            >
              + CTU
            </button>
          </div>

          <div className="flex items-center gap-1.5">
            <span className="text-neutral-500 mr-1">Coil:</span>
            <button
              onClick={() => onAddCoil(rung.id, 'coil')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
              title="Normal Output Coil"
            >
              + ( )
            </button>
            <button
              onClick={() => onAddCoil(rung.id, 'coil_set')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
              title="Set Coil (Latch)"
            >
              + (S)
            </button>
            <button
              onClick={() => onAddCoil(rung.id, 'coil_reset')}
              className="px-2 py-0.5 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
              title="Reset Coil (Unlatch)"
            >
              + (R)
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
