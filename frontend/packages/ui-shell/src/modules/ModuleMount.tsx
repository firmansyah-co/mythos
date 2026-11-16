import { useEffect, useRef } from 'react';
import type { Mount } from '@mythos/workbench-sdk';

/**
 * Where a module draws. The mount is called when this appears and again whenever `mount` or
 * `deps` change; what it returns is called before that and when this goes away. A mount that
 * throws is reported in place, not allowed to take the workbench down.
 */
export function ModuleMount<Context>({
  mount,
  context,
  deps,
  testId,
  className,
}: {
  mount: Mount<Context>;
  context: Context;
  /** What `context` depends on: the mount is redone when one of them changes. */
  deps: readonly unknown[];
  testId?: string;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const contextRef = useRef(context);
  contextRef.current = context;

  useEffect(() => {
    const container = ref.current;
    if (!container) return;
    let dispose: (() => void) | void;
    try {
      dispose = mount(container, contextRef.current);
    } catch (cause) {
      container.textContent = `This module failed to draw: ${
        cause instanceof Error ? cause.message : String(cause)
      }`;
      container.setAttribute('role', 'alert');
    }
    return () => {
      try {
        if (typeof dispose === 'function') dispose();
      } finally {
        container.replaceChildren();
        container.removeAttribute('role');
      }
    };
  }, [mount, ...deps]);

  return <div ref={ref} className={className} data-testid={testId} />;
}
