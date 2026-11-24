# ADR-013 evidence: C8d-core

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **C8d-core** (several watch lists per runtime in the core, and a revision precondition on the
  save of the runtimes configuration, 2026-11-24). Two changes to the core and its callers. Part A:
  callers that watch one runtime with different lists of variables no longer replace each other's
  list. Part B: `GET` of a project's runtimes gives a revision, and `PUT` refuses with 409 a save
  made from a read that is no longer current. No protocol, VM or runtime module change was made:
  `Call::Watch` carries one list and the module keeps no watch, so the core asks the union of its
  lists in one call and splits the reply. The slice is the follow-up that C8d-1 named
  ([`32-c8d.md`](32-c8d.md), "What is left out of C8d-1"); it adds no watcher to the workbench.

  **Part A, what the core does.**
  - **Each list is its own watch.** A watcher is its list: the exact sequence of names it sends,
    in order and spelling. Callers with the very same list share one watch and one answer; the
    same names in another order are another list. No token, no new route and no new MCP tool:
    `POST /api/v1/runtimes/:runtime/watch` and `watch_runtime` keep their body and answer
    (`{id, state, values, age_ms, fresh_ms}`) and their 400, 404, 502 and 504. A list is released
    only by expiry.
  - **The union is read once.** At each exchange the driver sends one `Call::Watch` with every
    name of every live list, each name once. The reply is given to a list by the names as asked
    (exact string match) and in the list's own order. A name that the reply has no value for is
    answered with `value: null`, `type: null`, not forced and the error "the runtime's module
    gave no value for it" (`NOT_GIVEN`), never a value. One list is never given another list's
    values as its own.
  - **A new list waits for a read of its own.** A list is answered only from a read that was kept
    after the list was first asked for, and that asked for every one of its names. A list added
    after the runtime was sent its union gets no answer from that reply; the route waits for the
    next read and is not answered from an older one.
  - **Expiry is per list.** A list nobody asks for again within 5 s (`WATCH_KEPT`) is let go,
    at the exchange and in `watch` before the bounds are computed, and the runtime is no longer
    asked for names that only it named. When no live list is left the instance's entry is gone.
  - **A module that does not answer.** The driver's unreachable branch stores the failure
    "the runtime's module does not answer: ..." for every list of the instance (`watch_failed`),
    so each watcher gets 502. Unloading the runtime drops every list.
  - **Two bounds.** At most 256 names in the union (`WATCH_LIMIT`, the figure that the route
    already applied to one list), and at most 256 lists of one instance. A new list that would
    pass either is refused with 409, nothing is registered, and the words are in `error`:
    "runtime '<id>' would be read for <n> variables at every exchange, and at most 256 are: other
    watches name <other> of them; a watch nobody asks for again within 5 s is let go", and
    "runtime '<id>' has <n> watches already, and at most 256 are kept: a watch nobody asks for
    again within 5 s is let go". A list that is already watched is renewed and never refused. The
    figure 256 for the number of lists is a design limit decided with this slice. It is not a
    figure published by the protocol, the VM or the compiler. The size in bytes of a name is not
    bounded (see "What is left out").
  - **Tool.** `watch_runtime`'s description says each list is its own, that callers with the very
    same list share one watch, the union of at most 256 names, the 256 lists, the 409 and that a
    watched list is never refused, the 5 s, and that a name without a value has value null and an
    error.

  **Part B, what the core does.**
  - **The revision.** `GET /api/v1/projects/:id/runtimes` answers `{config, revision}`. The
    revision is the lower hex SHA-256 of the bytes of `runtimes.json` as read, and of no bytes
    (`e3b0c442...b855`) when there is no file. The same bytes are parsed and hashed: one read. Any
    change to the bytes, by a save or by hand, whitespace included, changes it. An empty file has
    no bytes, so it has the revision of no file; the read refuses it as not a configuration, and a
    save that passes the revision of no file replaces it.
  - **The save.** `PUT ...?revision=<r>` (with or without `start=cold`) is saved as before when
    `<r>` is the stored file's revision, and its answer adds `revision` of the bytes written. A
    save with another revision is refused with 409 and "the project's runtimes have changed since
    they were read (revision <expected>, now <revision>), so nothing was saved: read them again";
    nothing is written, nothing is applied. The compare is made twice: before the module checks
    (so a stale save does not make the module load every program, and is answered 409 even where
    no I/O manager exists), and again under the lock before the write. The lock (`RUNTIMES_WRITES`)
    is taken by every save, with or without a revision, around the second compare, the write and
    the reload, so each answer's `changes` is its own save's among saves of this route. An empty
    `?revision=` is a given revision and never matches. A save without `revision` is saved as
    before and can overwrite a save made since the read; the descriptions say so.
  - **Query keys.** A key other than `start` and `revision` is refused with 400 "'<key>' is not
    something this save takes, so nothing was saved: it takes start and revision", and a key given
    twice with 400, so a misspelt `revision` is never an unconditioned save. The `GET` is not
    checked.
  - **MCP.** `get_runtimes_config` gives the revision and says what it is. `save_runtimes_config`
    takes the query argument `revision`, and its description says to pass the one the read gave.
    The logic module's `build_logic_program` and `build_logic_runtime` descriptions say the same.
  - **The logic UI.** Four saves send the revision of their own read: a runtime's Load, a
    runtime's Unload, a program's Load and a program's Unload. A 409 shows the core's words
    verbatim. A runtime's Load shows "The project's runtimes were changed since this Load read
    them, so nothing was loaded. Load again." before them. Load's second read of the
    configuration after the question is dropped: the revision of its first read covers the whole
    span. `plugins/logic` and `@mythos/logic-ui` are 0.12.0 (0.11.0 before; the stage wrote 0.11.0 on
    0.10.0, and the orchestrator set 0.12.0 when the slice was put after the palette slice, which
    took 0.11.0), with the `mythos-logic`
    entry of `Cargo.lock`. The core's version is unchanged. `ui-shell` and `data-ui` read `config`
    only and are unchanged.

  **The rules that keep it truthful.**
  - A name the runtime gave no value for is answered with value null and an error, never a value.
  - One watcher is never given another list's values; a list with no read of its own waits.
  - No bound is invented: the two bounds are 256 names and 256 lists, and nothing else.
  - A refused request says why in words in `error`, changes nothing stored or running, and the UI
    shows the refuser's words verbatim.
  - Every route keeps its MCP tool in step (ADR-010).
  - No OS-specific code: `std::fs::read`, `Path`, tempdirs, no fixed port, `DefaultShm`.

  **Tests, Part A** (`crates/mythos-core/src/runtimes.rs` unless named).
  - `two_watchers_of_one_runtime_each_read_the_values_of_their_own_list`: lists `[b,a]` and `[c]`
    are answered from one reply `a,b,c`; each gets its own names in its own order; the same list
    asked again keeps its answer; the same names in another order are an unanswered list.
  - `the_runtime_is_asked_for_the_union_of_its_lists_in_one_call`: `[c,a,d]`, `[b,c]`, `[a,b]` and
    an expired `[e]` give exactly `a,b,c,d`.
  - `a_list_is_answered_from_the_unions_reply_by_the_names_as_asked`: with a reply that has only
    `a`, the names `x` and `A` are value null with `NOT_GIVEN`, and `a` has its value.
  - `a_list_added_after_the_runtime_was_read_waits_for_a_read_of_its_own`: a list registered after
    another was sent gets no answer from that reply; the next read answers it.
  - `a_list_nobody_asks_for_again_within_5_s_stops_being_read_while_another_is_still_read`: with
    one list's `asked` set back past 5 s, only the other list is wanted and answered.
  - `a_watch_that_would_make_the_runtime_read_more_than_256_variables_is_refused_with_words`: the
    exact words with 257 names (other 256), nothing registered; after a list expires the same
    list is admitted.
  - `a_list_already_watched_is_never_refused_by_the_bound_when_it_is_asked_again`: at a full union
    and at 256 lists, a live list asked again is renewed.
  - `the_number_of_lists_of_one_instance_is_bounded`: a 257th list with no new name is refused
    with the exact words; after one expires it is admitted.
  - `a_module_that_does_not_answer_fails_every_list`: two answered lists both hold the failure;
    unloading drops every list and the instance's entry.
  - `asking_again_and_keeping_an_answer_cost_as_much_with_256_lists_as_with_one`: 256 rotations of
    the same 256 names; renewal and keeping an answer each cost less than 4 times the same work
    with one list (the fastest of 15 runs each, a ratio and not a time).
  - `a_watch_is_of_a_loaded_instance_and_its_answer_is_of_the_variables_asked` and
    `a_watch_nobody_renews_stops_and_one_of_an_unloaded_instance_is_forgotten` (existing, changed):
    they asserted that a second list replaces the first and now assert that it does not.
  - `the_watch_tool_says_each_list_is_its_own_and_what_bounds_it` (`mcp.rs`): the description
    and the two constants.
  - `two_watchers_of_one_runtime_each_read_their_own_list_through_the_server`
    (`crates/mythos-module-tests/tests/runtime_api.rs`): the real VM module through the router; a
    viewer's list and an engineer's list asked together for 10 rounds are each answered 200 with
    their own values in their own order (an unknown name with the VM's own error); a new 256-name
    list is refused 409 with the exact words and both watchers go on being answered.
  - `a_runtime_whose_module_does_not_answer_fails_every_list_watched_of_it` (same file): the
    module process is killed, and each process started in its place, until each list gets a 502
    whose error starts "the runtime's module does not answer: ".

  **Tests, Part B** (`crates/mythos-core/tests/runtimes_config_api_tests.rs` unless named).
  - `the_read_answers_the_revision_of_the_stored_bytes_and_of_no_bytes_without_a_file`: literal
    SHA-256 of no bytes and of hand-written compact bytes, not of the parsed value written again.
  - `a_save_with_the_revision_it_read_is_saved_and_answers_the_revision_of_what_it_wrote`: the
    answer's revision is the SHA-256 of the file on disk and what the next read gives.
  - `a_save_with_a_revision_the_configuration_no_longer_has_is_refused_and_nothing_is_written`:
    409 with the exact words, stored bytes unchanged.
  - `a_configuration_changed_by_hand_changes_its_revision_and_a_save_of_the_old_one_is_refused`.
  - `a_stale_save_is_refused_before_the_modules_check_its_programs`: 409 with no problems; the
    control with the current revision gets 400.
  - `a_save_without_a_revision_is_saved_whatever_is_stored`.
  - `start_and_revision_combine_and_a_key_the_save_does_not_take_is_refused`: `?revison=` and a
    key given twice are 400 and write nothing; `start=cold&revision=` works in either order.
  - `two_saves_of_one_revision_at_the_same_time_one_is_saved_the_other_is_refused`.
  - `an_empty_file_has_the_revision_of_no_file_and_a_save_with_that_revision_replaces_it`.
  - `an_agent_saving_runtimes_it_read_before_another_save_is_refused_with_words` (`mcp_tests.rs`):
    over MCP, the second save from one read is a tool error with status 409 and the core's words.
  - `mcp::tests::the_runtime_tools_say_what_they_take_and_report_of_data_blocks` (extended): the
    descriptions say the revision, the 409 and that a save without it can overwrite; the save
    tool's query keys are `start` and `revision`.
  - `runtime-load.test.tsx` and `programs.test.tsx`: each of the four saves sends the revision of
    its read and shows the core's 409 words; Load's rewritten test sends the old revision and
    shows its sentence and nothing as loaded; a core that gives no revision is sent no Load save;
    Unload names what the stored runtime held. The test of Load's read after the question is
    removed, since that read no longer exists.

  **Departures and decisions.**
  - Stage 1 changed `crates/mythos-module-tests/tests/runtime_vm.rs` (two asserts) because
    `RuntimeStates::watch` returns `Result<bool, String>` (`Ok(false)` not loaded, `Ok(true)`
    registered or renewed, `Err(words)` refused); only the asserts changed.
  - The unreachable branch calls the new `watch_failed`, which fails every list including one added
    since the last exchange, and does not expire lists; such a list gets the module's failure and
    never a value.
  - The contract's fixed targets T1, T2 and T5 and mutations M1 to M5 no longer exist as written:
    the review fix below replaced the per-list maps with one `Watching` per instance. T4's retain
    is in `Watching::let_go` and T6 is a check in `watched()`. The union is held as a count of
    lists per name, so it is updated as lists come and go. In the unit test of a loaded instance's
    answer, the last assertion now expects a list to wait after a read of another list's names
    only; the driver always sends the union, so that case does not occur in use.
  - The check order in `watch` is: let expired lists go, renew a list that exists (before any
    bound), the union bound, the lists bound, insert. A list that is present but expired is let go
    first, so it counts as new. Because a bound is checked only for a new list, the contract's
    mutation M2 is equivalent to the code and no test can make it red; R8 is proven on the lists
    bound.
  - Query: `put_config` takes `Query<Vec<(String, String)>>` and builds a private `LoadQuery`
    (`LoadQuery` was `pub` and used nowhere else), because axum's own rejection is plain text. A
    key given twice is refused beyond what the contract asked, so that which revision is compared
    does not depend on order.
  - `stored_bytes` and `load_with_revision` are added in `runtimes.rs`; the compare reads bytes
    only, so a file broken by hand has a revision and an unconditioned save can replace it; the
    `GET` of such a file stays 500.
  - The harness fake's revision is SHA-256 of the config as JSON, and it is not claimed equal to
    the core's bytes, only to change exactly when the stored configuration does.
  - The lock has no mutation that a test can catch: the second compare under it cannot be made red
    because the order of two writes that both passed the early compare is the scheduler's. The
    concurrent test shows the result the lock exists for, not the lock. The early compare does
    have one (the test of a stale save with a configuration the module refuses).
  - `tokio::join!` rotates which future it polls first, so the Part A server test was written and
    run once, alone, on the new code and was not run on `f0851d8`. Its red on the old code is
    argued, not shown; its 409 part fails on the old code for certain.

  **What the two reviews found, and what was done.** Reviews of Part A and Part B each returned.
  - Part A, medium, LATENT: `watch` hashed every name of every live list, and `watch_answer`
    built every list's values, under the one states lock that every runtime of the node shares.
    With 256 lists of 256 names the reviewer measured a round of renewals holding that lock for
    6.4 s in a debug build. Fixed: the count of lists per name, a renewal that costs its own list,
    and one kept read per instance with its index built before the lock. The test above was added;
    renewal 26 to 34 microseconds with one list against 36 to 42 with 256 (debug, measured once).
  - Part A, medium, LATENT: the bytes of a name are bounded nowhere; 256 lists of one name of
    about 2 MB pass both bounds, and the core would hold and send hundreds of MB. **Not fixed.**
    A bound needs a published figure (the protocol publishes none) or a decision of the owner. 24
    lists of a 2 MB name were accepted and sent as 48 MB per exchange for 10 s in the rig without
    the module going unreachable; a larger union was not tried. The fix above removed the CPU part
    under the lock; the memory and the line size are unchanged.
  - Part A, low, TEST-ONLY: no test drove the driver's unreachable branch. Fixed by the second
    server test above.
  - Part B, medium, LATENT: with the second read gone, Load against a core that gives no revision
    sent an unconditioned save without a word. Fixed: the read is checked at run time, and Load
    is refused before its question with words saying the core gave no revision. Program Load and
    Unload and runtime Unload still save without a precondition against such a core, as 0.10.0
    did.
  - Part B, low, LIVE: a runtime's Unload reads the configuration, and takes the revision, after
    its question, which was built from the controller's list. A save of that runtime made while
    the question was open is taken out without being asked about. **Not fixed**: the fix needs
    `RuntimeEditor.tsx`, outside the stage's files. The success words now name what the stored
    instance held when it was taken out, and a configuration that does not hold the runtime sends
    no save and says nothing was taken out.
  - Part B, low, LATENT: `RUNTIMES_WRITES` covers only this route. `plant::install` and
    `plant::restore` (`PUT /api/v1/slices/:project`) replace `runtimes.json` without it, and
    other reloads (the I/O configuration's save, an activation, the node's role, `run_here`) can
    apply a save's file online whatever `start` said. Both existed before the slice. **Not
    fixed**: the code is outside the stage's files. The comment above the lock now says what it
    covers. Not reproduced: there is no seam to hold the window open.
  - Part B, low, DOC: no file and an empty file have one revision, while the description said any
    change changes it. Fixed in words (the description and two doc comments) and pinned by a test.

  **What was not run.** The full `./scripts/check.sh`, the Windows lint and a mutation check are
  run after this text, so this text claims none of them. Nothing was looked at in a browser, and
  the agents ran nothing on Windows or macOS. End-to-end (`scripts/e2e.sh`) was not run. The suites
  `runtime_api.rs` (apart from its two new tests), `logic_module.rs` and `data_and_logic.rs`, and
  `runtime_vm.rs` were not run by the agents; they save without a revision, which still works, and
  CI runs them. Stage 2's `./scripts/check.sh --frontend` ran on the tree before its commit, not on
  the commit. The reviewers' timings are of a debug build, and of an optimised replica outside the
  crate on a throttled machine; no release timing of `mythos-core` itself was taken. Whether a
  union large enough to pass the 1 s exchange deadline restarts the module is read from `host.rs`,
  not reproduced. The cost of the lookup of a name in a reply is read, not measured.

  **What is left out.**
  - The logic UI's store keyed by runtime id (H8, LATENT): two components of one workbench that
    watch one runtime would overwrite each other in the store even though the core keeps both
    lists. For C8d-2 and D4b, which must key by list.
  - A save without a revision still overwrites (H11, LIVE for a caller that does not pass one):
    the descriptions say so; requiring it later is one line.
  - A gone watcher's names count against the 256 for up to 5 s (H3, LATENT): two near-full lists
    can refuse a third for that long; the refusal's words say the 5 s.
  - Lists surviving a hot swap or a new build (H6, LATENT, unchanged): `forget` of the old
    generation is skipped once a newer one reports, so lists survive; an answer of the old process
    carries its own state and version.
  - The bytes of a name (above, not fixed), the Unload question's span (above, not fixed) and the
    writers of `runtimes.json` outside the lock (above, not fixed).

  **Verification of the finished slice** (the orchestrator, after the review fixes, the stacking
  on the palette slice and the version line; this is what "run after this text" above was
  waiting for). The branch is stacked on PR #198 (library blocks in the palette), so every run
  below holds that slice too.
  - **The frontend gate** on `b42d0d8`: `./scripts/check.sh --frontend` printed "check.sh
    --frontend: PASS — every gate green", tree clean: `pnpm lint`, `pnpm test` (59 test files,
    887 tests) and `pnpm build`.
  - **Rust** on `b42d0d8`: `cargo fmt --all --check` clean; `cargo clippy -p mythos-core
    -p mythos-logic -p mythos-module-tests --all-targets -- -D warnings` clean;
    `cargo test -p mythos-core --lib` 95 passed; `cargo test -p mythos-logic` 79 passed;
    `cargo test -p mythos-module-tests --no-fail-fast` every suite green, among them
    `runtime_api` 16, `runtime_vm` 27, `logic_module` 12 and `data_and_logic` 18 (the suites the
    text above says the agents did not run). `cargo test -p mythos-core` (every test file of
    the crate) was green one commit earlier, before the added test below took its final form.
  - **The Windows lint** on `b42d0d8`: `win-clippy.sh` printed "win-clippy: GREEN".
  - **Mutations**, with `scripts/mutation-check.py`, in a copy made by `git archive` with its own
    install and target directory, both baselines green in every run. **27 of 27 red** in the end.
    - Frontend, 7 (4 of stage 2, 2 for review fixes, 1 of the orchestrator's), on `28b77eb`
      (the head of the branch before it was stacked again on the palette slice's corrections;
      the slice's own files were the same),
      against `runtime-load.test.tsx`, `runtime-editor.test.tsx`, `programs.test.tsx` and
      `runtimes.test.ts`: 7 red.
    - Rust, 20 (1 of stage 1, 6 of stage 2, 3 for review fixes, 10 of the orchestrator's written
      against the storage of the watches as the fixer left it, because the stage's own no
      longer applied), on `28b77eb`, against `runtimes::tests`, `mcp::tests`,
      `runtimes_config_api_tests` and the two new tests of `runtime_api`: 18 red, 1 did not
      compile, 1 survived.
    - Did not compile: the fixer's mutation for "a module that does not answer" took out the
      only caller of `watch_failed`, which the workspace refuses as dead code. Written again to
      fail the lists of another runtime id, it is red in
      `a_runtime_whose_module_does_not_answer_fails_every_list_watched_of_it`.
    - Survived, a real gap: "a name twice in a list counted twice when added". With it, a list
      that names a variable twice leaves that name read at every exchange after the list is let
      go, as long as another list of the runtime stays. No test asked for a list with a repeated
      name. The test `a_list_that_names_a_variable_twice_is_read_for_it_once_and_let_go_whole`
      was added. Its first form did not catch the mutation either (with no other list, the
      runtime's watches are dropped whole); with a second list it does: red on `b42d0d8`.
    - The frontend mutations and the other 18 Rust ones were not run again after `28b77eb`:
      since then the branch gained that one test and the palette slice's two corrections of
      test files.
  - **Read by one reader only.** The storage of the watches (`Watching`, `WatchRead`, the
    counts of names, the numbered reads) was made by the fixer after both reviews. The
    orchestrator read it line by line and wrote the ten mutations above against it; no second
    reviewer did.
  - **Not run by the orchestrator:** the full gate (`./scripts/check.sh`), macOS, and
    `scripts/e2e.sh`. No green full gate of this commit exists from this machine; CI is the
    gate. Nothing was looked at in a browser; the slice changes no screen except the words of
    two refusals.
  - **Not fixed, as above:** no bound on the bytes of a watch name (LATENT), the runtime
    card's Unload reads the revision after its question (LIVE, low; for C8d-2), and the writers
    of `runtimes.json` in the plant's install and restore are outside the lock (LATENT).
