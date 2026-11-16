// How each kind of screen element looks, and how it is edited. The kinds themselves are listed
// in `data/elementKinds`; the mapped type below makes a kind without a view a type error.

import { Gauge, Type, type LucideIcon } from 'lucide-react';
import type { ComponentType } from 'react';
import type { IoFieldElement, KnownElement, TextElement } from './screenModel';
import { IoField } from './IoField';

export interface PropertiesProps<E extends KnownElement> {
  element: E;
  update: (change: Partial<E>) => void;
  /** The tags the project's I/O configuration provides, sorted. */
  configuredTags: string[];
}

export interface ElementView<E extends KnownElement> {
  icon: LucideIcon;
  /** How it is drawn, in the editor and in the operator runtime alike. */
  Body: ComponentType<{ element: E }>;
  /** Its own rows in the inspector. Position and size are common and drawn by the inspector. */
  Properties: ComponentType<PropertiesProps<E>>;
}

type Views = { [K in KnownElement['type']]: ElementView<Extract<KnownElement, { type: K }>> };

function TextProperties({ element, update }: PropertiesProps<TextElement>) {
  return (
    <>
      <dt>Text</dt>
      <dd>
        <input
          className="prop-input prop-input--wide"
          aria-label="Text"
          value={element.text}
          onChange={(event) => update({ text: event.target.value })}
        />
      </dd>
    </>
  );
}

function IoFieldProperties({ element, update, configuredTags }: PropertiesProps<IoFieldElement>) {
  const missing = element.tag !== '' && !configuredTags.includes(element.tag);
  return (
    <>
      <dt>Tag</dt>
      <dd>
        <select
          className="prop-input prop-input--wide mono"
          aria-label="Tag"
          value={element.tag}
          onChange={(event) => update({ tag: event.target.value })}
        >
          <option value="">(not bound)</option>
          {configuredTags.map((uri) => (
            <option key={uri} value={uri}>
              {uri}
            </option>
          ))}
          {missing && <option value={element.tag}>{element.tag} — not in the I/O configuration</option>}
        </select>
        {missing && (
          <span className="field__error" role="alert" data-testid="tag-not-configured">
            No connection of this project provides this tag.
          </span>
        )}
      </dd>
      <dt>Label</dt>
      <dd>
        <input
          className="prop-input prop-input--wide"
          aria-label="Label"
          placeholder="Shown above the value. Empty shows the tag name."
          value={element.label}
          onChange={(event) => update({ label: event.target.value })}
        />
      </dd>
      <dt>Unit</dt>
      <dd>
        <input
          className="prop-input"
          aria-label="Unit"
          value={element.unit}
          onChange={(event) => update({ unit: event.target.value })}
        />
      </dd>
    </>
  );
}

export const ELEMENT_VIEWS: Views = {
  text: {
    icon: Type,
    Body: ({ element }) => <div className="textfield">{element.text}</div>,
    Properties: TextProperties,
  },
  io_field: { icon: Gauge, Body: IoField, Properties: IoFieldProperties },
};

/** The view of `element`'s kind. */
export function viewOf<E extends KnownElement>(element: E): ElementView<E> {
  return ELEMENT_VIEWS[element.type] as unknown as ElementView<E>;
}
