// When a watch answer's values are the program's current ones (ADR-013 §2.8): the exact edges
// of both bounds, at a given instant, and what is not said made not current, never current.
// The views' own tests (ladder, fbd, sfc, programs) prove each draws what this decides.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, type WatchAnswer } from '../bridge';
import { currencyOf, usePrograms, WATCH_POLL_MS } from '../programs';
import { runtimeOf, watchOf, type Ages } from './harness';

const runtime = runtimeOf('Tank', { interval_ms: 100 });
const answer = (ages: Ages = {}, mode = 'run') => watchOf({}, mode, ages)(['x']);
const at = (ages: Ages, nowMs: number, mode = 'run') => currencyOf(answer(ages, mode), 10_000, runtime, 10_000 + nowMs);

describe('whether watched values are current', () => {
  it('holds a read for its freshness and one interval of asking, and not a millisecond longer', () => {
    expect(WATCH_POLL_MS).toBe(500);
    // 1000 ms old when it came, fresh for 1000: current until 500 ms after it came.
    expect(at({ age_ms: 1000 }, 500)).toEqual({ current: true, untilMs: 10_500 });
    const late = at({ age_ms: 1000 }, 501);
    expect(late.current).toBe(false);
    expect(!late.current && late.why).toBe(
      'They were read 1501 ms ago and no newer answer has come: an answer is current for 1000 ms, and one is asked for every 500 ms.'
    );
    // The bound is the route's fresh_ms, not a number of this view's.
    expect(at({ age_ms: 1000, fresh_ms: 2000 }, 1500).current).toBe(true);
    expect(at({ age_ms: 1000, fresh_ms: 2000 }, 1501).current).toBe(false);
  });

  it('holds a program in RUN scanning while its last scan is within two intervals and its longest scan', () => {
    expect(at({ scan_age_us: 200_019, max_scan_us: 19 }, 0).current).toBe(true);
    const stalled = at({ scan_age_us: 200_020, max_scan_us: 19 }, 0);
    expect(!stalled.current && stalled.why).toBe(
      'When read, the program had completed no scan for 200.02 ms, past the 200.019 ms within which a program scanning every 100 ms completes one (two intervals and its longest scan): it is not scanning on time.'
    );
    // A slower program, a longer scan: the bound is the program's own.
    const slow = runtimeOf('Tank', { interval_ms: 1000 });
    expect(currencyOf(answer({ scan_age_us: 2_500_000, max_scan_us: 500_000 }), 0, slow, 0).current).toBe(true);
    expect(currencyOf(answer({ scan_age_us: 2_500_001, max_scan_us: 500_000 }), 0, slow, 0).current).toBe(false);
  });

  it('makes values whose age is not said not current, never current', () => {
    const why = (ages: Ages, mode = 'run', of = runtime) => {
      const verdict = currencyOf(answer(ages, mode), 0, of, 0);
      return verdict.current ? 'current' : verdict.why;
    };
    expect(why({ age_ms: undefined })).toBe('The controller does not say how old these values are.');
    expect(why({ fresh_ms: undefined })).toBe('The controller does not say for how long its values are current.');
    expect(why({ scan_age_us: undefined })).toBe('The runtime does not say when the program last completed a scan.');
    expect(why({ scan_age_us: null })).toBe('The runtime does not say when the program last completed a scan.');
    expect(why({ max_scan_us: undefined })).toBe('The runtime does not say how long the program’s longest scan took.');
    expect(why({}, 'run', runtimeOf('Tank', { interval_ms: 0 }))).toBe('The program’s interval is not known.');
    expect(why({ age_ms: -1 })).toBe('The controller does not say how old these values are.');
    // STOP is not scanning: only the read's age counts for what it stopped with.
    expect(why({ scan_age_us: undefined }, 'stop')).toBe('current');
    expect(why({ age_ms: undefined }, 'stop')).toBe('The controller does not say how old these values are.');
  });

  it('takes the interval only from the runtimes of the version that answered', () => {
    // The list still names version 1, every second; version 2, loaded since, scans every 10 ms
    // and has completed no scan for 1.5 s: against the old interval that would pass.
    const listed = runtimeOf('Tank', { version: '1', interval_ms: 1000 });
    const v2 = answer({ age_ms: 0, scan_age_us: 1_500_000 });
    v2.state.version = '2';
    const verdict = currencyOf(v2, 0, listed, 0);
    expect(verdict.current).toBe(false);
    expect(!verdict.current && verdict.why).toBe(
      'The runtimes were read of version 1 and the values are of version 2: the interval of the version that answered is not known yet.'
    );
  });
});

describe('what the editor keeps of a watch', () => {
  let clock = 1_000_000;
  const held = () => usePrograms.getState().watches.Tank;
  const valueOf = (value: number, ages: Ages = {}) => watchOf({ x: { type: 'INT', value } }, 'run', { age_ms: 0, ...ages })(['x']);
  /** Each next call of `api.watch` answers when the returned function is called with an answer, or an error. */
  const pending = () => {
    const answers: Array<(a: WatchAnswer | Error) => void> = [];
    vi.spyOn(api, 'watch').mockImplementation(
      () =>
        new Promise<WatchAnswer>((resolve, reject) => {
          answers.push((a) => (a instanceof Error ? reject(a) : resolve(a)));
        })
    );
    return answers;
  };

  beforeEach(() => {
    clock = 1_000_000;
    // One clock for both, so that what is measured does not depend on which one the store reads.
    vi.spyOn(performance, 'now').mockImplementation(() => clock);
    vi.spyOn(Date, 'now').mockImplementation(() => clock);
    usePrograms.getState().reset();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    usePrograms.getState().reset();
  });

  it('counts the time an answer spent coming back: a read is as old as when it was asked', async () => {
    const answers = pending();
    const reading = usePrograms.getState().readWatch('Tank', ['x']);
    // Composed at once (age 0), then held five seconds on its way back.
    clock += 5_000;
    answers[0]?.(valueOf(1));
    await reading;
    const watch = held();
    expect(watch?.answer?.values[0]?.value).toBe(1);
    const verdict = currencyOf(watch!.answer!, watch!.sentAtMs!, runtimeOf('Tank'), clock + 100);
    expect(verdict.current).toBe(false);
    expect(!verdict.current && verdict.why).toContain('They were read 5100 ms ago');
  });

  it('applies only the newest answer: one asked earlier that comes back later is dropped, and so is one after a drop', async () => {
    const answers = pending();
    const read = usePrograms.getState().readWatch;
    const first = read('Tank', ['x']);
    clock += 100;
    const second = read('Tank', ['x']);
    answers[1]?.(valueOf(2));
    await second;
    answers[0]?.(valueOf(1));
    await first;
    expect(held()?.answer?.values[0]?.value).toBe(2);
    expect(held()?.sentAtMs).toBe(1_000_100);

    // An earlier request that fails after a later one answered does not take its values away.
    const third = read('Tank', ['x']);
    const fourth = read('Tank', ['x']);
    answers[3]?.(valueOf(4));
    await fourth;
    answers[2]?.(new Error('the runtime’s module does not answer'));
    await third;
    expect(held()?.answer?.values[0]?.value).toBe(4);

    // Nothing asked before a drop is shown after it.
    const fifth = read('Tank', ['x']);
    usePrograms.getState().dropWatch('Tank');
    answers[4]?.(valueOf(5));
    await fifth;
    expect(held()).toBeUndefined();
  });
});

