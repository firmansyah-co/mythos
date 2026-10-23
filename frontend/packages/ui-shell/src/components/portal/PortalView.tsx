import React, { useState, useEffect } from 'react';
import {
  FolderPlus,
  FolderOpen,
  Package,
  Network,
  Cpu,
  Activity,
  Monitor,
  Radio,
  ArrowRight,
  Shield,
  Plus,
} from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';
import { ProjectMetadata } from '../../types/hmi';
import { MythosLogo } from '../icons/MythosLogo';

export type PortalCategory =
  | 'START'
  | 'DEVICES_NETWORKS'
  | 'PLC_PROGRAMMING'
  | 'MOTION_TECHNOLOGY'
  | 'VISUALIZATION'
  | 'ONLINE_DIAGNOSTICS';

export interface PortalViewProps {
  className?: string;
  onOpenCreateBlockModal?: () => void;
  onOpenCreateScreenModal?: () => void;
}

export const PortalView: React.FC<PortalViewProps> = ({
  className = '',
  onOpenCreateBlockModal,
  onOpenCreateScreenModal,
}) => {
  const [activeCategory, setActiveCategory] = useState<PortalCategory>('START');
  const [startTab, setStartTab] = useState<'CREATE' | 'OPEN' | 'IMPORT'>('CREATE');

  // Form states
  const [projectName, setProjectName] = useState('New_Plant_Station_01');
  const [projectAuthor, setProjectAuthor] = useState('MythOS Systems Team');
  const [projectComment, setProjectComment] = useState('Standard IEC 61131-3 Automation Project');
  const [projectPath, setProjectPath] = useState('/opt/mythos/projects');

  // Existing projects are loaded from the backend; an unavailable API is not a project list.
  const [existingProjects, setExistingProjects] = useState<ProjectMetadata[]>([]);
  const [selectedExistingProjectId, setSelectedExistingProjectId] = useState<string | null>(null);
  const [projectListError, setProjectListError] = useState<string | null>(null);
  const [projectListLoading, setProjectListLoading] = useState(true);
  const [listRevision, setListRevision] = useState(0);

  // Import package state
  const [importPackagePath, setImportPackagePath] = useState('');
  const [importError, setImportError] = useState<string | null>(null);

  // Store actions
  const createNewProject = useHmiStore((s) => s.createNewProject);
  const openExistingProject = useHmiStore((s) => s.openExistingProject);
  const setWorkspaceMode = useHmiStore((s) => s.setWorkspaceMode);
  const openWorkbenchDocument = useHmiStore((s) => s.openWorkbenchDocument);
  const currentProject = useHmiStore((s) => s.currentProject);

  useEffect(() => {
    // Attempt to load existing projects from backend
    let isMounted = true;
    const controller = new AbortController();
    setProjectListLoading(true);
    setProjectListError(null);
    fetch('/api/v1/projects', { signal: controller.signal })
      .then(async (res) => {
        if (!res.ok) {
          throw new Error(`Project list unavailable (HTTP ${res.status})`);
        }
        const data: unknown = await res.json();
        if (!Array.isArray(data)) {
          throw new Error('Project list response was invalid');
        }
        if (!data.every((id) => typeof id === 'string' && id.length > 0)) {
          throw new Error('Project list must contain project IDs');
        }
        return Promise.all(data.map(async (id: string) => {
          const response = await fetch(`/api/v1/projects/${encodeURIComponent(id)}`, { signal: controller.signal });
          if (!response.ok) throw new Error(`Project details unavailable (HTTP ${response.status})`);
          const project: ProjectMetadata = await response.json();
          if (project?.id !== id || typeof project.name !== 'string') throw new Error('Project details were invalid');
          return project;
        }));
      })
      .then((data) => {
        if (isMounted) {
          setExistingProjects(data);
          setSelectedExistingProjectId(data[0]?.id ?? null);
          setProjectListError(null);
        }
      })
      .catch((error: unknown) => {
        if (isMounted) {
          setProjectListError(error instanceof Error ? error.message : 'Project list unavailable');
          setExistingProjects([]);
          setSelectedExistingProjectId(null);
        }
      })
      .finally(() => { if (isMounted) setProjectListLoading(false); });

    return () => {
      isMounted = false;
      controller.abort();
    };
  }, [listRevision]);

  const handleCreateProject = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!projectName.trim()) return;
    await createNewProject(projectName.trim(), projectAuthor.trim(), projectComment.trim(), projectPath.trim());
  };

  const handleOpenExistingProject = async () => {
    if (!selectedExistingProjectId) return;
    await openExistingProject(selectedExistingProjectId);
  };

  const handleImportPackage = () => {
    setImportError(
      importPackagePath.trim()
        ? 'Package restore is unavailable: no backend import operation is connected.'
        : 'Select a package archive before restoring.'
    );
  };

  const jumpToProjectView = () => {
    setWorkspaceMode('PROJECT');
  };

  return (
    <div
      className={`flex w-full h-full bg-[var(--bg-base)] text-neutral-200 select-none overflow-hidden ${className}`}
      data-testid="portal-view"
    >
      {/* Left Navigation Launchpad (Workbench Launchpad Menu) */}
      <aside className="w-72 bg-[var(--bg-surface)] border-r border-neutral-800 flex flex-col justify-between flex-shrink-0">
        <div>
          {/* Portal Brand Header */}
          <div className="p-4 border-b border-neutral-800 bg-[var(--bg-base)]">
            <div className="flex items-center gap-2.5">
              <MythosLogo size={28} className="shrink-0 shadow-md" />
              <div>
                <h1 className="text-sm font-bold text-white tracking-wide">Portal View</h1>
                <p className="text-[10px] text-cyan-400 font-mono">Industrial Automation Suite</p>
              </div>
            </div>
          </div>

          {/* Navigation Items */}
          <nav className="p-2 space-y-1 text-xs">
            <button
              onClick={() => setActiveCategory('START')}
              data-testid="portal-nav-start"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'START'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <FolderPlus className="w-4 h-4 text-blue-400" />
              <span>Start (Projects &amp; Packages)</span>
            </button>

            <button
              onClick={() => setActiveCategory('DEVICES_NETWORKS')}
              data-testid="portal-nav-devices"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'DEVICES_NETWORKS'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <Network className="w-4 h-4 text-cyan-400" />
              <span>Devices &amp; Networks</span>
            </button>

            <button
              onClick={() => setActiveCategory('PLC_PROGRAMMING')}
              data-testid="portal-nav-plc"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'PLC_PROGRAMMING'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <Cpu className="w-4 h-4 text-emerald-400" />
              <span>PLC Programming</span>
            </button>

            <button
              onClick={() => setActiveCategory('MOTION_TECHNOLOGY')}
              data-testid="portal-nav-motion"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'MOTION_TECHNOLOGY'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <Activity className="w-4 h-4 text-amber-400" />
              <span>Motion &amp; Technology</span>
            </button>

            <button
              onClick={() => setActiveCategory('VISUALIZATION')}
              data-testid="portal-nav-visualization"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'VISUALIZATION'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <Monitor className="w-4 h-4 text-sky-400" />
              <span>Visualization (HMI)</span>
            </button>

            <button
              onClick={() => setActiveCategory('ONLINE_DIAGNOSTICS')}
              data-testid="portal-nav-diagnostics"
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md transition-colors text-left font-medium ${
                activeCategory === 'ONLINE_DIAGNOSTICS'
                  ? 'bg-blue-600/20 text-cyan-300 border border-blue-500/40 shadow-sm'
                  : 'hover:bg-neutral-800/60 text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <Radio className="w-4 h-4 text-purple-400" />
              <span>Online &amp; Diagnostics</span>
            </button>
          </nav>
        </div>

        {/* Switch to Project View footer shortcut */}
        <div className="p-3 border-t border-neutral-800 bg-[var(--bg-base)]">
          <button
            onClick={jumpToProjectView}
            data-testid="portal-jump-to-project-btn"
            className="w-full flex items-center justify-between px-3 py-2 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded text-xs text-neutral-300 hover:text-white transition-colors cursor-pointer"
          >
            <span className="font-semibold">🎛️ Switch to Project View</span>
            <ArrowRight className="w-3.5 h-3.5 text-cyan-400" />
          </button>
          {currentProject && (
            <div className="mt-2 text-[10px] text-neutral-500 truncate font-mono">
              ACTIVE: <span className="text-cyan-400 font-semibold">{currentProject.name}</span>
            </div>
          )}
        </div>
      </aside>

      {/* Main Task Launchpad Canvas */}
      <main className="flex-1 flex flex-col bg-[var(--bg-base)] overflow-y-auto">
        {/* Category Header Banner */}
        <div className="px-8 py-6 bg-[var(--bg-surface)] border-b border-neutral-800">
          <div className="max-w-4xl">
            <h2 className="text-xl font-bold text-white tracking-tight">
              {activeCategory === 'START' && 'Start / Project Administration'}
              {activeCategory === 'DEVICES_NETWORKS' && 'Devices & Networks Configuration'}
              {activeCategory === 'PLC_PROGRAMMING' && 'PLC Logic Programming & Blocks'}
              {activeCategory === 'MOTION_TECHNOLOGY' && 'Motion Control & Technology Objects'}
              {activeCategory === 'VISUALIZATION' && 'HMI Screen Visualization & Graphics'}
              {activeCategory === 'ONLINE_DIAGNOSTICS' && 'Online Discovery & System Diagnostics'}
            </h2>
            <p className="text-xs text-neutral-400 mt-1">
              Task-oriented workspace to initialize projects, add automation components, and inspect fieldbus hardware.
            </p>
          </div>
        </div>

        {/* Category Content Area */}
        <div className="p-8 max-w-4xl space-y-6">
          {/* CATEGORY 1: START */}
          {activeCategory === 'START' && (
            <div className="space-y-6">
              {/* Secondary Sub-Tabs: Create / Open / Import */}
              <div className="flex items-center gap-2 border-b border-neutral-800 pb-2">
                <button
                  onClick={() => setStartTab('CREATE')}
                  className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors flex items-center gap-1.5 ${
                    startTab === 'CREATE'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800'
                  }`}
                >
                  <FolderPlus className="w-3.5 h-3.5" />
                  <span>Create new project</span>
                </button>

                <button
                  onClick={() => setStartTab('OPEN')}
                  className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors flex items-center gap-1.5 ${
                    startTab === 'OPEN'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800'
                  }`}
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                  <span>Open existing project</span>
                </button>

                <button
                  onClick={() => setStartTab('IMPORT')}
                  className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors flex items-center gap-1.5 ${
                    startTab === 'IMPORT'
                      ? 'bg-blue-600 text-white shadow-sm'
                      : 'text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800'
                  }`}
                >
                  <Package className="w-3.5 h-3.5" />
                  <span>Import package (.mythos-pkg)</span>
                </button>
              </div>

              {/* Sub-Tab 1: Create New Project */}
              {startTab === 'CREATE' && (
                <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg">
                  <h3 className="text-sm font-semibold text-white mb-4 flex items-center gap-2">
                    <FolderPlus className="w-4 h-4 text-cyan-400" />
                    Create New Automation Project
                  </h3>

                  <form onSubmit={handleCreateProject} className="space-y-4 text-xs">
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <label className="block text-neutral-400 font-medium mb-1">Project Name</label>
                        <input
                          type="text"
                          value={projectName}
                          onChange={(e) => setProjectName(e.target.value)}
                          placeholder="Project name"
                          data-testid="input-project-name"
                          className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
                          required
                        />
                      </div>

                      <div>
                        <label className="block text-neutral-400 font-medium mb-1">Author</label>
                        <input
                          type="text"
                          value={projectAuthor}
                          onChange={(e) => setProjectAuthor(e.target.value)}
                          placeholder="Author name"
                          data-testid="input-project-author"
                          className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
                        />
                      </div>
                    </div>

                    <div>
                      <label className="block text-neutral-400 font-medium mb-1">Comment / Description</label>
                      <input
                        type="text"
                        value={projectComment}
                        onChange={(e) => setProjectComment(e.target.value)}
                        placeholder="Description of the plant automation project"
                        data-testid="input-project-comment"
                        className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500"
                      />
                    </div>

                    <div>
                      <label className="block text-neutral-400 font-medium mb-1">Path</label>
                      <input
                        type="text"
                        value={projectPath}
                        onChange={(e) => setProjectPath(e.target.value)}
                        placeholder="/opt/mythos/projects"
                        data-testid="input-project-path"
                        className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
                      />
                    </div>

                    <div className="pt-2 flex justify-end">
                      <button
                        type="submit"
                        data-testid="btn-create-project"
                        className="px-4 py-2 rounded bg-blue-600 hover:bg-blue-500 text-white font-medium text-xs shadow-md transition-colors flex items-center gap-2 cursor-pointer"
                      >
                        <Plus className="w-4 h-4" />
                        <span>Create Project &amp; Open Workbench</span>
                      </button>
                    </div>
                  </form>
                </div>
              )}

              {/* Sub-Tab 2: Open Existing Project */}
              {startTab === 'OPEN' && (
                <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg">
                  <h3 className="text-sm font-semibold text-white mb-3 flex items-center gap-2">
                    <FolderOpen className="w-4 h-4 text-cyan-400" />
                    Open Existing Project
                  </h3>

                  {projectListError && (
                    <div role="alert" className="mb-3 border border-rose-800 bg-rose-950/40 p-3 text-xs text-rose-200">
                      {projectListError}.
                      <button type="button" onClick={() => setListRevision((value) => value + 1)} className="ml-3 underline">Retry project list</button>
                    </div>
                  )}
                  {projectListLoading && <p role="status" className="mb-3 text-xs text-neutral-400">Loading projects…</p>}
                  {!projectListLoading && !projectListError && existingProjects.length === 0 && (
                    <div className="mb-3 border border-neutral-700 bg-neutral-900 p-3 text-xs text-neutral-400">
                      No projects are available from the backend.
                    </div>
                  )}
                  <div className="space-y-2 mb-4">
                    {existingProjects.map((p) => {
                      const isSelected = selectedExistingProjectId === p.id;
                      return (
                        <div
                          key={p.id}
                          onClick={() => setSelectedExistingProjectId(p.id)}
                          className={`p-3 rounded border cursor-pointer transition-all flex items-center justify-between ${
                            isSelected
                              ? 'bg-blue-600/20 border-blue-500/60 text-white'
                              : 'bg-neutral-900 border-neutral-800 text-neutral-300 hover:bg-neutral-800/80'
                          }`}
                        >
                          <div>
                            <div className="font-semibold text-xs flex items-center gap-2">
                              <span>{p.name}</span>
                              <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-neutral-800 text-neutral-400">
                                v{p.version || '1.0.0'}
                              </span>
                            </div>
                            <div className="text-[11px] text-neutral-400 mt-0.5">{p.description}</div>
                          </div>
                          <div className="text-[11px] text-neutral-500 font-mono">
                            {p.author || 'MythOS Engineer'}
                          </div>
                        </div>
                      );
                    })}
                  </div>

                  <div className="flex justify-end">
                    <button
                      onClick={handleOpenExistingProject}
                      disabled={projectListLoading || !selectedExistingProjectId}
                      data-testid="btn-open-project"
                      className="px-4 py-2 rounded bg-blue-600 hover:bg-blue-500 text-white font-medium text-xs shadow-md transition-colors flex items-center gap-2 cursor-pointer"
                    >
                      <FolderOpen className="w-4 h-4" />
                      <span>Open Selected Project</span>
                    </button>
                  </div>
                </div>
              )}

              {/* Sub-Tab 3: Import Package */}
              {startTab === 'IMPORT' && (
                <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg">
                  <h3 className="text-sm font-semibold text-white mb-2 flex items-center gap-2">
                    <Shield className="w-4 h-4 text-emerald-400" />
                    Restore Encrypted Package Archive (.mythos-pkg)
                  </h3>
                  <p className="text-xs text-neutral-400 mb-4">
                    Restore verified ChaCha20-Poly1305 encrypted project snapshot with Merkle-audited WAL logs.
                  </p>

                  <div className="space-y-4 text-xs">
                    <div>
                      <label className="block text-neutral-400 font-medium mb-1">Package Archive File</label>
                      <input
                        type="text"
                        value={importPackagePath}
                        onChange={(e) => setImportPackagePath(e.target.value)}
                        placeholder="Path to .mythos-pkg archive"
                        data-testid="input-import-package"
                        className="w-full px-3 py-2 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
                      />
                    </div>

                    {importError && (
                      <div role="alert" className="border border-rose-800 bg-rose-950/40 p-2.5 text-rose-200">
                        {importError}
                      </div>
                    )}

                    <div className="flex justify-end">
                      <button
                        onClick={handleImportPackage}
                        data-testid="btn-import-package"
                        className="px-4 py-2 rounded bg-emerald-600 hover:bg-emerald-500 text-white font-medium text-xs shadow-md transition-colors flex items-center gap-2 cursor-pointer"
                      >
                        <Package className="w-4 h-4" />
                        <span>Decrypt &amp; Restore Package</span>
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* CATEGORY 2: DEVICES & NETWORKS */}
          {activeCategory === 'DEVICES_NETWORKS' && (
            <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg space-y-4">
              <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                <Network className="w-4 h-4 text-cyan-400" />
                Devices &amp; Networks
              </h3>
              <p className="text-xs text-neutral-400">
                Configure network topology, add PROFINET IO controllers, and inspect hardware racks.
              </p>

              <div className="grid grid-cols-2 gap-4 pt-2">
                <button
                  onClick={() => {
                    openWorkbenchDocument('topology', 'network-view', '🌐 Network View');
                    setWorkspaceMode('PROJECT');
                  }}
                  data-testid="portal-action-configure-device"
                  className="p-4 bg-neutral-900 border border-neutral-800 hover:border-cyan-500 rounded-lg text-left transition-all group cursor-pointer"
                >
                  <div className="font-semibold text-xs text-neutral-200 group-hover:text-cyan-400 flex items-center justify-between">
                    <span>🌐 Configure a Device / Network</span>
                    <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>
                  <p className="text-[11px] text-neutral-400 mt-1">
                    Open graphic bus topology editor with PROFINET DCP discovery.
                  </p>
                </button>

                <button
                  onClick={() => {
                    openWorkbenchDocument('device', 'device-view', '🎛️ Device View');
                    setWorkspaceMode('PROJECT');
                  }}
                  data-testid="portal-action-write-controller"
                  className="p-4 bg-neutral-900 border border-neutral-800 hover:border-emerald-500 rounded-lg text-left transition-all group cursor-pointer"
                >
                  <div className="font-semibold text-xs text-neutral-200 group-hover:text-emerald-400 flex items-center justify-between">
                    <span>🎛️ Device View (Rack 0)</span>
                    <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>
                  <p className="text-[11px] text-neutral-400 mt-1">
                    View central rack chassis, module slots, and hardware address map.
                  </p>
                </button>
              </div>
            </div>
          )}

          {/* CATEGORY 3: PLC PROGRAMMING */}
          {activeCategory === 'PLC_PROGRAMMING' && (
            <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg space-y-4">
              <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                <Cpu className="w-4 h-4 text-emerald-400" />
                PLC Programming
              </h3>
              <p className="text-xs text-neutral-400">
                Author real-time control logic in ISO/IEC 61131-3 Structured Text (ST) or Ladder Diagram (LD).
              </p>

              <div className="grid grid-cols-2 gap-4 pt-2">
                <button
                  onClick={() => {
                    openWorkbenchDocument('logic', 'MainTask', 'Main [OB1]', { targetBlock: 'MainTask' });
                    setWorkspaceMode('PROJECT');
                  }}
                  data-testid="portal-action-write-plc"
                  className="p-4 bg-neutral-900 border border-neutral-800 hover:border-emerald-500 rounded-lg text-left transition-all group cursor-pointer"
                >
                  <div className="font-semibold text-xs text-neutral-200 group-hover:text-emerald-400 flex items-center justify-between">
                    <span>📝 Write PLC Program</span>
                    <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>
                  <p className="text-[11px] text-neutral-400 mt-1">
                    Open Monaco Logic Editor and write Structured Text control loops.
                  </p>
                </button>

                <button
                  onClick={() => {
                    if (onOpenCreateBlockModal) {
                      onOpenCreateBlockModal();
                    }
                    setWorkspaceMode('PROJECT');
                  }}
                  data-testid="portal-action-add-block"
                  className="p-4 bg-neutral-900 border border-neutral-800 hover:border-amber-500 rounded-lg text-left transition-all group cursor-pointer"
                >
                  <div className="font-semibold text-xs text-neutral-200 group-hover:text-amber-400 flex items-center justify-between">
                    <span>➕ Add New Block (OB / FB / FC / DB)</span>
                    <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>
                  <p className="text-[11px] text-neutral-400 mt-1">
                    Define new organization, function, or instance data blocks.
                  </p>
                </button>
              </div>
            </div>
          )}

          {/* CATEGORY 4: MOTION & TECHNOLOGY */}
          {activeCategory === 'MOTION_TECHNOLOGY' && (
            <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg space-y-4">
              <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                <Activity className="w-4 h-4 text-amber-400" />
                Motion &amp; Technology Objects
              </h3>
              <p className="text-xs text-neutral-400">
                Configure continuous PID control loops, virtual motion axes, and cam profiles.
              </p>

              <button
                onClick={() => {
                  openWorkbenchDocument('logic', 'PID_Compact', 'PID_Compact [FB41]', { targetBlock: 'PID_Compact' });
                  setWorkspaceMode('PROJECT');
                }}
                data-testid="portal-action-motion"
                className="w-full p-4 bg-neutral-900 border border-neutral-800 hover:border-amber-500 rounded-lg text-left transition-all group cursor-pointer"
              >
                <div className="font-semibold text-xs text-neutral-200 group-hover:text-amber-400 flex items-center justify-between">
                  <span>⚙️ Configure Technology Objects (PID_Compact, Axes)</span>
                  <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                </div>
                <p className="text-[11px] text-neutral-400 mt-1">
                  Launch auto-tuning continuous control loop configuration.
                </p>
              </button>
            </div>
          )}

          {/* CATEGORY 5: VISUALIZATION */}
          {activeCategory === 'VISUALIZATION' && (
            <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg space-y-4">
              <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                <Monitor className="w-4 h-4 text-sky-400" />
                Visualization (Web HMI)
              </h3>
              <p className="text-xs text-neutral-400">
                Design ANSI/ISA-101 compliant graphic mimics, high-performance faceplates, and alarms.
              </p>

              <button
                onClick={() => {
                  if (onOpenCreateScreenModal) {
                    onOpenCreateScreenModal();
                  }
                  setWorkspaceMode('PROJECT');
                }}
                data-testid="portal-action-configure-screens"
                className="w-full p-4 bg-neutral-900 border border-neutral-800 hover:border-sky-500 rounded-lg text-left transition-all group cursor-pointer"
              >
                <div className="font-semibold text-xs text-neutral-200 group-hover:text-sky-400 flex items-center justify-between">
                  <span>🖥️ Configure HMI Screens &amp; Graphic Mimics</span>
                  <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                </div>
                <p className="text-[11px] text-neutral-400 mt-1">
                  Add new plant screens or edit existing process mimics with live tag binding.
                </p>
              </button>
            </div>
          )}

          {/* CATEGORY 6: ONLINE & DIAGNOSTICS */}
          {activeCategory === 'ONLINE_DIAGNOSTICS' && (
            <div className="p-6 bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-lg space-y-4">
              <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                <Radio className="w-4 h-4 text-purple-400" />
                Online &amp; Diagnostics
              </h3>
              <p className="text-xs text-neutral-400">
                Scan fieldbus subnets, inspect PROFINET DCP nodes, and monitor real-time cycle times.
              </p>

              <button
                onClick={() => {
                  openWorkbenchDocument('topology', 'network-view', '🌐 Network View', { runScan: true });
                  setWorkspaceMode('PROJECT');
                }}
                data-testid="portal-action-diagnostics"
                className="w-full p-4 bg-neutral-900 border border-neutral-800 hover:border-purple-500 rounded-lg text-left transition-all group cursor-pointer"
              >
                <div className="font-semibold text-xs text-neutral-200 group-hover:text-purple-400 flex items-center justify-between">
                  <span>📡 Accessible Devices Sweep (PROFINET DCP / Modbus)</span>
                  <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity" />
                </div>
                <p className="text-[11px] text-neutral-400 mt-1">
                  Sweep subnet 192.168.1.0/24 for online RT stations and field bus couplers.
                </p>
              </button>
            </div>
          )}
        </div>
      </main>
    </div>
  );
};
