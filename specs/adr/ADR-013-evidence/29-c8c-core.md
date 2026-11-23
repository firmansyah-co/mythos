# ADR-013 evidence: C8c-core

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **C8c-core** (a program's identity, RETAIN values kept per program and one program in one
  runtime: what the review of C8c found in the core, 2026-11-23). The core compared programs by
  their PROGRAM name only, kept RETAIN values per runtime id, and did not refuse one project
  program in two runtimes. This slice gives a program an identity in the protocol, keeps and
  gives RETAIN values per program, changes a program online only within the same programs of
  one project, and refuses one project program in two runtimes when both instances name it.
  Nothing was run on Windows or macOS by the agents of this slice.

  **What it fixes.** Two findings of the review of C8c ([`28-c8c.md`](28-c8c.md)) and two the
  design agent added.
  - Review finding 5: a runtime id given another program gave it the RETAIN values kept for the
    first one on a warm load. LIVE once configured runtimes are used, and LIVE for an instance
    written by hand.
  - Review finding 7: a program moved from one configured runtime to another ran in both until
    the old runtime was taken out. LIVE. After this slice it is LATENT for instances built
    before it (see the open items).
  - Added by the design agent: an online change between two project programs that declare one
    PROGRAM name (the comparison was by PROGRAM name, so the second took over the first one's
    running variables). LIVE once configured runtimes are used.
  - Added by the design agent: going from one program in a runtime to several, or back, lost
    the RETAIN values of the program that stayed (read in code, not run
    before the slice; the end-to-end test of the program that keeps its values shows it kept after). LIVE.

  **The identity** (`crates/mythos-module/src/protocol.rs`).
  - `RuntimeProgram.built_from: Option<String>` is the project program the build is of, by its
    name in the project (the logic module's `programs.json`), which need not be the PROGRAM name.
    `RuntimeProgram.created: Option<String>` is a mark of the making of that project program
    (below). Both are `#[serde(default, skip_serializing_if = "Option::is_none")]`: an instance
    without them reads and writes byte for byte as before, and one with them round-trips.
  - `ProgramIdentity { name, built_from, created }` and `RuntimeProgram::identity()`.
  - `same_program(a, b)`: when both name their project program, the names are equal without
    regard to ASCII case and `created` is equal (both absent included); otherwise the PROGRAM
    names are equal exactly, whatever the other one's `built_from`.
  - `same_program_set(a, b)`: when every program of both names its project program, the project
    names sorted without regard to case, and the sorted (project name, `created`) pairs, are
    equal; otherwise the PROGRAM names sorted by bytes are equal. Lists are compared
    element by element, never joined, so `a,b` with `c` differs from `a` with `b,c`.
    `program_set_keys(a, b)` gives the keys as compared, for the words of a refusal.
  - An identity is never invented. `built_from` is never filled from `name` or from anything
    else, by the core or by the logic module's stand-ins; an absent one stays absent in the
    retain file and in the configuration `GET` serves. `PROTOCOL_VERSION` stays 1.
  - `RuntimeInstance::identities()` replaces `program_key`.
  - A program that does not name its project program is compared by PROGRAM name, which is the
    comparison before this slice. That fallback is why an instance saved before the slice is
    still changed online by, and still given the RETAIN values of, any build of a program of
    its PROGRAM name (see the open items).

  **The retain file** (`<system>/retain/<project>/<runtime id>.json`, the name is unchanged,
  written with `serde_json::to_vec_pretty` through `atomic_write_file`). It is an object with
  `deny_unknown_fields`:

      {
        "programs": [
          {
            "name": "A",
            "built_from": "A1",
            "created": "0123456789abcdef",
            "values": [ { "path": "total", "type": "DINT", "bytes": "2a000000" } ]
          },
          {
            "name": "B",
            "values": [ { "path": "count", "type": "DINT", "bytes": "01000000" } ]
          }
        ],
        "data": [ { "path": "S", "type": "T", "bytes": "00000000", "layout": "0123456789abcdef" } ]
      }

  (Pretty-printed, one key per line, in the file.) The example is the form
  `kept_of` writes, with `created` added by hand to show where it sits.
  - One `KeptProgram` per program of the instance that has at least one value in the module's
    answer, in instance order, values in the answer's order, paths without the program's name.
    `built_from` and `created` are written only where the instance's program has them. `data`
    holds the data masters (a value with a layout, path the block's name), unchanged, and is
    absent when there is none.
  - **What the module is given.** `given_of` turns the file into the wire values of a start. An
    entry is given only to the program of the instance that is `same_program` with it. With one
    program the paths are bare; with several each is `<program name>.<path>`. Every data master
    is given as kept, whatever the programs (blocks are of the project and keyed by name, type
    and layout, ADR-014). The wire form is unchanged: the module is given and answers the same
    `retained` list as before, and the conversion is the core's.
  - **What is kept.** `kept_of` turns the module's answer into the file. With one program every
    value without a layout is that program's, whatever its path. With several, a value is the
    program's whose name equals the part before the first `.`, trimmed, without regard to case,
    and the rest of the path is stored. A value that names no program is not stored and is
    logged: `a RETAIN value runtime '<id>' gave names no program of it: <path>`.
  - **The old form.** A file of before the slice is a bare JSON array. It is read once as
    `Kept::Legacy` and given unchanged, as before, with `not_given` empty. The first keep after
    that start writes the new form even when the values did not change. A downgrade is the
    reverse (open items).
  - **`not_given`.** `RuntimeLoad.not_given` (always serialized, like `dropped`) lists each kept
    value a warm start did not give, as `{variable, why}`. `variable` is `<program>.<path>`
    where `<program>` is the entry's `built_from` when it has one, else its `name`. The reasons
    are:
    - `kept for program '<p>', which runtime '<id>' does not run now`;
    - `kept for program '<p>' as it was made before: runtime '<id>' runs a program made again
      under that name, which is another program`;
    - `it could be of '<a>' or '<b>'` (an entry that two or more programs could be is given to
      none of them);
    - `program '<name>' could be any of '<x>' or '<y>', each kept apart: it is given none of
      them` (two entries that could each be only that one program: one by `built_from`, one by
      the PROGRAM-name fallback; neither is given).
    `not_given` is also set on a load that is changed online, since RETAIN values are given
    beside the carried ones. The same list is logged once per start (`tracing::warn!`, `runtime
    '<id>' was not given RETAIN values kept under its id: <variable> (<why>); ...`).
  - **When the file is written.** After a start that withheld anything, or read a legacy file,
    the first keep writes, so the file then holds only the instance's own programs. A cold load
    gives nothing and the first keep writes, as before. An answer with values that name no
    program is kept in memory in full, so it does not cause a write at every keep; the file
    drops those values.
  - **A file that cannot be read.** A file that is neither an array nor a `KeptFile`, or does
    not parse, is logged `<path> holds no RETAIN values: <reason>` and gives nothing, as before.

  **The online change** (`give_variables`). A new build takes over a running one's variables only
  when `same_program_set` holds, the module is the same, the runtime is in RUN and was not asked
  to stop. The refusal keeps its words, `the program running was '<keys>', not '<keys>'`, with
  the keys as compared (project names when both sides gave them, else PROGRAM names, sorted,
  joined with `,`), and the new build starts warm. When only `created` differs, the words are
  `the program running was '<keys>' as made before: a program made again under its name is
  another program`. The driver records the project active when it starts (read once, the same
  read that gives the retain file). An ask from another project is refused with `the program
  running was of project '<a>', not of project '<b>'` (or `no project known`), so the new build
  starts warm: a project activation does not stop the old project's runtimes, and a program of
  the newly active project with the same entry name in a runtime of the same id would otherwise
  have taken over the previous project's running variables.

  **The refusal of one program in two runtimes.**
  - `RuntimeInstance::check` (so also at `RuntimeDriver::parse`, at a start and at a describe)
    refuses a `built_from` that is blank after trimming, `runtime '<id>': built_from names no
    program of the project` for one program and `runtime '<id>': program '<name>': built_from
    names no program of the project` for several, and two programs of one instance with one
    `built_from` without regard to case, `runtime '<id>': programs '<a>' and '<b>' are both
    builds of program '<built_from>' of the project`.
  - `RuntimesConfig::validate`, right after `i.check()?` and before the data-block and output
    checks, refuses two instances holding programs of one `built_from` (without regard to case):
    `program '<built_from as the first instance spells it>' of the project is in runtimes
    '<first id>' and '<second id>'; a program runs in one runtime: take it out of one of them in
    the same save`. At `PUT` it answers 400 and writes nothing. A stored `runtimes.json` that
    breaks it is refused at `RuntimesConfig::load` in the same words. The plant download check
    calls `validate`.
  - The condition: the refusal holds only between instances that both name their project
    program, which the logic module's builds do since 0.8.0. An instance without `built_from` is
    not compared, with another without it or with one that has it.
  - Two instances whose programs declare one PROGRAM name and have different `built_from` are
    accepted.

  **What the logic module fills** (`plugins/logic`, version 0.7.0 to 0.8.0 in
  `plugins/logic/Cargo.toml`, `Cargo.lock` and `frontend/packages/logic-ui/package.json`).
  - Every build the module answers carries `built_from`, the entry's name as `programs.json`
    spells it (a task that names `mixer` still gives `Mixer`): the build of a program alone, the
    build of a program in a configured runtime and the implied runtime build, which stays byte
    for byte the program build. The stand-ins that are never answered (`stub`, `check_in`) keep
    `None`. A program that declares `PROGRAM Main` under the entry `Mixer` gives name `Main` and
    `built_from` `Mixer`.
  - `created` is a field of the `programs.json` entry. `POST /programs` sets it to a mark: the
    first 8 bytes of a SHA-256 of the name, the time, the process id and a process counter, as 16
    hex characters. The settings route keeps it. An entry of a `programs.json` of today has none,
    it is not filled in afterwards, and it builds and writes back byte for byte as before. A
    program made before the mark (no `created`) against one made again after it (`created`)
    counts as two programs; two builds of one unmarked program count as the same.
  - No version changed. `built_from` and `created` are in neither `version_of` nor
    `version_in_runtime`, so the pinned version literals and the byte-for-byte tests of C8c stay
    as they were. The module's version moved to 0.8.0 because its answers gained keys.
  - Summaries and the RETAIN warning say what the core does, with "since 0.8.0" where they speak
    of the refusal: `build_logic_program`, `save_logic_runtime`, `delete_logic_runtime`,
    `build_logic_runtime`, `delete_logic_program` (what a program made again is), and the core's
    `save_runtimes_config`, with the `built_from` and `created` properties in the schema of its
    body. No route was added, so no tool was added. `bridge.ts` gains `not_given?: NotCarried[]`
    on `RuntimeLoad`, a type that nothing reads.
  - The summaries say the exceptions: a retain file of before is given once as it is to whatever
    the runtime runs at its next warm start, `not_given` empty; a running instance that names no
    project program is replaced online by a build of any program of the same PROGRAM name; and
    the refusal of one program in two runtimes holds only when both instances name it.

  **The tests** (all by name; "core unit" is in `crates/mythos-core/src/runtimes.rs`).
  - `crates/mythos-module/src/protocol.rs`:
    - `a_program_without_a_project_name_reads_and_writes_as_before_and_one_with_it_round_trips`:
      both JSON texts as literals, an absent `built_from` never written.
    - `the_same_program_is_the_same_project_program_when_both_name_it_and_the_same_program_name_otherwise`:
      `T1` and `t1` are one, `T1` and `T2` are two with equal names, names exactly otherwise.
    - `two_runtimes_run_the_same_programs_by_project_name_when_every_program_says_it_and_by_program_name_otherwise`:
      all or nothing, lengths, lists never joined, the keys as compared.
    - `a_program_made_again_under_its_name_is_another_program`: `created` decides.
  - core unit:
    - `a_program_naming_no_project_program_or_one_named_twice_in_a_runtime_is_refused`: both
      messages, one and several programs, also through `RuntimeDriver::parse`.
    - `one_program_of_the_project_in_two_instances_is_refused_naming_both`: the literal, a case
      difference, refused before a shared output tag, a runtime of several too.
    - `programs_that_do_not_name_their_project_program_are_not_compared`
    - `two_project_programs_declaring_one_program_name_run_in_two_runtimes`
    - `a_saved_file_with_one_program_in_two_runtimes_is_refused_at_load_in_the_same_words`
    - `the_kept_file_of_one_program_and_of_several_is_written_as_this_text`: `kept_of`'s JSON
      pinned as a literal for one program with a data master and for two programs, and read back.
    - `a_value_the_module_gives_is_kept_for_the_program_its_name_says`: one program takes
      everything, several split at the first `.`, an unattributable value is dropped and said.
    - `a_value_kept_for_one_program_is_given_only_to_it`: one to one, another program by name and
      by `built_from` with equal names, an entry without `built_from`, one to several, several to
      one, A and B to A and C, an entry matching two programs, data masters, the `not_given`
      literals and the rewrite flag.
    - `a_retain_file_of_today_is_given_unchanged_and_written_again_in_the_new_form`
    - `a_retain_file_of_an_unknown_form_gives_nothing`: `{"programs":[],"x":1}`, `{}`, `"text"`,
      `[1,2]`, an unknown key in a program, text that is not JSON.
    - `a_program_made_again_under_its_name_is_given_none_of_the_values_kept_for_the_one_before`
    - `two_entries_kept_apart_are_never_both_given_to_one_program`
  - `crates/mythos-module-tests/tests/runtime_api.rs`:
    - `a_retain_file_of_today_is_given_as_before_and_written_again_in_the_new_form`: a bare array
      written before the first load, loaded in STOP, `not_given` is `[]`, the file becomes the
      object form with `Counter`, `total`, `2a000000` within 15 s, then RUN and `total` goes on
      from 42.
  - `crates/mythos-module-tests/tests/data_and_logic.rs`:
    - `a_program_moved_between_configured_runtimes_is_refused_until_the_old_runtime_lets_it_go`:
      400 with the exact words, `GET` unchanged, then 200 when the old runtime is built again
      without the program in the same save.
    - `another_project_program_of_the_same_program_name_is_neither_changed_online_nor_given_its_retained_values`
    - `a_program_whose_source_renames_its_program_is_still_changed_online`
    - `a_program_keeps_its_retained_values_when_programs_join_and_leave_its_runtime`: it runs at
      500 ms after the kept value reached 50000, so a total restarted from 0 could not reach it
      in the time the test allows (25 s), and the test cannot pass by chance.
    - `a_program_made_again_under_a_deleted_name_is_neither_changed_online_nor_given_the_values_of_the_one_before`
      (RUN variant) and
      `a_program_made_again_under_a_deleted_name_and_loaded_in_stop_starts_without_the_values_of_the_one_before`
      (STOP variant): the two repros of review finding 1, now asserting a warm start, the
      `not_given` text, a total of 0 and a file that holds only the new making.
    - `a_program_of_the_project_made_active_never_carries_on_from_another_project_s_running_program`:
      after activating another project, the new program's `n` is below 20 (it held 152 at the
      commit before the fix).
    - `a_running_instance_that_names_no_project_program_is_replaced_online_by_one_of_its_program_name_as_said`:
      pins what the summary says; a change of that behaviour must change the summary too.
  - `plugins/logic`:
    - `every_build_names_its_program_in_the_project` (`runtimes/tests.rs`): `Mixer` declaring
      `PROGRAM Main` alone and in a configured runtime, and a runtime of several; the implied
      build byte for byte the program build; the runtime version is still its programs'
      versions joined.
    - `a_program_made_again_under_its_name_is_marked_apart_from_the_one_before`: also that an
      entry without the mark builds without it and writes back byte for byte.
  - Every end-to-end test above waits for a state (RUN with a scan, a value, a file's content)
    with a deadline of about 15 s, asserts no latency, and puts the core's answer in its message.
    The tests of stage 1 and 2 were run five times in a row each, all green; the runs of the
    tests added by the review fixes are not recorded here.

  **Existing tests that changed.**
  - `the_program_key_is_the_sorted_names_and_one_program_s_is_its_name` and
    `two_different_program_sets_have_different_program_keys` (core unit): rewritten on
    `identities()`, `same_program_set` and `program_set_keys`, the same cases plus project names
    and project names that hold `,`.
  - `a_new_build_of_a_running_program_is_changed_online_and_a_cold_load_or_another_program_starts_as_before`
    (`runtime_api.rs`): flipped. Tally's `total` is now at most `n + 1` (it was at least the kept
    value plus `n`), and `not_given` is `[{variable: Counter.total, why: kept for program
    'Counter', which runtime 'counter' does not run now}]`.
  - `a_runtime_id_given_another_program_warns_of_the_values_a_warm_load_hands_it` is renamed
    `a_runtime_id_given_another_program_gives_it_none_of_the_values_kept_for_the_first`
    (`data_and_logic.rs`): flipped. The load starts warm, why `the program running was 'A', not
    'B'`, `not_given` is `[{A.total, kept for program 'A', which runtime 'Line' does not run
    now}]`, B's total is 0 (it was at least 1), and the file holds only B within 15 s.
  - `kept_when` and `kept_block_when` (`runtime_api.rs`): read the new file form.
  - `a_runtimes_file_of_today_round_trips_byte_for_byte` (`runtime_vm.rs`): its assertion
    `instance.program_key() == ["Tank"]` became the same claim on `identities()`. It is not
    named in the design; `program_key` no longer exists. The round trip is unchanged.
  - `the_runtime_tools_say_what_they_take_and_report_of_data_blocks` and
    `the_runtimes_body_schema_takes_what_the_route_takes_and_refuses_what_it_refuses` (`mcp.rs`):
    substrings and the `built_from` property added, nothing removed.
  - `a_runtime_build_names_what_it_replaces_and_warns_of_retained_values_not_taken` and
    `the_tools_say_that_a_runtime_a_program_left_still_runs_it_until_it_is_taken_out`
    (`plugins/logic/src/runtimes/tests.rs`): the new warning and summary texts; `or the program
    runs in both` became `the program then runs in both`; a loop over the four tools asserts that
    none says `does not refuse one program` and each says `since 0.8.0`.
  - `programs_sharing_a_block_loaded_alone_refuse_their_runtime_until_the_replaced_instances_go`
    (`data_and_logic.rs`): the second refusal, Writer alone beside Line, is now the refusal of
    one program in two runtimes, not the data-block one. The first refusal is unchanged.
  - `a_project_of_today_checks_and_builds_with_exactly_todays_keys` (`plugins/logic/src/lib.rs`):
    `built_from` is in the keys of a built program. Its name still says "today's keys" and was
    not changed.

  **Decisions and departures.**
  - The design decided: the retain file is an object (not a flat array); the refusal of one program in two runtimes sits
    before the data-block and output checks; the logic module and its UI package go to 0.8.0;
    the same project program whose PROGRAM is renamed is changed online; a value the core cannot
    attribute is not stored and is logged; no workbench behaviour changes; the refusal is always
    said with its condition; an identity is never invented.
  - `program_set_keys` is public and not in the design's names: `same_program_set` compares
    its lists and the refusal words need the keys as compared. `same_program_set` does not call
    `same_program`.
  - `given_of`: an entry that two or more programs could be is given to none. An entry's
    variable in `not_given` is named by `built_from` when it has one.
  - `RetainStore::read` parses the JSON first, then reads an array as legacy and anything else as
    a `KeptFile`.
  - Stage 1 left `plugins/logic` to the four struct literals (`built_from: None`), the RETAIN
    warning and the RETAIN sentence of `build_logic_runtime`, since the core's behaviour already
    made the old warning false and the flipped test asserts the new one.
  - A stale comment in `build_runtime` (`plugins/logic/src/runtimes.rs`), which said the core
    keeps RETAIN values per runtime id and not per program, was rewritten.
  - The design said the flipped `data_and_logic.rs` test turns red under the mutation that
    removes the rewrite after a withheld value. It does not: B's module answers its own total (0),
    which differs from the empty values it was given, so the first keep writes anyway. That
    mutation is red at the unit test that asserts the flag, and the end-to-end test that needs
    the flag is the legacy-file one.
  - **A load report at a server start and at a project activation (read in code, not run).** The
    report is taken but not surfaced. `crates/mythos-core/src/server.rs` calls `io.reload().await`
    at the server start and drops the return value; the activation handler in
    `crates/mythos-core/src/api/mod.rs` calls `io.reload().await` and answers only `{status,
    project_id}`. Both go through `IoManager::reload`, `begin_loads` and `end_loads`, so
    `record_load` fills a report that nothing reads. A process started again after a crash is
    given `sent` and withholds nothing. The log line of `not_given` is written at every start
    that withholds anything, whatever the caller.
  - The logic module's builds name `created` as well as `built_from`, a field that was not in the
    design; it came from review finding 1.

  **What the two reviews found, and what was done.**
  - Review of RETAIN, finding 1 (medium, LIVE): a program deleted while the core still ran it and
    made again under its name had the same `built_from`, so in RUN it was changed online and took
    over the old program's variables, and in STOP a warm start gave it the old RETAIN values with
    `not_given` empty (the reviewer's repro gave 105 and 101 where 0 was expected). Fixed with
    `created`: the mark, the identity rules above, and the two end-to-end tests. Before the
    slice equal PROGRAM names had the same effect, so this was not a regression.
  - Review of RETAIN, finding 2 (low, LATENT): two kept entries could both match one program,
    one by `built_from` and one by the PROGRAM-name fallback, both were given under one path and
    the later one won, with nothing said (the repro gave two `total` values and an empty
    `not_given`). Fixed: both are withheld, named in `not_given`, and the file is rewritten. Test
    `two_entries_kept_apart_are_never_both_given_to_one_program`.
  - Review of RETAIN, finding 3 (low, DOC): the summaries said a start gives a program only the
    values kept for it and did not say that a legacy file is given unchanged. Fixed in the texts
    of `save_runtimes_config`, the RETAIN warning and `build_logic_runtime`; the behaviour stays
    and is accepted as LATENT (open items).
  - Review of the identity, finding 1 (high, LIVE, a regression of this slice): `built_from` is
    a name inside one project, but the online change compared it with a running build that could
    belong to another project. After a project activation a program of the new project with the
    same entry name and runtime id took over the previous project's variables (the reviewer's
    repro: `n` of 152 carried from the other project's program, a warm start expected). Fixed
    by recording the project at start and refusing an ask from another project. Test
    `a_program_of_the_project_made_active_never_carries_on_from_another_project_s_running_program`.
  - Review of the identity, finding 2 (low, LATENT): an instance saved before the slice is
    replaced online by a build of another project program declaring the same PROGRAM name, and
    nothing said so. Fixed by saying it, not by changing the behaviour: the PROGRAM-name fallback
    is the design's, and a behaviour change would contradict it. The sentence is in
    `save_runtimes_config` and pinned by `a_running_instance_that_names_no_project_program_...`.
    Whether to withhold in that case (a warm start whenever one side does not name its project
    program) is a decision for the owner, not made here, and would not stop the name fallback
    of the kept values.
  - The mutations the review fixes specify (each with its old and new text and the tests it must
    turn red) were not run.

  **What was not done, and what was not run.**
  - The full gate (`./scripts/check.sh`) and a mutation check of the finished branch are run
    after this text and are not claimed here.
  - Mutations of stage 1 and 2 (M1 to M19 of the design, and O1 to O5) were run by the agents
    in the worktree before the review fixes, each file restored with `git checkout`, and each
    turned red at the tests the agents' reports name. M7, M8 and M12 as the design wrote them
    did not compile, and the texts that ran are in the agents' reports, not in this file. The
    end-to-end tests `a_retain_variable_keeps_its_value_across_a_stop_and_a_new_process_and_a_cold_start_resets_it`
    under M3 and `a_retained_block_survives_a_killed_process` under M12 were not run under
    those mutations; both were shown red at unit level only.
  - Windows and macOS: not run. No new path or file name is added: the retain file keeps its
    name and is written with `atomic_write_file`, and paths are built with `Path::join`.
  - A strict third-party runtime module that refuses a `built_from` or `created` key: NOT
    VERIFIED, none exists in the repository. An older core reading `runtimes.json` ignores both
    keys and writes it back without them on its next `PUT`.
  - The workbench was not changed beyond the type field and the version line.

  **Open items, each with its class.**
  - Instances built before 0.8.0 are not compared (no `built_from`), so a program can still run
    in two runtimes until each is built again and saved. LATENT after the slice. A stored
    `runtimes.json` of before reads and writes back unchanged.
  - A downgrade to a core of before the slice reads the new retain file as holding no RETAIN
    values (`invalid type: map, expected a sequence`), starts every RETAIN variable from its
    initial value and writes the old form at the next keep: the kept values are lost once.
    LATENT.
  - A retain file of before (a bare array) first met by another program than the one that kept
    it is given to it once, with `not_given` empty, because nothing in the file says which
    program kept it. LATENT.
  - A retain file that cannot be read is overwritten at the first keep, as before. LATENT.
  - RETAIN values do not follow a program from one runtime id to another, and a retained data
    block does not either: a block that moves into a configured runtime loses its kept values.
    LIVE once used; the logic module's build warns of variables only, not of blocks.
  - The workbench cannot take the other instance out when the core refuses a Load under the
    refusal above; it shows the core's words. Left to C8d.
  - The shell's `programTags.ts` reads `instances[].program.published` and ignores `programs`,
    so a runtime of several programs offers no published tag to the HMI's tag list. LATENT since
    C8b, not fixed here.
  - A running instance that names no project program is replaced online by a build of any program
    of the same PROGRAM name (review of the identity, finding 2): said in the summary, not
    prevented. LATENT.
  - The workbench does not show `not_given`; `RuntimeStatus` does not show `built_from`. C8d.
