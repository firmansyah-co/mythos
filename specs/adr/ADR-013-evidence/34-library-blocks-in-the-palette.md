# ADR-013 evidence: library blocks in the palette

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Library blocks in the palette** (the project's own functions and function blocks offered and
  drawn with their pins in the LD and FBD editors, 2026-11-24). The Instructions card and the
  favourites bar of both editors now offer the functions and function blocks of the project's
  library in a group of their own, "Library", and a box of one is drawn with the pins the logic
  module answered. No route and no MCP tool was added: `POST /library/check` (tool
  `check_logic_library`) answers `blocks` when the library checks, and the tools list stays at 19.
  The slice is the commits a974674 (the module's answer), 411dd79 (the palette and the boxes) and
  de1df87 (what the two reviews found). It is based on `c8d1-base` (f0851d8, PR #196).

  **What the slice adds.**
  - **The answer.** `mythos_logic_compiler::library_blocks(sources)` checks the project as
    `check_project` does (the Structured Text check, then `data_layouts`), refuses exactly what
    `check_project` refuses, and answers each function and function block of the checked unit that
    the standard library does not declare, in unit order, as `LibraryBlock {name, function_block,
    inputs, outputs, result}`. Inputs are the VAR_INPUT variables and outputs the VAR_OUTPUT ones,
    hidden variables left out, in declaration order; types are written by `Unit::ty_name`
    (`STRING[12]`, `REAL`, a structure's declared name); the result is the function's type, none
    for a function block. The filters are in a private `blocks_of(unit)`, so that the hidden rule
    can be tested over a hand-made unit. A PROGRAM is never answered. `check_library` in the logic
    module calls `library_blocks` in place of `check_project` and, only when `ok` is true, adds
    `"blocks": [{name, kind: "function"|"function_block", inputs: [{name, type}], outputs, result}]`
    through a private `block_json`. When `ok` is false the body is as before, with no `blocks` key.
    The order of the sources is by file name, then the order of their declarations.
  - **The group.** `libraryblocks.ts` maps the answer to `BoxDef`s of group "Library". A function
    block's box has its answered outputs; a function's box has its inputs and `OUT` of its result
    type. Pin names are upper-cased, as the document's own POUs are (the module answers the
    declared spelling); types are kept exactly as answered. `callable(own, library)` puts the
    document's own boxes first, then the library's. The group stands between the standard groups
    and "This document" in both kits, is closed by default, and each `<details>` has
    `data-testid="<p>-palette-group"` and `data-group=<title>`. The card and the favourites bar
    read the library store themselves, so the group follows the store's current state.
  - **A list that was not answered is said, never shown empty.** The group shows one line
    (`<p>-palette-library-why`) and no item when the check has not answered ("The library is being
    checked; its blocks are offered once it answers."), when the check could not be made, when the
    list of the library could not be read ("The library could not be read: <error>; its blocks are
    not offered."), when the library does not check (with the count of diagnostics, even if
    `blocks` came with it), when an ok answer has no `blocks` ("This logic module does not answer
    the library's blocks.") and when an entry has another shape ("The library's blocks were
    answered in a shape this workbench does not read."). `blocks: []` says "The project's library
    declares no function or function block." The shape check is strict and one bad entry makes the
    whole list unknown, never a partial list.
  - **Placing.** A library function block is placed with its pins and an instance (proposed with
    `instanceName`, declared in VAR as the derived type); a function with its pins and none. LD
    gets its power pins by `pinsOf` as for a standard block, FBD gets no EN or ENO. Typing a
    library block's name into an empty box gives it its pins.
  - **Types are the current declaration's.** The model takes a library pin's type as ANY when a
    text is opened (it has no library). `ldedit.declarationFor` and `ldedit.check`, and the FBD
    check, `orAt` and the declared type of an assigned variable, take the type from the block as
    it is declared now (`declaredType`), for inputs and for outputs. The model is not rebuilt, the
    session is not reopened and undo is kept.
  - **A changed or deleted block is said.** `unknownBoxWords`: with the library known, "<T> is not
    a function or function block of this program or of the project's library, nor a standard one
    this editor knows."; with it not known, the D4c sentence unchanged and a second line, "The
    project's library's blocks are not known here: <why>". `pinProblems` says "<X> is not a pin of
    <T> as it is declared now." and "<T> declares the input <X>, which this box does not have."
    (EN and ENO aside), for library blocks and for the document's own. LD says a power pin the
    declaration retyped; both editors say the instance a box keeps when its block is a function
    now, and show the instance field for it so it can be cleared.
  - **No diagram is rewritten.** The stored text is not changed by a library change; the program
    is not unsaved after it. No action takes a changed declaration's pins (section 9 item 3).

  **Rules that keep the screens truthful.**
  - A block's pins are those the module answered. The UI reads no Structured Text, infers no
    interface from a diagnostic, defaults no type and guesses no pin.
  - Blocks are offered only from an answer with `ok` true; a library that does not check offers
    none, and the module's words are the diagnostics.
  - A block answered with a standard name, or one name twice in any case, makes the whole list
    unknown (the module refuses both today, so this is LATENT), so the standard TON is never
    replaced by a library entry.
  - A library block named like one of the document's own POUs is not offered; the group says so
    in `<p>-palette-library-twins`, and a box of the document's block says the library declares
    the same name (the compiler refuses the name declared twice, at Check).
  - A project's blocks do not outlive a project switch (the store's reset and numbered answers).

  **Tests, by name.**
  Compiler, `crates/mythos-logic-compiler/tests/library_blocks.rs`:
  - `a_library_that_checks_answers_each_block_with_its_inputs_and_outputs_in_declaration_order`:
    two sources (`A_Valves.st`, `B_Pumps.st`), blocks Zvalve, Mvalve, Apump, pins out of name
    order with interleaved sections and `STRING[12]`; the exact order and types, so the order of
    the sources is kept across sources (H14).
  - `a_function_answers_its_result_type_and_no_outputs`: result REAL, outputs empty, a local VAR
    is no pin.
  - `a_block_of_the_standard_library_a_library_fb_uses_is_not_answered`: first that TON and
    R_TRIG are in the checked unit, then that only the library's Debounce is answered.
  - `a_library_that_does_not_check_answers_its_diagnostics_and_no_block`: exactly the
    diagnostics of `check_project`.
  - `a_library_of_types_alone_answers_an_empty_list`: a TYPE-only source and an empty library.
  - `tests::a_variable_the_front_end_made_is_not_a_pin` (unit test in `src/lib.rs`): a hand-made
    unit with a hidden input and output; no Structured Text can make a hidden variable.
  Logic module, `plugins/logic/src/lib.rs`:
  - `tests::a_library_that_checks_answers_each_block_with_its_inputs_and_outputs_in_declaration_order`:
    the exact body with a data type, function Scale and function block Valve (a structure input).
  - `tests::the_drafts_blocks_are_answered_not_the_stored_ones`: a draft answers its own blocks,
    a new name is put in by file name, and the stored library is unchanged afterwards.
  - `tests::a_library_that_does_not_check_answers_no_blocks`: the compiler's refusal and the
    module's own refusal (a source that is not UTF-8) both answer the keys `diagnostics`, `ok`.
  - `tests::a_library_of_types_alone_answers_an_empty_list`: the exact body with `blocks: []`.
  - Changed, not new: `tests::a_library_name_is_one_name_without_regard_to_case_and_never_a_device`
    and `tests::a_library_source_that_is_not_text_refuses_programs_and_its_own_routes_mend_it`,
    whose exact bodies of an ok check gained `blocks`.
  Workbench, `frontend/packages/logic-ui/src/__tests__`:
  - `libraryblocks.test.ts` (kind and result mapping, types kept as written, every R6 case, the
    list read failing, ten malformed entries each after a good one, a standard name, one name
    twice, the R11 words with the library known, not known and not given, `pinProblems`, the same
    pins and power pins as a document POU of the same declaration in LD and FBD, the group order,
    a twin of the document's, EN as a declared pin, Join with OR at a library pin, a retyped
    power pin, a function block that became a function, the output direction of the declared
    type, a drifted box of the document's own POU).
  - `library-palette.test.tsx`, for LD and FBD each: the group, its lines, a library saved while
    the editor is open (undo kept), placing a function block and a function, typing a name, an
    operand of the wrong family and a declared type, a deleted block, a block that lost a pin or
    gained an input, no diagram rewritten, a block of a project closed before, a twin, a list
    that could not be read, the instance a box keeps.
  - `ldmodel.test.ts` and `fbdmodel.test.ts`: the editor writes `ladder_library/editor_library.xml` and
    `fbd_library/editor_library.xml` with the module's boxes, held equal to the committed fixture, a
    missing fixture failing (H11); the FBD corpus test reads the new fixture too.
  Proof that it compiles, `crates/mythos-logic-compiler/tests`:
  - `ladder_from_the_editor.rs` `the_rung_the_editor_wrote_with_a_library_block_runs_as_the_same_program_in_st`
    and `fbd_from_the_editor.rs`
    `the_network_the_editor_wrote_with_a_library_block_runs_as_the_same_program_in_st`: the
    fixture compiles with `compile_sources` and the library `MOTOR_LIBRARY` (support/mod.rs) and
    runs equal to a Structured Text program over 400 scans.
  - `editor_library_blocks_are_what_the_workbench_tests_place`: reads `libraryanswer.ts`,
    extracts its JSON `ANSWER` and asserts it equals `library_blocks(MOTOR_LIBRARY)` in the
    module's shape, so an edited UI fixture turns it red.

  **Departures and decisions of the stages.**
  - H13 (formal-parameter case) is settled: the editor writes RUN, SPEED, RUNNING, RPM, upper
    case, against a library declaring them in lower case, and both fixtures compile and run equal
    to the Structured Text program.
  - H14 is settled by the first compiler test: the unit keeps the order of the sources.
  - The filters are in `blocks_of`, a private function, and not inline in `library_blocks`;
    each is paired with its pin first (`.map(|v| (pin(v), v))`), without `.enumerate()`.
  - `check_library` uses full paths (`mythos_logic_compiler::library_blocks`), the `use` line is
    untouched; in `plugins/logic/src/lib.rs` only `check_library`, `block_json`, the summary of
    `/library/check` and the tests were edited.
  - `Kit.groups` takes `(own, library)` and returns `[title, items, line?]`; the Library group is
    built by a shared `libraryGroup`. `unknownBoxWords` is a function, not a constant. A single
    diagnostic is written "1 diagnostic". The first letter of the why in the second R11 line is
    lower-cased.
  - The editors' `own` is a `useMemo` over the library store; the palette store's `own` stays
    the document's own blocks. A document POU shadows a library block of the same name, as `own`
    is searched first.
  - The document's own POUs are compared by `pinProblems` too (section 9 item 4); no existing
    test changed meaning, so the comparison was not limited to library blocks.
  - `ldedit.declarationFor` has an optional third parameter `own`; the FBD one is unchanged.
  - `ldmodel.test.ts`'s fixture helper (which writes a missing fixture) is unchanged; the new
    LD test uses `guardedFixture`. `MOTOR_LIBRARY` is in the shared `support/mod.rs` and not
    copied into two test files. The header comments of `ldcatalog.ts` and `fbdcatalog.ts` now
    say the editors add library blocks from the module's answer; only comments changed.
  - The R11 words of the contract were changed by a fix (below): the sentence no longer says the
    block is not a standard one, because the editor's table lacks standard variants.

  **What the two reviews found, and what was done.** Both reviews returned.
  Review of the answer and the palette:
  - A document POU and a library block of one name: the Library item placed the document's POU.
    Fixed by refusing and naming the clash (`twinsOf`, `twinWords`, `<p>-palette-library-twins`;
    the checks say it at a box of the document's block). Not covered: while the POU being edited
    is itself named like a library block, `documentOwn` leaves that POU out and the library block
    is still offered inside its own body.
  - The line "being checked" stayed when GET /library failed and no check was asked. Fixed:
    `libraryBoxes(stored, listError)` says "The library could not be read: <error>; ...".
  - A library entry named like a standard block or twice (LATENT, the module refuses both).
    Fixed: the whole list is unknown.
  - An input named EN gave the formal EN twice in LD (older than the slice, also for document
    POUs). Fixed in `ldedit.retype`: each formal once, the first EN being the power pin. Not
    done: no word tells the engineer that a declared input EN cannot be given in a diagram.
  - The Rust test compared with its own literal. Fixed: it reads `libraryanswer.ts`.
  Review of the boxes:
  - R11's sentence called a standard block the compiler declares (CTU_DINT, TON_TIME) no
    standard block. Fixed by the changed sentence above. Typed standard variants are still not in
    the editor's table (`ldcatalog.ts` is not a file of this slice).
  - FBD Join with OR read the model's ANY type at a library pin. Fixed (`orAt` uses
    `declaredType`).
  - LD did not say a retyped power pin. Fixed for library and document blocks; standard boxes
    are not compared. What the compiler says at Check for such a box: not run.
  - A block that became a function kept its instance unsaid and hidden. Fixed: said, and the
    field is shown so it can be cleared. What the compiler does with an instance on a function
    call: not run.
  - Two behaviours had no test (the document's own POU compared, the output direction of the
    declared type). Fixed: tests added, no code change.

  **What was not run.**
  - The full gate (`./scripts/check.sh`) and a mutation check are run after this text. During
    stage 2, `./scripts/check.sh --frontend` passed (`pnpm test` 863 tests); it was not re-run
    after the later edits. Mutations were not run by the agents; the fixes give theirs.
  - NOT VERIFIED, nothing was looked at in a browser: a library block with a long name in the
    Instructions card (the star stays inside the card; the suggestion list); a function block
    with ten inputs in an LD rung and an FBD network (box height, the rung's row, the pin
    fields wrapping); the group's line in the narrow `<details>`, and the flicker while the
    store's check is null after a library change in another tab (H12).
  - Nothing was run on Windows or macOS by the agents.
  - The version of the logic module is not changed by this slice's commits; the orchestrator
    sets it when the slice is put on dev.
  - Not checked by the reviews: whether a `data/types/*.st` source can hold a function or
    function block, which would then be answered as a library block.

  **What is left out, with its class.**
  - VAR_IN_OUT pins: no front end supports them, so none is offered or invented (LATENT; the
    answer needs an `in_outs` list when one does).
  - No action takes a changed declaration's pins, and LD retype's silent loss of an operand at a
    pin the new type lacks (H6, LATENT, older than the slice).
  - A favourite of a block not offered now disappears without a word (LATENT).
  - The source file of a block is not named in the answer (the IR has no source per POU).
  - SFC bodies: actions and conditions are kept as found and have no palette.
  - Not fixed: the twin case inside the POU's own body, and the word for a declared EN input
    (both above).

  **Verification of the finished slice** (the orchestrator, after both reviews' fixes, the
  rebase onto dev `fcfca27` and the version line; this is what "run after this text" above was
  waiting for).
  - **The frontend gate** on `40e5438`: `./scripts/check.sh --frontend` printed
    "check.sh --frontend: PASS — every gate green": `pnpm lint`, `pnpm test` (59 test files,
    883 tests) and `pnpm build`.
  - **Rust**, narrow, on `40e5438`: `cargo fmt --all --check` clean;
    `cargo test -p mythos-core --test version_agreement_tests` 3 passed;
    `cargo test -p mythos-logic-compiler --test library_blocks --test ladder_from_the_editor
    --test fbd_from_the_editor` 5 passed in each; `cargo test -p mythos-logic --lib` 79 passed;
    `cargo clippy -p mythos-logic-compiler -p mythos-logic --all-targets -- -D warnings` clean.
  - **Mutations**, with `scripts/mutation-check.py`, in a copy of `40e5438` made by `git archive`
    with its own install and its own target directory, both baselines green in every run.
    **33 of 33 red** in the end; two were not red in the first run, and in both the fault was the
    orchestrator's run, not a missing test:
    - Frontend, 23 (8 of stage 2, 6 of the orchestrator's, 9 for the review fixes), against
      `libraryblocks.test.ts`, `library-palette.test.tsx`, `unknown-box.test.ts`,
      `ldmodel.test.ts`, `fbdmodel.test.ts`, `ladder-editor.test.tsx` and `fbd-editor.test.tsx`:
      22 red, 1 timed out. The one that timed out was the orchestrator's "a drifted box re-pinned
      on a library change (LD)": as written it made an edit in an effect that the edit itself
      runs again, so the test run never ended; a hang is not a red. Written again to re-pin once
      per change of the library's blocks, it is red in 2 tests ("a library change rewrites no
      diagram" and "a block saved in the library while the editor is open is offered once the
      library is checked again").
    - Rust, 10 (5 of stage 1, 4 of the orchestrator's, 1 for a review fix), against the three
      compiler test files above and `mythos-logic --lib`: 9 red, 1 survived. The survivor, "a
      hidden output answered as a pin", is caught by the unit test
      `tests::a_variable_the_front_end_made_is_not_a_pin` in the compiler crate's `--lib`, which
      the run's test command did not include; run again with
      `cargo test -p mythos-logic-compiler --lib` it is red in that test.
    - Stage 2's mutation M14 no longer applies (a review fix rewrote its lines; fix F6 covers
      the same behaviour) and was not run.
    - vitest does not typecheck, so a frontend "red" is of the behaviour; the vitest reporter
      lists each test under two names, so the counts in the logs are twice the tests.
  - **Not run by the orchestrator:** the full gate (`./scripts/check.sh`) and the Windows lint.
    No green full gate of this commit exists from this machine; CI is the gate.
  - **NOT VERIFIED in a browser:** the four views named above. The orchestrator had said it
    would look at them and did not: it needs the server, the modules and a scripted browser
    built for the purpose. They stay as written above until a browser check of the palette is
    part of `scripts/e2e.sh`.
  - **Read by one reader only:** the ten review fixes (`ba738bc`) were read by the orchestrator
    and by no second reviewer.
  - **A red of this slice found by CI, and its correction.** On `648054e` the job "Logic VM on
    Bare Metal" failed in the step that runs `scripts/logic-vm-on-emulators.sh`. The script
    compiles every `*.xml` of `tests/ladder/` and `tests/fbd/` by itself, and the two fixtures
    this slice added there (`editor_library.xml`) call a block of a library, so they do not
    compile alone. Reproduced here without the emulators: `expected_scans` on each of the two
    files exits 1 with "Motor is not a type this compiler knows". The test
    `runs_in_pieces.rs` reads the same two folders the same way; neither it nor the script had
    been run by the agents or by the orchestrator, whose narrow commands named three test files
    only. The fixtures now live in `tests/ladder_library/` and `tests/fbd_library/`, which
    nothing sweeps. After the move: `cargo test -p mythos-logic-compiler` (the whole crate) every
    test file green, `runs_in_pieces` 8 passed; `expected_scans` over every program the script
    takes, for both target profiles, no failure; the four frontend test files that read the
    fixtures 102 passed; `cargo fmt --all --check` clean. The emulators themselves were not run
    here (QEMU is not installed on this machine): CI's job is that check. The mutations above
    were not run again: the change moves two files and the paths that name them.
