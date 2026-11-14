import type { ReactNode } from 'react';
import type { IoStatus } from '../api/client';
import { useRuntimeScope } from '../data/scope';

/**
 * Renders runtime content only for a project the controller is running, and otherwise says
 * plainly why there is nothing to show.
 */
export function RuntimeGate({ children }: { children: (io: IoStatus) => ReactNode }) {
  const scope = useRuntimeScope();

  switch (scope.kind) {
    case 'active':
      return <>{children(scope.io)}</>;
    case 'unknown':
      return <p className="editor__loading muted">Reading the controller…</p>;
    case 'unreachable':
      return (
        <div className="notice notice--bad" role="alert" data-testid="gate-unreachable">
          <div>
            <h3>The controller cannot be reached</h3>
            <p>{scope.error}</p>
            <p className="muted">Nothing is known about its field devices until it answers.</p>
          </div>
        </div>
      );
    case 'not-active':
      return (
        <div className="notice notice--warn" data-testid="gate-not-active">
          <div>
            <h3>This project is not running on the controller</h3>
            <p>
              {scope.activeProjectId
                ? `The controller is running “${scope.activeProjectId}”. Its devices and tags are not shown under this project.`
                : 'The controller is running no project, so it reads no field devices.'}
            </p>
          </div>
        </div>
      );
  }
}
