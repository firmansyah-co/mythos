import React from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { HardwareStationNode, DeviceStatus, NetworkPort, PROTOCOL_COLORS } from './types';
import {
  Cpu,
  Layers,
  Zap,
  Activity,
  Server,
  AlertTriangle,
  Radio,
} from 'lucide-react';

export interface IndustrialStationNodeData {
  station: HardwareStationNode;
  onSelectSlot?: (slotNumber: number) => void;
  onPortClick?: (port: NetworkPort) => void;
}

export const IndustrialStationNode: React.FC<NodeProps> = ({ data, selected }) => {
  const nodeData = data as unknown as IndustrialStationNodeData;
  const station = nodeData.station;

  if (!station) {
    return <div className="p-3 bg-red-900 text-white rounded">Invalid Node Data</div>;
  }

  // Visual cues based on status
  const getStatusBadge = (status: DeviceStatus) => {
    switch (status) {
      case 'RUN':
        return (
          <div className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-emerald-950/80 border border-emerald-500/40 text-emerald-400 text-[10px] font-mono font-medium">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <span>RUN</span>
          </div>
        );
      case 'STOP':
        return (
          <div className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-amber-950/80 border border-amber-500/40 text-amber-400 text-[10px] font-mono font-medium">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400" />
            <span>STOP</span>
          </div>
        );
      case 'FAULT':
        return (
          <div className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-rose-950/80 border border-rose-500/50 text-rose-400 text-[10px] font-mono font-medium">
            <AlertTriangle className="w-2.5 h-2.5 text-rose-400 animate-bounce" />
            <span>FAULT</span>
          </div>
        );
      case 'OFFLINE':
      default:
        return (
          <div className="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-slate-800/80 border border-slate-600/40 text-slate-400 text-[10px] font-mono font-medium">
            <span className="w-1.5 h-1.5 rounded-full bg-slate-500" />
            <span>OFFLINE</span>
          </div>
        );
    }
  };

  const getStationIcon = (type: HardwareStationNode['stationType']) => {
    switch (type) {
      case 'PLC_RACK':
        return <Cpu className="w-4 h-4 text-emerald-400" />;
      case 'REMOTE_IO':
        return <Layers className="w-4 h-4 text-sky-400" />;
      case 'SWITCH':
        return <Server className="w-4 h-4 text-indigo-400" />;
      case 'VFD_DRIVE':
        return <Zap className="w-4 h-4 text-amber-400" />;
      case 'FIELD_INSTRUMENT':
        return <Radio className="w-4 h-4 text-purple-400" />;
      default:
        return <Activity className="w-4 h-4 text-slate-400" />;
    }
  };

  const ports = station.ports || [];

  return (
    <div
      className={`relative min-w-[240px] max-w-[280px] rounded-lg border bg-[#0F141C] shadow-xl transition-all duration-150 select-none ${
        selected
          ? 'border-sky-500 shadow-[0_0_15px_rgba(14,165,233,0.35)] ring-1 ring-sky-500/50'
          : 'border-[#232B3B] hover:border-[#38455D]'
      }`}
    >
      {/* Station Header Bar */}
      <div className="flex items-center justify-between border-b border-[#232B3B] bg-[#161E2E] px-3 py-2 rounded-t-lg">
        <div className="flex items-center gap-2 overflow-hidden">
          <div className="p-1 rounded bg-[#0A0D12] border border-[#232B3B]">
            {getStationIcon(station.stationType)}
          </div>
          <div className="flex flex-col truncate">
            <span className="text-xs font-semibold text-[#F7F8F8] truncate" title={station.stationName}>
              {station.stationName}
            </span>
            <span className="text-[10px] text-[#8A8F98] font-mono truncate">
              {station.ipAddress || station.articleNumber || station.stationType}
            </span>
          </div>
        </div>
        <div>{getStatusBadge(station.status)}</div>
      </div>

      {/* Modular Chassis Slices Mini Preview (if PLC_RACK or REMOTE_IO) */}
      {station.slots && station.slots.length > 0 && (
        <div className="p-2.5 border-b border-[#232B3B] bg-[#0A0D12]/50">
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-[10px] font-mono uppercase text-[#8A8F98]">
              Chassis (Rack {station.rackNumber})
            </span>
            <span className="text-[10px] font-mono text-sky-400">
              {station.slots.length} slots
            </span>
          </div>

          {/* Slices Rail */}
          <div className="flex gap-1 overflow-x-auto py-1 px-1 bg-[#121824] rounded border border-[#1E2638]">
            {station.slots.slice(0, 8).map((slot) => {
              const categoryColor =
                slot.moduleCategory === 'POWER'
                  ? 'bg-amber-900/40 border-amber-600/50 text-amber-300'
                  : slot.moduleCategory === 'CPU'
                  ? 'bg-emerald-900/40 border-emerald-600/50 text-emerald-300'
                  : slot.moduleCategory === 'COMM'
                  ? 'bg-sky-900/40 border-sky-600/50 text-sky-300'
                  : 'bg-slate-800/60 border-slate-600/40 text-slate-300';

              return (
                <div
                  key={slot.slotNumber}
                  onClick={(e) => {
                    e.stopPropagation();
                    nodeData.onSelectSlot?.(slot.slotNumber);
                  }}
                  className={`flex flex-col items-center justify-center min-w-[24px] h-[34px] rounded border text-[9px] font-mono font-semibold cursor-pointer hover:scale-105 transition-transform ${categoryColor}`}
                  title={`Slot ${slot.slotNumber}: ${slot.name} (${slot.catalogArticleNo})`}
                >
                  <span className="text-[8px] opacity-75">{slot.slotNumber}</span>
                  <span className="text-[8px] uppercase truncate px-0.5">
                    {slot.moduleCategory}
                  </span>
                </div>
              );
            })}
            {station.slots.length > 8 && (
              <div className="flex items-center justify-center min-w-[20px] h-[34px] rounded bg-[#161E2E] border border-[#232B3B] text-[8px] text-[#8A8F98] font-mono">
                +{station.slots.length - 8}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Network Ports / Terminal Section */}
      <div className="p-2.5">
        <div className="flex items-center justify-between mb-1.5">
          <span className="text-[10px] font-mono uppercase text-[#8A8F98]">
            Industrial Ports
          </span>
          <span className="text-[9px] text-[#62666D] font-mono">
            {ports.length} Connected
          </span>
        </div>

        {ports.length === 0 ? (
          <div className="text-[10px] text-[#62666D] italic py-1 text-center">
            No physical ports configured
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-1.5">
            {ports.map((port, idx) => {
              const protoColor = PROTOCOL_COLORS[port.protocol]?.stroke || '#16A34A';
              const isEven = idx % 2 === 0;

              return (
                <div
                  key={port.id}
                  onClick={(e) => {
                    e.stopPropagation();
                    nodeData.onPortClick?.(port);
                  }}
                  className="relative flex items-center justify-between px-2 py-1 rounded bg-[#161E2E] border border-[#232B3B] hover:border-[#38455D] text-[10px] font-mono cursor-pointer transition-colors"
                >
                  <div className="flex items-center gap-1.5 truncate">
                    <span
                      className="w-2 h-2 rounded-full inline-block shrink-0"
                      style={{ backgroundColor: protoColor }}
                    />
                    <span className="truncate text-[#D0D6E0]">{port.name}</span>
                  </div>
                  <span
                    className={`text-[8px] px-1 rounded ${
                      port.linkStatus === 'UP'
                        ? 'text-emerald-400 bg-emerald-950/60'
                        : 'text-slate-400 bg-slate-800'
                    }`}
                  >
                    {port.connectorType}
                  </span>

                  {/* Left or Right Connection Handle */}
                  {isEven ? (
                    <Handle
                      type="target"
                      position={Position.Left}
                      id={port.id}
                      style={{
                        background: protoColor,
                        width: 8,
                        height: 8,
                        left: -5,
                        borderColor: '#0F141C',
                        borderWidth: 2,
                      }}
                    />
                  ) : (
                    <Handle
                      type="source"
                      position={Position.Right}
                      id={port.id}
                      style={{
                        background: protoColor,
                        width: 8,
                        height: 8,
                        right: -5,
                        borderColor: '#0F141C',
                        borderWidth: 2,
                      }}
                    />
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Default Input/Output Handles if no explicit ports */}
      {ports.length === 0 && (
        <>
          <Handle
            type="target"
            position={Position.Left}
            id="in"
            style={{ background: '#16A34A', width: 8, height: 8 }}
          />
          <Handle
            type="source"
            position={Position.Right}
            id="out"
            style={{ background: '#16A34A', width: 8, height: 8 }}
          />
        </>
      )}
    </div>
  );
};
