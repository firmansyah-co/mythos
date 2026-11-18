// A forced value is marked FORCED wherever the workbench shows a tag's value (ADR-013 §2.5),
// and only a tag the controller says is forced is marked: a list that is not known marks none.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { useController } from '../data/controller';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { TagTable } from '../editors/TagTable';
import { controllerView } from '../modules/host';
import { Inspector } from '../shell/Inspector';
import { OnlineTools } from '../shell/TaskCards';
import { connection, ioStatus, project, render, resetStores, sample, unmountAll } from './harness';

const FORCED = 'external.Plant.P1.speed';
const FREE = 'external.Plant.LT1.level';

function runningPlant(forcedTags: string[] | null) {
  act(() => {
    useWorkbench.setState({ project: project('plant'), view: 'project' });
    useController.setState({
      activeProjectId: 'plant',
      io: ioStatus([connection('plc1', [FORCED, FREE])]),
      forcedTags,
    });
    useTelemetry.setState({
      tags: new Map([
        [FORCED, sample(FORCED, { value: 40 })],
        [FREE, sample(FREE, { value: 7 })],
      ]),
      stream: 'OPEN',
      lastBatchAtMs: 1,
    });
  });
}

/** The FORCED mark in the table row of `uri`, or `null` when it has none. */
function rowMark(container: HTMLElement, uri: string): Element | null {
  const tr = container.querySelector(`[data-testid="tag-row"][data-uri="${uri}"]`);
  if (!tr) throw new Error(`no row for ${uri}`);
  return tr.querySelector('[data-testid="tag-forced"]');
}

/** The FORCED mark of the inspector's tag properties, or `null` when it has none. */
function inspectorMark(uri: string): Element | null {
  act(() => useWorkbench.setState({ selection: { kind: 'tag', uri }, inspectorTab: 'properties' }));
  const container = render(<Inspector collapsed={false} />);
  const props = container.querySelector('[data-testid="tag-properties"]');
  if (!props) throw new Error('the inspector shows no tag properties');
  return props.querySelector('[data-testid="tag-forced"]');
}

beforeEach(resetStores);
afterEach(unmountAll);

describe('a forced value in the tag table', () => {
  it('is marked FORCED, and a tag that is not forced is not', () => {
    runningPlant([FORCED]);
    const container = render(<TagTable />);
    const mark = rowMark(container, FORCED);
    expect(mark?.textContent).toBe('FORCED');
    expect(mark?.getAttribute('title')).toContain('An engineer forces the program variable bound to this tag');
    expect(rowMark(container, FREE)).toBeNull();
  });

  it('is not marked while the forced tags are not known', () => {
    runningPlant(null);
    const container = render(<TagTable />);
    expect(rowMark(container, FORCED)).toBeNull();
    expect(rowMark(container, FREE)).toBeNull();
  });

  it('loses its mark as soon as the controller says it is released', () => {
    runningPlant([FORCED]);
    const container = render(<TagTable />);
    expect(rowMark(container, FORCED)).not.toBeNull();
    act(() => useController.setState({ forcedTags: [] }));
    expect(rowMark(container, FORCED)).toBeNull();
  });
});

describe('a forced value in the inspector', () => {
  it('is marked FORCED, and a tag that is not forced is not', () => {
    runningPlant([FORCED]);
    expect(inspectorMark(FORCED)?.textContent).toBe('FORCED');
    unmountAll();
    expect(inspectorMark(FREE)).toBeNull();
  });

  it('is not marked while the forced tags are not known', () => {
    runningPlant(null);
    expect(inspectorMark(FORCED)).toBeNull();
  });
});

describe('the online tools card', () => {
  const forcedCell = (container: HTMLElement) => container.querySelector('[data-testid="card-forced"]');

  it('counts the forced tags and marks them', () => {
    runningPlant([FORCED]);
    const cell = forcedCell(render(<OnlineTools />));
    expect(cell?.textContent).toContain('1');
    expect(cell?.querySelector('[data-testid="tag-forced"]')?.textContent).toBe('FORCED');
  });

  it('says none is forced when the controller says so, with no mark', () => {
    runningPlant([]);
    const cell = forcedCell(render(<OnlineTools />));
    expect(cell?.textContent).toBe('0');
    expect(cell?.querySelector('[data-testid="tag-forced"]')).toBeNull();
  });

  it('says a dash, not zero, while the forced tags are not known', () => {
    runningPlant(null);
    const cell = forcedCell(render(<OnlineTools />));
    expect(cell?.textContent).toBe('—');
    expect(cell?.querySelector('[data-testid="tag-forced"]')).toBeNull();
  });
});

describe('what a module is told of forced tags', () => {
  it('is what the controller store holds, null included, in a new snapshot when it changes', () => {
    act(() => useController.setState({ forcedTags: null }));
    const unknown = controllerView();
    expect(unknown.forcedTags).toBeNull();
    act(() => useController.setState({ forcedTags: [FORCED] }));
    const known = controllerView();
    expect(known).not.toBe(unknown);
    expect(known.forcedTags).toEqual([FORCED]);
  });
});
