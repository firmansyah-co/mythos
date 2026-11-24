# ADR-014: Data Blocks, User Types and the Data Module

- **Status**: ACCEPTED (design; of its slices D1, the compiler, D2, the runtime's hand-over of
  a data block, D3, the data module and the logic module's use of it, D4a, the workbench's
  editors for user types and data blocks, and D4c, the library editor in the logic module's
  workbench UI, are built: §6)
- **Date**: 2026-11-22
- **Requested by**: Adrian ("kalau aku mau ga area bersama gitu, tapi ada modul data (db) dimana
  kita bisa susun UDT dan sejenisnya"; "gimana biar hampir instant dan ga kena exchange
  20-200ms"; "oke ikuti saranmu. tapi kita harus kembangkan untuk antar runtime dan antar device
  juga dalam waktu menengah")
- **Written by**: Claude, implementing the request

Builds on ADR-010 (modules are processes; the tag bus keeps one writer), ADR-012 (nodes) and
ADR-013 (the logic compiler and VM; §2.19, task configuration).

**What the owner decided, and what the author added.** The owner decided: no shared memory
area; a data module in which user types and data blocks are made; a copy per program with one
writer per member, handed over inside the runtime without an exchange; and the same between
runtimes and between devices in the medium term. The author added: when a hand-over is made
(§2.3: a later program of the same task sees an earlier one's writes at once; other tasks see
a task's run when the whole task run has ended); the library of function blocks and functions
(§2.7); data blocks typed by a function block (§2.6); and tier 3 waiting for the security
foundation (§2.4, §4). The owner answered each on 2026-11-23: §2.9 holds his words and what
follows from them.

## 1. Context

A runtime runs several programs in tasks (ADR-013 §2.19). Programs do not share memory:
`VAR_GLOBAL` and `VAR_EXTERNAL` are refused by both front ends, each program has its own
memory, and the only way a value gets from one program to another is a tag: an output or a
published variable of one, an input of the other, carried by the core's exchange. That path has
two limits.

1. **It is slow.** A value crosses at the exchange period of the runtime, tens to hundreds of
   milliseconds, not at the next task run.
2. **It carries one scalar.** A tag holds one value of at most 64 bits (`TagSlot`, 64 bytes,
   `crates/mythos-shm/src/slot.rs`). A structure, an array or a STRING cannot cross, and the
   members of a structure sent as separate tags may arrive from different scans.

The controllers the owner knows solve this with a shared memory area (markers, global data
blocks) that every block reads and writes at any time. With preemption that area is where a
program reads a structure another program has half written. The owner's decision is to have no
shared area, and to have named data blocks built from user types, in a module of their own.

## 2. Decision

### 2.1 What a project has

- A **user type** (UDT) is a type of the project (`TYPE … END_TYPE`: a structure, or a name
  for an array type), usable by every program of the project. Today a type is visible only in
  the source that declares it.
- A **data block** (DB) is a named instance of a structure type, with initial values, and a
  setting for whether it is retained. It belongs to the project, not to a program. From slice
  D5 on its type may also be a function block type (§2.6). A data block has members because
  a member is what has a writer (§2.3).
- There is **no shared memory area**: no `%M` markers, no variable that is not a member of a
  named data block, no address a program can reach that it did not declare.

### 2.2 How a program uses a data block

A program declares the data blocks it uses with the standard's own section:

```
VAR_EXTERNAL
    Recipe  : RecipeData;        (* read and, for the members it owns, written *)
END_VAR
VAR_EXTERNAL CONSTANT
    Line    : LineState;         (* read only *)
END_VAR
```

and uses `Recipe.speed` like a variable of its own. The declaration must name a data block of
the project and its type; a name that is no data block, or another type than the block's, is
refused at build with both named. The TC6 documents carry the same section (`externalVars`),
which the reader refuses today and will take.

`VAR_GLOBAL` in a source stays refused. A global declaration is made in the data module, not
in program text: the data block is the global declaration. `CONFIGURATION` text stays out
(ADR-013 §2.19).

### 2.3 One writer per member

- Every program has **its own copy** of each data block it declares, inside its own memory, at
  an address the compiler assigns. A program's code touches only its copy.
- The runtime keeps one **master copy** of each data block.
- **A member has one writer.** For each top-level member of a data block the build collects the
  programs that write it (an assignment to it or to a part of it, passing it as `VAR_IN_OUT`
  or `VAR_OUTPUT`, a call of it when it is a function block instance). A member written by two
  programs is refused, naming both. The unit is the top-level member: a structure member or an
  array member counts as a whole, because an index computed at run time cannot be attributed.
- A member no program writes can be marked **writable from outside** in the data block's
  definition: a setpoint or a recipe value. It is written through the path a published
  writable variable has today (the runtime applies it between runs), never by a direct write
  into a running program's memory. A member that is neither written by a program nor writable
  from outside keeps its initial value, and the build says so as a warning.
- When a program **starts in a task run**, the runtime copies into its copy every member it
  does not write: from the copy of the member's writer when that writer is an earlier program
  of the same task that has already ended in this task run, otherwise from the master copy.
- When the **task run ends**, the runtime copies, for each program of the task, the members it
  writes from its copy into the master copy, in the same step in which the task's outputs are
  written. A task run that faults, or that a change of mode cut short, copies nothing into the
  master. All copies are made by the scheduler thread between pieces of execution, so no
  program is running while a copy is made.
- (As first written, this section handed over when each *program* ended. That cannot be kept
  together with "a faulted run hands nothing over": when the second program of a task faults,
  the first has already handed over. D2 was built with the rule above.)

What follows:

- A reader in another task sees, for all members written by the programs of one task, the
  values of **one** completed run of that task. A preempted task has published nothing of the
  run it is in the middle of. A half-written structure cannot be observed, and neither can the
  first program's members of a run whose second program has not ended.
- Within one task the programs run in order, and a later program sees what an earlier one
  wrote in the same task run. Across tasks a program sees what the writer's task published at
  the end of its last completed run.
- The delay from a write to its use is the wait for the reader's next start, not an exchange
  period. Nothing crosses a process boundary and the core is not involved.
- A program's inputs from the plant stay fixed per **task** run (the process image, ADR-013
  §2.19). A data block member is taken in per **program** run and published per **task** run:
  an input changes outside the controller and must be the same for every program of the task;
  a data block member changes only when its one writer has run, and the task publishes it
  together with its outputs.

This is the rule IEC 61131-3 gives for data between program organization units in tasks: a
consumer's inputs from one producer all come from the same evaluation of it.

### 2.4 Three tiers

The same model at three distances. The program text is the same in all three; what differs is
where the master copy is and what the reader learns about its age.

| Tier | Distance | Carrier | Age of what a reader sees | When |
|------|----------|---------|---------------------------|------|
| 1 | programs of one runtime | a copy inside the runtime module's process | the writer's last completed run | first, before C8d |
| 2 | runtimes on one device | a data segment in shared memory, one writer process | the writer runtime's last publication, plus its age | medium term (§4) |
| 3 | devices | a publication over the network between nodes | the last message, plus its age | medium term (§4) |

**Tier 1** is §2.3 and is what gets built now.

**Tier 2, designed, not built.** A data block whose writer is in another runtime than a reader
lives in a **data segment**: a shared memory object separate from the tag bus, created through
the same portable layer (`DefaultShm`), holding the block's bytes and a header with the layout
hash (§2.5), a sequence counter, the writer's count of publications and a timestamp. The
writing runtime's process publishes into it when the writing task's run ends (§2.3); a reading
runtime's process copies from it when a reading program starts, retrying while the sequence
counter shows a publication in progress. Rules:

- **One writer process per segment.** A data block written by programs of two runtimes is
  split into one segment per writing runtime.
- **ADR-010 §2.2 keeps its meaning and is made exact.** "Modules never write the tag bus"
  stays true: the tag bus has one writer, the core. A data segment is not the tag bus. It has
  one writer too, the runtime module process that runs the writing program. This is the one
  place a module writes shared memory, and the ADR says so here instead of leaving it to be
  found in code.
- **A reader knows the age.** Runtimes are not synchronized: a reader may see the same
  publication twice or miss one. Each data block read across runtimes has a status the
  program can test (good or not, and the age of the publication), and a time-out in its
  definition. What a member reads when the time-out has passed is set per data block: hold the
  last value, or take the declared substitute values. The default is the substitute values:
  in doubt, a value is not presented as current.
- A stopped or faulted writer runtime stops publishing; the readers see the age grow and then
  the time-out. The core restarting does not interrupt a segment between two running runtimes.

**Tier 3, designed in outline, not built.** A data block is published from the node that
writes it to the nodes that read it, at a period set in its definition, with the same header
(layout hash, publication count, timestamp) and the same status and time-out on the reading
side. Two things are fixed now: the reading side behaves exactly as tier 2 (status, age,
time-out, substitute values), so a program does not change when a data block moves from
another runtime to another device; and **tier 3 is not built before the security foundation
(IEC 62443)**, because a data block received from the network that sets values in a running
program is a control path, and an unauthenticated one is not acceptable. The transport
(a publication of our own over UDP, or OPC UA PubSub) is decided when OPC UA is designed, not
here.

### 2.5 Layout and change

- The layout of a type is what the compiler computes today: members in declaration order,
  packed, no padding (`crates/mythos-logic-backend-vm`). This ADR makes that layout a
  **contract** for data blocks and gives it a **layout hash**: a hash over the member names,
  types, offsets and sizes, recursively.
- A compiled program carries, for each data block it declares, the block's name, the layout
  hash, the address of its copy, and which members it writes.
- A runtime is refused a configuration in which two programs disagree on a data block's
  layout hash, or a program's hash differs from the data block's definition. The message names
  the programs to build again. A type changed in the data module makes the programs that use it
  out of date; the logic module shows that, and builds them again on request, never silently.
  What is built (D3): the data module answers, for a save of a type or a block, which blocks
  changed their layout (`changed`), and a build carries the layout of each block in its
  `data`. Comparing that with a running runtime's status, and offering to build again, is left
  to the editor and the runtime slices.
- At an online change the master copy carries on member by member, by path and type, the way a
  program's variables carry on today. A member whose type changed takes its initial value.
- In tiers 2 and 3 a reader whose layout hash differs from the header's reads nothing and
  reports the data block as not good.
- A retained data block is kept with the runtime's retained values
  (`<system>/retain/<project>/<runtime>.json`, ADR-013) by the runtime that holds its master
  copy, and restored at a warm restart.

### 2.6 A data block typed by a function block (instance data block)

Today the data of a function block instance lives inside the program that declares it and
nothing outside sees it except as watched variables. A data block may have a function block
type. Then:

- one program calls it (`Motor1(start := …);` on the `VAR_EXTERNAL` declaration), and that
  program is the writer of the **whole** block, since the block's code writes its own state;
- every other program may declare it `VAR_EXTERNAL CONSTANT` and read its outputs and public
  variables, with the consistency of §2.3: the values of one completed call;
- a second program calling the same instance is refused, naming both.

This gives the instance data block of the controllers the owner knows, as a project object
that an HMI faceplate can bind to by name, without a second writer.

### 2.7 A library of blocks

A data block typed by a function block needs the function block to be known outside one
program's source. So the project gets a **library**: sources without a `PROGRAM` (function
blocks, functions) that every program of the project is compiled with. The compiler takes
several sources in one compilation; exactly one of them holds the `PROGRAM`. This closes the
item ADR-013 §5 lists as not built ("a library of POUs shared by a project's programs").
A library source holds functions and function blocks, and a `TYPE` in it is refused: the data
module is the one owner of the project's types (D3).

### 2.8 Which module owns what

- The **data module** (`plugins/data`, a service module under ADR-010, hot-swappable, with an
  MCP tool for every route) owns user types and data blocks: it stores them under
  `<project>/data/` (`types/<name>.st`, `blocks.json`), checks them with the compiler crate,
  and is their only writer on disk. Later it owns the tier 3 publication between nodes.
- The **logic module** owns programs and the library of function blocks and functions
  (`<project>/logic/library/`), since they are code. It **reads** `<project>/data/` when it
  checks or builds a program. The project directory is the contract between the two modules:
  neither calls the other, and a program builds with the data module unloaded.
- The **runtime module** holds the master copies and does the hand-over (tier 1) and the data
  segments (tier 2). It declares what it supports in its manifest profile, the way it declares
  task kinds, and the core refuses a configuration with data blocks to a module that does not
  declare them.
- The **core** validates a runtime's configuration (one writer per member across the
  runtime's programs, agreeing layout hashes), keeps retained values, and gives the workbench
  each member's writer from the configuration; the live values of a member are those of a
  program's own copy, which the watch of that program's variables reads, and the core has no
  route that reads a master copy. A scalar member can be bound to a tag like a published
  variable; a structure, array or STRING member cannot, because a tag cannot hold it.
- The **workbench** gets an editor for types and data blocks in the data module's own UI
  (ADR-011), which is built (D4a, §6), and an editor for the project's library in the logic
  module's own UI, which is built (D4c, §6), and shows live values of a data block per runtime,
  which is not built (D4b).

### 2.9 What the owner said of the author's additions (2026-11-23)

The four points the author added (the paragraph before §1) were put to the owner. His answers,
in his words, and what the author reads from each. Where a reading is the author's, it says so.

1. **When a value is handed over.** "In the same runtime as instantly as possible. In the same
   device as fast as the core. Between devices the network plus 100 ms of latency at most."
   - *One runtime (tier 1, built).* The author's reading: §2.3 is this. A later program of the
     same task sees a write at once; a program of another task sees it at the end of the
     writer's task run, which is the earliest moment at which it cannot see half of a run. No
     period of its own is added.
   - *One device (tier 2, not built).* A value crosses between two runtimes at the speed at
     which the core exchanges with them, with no slower period of its own. The figure is
     NOT MEASURED; T2's design publishes it and tests it.
   - *Between devices (tier 3, not built).* What MythOS adds to the network's own latency is at
     most 100 ms. How it is measured is T3's design.
2. **The library.** "As CODESYS and Arduino have it: a library is added to and removed from a
   project, from a store or a custom endpoint." This is more than §2.7 builds. Built: the
   project's own library sources and their editor (D4c). **Not built and not designed:**
   library packages (a name, a version, the function blocks and functions they hold), the
   places they come from (a store, an endpoint the user gives), adding one to and removing one
   from a project, how a project records which version it uses, and how what is downloaded is
   trusted. It gets its own design before any code. The author's reading: the project's own
   sources stay, as the place of blocks written in the project. The design of library packages
   is ADR-015. P1a and P1b of ADR-015 are built (evidence 35); programs are not compiled with
   packages yet.
3. **Memory.** "A function block has memory, as a data block has. A function has none. A
   program's memory is the program's own." §2.6 follows this: a data block typed by a function
   block is that memory as an object of the project, and D5 stays in the order of work. The
   author's reading: an instance declared inside a program stays in that program's memory, as
   IEC 61131-3 has it, and is not made a data block by itself.
4. **Tier 3 after the security foundation.** "We discuss this when IEC 61499 is about to be
   implemented." So the order "T3 after IEC 62443" in §2.4 and §4 is the author's proposal and
   is not decided.

The owner also deferred all medium-term work on the same day (alarms and the historian, T2,
IEC 62443, T3, OPC UA, firmware) until the application as it is has been tested by him. The
order of §4 holds among those items; when they start is his to say.

## 3. Consequences

- A value passes between programs of one runtime within one task run, with no exchange and no
  core involvement, and a structure passes whole.
- Every member has one writer that the build can name. "Who changes this value" has an answer
  without reading every program, which is what a shared area cannot give.
- Each program run costs two copies of the data blocks it declares. **NOT MEASURED.** The cost
  grows with the size of the blocks a program declares, not with the size of all blocks. On a
  microcontroller it is paid in RAM too: one copy per declaring program plus the master.
- A program that only needs to read one member still receives the whole block. Splitting large
  blocks is the user's remedy; copying only the members a program uses is a later optimisation
  and changes nothing a program can observe.
- Two programs cannot both write one member, even when the user knows they never run at the
  same time. The remedy is the usual one: one program owns the value, the other requests a
  change through a member it owns.
- Programs written for a shared-area controller (markers, `VAR_GLOBAL` lists) do not import
  unchanged.
- An IEC 61499 application (ADR-013 horizon) passes data on event connections and does not
  need this; a data block can still be read by a 61499 resource the same way a program reads
  it. Nothing here stands in its way.

## 4. Order of work

Each is one slice, built through the workflow topology, with its own evidence file.

| Slice | What | Depends on |
|-------|------|------------|
| D1 (done) | Compiler: several sources in one compilation; `VAR_EXTERNAL` against given data block declarations; the copy's address, the layout hash and the written members in the compiled program | — |
| D2 (done) | Protocol, runtime module and core: data blocks in a runtime's configuration, master copies, hand-over at program start and task end, the one-writer and layout checks, retained blocks; a program's copy can be watched | D1 (C8b is built) |
| D3 (done) | Data module: store, check, routes and MCP tools; the logic module builds with `<project>/data/` and its library | D1 |
| C8c (done) | Logic module: runtimes and task configuration (ADR-013 §2.19) | D2, D3 |
| D4a (done) | Workbench: editor for user types and data blocks, in the data module's UI | D3 |
| D4b | Workbench: live values of a data block, per runtime | C8d-2; the change in the core so that two watchers of one runtime do not replace each other's list is built (C8d-core) |
| D4c (done) | Workbench: the library editor (functions and function blocks), in the logic module's UI | C8c |
| C8d-1 (done) | Workbench: edit and load runtimes and tasks (ADR-013 §2.19), in the logic module's UI | C8c, D2 |
| C8d-2 | Workbench: the state of a task, and watch and force of a program in a configured runtime | C8d-1, and the core's watch lists |
| D5 | Data blocks typed by a function block (§2.6) | D2, D3 |
| T2 | Tier 2: data segments between runtimes of one device | D2; after alarms and historian |
| T3 | Tier 3: data blocks between devices | T2 and the IEC 62443 foundation |

T2 and T3 are the owner's medium-term requirement and are scheduled, not optional: T2 follows
alarms and the historian, T3 follows the security foundation and is designed together with
OPC UA.

## 5. Not decided here

- The transport of tier 3 (§2.4).
- The names a program uses to test the status and age of a data block read across runtimes
  or devices. They are needed first in T2 and are decided there.
- Whether a data block can be placed in a specific runtime by the user or always follows its
  writers. In tier 1 it lives in every runtime that has a program declaring it, and a data
  block declared by programs of two runtimes is refused until T2 is built, with a message that
  says so.
- Copying only the members a program uses.
- Enumerations and subrange types: the compiler has neither today, and this ADR adds neither.
- `VAR_EXTERNAL` in a function block or a function. D1 takes it in a `PROGRAM` only; a block
  is given a member as a parameter.

## 6. Status

**D1 is built** (evidence: [ADR-013 evidence, D1](ADR-013-evidence/25-d1.md)). The compiler
takes library sources and data block declarations (`Sources.library`, `Sources.data`), takes
`VAR_EXTERNAL` and `VAR_EXTERNAL CONSTANT` in a `PROGRAM` in Structured Text and
`externalVars` in a TC6 document, says for each data block a program declares where its copy
is, its layout hash and which top-level members the program writes (`Compiled.data`), and
lays a project's blocks out with no program (`check_project`). The library is checked alone
first, so it means the same in every program; what a program does not use adds nothing to
what is compiled, and a program without library or data blocks compiles to the bytes it
compiled to before.

**D2 is built** (evidence: [ADR-013 evidence, D2](ADR-013-evidence/26-d2.md)). The module
protocol takes a runtime's `data` (the blocks, each with its layout hash and `retain`) and, in
each program, the `DataUse` the compiler gave it; `check_data` refuses a bad block name, a
layout or members that differ from the program's, a CONSTANT block written, a member with two
writers, an input or a setpoint bound to a member, and members that overlap. The VM runtime
module (0.6.0, profile `data` tier 1) keeps one master of each block that a program declares,
and each program has its own copy in its image. A task run hands over as a whole at its end:
the master and the images change together, so a program of another task never sees a block
half written, and a later program of the same task takes an earlier one's members at once.
A run that faults hands nothing over. A forced member wins over the hand-over. A member has one
writer. The module also refuses a program whose code declares a block, or has a member, that
its `data` leaves out, and a task run in which a program changed a member its `data` says it
does not write faults the runtime instead of handing over. After a restart every copy is set from the master; a RETAIN block keeps its master
across a warm restart when its layout and size are the same; an online change composes each
master again from the writers' copies and adds nothing on the wire. The core takes `data` in
`runtimes.json` and in the MCP schema, refuses a block in two runtimes (R16) and a runtime
module that does not say it hands data blocks over (R17), and its status names each member's
writer. A runtime without data blocks runs and serialises as before. What a hand-over costs
per program run, and how long the lock is held for it, are NOT MEASURED. A kept value of a
RETAIN block that the module does not take is only a warning in the module's log.

**D3 is built** (evidence: [ADR-013 evidence, D3](ADR-013-evidence/27-d3.md)). The data
module (`plugins/data`, `org.mythos.data`) stores a project's types as `data/types/<Name>.st`
and its data blocks as `data/blocks.json`, checks every write with the compiler over the whole
new state and refuses one that leaves it not checking, answers each block's layout hash, size
and members and which blocks changed their layout, and has eight routes, each an MCP tool.
Its writes hold a lock between processes. The logic module (0.8.0) has a library of functions
and function blocks in `logic/library/`, refuses a `TYPE` in it, reads `<project>/data/`
without writing it, and builds a program with the project's types, library and blocks: the
instance carries `data` and the program's `data`, and the build is refused when the protocol's
`check_data` refuses it. When the stored data does not check, a program without `VAR_EXTERNAL`
still builds and the answer carries `project_problems`. The two modules meet only in the
project's files, and a program builds with the data module unloaded. A project without data
or library builds to the bytes and the version it had. Two programs that share a block are built
into one runtime by the logic module since C8c: the project's runtimes and their tasks are in
`logic/tasks.json`, a runtime build answers one instance with the union of its programs' blocks
once each, and the caller saves that instance and removes the ids in `replaces`. The module does
not load it, and a block shared between runtimes is still refused by the core (R16). The core also refuses a save in which one project program is in two runtimes, when both instances name it as built by the logic module (since 0.8.0; an instance built before does not name it and is not compared), so a program that shares a block is not run twice. The
workbench draws only a program's own diagnostics on its text and lists another file's apart (a
library source's as a button that opens the library editor at its line), and it lists
`project_problems` apart. The cost of checking the data before each compile is NOT MEASURED. Since
C8c the logic module's writes hold a lock between processes, as the data module's do; a runtime
build reads without it, and a write by another process during a hot swap could give one build a
torn view (LATENT, not reproduced).

**D4a is built.** The data module ships a workbench UI of its own, `frontend/packages/data-ui`
(`@mythos/data-ui`), which the shell loads from `modules-ui/org.mythos.data/` without a change to
the module's manifest. The project tree has a section Data with User types and Data blocks. A
user type is edited as its Structured Text, checked as a draft, saved and deleted; a data block
is edited as a type and an explicit choice of retain, and shows the layout, size and members that
the module answered, NOT KNOWN when the stored state does not check. A refused save shows the
module's words and diagnostics, and what is shown as saved is what the module answered, read
again after a type save. Each block is compared with the blocks that the open project's runtimes
configuration carries, and the most a row says is "laid out as saved": an initial value is outside
the layout hash, and a configuration is "configured", not "running". For this, a refusal thrown by
`host.api` carries its status and body. The checks in a real browser are NOT VERIFIED
(ADR-013 evidence, `30-d4a.md`). Not in D4a: rename, reorder, per-block initial values, nested
members, where-used, and a revision check, so the last save wins (LATENT).

**D4c is built.** The logic module's workbench UI, `frontend/packages/logic-ui` (version 0.9.0,
with `plugins/logic` at 0.9.0), has a section Library in the project tree (order 25) and an editor
for a library source, in Structured Text, the only language a library source can be written in. A
list that could not be read is said and not shown as empty. Save first checks the text with
`POST /library/check`; a text that does not check is not saved unless the engineer confirms Save
anyway, since the module itself saves any text and a change across two sources needs one save that
does not check. What is shown as saved is read again from the module, and a refusal shows the
module's words. An ok check says the library checks and nothing about programs: the module's check
compiles no program. The programs follow the library and the project's types: on
`LOGIC_LIBRARY_CHANGED` and `DATA_CHANGED` of the open project every program's check is marked as
of an earlier library, earlier types or both, nothing of it is drawn as current, and the programs
the workbench has read are checked again. A program's diagnostic in a library source opens that
source at its line, and the Library node says DOES NOT CHECK with the module's count when the
stored library does not check. The words of the LD and FBD editors for a box they do not know
were corrected (the editor does not know a library block; the compiler accepts it at Check). No
route, MCP tool or Rust behaviour changed. The checks in a real browser are NOT VERIFIED (ADR-013
evidence, `31-d4c.md`). Not in D4c: library blocks in the LD and FBD palette with their pins
(built since by a slice of its own, ADR-013 evidence `34-library-blocks-in-the-palette.md`), a
check in the module on save, a revision check (the last save wins, LATENT), and an answer
naming the programs a library change breaks.

**Not built.** The live values of a data block (D4b), D5, T2 and T3 are not built. A runtime with data blocks is configured over the API and the MCP,
and since C8d-1 also in the workbench, which edits and loads runtimes and tasks and shows no
live value of a data block (D4b). A retained block that moves into a configured
runtime loses its kept values, since the core keeps them per runtime id, and no warning of the
build says so (only RETAIN variables are
warned about). `VAR_GLOBAL` stays refused, and so do `VAR_EXTERNAL` in a function block or function, a
data block whose type is not a structure, and a per-block initial value.
