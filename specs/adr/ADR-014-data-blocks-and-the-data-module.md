# ADR-014: Data Blocks, User Types and the Data Module

- **Status**: ACCEPTED (design; nothing of it is built)
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
runtimes and between devices in the medium term. The author added, and the owner has not yet
said yes or no to: the hand-over at each program's start and end rather than each task's
(§2.3); the library of function blocks and functions (§2.7); data blocks typed by a function
block (§2.6); and tier 3 waiting for the security foundation (§2.4, §4). Each can be changed
without undoing the rest.

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
  does not write, from the master copy. When the program **ends**, the runtime copies the
  members it writes from its copy into the master copy. Both copies are made by the scheduler
  thread between pieces of execution, so no program is running while a copy is made.

What follows:

- A reader sees, for all members written by one program, the values of **one** completed run of
  that program. A preempted program has published nothing of the run it is in the middle of. A
  half-written structure cannot be observed.
- Within one task the programs run in order, and a later program sees what an earlier one
  wrote in the same task run. Across tasks a program sees what the writer published at the end
  of its last completed run.
- The delay from a write to its use is the wait for the reader's next start, not an exchange
  period. Nothing crosses a process boundary and the core is not involved.
- A program's inputs from the plant stay fixed per **task** run (the process image, ADR-013
  §2.19); data blocks are handed over per **program** run. The difference is deliberate: an
  input changes outside the controller and must be the same for every program of the task; a
  data block member changes only when its one writer ends a run.

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
writing runtime's process publishes into it when the writing program ends a run; a reading
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
  runtime's programs, agreeing layout hashes), keeps retained values, and carries the live
  values of data block members to the workbench the way it carries variables. A scalar member
  can be bound to a tag like a published variable; a structure, array or STRING member cannot,
  because a tag cannot hold it.
- The **workbench** gets an editor for types and data blocks in the data module's own UI
  (ADR-011), and shows live values of a data block per runtime.

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
| D1 | Compiler: several sources in one compilation; `VAR_EXTERNAL` against given data block declarations; the copy's address, the layout hash and the written members in the compiled program | — |
| D2 | Protocol, runtime module and core: data blocks in a runtime's configuration, master copies, hand-over at program start and end, the one-writer and layout checks, retained blocks, live values | D1 (C8b is built) |
| D3 | Data module: store, check, routes and MCP tools; the logic module builds with `<project>/data/` and its library | D1 |
| C8c | Logic module: runtimes and task configuration (ADR-013 §2.19) | — |
| D4 | Workbench: editor for types and data blocks, live values | D3 |
| C8d | Workbench: tasks (ADR-013 §2.19) | C8c, D2 |
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

Nothing of this ADR is built. `VAR_GLOBAL` and `VAR_EXTERNAL` are refused by both front ends
today, a source has exactly one `PROGRAM` and the logic module compiles each program from its
own source alone.
