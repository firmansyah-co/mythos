import React from 'react';
import { EdgeProps, BaseEdge, EdgeLabelRenderer } from '@xyflow/react';
import { ProtocolType, PROTOCOL_COLORS, OFFLINE_BUS_COLOR } from './types';
import { computeOrthogonalBusPath } from './busRouting';

export interface IndustrialBusEdgeData {
  protocol: ProtocolType;
  label?: string;
  isOffline?: boolean;
  lengthMeters?: number;
  isSelected?: boolean;
}

export const IndustrialBusEdge: React.FC<EdgeProps> = ({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  style = {},
  markerEnd,
  data,
  selected,
}) => {
  const edgeData = (data as unknown as IndustrialBusEdgeData) || { protocol: 'PROFINET' };
  const protocol = edgeData.protocol || 'PROFINET';
  const isOffline = edgeData.isOffline || false;

  const colorDef = isOffline ? OFFLINE_BUS_COLOR : (PROTOCOL_COLORS[protocol] || PROTOCOL_COLORS.PROFINET);

  const edgePath = computeOrthogonalBusPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    gridSize: 16,
    cornerRadius: 8,
  });

  const midX = (sourceX + targetX) / 2;
  const midY = (sourceY + targetY) / 2;

  return (
    <>
      {/* Outer glow on selection */}
      {selected && (
        <path
          d={edgePath}
          fill="none"
          stroke={colorDef.stroke}
          strokeWidth={colorDef.strokeWidth + 6}
          strokeOpacity={0.3}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )}

      {/* Main Bus Cable Path */}
      <BaseEdge
        id={id}
        path={edgePath}
        style={{
          ...style,
          stroke: colorDef.stroke,
          strokeWidth: colorDef.strokeWidth,
          strokeDasharray: colorDef.strokeDasharray,
          strokeLinecap: 'round',
          strokeLinejoin: 'round',
          transition: 'stroke 0.2s, stroke-width 0.2s',
        }}
        markerEnd={markerEnd}
      />

      {/* Optional Cable Label Badge */}
      {(edgeData.label || edgeData.lengthMeters !== undefined) && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${midX}px,${midY}px)`,
              pointerEvents: 'all',
            }}
            className="nodrag nopan px-1.5 py-0.5 rounded text-[10px] font-mono font-medium border shadow-sm transition-all"
          >
            <div
              className="flex items-center gap-1.5 bg-[#0A0D12]/90 backdrop-blur-sm border-[#232B3B] px-1.5 py-0.5 rounded text-[#D0D6E0]"
              style={{ borderColor: colorDef.stroke }}
            >
              <span
                className="w-1.5 h-1.5 rounded-full inline-block"
                style={{ backgroundColor: colorDef.stroke }}
              />
              <span>{edgeData.label || colorDef.label}</span>
              {edgeData.lengthMeters !== undefined && (
                <span className="text-[#8A8F98] text-[9px] border-l border-[#232B3B] pl-1">
                  {edgeData.lengthMeters}m
                </span>
              )}
            </div>
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
};
