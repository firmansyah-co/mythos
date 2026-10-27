import React, { useState } from 'react';
import { FbdBlock, FbdWire, FbdCanvasProps } from './types';
import { EmptySurface, EMPTY_SURFACE } from '@mythos/ui-empty-state';




export const FbdCanvas: React.FC<FbdCanvasProps> = ({
  blocks,
  wires,
  onAddBlock,
  onDeleteBlock,
  readOnly = false,
  className = '',
  debugActive = false,
  liveTelemetry = {},
}) => {
  const [activeBlocks, setActiveBlocks] = useState<FbdBlock[]>(blocks ?? []);
  const [activeWires, setActiveWires] = useState<FbdWire[]>(wires ?? []);
  const [selectedPin, setSelectedPin] = useState<{
    blockId: string;
    pinId: string;
    isOutput: boolean;
  } | null>(null);

  const handleToggleNegation = (blockId: string, pinId: string) => {
    if (readOnly) return;
    setActiveBlocks((prev) =>
      prev.map((b) => {
        if (b.id !== blockId) return b;
        return {
          ...b,
          inputs: b.inputs.map((p) => (p.id === pinId ? { ...p, isNegated: !p.isNegated } : p)),
        };
      })
    );
  };

  const handlePinClick = (blockId: string, pinId: string, isOutput: boolean) => {
    if (readOnly) return;
    if (!selectedPin) {
      if (isOutput) {
        setSelectedPin({ blockId, pinId, isOutput: true });
      }
    } else {
      if (!isOutput && selectedPin.blockId !== blockId) {
        // Create new wire connection
        const newWire: FbdWire = {
          id: `wire-${Date.now()}`,
          fromBlockId: selectedPin.blockId,
          fromPinId: selectedPin.pinId,
          toBlockId: blockId,
          toPinId: pinId,
          isEnergized: false,
        };
        setActiveWires((w) => [...w, newWire]);
        setSelectedPin(null);
      } else {
        setSelectedPin(null);
      }
    }
  };

  const handleAddStandardBlock = (type: string, category: any) => {
    const newBlock: FbdBlock = {
      id: `blk-${type.toLowerCase()}-${Date.now()}`,
      name: `${type}_${activeBlocks.length + 1}`,
      blockType: type as any,
      category,
      x: 100 + (activeBlocks.length % 4) * 150,
      y: 200 + Math.floor(activeBlocks.length / 4) * 100,
      inputs: [
        { id: `pin-${Date.now()}-in1`, name: 'IN1', dataType: 'BOOL' },
        { id: `pin-${Date.now()}-in2`, name: 'IN2', dataType: 'BOOL' },
      ],
      outputs: [{ id: `pin-${Date.now()}-out`, name: 'OUT', dataType: 'BOOL' }],
    };
    setActiveBlocks([...activeBlocks, newBlock]);
    onAddBlock?.(newBlock);
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-base)] text-neutral-200 p-4 select-none overflow-auto relative ${className}`}
      data-testid="fbd-canvas"
    >
      {/* Top Header & Palette */}
      <div className="flex items-center justify-between pb-3 border-b border-neutral-800 mb-4 z-10 bg-[var(--bg-base)]/90 backdrop-blur-sm">
        <div>
          <div className="flex items-center gap-2">
            <h3 className="text-xs font-semibold text-neutral-200 tracking-wide uppercase">
              IEC 61131-3 Function Block Diagram (FBD) Canvas
            </h3>
            <span className="px-1.5 py-0.2 rounded bg-neutral-800 text-[10px] text-cyan-400 font-mono">
              OpenPLC Parity
            </span>
            {debugActive && (
              <span className="px-1.5 py-0.2 rounded bg-emerald-950/60 text-emerald-400 border border-emerald-800/80 text-[10px] font-mono animate-pulse">
                LIVE TELEMETRY
              </span>
            )}
          </div>
          <p className="text-[11px] text-neutral-500">
            Modular Logic Blocks (AND/OR/XOR/Math/PID) • Pin Negation Bubbles • Routed Connections
          </p>
        </div>

        {!readOnly && (
          <div className="flex items-center gap-1.5 text-xs font-mono">
            <span className="text-neutral-500 mr-1 text-[11px]">Add Block:</span>
            <button
              onClick={() => handleAddStandardBlock('AND', 'LOGIC')}
              className="px-2 py-1 rounded bg-neutral-900 border border-neutral-700 hover:border-cyan-400 text-cyan-300"
            >
              + AND
            </button>
            <button
              onClick={() => handleAddStandardBlock('OR', 'LOGIC')}
              className="px-2 py-1 rounded bg-neutral-900 border border-neutral-700 hover:border-cyan-400 text-cyan-300"
            >
              + OR
            </button>
            <button
              onClick={() => handleAddStandardBlock('NOT', 'LOGIC')}
              className="px-2 py-1 rounded bg-neutral-900 border border-neutral-700 hover:border-amber-400 text-amber-300"
            >
              + NOT
            </button>
            <button
              onClick={() => handleAddStandardBlock('ADD', 'MATH')}
              className="px-2 py-1 rounded bg-neutral-900 border border-neutral-700 hover:border-emerald-400 text-emerald-300"
            >
              + ADD
            </button>
            <button
              onClick={() => handleAddStandardBlock('PID', 'CONTROL')}
              className="px-2 py-1 rounded bg-neutral-900 border border-neutral-700 hover:border-purple-400 text-purple-300"
            >
              + PID
            </button>
          </div>
        )}
      </div>

      {/* SVG Canvas for Wires & Connections */}
      <div className="relative w-full h-[650px] border border-neutral-800/60 rounded bg-[var(--bg-base)] overflow-hidden">
        {activeBlocks.length === 0 && (
          <div className="absolute inset-0 z-20 flex items-center justify-center p-6">
            <EmptySurface copy={EMPTY_SURFACE.logic} />
          </div>
        )}
        {/* Wire Connections Layer */}
        <svg className="absolute inset-0 w-full h-full pointer-events-none z-0">
          <defs>
            <marker
              id="fbd-arrow"
              viewBox="0 0 10 10"
              refX="6"
              refY="5"
              markerWidth="6"
              markerHeight="6"
              orient="auto-start-reverse"
            >
              <path d="M 0 1 L 8 5 L 0 9 z" fill="var(--border-focus)" />
            </marker>
          </defs>

          {activeWires.map((wire) => {
            const fromBlock = activeBlocks.find((b) => b.id === wire.fromBlockId);
            const toBlock = activeBlocks.find((b) => b.id === wire.toBlockId);
            if (!fromBlock || !toBlock) return null;

            // Approximate coordinates based on block layout
            const startX = fromBlock.x + 180;
            const startY = fromBlock.y + 40;
            const endX = toBlock.x;
            const endY = toBlock.y + 40;
            const midX = (startX + endX) / 2;

            return (
              <path
                key={wire.id}
                d={`M ${startX} ${startY} C ${midX} ${startY}, ${midX} ${endY}, ${endX} ${endY}`}
                stroke={wire.isEnergized || debugActive ? 'var(--bus-ethernet)' : 'var(--border-focus)'}
                strokeWidth={wire.isEnergized || debugActive ? '3' : '2'}
                fill="none"
                markerEnd="url(#fbd-arrow)"
                className={wire.isEnergized || debugActive ? 'drop-shadow-[0_0_6px_rgba(34,197,94,0.8)]' : ''}
              />
            );
          })}
        </svg>

        {/* Modular Blocks Display */}
        {activeBlocks.map((blk) => (
          <div
            key={blk.id}
            style={{ transform: `translate(${blk.x}px, ${blk.y}px)` }}
            className="absolute w-[200px] bg-[var(--bg-surface)] border border-neutral-700/80 rounded-md shadow-lg z-10 font-mono text-xs select-none"
            data-testid={`fbd-block-${blk.id}`}
          >
            {/* Block Header */}
            <div className="flex items-center justify-between px-2.5 py-1.5 bg-[var(--bg-surface)] border-b border-neutral-700/70">
              <span className="font-bold text-cyan-300 text-[11px]">{blk.blockType}</span>
              <div className="flex items-center gap-1">
                <span className="text-neutral-400 text-[10px]">{blk.name}</span>
                {!readOnly && (
                  <button
                    onClick={() => {
                      setActiveBlocks(activeBlocks.filter(b => b.id !== blk.id));
                      onDeleteBlock?.(blk.id);
                    }}
                    className="text-neutral-500 hover:text-rose-400 ml-1"
                    title="Delete block"
                  >
                    ×
                  </button>
                )}
              </div>
            </div>

            {/* Block Body & Pins */}
            <div className="p-2 flex justify-between gap-3 min-h-[70px]">
              {/* Inputs */}
              <div className="flex flex-col gap-2">
                {blk.inputs.map((pin) => (
                  <div
                    key={pin.id}
                    onClick={() => handlePinClick(blk.id, pin.id, false)}
                    className="flex items-center gap-1 group cursor-pointer"
                  >
                    {/* Inversion Bubble Toggle */}
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleToggleNegation(blk.id, pin.id);
                      }}
                      className="w-3 h-3 flex items-center justify-center rounded-full border border-neutral-500 text-[8px] hover:border-cyan-400"
                      title={pin.isNegated ? 'Inverted (Active Low)' : 'Normal (Active High)'}
                    >
                      {pin.isNegated ? 'o' : '•'}
                    </button>
                    <span className="text-[10px] text-neutral-300 font-semibold">{pin.name}</span>
                    <span className="text-[9px] text-neutral-500">({pin.dataType})</span>
                  </div>
                ))}
              </div>

              {/* Outputs */}
              <div className="flex flex-col gap-2 items-end">
                {blk.outputs.map((pin) => (
                  <div
                    key={pin.id}
                    onClick={() => handlePinClick(blk.id, pin.id, true)}
                    className={`flex items-center gap-1 group cursor-pointer ${
                      selectedPin?.pinId === pin.id ? 'ring-1 ring-cyan-400 rounded px-1' : ''
                    }`}
                  >
                    <span className="text-[9px] text-neutral-500">({pin.dataType})</span>
                    <span className="text-[10px] text-emerald-400 font-semibold">{liveTelemetry[pin.name] !== undefined ? `${pin.name}=${String(liveTelemetry[pin.name])}` : pin.name}</span>
                    <span className="w-2 h-2 rounded-full bg-emerald-500 group-hover:scale-125 transition-transform" />
                  </div>
                ))}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
