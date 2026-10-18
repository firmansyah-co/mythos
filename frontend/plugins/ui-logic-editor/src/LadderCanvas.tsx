import React, { useState } from 'react';

export interface LadderElement {
  id: string;
  type: 'contact_no' | 'contact_nc' | 'coil' | 'coil_set' | 'coil_reset';
  tag: string;
}

export interface LadderRung {
  id: string;
  comment: string;
  elements: LadderElement[];
}

export interface LadderCanvasProps {
  rungs?: LadderRung[];
  onAddRung?: () => void;
  className?: string;
}

const DEFAULT_RUNGS: LadderRung[] = [
  {
    id: 'rung-1',
    comment: 'Network 1: Feed Conveyor Run permissive interlock',
    elements: [
      { id: 'e1', type: 'contact_no', tag: 'external.DI_StartPB' },
      { id: 'e2', type: 'contact_nc', tag: 'external.DI_EStop_PB' },
      { id: 'e3', type: 'coil', tag: 'external.DO_ConveyorRun' },
    ],
  },
  {
    id: 'rung-2',
    comment: 'Network 2: Bottle Detect & Indexing Gate',
    elements: [
      { id: 'e4', type: 'contact_no', tag: 'external.DI_BottlePE' },
      { id: 'e5', type: 'coil_set', tag: 'internal.B_IndexActive' },
    ],
  },
];

export const LadderCanvas: React.FC<LadderCanvasProps> = ({
  rungs = DEFAULT_RUNGS,
  onAddRung,
  className = '',
}) => {
  const [activeRungs, setActiveRungs] = useState<LadderRung[]>(rungs);

  const renderElement = (el: LadderElement) => {
    switch (el.type) {
      case 'contact_no':
        return (
          <div key={el.id} className="flex flex-col items-center mx-2 group">
            <span className="text-[10px] font-mono text-cyan-400 mb-1">{el.tag}</span>
            <div className="flex items-center text-neutral-300 font-mono text-sm px-2 py-1 bg-neutral-900 border border-neutral-700 rounded group-hover:border-cyan-500 transition-colors">
              ──┤ ├───
            </div>
            <span className="text-[9px] text-neutral-500 mt-0.5">N.O. Contact</span>
          </div>
        );
      case 'contact_nc':
        return (
          <div key={el.id} className="flex flex-col items-center mx-2 group">
            <span className="text-[10px] font-mono text-amber-400 mb-1">{el.tag}</span>
            <div className="flex items-center text-neutral-300 font-mono text-sm px-2 py-1 bg-neutral-900 border border-neutral-700 rounded group-hover:border-amber-500 transition-colors">
              ──┤/├───
            </div>
            <span className="text-[9px] text-neutral-500 mt-0.5">N.C. Contact</span>
          </div>
        );
      case 'coil':
      case 'coil_set':
      case 'coil_reset':
        const symbol = el.type === 'coil_set' ? '(S)' : el.type === 'coil_reset' ? '(R)' : '( )';
        return (
          <div key={el.id} className="flex flex-col items-center ml-auto mr-4 group">
            <span className="text-[10px] font-mono text-emerald-400 mb-1">{el.tag}</span>
            <div className="flex items-center text-emerald-300 font-mono text-sm px-3 py-1 bg-neutral-900 border border-neutral-700 rounded group-hover:border-emerald-500 transition-colors">
              ───{symbol}──
            </div>
            <span className="text-[9px] text-neutral-500 mt-0.5">Coil Output</span>
          </div>
        );
    }
  };

  return (
    <div className={`flex flex-col w-full h-full bg-[#0A0D12] text-neutral-200 p-4 overflow-y-auto select-none ${className}`}>
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-4">
        <div>
          <h3 className="text-xs font-semibold text-neutral-300 tracking-wide uppercase">
            IEC 61131-3 Ladder Diagram (LD) Viewer
          </h3>
          <p className="text-[11px] text-neutral-500">
            Graphical Rung Representation • Synchronized with ST Engine
          </p>
        </div>
        <button
          onClick={() => {
            const newRung: LadderRung = {
              id: `rung-${activeRungs.length + 1}`,
              comment: `Network ${activeRungs.length + 1}: User Rung Logic`,
              elements: [
                { id: `e-${Date.now()}-1`, type: 'contact_no', tag: 'external.DI_Permissive' },
                { id: `e-${Date.now()}-2`, type: 'coil', tag: 'external.DO_Actuator' },
              ],
            };
            setActiveRungs([...activeRungs, newRung]);
            onAddRung?.();
          }}
          className="px-2.5 py-1 text-xs bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700 rounded transition-colors"
        >
          + Add Rung
        </button>
      </div>

      <div className="space-y-4">
        {activeRungs.map((rung, idx) => (
          <div key={rung.id} className="border border-neutral-800/80 rounded bg-[#0F141C] p-3 shadow-inner">
            <div className="text-[11px] font-mono text-neutral-400 mb-2 flex items-center gap-2">
              <span className="px-1.5 py-0.5 bg-neutral-800 text-cyan-400 text-[10px] rounded">
                RUNG {idx + 1}
              </span>
              <span>{rung.comment}</span>
            </div>
            
            {/* Rung Power Rails & Elements */}
            <div className="relative flex items-center py-4 px-2 bg-neutral-950/60 rounded border-l-4 border-r-4 border-l-blue-600 border-r-blue-600">
              <div className="flex items-center w-full">
                {rung.elements.map(renderElement)}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
