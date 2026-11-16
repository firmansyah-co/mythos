// The inspector's Properties for an element selected on a screen being edited.

import { useState } from 'react';
import { useController, useWorkbench } from './bridge';
import { elementKind } from './elementKinds';
import { viewOf } from './elementViews';
import { GRID, type KnownElement } from './screenModel';
import { useScreens } from './screens';

function NumberProperty({
  label,
  value,
  min,
  max,
  onCommit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onCommit: (value: number) => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const shown = text ?? String(value);
  const trimmed = shown.trim();
  const parsed = /^-?\d+$/.test(trimmed) ? Number(trimmed) : null;
  const error =
    text === null
      ? null
      : parsed === null
        ? 'Must be a whole number'
        : parsed < min || parsed > max
          ? `Must be between ${min} and ${max}`
          : null;

  return (
    <>
      <dt>{label}</dt>
      <dd>
        <input
          className="mono prop-input"
          inputMode="numeric"
          aria-label={label}
          aria-invalid={error !== null}
          value={shown}
          onChange={(event) => {
            const next = event.target.value;
            setText(next);
            const candidate = /^-?\d+$/.test(next.trim()) ? Number(next.trim()) : null;
            if (candidate !== null && candidate >= min && candidate <= max) onCommit(candidate);
          }}
          onBlur={() => setText(null)}
        />
        {error && (
          <span className="field__error" role="alert">
            {error}
          </span>
        )}
      </dd>
    </>
  );
}

export function ElementProperties({ screenId, elementId }: { screenId: string; elementId: string }) {
  const doc = useScreens((state) => state.screens.find((screen) => screen.id === screenId));
  const updateElement = useScreens((state) => state.updateElement);
  const removeElement = useScreens((state) => state.removeElement);
  const select = useWorkbench((state) => state.select);
  // The tags a field can be bound to are the ones the project's I/O configuration provides.
  const configured = useController((state) => state.configuredTags);

  const element = doc?.canvas.elements.find((candidate) => candidate.id === elementId);
  if (!doc || !element || element.type === 'unknown') {
    return <p className="inspector__empty muted">This element is no longer on the screen.</p>;
  }

  const update = (change: Partial<KnownElement>) => updateElement(screenId, elementId, change);
  const { Properties } = viewOf(element);

  return (
    <dl className="props" data-testid="element-properties">
      <dt>Element</dt>
      <dd>{elementKind(element.type)?.label ?? element.type}</dd>

      <Properties element={element} update={update} configuredTags={configured} />

      <NumberProperty label="X" value={element.x} min={0} max={doc.canvas.width} onCommit={(x) => update({ x })} />
      <NumberProperty label="Y" value={element.y} min={0} max={doc.canvas.height} onCommit={(y) => update({ y })} />
      <NumberProperty
        label="Width"
        value={element.width}
        min={GRID}
        max={doc.canvas.width}
        onCommit={(width) => update({ width })}
      />
      <NumberProperty
        label="Height"
        value={element.height}
        min={GRID}
        max={doc.canvas.height}
        onCommit={(height) => update({ height })}
      />
      <dt />
      <dd>
        <button
          type="button"
          className="btn"
          onClick={() => {
            removeElement(screenId, elementId);
            select(null);
          }}
        >
          Remove element
        </button>
      </dd>
    </dl>
  );
}
