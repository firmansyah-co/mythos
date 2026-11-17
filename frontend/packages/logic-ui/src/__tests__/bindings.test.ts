import { describe, expect, it } from 'vitest';
import {
  addBinding,
  bindingsOf,
  bindingsOfVariable,
  orphans,
  parseSafeNumber,
  problemOf,
  removeBinding,
  setSafe,
  setTag,
  setWritable,
} from '../bindings';
import type { ProgramEntry, Variable } from '../bridge';

type Lists = Pick<ProgramEntry, 'inputs' | 'outputs' | 'published'>;

const LISTS: Lists = {
  inputs: [
    { variable: 'level', tag: 'external.P.LT1.level' },
    { variable: 'level_ok', tag: 'external.P.LT1.level', of: 'good' },
  ],
  outputs: [{ variable: 'speed', tag: 'external.P.P1.speed', safe: 7 }],
  published: [{ variable: 'setpoint', tag: 'internal.P.Tank.setpoint', writable: true }],
};

const variable = (path: string, type = 'INT'): Variable => ({ path, type, section: 'input', bindable: true });

describe('the binding table', () => {
  it('has a row for every stored binding, each knowing its list and its place in it', () => {
    expect(bindingsOf(LISTS)).toEqual([
      { kind: 'input', index: 0, variable: 'level', tag: 'external.P.LT1.level' },
      { kind: 'good', index: 1, variable: 'level_ok', tag: 'external.P.LT1.level' },
      { kind: 'output', index: 0, variable: 'speed', tag: 'external.P.P1.speed', safe: 7 },
      { kind: 'published', index: 0, variable: 'setpoint', tag: 'internal.P.Tank.setpoint', writable: true },
    ]);
  });

  it('finds a variable\'s bindings whatever the case, and the bindings of no variable', () => {
    const rows = bindingsOf(LISTS);
    expect(bindingsOfVariable(rows, 'LEVEL').map((r) => r.kind)).toEqual(['input']);
    expect(orphans(rows, [variable('Level'), variable('speed')]).map((r) => r.variable)).toEqual(['level_ok', 'setpoint']);
  });

  it('gives a new output no safe value, and a stored one that is not a value none either', () => {
    const added = addBinding(LISTS, 'output', 'valve');
    expect(added.outputs[1]).toEqual({ variable: 'valve', tag: '', safe: null });
    const row = bindingsOf(added)[3];
    expect(row?.safe).toBeNull();
    expect(problemOf({ ...row!, tag: 'external.P.V1.open' })).toMatch(/safe value is required/);
    // What another client stored, which is neither a number nor a boolean, is not shown as 0.
    const odd = { ...LISTS, outputs: [{ variable: 'speed', tag: 't', safe: 'off' as unknown as number }] };
    expect(bindingsOf(odd)[2]?.safe).toBeNull();
  });

  it('reads an empty safe value as none, never as zero', () => {
    expect(parseSafeNumber('')).toBeNull();
    expect(parseSafeNumber('  ')).toBeNull();
    expect(parseSafeNumber('abc')).toBeNull();
    expect(parseSafeNumber('0')).toBe(0);
    expect(parseSafeNumber('-2.5')).toBe(-2.5);
  });

  it('changes one binding and leaves every other as it was stored', () => {
    const rows = bindingsOf(LISTS);
    const good = rows[1]!;
    const output = rows[2]!;
    const published = rows[3]!;
    expect(setTag(LISTS, good, 'external.P.LT2.level')).toEqual({
      ...LISTS,
      inputs: [LISTS.inputs[0], { variable: 'level_ok', tag: 'external.P.LT2.level', of: 'good' }],
    });
    expect(setSafe(LISTS, output, 0).outputs).toEqual([{ variable: 'speed', tag: 'external.P.P1.speed', safe: 0 }]);
    expect(setSafe(LISTS, good, 1)).toBe(LISTS);
    expect(setWritable(LISTS, published, false).published[0]?.writable).toBe(false);
    expect(setWritable(LISTS, output, true)).toBe(LISTS);
    expect(removeBinding(LISTS, good)).toEqual({ ...LISTS, inputs: [LISTS.inputs[0]] });
    expect(removeBinding(LISTS, output)).toEqual({ ...LISTS, outputs: [] });
  });

  it('says what a binding still lacks', () => {
    const [input, , output, published] = bindingsOf(LISTS);
    expect(problemOf(input!)).toBeNull();
    expect(problemOf(output!)).toBeNull();
    expect(problemOf({ ...input!, tag: ' ' })).toBe('No tag is given.');
    expect(problemOf({ ...published!, tag: 'external.P.X.y' })).toMatch(/starts with 'internal\.'/);
    // FALSE and 0 are safe values; only none is a problem.
    expect(problemOf({ ...output!, safe: false })).toBeNull();
    expect(problemOf({ ...output!, safe: 0 })).toBeNull();
  });
});
