/**
 * S-004 — the data-quality badge must never name a cause it does not know.
 *
 * The defect this file exists for: `ProcessDataQuality` is three values and the badge's label
 * mapped every bad quality to `BAD COMM FAILURE`. A failed SENSOR therefore read as a comm fault —
 * a wrong cause stated with confidence, which is worse than an unnamed one because an operator
 * acts on it.
 *
 * Three cases, and they must not be confused with one another:
 *
 *   A  detail supplied AND recognised      -> name the cause (`BAD COMM FAILURE`)
 *   B  detail supplied, NOT recognised     -> neutral wording (`BAD QUALITY`), cause unnamed
 *   C  no detail supplied at all           -> unchanged (`BAD COMM FAILURE`)
 *
 * THE TRAP, and the reason most assertions here check presentation as well as wording: in
 * `DataQualityBadge` the tone and tooltip key off the three-value CLASS. If a fallback were keyed
 * off the specific quality instead, an unrecognised bad cause would drop into the grey `NOT_BOUND`
 * branch — telling an operator a bound, failing device has no binding, and filing a live equipment
 * failure as a configuration note. Two false statements. A text-only assertion would go green while
 * that happened, so every bad-quality case below asserts `data-quality` and the amber tone too.
 */

import { describe, it, expect } from 'vitest';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';

import { DataQualityBadge } from './components/DataQualityBadge';
import {
  dataQualityLabel,
  isKnownQualityDetail,
  BAD_QUALITY_TEXT,
  BAD_QUALITY_UNNAMED_TEXT,
  NOT_BOUND_TEXT,
  KNOWN_QUALITY_DETAILS,
} from './processData';
import type { ProcessDataQuality } from './types';

function renderBadge(quality: ProcessDataQuality, qualityDetail?: string | null): HTMLElement {
  const container = document.createElement('div');
  const root: Root = createRoot(container);
  flushSync(() => {
    root.render(
      <DataQualityBadge quality={quality} qualityDetail={qualityDetail ?? undefined} />
    );
  });
  const badge = container.querySelector('[data-testid="faceplate-no-data"]') as HTMLElement;
  if (!badge) throw new Error('badge did not render');
  return badge;
}

const isAmber = (el: HTMLElement) => el.className.includes('amber');
const isGrey = (el: HTMLElement) => el.className.includes('neutral');

describe('S-004: the quality badge does not name a cause it does not know', () => {
  // ---- CASE B: the defect ----------------------------------------------------------------

  it('renders neutral wording and KEEPS the abnormal presentation for an unrecognised bad cause', () => {
    const badge = renderBadge('BAD_QUALITY', 'BAD_SENSOR_FAILURE');

    // (a) the wording must not claim a communication failure, and must not be blank
    expect(badge.textContent).not.toContain('COMM');
    expect(badge.textContent).toBe(BAD_QUALITY_UNNAMED_TEXT);

    // (b) the presentation must stay abnormal — this is the half a text-only test misses
    expect(badge.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(isAmber(badge), 'an unnamed bad cause must stay amber, not grey').toBe(true);
    expect(isGrey(badge)).toBe(false);
    expect(badge.getAttribute('title')).toContain('not reporting usable data');
    expect(badge.getAttribute('title')).not.toContain('No device binding');
  });

  it('treats an UNCERTAIN state as an unknown cause rather than guessing', () => {
    const badge = renderBadge('BAD_QUALITY', 'UNCERTAIN_SUBSTITUTE');
    expect(badge.textContent).toBe(BAD_QUALITY_UNNAMED_TEXT);
    expect(badge.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(isAmber(badge)).toBe(true);
  });

  it('treats the wire UNRECOGNIZED sentinel as an unknown cause', () => {
    const badge = renderBadge('BAD_QUALITY', 'UNRECOGNIZED');
    expect(badge.textContent).toBe(BAD_QUALITY_UNNAMED_TEXT);
    expect(isAmber(badge)).toBe(true);
  });

  // ---- CASE A: a cause this build does name ----------------------------------------------

  it('names the cause when the detail is recognised', () => {
    const badge = renderBadge('BAD_QUALITY', 'BAD_COMM_FAILURE');
    expect(badge.textContent).toBe(BAD_QUALITY_TEXT);
    expect(badge.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(isAmber(badge)).toBe(true);
  });

  // ---- CASE C: unchanged behaviour, pinned -------------------------------------------------

  it('keeps the existing wording when no detail is supplied at all', () => {
    const badge = renderBadge('BAD_QUALITY');
    expect(badge.textContent).toBe(BAD_QUALITY_TEXT);
    expect(badge.getAttribute('data-quality')).toBe('BAD_QUALITY');
    expect(isAmber(badge)).toBe(true);
  });

  it('keeps the existing wording when the detail is null or empty', () => {
    expect(renderBadge('BAD_QUALITY', null).textContent).toBe(BAD_QUALITY_TEXT);
    expect(renderBadge('BAD_QUALITY', '').textContent).toBe(BAD_QUALITY_TEXT);
  });

  // ---- NOT_BOUND is a configuration state and must stay grey ------------------------------

  it('renders NOT BOUND neutrally, and never as an abnormal quality', () => {
    const badge = renderBadge('NOT_BOUND');
    expect(badge.textContent).toBe(NOT_BOUND_TEXT);
    expect(badge.getAttribute('data-quality')).toBe('NOT_BOUND');
    expect(isGrey(badge)).toBe(true);
    expect(isAmber(badge)).toBe(false);
    // A configuration state names no cause at all, whatever detail arrives with it.
    const withDetail = renderBadge('NOT_BOUND', 'BAD_SENSOR_FAILURE');
    expect(withDetail.textContent).toBe(NOT_BOUND_TEXT);
    expect(isGrey(withDetail)).toBe(true);
  });

  it('renders nothing for live data', () => {
    const container = document.createElement('div');
    const root = createRoot(container);
    flushSync(() => {
      root.render(<DataQualityBadge quality="GOOD" />);
    });
    expect(container.querySelector('[data-testid="faceplate-no-data"]')).toBeNull();
  });
});

describe('S-004: the label function states the three cases directly', () => {
  it('is an allow-list, not a guess: only BAD_COMM_FAILURE is named', () => {
    expect(KNOWN_QUALITY_DETAILS).toEqual(['BAD_COMM_FAILURE']);
    expect(isKnownQualityDetail('BAD_COMM_FAILURE')).toBe(true);
    expect(isKnownQualityDetail('BAD_SENSOR_FAILURE')).toBe(false);
    expect(isKnownQualityDetail('BAD_DEVICE_FAILURE')).toBe(false);
    expect(isKnownQualityDetail('BAD_ANYTHING_ELSE')).toBe(false);
    expect(isKnownQualityDetail(undefined)).toBe(false);
    expect(isKnownQualityDetail(null)).toBe(false);
  });

  it('maps the three cases', () => {
    expect(dataQualityLabel('BAD_QUALITY', 'BAD_COMM_FAILURE')).toBe(BAD_QUALITY_TEXT); // A
    expect(dataQualityLabel('BAD_QUALITY', 'BAD_SENSOR_FAILURE')).toBe(BAD_QUALITY_UNNAMED_TEXT); // B
    expect(dataQualityLabel('BAD_QUALITY')).toBe(BAD_QUALITY_TEXT); // C
    expect(dataQualityLabel('NOT_BOUND')).toBe(NOT_BOUND_TEXT);
    expect(dataQualityLabel('GOOD')).toBeNull();
  });
});
