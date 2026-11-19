// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { ELEMENT_KINDS, elementKind } from '../elementKinds';
import { newElement, parseCanvas, serializeCanvas } from '../screenModel';
import { ELEMENT_VIEWS } from '../elementViews';
import { Toolbox } from '../Toolbox';
import { render, unmountAll } from './harness';

afterEach(() => unmountAll());

describe('the element kinds', () => {
  it('each kind has a view, and each view a kind', () => {
    expect(ELEMENT_KINDS.map((k) => k.type).sort()).toEqual(Object.keys(ELEMENT_VIEWS).sort());
  });

  it.each(ELEMENT_KINDS.map((k) => [k.type]))('a new %s is stored and read back as itself', (type) => {
    const canvas = parseCanvas({ components: [] });
    const element = newElement(canvas, type)!;
    expect(element.type).toBe(type);
    const stored = serializeCanvas({ ...canvas, elements: [element] });
    expect(parseCanvas(JSON.parse(JSON.stringify(stored))).elements).toEqual([element]);
  });

  it('a kind that does not exist places nothing', () => {
    expect(newElement(parseCanvas({}), 'trend')).toBeNull();
    expect(elementKind('trend')).toBeUndefined();
  });

  it('the toolbox offers every kind, once, by its name', () => {
    const container = render(<Toolbox screenId="s" />);
    const offered = [...container.querySelectorAll('.toolbox__item strong')].map((el) => el.textContent);
    expect(offered).toEqual(ELEMENT_KINDS.map((k) => k.label));
  });
});
