# ADR-013 evidence: library packages P1

Part of [ADR-013](../ADR-013-logic-compiler-and-vm.md), section 5.

- **Library packages P1a** (the package file read and checked by the new crate
  `mythos-logic-package`, ADR-015 step 1, first of three slices, 2026-11-24). The crate takes the
  bytes of a package file and gives a value, or refuses the file with words that name what was
  wrong. Nothing uses the crate yet: no route, no MCP tool, no event, no storage and no user
  interface was added, and a project cannot hold a package until P1b. P1b (stored list, routes,
  tools, event) and P1c (compiled with the project, users of a package, build answer) are not
  built.

  **The crate.** `crates/mythos-logic-package` is a platform crate (std; dependencies `zip` 0.6
  with deflate only, `sha2`, `hex`, `serde`, `serde_json`; all already in
  `Cargo.lock`, which gains only the 11 lines of the crate's own entry). Its one entry is
  `read(file: &[u8]) -> Result<Package, String>`. It opens no file, reads no environment, builds
  no path from a name in the file and writes nothing. `id_problem` (R3) and `source_name_problem`
  (the module's `library_name_problem`, restated) are also public, for P1b. A `Package` has
  private fields and the methods `manifest()`, `sources()` (path and text, sorted by path in byte
  order), `sha256()` (of the whole file, 64 lowercase hex digits) and `size()`; it is not
  `Serialize`. `Manifest` and `Dependency` are `Serialize` and `#[non_exhaustive]`; an optional
  field that is absent is left out when serialised and never filled with a default.

  **A package file in format 1** is a zip of `manifest.json`, `checksums.json`, any number of
  `src/<name>.st` and optionally the directory entry `src/`. `checksums.json` maps every file
  but `src/` to its SHA-256 (64 lowercase hex digits). `manifest.json` has these keys, no other:
  - `format` (integer, 1), `id`, `name`, `version`, `form`, `languages`, `files`, `depends`,
    `license`: required.
  - `id`: two or more parts joined by `.`; a part is an ASCII letter then ASCII letters, digits
    or `_`; at most 64 characters; no part is a Windows device name (CON, PRN, AUX, NUL, COM0 to
    COM9, LPT0 to LPT9) in any case.
  - `name`, `license`: strings that are not empty (not only white space).
  - `version`: `MAJOR.MINOR.PATCH`, decimal, no leading zero, each at most `u64::MAX`, optionally
    `-<pre>` of letters, digits, `.`, `-` with no empty part and no numeric part with a leading
    zero; no `+`; at most 32 characters. Kept as text, never compared.
  - `form` is `"source"` and `languages` is `["st"]`, nothing else. `files` lists the
    `src/<name>.st` entries once each (without case). `depends` is a list of `{id, version}` by
    the rules above, no other key, not the package itself, no id twice (without case); `[]` if none.
  - `authors`, `targets` (lists of strings) and `description` (string): optional. `interface`:
    optional, only `[]` is taken. `signature`: optional, only `null` is taken (ADR-015 section
    2.8). The last two are reserved and are answered as given.

  **The bounds.** Each is a named constant, and every refusal of it states the value from the
  constant. All are design limits decided with this slice (contract section 9 item 5).
  - `MAX_PACKAGE_BYTES` 1_048_576, the file. Basis, as the contract gives it: the default body
    limit of the web framework behind module routes, 2_097_152, with a package sent as base64.
    That limit was read by the contract's author, not run here.
  - `MAX_PACKAGE_BASE64_LEN` 1_398_104 (4 times 1_048_576 over 3, rounded up): for P1b, not used.
  - `MAX_UNPACKED_BYTES` 2_097_152, the sum of all entries unpacked.
  - `MAX_PACKAGE_ENTRIES` 1024, the entries and the items of `files`. Basis, as the contract
    gives it: the compiler's bound of 1024 sources per compilation (`MAX_SOURCES`), so a library
    of several hundred blocks, one per file, fits. The constant is
    `mythos_logic_st::project::MAX_SOURCES` (`crates/mythos-logic-st/src/project.rs`), the lines
    between `FIRST_LINE` 2^30 and 2^31 divided by `SOURCE_LINES` 2^20: 1024 (read by the
    orchestrator; the document's author had looked in two other crates).
  - `MAX_PACKAGE_ID_LEN` 64, `MAX_VERSION_LEN` 32, `MAX_SOURCE_NAME_LEN` 64 (the logic module's
    bound for a library source name). `depends` has no count bound; its duplicate check is linear.

  **How the file is read.** The size is checked first. A walk of the end record, the central
  directory and each local header (`zip_layout`, written for this crate) runs before the `zip`
  crate sees the file, because reading `zip` 0.6.6 showed three hazards: `by_index` panics on an
  AES extra field without the encryption flag, the data start adds a zip64 offset from the file
  without a checked add, and `ZipArchive::new` allocates for an entry count the file declares.
  The walk requires: the end record in the last 22 bytes, no comment, one disk, no zip64, at most
  1024 entries, the central directory ending at the end record, each local header agreeing with
  its central entry, and the entries covering every byte before the directory once. The crate
  `zip` only decompresses and checks the CRC. Names are checked before any entry is read. An
  entry is read with `take(budget + 1)`, the budget being `MAX_UNPACKED_BYTES` minus the bytes
  already read, into an empty buffer; the declared size bounds nothing, and the bytes read must
  then equal it. Names and texts from the file are escaped and cut at 120 characters before they
  go into words. Order: size, structure, the format when the package holds one `manifest.json`
  (R1), names, entries, checksums, the other manifest fields, sources against `files`, UTF-8 and
  byte order mark, sort, SHA-256 of the file.

  **The refusals, grouped.**
  - The file: over the size bound; not a zip; bytes before, between or after the entries; a zip
    comment or an entry comment; zip64; more than one disk; disagreeing headers; overlapping
    entries; over `MAX_PACKAGE_ENTRIES` entries or `MAX_UNPACKED_BYTES` unpacked; an entry whose
    bytes are not the size its header declares.
  - An entry: a name that is not `manifest.json`, `checksums.json`, `src/` or `src/<name>.st`
    (exact case; a backslash, drive letter, `..`, absolute path, nested folder, other extension,
    device name or stem over 64 characters is not that); two names equal or differing only in
    case; a symbolic link or any file type but a plain file, whatever system made it; encryption
    or an AES field; an unsupported method; a data descriptor on a stored entry or one that
    disagrees with the central directory; an extra field other than `0x000a` (NTFS times),
    `0x5455` (time stamp) and `0x7875` (Unix owner); bytes in `src/`.
  - The format: not an integer from 1; above 1, told with its number, before the entries are
    read; a whole number from 2^64 told as "a format past 18446744073709551615" (the JSON reader
    does not keep its digits).
  - `checksums.json`: absent; not an object of 64-hex-digit values; a key twice; the manifest or
    a source not listed; a listed name not held; it lists itself or `src/`; a mismatch.
  - `manifest.json`: not JSON; a byte order mark; a key twice; an unknown key; a required field
    absent, of the wrong kind or empty; an id, version, form, language, `depends`, `interface`
    or `signature` outside the rules; `files` over the bound, naming a name twice or naming
    `manifest.json`, `checksums.json` or `src/`.
  - A source: not among `files`; a `files` item not held; not UTF-8 text; a byte order mark.
    CRLF is taken, byte for byte.

  **Tests.** `crates/mythos-logic-package/tests/read.rs` had 89 tests when this was written (95
  after the second review below); the run printed `ok. 89 passed; 0 failed`, and `ok. 3 passed` for three `compile_fail` doctests that prove a
  `Package`, `Manifest` and `Dependency` cannot be built outside the crate. Zips are built in
  memory by the `zip` writer, or byte by byte (`zip_by_hand`) for headers the writer never makes.
  - `a_valid_package_is_read_whole_with_its_sources_in_byte_order`: R9, every field, sorted
    sources, size, the manifest serialises back to the JSON given. Also
    `an_absent_optional_field_is_answered_absent_and_serialised_absent`,
    `the_reserved_interface_and_signature_are_taken_empty_and_answered_as_given`,
    `a_manifest_with_no_dependency_is_taken`, `a_package_written_by_hand_is_read`,
    `a_source_with_crlf_line_ends_checks_as_with_lf`,
    `the_sha256_answered_is_of_the_file_not_of_its_sources` (R8).
  - R1: `a_package_of_a_newer_format_is_refused_naming_both_numbers`, the two
    `..._refused_by_its_format_before_its_entries` tests, `a_manifest_with_no_format_is_refused`,
    `a_format_past_u64_is_refused_as_a_newer_format_stating_it_as_written`.
  - R2, R3: one test per manifest rule, each asserting the field named in the words (all eight
    required fields, kind, empty, form, languages, interface, signature, `depends`, unknown and
    twice-given keys), the id tests (one part, `CON`, a non-ASCII letter, an empty part, 64 taken
    and 65 refused), the version tests (build metadata, malformed, pre-release, 32 taken and 33
    refused, `a_version_semver_refuses_is_refused`), and the tests of the `files` bound and of
    `depends` duplicates and places.
  - R4: size over the bound (1_048_577 bytes, exact words);
    `a_package_that_expands_past_the_bound_is_refused_and_not_read_further` (deflated zeros with
    a wrong CRC, so a read to the end would give the CRC words); the unpacked bound taken and one
    byte more refused; 1024 entries taken and 1025 refused.
  - Damage: `every_prefix_of_a_valid_package_is_refused_and_none_panics`,
    `a_package_whose_entry_lies_about_its_size_is_refused_by_the_bytes_read`, and bytes before,
    after and a zip comment (section 9 items 7 and 8).
  - R5: names (`con.st` in three cases, `..`, absolute, backslash, drive letter, nested,
    extension, other files, stems of 65 and 64, case-only and exact duplicates); links
    (`a_symbolic_link_made_on_any_system_is_refused`, hosts 0, 2, 3, 5, 16, 19, 30, and the ASi
    one); `an_entry_of_another_file_type_is_refused`; the Unicode Path and the taken extra
    fields; encryption; a method; the two data-descriptor tests (a bsdtar file, one byte changed).
  - R6, R7: each checksum and manifest/entry case in both directions, the three wrong-words cases
    of `checksums.json` and `files`, a source that is not UTF-8, a byte order mark.
  - Other writers: `packages_written_by_other_zip_writers_are_read` (Info-ZIP with and without
    `-X`, bsdtar, 7-Zip, Python, as byte arrays in `tests/writers/mod.rs`) and
    `a_package_written_to_a_stream_by_bsdtar_is_refused_for_its_block_padding`.

  **Departures and decisions.**
  - The contract asks for a `[workspace.dependencies]` entry for the crate. It was added; cargo
    warned `unused workspace dependency`; it was removed. The root `Cargo.toml` gains only the
    member line, and P1b adds the entry with its first user.
  - Contract mutation M5 leaves a loop variable unused, which fails under `-D warnings`; the M5
    given to the orchestrator also requires `&& listed.is_empty()`. M7 is given against
    `if device_name(name) {` in `source_name_problem`. The bounded read of M2 is bound to a local
    so that its text stays on one line.
  - Item 8: the `zip` crate exposes `archive_offset` and `comment()`; the crate does not rely on
    them, because its own walk refuses those cases first.
  - The refusals above that the contract does not list (a key twice, a second entry of one name,
    zip64, headers that disagree, overlap, an AES field and the like) are the crate's own, each
    for a hostile or ambiguous file.
  - The directory entry `src/` is taken and is not listed in `checksums.json`. The name list is
    exact in case. The extra-field ids taken are those that Info-ZIP, bsdtar, 7-Zip and Python
    wrote in the review's fixtures; that is the basis, not a standard.
  - `git diff p1a-base -- Cargo.lock` showed 11 added lines and no removed or changed one.

  **The two reviews.** Both returned. The hostile-file review made four findings and the
  manifest and tests review eight; all twelve were fixed in the second commit.
  - A symbolic link of host 5, 16, 19 or 30 or by an ASi Unix extra field was taken (`unzip` made a
    link of it), and a Unicode Path field (`0x7075`) renamed an entry for four other readers.
    Fixed: the mode is read whatever the host; extra ids are an allow-list.
  - Three refusals said a held file is not held; a data descriptor was not compared with the
    central directory and was taken on a stored entry. Fixed.
  - The duplicate checks on `files` and `depends` were quadratic (10,000 files took 8.6 s, 20,000
    `depends` 10 s; debug build, this machine). Fixed by a bound on `files` and sets; afterwards
    80,000 files were refused in 0.08 s and 20,000 `depends` taken in 0.22 s (debug).
  - A `Package` could be built without `read`. Fixed by private fields and `#[non_exhaustive]`; a
    clone of a `Manifest` can still be edited, so P1b takes a `Package` where it needs the proof.
  - The format was not told before an entry refusal; a format of 2^64 or more was told as no
    format; versions SemVer refuses were taken (that `semver` refuses them was read, not run);
    two bounds were tested from one side; no test used another writer's zip. All fixed.

  **Not run.** The full gate, the Windows lint and every mutation are run after this text. The
  agents ran `cargo fmt --all --check`, `cargo clippy -p mythos-logic-package --all-targets -j 6
  -- -D warnings` and `cargo test -p mythos-logic-package -j 6`. Nothing was run on Windows or
  macOS by the agents; the crate uses no file system, path or environment. The red tests that the
  fixes predict for their mutations are predictions. The release-build time of the `files` and
  `depends` checks was not measured. A stored entry whose data holds a fake descriptor, aimed at
  a streaming reader, was not run.

  **Left out, with its class.**
  - P1b and P1c: not built (LATENT: nothing uses the crate).
  - NOT VERIFIED by a test: bytes between two entries and an entry comment (the review ran both
    and saw the refusals); a JSON key twice inside `checksums.json`.
  - NOT VERIFIED: that Windows and macOS give the same answers; that a second reader sees only
    the names `read` checked, beyond the cases the review ran.
  - Not fixed: control characters and bidirectional text in `name`, `license`, `description` and
    `authors` are taken, since the contract asks only that they are not empty; P1b and the UI
    must escape them (LATENT). `depends` has no count bound (LATENT).

  **A second review, of the fix commit alone** (one reviewer, asked for by the orchestrator: the
  twelve fixes above had been read by nobody but their author). Six findings, all corrected by
  the orchestrator in one commit:
  - High, LATENT: the fix had turned on serde_json's `raw_value` feature to read the format's
    digits. With it, every `Value` of a build reads an object of the one key
    `$serde_json::private::RawValue` as the value its string holds, so a manifest could give
    `files` or `depends` in a form every other JSON reader sees as an object, and a key twice
    inside that string was not seen. The feature is gone; a whole number from 2^64 is told as a
    format past `u64::MAX`, and one of more digits than the JSON reader takes is "not a package
    manifest: number out of range". Tests:
    `a_manifest_field_wrapped_in_a_private_key_of_the_json_library_is_refused`,
    `a_format_past_u64_is_refused_as_a_newer_format`,
    `a_format_of_more_digits_than_a_json_reader_takes_is_not_a_manifest`.
  - Medium, a regression of the fix: a package Info-ZIP writes to a stream was refused (its local
    header holds a zero checksum, a zero compressed size and the real size, beside a data
    descriptor). With a descriptor, each of the three is now zero or the central directory's.
    Tests: `a_package_written_to_a_stream_with_its_size_in_the_local_header_is_read`,
    `a_local_header_size_that_is_neither_zero_nor_the_central_one_is_refused`.
  - Medium, LATENT: the DOS directory bit of an entry's attributes was not read, and 7-Zip and
    bsdtar make a directory of such an entry. Refused now, except for `src/`. Test:
    `an_entry_whose_dos_attributes_mark_it_a_directory_is_refused`.
  - Low, TEST-ONLY: no test had an extra field in the local header alone. Test:
    `an_extra_field_that_only_the_local_header_holds_is_refused_by_its_id`.
  - Low: a format of 310 digits or more (above); and the words of `Manifest`'s documentation,
    which said a manifest is always one `read` took although a clone can be changed: corrected.
  - The reviewer's 145 crafted files (made with Info-ZIP, bsdtar, 7-Zip, Python and by hand) were
    run against the corrected reader by the orchestrator: no panic; the 42 taken are plain
    packages of those writers and the shapes the review lists as right to take; the files of the
    six findings give the answers above. Those files are not in the repository.

  **A red of this slice found by CI, and its correction.** On the first push, "Rust Workspace
  Test Suite & Lints" failed in `the_logic_crates_keep_their_layers`
  (`crates/mythos-logic-compiler/tests/layers.rs`): every `mythos-logic-*` crate must be listed
  there with what it may depend on, and the new one was not. It is listed now with no workspace
  dependency at all. The orchestrator had run the crate's own tests and `module_boundaries`, not
  the compiler crate's.

  **Verification of the finished slice** (the orchestrator; this is what "run after this text"
  above was waiting for).
  - On `e582335` (the corrections of the second review): `cargo fmt --all --check` clean;
    `cargo clippy -p mythos-logic-package --all-targets -- -D warnings` clean;
    `cargo test -p mythos-logic-package` 95 passed and the 3 `compile_fail` doctests;
    `cargo test -p mythos-module-tests --test module_boundaries` 3 passed;
    `cargo check --workspace --all-targets --locked` finished; the Windows lint printed
    "win-clippy: GREEN". After the layer was added: `cargo test -p mythos-logic-compiler --test
    layers` 2 passed.
  - **Mutations**, with `scripts/mutation-check.py`, in a copy of `e582335` made by `git archive`
    with its own target directory, both baselines green, against the crate's whole test suite:
    **29 of 29 red.** They are 12 of the stage, 11 for the first review's fixes (the twelfth no
    longer applies: its lines were replaced by the second review's correction) and 6 of the
    orchestrator's for that correction, one of them turning `raw_value` on again. The tool prints
    "did not compile" for one, the mutation that takes `#[non_exhaustive]` off `Manifest`: its
    log shows the doctest "Manifest - compile fail ... FAILED", which is the red; the tool
    mislabels a failing `compile_fail` doctest.
  - **Read by one reader only:** the orchestrator's correction of the second review's findings
    (`e582335`) was read by no reviewer; its six mutations above are its check.
  - **Not run by the orchestrator:** the full gate (`./scripts/check.sh`), `cargo test
    --workspace`, macOS. No green full gate of this commit exists from this machine; CI is the
    gate.
