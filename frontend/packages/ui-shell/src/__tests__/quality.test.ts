import { describe, expect, it } from 'vitest';
import { formatValue, isGood, viewTag, type StreamState, type TagSample } from '../data/quality';
import { sample } from './harness';

const URI = 'external.Plant.Dev.value';

/** Every quality a slot can carry that is not the one named GOOD state. */
const NOT_GOOD: Array<Partial<TagSample>> = [
  { qualityName: 'BAD_NON_SPECIFIC', qualityCode: 0x0000 },
  { qualityName: 'BAD_CONFIG_ERROR', qualityCode: 0x0004 },
  { qualityName: 'BAD_NOT_CONNECTED', qualityCode: 0x0008 },
  { qualityName: 'BAD_DEVICE_FAILURE', qualityCode: 0x000c },
  { qualityName: 'BAD_SENSOR_FAILURE', qualityCode: 0x0010 },
  { qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x0018 },
  { qualityName: 'UNCERTAIN_SUBSTITUTE', qualityCode: 0x0044 },
  { qualityName: 'UNCERTAIN_INITIAL_VALUE', qualityCode: 0x0048 },
  { qualityName: 'UNRECOGNIZED', qualityCode: 0x1234 },
  // A name this build has never heard of, sitting on the GOOD code.
  { qualityName: 'GOOD_BUT_NEW', qualityCode: 0x00c0 },
  // No name at all: a word whose class bits read good, which this build cannot name.
  { qualityName: '', qualityCode: 0x7fff },
  { qualityName: '', qualityCode: -1 },
];

describe('which values may be shown as current', () => {
  it('shows a GOOD typed value as live', () => {
    expect(viewTag(sample(URI), 'OPEN')).toEqual({ kind: 'live', text: '1234' });
  });

  it('accepts a nameless word only when it is exactly the GOOD code', () => {
    expect(isGood(sample(URI, { qualityName: '', qualityCode: 0x00c0 }))).toBe(true);
  });

  it.each(NOT_GOOD)('never shows %o as live, even with a value in the slot', (quality) => {
    const view = viewTag(sample(URI, quality), 'OPEN');
    expect(view.kind).toBe('stale');
    expect(isGood(sample(URI, quality))).toBe(false);
  });

  it('does not call a GOOD quality healthy when there is no value behind it', () => {
    const view = viewTag(sample(URI, { type: 'UNSPECIFIED', value: null }), 'OPEN');
    expect(view.kind).toBe('never-read');
    expect(view.kind === 'never-read' && view.tone).not.toBe('good');
  });

  it('reports a tag that was never read with its fault and no value', () => {
    const view = viewTag(
      sample(URI, { type: 'UNSPECIFIED', value: null, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x18, timestampNs: 0 }),
      'OPEN'
    );
    expect(view).toEqual({ kind: 'never-read', qualityLabel: 'BAD COMM FAILURE', tone: 'bad' });
  });

  it('says NO DATA for a tag the open stream does not report', () => {
    expect(viewTag(undefined, 'OPEN')).toEqual({ kind: 'no-data' });
  });

  it.each<StreamState>(['IDLE', 'CONNECTING', 'CLOSED'])(
    'shows nothing for any sample while the stream is %s',
    (stream) => {
      for (const candidate of [sample(URI), ...NOT_GOOD.map((q) => sample(URI, q)), undefined]) {
        expect(viewTag(candidate, stream)).toEqual({ kind: 'offline' });
      }
    }
  );
});

describe('formatting a value in its declared type', () => {
  it('prints a REAL to the precision a 32-bit float has', () => {
    expect(formatValue('REAL', 56.23735046386719)).toBe('56.23735');
    expect(formatValue('REAL', 21.5)).toBe('21.5');
  });

  it('prints integers and booleans as they are', () => {
    expect(formatValue('DINT', -55)).toBe('-55');
    expect(formatValue('UINT', 0)).toBe('0');
    expect(formatValue('BOOL', true)).toBe('TRUE');
    expect(formatValue('BOOL', false)).toBe('FALSE');
  });

  it.each([
    ['UINT', 1.5],
    ['UINT', '12'],
    ['BOOL', 1],
    ['REAL', null],
    ['REAL', Number.NaN],
    ['UNSPECIFIED', 0],
    ['SOMETHING_NEW', 7],
  ])('returns no text for %s holding %o, rather than a guess', (type, value) => {
    expect(formatValue(type, value)).toBeNull();
  });
});
