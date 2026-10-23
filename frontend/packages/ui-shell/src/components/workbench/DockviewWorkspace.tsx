import React, { useRef, useCallback, useState, useEffect } from 'react';
import {
  DockviewReact,
  DockviewReadyEvent,
  IDockviewPanelProps,
} from 'dockview-react';
import 'dockview-react/dist/styles/dockview.css';

import { LogicEditorPlugin } from '@mythos/ui-logic-editor';
import { NetworkCanvas, HardwareStationNode, NetworkPort } from '@mythos/ui-topology';
import {
  MotorFaceplate,
  ValveFaceplate,
  PidControllerFaceplate,
  AnalogTransmitterFaceplate,
  resolveComponentData,
} from '@mythos/ui-faceplate-lib';
import { useHmiStore, isCanvasEqual } from '../../stores/useHmiStore';
import { Plus, Layers, FileCode2, Network, Save, Rocket, Undo2, Loader2 } from 'lucide-react';

// Panel 1: Monaco Logic Editor tab
const MonacoLogicPanel: React.FC<IDockviewPanelProps<{ targetBlock?: string; language?: string }>> = (props) => {
  const targetBlock = props.params?.targetBlock || 'MainTask';
  const blockLang = props.params?.language;
  const initialMode =
    blockLang === 'LD'
      ? 'LADDER'
      : blockLang === 'IL'
      ? 'IL'
      : blockLang === 'FBD'
      ? 'FBD'
      : blockLang === 'SFC'
      ? 'SFC'
      : 'ST';
  return (
    <div className="w-full h-full">
      <LogicEditorPlugin targetBlock={targetBlock} initialMode={initialMode} />
    </div>
  );
};

// Panel 2: HMI Screen Canvas tab (Real Faceplates & ISA-101 Canvas)
export const HmiScreenCanvasPanel: React.FC<IDockviewPanelProps<{ screenId?: string; title?: string; subscreenId?: string }>> = (props) => {
  const screenId = props.params?.screenId || 'screen_01_plant_overview';
  const title = props.params?.title || 'Screen Canvas';
  const lifecycleMode = useHmiStore((s) => s.lifecycleMode);
  const screens = useHmiStore((s) => s.screens);
  const draftScreens = useHmiStore((s) => s.draftScreens);
  const activeScreens = useHmiStore((s) => s.activeScreens);
  const updateDraftCanvas = useHmiStore((s) => s.updateDraftCanvas);
  const saveScreenDraft = useHmiStore((s) => s.saveScreenDraft);
  const commitHmiDraft = useHmiStore((s) => s.commitHmiDraft);
  const discardHmiDraft = useHmiStore((s) => s.discardHmiDraft);
  const currentProject = useHmiStore((s) => s.currentProject);
  const screenSaveStatus = useHmiStore((s) => s.screenSaveStatus);
  const setSelectedContextItem = useHmiStore((s) => s.setSelectedContextItem);

  // Screen & Subscreen resolution
  const baseId = screenId.includes('::') ? screenId.split('::')[0] : screenId;
  const subId = props.params?.subscreenId || (screenId.includes('::') ? screenId.split('::')[1] : null);

  const screen = screens.find((s) => s.id === baseId || s.id === screenId);
  const subscreen = subId ? screen?.subscreens?.find((sb) => sb.id === subId) : null;

  // Dual-buffer screen resolution: draft buffer priority, then committed active buffer, then screen model
  const canvasData =
    draftScreens.get(screenId) ||
    draftScreens.get(baseId) ||
    activeScreens.get(screenId) ||
    activeScreens.get(baseId) ||
    subscreen?.canvasData ||
    screen?.canvasData;

  const components: any[] = canvasData?.components || [];

  // Persistence state for this panel. The draft buffer is only written on edit, so a saved
  // screen still shows a draft entry: dirty has to be judged against the last saved revision.
  const savedScreens = useHmiStore((s) => s.savedScreens);
  const isSubscreenTab = screenId.includes('::');
  const hasDraft = draftScreens.has(screenId);
  const isDirty = hasDraft && !isCanvasEqual(canvasData, savedScreens.get(screenId));
  const isBusy = screenSaveStatus === 'SAVING' || screenSaveStatus === 'COMMITTING';
  let unsavedScreenCount = 0;
  draftScreens.forEach((draft, id) => {
    if (!isCanvasEqual(draft, savedScreens.get(id))) unsavedScreenCount += 1;
  });
  // The backend persists canvas data per screen; sub-screen canvases have no storage route yet.
  const canPersist = Boolean(currentProject) && !isSubscreenTab;

  // Save status is shared by the store, so it is only shown on a panel that actually has
  // pending work; a clean panel keeps reporting its own in-sync state.
  const statusLabel = isSubscreenTab
    ? 'SUB-SCREEN (NOT PERSISTED)'
    : isDirty && screenSaveStatus === 'ERROR'
    ? 'SAVE FAILED'
    : isDirty && isBusy
    ? screenSaveStatus === 'SAVING'
      ? 'SAVING…'
      : 'COMMITTING…'
    : isDirty
    ? 'UNSAVED CHANGES'
    : screenSaveStatus === 'SAVED' || screenSaveStatus === 'COMMITTED'
    ? 'SAVED'
    : 'IN SYNC';

  const statusTone = isSubscreenTab
    ? 'text-neutral-400 border-neutral-700 bg-neutral-900'
    : isDirty && screenSaveStatus === 'ERROR'
    ? 'text-rose-400 border-rose-800 bg-rose-950/60'
    : isDirty
    ? 'text-amber-400 border-amber-800 bg-amber-950/60'
    : 'text-emerald-400 border-emerald-800 bg-emerald-950/60';

  const handleSave = useCallback(async () => {
    if (!canPersist) return;
    // A panel on a never-edited screen has no draft entry yet: seed one so Save writes exactly
    // what the operator is looking at rather than silently doing nothing.
    if (!hasDraft && canvasData) updateDraftCanvas(screenId, canvasData);
    await saveScreenDraft(screenId);
  }, [canPersist, hasDraft, canvasData, screenId, saveScreenDraft, updateDraftCanvas]);

  const handleDiscard = useCallback(() => {
    discardHmiDraft();
    setSelectedContextItem({
      id: screenId,
      name: screen?.name || title,
      type: 'HMI_SCREEN',
      metadata: { screenId, subscreenId: subId, is_root: screen?.is_root },
    });
  }, [discardHmiDraft, screenId, screen, title, subId, setSelectedContextItem]);

  // Canvas selection handler
  const handleCanvasClick = useCallback(() => {
    setSelectedContextItem({
      id: screenId,
      name: screen?.name || title,
      type: 'HMI_SCREEN',
      metadata: {
        screenId,
        subscreenId: subId,
        is_root: screen?.is_root,
      },
    });
  }, [screenId, screen, title, subId, setSelectedContextItem]);

  // Component click selection handler
  const handleComponentClick = useCallback(
    (comp: any, e: React.MouseEvent) => {
      e.stopPropagation();
      const compType = (comp.type || 'DEVICE').toUpperCase();
      const tag = comp.tag || comp.props?.tag || comp.id || 'DEV-01';
      const label = comp.label || comp.props?.label || tag;
      const propsData = comp.props || {};

      setSelectedContextItem({
        id: comp.id || tag,
        name: tag,
        type: 'FACEPLATE',
        subType: comp.udtArchetype || `${compType}_UDT`,
        hardwareAddress: comp.hardwareAddress || propsData.hardwareAddress || '%QX0.0',
        udtArchetype: comp.udtArchetype || propsData.udtArchetype || `FB_${compType}_UDT`,
        metadata: {
          id: comp.id,
          tag,
          label,
          type: compType,
          ...propsData,
        },
      });
    },
    [setSelectedContextItem]
  );

  // Quick faceplate insertion for empty screens or testing.
  // Only design configuration is seeded. No process value is written: a component that has
  // just been placed is not bound to a device yet, and SYS-REQ-096 forbids inventing one.
  const handleAddFaceplate = (type: 'MOTOR' | 'VALVE' | 'PID' | 'TRANSMITTER') => {
    const newComp =
      type === 'MOTOR'
        ? {
            id: `mtr_${Date.now()}`,
            type: 'MOTOR',
            tag: `MTR-${Math.floor(Math.random() * 800 + 100)}`,
            label: 'Conveyor Drive Motor',
            udtArchetype: 'FB_Motor_UDT',
            hardwareAddress: '%QX0.0..%QX0.3',
            props: { tier: 2 },
          }
        : type === 'VALVE'
        ? {
            id: `vlv_${Date.now()}`,
            type: 'VALVE',
            tag: `VLV-${Math.floor(Math.random() * 800 + 100)}`,
            label: 'Pneumatic Isolation Valve',
            udtArchetype: 'FB_Digital_Valve_UDT',
            hardwareAddress: '%QX1.0..%QX1.1',
            props: { tier: 2 },
          }
        : type === 'PID'
        ? {
            id: `tic_${Date.now()}`,
            type: 'PID',
            tag: `TIC-${Math.floor(Math.random() * 800 + 100)}`,
            label: 'Temperature Loop Controller',
            udtArchetype: 'FB_PID_Controller_UDT',
            hardwareAddress: '%IW30..%QW30',
            props: { tier: 2 },
          }
        : {
            id: `pt_${Date.now()}`,
            type: 'TRANSMITTER',
            tag: `PT-${Math.floor(Math.random() * 800 + 100)}`,
            label: 'Line Pressure Transmitter',
            udtArchetype: 'FB_Analog_Transmitter_UDT',
            hardwareAddress: '%IW20',
            props: { tier: 2 },
          };

    const nextComponents = [...components, newComp];
    updateDraftCanvas(screenId, {
      ...(canvasData || {}),
      title: canvasData?.title || screen?.name || title,
      components: nextComponents,
    });
    setSelectedContextItem({
      id: newComp.id,
      name: newComp.tag,
      type: 'FACEPLATE',
      subType: type,
      hardwareAddress: newComp.hardwareAddress,
      udtArchetype: newComp.udtArchetype,
      metadata: { ...newComp },
    });
  };

  return (
    <div className="w-full h-full flex flex-col bg-[var(--bg-base)] text-neutral-200 select-none overflow-hidden">
      {/* Canvas Top Bar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-[var(--bg-surface)] border-b border-neutral-800 text-xs">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-neutral-300">{title}</span>
          <span className="text-[10px] font-mono text-neutral-500">[{screenId}]</span>
          <span
            className={`px-2 py-0.5 rounded text-[10px] font-mono font-semibold ${
              lifecycleMode === 'EDIT'
                ? 'bg-amber-950/60 text-amber-400 border border-amber-800'
                : lifecycleMode === 'PREVIEW'
                ? 'bg-sky-950/60 text-sky-400 border border-sky-800'
                : 'bg-emerald-950/60 text-emerald-400 border border-emerald-800'
            }`}
          >
            MODE: {lifecycleMode}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span
            data-testid="canvas-save-status"
            className={`px-2 py-0.5 rounded text-[10px] font-mono font-semibold border ${statusTone}`}
            title={
              canPersist
                ? 'Draft canvases are stored per project on the backend'
                : isSubscreenTab
                ? 'Sub-screen canvases are not persisted by the backend yet'
                : 'Open a project before saving a screen'
            }
          >
            {statusLabel}
          </span>

          <button
            type="button"
            data-testid="canvas-discard-btn"
            onClick={handleDiscard}
            disabled={!hasDraft || isBusy}
            title="Discard unsaved canvas edits and reload the last saved revision"
            className="flex items-center gap-1 px-2 py-1 rounded border border-neutral-700 bg-neutral-900 text-[11px] text-neutral-300 hover:bg-neutral-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <Undo2 className="w-3.5 h-3.5" />
            <span>DISCARD</span>
          </button>

          <button
            type="button"
            data-testid="canvas-save-btn"
            onClick={handleSave}
            disabled={!canPersist || isBusy}
            title="Save this screen draft to the backend (Ctrl+S)"
            className="flex items-center gap-1 px-2 py-1 rounded border border-cyan-700 bg-cyan-950/60 text-[11px] text-cyan-300 hover:bg-cyan-900 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            {screenSaveStatus === 'SAVING' ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Save className="w-3.5 h-3.5" />
            )}
            <span>SAVE</span>
          </button>

          <button
            type="button"
            data-testid="canvas-commit-btn"
            onClick={() => { void commitHmiDraft(); }}
            disabled={!canPersist || isBusy}
            title="Save all drafts and commit them to the active runtime screens (Ctrl+Shift+S)"
            className="flex items-center gap-1 px-2 py-1 rounded border border-emerald-700 bg-emerald-950/60 text-[11px] text-emerald-300 hover:bg-emerald-900 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            {screenSaveStatus === 'COMMITTING' ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Rocket className="w-3.5 h-3.5" />
            )}
            <span>COMMIT</span>
          </button>

          <span className="text-[11px] font-mono text-neutral-400 border-l border-neutral-800 pl-2">
            RESOLUTION: 1920x1080 (FHD) • GRID: 16px SNAP
          </span>
        </div>
      </div>

      {/* Industrial Canvas Area */}
      {components.length === 0 ? (
        /* Empty State: Clean ISA-101 design canvas with 16px grid snap */
        <div
          onClick={handleCanvasClick}
          className="flex-1 w-full relative overflow-auto p-8 flex items-center justify-center cursor-default"
          style={{
            backgroundImage:
              'radial-gradient(circle, rgba(255,255,255,0.06) 1px, transparent 1px)',
            backgroundSize: '16px 16px',
          }}
          data-testid="isa101-empty-canvas"
        >
          <div className="max-w-md w-full bg-[var(--bg-surface)] border border-neutral-800 rounded-xl p-8 text-center shadow-2xl space-y-4">
            <div className="w-12 h-12 rounded-xl bg-blue-600/20 border border-blue-500/40 text-cyan-400 mx-auto flex items-center justify-center shadow-inner">
              <Layers className="w-6 h-6 text-cyan-400" />
            </div>

            <div className="space-y-1.5">
              <h3 className="text-sm font-bold text-white tracking-wide">
                ISA-101 Design Canvas - No components placed yet
              </h3>
              <p className="text-xs text-neutral-400 leading-relaxed">
                Screen <span className="font-mono text-cyan-400">[{screenId}]</span> is ready for layout design.
                Place reusable faceplates from Zone 4 (Task Cards &amp; Libraries) or use quick inserts below.
              </p>
            </div>

            <div className="pt-2 flex flex-wrap gap-2 justify-center">
              <button
                type="button"
                data-testid="add-motor-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  handleAddFaceplate('MOTOR');
                }}
                className="px-2.5 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 text-xs font-mono text-cyan-400 rounded transition-colors cursor-pointer"
              >
                + Motor (FB_Motor)
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  handleAddFaceplate('VALVE');
                }}
                className="px-2.5 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 text-xs font-mono text-emerald-400 rounded transition-colors cursor-pointer"
              >
                + Valve (FB_Valve)
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  handleAddFaceplate('PID');
                }}
                className="px-2.5 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 text-xs font-mono text-amber-400 rounded transition-colors cursor-pointer"
              >
                + PID (FB_PID)
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  handleAddFaceplate('TRANSMITTER');
                }}
                className="px-2.5 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 text-xs font-mono text-purple-400 rounded transition-colors cursor-pointer"
              >
                + Analog (PT-101)
              </button>
            </div>
          </div>
        </div>
      ) : (
        /* Real Faceplates Container */
        <div
          onClick={handleCanvasClick}
          className="flex-1 w-full relative overflow-auto p-8"
          style={{
            backgroundImage:
              'radial-gradient(circle, rgba(255,255,255,0.06) 1px, transparent 1px)',
            backgroundSize: '16px 16px',
          }}
          data-testid="isa101-populated-canvas"
        >
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-2 xl:grid-cols-4 gap-6 items-start">
            {components.map((comp, idx) => {
              const compType = (comp.type || 'MOTOR').toUpperCase();
              const compId = comp.id || `comp-${idx}`;
              const tag = comp.tag || comp.props?.tag || compId;
              const label = comp.label || comp.props?.label || tag;
              // Canonical resolution of a canvas component into displayable data. There is no
              // default value on this path: a component with no binding and no stored value
              // resolves to `NOT_BOUND`, and the faceplate renders a quiescent no-data state.
              // SYS-REQ-096 — never substitute a plausible process value here.
              const { quality, values, config } = resolveComponentData(comp);
              const tier = (config.tier as 1 | 2 | 3 | undefined) ?? (comp.tier as 1 | 2 | 3 | undefined) ?? 2;

              if (compType === 'MOTOR' || compType === 'VFD') {
                return (
                  <div
                    key={compId}
                    onClick={(e) => handleComponentClick(comp, e)}
                    className="cursor-pointer hover:ring-2 hover:ring-cyan-500/50 rounded-lg transition-all"
                  >
                    <MotorFaceplate
                      tag={tag}
                      label={label}
                      quality={quality}
                      running={values.running as boolean | null | undefined}
                      tripped={values.tripped as boolean | null | undefined}
                      actualSpeed={values.actualSpeed as number | null | undefined}
                      speedSetpoint={values.speedSetpoint as number | null | undefined}
                      currentAmps={values.currentAmps as number | null | undefined}
                      torquePercent={values.torquePercent as number | null | undefined}
                      maxSpeed={config.maxSpeed as number | undefined}
                      operatingMode={values.operatingMode as 'MANUAL' | 'AUTO' | 'CASCADE' | 'LOCAL' | 'REMOTE' | undefined}
                      tier={tier}
                    />
                  </div>
                );
              }

              if (compType === 'VALVE' || compType === 'ACTUATOR') {
                return (
                  <div
                    key={compId}
                    onClick={(e) => handleComponentClick(comp, e)}
                    className="cursor-pointer hover:ring-2 hover:ring-cyan-500/50 rounded-lg transition-all"
                  >
                    <ValveFaceplate
                      tag={tag}
                      label={label}
                      quality={quality}
                      openLimit={values.openLimit as boolean | null | undefined}
                      closedLimit={values.closedLimit as boolean | null | undefined}
                      transitFault={values.transitFault as boolean | null | undefined}
                      positionPercent={values.positionPercent as number | null | undefined}
                      positionSetpoint={values.positionSetpoint as number | null | undefined}
                      operatingMode={values.operatingMode as 'MANUAL' | 'AUTO' | 'CASCADE' | 'LOCAL' | 'REMOTE' | undefined}
                      tier={tier}
                    />
                  </div>
                );
              }

              if (compType === 'PID' || compType === 'CONTROLLER') {
                return (
                  <div
                    key={compId}
                    onClick={(e) => handleComponentClick(comp, e)}
                    className="cursor-pointer hover:ring-2 hover:ring-cyan-500/50 rounded-lg transition-all"
                  >
                    <PidControllerFaceplate
                      tag={tag}
                      label={label}
                      quality={quality}
                      pv={values.pv as number | null | undefined}
                      sp={values.sp as number | null | undefined}
                      cv={values.cv as number | null | undefined}
                      unit={config.unit as string | undefined}
                      kp={config.kp as number | null | undefined}
                      tn={config.tn as number | null | undefined}
                      tv={config.tv as number | null | undefined}
                      spMin={config.spMin as number | null | undefined}
                      spMax={config.spMax as number | null | undefined}
                      operatingMode={values.operatingMode as 'MANUAL' | 'AUTO' | 'CASCADE' | 'LOCAL' | 'REMOTE' | undefined}
                      tier={tier}
                    />
                  </div>
                );
              }

              if (compType === 'TRANSMITTER' || compType === 'ANALOG') {
                return (
                  <div
                    key={compId}
                    onClick={(e) => handleComponentClick(comp, e)}
                    className="cursor-pointer hover:ring-2 hover:ring-cyan-500/50 rounded-lg transition-all"
                  >
                    <AnalogTransmitterFaceplate
                      tag={tag}
                      label={label}
                      quality={quality}
                      pv={values.pv as number | null | undefined}
                      unit={config.unit as string | undefined}
                      transmitterType={config.transmitterType as 'PRESSURE' | 'TEMPERATURE' | 'FLOW' | 'LEVEL' | undefined}
                      minRange={config.minRange as number | null | undefined}
                      maxRange={config.maxRange as number | null | undefined}
                      highLimit={config.highLimit as number | null | undefined}
                      highHighLimit={config.highHighLimit as number | null | undefined}
                      lowLimit={config.lowLimit as number | null | undefined}
                      lowLowLimit={config.lowLowLimit as number | null | undefined}
                      sensorFault={values.sensorFault as boolean | null | undefined}
                      tier={tier}
                    />
                  </div>
                );
              }

              return (
                <div
                  key={compId}
                  onClick={(e) => handleComponentClick(comp, e)}
                  className="p-4 bg-[var(--bg-surface)] border border-neutral-800 hover:border-cyan-500 rounded-lg cursor-pointer transition-all"
                >
                  <div className="text-[10px] font-mono text-cyan-400">{tag}</div>
                  <div className="text-xs font-semibold text-white mt-1">{label}</div>
                  <div className="text-[10px] text-neutral-400 mt-2 font-mono">[{compType}]</div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Canvas Bottom Status Bar */}
      <div className="flex items-center justify-between px-3 py-1 bg-[var(--bg-surface)] border-t border-neutral-800 text-[11px] text-neutral-500 font-mono">
        <span>TagBus Status: <strong className="text-emerald-400 font-mono">ACTIVE (100ms)</strong></span>
        <span>Components: <strong className="text-cyan-400 font-mono">{components.length}</strong></span>
        <span>
          Unsaved drafts:{' '}
          <strong className={unsavedScreenCount > 0 ? 'text-amber-400 font-mono' : 'text-emerald-400 font-mono'}>
            {unsavedScreenCount} {unsavedScreenCount === 1 ? 'screen' : 'screens'}
          </strong>
        </span>
      </div>
    </div>
  );
};

// Panel 3: Network Topology Canvas tab (Native @mythos/ui-topology xyflow Canvas)
const NetworkTopologyPanel: React.FC<IDockviewPanelProps<{ title?: string }>> = (_props) => {
  const setSelectedContextItem = useHmiStore((s) => s.setSelectedContextItem);

  const handleSelectStation = useCallback(
    (station: HardwareStationNode | null) => {
      if (station) {
        setSelectedContextItem({
          id: station.id,
          name: station.stationName,
          type: 'DEVICE',
          subType: station.stationType,
          hardwareAddress: station.ipAddress,
          udtArchetype: station.articleNumber,
          metadata: {
            stationType: station.stationType,
            rackNumber: station.rackNumber,
            ipAddress: station.ipAddress,
            articleNumber: station.articleNumber,
            status: station.status,
            slots: station.slots,
            ports: station.ports,
          },
        });
      } else {
        setSelectedContextItem(null);
      }
    },
    [setSelectedContextItem]
  );

  const handleSelectPort = useCallback(
    (stationId: string, port: NetworkPort) => {
      setSelectedContextItem({
        id: `${stationId}::${port.id}`,
        name: `${port.name} (${port.protocol})`,
        type: 'DEVICE',
        subType: 'PORT',
        hardwareAddress: port.protocol,
        udtArchetype: port.connectorType,
        metadata: {
          stationId,
          portId: port.id,
          protocol: port.protocol,
          connectorType: port.connectorType,
          linkStatus: port.linkStatus,
        },
      });
    },
    [setSelectedContextItem]
  );

  const handleSelectSlot = useCallback(
    (stationId: string, slotNumber: number) => {
      setSelectedContextItem({
        id: `${stationId}::slot_${slotNumber}`,
        name: `Slot ${slotNumber}`,
        type: 'DEVICE',
        subType: 'MODULE_SLOT',
        metadata: {
          stationId,
          slotNumber,
        },
      });
    },
    [setSelectedContextItem]
  );

  return (
    <div className="w-full h-full flex flex-col bg-[var(--bg-base)] text-neutral-200 overflow-hidden select-none">
      <NetworkCanvas
        className="w-full h-full"
        onSelectStation={handleSelectStation}
        onSelectPort={handleSelectPort}
        onSelectSlot={handleSelectSlot}
      />
    </div>
  );
};

export interface DockviewWorkspaceProps {
  className?: string;
  onOpenCreateBlockModal?: () => void;
  onOpenCreateScreenModal?: () => void;
}

export const DockviewWorkspace: React.FC<DockviewWorkspaceProps> = ({
  className = '',
  onOpenCreateBlockModal,
  onOpenCreateScreenModal,
}) => {
  const dockviewApiRef = useRef<any>(null);
  const [panelCount, setPanelCount] = useState<number>(0);
  const workbenchTabs = useHmiStore((s) => s.workbenchTabs);
  const activeWorkbenchTabId = useHmiStore((s) => s.activeWorkbenchTabId);
  const setSelectedContextItem = useHmiStore((s) => s.setSelectedContextItem);

  const openDocument = useCallback(
    (docType: string, id: string, title: string, params: Record<string, any> = {}) => {
      const api = dockviewApiRef.current;
      const panelId = `panel_${id.replace(/[^a-zA-Z0-9_-]/g, '_')}`;

      if (!api) return;

      const existing = api.getPanel(panelId) || api.panels?.find((p: any) => p.id === panelId);
      if (existing) {
        existing.api?.setActive();
        return;
      }

      let componentName = 'monacoLogic';
      if (docType === 'screen') {
        componentName = 'hmiScreenCanvas';
      } else if (docType === 'topology' || docType === 'device' || docType === 'network-view') {
        componentName = 'networkTopology';
      }

      try {
        api.addPanel({
          id: panelId,
          component: componentName,
          title,
          params: {
            targetBlock: id,
            screenId: id,
            title,
            ...params,
          },
        });
        setPanelCount(api.panels ? api.panels.length : 1);
      } catch (e) {
        console.warn('Dockview addPanel error:', e);
      }
    },
    []
  );

  const onReady = useCallback(
    (event: DockviewReadyEvent) => {
      dockviewApiRef.current = event.api;

      const updateCount = () => {
        setPanelCount(event.api.panels.length);
      };

      event.api.onDidAddPanel(updateCount);
      event.api.onDidRemovePanel(updateCount);
      event.api.onDidActivePanelChange((evt: any) => {
        const panel = evt?.panel;
        if (panel) {
          const pId = panel.id;
          const title = panel.title || '';
          const compName = panel.view?.component || (panel as any).component || '';
          setSelectedContextItem({
            id: pId,
            name: title,
            type: compName === 'hmiScreenCanvas' ? 'HMI_SCREEN' : compName === 'networkTopology' ? 'DEVICE' : 'LOGIC_BLOCK',
          });
        }
      });

      if (workbenchTabs && workbenchTabs.length > 0) {
        workbenchTabs.forEach((tab) => {
          openDocument(tab.docType, tab.id, tab.title, tab.params);
        });
        setPanelCount(event.api.panels.length);
      } else {
        setPanelCount(event.api.panels.length);
      }
    },
    [workbenchTabs, openDocument, setSelectedContextItem]
  );

  // Synchronize open tabs with store's workbenchTabs
  useEffect(() => {
    if (dockviewApiRef.current && workbenchTabs && workbenchTabs.length > 0) {
      workbenchTabs.forEach((tab) => {
        const panelId = `panel_${tab.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
        const existing = dockviewApiRef.current.getPanel(panelId);
        if (!existing) {
          openDocument(tab.docType, tab.id, tab.title, tab.params);
        }
      });
      setPanelCount(dockviewApiRef.current.panels ? dockviewApiRef.current.panels.length : 0);
    }
  }, [workbenchTabs, openDocument]);

  // Synchronize active tab
  useEffect(() => {
    if (dockviewApiRef.current && activeWorkbenchTabId) {
      const panelId = `panel_${activeWorkbenchTabId.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
      const existing = dockviewApiRef.current.getPanel(panelId);
      if (existing) {
        existing.api?.setActive();
      }
    }
  }, [activeWorkbenchTabId]);

  const isWorkspaceEmpty = panelCount === 0;

  return (
    <div
      className={`w-full h-full bg-[var(--bg-base)] overflow-hidden relative ${className}`}
      data-testid="dockview-workspace-host"
    >
      <DockviewReact
        components={{
          monacoLogic: MonacoLogicPanel,
          hmiScreenCanvas: HmiScreenCanvasPanel,
          networkTopology: NetworkTopologyPanel,
        }}
        onReady={onReady}
        className="dockview-theme-dark h-full w-full"
      />

      {/* Authentic Industrial Welcoming Empty State (TSK-602) */}
      {isWorkspaceEmpty && (
        <div
          className="absolute inset-0 z-20 flex flex-col items-center justify-center p-8 bg-[var(--bg-base)]/95 backdrop-blur-xs select-none pointer-events-auto"
          data-testid="dockview-empty-state"
        >
          <div className="max-w-md w-full bg-[var(--bg-surface)] border border-neutral-800 rounded-xl p-8 text-center shadow-2xl space-y-5">
            <div className="w-12 h-12 rounded-xl bg-blue-600/20 border border-blue-500/40 text-cyan-400 mx-auto flex items-center justify-center shadow-inner">
              <FileCode2 className="w-6 h-6 text-cyan-400" />
            </div>

            <div className="space-y-2">
              <h3 className="text-base font-bold text-white tracking-wide">Workbench Canvas is Empty</h3>
              <p className="text-xs text-neutral-400 leading-relaxed">
                Project is empty. Add a PLC block or HMI screen from the Project Tree to begin.
              </p>
            </div>

            <div className="flex flex-col gap-2 pt-2">
              <button
                onClick={() => onOpenCreateBlockModal?.()}
                data-testid="empty-add-block-btn"
                className="w-full px-4 py-2.5 bg-blue-600 hover:bg-blue-500 rounded-lg text-white text-xs font-semibold flex items-center justify-center gap-2 shadow-md transition-colors cursor-pointer"
              >
                <Plus className="w-4 h-4" />
                <span>Create Program Block (OB / FB / FC)</span>
              </button>

              <button
                onClick={() => onOpenCreateScreenModal?.()}
                data-testid="empty-add-screen-btn"
                className="w-full px-4 py-2.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded-lg text-neutral-200 text-xs font-semibold flex items-center justify-center gap-2 transition-colors cursor-pointer"
              >
                <Layers className="w-4 h-4 text-sky-400" />
                <span>Create HMI Screen</span>
              </button>

              <button
                onClick={() => openDocument('topology', 'network-view', '🌐 Network View')}
                data-testid="empty-open-network-btn"
                className="w-full px-4 py-2 bg-transparent hover:bg-neutral-800/60 rounded-lg text-neutral-400 hover:text-white text-[11px] font-medium flex items-center justify-center gap-1.5 transition-colors cursor-pointer"
              >
                <Network className="w-3.5 h-3.5 text-cyan-400" />
                <span>Open Network Topology</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
