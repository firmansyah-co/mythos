import React from 'react';
import {
  LadderContactElement,
  LadderCoilElement,
  LadderBlockElement,
  LadderElement,
} from './types';

export interface ElementRendererProps {
  element: LadderElement;
  onTagChange?: (id: string, newTag: string) => void;
  onDelete?: (id: string) => void;
  readOnly?: boolean;
}

export const ContactElementView: React.FC<{
  element: LadderContactElement;
  onTagChange?: (id: string, newTag: string) => void;
  onDelete?: (id: string) => void;
  readOnly?: boolean;
}> = ({ element, onTagChange, onDelete, readOnly }) => {
  const isEnergized = Boolean(element.isEnergized);

  const getSymbol = () => {
    switch (element.type) {
      case 'contact_no':
        return '──┤ ├───';
      case 'contact_nc':
        return '──┤/├───';
      case 'contact_p':
        return '──┤P├───';
      case 'contact_n':
        return '──┤N├───';
      default:
        return '──┤ ├───';
    }
  };

  const getSubLabel = () => {
    switch (element.type) {
      case 'contact_no':
        return 'N.O. Contact';
      case 'contact_nc':
        return 'N.C. Contact';
      case 'contact_p':
        return 'Rising Edge (P)';
      case 'contact_n':
        return 'Falling Edge (N)';
    }
  };

  const wireColor = isEnergized
    ? 'text-[#22C55E] drop-shadow-[0_0_8px_rgba(34,197,94,0.8)]'
    : 'text-neutral-400';

  const borderColor = isEnergized
    ? 'border-[#22C55E] bg-[#22C55E]/10'
    : 'border-neutral-700 bg-neutral-900 group-hover:border-cyan-500';

  return (
    <div className="flex flex-col items-center mx-2 group relative cursor-pointer" data-testid={`ladder-contact-${element.id}`}>
      {readOnly ? (
        <span className={`text-[10px] font-mono mb-1 ${isEnergized ? 'text-emerald-400 font-bold' : 'text-cyan-400'}`}>
          {element.tag}
        </span>
      ) : (
        <input
          type="text"
          value={element.tag}
          onChange={(e) => onTagChange?.(element.id, e.target.value)}
          className="text-[10px] font-mono text-cyan-400 bg-transparent border-b border-transparent hover:border-cyan-500 focus:border-cyan-400 text-center w-28 focus:outline-none mb-1"
        />
      )}

      <div className={`flex items-center font-mono text-sm px-2.5 py-1 border rounded transition-all duration-200 ${borderColor} ${wireColor}`}>
        <span className="font-bold tracking-tight">{getSymbol()}</span>
      </div>

      <span className="text-[9px] text-neutral-500 mt-0.5">{getSubLabel()}</span>

      {!readOnly && onDelete && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete(element.id);
          }}
          className="absolute -top-2 -right-2 hidden group-hover:flex w-4 h-4 bg-rose-600 hover:bg-rose-500 rounded-full text-white text-[9px] items-center justify-center shadow"
          title="Delete element"
        >
          ×
        </button>
      )}
    </div>
  );
};

export const CoilElementView: React.FC<{
  element: LadderCoilElement;
  onTagChange?: (id: string, newTag: string) => void;
  onDelete?: (id: string) => void;
  readOnly?: boolean;
}> = ({ element, onTagChange, onDelete, readOnly }) => {
  const isEnergized = Boolean(element.isEnergized);

  const getSymbol = () => {
    switch (element.type) {
      case 'coil_set':
        return '(S)';
      case 'coil_reset':
        return '(R)';
      case 'coil_negated':
        return '(/)';
      case 'coil':
      case 'coil_normal':
      default:
        return '( )';
    }
  };

  const getSubLabel = () => {
    switch (element.type) {
      case 'coil_set':
        return 'Set Coil (Latch)';
      case 'coil_reset':
        return 'Reset Coil (Unlatch)';
      case 'coil_negated':
        return 'Negated Coil';
      case 'coil':
      case 'coil_normal':
      default:
        return 'Normal Coil';
    }
  };

  const wireColor = isEnergized
    ? 'text-[#22C55E] drop-shadow-[0_0_8px_rgba(34,197,94,0.8)]'
    : 'text-emerald-400';

  const borderColor = isEnergized
    ? 'border-[#22C55E] bg-[#22C55E]/15'
    : 'border-neutral-700 bg-neutral-900 group-hover:border-emerald-500';

  return (
    <div className="flex flex-col items-center ml-auto mr-4 group relative cursor-pointer" data-testid={`ladder-coil-${element.id}`}>
      {readOnly ? (
        <span className={`text-[10px] font-mono mb-1 ${isEnergized ? 'text-emerald-300 font-bold' : 'text-emerald-400'}`}>
          {element.tag}
        </span>
      ) : (
        <input
          type="text"
          value={element.tag}
          onChange={(e) => onTagChange?.(element.id, e.target.value)}
          className="text-[10px] font-mono text-emerald-400 bg-transparent border-b border-transparent hover:border-emerald-500 focus:border-emerald-400 text-center w-28 focus:outline-none mb-1"
        />
      )}

      <div className={`flex items-center font-mono text-sm px-3 py-1 border rounded transition-all duration-200 ${borderColor} ${wireColor}`}>
        <span className="font-bold tracking-tight">───{getSymbol()}───</span>
      </div>

      <span className="text-[9px] text-neutral-500 mt-0.5">{getSubLabel()}</span>

      {!readOnly && onDelete && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete(element.id);
          }}
          className="absolute -top-2 -right-2 hidden group-hover:flex w-4 h-4 bg-rose-600 hover:bg-rose-500 rounded-full text-white text-[9px] items-center justify-center shadow"
          title="Delete element"
        >
          ×
        </button>
      )}
    </div>
  );
};

export const FunctionBlockElementView: React.FC<{
  element: LadderBlockElement;
  onDelete?: (id: string) => void;
  readOnly?: boolean;
}> = ({ element, onDelete, readOnly }) => {
  const isEnergized = Boolean(element.isEnergized);

  return (
    <div
      className={`mx-3 my-1 flex flex-col bg-[#141B26] border rounded shadow-md min-w-[140px] text-xs font-mono relative group ${
        isEnergized
          ? 'border-[#22C55E] shadow-[0_0_12px_rgba(34,197,94,0.3)]'
          : 'border-cyan-800/80 hover:border-cyan-500'
      }`}
      data-testid={`ladder-block-${element.id}`}
    >
      {/* Block Header */}
      <div className="bg-[#1B2433] px-2.5 py-1 border-b border-neutral-700/80 flex items-center justify-between text-[11px]">
        <span className="font-bold text-cyan-300">{element.blockType}</span>
        <span className="text-neutral-400 text-[10px]">{element.instanceName}</span>
      </div>

      {/* Block Body / Ports */}
      <div className="p-2 flex justify-between gap-4">
        {/* Inputs */}
        <div className="space-y-1">
          {element.inputs.map((inp, idx) => (
            <div key={idx} className="flex items-center gap-1 text-[10px]">
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
              <span className="text-neutral-300 font-semibold">{inp.name}</span>
              {inp.binding && <span className="text-neutral-500">({inp.binding})</span>}
            </div>
          ))}
        </div>

        {/* Outputs */}
        <div className="space-y-1 text-right">
          {element.outputs.map((out, idx) => (
            <div key={idx} className="flex items-center justify-end gap-1 text-[10px]">
              {out.binding && <span className="text-neutral-500">({out.binding})</span>}
              <span className="text-emerald-400 font-semibold">{out.name}</span>
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
            </div>
          ))}
        </div>
      </div>

      {!readOnly && onDelete && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDelete(element.id);
          }}
          className="absolute -top-2 -right-2 hidden group-hover:flex w-4 h-4 bg-rose-600 hover:bg-rose-500 rounded-full text-white text-[9px] items-center justify-center shadow"
          title="Delete block"
        >
          ×
        </button>
      )}
    </div>
  );
};
