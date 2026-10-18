import React, { useState } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Network,
  Cpu,
  FileCode2,
  Tag,
  Boxes,
  Monitor,
  Folder,
  Layers,
  Plus,
} from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';

export interface Zone2ProjectTreeProps {
  className?: string;
  onOpenDocument?: (docType: string, id: string, title: string) => void;
}

export const Zone2ProjectTree: React.FC<Zone2ProjectTreeProps> = ({
  className = '',
  onOpenDocument,
}) => {
  const screens = useHmiStore((s) => s.screens);
  const openScreenTab = useHmiStore((s) => s.openScreenTab);
  const activeScreenId = useHmiStore((s) => s.activeScreenId);
  const addNewScreen = useHmiStore((s) => s.addNewScreen);
  const addNewSubScreen = useHmiStore((s) => s.addNewSubScreen);

  const [expandedNodes, setExpandedNodes] = useState<Record<string, boolean>>({
    root: true,
    devicesNetworks: true,
    controller: true,
    programBlocks: true,
    plcTags: false,
    visualization: true,
    hmiScreens: true,
    faceplates: false,
  });

  const toggleNode = (nodeKey: string) => {
    setExpandedNodes((prev) => ({ ...prev, [nodeKey]: !prev[nodeKey] }));
  };

  const handleAddScreenPrompt = async () => {
    const screenName = window.prompt('Enter new HMI Screen Name:', `Screen_0${screens.length + 1}_Area`);
    if (screenName && screenName.trim().length > 0) {
      await addNewScreen(screenName.trim());
    }
  };

  const handleAddSubScreenPrompt = async (parentScreenId: string) => {
    const subName = window.prompt('Enter Sub-Screen Name:');
    if (subName && subName.trim().length > 0) {
      await addNewSubScreen(parentScreenId, subName.trim());
    }
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[#0F141C] border-r border-neutral-800 text-neutral-300 text-xs select-none overflow-y-auto ${className}`}
      data-testid="zone2-project-tree"
    >
      {/* Zone 2 Header */}
      <div className="flex items-center justify-between px-3 py-2 bg-[#0A0D12] border-b border-neutral-800 font-semibold text-[11px] text-neutral-400 tracking-wider uppercase">
        <span>Project Tree</span>
        <span className="text-[10px] text-neutral-600 font-mono">SYS-REQ-097</span>
      </div>

      <div className="p-1 space-y-0.5">
        {/* Root Project Node */}
        <div>
          <div
            onClick={() => toggleNode('root')}
            className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer font-medium text-neutral-200"
          >
            {expandedNodes.root ? <ChevronDown className="w-3.5 h-3.5 text-neutral-500" /> : <ChevronRight className="w-3.5 h-3.5 text-neutral-500" />}
            <Folder className="w-3.5 h-3.5 text-blue-400" />
            <span className="truncate">BottlingPlant_Line01</span>
          </div>

          {expandedNodes.root && (
            <div className="pl-4 space-y-0.5">
              {/* 1. Devices & Networks */}
              <div>
                <div
                  onClick={() => toggleNode('devicesNetworks')}
                  className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-300"
                >
                  {expandedNodes.devicesNetworks ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                  <Network className="w-3.5 h-3.5 text-cyan-400" />
                  <span>Devices & Networks</span>
                </div>

                {expandedNodes.devicesNetworks && (
                  <div className="pl-4 space-y-0.5 text-neutral-400">
                    <div
                      onClick={() => onOpenDocument?.('topology', 'network-view', '🌐 Network View')}
                      className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                    >
                      <span className="text-[10px]">🌐</span> Network View
                    </div>
                    <div
                      onClick={() => onOpenDocument?.('device', 'device-view', '🎛️ Device View')}
                      className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                    >
                      <span className="text-[10px]">🎛️</span> Device View (Rack 0)
                    </div>
                    <div
                      onClick={() => onOpenDocument?.('topology', 'topology-view', '🔌 Topology View')}
                      className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                    >
                      <span className="text-[10px]">🔌</span> Topology View
                    </div>
                  </div>
                )}
              </div>

              {/* 2. Controller STruC++ */}
              <div>
                <div
                  onClick={() => toggleNode('controller')}
                  className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-300"
                >
                  {expandedNodes.controller ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                  <Cpu className="w-3.5 h-3.5 text-emerald-400" />
                  <span>Controller [STruC++]</span>
                </div>

                {expandedNodes.controller && (
                  <div className="pl-4 space-y-0.5">
                    {/* Program Blocks */}
                    <div>
                      <div
                        onClick={() => toggleNode('programBlocks')}
                        className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-400"
                      >
                        {expandedNodes.programBlocks ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                        <FileCode2 className="w-3.5 h-3.5 text-amber-400" />
                        <span>Program Blocks</span>
                      </div>

                      {expandedNodes.programBlocks && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          <div
                            onClick={() => onOpenDocument?.('logic', 'MainTask', 'Main [OB1]')}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                          >
                            <span className="font-mono text-cyan-400 text-[10px]">[OB1]</span> MainTask
                          </div>
                          <div
                            onClick={() => onOpenDocument?.('logic', 'FastInterrupt', 'FastInterrupt [OB35]')}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                          >
                            <span className="font-mono text-cyan-400 text-[10px]">[OB35]</span> FastInterrupt (10ms)
                          </div>
                          <div
                            onClick={() => onOpenDocument?.('logic', 'FB_Motor', 'FB_Motor [FB10]')}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                          >
                            <span className="font-mono text-amber-400 text-[10px]">[FB10]</span> FB_Motor
                          </div>
                        </div>
                      )}
                    </div>

                    {/* PLC Tags */}
                    <div>
                      <div
                        onClick={() => toggleNode('plcTags')}
                        className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-400"
                      >
                        {expandedNodes.plcTags ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                        <Tag className="w-3.5 h-3.5 text-purple-400" />
                        <span>PLC Tags</span>
                      </div>

                      {expandedNodes.plcTags && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          <div className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer">
                            Show all tags
                          </div>
                          <div className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer">
                            Default Tag Table
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>

              {/* 3. Visualization HMI Screens */}
              <div>
                <div
                  onClick={() => toggleNode('visualization')}
                  className="flex items-center justify-between px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-300"
                >
                  <div className="flex items-center gap-1.5">
                    {expandedNodes.visualization ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                    <Monitor className="w-3.5 h-3.5 text-blue-400" />
                    <span>Visualization [Web HMI]</span>
                  </div>
                </div>

                {expandedNodes.visualization && (
                  <div className="pl-4 space-y-0.5">
                    {/* HMI Screens Header */}
                    <div className="flex items-center justify-between px-2 py-1 text-neutral-400">
                      <div
                        onClick={() => toggleNode('hmiScreens')}
                        className="flex items-center gap-1.5 cursor-pointer hover:text-white"
                      >
                        {expandedNodes.hmiScreens ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                        <Layers className="w-3.5 h-3.5 text-sky-400" />
                        <span>HMI Screens</span>
                      </div>
                      <button
                        onClick={handleAddScreenPrompt}
                        className="p-0.5 hover:bg-neutral-700 rounded text-neutral-400 hover:text-white transition-colors"
                        title="Add New Screen"
                      >
                        <Plus className="w-3.5 h-3.5 text-sky-400" />
                      </button>
                    </div>

                    {/* Screens Tree */}
                    {expandedNodes.hmiScreens && (
                      <div className="pl-3 space-y-0.5">
                        {screens.map((screen) => {
                          const isActive = activeScreenId === screen.id;
                          return (
                            <div key={screen.id}>
                              <div
                                onClick={() => openScreenTab(screen.id)}
                                className={`flex items-center justify-between px-2 py-1 rounded cursor-pointer group transition-colors ${
                                  isActive
                                    ? 'bg-blue-600/30 text-blue-200 border border-blue-500/40'
                                    : 'hover:bg-neutral-800/60 text-neutral-300'
                                }`}
                              >
                                <span className="truncate">{screen.name}</span>
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleAddSubScreenPrompt(screen.id);
                                  }}
                                  className="opacity-0 group-hover:opacity-100 p-0.5 hover:bg-neutral-700 rounded text-neutral-400 hover:text-white transition-opacity"
                                  title="Add Sub-Screen"
                                >
                                  <Plus className="w-3 h-3" />
                                </button>
                              </div>

                              {/* Nested Sub-screens */}
                              {screen.subscreens && screen.subscreens.length > 0 && (
                                <div className="pl-4 space-y-0.5 border-l border-neutral-800 ml-2 mt-0.5">
                                  {screen.subscreens.map((sub) => (
                                    <div
                                      key={sub.id}
                                      onClick={() => openScreenTab(screen.id, sub.id)}
                                      className="px-2 py-0.5 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer text-neutral-400 text-[11px] truncate flex items-center gap-1"
                                    >
                                      <span className="text-neutral-600">↳</span>
                                      <span>{sub.name}</span>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    )}

                    {/* Faceplates Library */}
                    <div>
                      <div
                        onClick={() => toggleNode('faceplates')}
                        className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-400"
                      >
                        {expandedNodes.faceplates ? <ChevronDown className="w-3 h-3 text-neutral-500" /> : <ChevronRight className="w-3 h-3 text-neutral-500" />}
                        <Boxes className="w-3.5 h-3.5 text-teal-400" />
                        <span>Faceplate Types</span>
                      </div>

                      {expandedNodes.faceplates && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          <div
                            onClick={() => onOpenDocument?.('faceplate', 'fp_motor_vfd', '📦 FP_Motor_VFD')}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer"
                          >
                            FP_Motor_VFD (v1.0.0)
                          </div>
                          <div
                            onClick={() => onOpenDocument?.('faceplate', 'fp_analog_valve', '📦 FP_AnalogValve')}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer"
                          >
                            FP_AnalogValve (v1.0.0)
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
