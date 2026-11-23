// Drawing into what the workbench hands the module: an editor's area, a task card, the inspector.

import type { ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import type { Dispose } from '@mythos/workbench-sdk';

/** Draw `element` into `container` with this module's own React; return what takes it down. */
export function draw(container: HTMLElement, element: ReactElement): Dispose {
  // Each root gets an element of its own: the workbench may mount again into the same container
  // before the previous root is gone, and two roots must never share one.
  const own = document.createElement('div');
  own.className = 'data-root';
  container.append(own);
  const root = createRoot(own);
  root.render(element);
  // Taken down after the workbench's own render has finished: a root cannot be unmounted
  // synchronously from inside a render.
  return () =>
    queueMicrotask(() => {
      root.unmount();
      own.remove();
    });
}
