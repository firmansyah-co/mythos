import { describe, expect, it } from 'vitest';
import {
  DEFAULT_CANVAS_HEIGHT,
  DEFAULT_CANVAS_WIDTH,
  GRID,
  newIoField,
  newTextElement,
  parseCanvas,
  place,
  serializeCanvas,
} from '../data/screenModel';

const TEXT = { id: 't1', type: 'text', x: 8, y: 16, width: 160, height: 32, text: 'Boiler 1' };
const FIELD = {
  id: 'f1',
  type: 'io_field',
  x: 40,
  y: 80,
  width: 200,
  height: 56,
  tag: 'external.BoilerHouse.Unit1.steam_temp',
  label: 'Steam',
  unit: '°C',
};

describe('reading a stored canvas', () => {
  it('reads the elements it knows', () => {
    const canvas = parseCanvas({ width: 800, height: 480, components: [TEXT, FIELD] });
    expect(canvas.width).toBe(800);
    expect(canvas.elements).toEqual([TEXT, FIELD]);
  });

  it.each([
    ['a component from another tool', { id: 'm1', type: 'motor', props: { speed: 1450 } }],
    ['a component with no type', { id: 'x' }],
    ['a text element with no text', { ...TEXT, text: undefined }],
    ['an I/O field with no tag', { ...FIELD, tag: undefined }],
    ['an element with a non-numeric position', { ...TEXT, x: '8' }],
    ['an element with no size', { ...TEXT, width: 0 }],
    ['an element with an infinite coordinate', { ...TEXT, y: Number.POSITIVE_INFINITY }],
    ['something that is not an object', 'LABEL'],
    ['null', null],
  ])('does not draw %s from a guess: it is kept exactly as stored', (_what, component) => {
    const canvas = parseCanvas({ components: [TEXT, component, FIELD] });

    expect(canvas.elements.map((e) => e.type)).toEqual(['text', 'unknown', 'io_field']);
    const unknown = canvas.elements[1];
    expect(unknown?.type === 'unknown' && unknown.raw).toBe(component);

    // Saved back, it is where it was and what it was.
    expect(serializeCanvas(canvas).components).toEqual([TEXT, component, FIELD]);
  });

  it('keeps every other key of the canvas', () => {
    const stored = { title: 'Overview', theme: { dark: true }, width: 800, height: 480, components: [] };
    expect(serializeCanvas(parseCanvas(stored))).toEqual(stored);
  });

  it('gives a canvas with no stated size the standard one', () => {
    const canvas = parseCanvas({ title: 'Old', components: [] });
    expect([canvas.width, canvas.height]).toEqual([DEFAULT_CANVAS_WIDTH, DEFAULT_CANVAS_HEIGHT]);
  });

  it.each([undefined, null, 42, 'canvas', [], { components: 'none' }])(
    'reads %o as an empty canvas instead of failing',
    (raw) => {
      expect(parseCanvas(raw).elements).toEqual([]);
    }
  );

  it('names what an unknown component says it is', () => {
    const [motor, untyped] = parseCanvas({ components: [{ id: 'm', type: 'motor' }, { id: 'u' }] }).elements;
    expect(motor?.type === 'unknown' && motor.declaredType).toBe('motor');
    expect(untyped?.type === 'unknown' && untyped.declaredType).toBe('untyped');
  });
});

describe('placing elements', () => {
  const canvas = { width: 400, height: 240 };

  it('keeps a box on the grid', () => {
    const placed = place({ id: 'a', x: 13, y: 21, width: 101, height: 37 }, canvas);
    for (const value of [placed.x, placed.y, placed.width, placed.height]) {
      expect(value % GRID).toBe(0);
    }
  });

  it.each([
    [{ x: -50, y: -50, width: 80, height: 40 }],
    [{ x: 9999, y: 9999, width: 80, height: 40 }],
    [{ x: 0, y: 0, width: 9999, height: 9999 }],
    [{ x: 390, y: 230, width: 80, height: 40 }],
  ])('keeps %o inside the canvas', (box) => {
    const placed = place({ id: 'a', ...box }, canvas);
    expect(placed.x).toBeGreaterThanOrEqual(0);
    expect(placed.y).toBeGreaterThanOrEqual(0);
    expect(placed.x + placed.width).toBeLessThanOrEqual(canvas.width);
    expect(placed.y + placed.height).toBeLessThanOrEqual(canvas.height);
    expect(placed.width).toBeGreaterThanOrEqual(GRID);
  });
});

describe('new elements', () => {
  it('a new I/O field is bound to nothing', () => {
    const field = newIoField(parseCanvas({ components: [] }));
    expect(field).toMatchObject({ type: 'io_field', tag: '', label: '', unit: '' });
  });

  it('new elements get distinct ids and do not land exactly on each other', () => {
    let canvas = parseCanvas({ components: [] });
    for (let i = 0; i < 5; i++) {
      canvas = { ...canvas, elements: [...canvas.elements, i % 2 ? newIoField(canvas) : newTextElement(canvas)] };
    }
    expect(new Set(canvas.elements.map((e) => e.id)).size).toBe(5);
    const positions = canvas.elements.map((e) => (e.type === 'unknown' ? '' : `${e.x},${e.y}`));
    expect(new Set(positions).size).toBe(5);
  });
});
