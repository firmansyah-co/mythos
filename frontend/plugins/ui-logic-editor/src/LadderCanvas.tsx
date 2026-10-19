import React, { useState, useEffect } from 'react';
import {
  LadderRung,
  LadderCanvasProps,
  LadderContactType,
  LadderCoilType,
  LadderBlockElement,
  LadderElement,
} from './ladder/types';
import { RungRenderer } from './ladder/RungRenderer';
import { Plus, Zap } from 'lucide-react';

export const DEFAULT_LD_RUNGS: LadderRung[] = [
  {
    id: 'rung-1',
    comment: 'Network 1: Feed Conveyor Run permissive interlock',
    isEnergized: false,
    elements: [
      { id: 'e1', type: 'contact_no', tag: 'external.DI_StartPB', isEnergized: false },
      { id: 'e2', type: 'contact_nc', tag: 'external.DI_EStop_PB', isEnergized: true },
      { id: 'e3', type: 'coil', tag: 'external.DO_ConveyorRun', isEnergized: false },
    ],
  },
  {
    id: 'rung-2',
    comment: 'Network 2: Bottle Detect & Indexing Gate with TON Timer',
    isEnergized: false,
    elements: [
      { id: 'e4', type: 'contact_no', tag: 'external.DI_BottlePE', isEnergized: false },
      {
        id: 'e-block-1',
        type: 'function_block',
        blockType: 'TON',
        instanceName: 'TON_BottleDwell',
        inputs: [
          { name: 'IN', type: 'BOOL', binding: 'DI_BottlePE' },
          { name: 'PT', type: 'TIME', binding: 'T#500MS' },
        ],
        outputs: [
          { name: 'Q', type: 'BOOL', binding: 'Gate_Open' },
          { name: 'ET', type: 'TIME', binding: 'Dwell_Elapsed' },
        ],
      },
      { id: 'e5', type: 'coil_set', tag: 'internal.B_IndexActive', isEnergized: false },
    ],
  },
  {
    id: 'rung-3',
    comment: 'Network 3: Dual Emergency Stop Bypass (Parallel OR Branch)',
    isEnergized: false,
    elements: [
      {
        id: 'branch-1',
        type: 'branch',
        isEnergized: false,
        branches: [
          [
            { id: 'b1-e1', type: 'contact_no', tag: 'DI_ManualPB', isEnergized: false },
          ],
          [
            { id: 'b2-e1', type: 'contact_no', tag: 'DI_AutoPermissive', isEnergized: false },
            { id: 'b2-e2', type: 'contact_nc', tag: 'DI_FaultAlarm', isEnergized: true },
          ],
        ],
      },
      { id: 'e6', type: 'coil', tag: 'DO_MainPowerContactor', isEnergized: false },
    ],
  },
];

export const LadderCanvas: React.FC<LadderCanvasProps> = ({
  rungs = DEFAULT_LD_RUNGS,
  onAddRung,
  onUpdateRung,
  onDeleteRung,
  targetBlock = 'MainTask',
  readOnly = false,
  className = '',
  debugActive = false,
  liveTelemetry = {},
  onForceVariable,
}) => {
  const [activeRungs, setActiveRungs] = useState<LadderRung[]>(rungs);

  useEffect(() => {
    setActiveRungs(rungs);
  }, [rungs]);

  // Evaluate power-flow when debugActive or liveTelemetry updates
  useEffect(() => {
    if (!debugActive) return;

    const evalElementEnergized = (el: LadderElement): LadderElement => {
      if (el.type === 'contact_no') {
        const val = Boolean(liveTelemetry[el.tag]);
        return { ...el, isEnergized: val };
      }
      if (el.type === 'contact_nc') {
        const val = !Boolean(liveTelemetry[el.tag]);
        return { ...el, isEnergized: val };
      }
      if (el.type === 'contact_p' || el.type === 'contact_n') {
        const val = Boolean(liveTelemetry[el.tag]);
        return { ...el, isEnergized: val };
      }
      if (el.type === 'branch') {
        let branchAnyTrue = false;
        const newBranches = el.branches.map((sub) => {
          let branchAllTrue = true;
          const evaluatedSub = sub.map((subEl) => {
            const evalSub = evalElementEnergized(subEl);
            if (!evalSub.isEnergized) branchAllTrue = false;
            return evalSub;
          });
          if (branchAllTrue && sub.length > 0) branchAnyTrue = true;
          return evaluatedSub;
        });
        return { ...el, branches: newBranches, isEnergized: branchAnyTrue };
      }
      if (el.type === 'function_block') {
        const inPin = el.inputs.find((i) => i.name === 'IN' || i.name === 'CU');
        const isTrue = inPin?.binding ? Boolean(liveTelemetry[inPin.binding]) : false;
        return { ...el, isEnergized: isTrue };
      }
      if (
        el.type === 'coil' ||
        el.type === 'coil_normal' ||
        el.type === 'coil_negated' ||
        el.type === 'coil_set' ||
        el.type === 'coil_reset'
      ) {
        const val = Boolean(liveTelemetry[el.tag]);
        return { ...el, isEnergized: val };
      }
      return el;
    };

    const evaluatedRungs = activeRungs.map((r) => {
      const leftElements = r.elements.filter(
        (el) => !['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(el.type)
      );
      const evalLeft = leftElements.map(evalElementEnergized);
      const rungEnergized =
        evalLeft.length === 0 || evalLeft.every((e) => Boolean(e.isEnergized));

      const updatedElements = r.elements.map((el) => {
        if (['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(el.type)) {
          return { ...el, isEnergized: rungEnergized };
        }
        return evalElementEnergized(el);
      });

      return {
        ...r,
        isEnergized: rungEnergized,
        elements: updatedElements,
      };
    });

    setActiveRungs(evaluatedRungs);
  }, [debugActive, liveTelemetry]);

  const handleUpdateRung = (updated: LadderRung) => {
    const next = activeRungs.map((r) => (r.id === updated.id ? updated : r));
    setActiveRungs(next);
    onUpdateRung?.(updated.id, updated);
  };

  const handleDeleteRung = (rungId: string) => {
    const next = activeRungs.filter((r) => r.id !== rungId);
    setActiveRungs(next);
    onDeleteRung?.(rungId);
  };

  const handleAddContact = (rungId: string, type: LadderContactType) => {
    const next = activeRungs.map((r) => {
      if (r.id !== rungId) return r;
      const newEl = {
        id: `c-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
        type,
        tag: 'VAR_' + (r.elements.length + 1),
      };
      // Insert contact before coils
      const coils = r.elements.filter((e) =>
        ['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      const nonCoils = r.elements.filter(
        (e) => !['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      return {
        ...r,
        elements: [...nonCoils, newEl, ...coils],
      };
    });
    setActiveRungs(next);
  };

  const handleAddCoil = (rungId: string, type: LadderCoilType) => {
    const next = activeRungs.map((r) => {
      if (r.id !== rungId) return r;
      const newEl = {
        id: `coil-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
        type,
        tag: 'OUT_' + (r.elements.length + 1),
      };
      return {
        ...r,
        elements: [...r.elements, newEl],
      };
    });
    setActiveRungs(next);
  };

  const handleAddBranch = (rungId: string) => {
    const next = activeRungs.map((r) => {
      if (r.id !== rungId) return r;
      const branchEl: LadderElement = {
        id: `branch-${Date.now()}`,
        type: 'branch',
        branches: [
          [{ id: `sub1-${Date.now()}`, type: 'contact_no', tag: 'IN_Branch1' }],
          [{ id: `sub2-${Date.now()}`, type: 'contact_no', tag: 'IN_Branch2' }],
        ],
      };
      const coils = r.elements.filter((e) =>
        ['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      const nonCoils = r.elements.filter(
        (e) => !['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      return {
        ...r,
        elements: [...nonCoils, branchEl, ...coils],
      };
    });
    setActiveRungs(next);
  };

  const handleAddBlock = (rungId: string, blockType: string) => {
    const next = activeRungs.map((r) => {
      if (r.id !== rungId) return r;
      const blockEl: LadderBlockElement = {
        id: `fb-${Date.now()}`,
        type: 'function_block',
        blockType,
        instanceName: `${blockType}_Inst_${Date.now().toString().slice(-4)}`,
        inputs:
          blockType === 'TON' || blockType === 'TOF' || blockType === 'TP'
            ? [
                { name: 'IN', type: 'BOOL', binding: 'Timer_In' },
                { name: 'PT', type: 'TIME', binding: 'T#1S' },
              ]
            : [
                { name: 'CU', type: 'BOOL', binding: 'Count_Up' },
                { name: 'PV', type: 'INT', binding: '10' },
              ],
        outputs:
          blockType === 'TON' || blockType === 'TOF' || blockType === 'TP'
            ? [
                { name: 'Q', type: 'BOOL', binding: 'Timer_Q' },
                { name: 'ET', type: 'TIME', binding: 'Timer_ET' },
              ]
            : [
                { name: 'Q', type: 'BOOL', binding: 'Count_Q' },
                { name: 'CV', type: 'INT', binding: 'Count_CV' },
              ],
      };
      const coils = r.elements.filter((e) =>
        ['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      const nonCoils = r.elements.filter(
        (e) => !['coil', 'coil_normal', 'coil_negated', 'coil_set', 'coil_reset'].includes(e.type)
      );
      return {
        ...r,
        elements: [...nonCoils, blockEl, ...coils],
      };
    });
    setActiveRungs(next);
  };

  const handleForce = (tag: string, val: any) => { onForceVariable?.(tag, val); };
  void handleForce;

  const handleAddNewRung = () => {
    const newRung: LadderRung = {
      id: `rung-${Date.now()}`,
      comment: `Network ${activeRungs.length + 1}: User Process Logic`,
      elements: [
        { id: `c-${Date.now()}-1`, type: 'contact_no', tag: 'DI_Permissive' },
        { id: `c-${Date.now()}-2`, type: 'coil', tag: 'DO_Actuator' },
      ],
    };
    setActiveRungs([...activeRungs, newRung]);
    onAddRung?.();
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[#0A0D12] text-neutral-200 p-4 overflow-y-auto select-none ${className}`}
      data-testid="ladder-canvas"
    >
      {/* Top Banner Toolbar */}
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-4">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-xs font-semibold text-neutral-200 tracking-wide uppercase">
              IEC 61131-3 Ladder Diagram (LD) Canvas
            </h3>
            <span className="px-1.5 py-0.2 rounded bg-neutral-800 text-[10px] text-cyan-400 font-mono">
              Target: {targetBlock}
            </span>
            {debugActive && (
              <span className="flex items-center gap-1 px-1.5 py-0.2 rounded bg-emerald-950/60 text-emerald-400 border border-emerald-800/80 text-[10px] font-mono animate-pulse">
                <Zap className="w-3 h-3 fill-current" />
                POWER FLOW ONLINE
              </span>
            )}
          </div>
          <p className="text-[11px] text-neutral-500">
            OpenPLC Editor v4 Graphical Parity • Dual 24VDC Power Rails • Parallel OR Branches • Function Blocks
          </p>
        </div>

        {!readOnly && (
          <div className="flex items-center gap-2">
            <button
              onClick={handleAddNewRung}
              data-testid="add-rung-button"
              className="flex items-center gap-1.5 px-3 py-1 text-xs bg-blue-600 hover:bg-blue-500 text-white rounded font-medium shadow transition-colors cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>Add Rung</span>
            </button>
          </div>
        )}
      </div>

      {/* Rungs List */}
      <div className="space-y-4">
        {activeRungs.map((rung, idx) => (
          <RungRenderer
            key={rung.id}
            rung={rung}
            index={idx}
            readOnly={readOnly}
            onUpdateRung={handleUpdateRung}
            onDeleteRung={handleDeleteRung}
            onAddContact={handleAddContact}
            onAddCoil={handleAddCoil}
            onAddBranch={handleAddBranch}
            onAddBlock={handleAddBlock}
          />
        ))}
      </div>
    </div>
  );
};
