import React, { useEffect, useState } from 'react';
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
import { useFaceplateStore } from '../../stores/useFaceplateStore';
import { versionLabel } from '../../types/faceplate';
import { LogicBlockDescriptor } from '../../types/hmi';

export interface Zone2ProjectTreeProps {
  className?: string;
  onOpenDocument?: (docType: string, id: string, title: string, params?: Record<string, any>) => void;
  onOpenCreateBlockModal?: () => void;
  onOpenCreateScreenModal?: () => void;
}

export const Zone2ProjectTree: React.FC<Zone2ProjectTreeProps> = ({
  className = '',
  onOpenDocument,
  onOpenCreateBlockModal,
  onOpenCreateScreenModal,
}) => {
  const currentProject = useHmiStore((s) => s.currentProject);
  const projectBlocks = useHmiStore((s) => s.projectBlocks);
  const screens = useHmiStore((s) => s.screens);
  const openScreenTab = useHmiStore((s) => s.openScreenTab);
  const activeScreenId = useHmiStore((s) => s.activeScreenId);
  const addNewSubScreen = useHmiStore((s) => s.addNewSubScreen);
  const setSelectedContextItem = useHmiStore((s) => s.setSelectedContextItem);

  // Master faceplate types come from the project-scoped faceplate routes.
  const faceplateTypes = useFaceplateStore((s) => s.types);
  const faceplateStatus = useFaceplateStore((s) => s.status);
  const faceplateError = useFaceplateStore((s) => s.errorMessage);
  const loadFaceplates = useFaceplateStore((s) => s.load);

  const currentProjectId = currentProject?.id ?? null;
  useEffect(() => {
    if (currentProjectId) {
      void loadFaceplates(currentProjectId);
    } else {
      // No project open: drop the previous project's library rather than showing it.
      useFaceplateStore.getState().reset();
    }
  }, [currentProjectId, loadFaceplates]);

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

  const handleAddSubScreenPrompt = async (parentScreenId: string) => {
    const subName = window.prompt('Enter Sub-Screen Name:');
    if (subName && subName.trim().length > 0) {
      await addNewSubScreen(parentScreenId, subName.trim());
    }
  };

  const rootProjectName = currentProject?.name || '(No Project Loaded)';

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-surface)] border-r border-neutral-800 text-neutral-300 text-xs select-none overflow-y-auto ${className}`}
      data-testid="zone2-project-tree"
    >
      {/* Zone 2 Header */}
      <div className="flex items-center justify-between px-3 py-2 bg-[var(--bg-base)] border-b border-neutral-800 font-semibold text-[11px] text-neutral-400 tracking-wider uppercase">
        <span>Project Tree</span>
        <span className="text-[10px] text-neutral-600 font-mono">SYS-REQ-097</span>
      </div>

      <div className="p-1 space-y-0.5">
        {/* Root Project Node */}
        <div>
          <div
            onClick={() => {
              toggleNode('root');
              if (currentProject) {
                setSelectedContextItem({
                  id: currentProject.id,
                  name: currentProject.name,
                  type: 'PROJECT',
                  metadata: { ...currentProject },
                });
              }
            }}
            className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer font-medium text-neutral-200"
          >
            {expandedNodes.root ? (
              <ChevronDown className="w-3.5 h-3.5 text-neutral-500" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-neutral-500" />
            )}
            <Folder className="w-3.5 h-3.5 text-blue-400" />
            <span className="truncate" data-testid="project-tree-root-name">
              {rootProjectName}
            </span>
          </div>

          {expandedNodes.root && (
            <div className="pl-4 space-y-0.5">
              {/* 1. Devices & Networks */}
              <div>
                <div
                  onClick={() => toggleNode('devicesNetworks')}
                  className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-300"
                >
                  {expandedNodes.devicesNetworks ? (
                    <ChevronDown className="w-3 h-3 text-neutral-500" />
                  ) : (
                    <ChevronRight className="w-3 h-3 text-neutral-500" />
                  )}
                  <Network className="w-3.5 h-3.5 text-cyan-400" />
                  <span>Devices &amp; Networks</span>
                </div>

                {expandedNodes.devicesNetworks && (
                  <div className="pl-4 space-y-0.5 text-neutral-400">
                    <div
                      onClick={() => {
                        onOpenDocument?.('topology', 'network-view', '🌐 Network View');
                        // Opening a view is not a device selection: these entries used to assert a
                        // named network, rack and IP (`PROFINET Bus Network`, `ET 200SP Rack 0`,
                        // `192.168.1.20`) that nothing had discovered. Clearing the selection is
                        // honest - the view opens with nothing selected rather than with a
                        // plausible-looking device carried over from a previous click.
                        setSelectedContextItem(null);
                      }}
                      className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                    >
                      <span className="text-[10px]">🌐</span> Network View
                    </div>
                    <div
                      onClick={() => {
                        onOpenDocument?.('device', 'device-view', '🎛️ Device View');
                        setSelectedContextItem(null);
                      }}
                      className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                    >
                      <span className="text-[10px]">🎛️</span> Device View (Rack 0)
                    </div>
                    <div
                      onClick={() => {
                        onOpenDocument?.('topology', 'topology-view', '🔌 Topology View');
                        setSelectedContextItem(null);
                      }}
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
                  {expandedNodes.controller ? (
                    <ChevronDown className="w-3 h-3 text-neutral-500" />
                  ) : (
                    <ChevronRight className="w-3 h-3 text-neutral-500" />
                  )}
                  <Cpu className="w-3.5 h-3.5 text-emerald-400" />
                  <span>Controller [STruC++]</span>
                </div>

                {expandedNodes.controller && (
                  <div className="pl-4 space-y-0.5">
                    {/* Program Blocks */}
                    <div>
                      <div className="flex items-center justify-between px-2 py-1 text-neutral-400">
                        <div
                          onClick={() => toggleNode('programBlocks')}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-white"
                        >
                          {expandedNodes.programBlocks ? (
                            <ChevronDown className="w-3 h-3 text-neutral-500" />
                          ) : (
                            <ChevronRight className="w-3 h-3 text-neutral-500" />
                          )}
                          <FileCode2 className="w-3.5 h-3.5 text-amber-400" />
                          <span>Program Blocks</span>
                        </div>
                        <button
                          onClick={() => onOpenCreateBlockModal?.()}
                          data-testid="add-block-btn"
                          className="p-0.5 hover:bg-neutral-700 rounded text-neutral-400 hover:text-white transition-colors"
                          title="Add New Block"
                        >
                          <Plus className="w-3.5 h-3.5 text-amber-400" />
                        </button>
                      </div>

                      {expandedNodes.programBlocks && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          {projectBlocks.length === 0 ? (
                            <div
                              onClick={() => onOpenCreateBlockModal?.()}
                              data-testid="empty-blocks-notice"
                              className="px-2 py-1 text-[11px] text-neutral-500 hover:text-amber-400 cursor-pointer italic"
                            >
                              (No blocks created - click [+ Add new block])
                            </div>
                          ) : (
                            projectBlocks.map((block: LogicBlockDescriptor) => (
                              <div
                                key={block.id}
                                onClick={() => {
                                  const title = `${block.name} [${block.blockType}${block.number ?? ''}]`;
                                  onOpenDocument?.('logic', block.id, title, { targetBlock: block.id });
                                  setSelectedContextItem({
                                    id: block.id,
                                    name: block.name,
                                    type: 'LOGIC_BLOCK',
                                    subType: block.blockType,
                                    metadata: { ...block },
                                  });
                                }}
                                className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center gap-1.5"
                              >
                                <span
                                  className={`font-mono text-[10px] ${
                                    block.blockType === 'OB'
                                      ? 'text-cyan-400'
                                      : block.blockType === 'FB'
                                      ? 'text-amber-400'
                                      : 'text-emerald-400'
                                  }`}
                                >
                                  [{block.blockType}
                                  {block.number ?? ''}]
                                </span>
                                <span>{block.name}</span>
                              </div>
                            ))
                          )}
                        </div>
                      )}
                    </div>

                    {/* PLC Tags */}
                    <div>
                      <div
                        onClick={() => toggleNode('plcTags')}
                        className="flex items-center gap-1.5 px-2 py-1 hover:bg-neutral-800/60 rounded cursor-pointer text-neutral-400"
                      >
                        {expandedNodes.plcTags ? (
                          <ChevronDown className="w-3 h-3 text-neutral-500" />
                        ) : (
                          <ChevronRight className="w-3 h-3 text-neutral-500" />
                        )}
                        <Tag className="w-3.5 h-3.5 text-purple-400" />
                        <span>PLC Tags</span>
                      </div>

                      {expandedNodes.plcTags && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          <div
                            onClick={() => {
                              // No tag table is loaded, so there is no table to select. The entry
                              // used to assert one named "Default Tag Table" occupying an IEC
                              // address range nothing had read.
                              setSelectedContextItem(null);
                            }}
                            className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer"
                          >
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
                    {expandedNodes.visualization ? (
                      <ChevronDown className="w-3 h-3 text-neutral-500" />
                    ) : (
                      <ChevronRight className="w-3 h-3 text-neutral-500" />
                    )}
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
                        {expandedNodes.hmiScreens ? (
                          <ChevronDown className="w-3 h-3 text-neutral-500" />
                        ) : (
                          <ChevronRight className="w-3 h-3 text-neutral-500" />
                        )}
                        <Layers className="w-3.5 h-3.5 text-sky-400" />
                        <span>HMI Screens</span>
                      </div>
                      <button
                        onClick={() => onOpenCreateScreenModal?.()}
                        data-testid="add-screen-btn"
                        className="p-0.5 hover:bg-neutral-700 rounded text-neutral-400 hover:text-white transition-colors"
                        title="Add New Screen"
                      >
                        <Plus className="w-3.5 h-3.5 text-sky-400" />
                      </button>
                    </div>

                    {/* Screens Tree */}
                    {expandedNodes.hmiScreens && (
                      <div className="pl-3 space-y-0.5">
                        {screens.length === 0 ? (
                          <div
                            onClick={() => onOpenCreateScreenModal?.()}
                            data-testid="empty-screens-notice"
                            className="px-2 py-1 text-[11px] text-neutral-500 hover:text-sky-400 cursor-pointer italic"
                          >
                            (No screens created - click [+ Add new screen])
                          </div>
                        ) : (
                          screens.map((screen) => {
                            const isActive = activeScreenId === screen.id;
                            return (
                              <div key={screen.id}>
                                <div
                                  onClick={() => {
                                    openScreenTab(screen.id);
                                    onOpenDocument?.('screen', screen.id, screen.name, {
                                      screenId: screen.id,
                                      title: screen.name,
                                    });
                                    setSelectedContextItem({
                                      id: screen.id,
                                      name: screen.name,
                                      type: 'HMI_SCREEN',
                                      metadata: { is_root: screen.is_root },
                                    });
                                  }}
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
                                        onClick={() => {
                                          openScreenTab(screen.id, sub.id);
                                          onOpenDocument?.('screen', `${screen.id}::${sub.id}`, `${screen.name} / ${sub.name}`, {
                                            screenId: screen.id,
                                            subscreenId: sub.id,
                                            title: sub.name,
                                          });
                                          setSelectedContextItem({
                                            id: sub.id,
                                            name: sub.name,
                                            type: 'HMI_SCREEN',
                                            subType: sub.type,
                                          });
                                        }}
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
                          })
                        )}
                      </div>
                    )}

                    {/* Faceplate Types Library — real master types only (TSK-309) */}
                    <div>
                      <div className="flex items-center justify-between px-2 py-1 text-neutral-400">
                        <div
                          onClick={() => toggleNode('faceplates')}
                          className="flex items-center gap-1.5 cursor-pointer hover:text-white"
                        >
                          {expandedNodes.faceplates ? (
                            <ChevronDown className="w-3 h-3 text-neutral-500" />
                          ) : (
                            <ChevronRight className="w-3 h-3 text-neutral-500" />
                          )}
                          <Boxes className="w-3.5 h-3.5 text-teal-400" />
                          <span>Faceplate Types</span>
                        </div>
                        <button
                          onClick={() => onOpenDocument?.('faceplate', 'new', 'Master Faceplate Types')}
                          data-testid="add-faceplate-type-btn"
                          className="p-0.5 hover:bg-neutral-700 rounded text-neutral-400 hover:text-white transition-colors"
                          title="Add New Faceplate Type"
                        >
                          <Plus className="w-3.5 h-3.5 text-teal-400" />
                        </button>
                      </div>

                      {expandedNodes.faceplates && (
                        <div className="pl-4 space-y-0.5 text-neutral-400">
                          {faceplateStatus === 'LOADING' && (
                            <div className="px-2 py-1 text-[11px] italic text-neutral-500">
                              Loading faceplate types…
                            </div>
                          )}

                          {(faceplateStatus === 'IDLE' || faceplateStatus === 'READY') && !currentProjectId && (
                            <div
                              data-testid="faceplate-tree-no-project"
                              className="px-2 py-1 text-[11px] italic text-neutral-500"
                            >
                              Open a project to see its faceplate types.
                            </div>
                          )}

                          {faceplateStatus === 'ERROR' && (
                            <div
                              data-testid="faceplate-tree-error"
                              className="px-2 py-1 text-[11px] text-rose-400"
                            >
                              {faceplateError}
                            </div>
                          )}

                          {faceplateStatus === 'READY' && currentProjectId && faceplateTypes.length === 0 && (
                            <div
                              onClick={() => onOpenDocument?.('faceplate', 'new', 'Master Faceplate Types')}
                              data-testid="empty-faceplates-notice"
                              className="px-2 py-1 text-[11px] text-neutral-500 hover:text-teal-400 cursor-pointer italic"
                            >
                              (No faceplate types defined - click [+ Add new type])
                            </div>
                          )}

                          {faceplateTypes.map((type) => (
                            <div
                              key={type.id}
                              data-testid="faceplate-type-node"
                              onClick={() => {
                                onOpenDocument?.('faceplate', type.id, type.name);
                                setSelectedContextItem({
                                  id: type.id,
                                  name: type.name,
                                  type: 'FACEPLATE',
                                  udtArchetype: type.udtName,
                                  metadata: {
                                    typeId: type.id,
                                    activeVersion: type.activeVersion,
                                    instanceCount: type.instanceCount,
                                    hasDraft: type.hasDraft ?? false,
                                  },
                                });
                              }}
                              className="px-2 py-1 hover:bg-neutral-800/60 hover:text-white rounded cursor-pointer flex items-center justify-between gap-1"
                            >
                              <span className="truncate">
                                {type.name}{' '}
                                <span className="text-neutral-600">
                                  ({versionLabel(type.activeVersion)})
                                </span>
                              </span>
                              {type.hasDraft && (
                                <span className="text-[9px] text-amber-400">draft</span>
                              )}
                            </div>
                          ))}
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
