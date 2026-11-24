# ADR-013 evidence: the runtimes editor and the Library palette in a real browser

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **E2E of the runtimes editor and the Library palette** (two groups of cases in
  `frontend/e2e/workbench.mjs`, run by `scripts/e2e.sh`, 2026-11-24). Test tooling only: no
  product file, no route and no MCP tool changed; `scripts/e2e.sh` is unchanged, since every
  binary and file the cases need was already built and installed by it. The cases drive the
  workbench in headless Chromium (1440 x 900) against a real `mythos-server`, the logic module,
  the VM runtime module and the simulated Modbus device, as the rest of the script does.

  **Where they run, and why there.** The groups are 8j and 8k, placed after 8b and before 8c.
  On `dev` (`3e0b135`) the script stops in 8c: the program editor's Load is refused with
  "400 Bad Request: the body is not what this route takes: unknown field `created`", and the
  script then throws at "Force twice" (`Cannot read properties of null (reading 'click')`), so
  nothing after 8c runs (finding F2). Placed after 8i, as first written, the groups never ran
  (run 1). Before 8c they run, and they leave the project as they found it: the runtime and the
  library source are deleted through the page, and the three programs they make (Counter,
  Rung10, Flow10) over `DELETE /api/v1/projects/plant_a/logic/programs/<name>`, since the page
  has no command that deletes a program (said in a comment). The 8d check that lists every
  program's language is then unaffected. Run 1 (groups not executed) and run 3 (groups
  executed) print the same failures from 8c on, the same ten lines and the same throw.

  **What the cases do.**
  - 8j, the runtimes editor (PR #196): a program Counter (ST) is written and saved; "Add new
    runtime" in the tree's Runtimes section, id `line1`; module `vm` chosen, target `pc`, "Run
    when loaded"; a task `fast`, cyclic, 100 ms, priority 1, running Counter from "Add program";
    Save; Check; Load (which checks, builds and asks before it saves the core's runtimes); the
    Runtime task card read; Stop, Run and Unload on the card; Delete. Every question the page
    asks (`window.confirm`) is answered yes and its words are checked.
  - 8k, the Library group (PR #198): a library source `Machines` with a function block `Big10`
    of ten inputs (`speed_set: INT`, `k_run`, `a_stop: BOOL`, `m_level`, `b_temp: REAL`, `x_ok:
    BOOL`, `c_mode: INT`, `y_fault`, `d_reset`, `e_ack: BOOL`, not in name order) and a function
    block `Conveyor_Speed_Controller_With_Long_Name_01` (43 characters) is saved in the Library
    editor; an LD program `Rung10` and an FBD program `Flow10` are made from the tree; the Library
    group is opened by its summary (it is closed at first); Big10 is put by Ctrl+R and a click on
    its item, its instance declared with Enter; the source is saved with a mistake (held, then
    "Save anyway"), then corrected; then saved without `e_ack`.
  - Measures. `reach(el)` scrolls the element into view as far as its boxes let it
    (`scrollIntoView`, nearest) and gives how far it still lies past the window or past any
    ancestor whose overflow is not `visible`, on both axes, with that box's name. The card
    measure gives the distance from each edge of the item (or line) to the same edge of the
    `module-task-card` it is in; a negative number is past the card. Every screenshot runs the
    script's existing survey (no control past the window or a box that cuts it off, horizontal).

  **Each added check, as run 3 printed it** (`./scripts/e2e.sh`, exit 1, "14 CHECK(S) FAILED";
  54 checks added, 51 PASS, 3 FAIL; the other 11 failures are 8c's, finding F2).
  8j:
  - PASS "a program Counter is written and saved in the program editor".
  - PASS "the logic module adds Runtimes to the project tree, with a row to add one, and says
    none is configured": "No runtime is configured: every program runs alone."
  - PASS "a runtime added from the tree opens in the runtime editor, NOT SAVED YET, with no
    task": `NOT SAVED YET / Boiler HouseRuntimesline1`.
  - PASS "the module field offers the installed VM runtime module": `["","vm"]`.
  - PASS "the task offers the project's programs, Counter among them": `["Counter"]`.
  - PASS "the form is complete: the task runs Counter, nothing is said to be missing, Save and
    Load are offered".
  - FAIL "e2e_11a_runtime_editor: no control is past the edge of the window or of a box that
    cuts it off": `["task-priority @logic-runtime__form/auto 13px"]` (finding F1).
  - PASS "Save stores the runtime in the logic module and says nothing was built or loaded":
    "Saved; the module made the runtime. Nothing was built or loaded.", and the module's
    `GET /runtimes/line1` answers module `vm`, target `pc`, run true, the task `fast` cyclic
    100 ms phase 0 priority 1 with programs `["Counter"]`.
  - PASS "the tree lists the saved runtime": `["line1"]`.
  - PASS "Check: the module says the runtime checks": `CHECKS`, no problem listed.
  - PASS "the Runtime card says it is configured, not loaded": NOT LOADED, "The controller lists
    no runtime line1: it is configured, not loaded."
  - PASS "Load builds it and asks before it puts the runtime into the controller's runtimes":
    one question, "Load runtime line1? ... Counter: version 79628f694ffc to load; runs now: none:
    the controller runs no build of it ... It is put in RUN".
  - PASS "the controller applied it, as the result says in the core's words": "Saved into the
    project's runtimes and applied by the controller. ..."; start: "Not changed online: no build
    of it was loaded. ..."; no `load-failed` line.
  - PASS "the Runtime card says RUN as the controller lists it, the program at the version built
    here": RUN, `Counter · version 79628f694ffc`, "As the controller lists it.", "Counter: the
    same as the one built here"; the API lists `line1` mode run, scans 2.
  - FAIL "e2e_11b_runtime_loaded: ..." and FAIL "e2e_11c_runtime_unloaded: ...": the same
    `task-priority @logic-runtime__form/auto 13px` (F1).
  - PASS "Stop on the card puts it in STOP, and the controller says the same": STOP, "Runtime
    line1 was asked to stop."
  - PASS "Run on the card asks first, names the program, and it runs again": RUN, "Put runtime
    line1 in RUN? It runs Counter. ..."
  - PASS "Unload on the card asks, says the outputs go to their safe values, and takes it out of
    the controller's runtimes": NOT LOADED, "Runtime line1 was taken out of the project's
    runtimes. As stored when it was taken out, it held Counter."
  - PASS "Delete asks first, closes the editor, and the tree again says no runtime is
    configured".
  8k (card numbers: left, right, top, bottom in px from the card's edges; Instructions card
  260 x 707 in LD, 260 x 603 in FBD):
  - PASS "B1: the long block name has 40 characters or more": 43.
  - PASS "a library source added from the tree opens in the library editor".
  - PASS "the library source with Big10 (ten inputs) and the long-named block is saved through
    the Library editor": "Saved; the module made the source."
  - PASS "B1: the Library group of the Ladder palette offers both blocks":
    `["box:Big10","box:Conveyor_Speed_Controller_With_Long_Name_01"]`.
  - PASS "B1: the item Big10 and its favourite star lie inside the Instructions card (left,
    right, top, bottom)": group open, visible; item 8 / 30 / 630.9 / 50.2, width 222; star
    230 / 8 / 632.9 / 52.2, width 22; star reached, 0 px past any box.
  - PASS "B1: the item Conveyor_Speed_Controller_With_Long_Name_01 and its favourite star lie
    inside the Instructions card (left, right, top, bottom)": item 8 / 30 / 656.9 / 24.2, width
    222; star 230 / 8 / 658.9 / 26.2; the name is shortened with an ellipsis
    (`nameCutByEllipsis: true`; observation O1).
  - PASS "e2e_11d_library_palette: no control is past the edge ...": `[]`.
  - PASS "B2: Big10 from the Library group is placed in a rung, its instance proposed":
    `["ld-box-instance","big101"]`.
  - PASS "B2: in the rung the box shows ten input pins in declaration order (fields and drawing;
    the drawing adds EN)": fields `SPEED_SET, K_RUN, A_STOP, M_LEVEL, B_TEMP, X_OK, C_MODE,
    Y_FAULT, D_RESET, E_ACK`; drawing `EN` then the same ten.
  - PASS "B2: in the rung every pin field and the box lie inside the editor or can be scrolled
    to, cut by no box": box 100 x 253 at (371, 238), 0 px past; 10 fields, none cut.
  - PASS "e2e_11e_library_block_ld: ...": `[]`.
  - PASS "B2: the rung with Big10, unsaved until then, is saved".
  - PASS "B2: the FBD palette's Library group offers both blocks too".
  - PASS "B2: in the FBD palette the item Big10 and its star lie inside the Instructions card":
    item 8 / 30 / 526.9 / 50.2; star 230 / 8 / 528.9 / 52.2.
  - PASS "B2: in the FBD palette the item Conveyor_Speed_Controller_With_Long_Name_01 and its
    star lie inside the Instructions card": item 8 / 30 / 552.9 / 24.2; star 230 / 8 / 554.9 /
    26.2; ellipsis as in LD.
  - PASS "B2: Big10 is placed in an FBD network, its instance proposed":
    `["fbd-box-instance","big101"]`.
  - PASS "B2: in the network the box shows ten input pins in declaration order (fields and
    drawing, then Q)": the ten in order in the fields; drawing the ten, then `Q`.
  - PASS "B2: in the network every pin field and the box lie inside the editor or can be scrolled
    to, cut by no box": box 100 x 233 at (331, 238), 0 px past; 10 fields, none cut.
  - PASS "e2e_11f_library_block_fbd: ...": `[]`.
  - PASS "B2: the network with Big10, unsaved until then, is saved".
  - PASS "B3: a source that does not check is held, then saved anyway after asking": "Saved.",
    after "Save Machines although the library does not check with it? ..."
  - PASS "B3: the Library group says why it offers nothing: the library does not check": "The
    library does not check (1 diagnostic); its blocks are offered once it does.", 0 items.
  - PASS "B3: the group's line lies inside the card, not cut": group open, visible; 8 / 8 / 643
    / 36, 244 x 32 (two lines), not cut, 0 px past any box.
  - PASS "e2e_11g_library_does_not_check: ...": `[]`.
  - PASS "B3: corrected, the blocks are offered again without reloading the page": "Saved.",
    both items visible again.
  - PASS "B4: Big10 without E_ACK is saved in the library": "Saved."
  - PASS "B4: the box in the rung says the pin E_ACK is not a pin of Big10 as it is declared
    now": `["E_ACK is not a pin of Big10 as it is declared now."]`.
  - PASS "B4: and the rung's program is not marked unsaved".
  - PASS "e2e_11h_library_block_changed_ld: ...": `[]`.
  - PASS "B4: the box in the network says the pin E_ACK is not a pin of Big10 as it is declared
    now": the same sentence.
  - PASS "B4: and the network's program is not marked unsaved".
  - PASS "e2e_11h_library_block_changed_fbd: ...": `[]`.
  - PASS "the library source is deleted through its editor, after asking".
  - PASS "the programs made here are deleted, and the tree lists no program again, with the
    screen of 7 open": `Rung10 200, Flow10 200, Counter 200 / []`.

  **Evidence 34's "NOT VERIFIED, nothing was looked at in a browser", item by item.**
  - A library block with a long name in the Instructions card, the star inside the card:
    VERIFIED in LD and FBD (numbers above). The name itself is shortened with an ellipsis (O1).
    "The suggestion list" (the box type field's list): NOT VERIFIED, it was not opened.
  - A function block with ten inputs in an LD rung and an FBD network (box height, the rung's
    row, the pin fields): VERIFIED that the box is drawn 100 x 253 (LD, with EN) and 100 x 233
    (FBD), its pins in declaration order, and that the box and every pin field can be scrolled
    into view and are cut by no box. How the pin fields wrap was looked at in the screenshots
    only, not measured.
  - The group's line in the narrow `<details>`: VERIFIED (B3, two lines, inside the card).
  - The flicker while the store's check is null after a library change in another tab (H12):
    NOT VERIFIED; the cases use one tab.

  **Findings.**
  - F1 (LIVE, PR #196's form). At 1440 x 900 the runtime editor's task row is wider than its
    form: the Priority field (`task-priority`) ends 13 px past the right edge of
    `.logic-runtime__form`, a box with `overflow: auto` (the script's survey, in all three
    screenshots of 8j). The screenshot `e2e_11a_runtime_editor` also shows the Interval field
    running under the Phase field beside it; that overlap was seen, not measured. The check is
    kept and fails.
  - F2 (LIVE, older than these cases). The program editor's Load is refused by the logic
    module: `saveSettings` (`frontend/packages/logic-ui/src/bridge.ts`) sends the whole program
    entry except `name` and `language`; the entry the module answers carries `created` since
    #192 (`git log -S`), and `PUT .../settings` refuses unknown fields
    (`deny_unknown_fields` on `ProgramEntry`). 8c's ten checks fail and the
    script throws there, so 8d to 10 (LD, FBD, SFC, Settings) do not run on `dev`. Not fixed
    here (product code).
  - O1 (observation, not a check). In the palette the 43-character name is shortened with an
    ellipsis (`scrollWidth > clientWidth`); its full name is in the button's title. No check
    fails on it.
  - O2 (observation, not a check). The Library group, opened by the engineer, is closed again
    when the card is drawn anew (on a change of editor tab), so a block saved in the library is
    not seen until the group is opened again. B3 opens it again before it measures.

  **What was not run.**
  - Only Linux (CachyOS), Chromium headless, Node 24, one window size (1440 x 900). Not Windows
    or macOS.
  - `scripts/e2e.sh` is not part of `scripts/check.sh` or of CI; these cases run only when the
    script is run by hand.
  - `./scripts/check.sh` was not run: no product file changed. `pnpm lint` does not cover
    `frontend/e2e/` (it typechecks the five packages only), so it was not run.
  - The runtime editor's Load cold, Discard, a refused save, a viewer's read-only form, and a
    second tab were not driven.

  **What was done about the findings, and the run after it** (the orchestrator; the text above is
  of run 3, on dev as it was, with the product untouched). The cases found three defects of the
  product. All three are corrected in the same pull request as the cases, each in a commit of its
  own, and the whole script was run again on top of them.
  - F2, LIVE since #192: a save of a program's settings was refused by the module ("unknown
    field `created`"), so Load from the program editor failed once a setting had changed.
    `saveSettings` (`bridge.ts`) now sends exactly the seven keys the route takes. The test that
    held the body's keys had passed because the fake of the route in the test harness had no
    `created` in its entries and refused nothing: the fake now carries `created` and refuses an
    unknown key in the module's words. Mutation (the old body sent again): red in "keeps an edit
    unsaved until it is saved, and saves only what changed".
  - F1, LIVE: in the runtime's form a number field was wider than its column, lay over the next
    field and passed the form's edge (Priority by 13 px at 1440 x 900; Interval under Phase).
    A field of `.logic-fields` now keeps to its column (`logic.css`: `min-width: 0` on the field,
    its text, number and select controls as wide as the field). No DOM test can see it; the
    three "no control is past the edge" checks of group 8j are its test.
  - F3, LIVE, found by run 4: a program deleted elsewhere stayed in the tree. While the list of
    programs was being read again, a further change (a second and third program deleted by
    another client) was dropped, and the answer that came back was older than the project. A
    change that arrives while the list is read now asks for it again, and only the newest read is
    applied (`entry.tsx`, `programs.ts`). Test: "follows two programs deleted elsewhere one
    after the other, and lists none that is gone" (the first answer held back). Mutations (an
    older answer applied over a newer one; a change during the read dropped): both red in it.
  - **Run 4**, on the first two corrections: 306 checks passed, 1 failed ("the tree says each
    program's language" listed `Counter`, deleted three checks earlier: F3). Sections 8c to 10
    ran again for the first time since #192.
  - **Run 5**, on all three: `./scripts/e2e.sh` printed "ALL CHECKS PASSED", "VERDICT: PASS",
    exit 0; 307 checks, among them the 54 of groups 8j and 8k, the three edge checks of 8j with
    an empty list, and "the programs made here are deleted, and the tree lists no program
    again" with `[]`.
  - `./scripts/check.sh --frontend` on the three corrections: "PASS — every gate green", 59 test
    files, 888 tests. `cargo test -p mythos-core --test version_agreement_tests` 3 passed (the
    logic module and its UI are 0.13.1).
  - **Read by one reader only:** the three corrections were written by the orchestrator and read
    by no reviewer; their tests, their mutations and run 5 are their check.
  - Not corrected: O1 and O2 above (a long name cut with an ellipsis; the Library group closed
    again when the card is drawn anew), which are for the owner's visual pass. Still NOT
    VERIFIED of evidence 34: the suggestion list of a box's type, how pin fields wrap, and the
    flicker after a library change in another tab.
  - Not run: the full gate, the Rust suites (one version line of Rust changes), Windows and
    macOS. CI is the gate; `scripts/e2e.sh` is still not part of it.
