// The workbench loads the UIs of enabled modules and draws what they register (ADR-011 §2.3).

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApiRefusal, WorkbenchHost, WorkbenchModule } from '@mythos/workbench-sdk';
import type { ModuleList } from '../api/client';
import { useLog } from '../data/log';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { loadModuleUis, unloadModuleUis, type Importer } from '../modules/loader';
import { ModuleViewHost } from '../modules/ModuleViewHost';
import { useModuleUi } from '../modules/registry';
import { Inspector } from '../shell/Inspector';
import { ProjectTree } from '../shell/ProjectTree';
import { ModuleTaskCards } from '../shell/TaskCards';
import { PortalView } from '../shell/PortalView';
import { WorkArea } from '../shell/WorkArea';
import { MODULES, project, render, resetStores, sample, unmountAll } from './harness';

const row = (id: string, ui: string | null, enabled = true) => ({
  id,
  name: id,
  version: '1.0.0',
  kind: 'service' as const,
  provides: id.split('.').pop() ?? id,
  description: '',
  path: `/opt/mythos/bin/mythos-module-${id}`,
  enabled,
  removable: false,
  ui,
});

/** A module's refusal with its reasons, as the data module answers a save that does not check. */
const REFUSAL = {
  error: "with this source the project's types and data blocks do not check, and nothing was saved",
  diagnostics: [{ line: 2, col: 3, end_line: 2, end_col: 8, message: 'Nope is not a type', source: 'data/types/Recipe.st' }],
};

function controller(list: ModuleList) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input);
      if (path === '/api/v1/modules') return new Response(JSON.stringify(list), { status: 200 });
      if (path === '/api/v1/projects/plant/notes/notes') {
        return new Response(JSON.stringify({ notes: ['pump 1'] }), { status: 200 });
      }
      if (path === '/api/v1/projects/plant/notes/empty') return new Response('', { status: 204 });
      if (path === '/api/v1/projects/plant/notes/refused') {
        return new Response(JSON.stringify(REFUSAL), { status: 400, statusText: 'Bad Request' });
      }
      if (path === '/api/v1/projects/plant/notes/garbled') {
        return new Response('<html>proxy error</html>', { status: 502, statusText: 'Bad Gateway' });
      }
      return new Response(JSON.stringify({ error: 'no such note' }), { status: 404 });
    })
  );
}

/** A module UI written as a module's `entry.js` would be, with plain DOM. */
function notesModule(record: { host?: WorkbenchHost; disposed: string[] }): WorkbenchModule {
  return {
    id: 'org.example.notes',
    activate(host) {
      record.host = host;
      host.register.treeSection({
        id: 'notes',
        mount(container, { project }) {
          const node = document.createElement('div');
          node.className = 'tree__node';
          node.dataset.testid = 'tree-notes';
          node.textContent = `Notes of ${project.name}`;
          node.ondblclick = () => host.openEditor('note', 'pump 1');
          container.append(node);
          return () => record.disposed.push('tree');
        },
      });
      host.register.editor({
        kind: 'note',
        title: (target) => ['Notes', target ?? '?'],
        live: true,
        mount(container, { target }) {
          const text = document.createElement('p');
          text.dataset.testid = 'note-text';
          const draw = () => {
            const value = host.telemetry.get('external.A.B.level')?.value;
            text.textContent = `${target}: ${value === undefined ? 'NOT MEASURED' : String(value)}`;
          };
          draw();
          const stop = host.telemetry.subscribe(draw);
          container.append(text);
          host.setTaskCard('note-tools', {
            title: 'Note tools',
            mount: (card) => {
              card.textContent = `tools for ${target}`;
            },
          });
          host.setInspector('note-props', {
            title: 'Note',
            mount: (panel) => {
              panel.textContent = `properties of ${target}`;
            },
          });
          return () => {
            stop();
            host.setTaskCard('note-tools', null);
            host.setInspector('note-props', null);
            record.disposed.push('editor');
          };
        },
      });
      host.register.view({
        id: 'board',
        mount(container, { target }) {
          container.textContent = `board ${target ?? 'all'}`;
        },
      });
      return () => record.disposed.push('module');
    },
  };
}

function importer(modules: Record<string, unknown>): Importer {
  return async (url) => {
    if (!(url in modules)) throw new Error(`Failed to fetch dynamically imported module: ${url}`);
    const loaded = modules[url];
    if (loaded instanceof Error) throw loaded;
    return { default: loaded };
  };
}

beforeEach(() => {
  resetStores();
  useWorkbench.getState().openProject(project('plant', 'Plant A'));
});

afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('loading the modules’ workbench UIs', () => {
  it('imports the entry of each module that has a UI, and only those', async () => {
    controller({
      ...MODULES,
      modules: [
        row('org.example.notes', '/modules/org.example.notes/ui/entry.js'),
        row('org.mythos.driver.modbus', null),
        // Disabled: the server gives it no `ui`, so the workbench never imports it.
        row('org.example.off', null, false),
      ],
    });
    const imported: string[] = [];
    const record = { disposed: [] as string[] };
    await loadModuleUis(async (url) => {
      imported.push(url);
      return { default: notesModule(record) };
    });

    expect(imported).toEqual(['/modules/org.example.notes/ui/entry.js']);
    expect(useModuleUi.getState().phase).toBe('ready');
    expect(useModuleUi.getState().loads).toEqual([
      { id: 'org.example.notes', url: '/modules/org.example.notes/ui/entry.js', state: 'active' },
    ]);
  });

  it('reports a module whose UI does not load, and loads the others regardless', async () => {
    controller({
      ...MODULES,
      modules: [
        row('org.example.notes', '/n.js'),
        row('org.example.gone', '/gone.js'),
        row('org.example.liar', '/liar.js'),
        row('org.example.throws', '/throws.js'),
      ],
    });
    const record = { disposed: [] as string[] };
    await loadModuleUis(
      importer({
        '/n.js': notesModule(record),
        '/liar.js': { ...notesModule(record), id: 'org.example.other' },
        '/throws.js': {
          id: 'org.example.throws',
          activate(host: WorkbenchHost) {
            host.register.view({ id: 'half', mount: () => undefined });
            throw new Error('boom');
          },
        },
      })
    );

    const loads = Object.fromEntries(useModuleUi.getState().loads.map((load) => [load.id, load]));
    expect(loads['org.example.notes']?.state).toBe('active');
    expect(loads['org.example.gone']).toMatchObject({ state: 'failed' });
    expect(loads['org.example.gone']?.error).toContain('/gone.js');
    expect(loads['org.example.liar']).toMatchObject({
      state: 'failed',
      error: "its entry says it is 'org.example.other'",
    });
    expect(loads['org.example.throws']).toMatchObject({ state: 'failed', error: 'boom' });
    // What a module registered before failing is taken back.
    expect(useModuleUi.getState().views.map((view) => view.item.id)).toEqual(['board']);
    const errors = useLog.getState().entries.filter((entry) => entry.level === 'error');
    expect(errors.map((entry) => entry.text)).toHaveLength(3);
  });

  it('says so when the module list cannot be read, instead of showing no modules as if there were none', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"down"}', { status: 500 })));
    await loadModuleUis(importer({}));
    expect(useModuleUi.getState().phase).toBe('ready');
    expect(useLog.getState().entries.at(-1)?.text).toContain('could not be listed');
  });

  it('refuses a second claim on a name, and the workbench’s own editors', async () => {
    controller({ ...MODULES, modules: [row('org.example.notes', '/n.js'), row('org.example.copy', '/c.js')] });
    const record: { host?: WorkbenchHost; disposed: string[] } = { disposed: [] };
    const errors: string[] = [];
    await loadModuleUis(
      importer({
        '/n.js': notesModule(record),
        '/c.js': {
          id: 'org.example.copy',
          activate(host: WorkbenchHost) {
            for (const attempt of [
              () => host.register.editor({ kind: 'tags', title: () => 'x', mount: () => undefined }),
              () => host.register.view({ id: 'project', mount: () => undefined }),
              () => host.register.treeSection({ id: 'own', mount: () => undefined }),
            ]) {
              try {
                attempt();
              } catch (cause) {
                errors.push((cause as Error).message);
              }
            }
          },
        },
      })
    );
    expect(errors).toEqual([
      "module 'org.example.copy' cannot register the workbench's own editor 'tags'",
      "module 'org.example.copy' cannot register the view 'project'",
    ]);
    // Order of loading is not fixed, so a duplicate is checked directly.
    expect(() =>
      record.host?.register.treeSection({ id: 'own', mount: () => undefined })
    ).toThrow("module 'org.example.notes' cannot register 'own': module 'org.example.copy' already did");
  });
});

describe('what a module registered, in the workbench', () => {
  async function loadNotes() {
    controller({ ...MODULES, modules: [row('org.example.notes', '/n.js')] });
    const record: { host?: WorkbenchHost; disposed: string[] } = { disposed: [] };
    await loadModuleUis(importer({ '/n.js': notesModule(record) }));
    return record;
  }

  it('draws its tree section, opens its editor with live values, and shows its panels', async () => {
    await loadNotes();
    const tree = render(<ProjectTree />);
    const area = render(<WorkArea />);
    const cards = render(<ModuleTaskCards />);
    const inspector = render(<Inspector collapsed={false} />);

    const node = tree.querySelector<HTMLElement>('[data-testid="tree-notes"]');
    expect(node?.textContent).toBe('Notes of Plant A');
    act(() => node?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })));

    expect(area.querySelector('.workarea__path')?.textContent).toBe('Plant ANotespump 1');
    // Offline, the module says it has no value, and the header does not say ONLINE.
    expect(area.querySelector('[data-testid="note-text"]')?.textContent).toBe('pump 1: NOT MEASURED');
    expect(area.querySelector('.workarea__online')).toBeNull();

    act(() => {
      useTelemetry.setState({ stream: 'OPEN', tags: new Map([['external.A.B.level', sample('external.A.B.level', { value: 42 })]]) });
    });
    expect(area.querySelector('[data-testid="note-text"]')?.textContent).toBe('pump 1: 42');
    expect(area.querySelector('.workarea__online')?.textContent).toBe('ONLINE');

    expect(cards.querySelector('[data-testid="module-task-card"]')?.textContent).toBe('Note toolstools for pump 1');
    expect(inspector.querySelector('[data-testid="inspector-module"]')?.textContent).toBe('properties of pump 1');

    // Selecting something of the workbench's own hides the module's panel.
    act(() => useWorkbench.getState().select({ kind: 'project' }));
    expect(inspector.querySelector('[data-testid="inspector-module"]')).toBeNull();
  });

  it('takes its panels away when its editor closes, and everything when it is unloaded', async () => {
    const record = await loadNotes();
    const tree = render(<ProjectTree />);
    const area = render(<WorkArea />);
    const cards = render(<ModuleTaskCards />);
    act(() => useWorkbench.getState().openEditor('note', 'pump 1'));
    expect(cards.querySelector('[data-testid="module-task-card"]')).not.toBeNull();

    act(() => useWorkbench.getState().closeEditor('note:pump 1'));
    expect(record.disposed).toEqual(['editor']);
    expect(cards.querySelector('[data-testid="module-task-card"]')).toBeNull();

    act(() => useWorkbench.getState().openEditor('note', 'pump 1'));
    act(() => unloadModuleUis());
    // Each of them, once: the module's own undo, and every mount it had on screen.
    expect([...record.disposed].sort()).toEqual(['editor', 'editor', 'module', 'tree']);
    expect(tree.querySelector('[data-testid="tree-notes"]')).toBeNull();
    expect(area.querySelector('[data-testid="module-editor-missing"]')?.textContent).toBe(
      'The modules are loading.'
    );
  });

  it('shows what a module diagnoses in the Diagnostics tab, counted there, and brings the tab forward when asked', async () => {
    const record = await loadNotes();
    const host = record.host as WorkbenchHost;
    const inspector = render(<Inspector collapsed={false} />);
    let mounts = 0;
    const mount = (panel: HTMLElement) => {
      mounts += 1;
      panel.textContent = 'note 3 is empty';
    };
    const count = () => inspector.querySelector('[data-testid="problem-count"]')?.textContent ?? null;

    // Not known is not counted: nothing is claimed either way.
    act(() => host.setDiagnostics('note-check', { title: 'Check', mount, count: null }));
    expect(count()).toBeNull();
    act(() => host.setDiagnostics('note-check', { title: 'Check', mount, count: 2 }));
    expect(count()).toBe('2');

    act(() => host.showDiagnostics());
    expect(useWorkbench.getState().inspectorTab).toBe('diagnostics');
    expect(inspector.querySelector('[data-testid="module-diagnostics"]')?.textContent).toBe('Checknote 3 is empty');
    // Only the count changes when the same mount is set again: nothing is drawn twice.
    act(() => host.setDiagnostics('note-check', { title: 'Check', mount, count: 1 }));
    expect([count(), mounts]).toEqual(['1', 1]);

    // A collapsed inspector is opened to show them.
    act(() => useWorkbench.getState().togglePane('inspector'));
    expect(useWorkbench.getState().panes.inspector).toBe(false);
    act(() => host.showDiagnostics());
    expect(useWorkbench.getState().panes.inspector).toBe(true);

    act(() => host.setDiagnostics('note-check', null));
    expect(count()).toBeNull();
    expect(inspector.querySelector('[data-testid="module-diagnostics"]')).toBeNull();

    // Unloading the module takes away what it left there.
    act(() => host.setDiagnostics('note-check', { title: 'Check', mount, count: 3 }));
    act(() => unloadModuleUis());
    expect(useModuleUi.getState().diagnostics).toEqual({});
  });

  it('says plainly when no enabled module has an editor that is open', async () => {
    controller({ ...MODULES, modules: [] });
    await loadModuleUis(importer({}));
    const area = render(<WorkArea />);
    act(() => useWorkbench.getState().openEditor('note', 'pump 1'));
    expect(area.querySelector('[data-testid="module-editor-missing"]')?.textContent).toBe(
      'No enabled module provides the editor “note”.'
    );
    expect(area.querySelector('.workarea__path')?.textContent).toBe('Plant Anote');
  });

  it('shows its full-window view, and says when no module has the view asked for', async () => {
    await loadNotes();
    const view = render(<ModuleViewHost />);
    act(() => useWorkbench.getState().showModuleView('board', 'east'));
    expect(view.querySelector('[data-testid="module-view-board"]')?.textContent).toBe('board east');
    act(() => useWorkbench.getState().showModuleView('trend'));
    expect(view.querySelector('[data-testid="module-view-missing"]')?.textContent).toContain(
      'No enabled module provides the view “trend”.'
    );
  });

  it('gives it the controller as the signed-in user, with errors and empty answers as they are', async () => {
    const record = await loadNotes();
    const host = record.host!;
    await expect(host.api('/api/v1/projects/plant/notes/notes')).resolves.toEqual({ notes: ['pump 1'] });
    await expect(host.api('/api/v1/projects/plant/notes/empty')).resolves.toBeNull();
    await expect(host.api('/api/v1/projects/plant/notes/missing')).rejects.toThrow('404');
    // A refusal reaches the module whole: its status and its body beside the same message.
    const refused = await host.api('/api/v1/projects/plant/notes/refused').then(
      () => null,
      (cause: unknown) => cause as ApiRefusal
    );
    expect(refused).toBeInstanceOf(Error);
    expect(refused?.message).toBe(`400 Bad Request: ${REFUSAL.error}`);
    expect(refused?.status).toBe(400);
    expect(refused?.body).toEqual(REFUSAL);
    const garbled = await host.api('/api/v1/projects/plant/notes/garbled').then(
      () => null,
      (cause: unknown) => cause as ApiRefusal
    );
    expect(garbled?.message).toBe('502 Bad Gateway');
    expect(garbled?.status).toBe(502);
    expect(garbled?.body).toBeNull();
    expect(host.project()).toEqual({ id: 'plant', name: 'Plant A' });
  });
});

describe('a module’s page in the Portal', () => {
  it('is listed in the Portal’s navigation and drawn when chosen; the Portal’s own pages cannot be taken', async () => {
    controller({ ...MODULES, modules: [row('org.example.net', '/net.js')] });
    const refused: string[] = [];
    await loadModuleUis(
      importer({
        '/net.js': {
          id: 'org.example.net',
          activate(host: WorkbenchHost) {
            try {
              host.register.portalSection({ id: 'open', label: 'Mine', mount: () => undefined });
            } catch (cause) {
              refused.push((cause as Error).message);
            }
            host.register.portalSection({
              id: 'network',
              label: 'MythOS on this network',
              mount: (container) => {
                container.textContent = 'two nodes heard';
              },
            });
          },
        },
      })
    );
    expect(refused).toEqual(["module 'org.example.net' cannot register the Portal's own 'open'"]);
    act(() => useWorkbench.getState().closeProject());
    const portal = render(<PortalView />);
    const entry = portal.querySelector<HTMLButtonElement>('[data-testid="portal-module-network"]');
    expect(entry?.textContent?.trim()).toBe('MythOS on this network');
    act(() => entry?.click());
    expect(portal.querySelector('[data-testid="portal-section-network"]')?.textContent).toBe('two nodes heard');

    act(() => unloadModuleUis());
    expect(portal.querySelector('[data-testid="portal-module-network"]')).toBeNull();
  });
});
