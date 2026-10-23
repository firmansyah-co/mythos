import React, { useState } from 'react';
import { SfcStep, SfcTransition, SfcCanvasProps } from './types';
import { Plus, ArrowDown } from 'lucide-react';

export const DEFAULT_SFC_STEPS: SfcStep[] = [
  {
    id: 'step-0',
    name: 'Init_Step',
    isInitial: true,
    isActive: true,
    x: 180,
    y: 40,
    actions: [
      { id: 'act-1', qualifier: 'N', actionName: 'DO_IdleIndicator := TRUE;' },
    ],
  },
  {
    id: 'step-1',
    name: 'Conveyor_Run',
    isInitial: false,
    isActive: false,
    x: 180,
    y: 200,
    actions: [
      { id: 'act-2', qualifier: 'S', actionName: 'DO_ConveyorMotor' },
      { id: 'act-3', qualifier: 'L', duration: 'T#5S', actionName: 'FeedWarningLight' },
    ],
  },
  {
    id: 'step-2',
    name: 'Cap_Cylinder_Dwell',
    isInitial: false,
    isActive: false,
    x: 180,
    y: 360,
    actions: [
      { id: 'act-4', qualifier: 'P', actionName: 'TriggerBottleCountInc' },
      { id: 'act-5', qualifier: 'D', duration: 'T#500MS', actionName: 'ExtendCappingArm' },
    ],
  },
];

export const DEFAULT_SFC_TRANSITIONS: SfcTransition[] = [
  {
    id: 'trans-0-1',
    name: 'T1',
    condition: 'DI_StartPB = TRUE',
    fromStepId: 'step-0',
    toStepId: 'step-1',
    isSatisfied: true,
  },
  {
    id: 'trans-1-2',
    name: 'T2',
    condition: 'DI_BottleInPosition = TRUE',
    fromStepId: 'step-1',
    toStepId: 'step-2',
    isSatisfied: false,
  },
];

export const SfcCanvas: React.FC<SfcCanvasProps> = ({
  steps = DEFAULT_SFC_STEPS,
  transitions = DEFAULT_SFC_TRANSITIONS,
  onAddStep,
  onDeleteStep,
  readOnly = false,
  className = '',
  debugActive = false,
  liveTelemetry = {},
}) => {
  const [activeSteps, setActiveSteps] = useState<SfcStep[]>(steps);
  const [activeTransitions] = useState<SfcTransition[]>(transitions);

  const handleAddStep = () => {
    const newIdx = activeSteps.length;
    const newStep: SfcStep = {
      id: `step-${Date.now()}`,
      name: `Step_${newIdx}`,
      isInitial: false,
      isActive: false,
      x: 180,
      y: 40 + newIdx * 160,
      actions: [
        { id: `act-${Date.now()}`, qualifier: 'N', actionName: `Action_${newIdx}` },
      ],
    };
    setActiveSteps([...activeSteps, newStep]);
    onAddStep?.(newStep);
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-base)] text-neutral-200 p-4 select-none overflow-auto ${className}`}
      data-testid="sfc-canvas"
    >
      {/* Top Header */}
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-4 bg-[var(--bg-base)]">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-xs font-semibold text-neutral-200 tracking-wide uppercase">
              IEC 61131-3 Sequential Function Chart (SFC) Canvas
            </h3>
            <span className="px-1.5 py-0.2 rounded bg-neutral-800 text-[10px] text-cyan-400 font-mono">
              State Machine Engine
            </span>
            {debugActive && (
              <span className="px-1.5 py-0.2 rounded bg-emerald-950/60 text-emerald-400 border border-emerald-800/80 text-[10px] font-mono animate-pulse">
                ACTIVE STEP TOKENS ON
              </span>
            )}
          </div>
          <p className="text-[11px] text-neutral-500">
            Initial & Regular Steps • Transition Boolean Gates • Action Blocks with Qualifiers (N, S, R, P, L, D)
          </p>
        </div>

        {!readOnly && (
          <button
            onClick={handleAddStep}
            className="flex items-center gap-1.5 px-3 py-1 text-xs bg-blue-600 hover:bg-blue-500 text-white rounded font-medium shadow transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Add Step</span>
          </button>
        )}
      </div>

      {/* SFC Sequence Viewer */}
      <div className="relative w-full h-[650px] border border-neutral-800/60 rounded bg-[var(--bg-base)] overflow-y-auto p-8 flex flex-col items-center">
        {activeSteps.map((step, idx) => {
          const trans = activeTransitions.find((t) => t.fromStepId === step.id);
          const isStepActive = Boolean(step.isActive || (debugActive && (idx === 0 || liveTelemetry[step.name] === true)));

          return (
            <React.Fragment key={step.id}>
              {/* Step & Associated Action Blocks */}
              <div className="flex items-start gap-4 z-10" data-testid={`sfc-step-${step.id}`}>
                {/* Step Box */}
                <div
                  className={`w-32 h-14 rounded flex flex-col items-center justify-center font-mono relative transition-all duration-200 ${
                    step.isInitial
                      ? 'border-4 border-double border-cyan-400 bg-neutral-900 shadow-md'
                      : 'border-2 border-neutral-600 bg-neutral-900'
                  } ${
                    isStepActive
                      ? 'border-[var(--bus-ethernet)] bg-[var(--bus-ethernet)]/20 text-[var(--bus-ethernet)] drop-shadow-[0_0_10px_rgba(34,197,94,0.6)]'
                      : 'text-neutral-200'
                  }`}
                >
                  {isStepActive && (
                    <span className="absolute -top-2 -left-2 w-4 h-4 rounded-full bg-[var(--bus-ethernet)] animate-ping" />
                  )}
                  <div className="flex items-center gap-1">
                    <span className="font-bold text-xs">{step.name}</span>
                    {!readOnly && (
                      <button
                        onClick={() => {
                          setActiveSteps(activeSteps.filter(s => s.id !== step.id));
                          onDeleteStep?.(step.id);
                        }}
                        className="text-neutral-500 hover:text-rose-400 text-[10px]"
                        title="Delete step"
                      >
                        ×
                      </button>
                    )}
                  </div>
                  <span className="text-[9px] text-neutral-400">
                    {step.isInitial ? 'Initial Step' : `Step #${idx}`}
                  </span>
                </div>

                {/* IEC 61131-3 Action Blocks */}
                <div className="flex flex-col gap-1.5">
                  {step.actions.map((act) => (
                    <div
                      key={act.id}
                      className="flex items-center border border-neutral-700 rounded bg-[var(--bg-surface)] text-[11px] font-mono shadow overflow-hidden"
                    >
                      {/* Qualifier Box (N, S, R, P, L, D) */}
                      <span className="px-2 py-1 bg-cyan-900/40 text-cyan-300 font-bold border-r border-neutral-700 min-w-[28px] text-center">
                        {act.qualifier}
                      </span>
                      {/* Duration for L / D */}
                      {act.duration && (
                        <span className="px-1.5 py-1 text-amber-400 border-r border-neutral-700 text-[10px]">
                          {act.duration}
                        </span>
                      )}
                      {/* Action Expression */}
                      <span className="px-2.5 py-1 text-neutral-300">{act.actionName}</span>
                    </div>
                  ))}
                </div>
              </div>

              {/* Transition Link */}
              {trans && (
                <div className="flex flex-col items-center my-3 z-0">
                  {/* Vertical Wire */}
                  <div
                    className={`w-0.5 h-6 ${
                      trans.isSatisfied || debugActive ? 'bg-[var(--bus-ethernet)]' : 'bg-neutral-600'
                    }`}
                  />

                  {/* Transition Cross-Bar */}
                  <div className="flex items-center gap-2">
                    <div
                      className={`h-1 w-10 ${
                        trans.isSatisfied || debugActive
                          ? 'bg-[var(--bus-ethernet)] shadow-[0_0_8px_var(--bus-ethernet)]'
                          : 'bg-neutral-400'
                      }`}
                    />
                    <div className="flex flex-col font-mono text-[10px]">
                      <span className="text-cyan-400 font-bold">{trans.name}</span>
                      <span className="text-neutral-400 bg-neutral-900 px-1.5 py-0.5 rounded border border-neutral-800">
                        {trans.condition}
                      </span>
                    </div>
                  </div>

                  {/* Vertical Wire down to next step */}
                  <div
                    className={`w-0.5 h-6 ${
                      trans.isSatisfied || debugActive ? 'bg-[var(--bus-ethernet)]' : 'bg-neutral-600'
                    }`}
                  />
                  <ArrowDown
                    className={`w-3.5 h-3.5 -mt-1 ${
                      trans.isSatisfied || debugActive ? 'text-[var(--bus-ethernet)]' : 'text-neutral-600'
                    }`}
                  />
                </div>
              )}
            </React.Fragment>
          );
        })}
      </div>
    </div>
  );
};
