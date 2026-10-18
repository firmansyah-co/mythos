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
const DEFAULT_STATIONS: HardwareStationNode[] = [
  {
    id: 'station-plc-01',
    stationName: 'PLC_RACK_01',
    stationType: 'PLC_RACK',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.10',
    articleNumber: '6ES7515-2AM02-0AB0',
    position: { x: 80, y: 120 },
    slots: [
      {
        slotNumber: 0,
        name: 'PM 190W 24VDC',
        catalogArticleNo: '6EP1333-4BA00',
        moduleCategory: 'POWER',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [] },
      },
      {
        slotNumber: 1,
        name: 'CPU 1515-2 PN',
        catalogArticleNo: '6ES7515-2AM02-0AB0',
        moduleCategory: 'CPU',
        startAddressI: '%I0.0',
        endAddressI: '%I0.7',
        startAddressQ: '%Q0.0',
        endAddressQ: '%Q0.7',
        ports: [
          { id: 'plc-x1-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
          { id: 'plc-x1-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
        ],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, true, false, true] },
      },
      {
        slotNumber: 2,
        name: 'DI 16x24VDC HF',
        catalogArticleNo: '6ES7521-1BH00-0AB0',
        moduleCategory: 'DI',
        startAddressI: '%I10.0',
        endAddressI: '%I11.7',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, false, true, false, true] },
      },
      {
        slotNumber: 3,
        name: 'DQ 16x24VDC HF',
        catalogArticleNo: '6ES7522-1BH01-0AB0',
        moduleCategory: 'DQ',
        startAddressQ: '%Q10.0',
        endAddressQ: '%Q11.7',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [false, true, true, false] },
      },
    ],
    ports: [
      { id: 'plc-x1-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'plc-x1-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-sw-01',
    stationName: 'SCALANCE_XC208',
    stationType: 'SWITCH',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.2',
    articleNumber: '6GK5208-0BA00-2AC2',
    position: { x: 440, y: 120 },
    slots: [],
    ports: [
      { id: 'sw-p1', name: 'Port 1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p2', name: 'Port 2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p3', name: 'Port 3', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'sw-p4', name: 'Port 4', protocol: 'SAFETY_PROFISAFE', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-et200-01',
    stationName: 'ET200SP_ISLAND',
    stationType: 'REMOTE_IO',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.11',
    articleNumber: '6ES7155-6AU01-0BN0',
    position: { x: 800, y: 60 },
    slots: [
      {
        slotNumber: 1,
        name: 'IM 155-6 PN HF',
        catalogArticleNo: '6ES7155-6AU01-0BN0',
        moduleCategory: 'COMM',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [] },
      },
      {
        slotNumber: 2,
        name: 'AI 4xU/I 2-wire HF',
        catalogArticleNo: '6ES7134-6HD00-0BA1',
        moduleCategory: 'AI',
        startAddressI: '%IW20',
        endAddressI: '%IW26',
        ports: [],
        diagnostics: { runLed: true, errLed: false, maintLed: false, channelBits: [true, true] },
      },
    ],
    ports: [
      { id: 'et-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'et-p2', name: 'X1 P2', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
  {
    id: 'station-vfd-01',
    stationName: 'SINAMICS_G120_PUMP',
    stationType: 'VFD_DRIVE',
    rackNumber: 0,
    status: 'RUN',
    ipAddress: '192.168.1.12',
    articleNumber: '6SL3210-1PE21-8UL0',
    position: { x: 800, y: 280 },
    slots: [],
    ports: [
      { id: 'vfd-p1', name: 'X1 P1', protocol: 'PROFINET', connectorType: 'RJ45', linkStatus: 'UP' },
      { id: 'vfd-p2', name: 'STO', protocol: 'SAFETY_PROFISAFE', connectorType: 'RJ45', linkStatus: 'UP' },
    ],
  },
];

const DEFAULT_CABLES: TopologyCableEdge[] = [
  {
    id: 'cable-plc-sw',
    sourceStationId: 'station-plc-01',
    sourcePortId: 'plc-x1-p1',
    targetStationId: 'station-sw-01',
    targetPortId: 'sw-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 12.4,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.8, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-et',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p2',
    targetStationId: 'station-et200-01',
    targetPortId: 'et-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 4.2,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.5, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-vfd',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p3',
    targetStationId: 'station-vfd-01',
    targetPortId: 'vfd-p1',
    protocol: 'PROFINET',
    cableLengthMeters: 8.5,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 1.1, packetLossPct: 0.0, retriesPerSec: 0 },
  },
  {
    id: 'cable-sw-safety',
    sourceStationId: 'station-sw-01',
    sourcePortId: 'sw-p4',
    targetStationId: 'station-vfd-01',
    targetPortId: 'vfd-p2',
    protocol: 'SAFETY_PROFISAFE',
    cableLengthMeters: 6.0,
    nominalMatch: true,
    health: { status: 'OPTIMAL', latencyMs: 0.2, packetLossPct: 0.0, retriesPerSec: 0 },
  },
];

const NetworkCanvasInner: React.FC<NetworkCanvasProps> = ({
  initialStations = DEFAULT_STATIONS,
  initialCables = DEFAULT_CABLES,
  onSelectStation,
  onSelectSlot,
  onSelectPort,
  className = '',
  viewMode = 'NETWORK',
  onViewModeChange,
}) => {
  const [stations, setStations] = useState<HardwareStationNode[]>(initialStations);
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
    return initialCables.map((c) => ({
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
    <div className={`relative w-full h-full flex flex-col bg-[#08090A] overflow-hidden select-none ${className}`}>
      {/* Top Segmented Hardware View Switcher & Toolbar (TIA Portal Parity) */}
      <div className="flex items-center justify-between px-3 py-2 bg-[#0F141C] border-b border-[#232B3B] z-10">
        <div className="flex items-center gap-1 bg-[#0A0D12] p-1 rounded-lg border border-[#232B3B]">
          <button
            type="button"
            onClick={() => handleModeChange('NETWORK')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'NETWORK'
                ? 'bg-[#1E2638] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#161E2E]'
            }`}
          >
            <Network className="w-3.5 h-3.5" />
            <span>Network View</span>
            <span className="text-[10px] text-[#62666D] font-mono">Alt+1</span>
          </button>

          <button
            type="button"
            onClick={() => handleModeChange('DEVICE')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'DEVICE'
                ? 'bg-[#1E2638] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#161E2E]'
            }`}
          >
            <Cpu className="w-3.5 h-3.5" />
            <span>Device View</span>
            <span className="text-[10px] text-[#62666D] font-mono">Alt+2</span>
          </button>

          <button
            type="button"
            onClick={() => handleModeChange('TOPOLOGY')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium transition-all ${
              activeMode === 'TOPOLOGY'
                ? 'bg-[#1E2638] text-sky-400 shadow-sm border border-sky-500/30 font-semibold'
                : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#161E2E]'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Topology View</span>
            <span className="text-[10px] text-[#62666D] font-mono">Alt+3</span>
          </button>
        </div>

        {/* Action Controls */}
        <div className="flex items-center gap-2">
          {/* Legend indicator */}
          <div className="hidden lg:flex items-center gap-3 px-2.5 py-1 rounded bg-[#0A0D12] border border-[#232B3B] text-[10px] font-mono text-[#8A8F98]">
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[#16A34A]" /> PROFINET
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[#0284C7]" /> Modbus
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[#DC2626]" /> Safety STO
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-[#9333EA]" /> IO-Link
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
          className="bg-[#0A0D12]"
        >
          {/* 16px Snap Grid Pattern */}
          <Background
            variant={BackgroundVariant.Dots}
            gap={16}
            size={1.5}
            color="#232B3B"
            className="opacity-60"
          />

          <MiniMap
            nodeColor={(n) => {
              const data = n.data as unknown as IndustrialStationNodeData;
              switch (data?.station?.status) {
                case 'RUN':
                  return '#16A34A';
                case 'FAULT':
                  return '#DC2626';
                case 'STOP':
                  return '#CA8A04';
                default:
                  return '#475569';
              }
            }}
            maskColor="rgba(10, 13, 18, 0.75)"
            style={{
              backgroundColor: '#0F141C',
              border: '1px solid #232B3B',
              borderRadius: '6px',
            }}
          />

          {/* Standard Zoom / Fit controls panel */}
          <Panel position="bottom-right" className="flex items-center gap-1 bg-[#0F141C] border border-[#232B3B] p-1 rounded-md shadow-lg">
            <button
              type="button"
              onClick={() => zoomIn()}
              className="p-1.5 rounded hover:bg-[#1E2638] text-[#D0D6E0] transition-colors"
              title="Zoom In (+)"
            >
              <ZoomIn className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => zoomOut()}
              className="p-1.5 rounded hover:bg-[#1E2638] text-[#D0D6E0] transition-colors"
              title="Zoom Out (-)"
            >
              <ZoomOut className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => fitView({ padding: 0.2 })}
              className="p-1.5 rounded hover:bg-[#1E2638] text-[#D0D6E0] transition-colors"
              title="Fit to View"
            >
              <Maximize2 className="w-4 h-4" />
            </button>
            <button
              type="button"
              onClick={() => setViewport({ x: 0, y: 0, zoom: 1 })}
              className="p-1.5 rounded hover:bg-[#1E2638] text-[#D0D6E0] transition-colors"
              title="Reset 100%"
            >
              <RotateCcw className="w-4 h-4" />
            </button>
          </Panel>
        </ReactFlow>

        {/* Hardware Catalog Slide-in Overlay Palette */}
        {showCatalogDrawer && (
          <div className="absolute top-2 right-2 w-80 max-h-[85%] bg-[#0F141C]/95 backdrop-blur-md border border-[#232B3B] rounded-lg shadow-2xl z-20 flex flex-col overflow-hidden animate-in fade-in slide-in-from-right duration-150">
            <div className="flex items-center justify-between p-3 border-b border-[#232B3B] bg-[#161E2E]">
              <div className="flex items-center gap-2">
                <Plus className="w-4 h-4 text-sky-400" />
                <h3 className="text-xs font-semibold text-white">Hardware Catalog</h3>
              </div>
              <button
                type="button"
                onClick={() => setShowCatalogDrawer(false)}
                className="text-xs text-[#8A8F98] hover:text-white"
              >
                ✕
              </button>
            </div>

            <div className="p-2 border-b border-[#232B3B]">
              <input
                type="text"
                placeholder="Search catalog or article no..."
                value={catalogFilter}
                onChange={(e) => setCatalogFilter(e.target.value)}
                className="w-full bg-[#0A0D12] border border-[#232B3B] rounded px-2.5 py-1 text-xs text-white placeholder-[#62666D] focus:outline-none focus:border-sky-500 font-mono"
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
                  className="p-2 rounded bg-[#161E2E] border border-[#232B3B] hover:border-sky-500/60 cursor-pointer transition-all hover:bg-[#1A2336] group"
                >
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-[#F7F8F8] group-hover:text-sky-400">
                      {item.name}
                    </span>
                    <span className="text-[10px] font-mono text-sky-400 bg-sky-950/60 px-1 rounded">
                      {item.category}
                    </span>
                  </div>
                  <div className="text-[10px] text-[#8A8F98] font-mono mt-0.5">
                    {item.articleNumber}
                  </div>
                  <div className="text-[11px] text-[#A0A6B2] mt-1 line-clamp-2">
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
