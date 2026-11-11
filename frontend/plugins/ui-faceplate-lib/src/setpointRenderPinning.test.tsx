/**
 * S-014 — render-level pinning for the two things a constant-to-constant assertion cannot catch.
 *
 * Both of these were flagged in review, and both share one shape: the tests compared against the
 * exported constant, so moving the constant moved both sides of the comparison together.
 *
 *  - The NOTICE wording. Every assertion read `DRAFT_DISCARD_NOTICE.QUALITY_LOST`, so replacing that
 *    string with `'Value committed successfully'` passed the entire suite. A discard announcing a
 *    successful commit is a silent discard wearing a visible notice — confidently wrong in the one
 *    place an operator trusts it.
 *  - The RENDERED drift status. `Drift: NOT CONFIGURED` existed and was asserted nowhere, so it
 *    could be removed or reworded without a red.
 *
 * These assert the LITERAL text that reaches the operator, and they assert the constant TABLE as
 * well, so the next notice added is pinned too rather than only the ones with a test today.
 */

import { describe, it, expect } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';

import { MotorFaceplate } from './components/MotorFaceplate';
import { PidControllerFaceplate } from './components/PidControllerFaceplate';
import { DRAFT_DISCARD_NOTICE, DRIFT_NOT_CONFIGURED_TEXT } from './useOperatorDraft';

const act = (fn: () => void) => React.act(fn);

function mount(element: React.ReactElement) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root: Root = createRoot(container);
  const render = (el: React.ReactElement) => {
    act(() => {
      root.render(el);
    });
  };
  render(element);
  return {
    container,
    render,
    byTestId: (id: string) => container.querySelector(`[data-testid="${id}"]`),
    textOf: (id: string) => container.querySelector(`[data-testid="${id}"]`)?.textContent ?? '',
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

/** Fire a native input event the way a browser does, so React's onChange runs. */
function changeValue(input: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
    'value'
  )?.set;
  act(() => {
    setter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

const MOTOR_BASE = {
  tag: 'MTR-1',
  label: 'Feed Pump',
  quality: 'GOOD' as const,
  running: true,
  actualSpeed: 1450,
  speedSetpoint: 1400,
  maxSpeed: 1800,
  operatingMode: 'MANUAL' as const,
  // The speed setpoint control lives in the tier-3 diagnostic panel, not the tier-2 summary card.
  tier: 3 as const,
};

const PID_BASE = {
  tag: 'TIC-1',
  label: 'Loop 1',
  quality: 'GOOD' as const,
  pv: 50,
  sp: 55,
  cv: 42,
  kp: 1,
  tn: 5,
  tv: 0.5,
  tier: 3 as const,
};

describe('S-014: the constant table itself is pinned (so the next notice is too)', () => {
  it('pins every discard notice to its exact operator-facing words', () => {
    // A constant-to-constant assertion elsewhere cannot catch a meaning inversion here, because both
    // sides move together. This test is the one that can.
    expect(DRAFT_DISCARD_NOTICE).toEqual({
      QUALITY_LOST: 'Draft discarded: the device stopped reporting',
      IDENTITY_CHANGED: 'Draft discarded when the faceplate changed device',
      CANCELLED: 'Draft discarded',
    });
  });

  it('pins the unconfigured-drift wording', () => {
    expect(DRIFT_NOT_CONFIGURED_TEXT).toBe('NOT CONFIGURED');
  });

  it('never lets a discard notice claim a successful commit', () => {
    // The exact mutation from review: swap the string for its opposite and the suite stayed green.
    for (const words of Object.values(DRAFT_DISCARD_NOTICE)) {
      expect(words.toLowerCase()).not.toContain('committed successfully');
      expect(words.toLowerCase()).toContain('discard');
    }
  });
});

describe('S-014: the rendered drift status is pinned (O4)', () => {
  it('renders "Drift: NOT CONFIGURED" on the motor faceplate while a draft is open', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} driftThreshold={null} />);

    // Nothing is claimed before an episode opens: the control is showing the committed value, not a
    // draft, so there is no draft-versus-device comparison and nothing to qualify.
    expect(h.byTestId('motor-speed-drift-status')).toBeNull();

    const slider = h.container.querySelector('input[type="range"]') as HTMLInputElement;
    changeValue(slider, '1500');

    // The literal text, not the constant: this is the assertion the constant-based tests could not
    // make.
    expect(h.textOf('motor-speed-drift-status')).toBe('Drift: NOT CONFIGURED');
    h.unmount();
  });

  it('does not render the drift status when a threshold IS configured', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} driftThreshold={25} />);
    const slider = h.container.querySelector('input[type="range"]') as HTMLInputElement;
    changeValue(slider, '1500');
    expect(h.byTestId('motor-speed-drift-status')).toBeNull();
    h.unmount();
  });

  it('renders "Drift: NOT CONFIGURED" on the PID panel while the tuning draft is open', () => {
    const h = mount(<PidControllerFaceplate {...PID_BASE} driftThreshold={null} />);
    expect(h.byTestId('pid-tune-drift-status')).toBeNull();

    const kpInput = h.container.querySelector('input[type="number"]') as HTMLInputElement;
    changeValue(kpInput, '2');

    expect(h.textOf('pid-tune-drift-status')).toBe('Drift: NOT CONFIGURED');
    h.unmount();
  });
});

describe('S-014: the rendered discard notice is pinned, in its actual words', () => {
  it('announces the exact quality-loss wording when the device stops reporting', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} driftThreshold={null} />);
    const slider = h.container.querySelector('input[type="range"]') as HTMLInputElement;
    changeValue(slider, '1500');
    expect(h.byTestId('motor-speed-draft')).not.toBeNull();

    // The device stops reporting while the edit is in flight.
    h.render(<MotorFaceplate {...MOTOR_BASE} quality="BAD_QUALITY" driftThreshold={null} />);

    // Literal text. An inverted constant would still satisfy `toBe(DRAFT_DISCARD_NOTICE.X)`.
    expect(h.textOf('motor-speed-draft-notice')).toBe(
      'Draft discarded: the device stopped reporting'
    );
    expect(h.textOf('motor-speed-draft-notice')).not.toContain('success');
    h.unmount();
  });

  it('announces the exact re-key wording when the surface changes device (O6)', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} driftThreshold={null} />);
    const slider = h.container.querySelector('input[type="range"]') as HTMLInputElement;
    changeValue(slider, '1500');

    h.render(<MotorFaceplate {...MOTOR_BASE} tag="MTR-2" driftThreshold={null} />);

    expect(h.textOf('motor-speed-draft-notice')).toBe(
      'Draft discarded when the faceplate changed device'
    );
    h.unmount();
  });

  it('keeps the draft visibly marked uncommitted beside the committed value (R6)', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} driftThreshold={null} />);
    const slider = h.container.querySelector('input[type="range"]') as HTMLInputElement;
    changeValue(slider, '1500');

    const band = h.textOf('motor-speed-draft');
    expect(band).toContain('DRAFT');
    expect(band).toContain('uncommitted');
    // The device's own reading is shown beside the edit, so the operator can see it move.
    expect(band).toContain('1400');
    h.unmount();
  });
});

describe('S-014: no fabricated value and no fabricated bound reach the operator', () => {
  it('does not draw a slider thumb when nothing was measured', () => {
    // A range input always parks a thumb somewhere, and a thumb is a position. With no reported
    // setpoint there is no position to draw.
    const h = mount(
      <MotorFaceplate {...MOTOR_BASE} speedSetpoint={null} driftThreshold={null} />
    );
    expect(h.byTestId('motor-speed-slider-absent')).not.toBeNull();
    expect(h.container.querySelector('input[type="range"]')).toBeNull();
    h.unmount();
  });

  it('does not invent a scale when the design stores no range (O4 on the bound)', () => {
    const h = mount(<MotorFaceplate {...MOTOR_BASE} maxSpeed={null} driftThreshold={null} />);
    expect(h.byTestId('motor-speed-range-absent')).not.toBeNull();
    expect(h.container.querySelector('input[type="range"]')).toBeNull();
    // The range-absent line says so in words; it never renders "0 - <something>" as a scale.
    expect(h.textOf('motor-speed-range-absent')).toContain('----');
    expect(h.textOf('motor-speed-range-absent')).not.toContain('0 - ');
    h.unmount();
  });
});
