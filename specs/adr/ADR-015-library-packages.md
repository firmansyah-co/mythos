# ADR-015: Library Packages

- **Status**: PROPOSED (design; nothing of it is built: §7)
- **Date**: 2026-11-24
- **Requested by**: Adrian (2026-11-23 and 2026-11-24, in Indonesian; the words quoted below are
  translated into English)
- **Written by**: Claude, from a design draft and the owner's answers to it

Builds on ADR-010 (modules are processes; every route has its MCP tool), ADR-011 (service
modules; the module package; the module feed of §2.5), ADR-012 (a project moves to a controller
as its files), ADR-013 (the compiler; §2.14, the standard library; §2.15 to §2.17, the TC6
documents) and ADR-014 (§2.7, the project's library; §2.9 item 2, which asked for this design).

**What the owner decided, and what the author added.** The owner decided: a library is added to
and removed from a project from a store or a custom endpoint, as CODESYS and Arduino have it; a
package holds sources in ST, FBD, SFC or LD and comes as sources or compiled with its I/O
parameters exposed, compiled so that a developer can publish a package closed source; the store
is run by the project and may be hosted on GitHub; installing shows a page of the package's
dependencies, each ticked by default, and the ticked ones are installed with it; OSCAT comes as
a package, not inside the compiler; the order of work of §4; and no reading of CODESYS library
files, the route for other tools being import of PLCopen XML. Everything else in this ADR is the
author's: what a removal, a name clash, an endpoint, an update and the lock mean (§2.1, list
A), the manifest's fields, the layout of the store, the assessments of the compiled form,
of CODESYS and of OSCAT, and which module owns what. Each is marked where it stands. Where the
owner has not answered, §5 asks.

## 1. Context

### 1.1 What a library is today

- **The standard library** is Structured Text inside the ST front end,
  `crates/mythos-logic-st/src/library/` (`bistables.st`, `counters.st`, `edges.st`, `timers.st`),
  embedded in the compiler (ADR-013 §2.14, `ADR-013-logic-compiler-and-vm.md:1397-1453`). Every
  name it declares is refused as the name of a program's POU or type (`:1434-1439`).
- **Intrinsics** (`__NOW()`) are refused by the lexer in a program and taken only in the
  standard library's text (`:1427-1433`). A package of sources reaches no intrinsic unless that
  rule changes.
- **The project's library** (ADR-014 §2.7): Structured Text sources of functions and function
  blocks in `<project>/logic/library/<name>.st`, no PROGRAM and no TYPE, compiled into every
  program (`plugins/logic/src/lib.rs:21-31`). A TYPE in it is refused because the data module
  owns the project's types (`plugins/logic/src/lib.rs:550-582`). A source name is a letter, then
  letters, digits or `_`, at most 64, never a Windows device name (`lib.rs:240-260`), and names
  that differ only in case are one name (`lib.rs:760-771`).
- The logic module's library routes, each an MCP tool: `GET /library`, `GET /library/:name`,
  `PUT /library/:name` (engineer), `DELETE /library/:name` (engineer), `POST /library/check`
  (`lib.rs:1649-1662`); the event `LOGIC_LIBRARY_CHANGED` (`lib.rs:66`). The logic module is at
  version 0.10.0 (`plugins/logic/Cargo.toml:5`).
- The workbench edits it in `frontend/packages/logic-ui/src/LibraryEditor.tsx`, `LibraryTree.tsx`,
  `librarystore.ts` and `library.ts`, in the tree section Library at order 25 (`entry.tsx:180`).
  Library blocks are not in the LD and FBD palette (`ADR-014-…md:433-434`).

### 1.2 How library sources enter a compilation

- `Sources { st, tc6, library, data }` (`crates/mythos-logic-compiler/src/lib.rs:45-57`): the
  library is a list of `(name, text)` of Structured Text.
- **One namespace.** Every POU and type of every library source joins the program's names; a
  name declared twice is refused, naming both sources (`crates/mythos-logic-st/src/project.rs:6-12`,
  `crates/mythos-logic-st/src/check.rs:364-371`). `NAMESPACE` and `USING` are parsed as not
  supported yet (`crates/mythos-logic-st/src/parser.rs:98-109`).
- The library is checked alone first, so it means the same in every program; everything is
  checked and only what a program uses is compiled (`project.rs:9-18`). A library source a
  program does not use does not change its bytecode.
- A built program's version is a SHA-256 over its bytecode, interval, bindings and data uses,
  truncated to 6 bytes (`plugins/logic/src/lib.rs:1175-1186`), without the compiler's version. The backend refuses an IR unit of another `IR_VERSION`
  (`crates/mythos-logic-backend-vm/src/lib.rs:383`). No language or compiler version constant
  exists that a package could name: UNKNOWN beyond `IR_VERSION`, the bytecode `format_version`
  of a target profile and the module protocol's `PROTOCOL_VERSION`.

### 1.3 What exists to build on

- **The module package** (ADR-011 §2.4; `crates/mythos-core/src/modules.rs:8-22`): a zip with a
  manifest and `checksums.json` (the SHA-256 of every other file). Installing checks every file
  against it and refuses an entry that would land outside its directory (`modules.rs:303-360`).
  Routes `POST /api/v1/modules/install` and `POST /api/v1/modules/install-url`, the second with
  the SHA-256 the download must have (`crates/mythos-core/src/api/modules.rs:199-238`).
- **The module feed** (ADR-011 §2.5, `ADR-011-…md:123-128`) is designed and not built
  (`:188`): only feeds an administrator configures are offered, and packages are signed before a
  public one exists. ADR-011 calls it a "module library"; the word collides with this ADR's
  subject (§5, question 7).
- **The updater** (`crates/mythos-update/src/release.rs:1-21`): a release feed of GitHub's shape,
  a `SHA256SUMS` file, install only after the checksum matches, and no fallback: an unreachable
  feed is an error. No signature is verified there or in `modules.rs`.
- **The slice** (ADR-012 §2.5; `crates/mythos-core/src/plant.rs:197-247`): a device is given all
  files of its project except `plant.json`, `*.tmp` and `.deploy*`, with a SHA-256 over every path
  and content. What is inside the project goes to the controller and into that hash; what is in
  a cache on the machine does not.
- **Routes carry JSON**: a module route's body is `Option<Value>`
  (`crates/mythos-module/src/protocol.rs:166`), so a package file reaches a module only encoded
  inside JSON. The size limit of a module route's body is UNKNOWN.
- **Targets**: `pc`, `rp2040` (64 KiB memory, 512 KiB code), `esp32` (128 KiB, 1 MiB)
  (`crates/mythos-logic-compiler/src/profile.rs:62-77`). Each built-in profile takes the one
  `FORMAT_VERSION` of the VM (`profile.rs:41-51`), and the bytecode is little-endian on every
  target, "so one file runs anywhere" (`crates/mythos-logic-vm/src/format.rs:19-21`).

## 2. Decision

### 2.1 The decisions, and whose each is

The owner's (O), in his words where they were given:

- **O1** (2026-11-23): "For the library I want to follow the concept of CODESYS / Arduino, where
  we can add / remove a library from a store / a custom endpoint."
- **O2** (2026-11-24): a package is sources in ST, FBD, SFC or LD. It can be compiled, with its
  I/O parameters exposed, or raw (source). Compiled because a developer may want to publish a
  package closed source.
- **O3** (2026-11-24): the store is run by the project; GitHub is acceptable as its host.
- **O4** (2026-11-24): as in CODESYS, installing shows a page of the dependencies; each is ticked
  by default, and the ticked ones are installed together when Install is pressed.
- **O5** (2026-11-24): OSCAT as a library (a package), not inside the compiler.
- **O6** (2026-11-24, agreed as the author proposed it): the order of work of §4.
- **O7** (2026-11-24, agreed as the author proposed it): no reading of CODESYS library files; the
  route is import of PLCopen XML (IEC 61131-10 / TC6), which the project's LD, FBD and SFC
  documents already are.

The author's (A), taken as recommended unless the owner objects:

- **A1** Removing a package is refused while a program uses one of its blocks, naming the
  programs (and, from step 2, the data blocks typed by one of its types).
- **A2** A name clash is refused at import, naming both places. IEC 61131-3 namespaces are a
  compiler slice before the store opens to other publishers.
- **A3** Only an administrator adds, changes or removes an endpoint.
- **A4** An endpoint may be a folder or a file share as well as an HTTPS address.
- **A5** Nothing is updated without a person confirming it.
- **A6** A package is copied into the project and pinned by exact version and SHA-256, so the
  project builds offline and goes to the controller whole.
- **A7** The logic module owns the project side; a new service module owns endpoints and
  downloads (ADR-010: every route has its MCP tool; modules do not call each other).
- **A8** The manifest names the form (`source` or `compiled`) and the languages from the first
  slice, although only `source` in Structured Text is built first.

### 2.2 The package

A package is a file: a zip holding a manifest, a checksums file in the shape of the module
package's (§1.3), and the package's sources. One package has one id and one version. Its
contents by step: functions and function blocks in Structured Text (step 1), user types (step
2), LD, FBD and SFC documents (step 4), the compiled form (step 6). Faceplates, device
descriptions and data blocks are not in a package; each may later be another kind of package
owned by its own module, sharing the format and the endpoints (the author's).

The manifest's fields (the author's; the names are proposals, the meanings are the design):

| Field | Meaning |
|-------|---------|
| `id` | The package's identity, reverse-domain like a module id; one name regardless of case |
| `name` | The name shown to people |
| `version` | Semantic version of the package |
| `form` | `source` or `compiled` (A8); only `source` is taken until step 6 |
| `languages` | The languages of its sources: any of `st`, `ld`, `fbd`, `sfc` (A8); only `st` until step 4 |
| `format` | An integer the logic module declares, raised only when the language the compiler takes changes in a way a package could notice; a package above it is refused |
| `depends` | Other packages by id, each with the versions it takes (§2.7, §5 question 1) |
| `targets` | Target profiles the publisher has built it for; advice, not a check (§2.13) |
| `license` | The license of the package, as text the publisher gives |
| `authors`, `description` | Shown to people |
| `files` | The sources, each a path inside the package |
| `interface` | For `form: compiled` only: the blocks and their I/O parameters (O2); empty for `source`, whose interface is read from its text |
| `signature` | Reserved; nothing verifies it before the IEC 62443 work (§2.8) |

### 2.3 Where a package lives in a project, and what add and remove mean

- An added package is copied into `<project>/logic/packages/<id>/<version>/`, and the project's
  list `<project>/logic/packages.json` names each package, its version, its SHA-256 and its origin
  (A6). The list is the lock of §2.5. A project holds one version of a package.
- Every check and build reads the packages from the project, never from a cache. The slice
  carries them to a controller, and they enter the slice's hash (§1.3).
- **Add** checks the file (§2.8), reads its manifest, checks its sources with the project's
  library and its other packages, refuses a name clash naming both places (A2), copies it in,
  writes the list and sends an event. A program's bytecode changes only when the program uses a
  block of the package (§1.2). Adding is an engineer's action, as `PUT /library/:name` is.
- **Remove** is refused while a program uses one of the package's blocks or while another
  package of the project depends on it, naming them (A1). Otherwise it deletes the copy and the
  entry. A runtime that is running is not changed: it runs bytecode already built, and a removal
  affects the next build only.
- A **detach** (copy the used blocks into the project's own library and remove the package) is
  the author's proposal for later, as an explicit action of its own (§5 question 6).
- The project's own library stays, as the place of blocks written in the project (ADR-014 §2.9).

### 2.4 Names and versions

- Until namespaces exist, a package's block and type names join the one namespace of §1.2; a
  name already declared by the standard library, the project's library, the data module's types
  or another package refuses the import (A2). Renaming on import is not done: it breaks the
  package's own references and its documentation.
- IEC 61131-3 ed. 3 `NAMESPACE` and `USING` are a compiler slice (N1 in §4) in the ST and TC6
  front ends and the palette. It comes before a publisher other than the project puts a package
  in the store (A2).
- A package id follows the library source name rules (§1.1) extended by `.` between its parts.
  Its version is semantic; a version is never published twice with different contents (the store
  refuses it, §2.6).
- Intrinsics stay the standard library's (§1.1). A package that needs one is not taken; whether
  OSCAT needs one is NOT VERIFIED (§2.11).

### 2.5 The lock of what was built

- `packages.json` pins each package by exact version and SHA-256 (A6). "Newest" is never
  resolved at build time; an update is an action a person confirms, shown with the difference of
  the sources (A5).
- The answer of a build will carry the logic module's version and the SHA-256 of each package
  the program used, so a build can be repeated. They do not enter the program's version hash
  (§1.2), which would change every program's version at every module release.
- Whether the compiler's output for one source is the same on Linux, Windows and macOS is NOT
  VERIFIED; a repeated build is only as exact as that.

### 2.6 Endpoints and the store

- An **endpoint** is a place that holds an index and package files. The index lists each package
  with its versions, each version with the file's location, its SHA-256 and its dependencies. The
  index is the same for the store and for every custom endpoint (O1); an endpoint is its address.
- **The store** (O3) is an endpoint run by the project. The author's layout for GitHub: a public
  repository whose index is a static file and whose package files are release files, fetched as
  plain files over HTTPS with no use of the GitHub API, so no API rate limit applies. A custom
  endpoint is another address with the same layout: a web server, a mirror inside a plant
  network, or a folder or file share (A4).
- **Offline**, a plant imports a package from a file (step 1) or a folder endpoint (step 3).
- **Closed-source packages that are sold** do not fit a public repository; they wait for the
  compiled form (step 6) and for a way to serve a file only to a buyer, which is not designed.
- Creating the store's repository is an outward act that waits for the owner's word. This ADR
  names no address.
- Endpoints are settings of the installation, kept by an administrator (A3). The store is
  configured by default and may be removed. A download is checked against the index's SHA-256
  and the package's checksums, and nothing is installed on a mismatch, with no fallback (as the
  updater does, §1.3).

### 2.7 The dependency page

- Installing a package from an endpoint (step 3) shows a page listing every package it depends
  on, directly or through another, that the project does not already have at a version that
  satisfies it. Each is ticked by default; Install installs the package and every ticked one
  together (O4).
- A dependency the person unticks is not installed, and the page says, before Install, that the
  project will not check until that dependency is there (the author's).
- The page names, for each, its id, version, endpoint, license and SHA-256. A dependency that no
  configured endpoint holds is listed as not found and cannot be ticked.
- Installed together means all or none: if one download or check fails, nothing is added.
- A file import (step 1) has no page: its manifest's dependencies that the project does not have
  are named, and the import answers that the project will not check until they are added.
- Which version a range resolves to when two packages ask for different ones is open (§5
  question 1).

### 2.8 Trust, and what waits for IEC 62443

A package of sources puts behaviour into a control program; it runs as VM bytecode, reaches no
intrinsic and no native code. The risk is that behaviour, and the host's reading of the zip. The
author's minimum, from step 1:

1. Every file checked against the package's checksums; from step 3, the package's SHA-256
   against the index; nothing added on a mismatch.
2. Zip entries that would land outside their directory refused, as `modules.rs` does.
3. Endpoints are an administrator's (A3); adding, updating and removing a package in a project
   is an engineer's.
4. Before a package enters a project the workbench shows its id, version, origin, SHA-256,
   license and the blocks and types it adds; on an update, the difference of its sources.
5. A package is never added by a build, by discovery or without a person's action (A5).
6. `packages.json` records where each package came from, so a review can see it.

What waits for the IEC 62443 work, which the owner deferred (ADR-014 §2.9): signatures, a list
of trusted publisher keys, a per-site policy (for example only signed packages) and an audit log
of who added what. The manifest's `signature` field is reserved and nothing verifies it before
then. The compiled form needs a signature (§2.9), so step 6 comes with signatures.

### 2.9 The compiled form (the author's assessment; not built, not designed)

- **One file for every target.** The VM's bytecode format is versioned and the same on every
  built-in target (§1.3: one `FORMAT_VERSION`, little-endian everywhere). This favours one compiled
  package for all targets rather than one per target.
- **No unit format and no link step.** The compiler builds a whole program from sources
  (`Sources`, §1.2). The bytecode is one program: bodies in a table, each callable only by later
  ones, every variable at a fixed absolute address (`format.rs:3-35`). Nothing in it can be
  placed into another program. A compiled package needs a unit that carries its blocks'
  interfaces and code with addresses to be fixed at a link step, and that link step; that is the
  work step 6 designs. Whether it would compile at the IR level instead of the bytecode level is
  UNKNOWN.
- **The protection is a license more than a technical one.** Bytecode can be read back
  instruction by instruction: the VM loads and checks it (`Program::load`, `format.rs:559`). The
  bytecode header holds no names (`format.rs:3-17`), so a reader sees code without the source's
  words; the logic is still there to be read.
- **A signature becomes necessary.** Code nobody can read goes into a controller, so whoever
  installs it must be able to know who published it and that it is unchanged. Signatures belong
  to the IEC 62443 work the owner deferred (§2.8).

### 2.10 CODESYS and PLCopen XML

The author's assessment, NOT VERIFIED against the vendor's documents: CODESYS library files are a
vendor format; its Structured Text has extensions this compiler does not take; and its libraries
lean on its own system libraries, which MythOS does not have. So reading those files would give
little that compiles. The route is import of PLCopen XML (O7): the project's LD, FBD and SFC
programs are already PLCopen TC6 XML v2.01 documents, the basis of IEC 61131-10
(`ADR-013-…md:1461-1470`). The vendor's license terms for its files were not checked. Whether
the TC6 reader takes documents exported by other tools as they are is NOT VERIFIED; step 4 tests
that.

### 2.11 OSCAT

ADR-013 §2.14 said OSCAT is to follow as the standard library, its license to be checked first.
The owner decided on 2026-11-24 that OSCAT comes as a package (O5). Its license is still to be
checked before any of its source is copied (NOT VERIFIED; ADR-013 records it as an Eclipse
project under EPL-2.0). Whether OSCAT compiles with this compiler is NOT VERIFIED: it may use
language this compiler refuses, or need an intrinsic (§2.4). OSCAT declares data types, which is
why types in a package come second in the order (§4). Whether a new project gets it by default
is open (§5 question 5).

### 2.12 Which module owns what (a plan; nothing of it is built)

- The **logic module** owns the project side (A7): the copies in `<project>/logic/packages/`, the
  list, add, remove, check, and building with them. It already owns `logic/library/` (ADR-014
  §2.8).
- A new service module, **`org.mythos.packages`** (name proposed), owns the endpoints (settings
  of the installation), the index, downloads, the checks of §2.8 and a download cache in its own
  system directory. Its routes are not under a project.
- The two do not call each other. The workbench, or an agent over MCP, takes the checked file from
  the packages module and gives it to the logic module, as it carries a built instance into the
  runtimes today (`plugins/logic/src/lib.rs:33-35`).
- From step 2 the **data module** reads the types of `<project>/logic/packages/` without writing
  them, as a contract named here (ADR-011 §2.2 allows a module to read a directory an ADR names),
  so a data block can be typed by a package's type. A package's type is read-only in the data
  module's editor.

Routes and tools each step will add (names proposed; each route an MCP tool, ADR-010 §2.5):

| Step | Module | Route | Role | MCP tool |
|------|--------|-------|------|----------|
| 1 | logic | `GET /packages` | viewer | `list_logic_packages` |
| 1 | logic | `GET /packages/:id` | viewer | `get_logic_package` |
| 1 | logic | `POST /packages/check` (a package file: what it adds, clashes, missing dependencies) | viewer | `check_logic_package` |
| 1 | logic | `POST /packages` (add from a file, base64 in JSON) | engineer | `add_logic_package` |
| 1 | logic | `DELETE /packages/:id` | engineer | `remove_logic_package` |
| 1 | logic | event `LOGIC_PACKAGES_CHANGED` | | |
| 3 | packages | `GET /endpoints` | viewer | `list_package_endpoints` |
| 3 | packages | `PUT /endpoints/:name`, `DELETE /endpoints/:name` | admin | `save_package_endpoint`, `delete_package_endpoint` |
| 3 | packages | `GET /index` (merged, each entry with its endpoint) | viewer | `list_package_index` |
| 3 | packages | `POST /resolve` (the dependency page's list for a package and a project's list) | viewer | `resolve_package_dependencies` |
| 3 | packages | `POST /fetch` (download, check, cache, answer the file) | engineer | `fetch_package` |
| 3 | logic | `PUT /packages/:id` (update to a confirmed version) | engineer | `update_logic_package` |

### 2.13 Every OS and microcontroller targets

- Package ids and file names follow the rules of §1.1: one name regardless of case, no Windows
  device name, paths inside a package separated by `/`, no symbolic links.
- Sources are UTF-8. The SHA-256 is over the package file as published, never over files
  checked out again, so a CRLF checkout on Windows does not change it.
- A package of sources is compiled for each target from source; nothing per target is shipped.
  `targets` is advice; the check is the profile's limits of memory, code and stack. A package
  using REAL runs in software on the RP2040 (`profile.rs:66-70`).
- A microcontroller never fetches a package: the workstation builds, and the controller receives
  bytecode. A node that builds receives the packages in its slice (§1.3).

## 3. Consequences

- A project builds offline and on another machine with the same packages, because they are in
  it; each project pays for its own copy in disk (NOT MEASURED).
- Until namespaces exist, two packages that declare one name cannot be in one project.
- A source package can be reviewed before it is added; a compiled one cannot, which is why it
  waits for signatures.
- CODESYS projects and libraries do not import as they are; what comes over is what a tool
  exports as PLCopen XML and this compiler takes.

## 4. Order of work

The owner's order (O6), each step cut into slices: one module change with its MCP tools, or one
UI piece. Each slice has its own evidence file.

| Slice | Step | What | Needs first |
|-------|------|------|-------------|
| P1 | 1 | Logic module: the package file (zip, manifest with `form` and `languages`, checksums), `packages.json`, the step-1 routes and event, packages compiled with the project, clashes and removals refused, origin and SHA-256 recorded, logic module version and package hashes in the build answer | §5 questions 2, 3 |
| P2 | 1 | CLI: pack a directory into a package, as `mythos module pack` does for modules | P1; the command's word (§5 question 7) |
| P3 | 1 | logic-ui: a Packages node in the Library section; add from a file, details, remove with the users named on refusal | P1 |
| P4 | 2 | Logic module and data module: types in a package; the data module reads them as a contract; a data block typed by one | P1 |
| P5 | 2 | data-ui: package types shown read-only | P4 |
| P6 | 3 | Packages module: endpoints (administrator), index, resolve, fetch, checks, cache, MCP tools | P1 |
| P7 | 3 | Workbench: browse endpoints, the dependency page, install, update with the source difference | P6, P3 |
| P8 | 3 | The store: the repository, its static index and the release files | P6; the owner's word to create it |
| N1 | before other publishers | Compiler: IEC 61131-3 namespaces in ST and TC6 and the palette | none; before the store takes another publisher's package |
| P9 | 4 | Logic module: LD, FBD and SFC documents in a package | P1; a library of TC6 documents, which today's project library does not have |
| P10 | 4 | Logic module and logic-ui: import of a PLCopen XML file from another tool into a program or a package | P9 |
| P11 | 5 | OSCAT as a package, published to the store | license checked; P4; P8; whether it compiles |
| P12 | 6 | The compiled form: its own design (unit format, link step), then its slices, with signatures | the IEC 62443 work; its own ADR |

Library blocks in the LD and FBD palette (ADR-014 D4c, not built) are not a step of this ADR; a
package's blocks join the palette when the project library's do. A package block is usable from
ST at once and from LD and FBD by name, as a project library block is today.

## 5. What is open

The draft's questions that the decisions above do not answer, each with the author's
recommendation.

1. **Dependency versions.** Does a dependency name an exact version or a range? Recommendation:
   a range within one major version; the dependency page proposes the newest version that
   satisfies every package of the project, and the lock pins what was installed. Two packages that
   need versions no one version satisfies refuse the install, naming both.
2. **Are A1 to A8 accepted?** Recommendation: yes, as written in §2.1.
3. **The `format` field.** Is it an integer the logic module declares (the author's), the logic
   module's semantic version as a minimum, or nothing? Recommendation: the integer, since it moves
   only when the language moves, not with every module release.
4. **Updating with dependencies.** When an update needs newer dependencies, are they ticked by
   default on the same page as for an install? Recommendation: yes, the same page.
5. **OSCAT by default.** Once it is checked and builds, is it added to every new project?
   Recommendation: no; it is offered on the page of a new project, since a project that adds it
   takes its names into the one namespace until N1.
6. **Detach.** Is copying a package's used blocks into the project's own library an action to
   build? Recommendation: yes, after step 3, as an explicit action.
7. **Words.** Is the user's word "library package", with ADR-011's "module library" renamed
   "module feed", and is the CLI command `mythos library pack`? Recommendation: yes, to keep the
   two apart.
8. **Sold closed-source packages.** Is a private endpoint per vendor the intended way, or does
   the store sell? Recommendation: a private endpoint per vendor; the store stays free and public.

## 6. NOT VERIFIED

- Every CODESYS and Arduino statement of §2.10 and §8 comes from search snippets of public help
  pages, not full pages read; the "newest version" reference form of CODESYS is from general
  knowledge; that the Arduino library manager accepts no custom index was read in a third-party
  mirror of the registry's FAQ.
- The license of CODESYS library files, and of OSCAT.
- Whether OSCAT compiles with this compiler, and whether it needs an intrinsic.
- Whether the TC6 reader takes PLCopen XML exported by other tools as it is.
- That GitHub release files are served with no rate limit of their own when fetched without the
  API.
- Whether the compiler's output for one source is the same on every OS.
- Whether a module crate may depend on `mythos-update` or on the module package code under the
  boundary guard (ADR-011 §2.6); the install code is in the core today.
- The body size limit of a service module route.
- Whether a whole project can be exported or imported as a file today: UNKNOWN; none was found.
- Whether a compiled unit would sit at the IR or the bytecode level.

## 7. Status

Nothing of this ADR is built. The project's own library (ADR-014 D4c) is built and unchanged by
it. The routes, tools, fields and module named here are a plan. The store does not exist.

## 8. Public concepts referred to

Concepts in the author's own words; no wording, code or file format is copied from a product.

- A CODESYS-style library manager: a project lists the libraries it references by name, vendor
  and version, or "the newest installed"; libraries live in a repository on the engineering
  machine; a library may be installed compiled; each has a namespace; a placeholder lets the
  device decide which library a name resolves to; libraries arrive through a package manager and
  a store.
  https://content.helpme-codesys.com/en/CODESYS%20Development%20System/_cds_obj_library_manager.html,
  https://content.helpme-codesys.com/en/LibDevSummary/placeholder.html,
  https://content.helpme-codesys.com/en/CODESYS%20Development%20System/_cds_cmd_library_repository.html,
  https://help.plc.abb.com/AB270_en/_cds_cmd_package_manager.html,
  https://product-help.schneider-electric.com/Machine%20Expert/V1.1/en/LibDevSummary/topics/prefix.htm
- The Arduino library manager: a central index of libraries and versions, each with a download
  location and a checksum; a manifest in each library naming its dependencies, installed with it
  by default; libraries shared by every sketch on the machine; install from a zip with no index;
  a user's own index URL exists for boards, not for libraries.
  https://docs.arduino.cc/arduino-cli/library-specification,
  https://docs.arduino.cc/arduino-cli/package_index_json-specification,
  https://www.github.com/arduino/library-registry/blob/main/FAQ.md
- PLCopen TC6 XML v2.01 and IEC 61131-10, as ADR-013 §2.15 records them.
- OSCAT: https://eclipse.dev/oscat/ (from ADR-013 §2.14).
