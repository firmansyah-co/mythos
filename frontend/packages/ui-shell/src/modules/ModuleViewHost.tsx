import { useWorkbench } from '../data/workbench';
import { ModuleMount } from './ModuleMount';
import { moduleView, useModuleUi } from './registry';

/** A module's full-window view (an operator runtime), or why it cannot be shown. */
export function ModuleViewHost() {
  const shown = useWorkbench((state) => state.moduleView);
  const project = useWorkbench((state) => state.project);
  const exit = useWorkbench((state) => state.exitModuleView);
  const entry = useModuleUi((state) => (shown ? moduleView(state, shown.id) : undefined));
  const phase = useModuleUi((state) => state.phase);

  if (!shown || !project || !entry) {
    return (
      <main className="module-view module-view--missing" data-testid="module-view-missing">
        <p className="muted">
          {shown && phase === 'ready'
            ? `No enabled module provides the view “${shown.id}”.`
            : 'The modules are loading.'}
        </p>
        <button type="button" className="btn" onClick={exit}>
          Back to the project
        </button>
      </main>
    );
  }
  return (
    <ModuleMount
      mount={entry.item.mount}
      context={{ project: { id: project.id, name: project.name }, target: shown.target }}
      deps={[project.id, shown.id, shown.target]}
      className="module-view"
      testId={`module-view-${shown.id}`}
    />
  );
}
