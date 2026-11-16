// The HMI module's workbench UI (ADR-011 §2.3, step 4): the "HMI screens" section of the project
// tree, the screen editor with its toolbox and element properties, and the operator runtime.
// Built into `entry.js` (see vite.config.ts) and served by the controller beside the module.

import type { ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import type { Dispose, WorkbenchHost, WorkbenchModule } from '@mythos/workbench-sdk';
import { bind, useWorkbench, whenSelected } from './bridge';
import { ElementProperties } from './ElementProperties';
import { RuntimeView } from './RuntimeView';
import { ScreenEditor } from './ScreenEditor';
import { ScreensTree } from './ScreensTree';
import { useScreens } from './screens';
import { Toolbox } from './Toolbox';
// Extracted by the build into `hmi.css` beside `entry.js`, which `stylesheet()` links.
import './hmi.css';

export const MODULE_ID = 'org.mythos.hmi';

/** Draw `element` into `container` with this module's own React; return what takes it down. */
function draw(container: HTMLElement, element: ReactElement): Dispose {
  // Each root gets an element of its own: the workbench may mount again into the same container
  // before the previous root is gone, and two roots must never share one.
  const own = document.createElement('div');
  own.className = 'hmi-root';
  container.append(own);
  const root = createRoot(own);
  root.render(element);
  // Taken down after the workbench's own render has finished: the workbench removes the mount
  // while it renders, and a root cannot be unmounted synchronously from inside a render.
  return () =>
    queueMicrotask(() => {
      root.unmount();
      own.remove();
    });
}

/** The stylesheet built beside `entry.js`, while the module is active. */
function stylesheet(): Dispose {
  // Tests draw without stylesheets: their DOM loads no CSS.
  if (typeof document === 'undefined' || import.meta.env.MODE === 'test') return () => undefined;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./hmi.css', import.meta.url).href;
  link.dataset.module = MODULE_ID;
  document.head.append(link);
  return () => link.remove();
}

export function activate(host: WorkbenchHost): Dispose {
  const stops: Dispose[] = [stylesheet(), bind(host)];

  // The open project's screens are read when it opens, and dropped when it closes.
  const load = (projectId: string | null) => {
    if (projectId === null) useScreens.getState().reset();
    else void useScreens.getState().load(projectId);
  };
  load(host.project()?.id ?? null);
  stops.push(host.onProject((project) => load(project?.id ?? null)));

  // A screen renamed or removed changes the titles of the editors open on it.
  stops.push(
    useScreens.subscribe((state, previous) => {
      if (state.screens !== previous.screens) host.titlesChanged();
    })
  );

  // An element selected on a screen shows its properties in the inspector.
  whenSelected((selection) =>
    host.setInspector(
      'hmi-element',
      selection
        ? {
            title: 'Element',
            mount: (container) =>
              draw(
                container,
                <ElementProperties
                  key={`${selection.screenId}:${selection.elementId}`}
                  screenId={selection.screenId}
                  elementId={selection.elementId}
                />
              ),
          }
        : null
    )
  );
  stops.push(() => {
    whenSelected(() => undefined);
    host.setInspector('hmi-element', null);
  });

  stops.push(
    host.register.treeSection({
      id: 'hmi-screens',
      mount: (container) =>
        draw(container, <ScreensTree openEditor={(kind, id) => host.openEditor(kind, id)} />),
    })
  );

  stops.push(
    host.register.editor({
      kind: 'screen',
      live: true,
      title: (screenId) => [
        'HMI screens',
        useScreens.getState().screens.find((screen) => screen.id === screenId)?.name ?? 'removed',
      ],
      mount: (container, { target }) => {
        const screenId = target ?? '';
        host.setTaskCard('hmi-toolbox', {
          title: 'Toolbox',
          mount: (card) => draw(card, <Toolbox screenId={screenId} />),
        });
        const undraw = draw(container, <ScreenEditor screenId={screenId} />);
        return () => {
          undraw();
          host.setTaskCard('hmi-toolbox', null);
          const { selection, select } = useWorkbench.getState();
          if (selection?.screenId === screenId) select(null);
        };
      },
    })
  );

  stops.push(
    host.register.view({
      id: 'runtime',
      launch: 'Start runtime',
      mount: (container, { target }) => {
        useWorkbench.setState({ runtimeScreenId: target ?? null });
        return draw(container, <RuntimeView />);
      },
    })
  );

  return () => {
    for (const stop of stops.reverse()) stop();
    useScreens.getState().reset();
  };
}

const hmi: WorkbenchModule = { id: MODULE_ID, activate };
export default hmi;
