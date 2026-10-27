import React, { useState, useCallback, useMemo } from 'react';
import {
  ReactFlow,
  MiniMap,
  Background,
  BackgroundVariant,
  useNodesState,
  useEdgesState,
  addEdge,
  Connection,
  Edge,
  Node,
  OnConnect,
  Panel,
  useReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import {
  HardwareStationNode,
  TopologyCableEdge,
  PROTOCOL_COLORS,
  HardwareViewMode,
  NetworkPort,
} from './types';
import { IndustrialStationNode, IndustrialStationNodeData } from './IndustrialStationNode';
import { IndustrialBusEdge } from './IndustrialBusEdge';
import { INDUSTRIAL_CATALOG } from './catalogData';
import { EmptySurface, EMPTY_SURFACE } from '@mythos/ui-empty-state';
import {
  Plus,
  Maximize2,
  ZoomIn,
  ZoomOut,
  RotateCcw,
  Layers,
  Network,
  Cpu,
} from 'lucide-react';

export interface NetworkCanvasProps {
  initialStations?: HardwareStationNode[];
  initialCables?: TopologyCableEdge[];
  onSelectStation?: (station: HardwareStationNode | null) => void;
  onSelectSlot?: (stationId: string, slotNumber: number) => void;
  onSelectPort?: (stationId: string, port: NetworkPort) => void;
  className?: string;
  viewMode?: HardwareViewMode;
  onViewModeChange?: (mode: HardwareViewMode) => void;
}

const nodeTypes = {
  industrialStation: IndustrialStationNode,
};

const edgeTypes = {
  industrialBus: IndustrialBusEdge,
};

// Default Sample Stations


const NetworkCanvasInner: React.FC<NetworkCanvasProps> = ({
  initialStations,
  initialCables,
  onSelectStation,
  onSelectSlot,
  onSelectPort,
  className = '',
  viewMode = 'NETWORK',
  onViewModeChange,
}) => {
  const [stations, setStations] = useState<HardwareStationNode[]>(initialStations ?? []);
  const [activeMode, setActiveMode] = useState<HardwareViewMode>(viewMode);
  const [showCatalogDrawer, setShowCatalogDrawer] = useState(false);
  const [catalogFilter, setCatalogFilter] = useState('');

  const { fitView, zoomIn, zoomOut, setViewport } = useReactFlow();

  // Convert station nodes to xyflow nodes
  const initialNodes: Node[] = useMemo(() => {
    return stations.map((st) => ({
      id: st.id,
      type: 'industrialStation',
      position: st.position,
      data: {
        station: st,
        onSelectSlot: (slotNum: number) => onSelectSlot?.(st.id, slotNum),
        onPortClick: (port: NetworkPort) => onSelectPort?.(st.id, port),
      },
    }));
  }, [stations, onSelectSlot, onSelectPort]);

  // Convert cables to xyflow edges
  const initialEdges: Edge[] = useMemo(() => {
    return (initialCables ?? []).map((c) => ({
      id: c.id,
      source: c.sourceStationId,
      sourceHandle: c.sourcePortId,
      target: c.targetStationId,
      targetHandle: c.targetPortId,
      type: 'industrialBus',
      data: {
        protocol: c.protocol,
        label: PROTOCOL_COLORS[c.protocol]?.label || c.protocol,
        lengthMeters: c.cableLengthMeters,
        isOffline: c.health.status === 'OFFLINE',
      },
    }));
  }, [initialCables]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  // Connection Handler
  const onConnect: OnConnect = useCallback(
    (connection: Connection) => {
      const newEdge: Edge = {
        ...connection,
        id: `cable-${Date.now()}`,
        type: 'industrialBus',
        data: {
          protocol: 'PROFINET',
          label: 'PROFINET',
          lengthMeters: 5.0,
        },
      };
      setEdges((eds) => addEdge(newEdge, eds));
    },
    [setEdges]
  );

  // Selection Handler
  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      const found = stations.find((s) => s.id === node.id) || null;
      onSelectStation?.(found);
    },
    [stations, onSelectStation]
  );

  const onPaneClick = useCallback(() => {
    onSelectStation?.(null);
  }, [onSelectStation]);

  // Add Device from Catalog
  const handleAddFromCatalog = (item: typeof INDUSTRIAL_CATALOG[0]) => {
    const newId = `station-${item.stationType.toLowerCase()}-${Date.now().toString().slice(-4)}`;
    const newStation: HardwareStationNode = {
      id: newId,
      stationName: `${item.name.replace(/\s+/g, '_').toUpperCase()}`,
      stationType: item.stationType,
      rackNumber: 0,
      status: 'RUN',
      articleNumber: item.articleNumber,
      position: { x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 },
      slots: [],
      ports: item.defaultPorts,
    };

    setStations((prev) => [...prev, newStation]);
    setNodes((nds) => [
      ...nds,
      {
        id: newId,
        type: 'industrialStation',
        position: newStation.position,
        data: {
          station: newStation,
          onSelectSlot: (slotNum: number) => onSelectSlot?.(newId, slotNum),
          onPortClick: (port: NetworkPort) => onSelectPort?.(newId, port),
        },
      },
    ]);
    setShowCatalogDrawer(false);
  };

  const handleModeChange = (mode: HardwareViewMode) => {
    setActiveMode(mode);
    onViewModeChange?.(mode);
  };

  return (
    <div className={`relative w-full h-full flex flex-col bg-[var(--bg-base)] overflow-hidden select-none ${className}`}>
      {/* Top Segmented Hardware View Switcher & Toolbar (TIA Portal Parity) */}
      <div className="flex items-center justify-between px-3 py-2 bg-[var(--bg-surface)] border-b border-[var(--bg-highlight)] z-10">
        <div className="flex items-center gap-1 bg-[var(--bg-base)] p-1 rounded-lg border border-[var(--bg-highlight)]">
          <button
            type="button"
            onClick={() => handleModeChange('NETWORK')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'NETWORK'
                ? 'bg-[var(--bg-surface)] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]'
            }`}
          >
            <Network className="w-3.5 h-3.5" />
            <span>Network View</span>
            <span className="text-[10px] text-[var(--text-muted)] font-mono">Alt+1</span>
          </button>

          <button
            type="button"
            onClick={() => handleModeChange('DEVICE')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'DEVICE'
                ? 'bg-[var(--bg-surface)] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]'
            }`}
          >
            <Cpu className="w-3.5 h-3.5" />
            <span>Device View</span>
            <span className="text-[10px] text-[var(--text-muted)] font-mono">Alt+2</span>
          </button>

          <button
            type="button"
            onClick={() => handleModeChange('TOPOLOGY')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'TOPOLOGY'
                ? 'bg-[var(--bg-surface)] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Topology View</span>
            <span className="text-[10px] text-[var(--text-muted)] font-mono">Alt+3</span>
          </button>
        </div>

        {/* Action Controls */}
        <div className="flex items-center gap-2">
          {/* Legend indicator */}
          <div className="hidden lg:flex items-center gap-3 px-2.5 py-1 rounded bg-[var(--bg-base)] border border-[var(--bg-highlight)] text-[10px] font-mono text-[var(--text-secondary)]">
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[var(--bus-ethernet)]" /> PROFINET
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[var(--bus-serial)]" /> Modbus
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[var(--alarm-critical)]" /> Safety STO
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[var(--bus-io-link)]" /> IO-Link
            </span>
          </div>

          <button
            type="button"
            onClick={() => setShowCatalogDrawer(!showCatalogDrawer)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded bg-sky-600 hover:bg-sky-500 text-white text-xs font-medium shadow transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Add Device</span>
          </button>
        </div>
      </div>

      {/* Main xyflow Canvas Viewport */}
      <div className="flex-1 w-full h-full relative">
        {stations.length === 0 && (
          <div className="absolute inset-0 z-10 flex items-center justify-center p-8 pointer-events-none">
            <EmptySurface copy={EMPTY_SURFACE.devices} />
          </div>
        )}
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          snapToGrid={true}
          snapGrid={[16, 16]}
          defaultViewport={{ x: 0, y: 0, zoom: 1 }}
          minZoom={0.2}
          maxZoom={2.5}
          fitView
          attributionPosition="bottom-left"
          className="bg-[var(--bg-base)]"
        >
          {/* 16px Snap Grid Pattern */}
          <Background
            variant={BackgroundVariant.Dots}
            gap={16}
            size={1.5}
            color="var(--bg-highlight)"
            className="opacity-60"
          />

          <MiniMap
            nodeColor={(n) => {
              const data = n.data as unknown as IndustrialStationNodeData;
              switch (data?.station?.status) {
                case 'RUN':
                  return 'var(--bus-ethernet)';
                case 'FAULT':
                  return 'var(--alarm-critical)';
                case 'STOP':
                  return 'var(--alarm-minor)';
                default:
                  return 'var(--bus-offline)';
              }
            }}
            maskColor="rgba(10, 13, 18, 0.75)"
            style={{
              backgroundColor: 'var(--bg-surface)',
              border: '1px solid var(--bg-highlight)',
              borderRadius: '6px',
            }}
          />

          {/* Standard Zoom / Fit controls panel */}
          <Panel position="bottom-right" className="flex items-center gap-1 bg-[var(--bg-surface)] border border-[var(--bg-highlight)] p-1 rounded-md shadow-lg">
            <button
              type="button"
              onClick={() => zoomIn()}
              className="p-1.5 rounded hover:bg-[var(--bg-surface)] text-[var(--text-primary)] transition-colors"
              title="Zoom In (+)"
            >
              <ZoomIn className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => zoomOut()}
              className="p-1.5 rounded hover:bg-[var(--bg-surface)] text-[var(--text-primary)] transition-colors"
              title="Zoom Out (-)"
            >
              <ZoomOut className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => fitView({ padding: 0.2 })}
              className="p-1.5 rounded hover:bg-[var(--bg-surface)] text-[var(--text-primary)] transition-colors"
              title="Fit to View"
            >
              <Maximize2 className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => setViewport({ x: 0, y: 0, zoom: 1 })}
              className="p-1.5 rounded hover:bg-[var(--bg-surface)] text-[var(--text-primary)] transition-colors"
              title="Reset 100%"
            >
              <RotateCcw className="w-4 h-4" />
            </button>
          </Panel>
        </ReactFlow>

        {/* Hardware Catalog Slide-in Overlay Palette */}
        {showCatalogDrawer && (
          <div className="absolute top-2 right-2 w-80 max-h-[85%] bg-[var(--bg-surface)]/95 backdrop-blur-md border border-[var(--bg-highlight)] rounded-lg shadow-2xl z-20 flex flex-col overflow-hidden animate-in fade-in slide-in-from-right duration-150">
            <div className="flex items-center justify-between p-3 border-b border-[var(--bg-highlight)] bg-[var(--bg-elevated)]">
              <div className="flex items-center gap-2">
                <Plus className="w-4 h-4 text-sky-400" />
                <h3 className="text-xs font-semibold text-white">Hardware Catalog</h3>
              </div>
              <button
                type="button"
                onClick={() => setShowCatalogDrawer(false)}
                className="text-xs text-[var(--text-secondary)] hover:text-white"
              >
                ✕
              </button>
            </div>

            <div className="p-2 border-b border-[var(--bg-highlight)]">
              <input
                type="text"
                placeholder="Search catalog or article no..."
                value={catalogFilter}
                onChange={(e) => setCatalogFilter(e.target.value)}
                className="w-full bg-[var(--bg-base)] border border-[var(--bg-highlight)] rounded px-2.5 py-1 text-xs text-white placeholder-[var(--text-muted)] focus:outline-none focus:border-sky-500 font-mono"
              />
            </div>

            <div className="flex-1 overflow-y-auto p-2 space-y-2">
              {INDUSTRIAL_CATALOG.filter(
                (item) =>
                  item.name.toLowerCase().includes(catalogFilter.toLowerCase()) ||
                  item.articleNumber.toLowerCase().includes(catalogFilter.toLowerCase())
              ).map((item) => (
                <div
                  key={item.id}
                  onClick={() => handleAddFromCatalog(item)}
                  className="p-2 rounded bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] hover:border-sky-500/60 cursor-pointer transition-all hover:bg-[var(--bg-surface)] group"
                >
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-[var(--text-primary)] group-hover:text-sky-400">
                      {item.name}
                    </span>
                    <span className="text-[10px] font-mono text-sky-400 bg-sky-950/60 px-1 rounded">
                      {item.category}
                    </span>
                  </div>
                  <div className="text-[10px] text-[var(--text-secondary)] font-mono mt-0.5">
                    {item.articleNumber}
                  </div>
                  <div className="text-[11px] text-[var(--text-secondary)] mt-1 line-clamp-2">
                    {item.description}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export const NetworkCanvas: React.FC<NetworkCanvasProps> = (props) => {
  return (
    <ReactFlowProvider>
      <NetworkCanvasInner {...props} />
    </ReactFlowProvider>
  );
};
