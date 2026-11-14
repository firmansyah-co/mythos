import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { useController } from '../data/controller';
import type { TagSample } from '../data/quality';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { TagTable } from '../editors/TagTable';
import { connection, ioStatus, project, render, resetStores, sample, unmountAll } from './harness';

const LIVE = 'external.Plant.Unit1.speed';
const STALE = 'external.Plant.Unit1.pressure';
const UNREAD = 'external.Plant.Unit1.level';
const SILENT = 'external.Plant.Unit1.flow';
const ALL = [LIVE, STALE, UNREAD, SILENT];

function setTags(samples: TagSample[]) {
  act(() =>
    useTelemetry.setState({
      tags: new Map(samples.map((s) => [s.uri, s])),
      stream: 'OPEN',
      lastBatchAtMs: 1000,
    })
  );
}

/** The value cell and quality badge of one tag's row, read from the rendered table. */
function row(container: HTMLElement, uri: string) {
  const tr = container.querySelector(`[data-testid="tag-row"][data-uri="${uri}"]`);
  if (!tr) throw new Error(`no row for ${uri}`);
  const value = tr.querySelector('[data-testid="tag-value"]');
  const badge = tr.querySelector('[data-testid="quality-badge"]');
  return {
    state: value?.getAttribute('data-state'),
    valueText: value?.textContent ?? '',
    badge: badge?.textContent ?? '',
    text: tr.textContent ?? '',
  };
}

beforeEach(() => {
  resetStores();
  useWorkbench.setState({ project: project('plant'), view: 'project' });
  useController.setState({
    activeProjectId: 'plant',
    io: ioStatus([connection('plc1', ALL)]),
  });
});

afterEach(unmountAll);

describe('the tag table', () => {
  it('shows a GOOD value as live and nothing else as live', () => {
    const container = render(<TagTable />);
    setTags([
      sample(LIVE, { value: 1450 }),
      sample(STALE, { value: 730, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x18 }),
      sample(UNREAD, { type: 'UNSPECIFIED', value: null, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x18, timestampNs: 0 }),
      // SILENT is configured but absent from the batch.
    ]);

    expect(row(container, LIVE)).toMatchObject({ state: 'live', valueText: '1450', badge: 'GOOD' });

    const stale = row(container, STALE);
    expect(stale.state).toBe('stale');
    expect(stale.valueText).toContain('730');
    expect(stale.valueText).toContain('stale');
    expect(stale.badge).toBe('BAD COMM FAILURE');

    const unread = row(container, UNREAD);
    expect(unread.state).toBe('never-read');
    expect(unread.valueText).toBe('—');
    expect(unread.badge).toBe('BAD COMM FAILURE');

    const silent = row(container, SILENT);
    expect(silent.state).toBe('no-data');
    expect(silent.valueText).toBe('—');
    expect(silent.badge).toBe('NO DATA');

    const liveCells = container.querySelectorAll('[data-testid="tag-value"][data-state="live"]');
    expect(liveCells.length).toBe(1);
  });

  it('keeps every configured tag on screen when the stream reports none of them', () => {
    const container = render(<TagTable />);
    setTags([]);
    expect(container.querySelectorAll('[data-testid="tag-row"]').length).toBe(ALL.length);
    for (const uri of ALL) expect(row(container, uri).badge).toBe('NO DATA');
  });

  it('a value that was live stops being shown as live the moment its quality drops', () => {
    const container = render(<TagTable />);
    setTags([sample(LIVE, { value: 1450 })]);
    expect(row(container, LIVE).state).toBe('live');

    setTags([sample(LIVE, { value: 1450, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x18 })]);
    expect(row(container, LIVE).state).toBe('stale');
  });

  it.each(['IDLE', 'CONNECTING', 'CLOSED'] as const)(
    'shows no value at all while the stream is %s, whatever the cache holds',
    (stream) => {
      const container = render(<TagTable />);
      setTags(ALL.map((uri) => sample(uri, { value: 999 })));
      act(() => useTelemetry.setState({ stream }));

      for (const uri of ALL) {
        const r = row(container, uri);
        expect(r.state).toBe('offline');
        expect(r.valueText).toBe('—');
        expect(r.badge).toBe('OFFLINE');
        expect(r.text).not.toContain('999');
        expect(r.text).not.toContain('UINT');
      }
      expect(container.querySelector('[data-testid="offline-hint"]')).not.toBeNull();
    }
  );

  it('lists only the tags of the connection it is scoped to', () => {
    useController.setState({
      io: ioStatus([connection('plc1', [LIVE]), connection('plc2', [STALE, UNREAD])]),
    });
    const container = render(<TagTable connectionId="plc2" />);
    const uris = [...container.querySelectorAll('[data-testid="tag-row"]')].map((tr) =>
      tr.getAttribute('data-uri')
    );
    expect(uris).toEqual([UNREAD, STALE].sort());
  });
});
