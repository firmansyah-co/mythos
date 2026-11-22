# ADR-013 evidence: SD, SL and P0

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **SD, SL and P0** (the three qualifiers refused until the owner decided, 2026-11-22: taken as
  Table 59 names them, §2.17). Changed: `sfc.rs` (`Qual::Sd`, `Sl`, `P0`; for each SD and SL
  association a flip-flop, a TON and a duration kept, `__SFC_<action>_SD_<localId>_FF`, `_TMR`,
  `_KEPT`, and for SD `_WAS`; the latches `__SFC_<action>_SD_START` and `_SL_END`; P0's edge in
  `__SFC_<action>_P0_WAS` and `_P0_PULSE`; rule d counts SD and SL); in
  `frontend/packages/logic-ui`, `QUALIFIERS` is Table 59's eleven in its order, the action table
  and the Instructions card offer them, and the editor's own check no longer refuses the three.
  No route was added, so no MCP tool. Tests: `sfc_runs_on_the_vm.rs` (48, 16 new);
  `sfc_refusals.rs` (39: the refusal's test removed); `sfc_from_the_editor.rs` (7, 1 new:
  `editor_qualifiers.xml`, a chart with the three written by the editor's own writer in
  `sfcmodel.test.ts`, compiled and run: purge `0000110`, horn `0111000`, left `0010000` over
  seven scans); `sfcmodel.test.ts` (32) and `sfc-editor.test.tsx` (9: the action table offers
  the eleven).
  **Found in an independent review of the PR (head 140b9d5), each fixed with a named test:**
  (1) the duration kept was one variable per action, `__SFC_<action>_SD_T`, which is also the T
  of an action named `<action>_SD`: its `T#0s`, written every scan, ended the delay at once (a
  100 ms delay took 20 ms); the names carry the association's localId and no part is named `T`
  (`an_action_named_as_a_part_of_another_action_s_control_shares_nothing_with_it`). (2) With one
  duration and one timer per action, a later SD step's shorter duration started the action
  before the delay of the step that stored it (`an_sd_action_two_steps_store_starts_only_when_the_delay_of_each_is_over`),
  and (3) a later SD step's longer duration stopped an action that had started, with no R
  (`an_sd_action_that_has_started_stays_active_until_r_whatever_stores_it_again`): each
  association is stored and timed by itself, and started and ended are kept until R. With
  them, decided for the same cases: a step entered again starts its SD delay again
  (`an_sd_step_entered_again_while_its_delay_runs_starts_the_delay_again`); an SL action ends
  at the first limit over and stays ended (`an_sl_action_ends_at_the_first_limit_that_is_over_and_stays_ended_until_r`,
  `an_sl_action_that_has_ended_stays_so_when_its_duration_grows`, and
  `an_sd_action_that_has_started_stays_so_when_its_duration_grows` beside it). (4) Two
  P0 steps one after the other pulse once, when the last is left: kept as Figure 23 has it,
  said in §2.17 and pinned (`several_p0_associations_of_an_action_are_one_falling_edge_as_figure_23_has_them`).
  The reviewer's own repro file, run on the fix: the three of (1) to (3) pass, the one of (4)
  fails as it says. **A second review, of the fix (head 5802520), found no action running where
  it should be off and no name colliding;** it found the rule of the step entered again
  written wider than built (a step that loops to itself does not start its SD delay again: its
  flag never falls, as for D and every qualifier), now said as built and pinned
  (`a_step_that_loops_to_itself_is_not_entered_again_for_an_sd_delay_or_a_p0_pulse`), and the
  sentence on Figure 23's SD corrected. It also noted that a negative duration starts an SD
  action in the scan it is stored, as a TON does for D, L and DS: not refused, not changed
  here.
  Mutation-checked in a copy of the commit (`git archive`, a target directory of its own, each
  injection checked as landed, the file restored and the tests of the two Rust files green
  before and after), eleven mutations, each red: rule c's `T#0s` once the step is left (8
  tests); P0 as Figure 23's F_TRIG (5); P0's memory, and an association's flip-flop and timer,
  not cleared when the chart starts again (`a_chart_started_again_by_a_build_of_another_chart_runs_no_p0_action_and_drops_an_sd_delay`,
  each); SL never ended by a limit (4); SD and SL timed by the step and not by what was stored
  (7); R not resetting an association's flip-flop (`an_r_association_ends_an_sd_delay_and_the_next_one_runs_in_full`);
  no restart of the delay when the step is entered again and an SD action starting when any
  delay is over (the test of each, above); started and ended not kept (the two tests of a
  duration that grows, and the started action's); one duration per action
  again (3, the collision's among them). Not done: no ST twin of the three in
  `sfc_equals_st.rs` (the traces above are their statement), and P0 was not compared with
  another tool.
