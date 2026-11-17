# ADR-013: Control Logic: Our Own IEC 61131-3 Compiler, a Portable VM, and the Logic Module

- **Status**: ACCEPTED, 2026-11-17. Adrian reviewed it in two rounds (agreed the `runtime` module
  kind and where runtime management lives; supplied the standard and decided "mengikuti IEC"),
  then left the decision to merge to its author ("mana yang paling bagus menurutmu"), and
  decided IL is not built. Step 0 is done; step 1 is in progress.
- **Date**: 2026-11-17
- **Requested by**: Adrian ("bagaimana planning implementasi logika kontrol ini? harus ada runtime,
  editor, dan manager kan?"; "menurutmu mending pakai struc++ atau kita bangun compiler sendiri";
  "kedepannya aku juga mau program dari compiler ST kita bisa jalan di bare metal /
  mikrokontroller dan PLC murah lain ... jadi harus kita siapkan sejak dini juga")
- **Decided with Adrian, 2026-11-17**: outputs go to a safe value when logic stops; the operator
  may not write an output logic owns; all five IEC languages if possible (later: four, IL is not built); the manager manages the
  runtimes on a PC, possibly several at once; first boards ESP32 and RP2040; closed PLCs
  (Mitsubishi, Schneider M221) not now.
- **Agreed with Adrian after review, 2026-11-17**: a third module kind, `runtime`, started and
  watched by the core; managing runtimes belongs to the core, not to the logic module; the crates
  are split so the IR is the only thing languages and targets share.
- **Written by**: Claude, implementing the request

**About the standard.** The reference is IEC 61131-3 **ed. 3 (2013)**, read from a copy Adrian
holds (not in this repository; it is copyright IEC). Every table, figure and clause cited here
was read in that text on 2026-11-17. Ed. 3 was withdrawn in 2025; the current edition is ed. 4
(IEC 61131-3:2025), which this project has not read. What ed. 4 changed is in its Annex B and is
unknown here; nothing below claims to follow ed. 4.

---

## 1. Context

MythOS has no control logic. `POST /api/v1/control/deploy` answers 501. The plan has three parts
a user sees: a **runtime** that executes programs, an **editor** to write them, and a **manager**
that compiles, deploys, starts, stops and watches them. Underneath sits the question of how a
program becomes something a machine runs.

STruC++ (Autonomy Logic, used by OpenPLC v4) compiles Structured Text to C++14. Using it would
mean, on every machine that compiles: Node 22 and a C++ toolchain; no published arm64 build of
the compiler; C++ generated code whose memory errors can corrupt the runtime; no way, short of a
spike, to enumerate and set variables by name (needed for monitoring, forcing and online change);
and a GPL-3.0 compiler to ship beside MythOS. Compiling ST to Rust source instead would need
`rustc` on every machine, which is worse.

## 2. Decision

### 2.1 Libraries: one seam, a crate per language, a crate per target kind

```text
 front-ends (a crate per language)            backends (a crate per target kind)
 mythos-logic-st ─┐                        ┌─► mythos-logic-backend-vm ─► bytecode ─► mythos-logic-vm
 (LD, FBD, SFC    ┼──► mythos-logic-ir ────┤                                          (PC runner,
  later)         ─┘     (the seam)         └─► (C99 later: MCUs too small for the VM)   ESP32, RP2040)

 mythos-logic-compiler: joins a front-end, the target's profile and a backend. Owns neither.
```

| Crate | May depend on | Why |
| :--- | :--- | :--- |
| `mythos-logic-ir` | nothing of the workspace | The seam. A *unit*: structures, array types and POUs (functions, function block types, one program), every name resolved to an index; no addresses, word sizes or instructions. `serde` data with a version (`IR_VERSION`), so a front-end and a backend need not share a process |
| `mythos-logic-vm` | nothing at all | Builds for a microcontroller with `core` alone |
| `mythos-logic-st` (and each later language) | the IR | A language is added without touching a target |
| `mythos-logic-backend-vm` (and each later backend) | the IR, its own target | A target kind is added without touching a language |
| `mythos-logic-compiler` | all of the above | The one call from source to a program |

`crates/mythos-logic-compiler/tests/layers.rs` reads the real dependency graph and fails on a
break, and on a new `mythos-logic-*` crate that has not declared its layer; a second test feeds
the guard each kind of break.

- **Nothing unsupported is compiled into something else.** A construct a front-end does not take
  yet is a diagnostic naming it, with its line and column.
- **LD and FBD are lowered from their graphs**, not printed as ST text.
- **Standard function blocks.** The IR holds their catalog (names, members, types, directions);
  the VM holds how an instance is stored and run; a test in the backend holds the two equal.
  This is a stopgap: the standard blocks are to become a library written in the IEC
  languages, so that adding a block no longer means new firmware. User function blocks exist
  since format 1; the standard blocks are still native in the VM.
- **IL is not built** (Adrian, 2026-11-17: "iya, IL tidak dibuat"). Ed. 3 §7.2.1: "This
  language is outdated as an assembler like language. Therefore it is deprecated and will not
  be contained in the next edition of this standard." The languages are ST, LD, FBD and SFC.

### 2.2 Target profiles are data

A target declares what it can run: the bytecode format its VM reads, types it refuses, the
variable memory and code a program may use, and how deep its evaluation stack and its calls may
go (`max_stack_depth`, `max_call_depth`: what the target's firmware has). The compiler checks
the IR and its output against the profile, so a program that does not fit is refused at compile
time with the reason, never on the controller: `the program needs a stack 7 values deep; the
target board allows 6`. A profile that leaves a depth out states no limit, which is not a limit
of zero; the `pc` profile states none, because a PC runner gives each program what it declares. A profile is `serde` data: **a runtime module carries its own in its manifest**
(§2.4), so a new board brings its profile without a compiler change. Until runtime modules exist,
three stand in (`pc`, `rp2040`, `esp32`); their microcontroller limits are budgets chosen for a
program, not the chips' sizes. The stack and call depths of `rp2040` and `esp32` (64 and 16)
are those of the firmware this repository builds for their instruction sets (§2.9), which a
test holds equal.

### 2.3 The logic VM

`crates/mythos-logic-vm`. The same code runs in the PC runner and in microcontroller firmware:

- **`no_std`, no allocation, no `unsafe`.** The host gives it the program's bytes, a memory and
  a stack of the sizes the program declares. Clock, I/O and communication belong to the host.
- **A portable format.** Little-endian on every target, versioned (a program of a version this
  VM does not read is refused, not guessed at; see *Format versions* below), CRC-32 checked.
  Variables are packed bytes with no alignment.
- **Checked twice.** At load, every instruction is decoded and every address, jump and function
  block instance checked. At run time, every memory and stack access is checked again; a program
  that would go outside them stops with a fault.
- **As deep as it declares, on every host.** A program declares the depth of its evaluation
  stack and of its calls. A scan uses that many stack slots and call frames and no more,
  whatever the host gave: a push beyond them is `StackOverflow`, a call beyond them
  `CallDepthExceeded`. So a program that would go deeper than it says is found on a PC, not
  on the controller. A host with arrays of a fixed size (firmware) loads a program within its
  limits (`Program::load_within`): one that declares more memory, a deeper stack or deeper
  calls than the host has is refused when it is loaded, naming what it declares.
- **A bounded scan.** An instruction budget stops an endless loop with `BudgetExceeded`.
  Division by zero is a fault, not a value.
- **Format versions: a VM reads every format it extends.** A new format may only add
  instructions to the one before it: the header, every operand layout and the meaning of
  every instruction stay as they were, so a VM reads a program of any version it extends
  exactly as it was written, and the loader is told nothing new to read it. The loader takes
  the version from the header before it believes anything else, refuses one newer than its
  own (`UnsupportedVersion`), and checks each instruction against the version the program
  declares: an instruction a later format brought in, in a program of an earlier one, is
  refused where it is (`NotInFormat`), since whatever wrote it did not write that format. A
  format that changes an instruction's meaning, an operand's layout or the header is a new
  major: it starts a new line, and a VM of that line refuses every version before it.
  - *What it means for an update.* A controller keeps the bytecode it was given (a project's
    `runtimes.json`) across an update of its VM. A VM that extends that bytecode's format
    loads it and runs it scan for scan as before: the program does not stop and is not built
    again. Only a new major stops the programs of the line before until they are built again;
    such a format says so where it is introduced. The compiler makes the newest format only.
  - *Today.* This VM reads formats 1 and 2 (`OLDEST_FORMAT_VERSION`, `FORMAT_VERSION`). Format
    2 added the thirteen instructions on strings (`0xD0`–`0xDC`, opcodes no format-1
    instruction had) and changed nothing of format 1: the six programs of the corpus compiled
    by the format-1 and the format-2 compiler differ only in the version field and the
    checksum. Format 0, the spike's, had a 16-byte header with no call depth and no table of
    bodies; format 1 did not extend it, so it is refused. `older_formats_run_alike.rs` runs
    the corpus as the format-1 compiler made it (committed bytes, never regenerated) against
    the same sources compiled now, 200 scans each with the same inputs, memory equal after
    every scan.
- **Format 2** (format 1 with strings; L2c).
  - *Header*: magic, version, stack depth, memory size, code length, call depth, number of
    bodies, then a table of bodies (code offset, instance size), the initial memory image, the
    code, CRC-32.
  - *Bodies.* The code is one body after another: each function, each function block type, the
    program last (a scan starts there). A body may call only bodies before it in the table, so
    nothing a loaded program does is recursive.
  - *No data frames.* A unit has no recursion (refused at compile time, naming the cycle), so
    the program's variables, every instance inside them, and every function's parameters and
    locals have one fixed place, decided by the backend. A function's locals and result are put
    back to their initial values at each call. A call frame is a return address and a saved
    base; the host supplies as many as the program declares.
  - *Addressing.* Absolute for the program's variables and a function's; relative to the *base*
    for the members of the instance a function block's body was called for; and through a
    *place* for an array element: a region, and an offset popped from the stack, which may not
    take the access outside the region.
  - *Subscripts.* Each subscript is checked against its own declared range at run time; outside
    it the scan stops with `IndexOutOfRange`. A literal subscript out of range is refused at
    compile time; inside its range it is part of the address and costs nothing at run time.
  - *Checked twice, as before.* The loader checks every instruction's operands: addresses,
    regions, instance sizes, call targets (an earlier body only), jumps inside their own body,
    and that no body can run into the next. At run time every access is checked again, every
    sum is checked, and calls nested deeper than the declared depth stop the scan with
    `CallDepthExceeded`.
  - *A jump lands where an instruction starts.* The loader walks each body a second time and
    refuses a jump to a byte that is not the start of an instruction of that body
    (`BadJump`); landing inside one would run its operands as code. (A call goes to the start
    of a body, which is such a start by construction.) The loader has no memory to note the
    starts in, so it proves a target by walking to it from a start it already knows: the
    start of the body, the jump itself, or one of four targets it has proved (the END a CASE's
    arms all jump to, the last target ahead, the heads of the last two loops). For what the
    compiler emits this keeps the second walk about as long as the first. For code the
    compiler did not make, the walk for one jump can be as long as its body: a hostile
    program can make the loader work in proportion to its jumps times its length. The
    run-time checks hold as before. `damaged_programs_never_panic.rs` damages every compiled
    program of the corpus at random, thousands of times, and requires each to be refused or to
    fault, never to panic.
  - *Strings* (format 2). A `STRING[n]` is `2 + n` bytes: its length now (u16), then room
    for `n` characters of one byte each. A string never goes on the stack: an instruction on
    strings names each one by a *string operand*, its length `n` and a place of `2 + n`
    bytes, which the loader checks as it checks any place. Thirteen instructions: assignment
    (`SMOVE`, which cuts to the target's length), comparison, `LEN`, `FIND`, the six
    functions of Table 34 that give a string, CONCAT two at a time, and an integer to and
    from its text. A string a function gives is written to scratch the compiler sized for
    any result of its arguments; one longer than that (which can only be past the longest
    STRING, 32,767 characters) stops the scan with `StringTooLong`. A length or position
    below zero, or one naming a character the string does not have, stops it with
    `StringArgument` (§6.6.2.5.11 calls both an error). A stored length greater than the
    string's own is never followed: `BadString` (the program's instructions never write
    one; the host or damage did). Text that does not write an integer, converted to one:
    `BadNumber`. A literal is a string in the memory image, one for each distinct text, never
    written.

### 2.4 A third module kind: `runtime`

ADR-010 has `driver` (reads field devices) and ADR-011 `service` (serves routes). Neither fits a
thing that executes continuously against the tags. A **runtime** module is a process the core
starts, watches, restarts and swaps like any module, speaking the module protocol with these
calls:

- `start` with a program and how it is tied to the tags (`RuntimeProgram`: the code and symbols,
  which are the module's own and opaque to the core, and the bindings, which the core reads),
  `stop`, and `mode` (RUN, from a cold start, or STOP);
- `exchange`: the tags the program reads go in, as they are now, with whether each is GOOD;
  what it publishes and what it wants written to field tags come out, with its state (RUN,
  STOP or FAULT and why; scans; scan time last and max; overruns; program and version). The
  core writes both: the tag bus keeps one writer (ADR-002, ADR-010 §2.2);
- `write` of a published variable marked writable: an operator's setpoint;
- reading and forcing any variable by its path, for monitoring (C5);
- its manifest declares its **target profile** (§2.2).

**How the core hosts one.** A runtime instance is a connection of the field I/O runtime
(`crate::runtimes::RuntimeDriver`): the tags its program publishes are the connection's tags,
and the connection's poll cycle is one exchange. So what the read path promises holds for a
runtime's tags without being built twice: a slot and a catalog entry per tag, a deadline per
cycle, `BAD_COMM_FAILURE` when the process dies, a restart with the same program, and
replacement in place when a new build or program publishes the same tags. A project's
runtimes are in `runtimes.json`, beside `io.json`, and are applied with it. A program
therefore runs only where field I/O runs.

The protocol is not specific to logic or to our VM: it is "compute continuously from tags;
publish tags; request writes; report state". The logic VM runner on a PC
(`mythos-module-runtime-vm`) is the first. A board family (ESP32, RP2040) is another: a module
that carries the same calls to firmware over serial, USB or TCP. A different engine can be a
third without a core change. Server-side functions that also execute continuously (an alarm
engine, a historian's logger) are candidates for the same kind when they are built.

Rules the core enforces for every runtime:

- **Process image.** A scan sees one set of inputs: those of the last exchange. The module
  scans on its own clock; exchanges happen beside it (every 20 to 200 ms), so the scan time
  does not depend on them. Only outputs that changed are written to the field. A runtime
  reading the tag bus directly, as a reader, is not built.
- **No scan before the first inputs, no output before the first scan.** Nothing computed from
  initial values alone reaches the plant. A tag that is not GOOD feeds its variable no value:
  the variable keeps what it last took (its initial value, if it never took one), and a
  binding `of: good` gives the program a BOOL to act on.
- **Safe values.** Every output bound to a field tag has a safe value, set in the project and
  required (there is no default). When the program is stopped, faults, its process dies or
  does not answer an exchange within a second, or the instance is removed, the core writes
  every output its safe value, and writes it again each cycle until the device confirms. A
  program that is not running publishes no values: its tags are bad and say why.
- **Write authority.** An output bound to a runtime is owned by it for as long as it is
  loaded, running or not. The operator and the HMI cannot write it (`409`, naming the owner);
  two runtimes cannot bind the same output. A setpoint is a published variable marked
  writable, written through the same route as any tag.
- **Swap.** A new build starts beside the old one; variables whose path and type match are copied
  at a scan boundary; then I/O switches and the old one stops. A build that cannot start, **or
  whose profile cannot run the program now running**, is refused and the old one keeps running.
- **Timing is claimed only as measured.** On a general-purpose OS the target is task intervals of
  10 ms and above. A real-time mode (Linux, PREEMPT_RT, `SCHED_FIFO`) comes later, measured.
  ADR-003's 1 ms / 50 µs figures are withdrawn: nothing measured them.

### 2.5 Managing runtimes is the core's job

The module host already starts, watches, restarts and swaps module processes. Runtimes are
managed there, not in the logic module, so one page and one set of MCP tools covers every
runtime whatever it executes:

- `GET /api/v1/runtimes` and Settings → Runtimes: every runtime on this node, possibly several
  (one per device of a plant simulated on this PC), with its state, scan time and program
  version; start, stop, restart. Fleet (ADR-012 §2.6) shows the same for paired nodes.
  A scan figure is what the runtime's module last said, or absent (`null`): while its module
  does not answer nothing is known, and before the first scan no scan time was measured. It
  is never `0` in their place, which would read as a measurement.
- The write path to field devices, the safe values and the write authority of §2.4.
- **Watching.** `POST /api/v1/runtimes/<id>/watch` (any signed-in role; MCP `watch_runtime`)
  gives the values of a program's variables as its last scan left them: any variable with one
  value (BOOL, the integers, REAL, LREAL, TIME, STRING), bound to a tag or not, and one
  element of an array of those. A STRING is sent as its characters, each byte the character
  of that code (Table 10 footnote l: ISO/IEC 10646, whose first 256 codes a single-byte
  string writes); one whose stored length is past its own has an error in place of a value. The protocol call is `Call::Watch`; the VM module reads them under the lock
  its scan thread holds for a whole scan, so every value of an answer is of one scan. The core
  asks at the runtime's exchange while somebody watches (a watch nobody renews for five
  seconds stops), and an answer carries the program's state and its age. Reading changes
  nothing in the program. A variable the program does not have, a whole array, a structure or
  an instance has an error in place of a value; a REAL that is not a finite number is said,
  not sent as null. A runtime module without `watch` answers that it gives no values.
- Data types shared by more than one module (logic and the HMI both have them today, apart) live
  in the core, since a module may not depend on another (ADR-011 §2.6).

### 2.6 The logic module is authoring

`plugins/logic` (`mythos-module-logic`), a `service` module (ADR-011 §3 step 5): programs in
`<project>/logic/`; check; compile for a target; the variable table and each variable's binding
to a tag. It never talks to a runtime itself. Every route is an MCP tool. A device's programs
are part of its slice (ADR-012 §2.5).

As built (C3):

- A program is its source (`programs/<name>.st`) and an entry in `programs.json`: its interval,
  the runtime module and the target profile it is built for, whether it runs when loaded, and
  its bindings (inputs, outputs with their safe values, published variables). One program is
  one cyclic task; a task that runs several programs is not built.
- `check` compiles the saved source, or a text sent with the request and not saved, and answers
  with the diagnostics (each with the range it is about, §2.8), or with the variables and every
  binding that names a variable the program does not have or one that cannot be bound (an
  array, a TIME, a STRING). A STRING cannot be bound because a tag holds no text: a tag's
  value is a BOOL or a number (`mythos_plugin_sdk::SampleValue`, and the tag bus's slot);
  it is watched instead.
- `build` compiles the saved source and answers with the runtime instance of §2.4, its version
  a hash of the code, the interval and the bindings. **A module cannot call the core**
  (ADR-011 §2.6), so the module does not load what it builds: the workbench, or an agent, saves
  the instance into the project's runtimes, and the core loads it. An edit therefore changes
  nothing that runs until it is built and loaded. A bound tag that no connection provides now
  is a warning of the build, not a refusal.
- Not built: unit tests in ST, and programs in a device's slice (C6).

Its workbench UI (C4), `frontend/packages/logic-ui`, served beside the module (ADR-011 §2.3):

- Programs in the project tree, and an editor per program: the Structured Text (a text area
  over the same text painted in colour; no editor library), the last check's diagnostics,
  each underlined over the whole range it is about (§2.8) and listed with where it starts, the task settings, and the variables of the last text that
  compiled, each with its bindings to tags and each bound tag's live value.
- The editor keeps three things apart: the text in the editor, the text that is saved, and the
  program that runs. **Load** saves, checks, builds, reads the project's runtimes, replaces the
  instance with the program's name and saves them: that is the call the logic module cannot
  make. A program that does not compile, or whose bindings do not match its variables, is not
  built. A program the controller saved and then lists under `changes.failed` is said not to
  have started, with the controller's reason, and not to be loaded. **Run**, **Stop** and **Unload** go to the core.
- It reads `GET /api/v1/runtimes` once a second while an editor is open. Before the first
  answer the runtime is UNKNOWN; after a read fails, what was read before is marked LAST KNOWN
  and Run is disabled. Stop is never taken away, by a stale state or by a request still
  unanswered: it is the safe direction. Run asks first, as on Settings → Runtimes. Scan times are NOT MEASURED until a scan has run. A new output
  binding has no safe value, and none is written for it, until one is given.
- While a runtime runs the program, the variables table shows each variable's value in it,
  read twice a second by name with `watch` (§2.5); a STRING as Structured Text writes it,
  quoted, with `$` combinations for `$`, `'` and what is not printable. A STRING has no "Bind…"
  and says why. A variable the running version does not
  have says so; a value read from a program that is not in RUN is marked as what it stopped
  with; when the values cannot be read the cell says UNKNOWN and shows none read before.
- Not built: forcing a variable (C5); watching an element of an array from the table; an
  editor for LD, FBD, SFC.

### 2.7 The HMI is not a runtime

Checked on 2026-11-17: `plugins/hmi` has no task, timer or loop; all of its 14 routes answer a
request and return. What an operator sees runs in the browser, from the core's telemetry stream.
So the HMI has nothing that would have a RUN/STOP state, a scan, outputs or safe values, and
making it a `runtime` would add a process that does nothing.

- The HMI stays a `service` module with a UI. Which release of the screens a node shows is
  already part of its slice and of compare.
- What operators associate with an HMI and that **does** run without anyone looking — alarms,
  trend logging, scripts, recipes — must not go into the HMI module. Each is a module of its own
  (a `runtime` where it computes from tags), so the HMI remains a view that can be unloaded
  without stopping any of them.

### 2.8 The editor

The logic module's workbench UI (ADR-011 §2.3): a POU tree, the ST editor with diagnostics, the
variable table, task settings, check and test, and live values beside the code. Each later
language brings its editor as an editor kind.

**What the front-end gives an editor** (L2a; the editor of §2.6 shows it). The spike's parser
stopped at the first error and recorded only where a construct starts. The ST front-end now:

- **Tells every independent mistake of a source in one pass.** A rule that cannot go on says why
  and gives up; whoever holds the list reads on from where the next item can start: a statement
  is left at its `;` or before the keyword that ends its list; a declaration at its `;` or
  before what ends its section; a POU with a head that cannot be read at its `END_`. A statement
  with an `END_` of its own whose head cannot be read is still read from THEN, OF or DO, so the
  statements under it are checked for syntax; without that keyword it is left at its own `END_`,
  the statements nested in it included.
- **Tells a mistake once.** The parser never complains twice about one token, and says nothing
  about text the lexer could not read. The checker says nothing about what follows from a gap:
  the statements of a POU that was not read whole are not checked (its declarations are, and
  every other POU is checked in full); a variable whose declaration was refused is not
  "not declared" where it is used; a member not found in a type with an unread or refused
  declaration is not called missing; a source whose PROGRAM has no readable name is not said
  to have none. What stays untold until the first mistake is mended: mistakes of meaning in a
  POU that has a mistake of syntax.
- **Says where a diagnostic ends.** `Diagnostic` is `line`, `col`, `end_line`, `end_col`,
  `message`: all 1-based, the end exclusive, a column a character (not a byte). The range is
  the token or construct the message is about: a name, an operator for an operation, a call to
  its `)`, a place with its members and subscripts. It is empty at the end of the source, and
  all four are 0 for a message about the whole program. `line` and `col` are what they were.
  There are no warnings, so there is no severity.
- **Refuses a keyword of the standard as a name** (§2.13).
- **Refuses what is nested too deeply, and cannot be made to crash by it.** Every pass over a
  program is recursive, and the compiler runs on a thread of the logic module's process, whose
  stack is 2 MiB. A body nested more than `MAX_NESTING` (100) deep is refused where the limit is
  reached: statements within statements, and operations, calls, subscripts, parentheses and
  array types within one another; a chain of operators counts once per operator (`a + b + c` is
  two deep), because the tree it makes is that deep. A type nested more than
  `MAX_TYPE_NESTING` (100) deep (structures, arrays and function block instances within one
  another) is refused at its declaration; one that holds a type refused for that is refused
  silently. A structure that contains itself is told and loses its members before anything
  follows them round. The order POUs and structures are taken in is found without recursion,
  so a chain of thousands of functions compiles. *Why 100*: C11 §5.2.4.1 asks a compiler for at
  least 63 levels of parentheses and 127 of blocks; a program written by hand nests a few levels.
  *Why it fits*: in a debug build (where frames are largest; `cargo test` is one) the deepest
  source of each kind allowed needs at most 544 KiB of stack on Linux (nested CASE), measured by
  running it on threads of falling stack size; `deep_sources_are_refused.rs` compiles each on a
  1 MiB thread, on every system CI runs. A chain of 100 operators is the realistic edge: a
  longer one is split with a variable.

### 2.9 Microcontrollers, and simulators before boards

Firmware for a board family is the VM plus a host for that board. Before a real board:

- **Run in an emulator, in CI.** `scripts/logic-vm-on-emulators.sh` builds firmware around each
  program's bytecode and runs it in QEMU on a Cortex-M0 (machine `microbit`; thumbv6m, the
  RP2040's instruction set, with no hardware divide or floating point) and on an RV32IMC machine
  (`virt`; the ESP32-C3's instruction set). After every scan the firmware prints a CRC-32 of the
  VM's whole memory, and the script requires each to equal what the same program gives on the
  machine running the script. The format lays memory out identically everywhere, so this
  compares every variable and every function block's inner state, byte for byte. These machines
  stand for instruction sets, not for the boards: no RP2040 or ESP32 peripheral is emulated
  (board emulators exist for that: rp2040js, picoem, Espressif's QEMU).
- **Real timing needs a real board.** The emulator run counts no processor instructions (the
  number it prints is VM instructions), and an emulator does not reproduce
  flash wait states, cache or the cost of software floating point exactly. Scan time on a
  microcontroller stays "not measured" until one is attached.

### 2.10 Closed PLCs

Not now. They run only their vendor's programs. Exporting through a backend (PLCopen XML, a
vendor's dialect) stays possible because the IR is neutral; it needs research per vendor first.
Today a cheap PLC can serve as remote I/O over Modbus TCP.

### 2.11 What MythOS logic is not

**It is not a safety system.** Safe values are written by the core; if the core itself stops, no
software of ours writes anything, and a field device holds its last value. A plant's safe state
must be configured in the field devices (a communication watchdog) and in hard-wired interlocks.
No SIL claim is made.

### 2.12 STruC++ as an oracle

STruC++ is not used to build or run anything. A differential test (by hand; it needs Node, g++
and STruC++) runs each program in `crates/mythos-logic-compiler/tests/differential/` on STruC++
and on our VM for 60 cycles and compares every variable after every cycle. STruC++ is GPL-3.0;
it is run as a separate program and none of its code is copied. Its limits: seven programs so
far, reals compared at the precision its REPL prints, strings character by character as it
prints them (so the corpus writes none that would need a `$` combination), only scalar
variables of the program compared, and STruC++ can itself be wrong. It takes a STRING's
length only as `STRING(n)`, not as `STRING[n]` (Annex A), a function's result only as a
plain `STRING`, and neither CONCAT of three strings of different lengths nor `<` against a
literal: the corpus is written around those.

### 2.13 Semantics settled against the standard

Adrian, 2026-11-17: where an implementation and the standard differ, follow the standard
("mengikuti IEC"). Each rule below was read in ed. 3 and is pinned by a named test.

| Rule | IEC 61131-3 ed. 3 | We do | STruC++ 0.7.0 |
| :--- | :--- | :--- | :--- |
| CTU counts past PV | Table 45: `ELSIF CU AND (CV < PVmax)`; PVmax is the implementer's | to the type's maximum | stops at PV (as OpenPLC) |
| CTD counts below 0 | Table 45: `ELSIF CD AND (CV > PVmin)` | to the type's minimum | stops at 0 |
| REAL/LREAL to integer | Table 22 footnote b: IEC 60559, a half to the nearest even integer; `REAL_TO_INT(1.5)` is 2 | 2.5 → 2, −2.5 → −2 | a half away from zero (2.5 → 3) |
| F_TRIG | Table 44: `Q := NOT CLK AND NOT M; M := NOT CLK;`, M starting FALSE; its note: Q is TRUE on the first call after a cold start when CLK is FALSE | as the table | M starts TRUE: no Q on the first call |
| MOD by zero | Table 29 footnote e: the result is 0 | 0 | not compared |
| Integer division | Table 29 footnote d: truncation toward zero; by zero is an error (§7.3.2) | truncates; by zero the scan stops with a fault | not compared |
| `=` `<>` and `<` `>` `<=` `>=` | Table 71: one precedence (4), left to right | `a < b = c` is `(a < b) = c` | not compared |
| Implicit conversions | Figure 11, entries "i": SINT→INT→DINT→LINT; SINT, INT→REAL; SINT, INT, DINT→LREAL; REAL→LREAL | exactly those | agrees |
| Timers | Figure 15 (timing diagrams) | TON, TOF, TP as drawn, ET included | agrees (it found our TP bug) |
| Function: input left out of a formal call | §6.6.1.4.2, Table 20 feature 1b: it has its initial value | its declared initial value, at every call | agrees |
| Function: internal variables and result | §6.6.2.1, Table 19 feature 2e: not stored | start afresh at every call | agrees |
| Function block: input not given | §6.6.3.4.1 rule 3c: keeps its initial value or that of the latest call | keeps it; `inst.in := x;` apart from the call takes effect at the next call | agrees |
| Subscript out of range | §6.4.4.5.1: an error | a diagnostic for a literal; an `IndexOutOfRange` fault at run time | not compared |
| Array initial values | §6.4.4.5.2: rightmost subscript varies fastest; `n(…)` repeats; missing entries take the default | as the section; more values than elements is refused (the standard ignores them with a warning; there is no warning channel) | not compared |
| Assignment of structures and arrays | §6.4.4.6.1, §7.3.3.2.1: only between the same data type | refused otherwise | agrees on what compiles here |
| Recursion | §6.6.1.1: implementer specific | refused at compile time, naming the cycle | not compared |
| A result must be assigned | §7.3.3.2.3 | a function that never assigns its name is refused | not compared |
| Writing an input inside its POU | Figure 7 ("not modifiable within the entity"), Figure 13 rule 2a | refused in a FUNCTION, a FUNCTION_BLOCK and a PROGRAM | not compared |
| Reading an instance's input from outside | Figure 13 rule 1b: not allowed | refused | not compared |
| Writing an output, or anything inside an instance, from outside | Figure 13 rule 4b; §6.6.3.2 rule 11 (VAR is private) | refused; an internal variable cannot be read from outside either, except to look into an instance held there (`a.b.q`) | not compared |
| A keyword as a name | §6.1.3: keywords "shall not be used for any other purpose, for example, variable names" | refused where it is declared (a variable, a member, a type, a POU), naming the word: 127 keywords | not compared |
| A STRING declared without a length | Table 10 footnote k, §6.4.2.2: the maximum length is the implementer's; ed. 3 gives no default | 254 characters; a declared length is 1 to 32,767 (so `LEN`, an INT, can always say it) | 254 (its source says "per IEC standard"; ed. 3 says no such thing) |
| `STRING[n]` and `STRING(n)` | Annex A `S_Byte_Str_Spec` writes `[n]`; §6.4.2.2 calls the length "parenthesized" | both | `(n)` only |
| A string literal | §6.3.3, Table 6, Table 7, Annex A `S_Byte_Char_Value`: printable characters but `$`, `'`; `$` and two hexadecimal digits; `$$ $' $L $N $P $R $T`; `$"` only in a double-byte string (Table 7 note 3) | as the clauses; any other character, and `$"`, refused with what to write instead; `$N` (the newline, the implementer's) is a line feed, `$0A`; `STRING#'…'` | `$N` a line feed; also takes `''` for a quote (not in Annex A) |
| Assigning a longer STRING | §6.6.1.2.2: "the result is Implementer specific" | cut to the target's length, the first characters kept; the same for an argument to a shorter input. An initial value longer than its variable is refused (there is no warning channel) | cut |
| Comparing strings | §6.6.2.5.11: the shorter extended on the right with characters of value zero; by character codes | as the clause: `'Z' > 'AZ'`, `'AZ' > 'ABC'`, `'A' = 'A$00'` | agrees on what was compared |
| An error of a string function | §6.6.2.5.11: an ANY_INT input below zero, a non-existent character position, a result longer than the maximum, are each an error | the scan stops with `StringArgument` or `StringTooLong`; a literal below zero is refused at compile time. A position is from 1; `MID`, `DELETE`, `REPLACE` take `P` + `L` − 1 ≤ `LEN`, so with `L` 0 `P` may be one past the end | clamps, or returns '' (not compared) |
| INSERT's position | Table 34 feature 6: "Insert IN2 into IN1 after the P-th character position"; `INSERT('ABC', 'XY', 2)` is `'ABXYC'` | after the P-th; P from 0 (first) to `LEN(IN1)` (last) | before the P-th: `'AXYBC'` (deliberate difference) |
| FIND of an empty string | Table 34 feature 9: the position where IN2 first begins, 0 if none | 1: an empty string stands at position 1 | not compared |
| CONCAT's result | Table 34 feature 5, §6.6.2.5.11 | as long as all its inputs together, up to 32,767; longer stops the scan | as long as its longest input |
| An integer to and from STRING | Table 22 footnote e: the "external representation" of the value | `INT_TO_STRING(-5)` is `'-5'`; `STRING_TO_DINT` reads an integer literal: a sign and decimal digits, or `2#`, `8#`, `16#` digits, single underscores between digits. Nothing else (no space, no type prefix): `BadNumber`. A value its type cannot hold wraps, as narrowing does | stops at the first character that is not a digit: `'16#7F'` is 16 (deliberate difference) |

Three places where the standard leaves a choice or we go beyond it:

- **Overflow.** §7.3.2 calls a result outside its type's range an error. We wrap at the type's
  width, as STruC++ does, and do not stop the scan. Whether to fault instead is open.
- **Narrowing** a value that does not fit (Table 22's example: implementer specific): we wrap.
- **`INT_TO_BOOL` and `BOOL_TO_REAL`** families are not defined by Figure 11 ("the implementation
  may support additional conversions"). We have them: non-zero is TRUE; FALSE and TRUE are 0.0
  and 1.0.

**Which words are keywords.** Ed. 3 has no table of them: §6.1.3 gives the rule and nothing
else. The 127 are the terminal symbols of Annex A (normative) that are words standing as a
syntactic element of their own, in the productions of the common elements, Structured Text, the
textual form of SFC, configurations and namespaces, with the keyword column of Table 10 for
the data types (`mythos_logic_ir::KEYWORDS`; the reasoning is at the top of
`crates/mythos-logic-ir/src/keywords.rs`). Words Annex A writes in quotes that are *not*
keywords here, because the standard itself uses them as names:

- **The operators of IL** (Table 68: LD, ST, S, R, JMP, CAL, RET, …). §7.2.4.3 calls CAL a
  keyword "in the operator field", and that is where they are one. Outside IL the standard
  names variables with them: `LD` and `R` are inputs of its counters, read in their ST bodies
  (Table 45); `S` and `R` are inputs of its bistables (Table 43); Table 39 declares
  `VAR R: REAL; END_VAR`. So IL not being built (§2.1) leaves no word reserved for it; AND, OR,
  XOR, NOT and MOD are keywords as operators of ST.
- **The action qualifiers of SFC** (Table 59: N, R, S, L, D, P, SD, DS, SL), the inputs of a
  TASK (Table 63: SINGLE, INTERVAL, PRIORITY), and EN and ENO (§6.6.1.5 declares them as
  variables).
- **The names of standard functions and function blocks.** Annex A marks both lists "incomplete
  list": they are names of a library. `ton : TON` is accepted.

A program that names a variable `step`, `on`, `to` or `from` is therefore refused (the
differential corpus and two tests did, and were renamed). This list is a reading of ed. 3, not
a table copied from it; ed. 4, which drops IL, has not been read.

Closed in L2b: the evaluation stack stops at the depth the program declares, and a target
profile carries a stack depth and a call depth (§2.2, §2.3). Found while reading §6.1.2 and
not built: an identifier
with two underscores together, or one at its end, is accepted (the standard forbids both).

The differential test lists the first three rows, INSERT's position and the integer read
from a based literal as deliberate differences from STruC++ (287 of 7,500 values); a rounding
difference must be exactly one apart to count as one.

Not built in L2c (each refused by name): WSTRING, CHAR and WCHAR, and so a string's character
by subscript (`s[2]`, §6.4.2.2); an initial value on a TYPE; conversions between STRING and
BOOL, REAL, LREAL or TIME; STRING in SEL, MAX, MIN, LIMIT, MUX.

## 3. Order of work

Two tracks. The core path comes first because Adrian asked for logic that runs end to end
(2026-11-17), and it does not wait for the language: a runtime takes bytecode as a black box.

**Done:** the spike and the crate split (§2.1); programs run on emulated microcontrollers in CI
(§2.9).

**Track C, the core path, in order:**

- C1. **Writes to field devices** (done): `PUT /api/v1/tags/<uri>/value`, a driver's `write`,
  the tag's declared access, an MCP tool. Also what HMI setpoints need.
- C2. **The `runtime` module kind** (done): the protocol, the PC runner
  (`mythos-module-runtime-vm`), its host in the core, safe values and write authority (§2.4),
  `GET /api/v1/runtimes`, a project's `runtimes.json`, run and stop, MCP tools. Proven against
  real Modbus simulators through the real module process on Linux, macOS and Windows. The
  Runtimes page is C4a.
- C3. **The logic module** (done): programs, their bindings, check and build (§2.6), each an
  MCP tool. Proven through the real logic, runtime and Modbus module processes: a program is
  written, checked, built, loaded and runs a Modbus simulator.
- C4. **The ST editor** (done, §2.6) in the workbench.
  - C4a. **The Runtimes page** (done): Settings → Runtimes shows every runtime of this node as
    `GET /api/v1/runtimes` reports it, with run and stop for an engineer (§2.5, §5).
- C5. **Online**: monitor (done: `watch`, §2.5), runtime tags in the HMI (done: the tags a
  project's programs publish are offered wherever a binding names a tag), force.
- C6. **Swap and online change; programs in a device's slice.**
- C7. **Hardening**: watchdog, RETAIN variables across restarts.

**Track L, the language, beside it:**

- L1. **The VM's real model** (done): functions, user function blocks, arrays and structures;
  static allocation, no recursion, instance-relative addressing, a bounded call stack;
  format 1.
- L2. In three parts. **L2a** (done): a parser that recovers from errors and gives ranges, for
  the editor (§2.8); a keyword is refused as a name (§2.13). **L2b** (done): the limits the VM
  and the profiles enforce: the stack stops at the declared depth, profiles carry a stack
  depth and a call depth, a jump into the middle of an instruction is refused at load. **L2c**, in two:
  **L2c-1** (done): STRING, its literals, assignment, comparison, the functions of Table 34
  and integer conversions; bytecode format 2 (§2.3, §2.13). **L2c-2**: more of the standard
  library (numeric, selection, the functions of operators).
- L3. **LD, FBD, SFC** (not IL, §2.1), each a front-end crate and an editor kind.

**Later:** ESP32 and RP2040 firmware as runtime modules on real boards; a C backend; a Linux
real-time mode.

Each step leaves `dev` working.

## 4. Consequences

- No toolchain is installed with MythOS or on a controller to compile logic: the compiler is Rust
  and part of the logic module.
- A controller runs bytecode, not native code: slower than STruC++'s native output (§5), and the
  price of memory safety, one format for every target, and variables enumerable by name.
- The language coverage is ours to build. STruC++ covers far more today (OOP, pointers, WSTRING,
  80+ functions). Until §3 step 8 is far along, a program using those is refused by name.
- Integer and real semantics are pinned by our tests against the standard's text (§2.13), and
  compared with the oracle. A program moved from OpenPLC that relies on a counter stopping at
  PV, or on 2.5 rounding to 3, behaves differently here.
- The core grows: a third module kind, the write path, safe values and runtime management are
  core work, not module work.

## 5. Status of implementation

| Part | Built |
| :--- | :--- |
| 2.1 Crates and layers | `mythos-logic-ir`, `-st`, `-vm`, `-backend-vm`, `-compiler`; `layers.rs` guards them |
| 2.1 ST front-end | Several POUs in any order: TYPE (STRUCT, named ARRAY), FUNCTION (VAR_INPUT, VAR; positional and formal calls), FUNCTION_BLOCK (VAR_INPUT, VAR_OUTPUT, VAR; nesting; arrays of instances), one PROGRAM; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME, STRING (`STRING[n]`, `STRING(n)`, 254 by default; literals with `$` combinations; assignment that cuts; comparison; LEN, LEFT, RIGHT, MID, CONCAT, INSERT, DELETE, REPLACE, FIND; integer to and from STRING; in functions, function blocks, arrays, structures and a TYPE name); arrays of any dimensions with initial value lists; whole-structure and whole-array assignment; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between the elementary types; implicit widening per Figure 11; recursion refused naming the cycle; a keyword of the standard refused as a name; diagnostics for everything else (enumerations, subranges, WSTRING, CHAR, VAR_IN_OUT, methods, and more: each refused by name); every independent mistake of a source in one pass, each once, with the range it is about (§2.8); a body or a type nested more than 100 deep refused, a structure that contains itself told before anything follows it (§2.8) |
| 2.2 Profiles | `serde` data with a format version; `pc`, `rp2040`, `esp32` stand in; refusals (type, memory, format, stack depth, call depth) proven with profiles read from JSON; a profile written before the depths existed still loads and states no limit |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; format 2 (L2c: format 1 with strings, §2.3), and format 1 read as it was written (a VM reads every format it extends; an instruction newer than a program's format refused at load): bodies, call frames, instance-relative and region-confined addressing; loader checks, with every jump landing on an instruction of its body, and `load_within` a host's limits; run-time checks, budget; the stack and the calls stop at the depths the program declares; faults, with `IndexOutOfRange`, `CallDepthExceeded`, `StackOverflow`, and for strings `StringTooLong`, `StringArgument`, `BadString`, `BadNumber` |
| 2.4 Writes to field devices (C1) | Built: `IoStatusHandle::write_tag`, `PUT /api/v1/tags/<uri>/value` (operator), MCP `write_tag`; a driver declares each tag writable or not and the core refuses a read-only one without asking it; a write runs in its connection's task between polls, bounded by the poll's deadline; the answer is `written: true`, `false` or `"unknown"`. Proven by `field_write_tests.rs` (a real Modbus simulator) and through the Modbus module process in `modbus_module.rs`. Ownership of an output by a runtime (write authority) comes with the `runtime` kind |
| 2.4 The `runtime` kind (C2) | Built: `ModuleKind::Runtime`, `Call::Exchange`, `Call::Mode`, `RuntimeProgram` (`mythos-module`); `plugins/runtime-vm` (`mythos-module-runtime-vm`: the VM, a scan thread, faults, cold start); `crate::runtimes` (`RuntimeDriver`, ownership, safe values, `BusTags`). Proven by `runtime_vm.rs` (ST compiled, run in the real module process, reading one Modbus simulator and commanding another: follows the plant; an owned output refuses an operator; a setpoint is taken; stop, fault, a killed process and removal each leave the safe values; a tag that is not GOOD feeds no value) and `runtime_api.rs`. A program that publishes no tag, or is bound to nothing, runs all the same (found by the Runtimes page's end-to-end run: it was saved, reported as applied, and never started). Reading variables is built (`Call::Watch`, C5): `runtime_vm.rs` reads locals, array elements, a timer's members and a REAL from the running module process, in RUN and in STOP, and a module without `watch` says so and runs on. Not built: forcing variables, state carried across a replacement, a watchdog on the scan thread, RETAIN |
| 2.5 Managing runtimes | Built in the API: `GET /api/v1/runtimes`, `POST /api/v1/runtimes/:runtime/run`, `/stop` and `/watch`, `GET`/`PUT /api/v1/projects/:id/runtimes`, each an MCP tool; engineer for all but reading. Built in the workbench (C4a): Settings → Runtimes (`RuntimesSettings.tsx`) reads the list about once a second and shows each runtime's module, program and version, its mode as reported with the reason given, its scan figures and its outputs (safe value, what the device last confirmed, whether all are confirmed safe); run and stop for an engineer; the runtime modules with the profile each declares. What the server did not send is `UNKNOWN` or `NOT MEASURED` (the scan figures of a runtime whose module does not answer among them: the server sends zeros there), and a read that fails or is not answered marks the rows as the last known state with their age. Proven by `runtimes.test.tsx` and, against a real server and runtime module, by `scripts/e2e.sh`. Not built: restart as a command of its own; loading a program from the page; the same for paired nodes in Fleet; data types shared in the core |
| 2.6 The logic module | Built: `plugins/logic` (programs, bindings, check, build; eight MCP tools), proven by `logic_module.rs` through the real logic, runtime and Modbus module processes; its workbench UI `frontend/packages/logic-ui` (tree section, program editor, load, run, stop, unload), proven by its vitest suite against the workbench's real host and by `scripts/e2e.sh`, where a program typed into the editor is checked, bound, loaded and its result read live from the tag bus. Not built: unit tests in ST, programs in a device's slice |
| 2.8, 2.10 | 2.8: the ST editor (2.6), with what the front-end gives it (2.1); editors for LD, FBD, SFC are not built. 2.10: not built |
| 2.9 Emulators | Built: seven programs (`strings.st` since L2c-1), 60 scans each, on both emulated instruction sets, every scan equal to the host's (CI job `logic-vm-bare-metal`). A bug put into REAL addition for Arm only made the two programs that add REALs fail there and nowhere else |
| 2.12 Oracle | `differential_strucpp.rs`: 7 programs, 60 cycles, 7,500 values, all agree except the five deliberate rows of §2.13 (287 values: 167 of the first three, 60 each of INSERT's position and a based literal read as an integer). It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE |

Evidence of the spike (2026-11-17):

- **Tests**: 107 in the five logic crates (42 before format 1). Format 1 was mutation-checked
  with 29 mutations (index range, call depth, recursion at three layers, locals re-initialised,
  region confinement, the loader's check of each new instruction), each turning named tests
  red; and a damaged-bytecode test goes red when one run-time range check is removed. Earlier
  mutation checks: removing INT wrapping, the TP reset, the loader's address check, the
  division-by-zero fault, or counting CTU only to PV each turns a named test red (run before the crates were split;
  the code moved, it did not change). So does undoing each rule taken from the standard:
  rounding a half away from zero, faulting on MOD by zero, the earlier F_TRIG, and giving `=`
  a lower precedence than `<`.
- **L2a** (the parser the editor needs): 136 tests in the five logic crates. `recovery.rs`
  (21 tests) gives the exact list of diagnostics, with ranges, for sources with several
  mistakes: one for each way of reading on, one for each thing the checker stays silent
  about, and every one of the 127 keywords as a variable's name in upper and lower case.
  `damaged_sources_never_panic.rs` damages the six programs of the corpus the way a buffer is
  damaged while someone types (14,680 damaged sources: 14,218 refused, 8,666 of them with
  more than one diagnostic, and 462 that still compile) and requires each to compile or to be refused with diagnostics in
  the order of the source, with ranges inside it, none twice, and no two from the lexer and
  parser about one place. It found three double reports while it was written, each fixed. The
  differential test is unchanged by the rename of `step` (6,120 values, 167 deliberate
  differences). Mutation-checked with 37 mutations, each in a copy of the tree: every way of
  reading on, every silence about what follows from a gap, the ranges, the order, and the
  refusal of a keyword (where it is declared, where it is used, and one word dropped from the
  list) removed in turn; each turned named tests red, none survived. One did at first (a call
  of a function whose result type is unknown): the test that should have caught it could not
  tell, and was made to.
- **Depth** (L2a, after a stack overflow on Windows CI). The overflow was a damaged source in
  which a structure held itself (`range : Limits` inside `Limits`): `holds_instance` followed it
  round without end before the containment check ran, on `dev` too, so any `check` sent to the
  logic module with such a type killed the module's process. Windows met it first only because
  its checkout gives the corpus CRLF line ends, so the fixed seed damaged other text there
  (`.gitattributes` now keeps `*.st` LF everywhere, and the damage test damages CRLF forms on
  purpose). Nesting thousands deep overflowed as well, in each of the 14 kinds and for chains
  of 3,000 structures, named array types and function blocks. Measured before the bound, in a
  debug build: a level of parentheses took 14.8 KiB of stack, a nested IF 11.7 KiB. The parser now
  reads operators by precedence in one loop rather than a function per precedence, and the
  recursive functions of the parser and the checker keep what they say off the path a nested
  construct takes: the deepest allowed takes 320 KiB in parentheses, 536 KiB in IFs and 544 KiB
  in CASEs, all of it included.
  `deep_sources_are_refused.rs` (6 tests) refuses each kind nested 5,000 deep with one
  diagnostic where the limit is reached, compiles the deepest allowed of each on a 1 MiB
  thread, and refuses structures that contain themselves; `logic_module.rs` sends such sources
  to the real module's `check` and `build`, and the same process answers the next request.
- **Scan time on a PC** (`scan_time.rs`, release, i9-11980HK, Linux, not real-time, desktop
  load): a program shaped like a small plant (CASE sequence, 8 TON, CTU, R_TRIG, a PI loop in
  REAL, a 50-turn FOR) is 884 bytes of code and 280 of memory, 970 instructions per scan:
  over 200,000 scans the mean was between 7.3 and 13.1 µs in six runs of the same code (the
  machine was in use; the fastest run: p99 9.3 µs, max 100 µs; the slowest: p99 22.5 µs; the
  worst single scan seen in any run: 336 µs). The VM has not been
  optimised. Windows and macOS: not measured.
- **Microcontrollers** (`scripts/logic-vm-footprint.sh`, CI job `logic-vm-bare-metal`): the VM
  with its loader and scan loop, `opt-level=s`, LTO, linked with `core` alone: **RP2040**
  (thumbv6m-none-eabi) 23,272 bytes of code with strings and format 1 read as well (23,216
  with strings, L2c-1; 19,304 before, 19,200 before L2b, 15,884 before format 1);
  **ESP32-C3** (riscv32imc-unknown-none-elf) 27,254 bytes (27,204 with strings; 22,812 before,
  22,908 before L2b, 19,146 before format 1). The L2b figures and the ones before it were both measured with rustc 1.99.1; the
  RISC-V build came out smaller with more in it, which is the compiler's doing and was not
  looked into. Run on a real board: not yet (none attached). The original ESP32 and the S3 are
  Xtensa, which needs Espressif's Rust toolchain: not built.
- **L2b** (the limits): 13 tests added to the five logic crates (120 on the branch it was built
  on, from 107). `jumps_land_on_instructions.rs` lists every start of every body the plain
  way and points every jump of the six corpus programs, and of one written to have every
  shape the loader's shortcuts are for, at every byte of its body: 99 jumps (81 forward, 18
  back), 72,611 placements, 16,260 taken and 56,351 refused, the loader agreeing with the
  list on every one. Of 24,000 damaged programs, 5,304 now pass the loader (5,495 before).
  The emulators: all 12 runs equal, with the firmware loading within its limits.
  **Load time on a PC** (`load_time.rs`, release, i9-11980HK, Linux, desktop load; best of
  20): a program of 374,211 bytes of code with 18,400 jumps, 1,200 of them back and loops
  nested three deep at its end, loads in 10.1 ms (6.3 ms before the second walk); one of
  1,555,011 bytes in 42.5 ms (20.3 ms before). With three of the four proved targets kept (not
  the head of the loop before a nest), the first took 152 ms: the walks to the heads of nested
  loops started at the top of the body. Load time on a microcontroller: NOT MEASURED.
- **L2c-1** (strings): 21 tests added to the five logic crates (179, from 158), among them
  `strings_run_on_the_vm.rs` (14: every example of Table 34, the edges, every error of
  §6.6.2.5.11 as its fault, strings through functions, blocks, arrays and structures, an element
  read where it is with the stack it needs pinned) and four loader tests for string operands;
  `runtime_vm.rs` watches a STRING and an element of an array of them through the real module
  process; the editor's tests show one. `strings.st` joins the corpus: the oracle compares
  1,380 more values (7,500 in all; the 120 that differ are INSERT's position and a based
  literal, §2.13), the emulators run it on both instruction sets (all 14 runs equal), and the
  fuzz damages it: 20,733 damaged sources, and of 4,000 damaged copies of its bytecode 1,557
  pass the loader and are scanned (the test now requires one in twenty of each program's).
  Mutation-checked with 21 mutations, each in a copy of the tree: every one turned named tests
  red. One survived at first (copying an array element to scratch before a string instruction
  reads it): the copy was redundant, as the VM pops an operand's offset itself, and was
  removed; the two mutations of the stack accounting that replaced it then went red once the
  test put a comparison of elements before its deepest statement.
- **Format versions** (review of L2c-1: the loader refused format 1, which every controller
  running a program held). The six corpus programs compiled by the format-1 and the format-2
  compiler, each tree extracted with its own target directory, differ only in the version
  field and the checksum (the reviewer's comparison); the diff of the VM changes no format-1
  opcode, operand layout, loader check or run-time step. 5 tests added to the five logic
  crates (184, from 179), and one to the runtime module, which takes a format-1 program
  through its own configuration and refuses a newer one. `older_formats_run_alike.rs`: the
  six format-1 fixtures (made once by `dev`'s compiler at `f4637cd`, committed with their
  hashes) run 200 scans each beside the same source compiled now, the same inputs written
  before every scan, memory equal after every scan and changed by more than half of them.
  Mutation-checked, each in a copy of the tree: reading only the current version turns
  `every_format_this_vm_extends_is_read_and_runs_alike`,
  `an_instruction_newer_than_the_programs_format_is_refused_at_load`,
  `format_1_bytecode_runs_to_the_same_scans_as_its_source_compiled_now` and the runtime
  module's `a_program_built_before_an_update_of_the_vm_is_still_taken` red; reading any
  version turns `a_format_newer_than_this_vm_is_refused_not_guessed_at`,
  `a_program_of_the_format_before_this_one_is_refused_by_its_version` and the runtime
  module's test red; skipping the check of each instruction against the program's version
  turns `an_instruction_newer_than_the_programs_format_is_refused_at_load` red. The oracle
  and the emulators are unchanged (7,500 values, 287 deliberate; all 14 runs equal).
