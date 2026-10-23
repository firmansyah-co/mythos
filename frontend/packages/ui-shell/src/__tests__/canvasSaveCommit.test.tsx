import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { useHmiStore, isCanvasEqual } from '../stores/useHmiStore';
import { HmiScreenCanvasPanel } from '../components/workbench/DockviewWorkspace';

let container: HTMLDivElement;
let root: Root;

/** Minimal IDockviewPanelProps stand-in: the panel only reads `params`. */
const panelProps = (params: { screenId?: string; title?: string; subscreenId?: string }) =>
  ({ params } as any);

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

/** In-memory backend: PUT draft stores the canvas, GET screens returns what was stored. */
function installFakeBackend(projectId: string) {
  const persisted = new Map<string, any>();
  const calls: Array<{ method: string; url: string; body?: any }> = [];
  global.fetch = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    calls.push({ method: init?.method ?? 'GET', url, body: init?.body ? JSON.parse(String(init.body)) : undefined });
    const draftMatch = url.match(/\/hmi\/screens\/([^/]+)\/draft$/);
    if (draftMatch && init?.method === 'PUT') {
      persisted.set(draftMatch[1], JSON.parse(String(init.body)));
      return { ok: true, status: 204, json: async () => ({}) };
    }
    if (url.endsWith('/hmi/commit') && init?.method === 'POST') {
      return { ok: true, status: 200, json: async () => Object.fromEntries(persisted) };
    }
    if (url === `/api/v1/projects/${projectId}` && !init?.method) {
      return { ok: true, status: 200, json: async () => ({ id: projectId, name: 'Plant A', version: '1.0.0' }) };
    }
    if (url.endsWith('/hmi/screens') && !init?.method) {
      return {
        ok: true,
        status: 200,
        json: async () => [...persisted].map(([id, canvasData]) => ({
          id, name: id, is_root: false, subscreens: [], canvasData,
        })),
      };
    }
    return { ok: false, status: 404, json: async () => ({}) };
  });
  return { persisted, calls, projectId };
}

describe('Canvas Save / Commit persistence (save → reload keeps canvas components)', () => {
  let backend: ReturnType<typeof installFakeBackend>;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    backend = installFakeBackend('plant_a');
    useHmiStore.getState().resetToEmptyProject();
    useHmiStore.getState().loadSampleProject();
    useHmiStore.setState({ currentProject: { id: 'plant_a', name: 'Plant A', version: '1.0.0' } });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    flushSync(() => root.unmount());
    container.remove();
  });

  it('renders Save, Commit and Discard controls on the canvas toolbar', () => {
    const html = renderClient(<HmiScreenCanvasPanel {...panelProps({ screenId: 'screen_02_boiler_area', title: 'Boiler' })} />);
    expect(html).toContain('data-testid="canvas-save-btn"');
    expect(html).toContain('data-testid="canvas-commit-btn"');
    expect(html).toContain('data-testid="canvas-discard-btn"');
    expect(html).toContain('data-testid="canvas-save-status"');
    expect(html).toContain('SAVE');
    expect(html).toContain('COMMIT');
    expect(html).toContain('DISCARD');
  });

  it('marks an edited canvas as unsaved, then clears the badge once Save persists it', async () => {
    renderClient(<HmiScreenCanvasPanel {...panelProps({ screenId: 'screen_02_boiler_area', title: 'Boiler' })} />);
    const canvas = { title: 'Boiler Mimic', components: [{ id: 'mtr_900', type: 'MOTOR', tag: 'MTR-900' }] };

    flushSync(() => useHmiStore.getState().updateDraftCanvas('screen_02_boiler_area', canvas));
    expect(useHmiStore.getState().isHmiDirty).toBe(true);
    expect(container.innerHTML).toContain('UNSAVED CHANGES');

    await flushSync(() => {}) ?? null;
    await useHmiStore.getState().saveScreenDraft('screen_02_boiler_area');

    const state = useHmiStore.getState();
    expect(state.screenSaveStatus).toBe('SAVED');
    expect(backend.persisted.get('screen_02_boiler_area')).toEqual(canvas);
    expect(container.innerHTML).toContain('SAVED');
    expect(container.innerHTML).not.toContain('UNSAVED CHANGES');
  });

  it('keeps the saved canvas components after a reload of the same project', async () => {
    const screenId = 'screen_02_boiler_area';
    const canvas = {
      title: 'Boiler Mimic',
      components: [
        { id: 'mtr_900', type: 'MOTOR', tag: 'MTR-900', label: 'Feed Pump Motor' },
        { id: 'vlv_900', type: 'VALVE', tag: 'VLV-900', label: 'Boiler Isolation Valve' },
      ],
    };

    useHmiStore.getState().updateDraftCanvas(screenId, canvas);
    expect(await useHmiStore.getState().saveScreenDraft(screenId)).toBe(true);

    // Simulate a browser reload: every in-memory buffer is dropped, project is re-opened from the backend.
    useHmiStore.getState().resetToEmptyProject();
    useHmiStore.setState({ currentProject: { id: 'plant_a', name: 'Plant A', version: '1.0.0' } });
    expect(useHmiStore.getState().draftScreens.size).toBe(0);

    const reopened = await useHmiStore.getState().openExistingProject('plant_a');
    expect(reopened).toBe(true);

    const reloaded = useHmiStore.getState();
    const screen = reloaded.screens.find((s) => s.id === screenId);
    expect(screen?.canvasData).toEqual(canvas);
    expect(reloaded.savedScreens.get(screenId)).toEqual(canvas);
    expect(reloaded.draftScreens.size).toBe(0);

    const html = renderClient(<HmiScreenCanvasPanel {...panelProps({ screenId, title: 'Boiler Mimic' })} />);
    expect(html).toContain('data-testid="isa101-populated-canvas"');
    expect(html).toContain('MTR-900');
    expect(html).toContain('VLV-900');
  });

  it('commits saved drafts to the active buffer and reports the committed payload', async () => {
    const screenId = 'screen_02_boiler_area';
    const canvas = { title: 'Committed Mimic', components: [{ id: 'pt_900', type: 'TRANSMITTER', tag: 'PT-900' }] };
    useHmiStore.getState().updateDraftCanvas(screenId, canvas);

    expect(await useHmiStore.getState().commitHmiDraft()).toBe(true);

    const state = useHmiStore.getState();
    expect(backend.calls.some((c) => c.method === 'POST' && c.url.endsWith('/hmi/commit'))).toBe(true);
    expect(backend.calls.some((c) => c.method === 'PUT' && c.url.endsWith(`/screens/${screenId}/draft`))).toBe(true);
    expect(state.activeScreens.get(screenId)).toEqual(canvas);
    expect(state.savedScreens.get(screenId)).toEqual(canvas);
    expect(state.draftScreens.size).toBe(0);
    expect(state.isHmiDirty).toBe(false);
    expect(state.screenSaveStatus).toBe('COMMITTED');
  });

  it('discard reverts to the last saved revision instead of the stale runtime buffer', async () => {
    const screenId = 'screen_02_boiler_area';
    const saved = { title: 'Saved Mimic', components: [{ id: 'mtr_901', type: 'MOTOR', tag: 'MTR-901' }] };
    const unsaved = { title: 'Unsaved Mimic', components: [{ id: 'mtr_902', type: 'MOTOR', tag: 'MTR-902' }] };

    useHmiStore.getState().updateDraftCanvas(screenId, saved);
    await useHmiStore.getState().saveScreenDraft(screenId);
    useHmiStore.getState().updateDraftCanvas(screenId, unsaved);
    expect(useHmiStore.getState().isHmiDirty).toBe(true);

    useHmiStore.getState().discardHmiDraft();

    const state = useHmiStore.getState();
    expect(state.draftScreens.get(screenId)).toEqual(saved);
    expect(state.isHmiDirty).toBe(false);
  });

  it('reports a failed save without claiming the canvas was stored', async () => {
    const screenId = 'screen_02_boiler_area';
    const baseline = useHmiStore.getState().savedScreens.get(screenId);
    global.fetch = vi.fn().mockResolvedValue({ ok: false, status: 500, json: async () => ({}) });
    useHmiStore.getState().updateDraftCanvas(screenId, { title: 'x', components: [] });

    expect(await useHmiStore.getState().saveScreenDraft(screenId)).toBe(false);

    const state = useHmiStore.getState();
    expect(state.screenSaveStatus).toBe('ERROR');
    expect(state.errorMessage).toContain('Screen draft save failed (HTTP 500)');
    // The failed payload must not become the discard baseline.
    expect(state.savedScreens.get(screenId)).toEqual(baseline);
    expect(state.isHmiDirty).toBe(true);
  });

  it('disables persistence for sub-screen canvases the backend cannot store yet', () => {
    const html = renderClient(
      <HmiScreenCanvasPanel {...panelProps({ screenId: 'screen_02_boiler_area::sub_boiler_tuning', title: 'Popup' })} />
    );
    expect(html).toContain('SUB-SCREEN (NOT PERSISTED)');
    expect(html).toContain('data-testid="canvas-save-btn"');
    expect(html).toMatch(/data-testid="canvas-save-btn"[^>]*disabled/);
  });

  it('compares canvas payloads structurally', () => {
    expect(isCanvasEqual({ a: 1, b: [1, 2] }, { b: [1, 2], a: 1 })).toBe(true);
    expect(isCanvasEqual({ components: [{ id: 'a' }] }, { components: [{ id: 'b' }] })).toBe(false);
    expect(isCanvasEqual(undefined, undefined)).toBe(true);
    expect(isCanvasEqual({ a: undefined, b: 1 }, { b: 1 })).toBe(true);
    expect(isCanvasEqual(null, {})).toBe(false);
  });
});
