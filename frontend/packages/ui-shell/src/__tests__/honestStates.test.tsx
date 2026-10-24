import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { useHmiStore } from '../stores/useHmiStore';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from '../components/workbench/Zone5BottomConsole';
import { PortalView } from '../components/portal/PortalView';
import { HmiScreenCanvasPanel } from '../components/workbench/DockviewWorkspace';
import { readFileSync } from 'node:fs';
import path from 'node:path';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

/** Reject the way `fetch` does when the daemon is not listening. */
async function unreachable(): Promise<never> {
  throw new TypeError('Failed to fetch');
}

describe('Honest states: no fabricated operational data or silent failures', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
    vi.restoreAllMocks();
  });

  it('reports an unreachable backend by name, not a bare fetch error', async () => {
    global.fetch = vi.fn().mockImplementation(unreachable);

    const created = await useHmiStore.getState().createNewProject('Unreachable_Project');
    expect(created).toBeNull();
    expect(useHmiStore.getState().currentProject).toBeNull();
    expect(useHmiStore.getState().errorMessage).toMatch(/Project creation failed: backend unreachable/);

    const screenId = await useHmiStore.getState().addNewScreen('Unreachable_Screen');
    expect(screenId).toBeNull();
    expect(useHmiStore.getState().screens).toHaveLength(0);

    const opened = await useHmiStore.getState().openExistingProject('any_project');
    expect(opened).toBe(false);
    expect(useHmiStore.getState().errorMessage).toMatch(/backend unreachable/);
  });

  it('never inserts a project or screen the backend did not accept', async () => {
    global.fetch = vi.fn().mockResolvedValue({ ok: false, status: 503, json: async () => ({}) });

    expect(await useHmiStore.getState().createNewProject('Rejected_Project')).toBeNull();
    expect(useHmiStore.getState().currentProject).toBeNull();
    expect(await useHmiStore.getState().addNewScreen('Rejected_Screen')).toBeNull();
    expect(useHmiStore.getState().screens).toHaveLength(0);
  });

  it('gives the portal a retry action when project creation fails', async () => {
    global.fetch = vi.fn().mockImplementation(unreachable);

    renderClient(<PortalView />);
    const form = container.querySelector('form');
    expect(form).not.toBeNull();

    // Submitting with the backend down must leave the form in place and offer a retry.
    flushSync(() => {
      form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    });
    await vi.waitFor(() => {
      expect(container.querySelector('[data-testid="portal-create-error"]')).not.toBeNull();
    });
    const alert = container.querySelector('[data-testid="portal-create-error"]')!.textContent ?? '';
    expect(alert).toContain('backend unreachable');
    expect(alert).toContain('Retry');
    // Nothing fictional reached the workspace.
    expect(useHmiStore.getState().currentProject).toBeNull();
    expect(container.querySelector('[data-testid="btn-create-project"]')).not.toBeNull();
  });

  it('does not show invented interlocks, alarms, console output or cycle times', () => {
    const inspectors = renderClient(<Zone4TaskCards isCollapsed={false} onToggleCollapse={() => {}} />);
    expect(inspectors).toContain('Interlock Matrix Checklist');
    expect(inspectors).toContain('No interlock telemetry');
    expect(inspectors).not.toContain('1.1 bar');

    const consoleHtml = renderClient(<Zone5BottomConsole isCollapsed={false} onToggleCollapse={() => {}} />);
    expect(consoleHtml).toContain('ISA-18.2 Alarms (0 Active)');
    expect(consoleHtml).toContain('No alarms are published by the backend');
    expect(consoleHtml).not.toContain('Boiler 02');
    expect(consoleHtml).not.toContain('410µs');
    expect(consoleHtml).not.toContain('libstrucpp_program_v1.so');
  });

  it('does not assert TagBus health it never measured', () => {
    // There is no supervisor telemetry transport in the shell, so the canvas footer cannot
    // know TagBus state. It must say so rather than render a green "ACTIVE".
    const html = renderClient(<HmiScreenCanvasPanel params={{}} /> as any);

    expect(html).toContain('TagBus Status');
    expect(html).toContain('UNKNOWN (NOT MEASURED)');
    expect(html).not.toContain('ACTIVE (100ms)');
    expect(html).not.toContain('ACTIVE');

    // The two neighbouring figures are measured from the store, so they stay.
    expect(html).toContain('Components');
  });

  it('has no hardcoded TagBus health literal left in the panel source', () => {
    // Second layer: a DOM test alone can stay green if a quality gate suppresses the value,
    // so guard the source too. This fails on a reintroduced "ACTIVE" announcement.
    // Vitest runs from the frontend workspace root (where vitest.config.ts lives).
    const source = readFileSync(
      path.resolve(process.cwd(), 'packages/ui-shell/src/components/workbench/DockviewWorkspace.tsx'),
      'utf-8'
    );
    expect(source).not.toContain('ACTIVE (100ms)');
    expect(source).not.toMatch(/TagBus Status:[^\n]*text-emerald/);
  });
});
