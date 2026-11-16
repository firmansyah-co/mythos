import { describe, expect, it } from 'vitest';
import { formatLocation, parseLocation, type WorkbenchLocation } from '../data/location';

describe('the workbench address', () => {
  it.each<WorkbenchLocation>([
    { projectId: 'boiler_house' },
    { projectId: 'boiler_house', editor: { kind: 'tags', connectionId: undefined } },
    { projectId: 'boiler_house', editor: { kind: 'tags', connectionId: 'boiler-plc' } },
    { projectId: 'boiler_house', editor: { kind: 'diagnostics', connectionId: 'a/b c' } },
  ])('round-trips %o', (location) => {
    expect(parseLocation(formatLocation(location))).toEqual(location);
  });

  it.each(['', '#', '#/', '#/nonsense', '#/project', '#/project/'])(
    'reads %o as no project',
    (hash) => {
      expect(parseLocation(hash)).toBeNull();
    }
  );

  it("reads any other first word as a module's view on a project (ADR-011 §2.3)", () => {
    expect(parseLocation('#/trend/p1')).toEqual({ projectId: 'p1', view: { id: 'trend' } });
    expect(parseLocation('#/trend/p1/pump%201')).toEqual({
      projectId: 'p1',
      view: { id: 'trend', target: 'pump 1' },
    });
    expect(formatLocation({ projectId: 'p1', view: { id: 'trend', target: 'pump 1' } })).toBe(
      '#/trend/p1/pump%201'
    );
  });

  it('opens the project but no editor when the editor kind is unknown', () => {
    expect(parseLocation('#/project/p1/teleporter')).toEqual({ projectId: 'p1' });
  });
});
