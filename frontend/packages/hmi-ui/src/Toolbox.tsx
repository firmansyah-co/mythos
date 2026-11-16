import { useWorkbench } from './bridge';
import { ELEMENT_KINDS } from './elementKinds';
import { ELEMENT_VIEWS } from './elementViews';
import { useScreens } from './screens';

/** The "Toolbox" task card's content: the elements that can be placed on the open screen. */
export function Toolbox({ screenId }: { screenId: string }) {
  const addElement = useScreens((state) => state.addElement);
  const select = useWorkbench((state) => state.select);

  const add = (type: string) => {
    const elementId = addElement(screenId, type);
    if (elementId) select({ kind: 'element', screenId, elementId });
  };

  return (
    <div data-testid="toolbox">
      <div className="toolbox">
        {ELEMENT_KINDS.map((kind) => {
          const Icon = ELEMENT_VIEWS[kind.type].icon;
          return (
            <button key={kind.type} type="button" className="toolbox__item" onClick={() => add(kind.type)}>
              <Icon size={16} aria-hidden />
              <span>
                <strong>{kind.label}</strong>
                <small>{kind.summary}</small>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
