// The configured runtimes' pure rules (C8d-1): the body the module takes, ids without case, a
// draft that lacks a decision, and a refusal of a runtime route in the module's own words.

import { describe, expect, it } from 'vitest';
import { refusalOf } from '../library';
import { bodyOf, holdersOf, missingOf, newRuntime, newTask, sameId, sentOf, type Instance, type RuntimeEntry } from '../runtimes';

const line: RuntimeEntry = {
  id: 'Line',
  module: 'vm',
  target: 'pc',
  run: false,
  tasks: [{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['Tank'] }],
};

describe('the runtime rules', () => {
  it('bodyOf is the four keys the module takes', () => {
    const body = bodyOf(line);
    expect(Object.keys(body)).toEqual(['module', 'target', 'run', 'tasks']);
    expect(body).toEqual({ module: 'vm', target: 'pc', run: false, tasks: line.tasks });
    // The id is the path's, never the body's: the module refuses a key it does not know.
    expect('id' in body).toBe(false);
    // What is sent of a complete draft is the same four keys, and each task the protocol's keys.
    const sent = sentOf(line);
    expect(Object.keys(sent ?? {})).toEqual(['module', 'target', 'run', 'tasks']);
    expect(sent?.tasks).toEqual([{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['Tank'] }]);
    // A number the module has a default for, not given, is left out rather than sent as 0 or null.
    const blank = sentOf({ ...line, tasks: [{ name: 'f', kind: { type: 'cyclic', interval_ms: 5, phase_ms: null }, priority: 2, watchdog_ms: null, programs: [] }] });
    expect(blank?.tasks).toEqual([{ name: 'f', kind: { type: 'cyclic', interval_ms: 5 }, priority: 2, programs: [] }]);
  });

  it('sameId reads ids without case', () => {
    expect(sameId('Line', 'line')).toBe(true);
    expect(sameId('LINE', 'Line')).toBe(true);
    expect(sameId('Line', 'Line')).toBe(true);
    expect(sameId('Line', 'Line2')).toBe(false);
    expect(sameId('Line', 'Lin')).toBe(false);
  });

  it('a draft without run, a task\'s kind or priority is incomplete and names the field', () => {
    // A new runtime: nothing chosen, and run is said to be missing.
    expect(newRuntime('Line')).toEqual({ id: 'Line', module: '', target: '', run: null, tasks: [] });
    expect(missingOf(newRuntime('Line'))).toEqual([
      'whether it runs when loaded: choose "Run when loaded" or "Stay in STOP when loaded"',
    ]);
    expect(sentOf(newRuntime('Line'))).toBeNull();

    // A new task has no kind and no priority, and each is named, by its name or its place.
    expect(newTask()).toEqual({ name: '', kind: null, priority: null, programs: [] });
    const draft: RuntimeEntry = { ...line, tasks: [newTask(), { ...newTask(), name: 'slow', kind: { type: 'cyclic', interval_ms: null } }] };
    expect(missingOf(draft)).toEqual(['task 1: its kind', 'task 1: its priority', 'task slow: its interval', 'task slow: its priority']);
    expect(sentOf(draft)).toBeNull();

    // Every rule the module owns is left to it: an empty id, module, target or task name, a
    // priority past 31, a task with no program are all complete bodies, for the module to answer.
    const theirs: RuntimeEntry = { id: '', module: '', target: '', run: true, tasks: [{ name: '', kind: { type: 'event', tag: '' }, priority: 99, programs: [] }] };
    expect(missingOf(theirs)).toEqual([]);
    expect(sentOf(theirs)).toEqual({ module: '', target: '', run: true, tasks: [{ name: '', kind: { type: 'event', tag: '' }, priority: 99, programs: [] }] });
  });

  it("refusalOf keeps the module's words", () => {
    const words = "program 'Tank' is in runtime 'Other' already; a program is in one runtime";
    const thrown = Object.assign(new Error(`409 Conflict: ${words}`), { status: 409, body: { error: words } });
    expect(refusalOf(thrown)).toEqual({ status: 409, error: words, diagnostics: null, withheld: false });
  });

  it('holdersOf names the other instances that hold a program of the build, never one it replaces', () => {
    const holds = (id: string, from: string): Instance => ({ id, program: { name: from, built_from: from } });
    const saved = [holds('Line', 'Tank'), holds('Old', 'tank'), holds('Other', 'TANK'), holds('Apart', 'Pump')];
    const built: Instance = { id: 'Line', programs: [{ name: 'Tank', built_from: 'Tank' }] };
    // By the project name, without case; never the instance itself.
    expect(holdersOf(saved, built, []).map((i) => i.id)).toEqual(['Old', 'Other']);
    // An instance the build replaces is replaced, not a holder taken out beside it.
    expect(holdersOf(saved, built, ['Old']).map((i) => i.id)).toEqual(['Other']);
  });
});
