/**
 * S-014 — the draft-vs-committed rule, tested against the cases that go green on a broken
 * implementation.
 *
 * The rule's own §6 names the trap: "a test that only renders with no telemetry — or only without
 * an open draft — passes against a broken implementation and proves nothing." So every case here
 * opens a real episode first and then delivers telemetry underneath it.
 */

import { describe, it, expect } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';

import {
  useOperatorDraft,
  hasDrifted,
  isDriftConfigured,
  formatCommitted,
  useTuningDraft,
  COMMITTED_UNKNOWN_TEXT,
  DRAFT_DISCARD_NOTICE,
  type TuningDraft,
  type UseTuningDraftOptions,
} from './useOperatorDraft';

type Options = Parameters<typeof useOperatorDraft>[0];
type Draft = ReturnType<typeof useOperatorDraft>;

/**
 * A mounted host that re-renders in place, so a prop update is observable.
 *
 * `renderToString` renders once and cannot see a prop change at all, which would make every
 * "telemetry arrived underneath the draft" case invisible — the repo's existing faceplate tests
 * use this same pattern for the same reason.
 */
function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root: Root = createRoot(container);
  let latest: Draft | null = null;

  function Probe(props: Options) {
    latest = useOperatorDraft(props);
    return null;
  }

  const defaults: Options = {
    committed: 100,
    isLive: true,
    identity: 'mtr_1',
    driftThreshold: null,
  };

  return {
    render(props: Partial<Options> = {}) {
      act(() => {
        root.render(React.createElement(Probe, { ...defaults, ...props }));
      });
    },
    get current(): Draft {
      if (!latest) throw new Error('hook not rendered');
      return latest;
    },
    unmount() {
      act(() => {
        root.unmount();
      });
      container.remove();
    },
  };
}

const setup = (overrides: Partial<Options> = {}) => {
  const harness = mount();
  harness.render(overrides);
  return {
    result: {
      get current() {
        return harness.current;
      },
    },
    rerender: (props: Partial<Options>) => harness.render(props),
    unmount: () => harness.unmount(),
  };
};

/**
 * `act` from React 19 itself, so no testing-library dependency is needed.
 *
 * This matters for correctness of the TEST, not just style: `flushSync` does not flush passive
 * effects, and every R7/O6 discard is applied from a `useEffect`. Under `flushSync` those effects
 * never ran and the discard looked broken when it was not — the failure was in the harness, and it
 * is the reason these two cases must use `act`.
 */
const act = (fn: () => void) => React.act(fn);

describe('S-014: no mount-time seed (R2)', () => {
  it('shows the committed value and opens no draft on mount', () => {
    const { result } = setup({ committed: 1450 });
    expect(result.current.value).toBe(1450);
    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.draft).toBeNull();
  });

  it('shows nothing when the device reports nothing, rather than inventing 0', () => {
    // The replaced defect was `useState(speedSetpoint ?? 0)`, which rendered a fabricated 0.
    const { result } = setup({ committed: null });
    expect(result.current.value).toBeNull();
    expect(result.current.isDraftOpen).toBe(false);
  });

  it('follows telemetry that arrives after mount when no draft is open', () => {
    // The stale-as-current half of the defect: the old code pinned the mount-time value forever.
    const { result, rerender } = setup({ committed: 100 });
    expect(result.current.value).toBe(100);
    rerender({ committed: 1200, isLive: true, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.value).toBe(1200);
  });
});

describe('S-014: an episode opens on input, not on focus (R3)', () => {
  it('stays closed until the operator actually changes something', () => {
    const { result } = setup();
    expect(result.current.isDraftOpen).toBe(false);
    act(() => result.current.update(140));
    expect(result.current.isDraftOpen).toBe(true);
  });

  it('shows the draft, not the committed value, once open', () => {
    const { result } = setup({ committed: 100 });
    act(() => result.current.update(140));
    expect(result.current.value).toBe(140);
    expect(result.current.committed).toBe(100);
  });

  it('treats a cleared field as a real input, not as "no episode"', () => {
    // A numeric input can legitimately be emptied. Openness is tracked explicitly for this reason.
    const { result } = setup({ committed: 100 });
    act(() => result.current.update(null));
    expect(result.current.isDraftOpen).toBe(true);
    expect(result.current.value).toBeNull();
  });
});

describe('S-014: a telemetry frame never overwrites an open draft (R5)', () => {
  it('keeps the draft when the committed value moves underneath it', () => {
    const { result, rerender } = setup({ committed: 100 });
    act(() => result.current.update(140));

    // A frame lands with a different committed value.
    rerender({ committed: 900, isLive: true, identity: 'mtr_1', driftThreshold: null });

    expect(result.current.draft).toBe(140); // the edit survived the frame
    expect(result.current.committed).toBe(900); // and the committed value still moved
    expect(result.current.value).toBe(140); // what the control shows
  });
});

describe('S-014: divergence is visible, and only when configured (R6/O4)', () => {
  it('reports divergence once the committed value moves past the threshold', () => {
    const { result, rerender } = setup({ committed: 100, driftThreshold: 10 });
    act(() => result.current.update(140));
    expect(result.current.diverged).toBe(false);

    rerender({ committed: 105, isLive: true, identity: 'mtr_1', driftThreshold: 10 });
    expect(result.current.diverged).toBe(false); // within the bound

    rerender({ committed: 130, isLive: true, identity: 'mtr_1', driftThreshold: 10 });
    expect(result.current.diverged).toBe(true); // 30 > 10
  });

  it('never reports divergence when no threshold is configured', () => {
    // O4: the threshold is REQUIRED config with NO DEFAULT. Unconfigured means inactive, never a
    // guessed bound.
    const { result, rerender } = setup({ committed: 100, driftThreshold: null });
    act(() => result.current.update(140));
    rerender({ committed: 10000, isLive: true, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.diverged).toBe(false);
  });

  it('never reports divergence with no episode open', () => {
    const { result } = setup({ committed: 100, driftThreshold: 10 });
    expect(result.current.diverged).toBe(false);
  });
});

describe('S-014: leaving GOOD discards the draft with a visible reason (R7)', () => {
  it('discards and announces when quality leaves GOOD', () => {
    const { result, rerender } = setup({ committed: 100 });
    act(() => result.current.update(140));
    expect(result.current.isDraftOpen).toBe(true);

    rerender({ committed: 100, isLive: false, identity: 'mtr_1', driftThreshold: null });

    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.draft).toBeNull();
    expect(result.current.notice).toBe(DRAFT_DISCARD_NOTICE.QUALITY_LOST);
    expect(result.current.notice).not.toBeNull(); // the reason is on screen, not silent
  });

  it('does not invent a draft when quality leaves GOOD with nothing open', () => {
    const { result, rerender } = setup({ committed: 100 });
    rerender({ committed: 100, isLive: false, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.notice).toBeNull();
  });
});

describe('S-014: a re-key discards with a visible notice (O6)', () => {
  it('discards the draft when the surface changes device, and says so', () => {
    const { result, rerender } = setup({ committed: 100, identity: 'mtr_1' });
    act(() => result.current.update(140));

    rerender({ committed: 100, isLive: true, identity: 'mtr_2', driftThreshold: null });

    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.notice).toBe(DRAFT_DISCARD_NOTICE.IDENTITY_CHANGED);
  });

  it('does not discard when the identity is merely re-rendered unchanged', () => {
    const { result, rerender } = setup({ committed: 100, identity: 'mtr_1' });
    act(() => result.current.update(140));
    rerender({ committed: 100, isLive: true, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.draft).toBe(140);
    expect(result.current.notice).toBeNull();
  });
});

describe('S-014: closing an episode (R4, no echo path)', () => {
  it('commit closes the draft and returns the control to the committed value', () => {
    const { result, rerender } = setup({ committed: 100 });
    act(() => result.current.update(140));
    act(() => result.current.commit());

    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.draft).toBeNull();
    expect(result.current.notice).toBeNull(); // a commit is not a discard; nothing to announce
    rerender({ committed: 155, isLive: true, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.value).toBe(155);
  });

  it('cancel discards with a visible reason', () => {
    const { result } = setup({ committed: 100 });
    act(() => result.current.update(140));
    act(() => result.current.cancel());
    expect(result.current.isDraftOpen).toBe(false);
    expect(result.current.notice).toBe(DRAFT_DISCARD_NOTICE.CANCELLED);
  });

  it('has no echo path: a frame landing on the drafted value does NOT close the episode', () => {
    // O2 is struck because value equality cannot distinguish an echo from a coincidence. If an
    // echo path existed, this frame would close the episode. It must not.
    const { result, rerender } = setup({ committed: 100 });
    act(() => result.current.update(140));
    rerender({ committed: 140, isLive: true, identity: 'mtr_1', driftThreshold: null });
    expect(result.current.isDraftOpen).toBe(true);
    expect(result.current.draft).toBe(140);
  });

  it('the notice can be dismissed', () => {
    const { result } = setup({ committed: 100 });
    act(() => result.current.update(140));
    act(() => result.current.cancel());
    expect(result.current.notice).not.toBeNull();
    act(() => result.current.dismissNotice());
    expect(result.current.notice).toBeNull();
  });
});

describe('S-014: helpers', () => {
  it('treats only a finite non-negative number as a configured threshold', () => {
    expect(isDriftConfigured(10)).toBe(true);
    expect(isDriftConfigured(0)).toBe(true);
    expect(isDriftConfigured(null)).toBe(false);
    expect(isDriftConfigured(undefined)).toBe(false);
    expect(isDriftConfigured(Number.NaN)).toBe(false);
    expect(isDriftConfigured(Number.POSITIVE_INFINITY)).toBe(false);
    expect(isDriftConfigured(-1)).toBe(false);
  });

  it('never drifts on an unconfigured threshold or a missing comparison', () => {
    expect(hasDrifted(100, 10000, null)).toBe(false);
    expect(hasDrifted(null, 100, 10)).toBe(false);
    expect(hasDrifted(100, null, 10)).toBe(false);
    expect(hasDrifted(100, 120, 10)).toBe(true);
    expect(hasDrifted(120, 100, 10)).toBe(true); // direction does not matter
  });

  it('states an unreported committed value as UNKNOWN rather than a number (O5)', () => {
    expect(formatCommitted(null, (v) => `${v} RPM`)).toBe(COMMITTED_UNKNOWN_TEXT);
    expect(formatCommitted(1450, (v) => `${v} RPM`)).toBe('1450 RPM');
  });
});

describe('S-014: the tuning trio is ONE episode (R8)', () => {
  const committed = { kp: 1, tn: 5, tv: 0.5 };

  function mountTrio(overrides: Partial<UseTuningDraftOptions> = {}) {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let latest: TuningDraft | null = null;
    function Probe(props: UseTuningDraftOptions) {
      latest = useTuningDraft(props);
      return null;
    }
    const defaults: UseTuningDraftOptions = {
      committed,
      isLive: true,
      identity: 'pid_1',
      driftThreshold: null,
    };
    const render = (props: Partial<UseTuningDraftOptions> = {}) => {
      act(() => {
        root.render(React.createElement(Probe, { ...defaults, ...props }));
      });
    };
    render(overrides);
    return {
      get current(): TuningDraft {
        if (!latest) throw new Error('not rendered');
        return latest;
      },
      rerender: render,
      unmount: () => {
        act(() => root.unmount());
        container.remove();
      },
    };
  }

  it('opens one episode covering all three fields', () => {
    const h = mountTrio();
    expect(h.current.isDraftOpen).toBe(false);
    act(() => h.current.update({ kp: 2, tn: 5, tv: 0.5 }));
    expect(h.current.isDraftOpen).toBe(true);
    expect(h.current.draft).toEqual({ kp: 2, tn: 5, tv: 0.5 });
  });

  it('does not seed from props at mount', () => {
    const h = mountTrio({ committed: { kp: null, tn: null, tv: null } });
    expect(h.current.isDraftOpen).toBe(false);
    expect(h.current.value).toEqual({ kp: null, tn: null, tv: null });
  });

  it('keeps all three drafts when a frame lands, and closes them together on commit', () => {
    const h = mountTrio();
    act(() => h.current.update({ kp: 2, tn: 9, tv: 0.9 }));
    h.rerender({ committed: { kp: 1.5, tn: 5, tv: 0.5 } });
    expect(h.current.draft).toEqual({ kp: 2, tn: 9, tv: 0.9 }); // survived the frame
    expect(h.current.committed).toEqual({ kp: 1.5, tn: 5, tv: 0.5 });

    act(() => h.current.commit());
    expect(h.current.isDraftOpen).toBe(false);
    expect(h.current.draft).toBeNull();
    expect(h.current.value).toEqual({ kp: 1.5, tn: 5, tv: 0.5 }); // back to reporting telemetry
  });

  it('discards all three together when quality leaves GOOD, with a visible reason', () => {
    const h = mountTrio();
    act(() => h.current.update({ kp: 2, tn: 9, tv: 0.9 }));
    h.rerender({ isLive: false });
    expect(h.current.isDraftOpen).toBe(false);
    expect(h.current.draft).toBeNull();
    expect(h.current.notice).toBe(DRAFT_DISCARD_NOTICE.QUALITY_LOST);
  });

  it('discards all three on a device change, with a visible reason', () => {
    const h = mountTrio();
    act(() => h.current.update({ kp: 2, tn: 9, tv: 0.9 }));
    h.rerender({ identity: 'pid_2' });
    expect(h.current.isDraftOpen).toBe(false);
    expect(h.current.notice).toBe(DRAFT_DISCARD_NOTICE.IDENTITY_CHANGED);
  });

  it('reports divergence when ANY field moves past the threshold', () => {
    const h = mountTrio({ driftThreshold: 0.2 });
    act(() => h.current.update({ kp: 2, tn: 5, tv: 0.5 }));
    expect(h.current.diverged).toBe(false);
    h.rerender({ committed: { kp: 1, tn: 5, tv: 0.9 }, driftThreshold: 0.2 });
    expect(h.current.diverged).toBe(true); // tv moved 0.4 > 0.2
  });
});
