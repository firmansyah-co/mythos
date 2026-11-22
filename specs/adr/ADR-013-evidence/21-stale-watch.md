# ADR-013 evidence: Stale watch

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Stale watch** (only current values drawn live, found in review of L3d-2, 2026-11-22). A
  runtime that says RUN and has stopped scanning answered a watch with its last values and a
  fresh `age_ms` (the core's age of its read, not of the scan), and every view drew them as
  current, as it did an answer held while asking got none newer. Added: `RuntimeState::scan_age_us`
  (`mythos-module`, optional on the wire), filled by the VM module from the instant its last scan
  completed and absent before the first; `fresh_ms` in the watch answer; MCP `watch_runtime`
  says how to read both; `currencyOf` and the editor's `useCurrency` (§2.8), drawing values that
  are not current UNKNOWN with why in the LD, FBD and SFC views and the variables table. Tests:
  `currency.test.ts` (3: each bound at its exact edge, what is not said), a table of ten answers
  through each view and the table (`CURRENCY_CASES`: a scan just inside two intervals and the
  longest scan, the counter held and its age 1 µs past, the counter on again, a read just past
  1.5 s, one of a second, and an answer without `age_ms`, `fresh_ms`, `scan_age_us` (absent and
  null) or `max_scan_us`), a controller that answers nothing (watch and runtimes left hanging) in
  each view and the table, and a stopped program's values kept only while the read is recent;
  `the_state_says_how_long_ago_a_scan_completed_and_nothing_before_the_first` (the module);
  `runtime_api.rs` asserts `fresh_ms` and a `scan_age_us` from the real module process. Not
  covered end to end: `scripts/e2e.sh` cannot stall a scan thread cheaply (the module has no
  such hook, and adding one to a safety runtime for a test was not done); it runs unchanged
  against the real server, every live sample of the LD, FBD and SFC scenarios still live: 237
  checks, all passed. Mutation-checked in a copy of the tree (`git archive` of the commit, its own
  `pnpm install --frozen-lockfile` and `pnpm lint`, each injection checked as landed, each file
  restored with `git show`; the copy passed 177 before and after): the check removed from
  `liveOf` turned the not-current and the hanging test of each of `ladder.test.tsx`,
  `fbd.test.tsx` and `sfc.test.tsx` red (`draws UNKNOWN, never a state, from values that are not
  current, and live again once they are`, `… never a value …` for FBD; `stops drawing the last
  values as live when no newer answer comes`, `… the last state …` for SFC); the check removed
  from the variables table turned its three red (`says UNKNOWN for values that are not current,
  and shows them again once they are`, `shows what a stopped program stopped with only while it
  was read lately`, `stops showing the last values when no newer answer comes`); the scan bound
  not applied turned `holds a program in RUN scanning while its last scan is within two intervals
  and its longest scan` and the four table tests red; the read's age not applied turned `holds a
  read for its freshness and one interval of asking, and not a millisecond longer`, the four table
  tests, the four hanging tests and the stopped program's red; a scan age or an age not said
  taken as current each turned `makes values whose age is not said not current, never current`
  and the four table tests red; the module's `scan_age_us` given before the first scan turned its
  test red. Not judging again at the instant an answer stops being current survived at first:
  the runtimes' answers each second redrew the editor; the hanging tests now leave both requests
  unanswered, and the mutation turns the four of them red. One run of the copy under a load
  average of 34 (another worktree's build) timed out every test of `fbd.test.tsx` and
  `fbdmodel.test.ts`'s mixer (untouched here; 0.3 s alone); run again at load 7 it passed 177.
  **Found in an independent review of the PR (head 8874be6; the Rust side found correct), each
  in the dangerous direction, fixed:** (A) an answer was aged from when it came, so time in
  transit did not count (asked at T, `age_ms` 0, held 5 s on its way back: current 1.4 s more);
  it is aged from when it was asked. (B) the clock was `Date.now()`: the wall clock set back an
  hour kept held values current and put the re-check off an hour; every age is on
  `performance.now()`. (C) the interval came from the runtimes list whatever its version (a list
  of version 1 at 1000 ms judged version 2's answer, 10 ms, 1.5 s without a scan, current); a
  list of another version than the answer's makes the values not current. (D) polls overlap and
  an older answer coming back last replaced a newer one, stamped as just come; requests are
  numbered and only the newest is applied (the runtimes list likewise). And, adjacent: a runtimes
  read left unanswered left RUN and the last scan figures on the Runtime card unmarked; one still
  out 3 s after the last answer now marks them LAST KNOWN (§2.6). The review's four repros became
  named tests, each red on 8874be6's `programs.ts` and `ProgramEditor.tsx` and green after (a copy
  of da53798 with those two files put back; the other 55 tests of the two files green):
  `counts the time an answer spent coming back: a read is as old as when it was asked`, `ages
  what it read on a monotonic clock: a wall clock set back an hour keeps nothing current`, `takes
  the interval only from the runtimes of the version that answered`, `applies only the newest
  answer: one asked earlier that comes back later is dropped, and so is one after a drop`, and
  `marks the runtime last known when the runtimes go unanswered, and shows no value`. The hanging
  tests now wait for the watch rule's own reason (both requests hang, and the runtimes' rule also
  ends in UNKNOWN, 3 s after their last answer; the watch's, due at least a second earlier, is
  the one they prove). Mutation-checked again in a copy of da53798 (182 before and after): the
  seven mutations above, each still red; the stamp taken on arrival turned `counts the time an
  answer spent coming back …` red; the wall clock for the monotonic one turned `ages what it read
  on a monotonic clock …` red; the interval of any version turned `takes the interval only from
  the runtimes of the version that answered` red; every answer applied turned `applies only the
  newest answer …` red; an unanswered runtimes read never marked turned `marks the runtime last
  known when the runtimes go unanswered, and shows no value` red. Under a load average of 10–20
  (other worktrees' builds) the mixer of `fbdmodel.test.ts` timed out in three of those runs, and
  only there.
