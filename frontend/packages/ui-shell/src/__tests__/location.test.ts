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

  it.each(['', '#', '#/', '#/nonsense/x', '#/project', '#/project/'])(
    'reads %o as no project',
    (hash) => {
      expect(parseLocation(hash)).toBeNull();
    }
  );

  it('opens the project but no editor when the editor kind is unknown', () => {
    expect(parseLocation('#/project/p1/teleporter')).toEqual({ projectId: 'p1' });
  });
});
