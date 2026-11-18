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
 front-ends (a crate per language)                 backends (a crate per target kind)
 mythos-logic-st ──────┐                        ┌─► mythos-logic-backend-vm ─► bytecode ─► mythos-logic-vm
 mythos-logic-graphic ─┼──► mythos-logic-ir ────┤                                          (PC runner,
 (LD, FBD; SFC later) ─┘     (the seam)         └─► (C99 later: MCUs too small for the VM)   ESP32, RP2040)

 mythos-logic-compiler: joins a front-end, the target's profile and a backend. Owns neither.
```

| Crate | May depend on | Why |
| :--- | :--- | :--- |
| `mythos-logic-ir` | nothing of the workspace | The seam. A *unit*: structures, array types and POUs (functions, function block types, one program), every name resolved to an index; no addresses, word sizes or instructions. `serde` data with a version (`IR_VERSION`), so a front-end and a backend need not share a process |
| `mythos-logic-vm` | nothing at all | Builds for a microcontroller with `core` alone |
| `mythos-logic-st` (and each later language) | the IR | A language is added without touching a target |
| `mythos-logic-graphic` (LD and FBD; `mythos-logic-ld` until L3c-1, §2.16) | the IR, and the ST front-end | A graphical language reads the common elements (declarations, types, literals), the text inside its elements and the standard library through the ST front-end, so that both have one rule for each (§2.15); it reaches no target. LD and FBD are one crate because one PLCopen TC6 document holds POUs of both, each calling the others (§2.16) |
| `mythos-logic-backend-vm` (and each later backend) | the IR, its own target | A target kind is added without touching a language |
| `mythos-logic-compiler` | all of the above | The one call from source to a program |

`crates/mythos-logic-compiler/tests/layers.rs` reads the real dependency graph and fails on a
break, and on a new `mythos-logic-*` crate that has not declared its layer; a second test feeds
the guard each kind of break.

- **Nothing unsupported is compiled into something else.** A construct a front-end does not take
  yet is a diagnostic naming it, with its line and column.
- **LD and FBD are lowered from their graphs**, not printed as ST text: each to the ST
  front-end's syntax tree, built element by element, never through text (§2.15, §2.16).
- **Standard function blocks** are a library written in Structured Text (§2.14, L4): a block
  is source compiled with the program that uses it, so adding one needs no new firmware. The
  IR has no catalog of them and the compiler emits no native call; the VM keeps its native
  blocks only so that programs of formats 1 to 3 keep running.
- **IL is not built** (Adrian, 2026-11-17: "iya, IL tidak dibuat"). Ed. 3 §7.2.1: "This
  language is outdated as an assembler like language. Therefore it is deprecated and will not
  be contained in the next edition of this standard." The languages are ST, LD, FBD and SFC.

### 2.2 Target profiles are data

A target declares what it can run: the bytecode format its VM reads (the newest; a VM
reads every format before it, §2.3), types it refuses, the
variable memory and code a program may use, and how deep its evaluation stack and its calls may
go (`max_stack_depth`, `max_call_depth`: what the target's firmware has). The compiler checks
the IR and its output against the profile, so a program that does not fit is refused at compile
time with the reason, never on the controller: `the program needs a stack 7 values deep; the
target board allows 6`. A profile that leaves a depth out states no limit, which is not a limit
of zero; the `pc` profile states none, because a PC runner gives each program what it declares. A profile is `serde` data: **a runtime module carries its own in its manifest**
(§2.4), so a new board brings its profile without a compiler change. Until runtime modules exist,
three stand in (`pc`, `rp2040`, `esp32`), each declaring the VM's own format, 4 since L4; a
profile that declares another is refused with the reason (`the target board-3 runs bytecode
format 3; this compiler makes format 4`): such a VM cannot load what this compiler makes, and
the compiler makes the newest format only. Their microcontroller limits are budgets chosen for a
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
  - *Today.* This VM reads formats 1, 2, 3 and 4 (`OLDEST_FORMAT_VERSION`,
    `FORMAT_VERSION`). Format 2 added the thirteen instructions on strings (`0xD0`–`0xDC`),
    format 3 the thirteen numeric ones (`0xE0`–`0xEC`), format 4 one, `NOW` (`0xF0`), each
    on opcodes no earlier instruction had, and none changed anything of the formats before:
    the programs of the corpus compiled by the format-1, the format-2 and the format-3
    compiler differ only in the version field and the checksum. Format 0, the spike's, had a
    16-byte header with no call depth and no table of bodies; format 1 did not extend it, so
    it is refused. `older_formats_run_alike.rs` runs the corpus as the format-1, the format-2
    and the format-3 compiler made it (committed bytes, never regenerated; the format-3 ones
    with their symbols) against the same sources compiled now, 200 scans each with the same
    inputs. Since format 4 the standard blocks are ST and lay their state out as variables of
    their own (§2.14), so memory is laid out otherwise: every variable the two programs share
    by path and type, every public member of a standard block among them, is equal after
    every scan.
  - *Format 4* (L4). `NOW` pushes the scan clock: the `now` the host gives the scan, the time
    since the program was put in RUN in nanoseconds, the same for every instruction of the
    scan; the native blocks were given the same. Only the standard library asks for it,
    through the intrinsic `__NOW()` (§2.14). In a program of format 1 to 3 it is refused at
    load (`NotInFormat`). `FB` and `FBP`, the native standard blocks, stay in every format,
    4 included (a format only adds to the one before), so the programs of formats 1 to 3 run
    as they did; the compiler of format 4 no longer emits them.
- **Format 4** (format 1 with strings, L2c-1, the numeric functions, L2c-2, and the scan
  clock, L4).
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
  - *Numeric functions* (format 3): `RFN which` (and `RFN32` for a REAL) for SQRT, LN, LOG,
    EXP, SIN, COS, TAN, ASIN, ACOS, ATAN and truncation, `ATAN2_*`, `EXPT_*` (a real power)
    and `EXPTI_*` (an integer power), `ABS_*`, and `CHKSEL count`, which stops the scan with
    `SelectorOutOfRange` when MUX's K has no input. SEL, MAX, MIN, LIMIT and MUX are jumps
    and comparisons the backend emits; SEL and MUX work out only the input they choose.
    **The VM computes the real functions itself** (`mythos_logic_vm::math`): `core` has none
    on the targets the VM runs on (no libm), and the VM takes no dependency (§2.1); `libm`, a
    port of musl, would be one, and would be code nobody here has read. The methods are
    fdlibm's (argument reduction, a minimax polynomial, the reduction undone), with Payne and
    Hanek's reduction for SIN, COS and TAN past 2^20 · π/2, using 1,280 bits of 2/π computed
    for it; SQRT is exact. Built from the basic operations only, which IEC 60559 makes exact
    in software as in hardware, they give the same bits on every target, which the emulators
    check scan by scan (a libm differs between platforms). A REAL's is computed in LREAL and
    rounded once.

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
- **Swap.** A new build starts beside the old one; the old one's variables whose path, type and
  size match, read under the lock of a whole scan, are carried into it before its first scan
  (§2.5, online change); then I/O switches and the old one stops. A build that cannot start,
  **or whose profile cannot run the program now running**, is refused and the old one keeps
  running.
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
- **RETAIN, warm and cold restarts** (C7b; IEC 61131-3 §6.5.6). `run` from STOP or FAULT is
  a *warm restart*: a variable the compiler marks RETAIN (`"retain": true` in the symbol table)
  keeps the value it had, every other variable takes its initial value. `POST
  /api/v1/runtimes/<id>/cold-start` (engineer; MCP `cold_start_runtime`) is a *cold restart*:
  every variable takes its initial value, RETAIN ones included, and the program runs; a
  program in RUN is restarted too. The core keeps the RETAIN values of each runtime of the
  active project in `<system>/retain/<project>/<runtime>.json`, beside the controller's own
  data and not in the project: it asks the module (`Call::Retained`) every two seconds and
  writes the file when they changed, and again at once after a change of mode and before an
  unload. A module process started by the core (a load, a server start, a process started
  again after a crash) is given the values kept last (`retained` in its start
  configuration), so a start into RUN is a warm restart: a restart of the server loses at most
  two seconds of them. A value is taken only into a RETAIN variable of the same path, type and
  size; a program built again from changed text takes what still fits and the module logs the
  rest. A program loaded in STOP reports the values it was given until it runs, so waiting
  never replaces them with its zeros. A module that keeps no RETAIN values says so and every
  start of it is cold.
- **Online change** (C6b; the owner's decisions of 2026-11-18). When a load replaces a program
  in RUN with a new build of the same program (the same program name, in the same runtime
  module), the new build takes over the running one's variables and scans on, without a cold
  start or a warm one. Every variable present in both with the same path, type and size carries
  over, RETAIN or not, the members of a function block instance included: the standard blocks
  are Structured Text since L4, so a timer's or a counter's state is variables of its own. Every
  other variable takes its initial value, or a RETAIN one the value kept for it, as at a warm
  restart. A value whose bytes do not fit its variable is not carried: the variable is
  initialized and listed, never given a guess. An array of structures or of instances is not
  carried either: its type names its elements but not their layout, which a build may change
  under the same name. The program's scan clock goes on from the running one's, with the time
  the swap took, so a timer neither loses nor gains it.
  - *How.* Just before the new build's module process is given its program, the core asks the
    running one for its variables at its next exchange (`Call::Variables`: every symbol's bytes
    in the form of a `RetainedValue`, and its clock; a program not in RUN gives none) and gives
    them to the new one (`carried` beside `program` in the start configuration). The new one
    puts them over its initial and RETAIN values before its first scan, and says what it took
    (`Call::Carried`). A process started again after a crash is not given them again: that start
    is a warm restart. The snapshot is taken at most one exchange before the new process is
    given its program, and the running build goes on scanning until the swap: **the old program
    may scan once more after the snapshot** (more often when its interval is shorter than the
    time the new build takes to load), and what it computes then is not carried. A counter may
    therefore go on from a count a scan behind the last one the old build published.
  - *Outputs.* The new build claims the program's outputs once its process has started, before
    the old one stops (write authority, §2.4: an owner claiming again takes its tags over). The
    old one, stopping, leaves alone an output a newer claim holds, so no safe value is written:
    the device holds what the old build last wrote until the new build's first scan writes it
    from the carried state. An output the new build no longer has is the old one's still, and is
    written its safe value when it stops. The new build's published tags read as not yet scanned
    for the exchange before its first scan, as at any load.
  - *When it is not.* A program of another name, one in another runtime module, one that was not
    in RUN or that an engineer had asked to stop, one loaded in STOP (`"run": false`), and one
    whose module cannot give its variables or does not within 2.5 s, start as before: RETAIN
    variables from the values kept, the rest from their initial values. `PUT
    /api/v1/projects/:id/runtimes?start=cold` (MCP `save_runtimes_config`, query `start`:
    `cold`) starts every runtime the load starts or replaces cold, RETAIN variables included;
    one the load leaves unchanged is not touched (`cold-start` restarts it). Another value of
    `start` is refused and nothing is saved.
  - *What the load says.* The answer's `changes.runtimes` has, for each runtime the load
    started, `start`: `online`, `warm`, `cold`, or `unknown` (its module was given the running
    values and does not say what it took, which is never reported as online); `why` it was not
    changed online; and, changed online, `carried` (how many variables) with `dropped` (the
    running build's variables it did not take) and `initialized` (its own that took none), each
    with why. The editor's Load says the same (§2.6).
  - Forces are released by a load, as before (Forcing, below): a forced variable carries the
    value it was held at, as a release leaves it, and the program decides it again from its
    first scan. The kept RETAIN values are the new build's to write from the replacement on: the
    old one, stopping, no longer writes them, and the new one keeps its own at its first
    exchange (after a cold load, in place of the old one's).
- **Forcing** (C5c; the owner's decisions of 2026-11-18). `POST /api/v1/runtimes/<id>/force`
  `{variable, value}` holds a variable of a running program at a value; `POST
  /api/v1/runtimes/<id>/release` `{variable?}` releases one, or every one. MCP
  `force_runtime_variable` and `release_runtime_forces`. **Only an engineer** may do either,
  and each is written to the audit log with who did it. A forced variable is written its
  value before every scan, so the program reads it, and after, so what the program wrote does
  not stand: an output bound to it is written it, a published tag gives it, an input bound to
  it no longer takes the field's value, and an operator's setpoint to it is refused saying it
  is forced. Only a BOOL or a number (or one element of an array of those) is forced; a TIME
  and a STRING are refused. A program not in RUN is refused. **Every force is released when
  the program leaves RUN** (stopped by an engineer, faulted) **and when a program is loaded**
  (a new module process starts with none), so none outlives the run it was set in; the VM
  module holds at most 64. **It is visible wherever the value is:** `RuntimeState.forced`
  (each exchange) lists the forced variables with their values, `GET /api/v1/runtimes` gives
  them per runtime (`forced`, `null` while the module does not answer) with `forced_tags`,
  every tag bound to a forced variable, and a watched value carries `forced: true`. The
  protocol calls are `Call::Force` and `Call::Unforce`; the core carries an ask to the
  runtime at its next exchange, after any mode change, and waits up to 2.5 s for the answer:
  an ask whose caller stopped waiting (`504`) is dropped, not carried out later. A runtime
  module without forcing answers that it cannot force.
- Data types shared by more than one module (logic and the HMI both have them today, apart) live
  in the core, since a module may not depend on another (ADR-011 §2.6).

### 2.6 The logic module is authoring

`plugins/logic` (`mythos-module-logic`), a `service` module (ADR-011 §3 step 5): programs in
`<project>/logic/`; check; compile for a target; the variable table and each variable's binding
to a tag. It never talks to a runtime itself. Every route is an MCP tool. A device's programs
are part of its slice (ADR-012 §2.5).

As built (C3):

- A program is its source and an entry in `programs.json`: its language, its interval, the
  runtime module and the target profile it is built for, whether it runs when loaded, and its
  bindings (inputs, outputs with their safe values, published variables). One program is one
  cyclic task; a task that runs several programs is not built.
- **Its language** (L3b-1; `fbd` since L3c-1) is `st`, `ld` or `fbd`, chosen when it is
  created (`create_logic_program` takes `language`, `st` by default) and not changed after.
  Structured Text is `programs/<name>.st`; a Ladder Diagram or a Function Block Diagram is
  `programs/<name>.xml`, a PLCopen TC6 XML v2.01 document holding its POUs (§2.15, §2.16). A
  new FBD program is a TC6 project holding the PROGRAM with no variable and an empty `FBD`
  body, which compiles to a program that does nothing, with the file header an LD program
  has. One reader takes both languages and each POU's body says its own, so the language of
  an entry chooses the template and the view; `check` gives `power` for `ld` and `values` for
  `fbd` (§2.16). An entry without `language`, as C3 wrote them, is `st`,
  and is written back with it. A new LD program is a TC6 project holding the PROGRAM with no
  variable and a body of the two power rails and no rung between them, which compiles to a
  program that does nothing; its file header names no company (who wrote it is not known) and
  gives the module's name, version and the time it was made. The source routes take and give
  the document's text as they take ST.
- **Each program is compiled from its own source alone**, through `compile_sources` with that
  one source. A project's programs share no POU: every source holds a PROGRAM and a unit has
  exactly one, so another program's functions and function blocks could not be compiled in
  without taking its PROGRAM out of its file; and a build that took another program's POUs
  would change, and get a new version, when that other program is edited, so a running
  program would be changed online by an edit nobody made to it. What a program calls is in
  its own source: in an LD document, function and function block POUs beside its PROGRAM, in
  LD; in ST, before it. A library of the project's (sources with no PROGRAM, given to every
  build) is not built; `compile_sources` already takes one ST source and many LD documents, so
  it needs storage and routes, not a compiler change.
- `check` compiles the saved source, or a text sent with the request and not saved, and answers
  with the diagnostics (each with the range it is about, §2.8; in a Ladder Diagram also the
  element, §2.15), or with the variables and every binding that names a variable the program
  does not have or one that cannot be bound (an array, a TIME, a STRING). It also gives the
  version a build of that text with the saved settings would have (the hash `build` gives), so
  an editor knows whether what it shows is what a runtime runs; each variable whether the LD
  front-end made it for itself (`hidden`: a part of its path begins with two underscores, which
  §6.1.2 forbids in every name a program declares); and for a Ladder Diagram `power`, where the
  power of each element output is held (§2.15, power flow). A STRING cannot be bound because a tag holds no text: a tag's
  value is a BOOL or a number (`mythos_plugin_sdk::SampleValue`, and the tag bus's slot);
  it is watched instead.
- `build` compiles the saved source and answers with the runtime instance of §2.4, its version
  a hash of the code, the interval and the bindings. **A module cannot call the core**
  (ADR-011 §2.6), so the module does not load what it builds: the workbench, or an agent, saves
  the instance into the project's runtimes, and the core loads it. An edit therefore changes
  nothing that runs until it is built and loaded. A bound tag that no connection provides now
  is a warning of the build, not a refusal.
- Not built: unit tests in ST; a library of POUs shared by a project's programs (above). A
  device's slice carries its programs (C6a, §3), LD documents as any file of `logic/`.

Its workbench UI (C4), `frontend/packages/logic-ui`, served beside the module (ADR-011 §2.3):

- Programs in the project tree, each with its language (ST, LD, FBD), added in the language
  chosen beside the name, and an editor per program: the Structured Text (a text area
  over the same text painted in colour; no editor library), or the Ladder Diagram: its editor
  for an engineer and the read-only view for everyone else (§2.8), or the Function Block
  Diagram, read-only for everyone until its editor (L3c-2, §2.8), the last check's diagnostics,
  each underlined over the whole range it is about (§2.8) and listed with where it starts, the task settings, and the variables of the last text that
  compiled, each with its bindings to tags and each bound tag's live value.
- The editor keeps three things apart: the text in the editor, the text that is saved, and the
  program that runs. **Load** saves, checks, builds, reads the project's runtimes, replaces the
  instance with the program's name and saves them: that is the call the logic module cannot
  make. A program that does not compile, or whose bindings do not match its variables, is not
  built. A program the controller saved and then lists under `changes.failed` is said not to
  have started, with the controller's reason, and not to be loaded. **Run**, **Stop** and **Unload** go to the core.
- **How a load started** (C6b): after Load the editor says what the controller said of the
  runtime in `changes.runtimes` (§2.5, online change): changed online, how many variables
  carried over, and each one that did not, with why; not changed online, with why; loaded cold;
  or that how its variables started is not known. When the controller says nothing of it, the
  editor says that, and claims nothing. **Load cold**, asked first, loads with `?start=cold`.
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
- **Forcing from the editor** (C5c): while a runtime is known to run the program, an engineer
  has "Force…" beside each BOOL and number of the table; it asks for the value (TRUE/FALSE or
  1/0 for a BOOL, a whole number for an integer), which is the confirmation, and refuses what
  is not a value of the type before asking the controller. A forced value is marked FORCED in
  the table and has "Release" in place of "Force…"; the Runtime panel says FORCED and how many,
  lists each forced variable with its value (UNKNOWN when the controller does not say, never
  "none"), and has "Release all", which asks first. Nobody but an engineer is offered either;
  everyone sees what is forced.
- **The forced mark on a tag's value** (C5c-3): the workbench reads `GET /api/v1/runtimes`
  beside the controller's state and gives every module `forcedTags` (`ControllerView`): the
  tags bound to a forced variable, or `null` when the runtimes cannot be read (never `[]` in its
  place, which would claim nothing is forced). One mark of the SDK (`ForcedMark`) says FORCED
  beside the value in the HMI's I/O field (for the project the controller runs), the tag table,
  the inspector and the editor's bound tags; the online tools card counts the forced tags.
- Not built: watching an element of an array from the table; the FBD editor (L3c-2) and SFC.
  An FBD program is drawn read-only, and follows a source saved through `save_logic_source`
  elsewhere unless it has changes of its own, as a Ladder Diagram does. A Ladder
  Diagram is edited in the workbench (§2.8, L3b-2b) and can still be saved through
  `save_logic_source`: an open diagram re-reads a source saved elsewhere unless it has changes
  of its own, and the editor then opens it afresh (its undo starts again; the localIds it gave
  out are not given out again).

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

**The Ladder Diagram view** (L3b-1; `LadderView.tsx`, `ladder.ts`). An LD program opens in the
same editor as ST, with the diagram in place of the text; the variables table, bindings, Load,
Load cold, Run, Stop, Unload, forcing and watching are the ST editor's, unchanged. The diagram
is SVG, read-only, drawn from the document's own `position`s (and `relPosition`s of pins where
the document gives them): the power rails; each rung numbered in the order it is evaluated
(the front-end's rule, §2.13: connected elements without the rails, top to bottom by the
topmost, then the leftmost); contacts (normally open, negated, P and N for rising and falling),
coils (plain, negated, S, R, P, N), blocks with their type, instance and pins (an input's
expression written at it, a negated pin marked), in, out and in-out variables, jumps, labels,
returns and comments. An element it does not know (a connector) is listed as not drawn. Each
POU of the document is drawn; a function's or function block's has no live state, since each
call or instance has its own. A diagnostic with an `element` is drawn as a mark around it and
listed as `<pou> element <localId>` (or `variable <name>`). It uses the workbench's tokens
only (`--c-good` for powered, drawn solid; `--c-text-muted` for not powered, drawn dashed, the
meaning of TIA Portal's convention; `--c-warn` dotted for UNKNOWN; `--c-text-faint` for a rung
not executed); its visual design is the owner's to lead.

**Live state is drawn only from watched values, and only of the version on screen**: while the
check of the text shown is current, its `version` equals the runtime's, the runtime is in RUN
and the watch answer is of that version in RUN. Otherwise nothing is drawn live and the view
says why (another version runs, the program is in STOP, it is not loaded, the text does not
compile). Then:

- every element that carries power (a contact, a coil, a BOOL variable) and every link is
  drawn with the power the compiler kept for it when its rung was evaluated in the last scan
  (L3b-2a, §2.15, power for monitoring): a link carries the power of the output it starts
  from, and past the point where several links join, the junction's (`ld-junction`). ON (from
  the left rail) needs no value. An output that carries no power (a value that is not a BOOL)
  is drawn with no state, never one worked out;
- a rung whose flag says it did not run in the last scan (a jump or a return skipped it) is
  drawn as not executed: its number marked, and none of its elements or links with a power
  state, since what they keep is from a scan that ran them; FORCED marks stay;
- under a contact or a coil, its variable's value (a negated contact's negation; a
  transition-sensing contact's none, since its variable does not say whether it conducted): a
  contact may conduct while no power reaches it, and a coil's variable need not be its power
  (a negated, set or reset coil);
- a value that cannot be read (the watch fails, the runtime does not have the variable, it is
  not a BOOL) is UNKNOWN, never ON or OFF, and a rung whose flag cannot be read is UNKNOWN as a
  whole; when the runtimes can no longer be read every state is UNKNOWN; a forced variable is
  marked FORCED under its element (`variable-forced`), as in the table.

The front-end's hidden variables are watched for the diagram and not listed in the table. An
element's variable that is not a variable of the program by itself (an element of an array) is
watched as well.

**The Ladder Diagram editor** (L3b-2b; `LadderEditor.tsx` beside the view, and `ldmodel.ts`,
`ldedit.ts`, `ldcatalog.ts`, `ldxml.ts`; what it writes is in §2.15). **Decided by the owner
(2026-11-18):** the IEC 61131-3 editors follow the TIA Portal's workflow and conventions as
closely as they can: networks, each with a title and a comment; an instruction palette and a
favourites bar; symbolic operands with completion; online monitoring with power-flow colours.
Its artwork is our own, never Siemens' (no artwork, icon or branding of it), and no code of TIA
Portal, CODESYS, Beremiz, OpenPLC or any other tool was read or copied; the palette's symbols
are IEC 61131-3's semigraphic forms (`-| |-`, `-|/|-`, `-( )-`, …). The visual design pass is
the owner's to lead: the editor uses the workbench's classes and tokens and decides nothing
past clarity and legibility.

- **Who.** An engineer gets the editor in place of the read-only view; a viewer and an
  operator get the view, as for ST. The program is unsaved until Save (or Ctrl+S), as ST is;
  Load saves first, as for ST.
- **What is drawn is what is saved.** Every edit changes the editor's model, which is written
  back into the TC6 text at once (§2.15); each network is drawn by the view's `PouView` from
  that text as the view reads it, moved to the top of its own drawing, with the body's rails.
  The live state is the view's (§2.8 above), drawn only on the PROGRAM; the rung number the
  view draws is hidden, as the network's header numbers it. Over the drawing the editor draws
  the selection, its own marks of what is wrong, and the insertion points (small squares on
  the rung).
- **Networks**: a list in the order of evaluation; added after the selected one (or at the
  end), inserted above one, deleted, moved up and down; each with its number, a title, a label
  (§8.1.4, what a jump names) and a comment. A network another tool wrote that is not a series
  of contacts, boxes and coils with parallel branches is kept as it is (§2.15) and says why: it
  can be moved, deleted, titled, labelled and commented, and nothing is put into it.
- **Instructions**, from the palette (grouped: bit logic; timers; counters; edges and
  bistables; compare; math; move; strings; the document's own function and function block
  POUs; program control) or from the favourites bar above the networks, by click or by drag:
  contacts normally open, normally closed, positive and negative edge; coils plain, negated,
  set, reset, positive and negative edge; open and close a parallel branch; a box of any
  function or function block, the standard library's (TON, TOF, TP, CTU, CTD, CTUD, R_TRIG,
  F_TRIG, SR, RS, the comparisons, ADD … MOVE, SEL, the numeric and string functions, a
  conversion `X_TO_Y` typed into an empty box) or the document's own, with its pins; a jump,
  the network's label, a return. A program compiles from its own source alone (§2.6), so the
  POUs a box can call are the library's and the document's: an ST POU of the project is not
  among them. The favourites start as TIA's set (normally open, normally closed, coil, empty
  box, open branch, close branch); any instruction is added or removed with its star, and the
  set is kept per viewer in the browser, as is whether the palette is shown: hidden, the
  networks have its width.
- **Where an instruction goes**, as TIA inserts: after the selected element in its series; at
  a selected insertion point; with only the network selected, a contact or a box before the
  coils at the end of its rung, and a coil, jump or return at its end. Dropped on an element it
  goes after it; on an insertion point, there; on the network, as with the network selected.
  A jump or a return ends its rung: nothing is put after one, and one is put only at the end.
- **A parallel branch** is opened at an element and closed at an element of the same series
  (Close alone wraps the selection, which Shift+click or Shift+←/→ widens along its series):
  that run becomes the upper branch and a new, empty branch below it gets the insertion point.
  Branches are always closed in the model, so a parallel coil is a branch around a coil.
- **Boxes.** A box's pins are its declaration's (§2.14 for the library; the interface for the
  document's POUs). Which pin carries the rung (§8.2.6): a function block whose first input is
  a BOOL takes it there (IN, CU, CLK, S1) and gives it from its first output when that is a
  BOOL (Q, QU, Q1); any other box takes it at EN and gives it from ENO, except a function whose
  result is a BOOL (the comparisons), which gives it as OUT, so a comparison passes power while
  it holds. Every other input takes an operand written at the pin (a TC6 `expression`); every
  other output an operand written by an output variable wired to it. A function block gets an
  instance, as TIA asks for an instance DB: a name no variable has (its type in lower case and
  a number, `ton1`) is proposed in the instance field, and Enter there declares it in VAR.
- **Operands** are typed with completion from the POU's variables (name and type; a contact or
  coil is offered BOOL ones; an instance's outputs as `ton1.Q`). Enter takes what was typed,
  or the suggestion the arrows went to. What is wrong is shown at once, from the editor's own
  check, written under the element's fields and marked around it: a name not declared; a value
  of another family than the place takes (BOOL, TIME, STRING, number); a coil writing an input
  of the POU; an input of a function block read from outside; a function's input with no
  operand; a box with no type or one nothing declares; a function block with no instance, one
  of another type, or one called twice in a network; a jump to a label no network has; an
  empty branch (a wire: what is in parallel with it is bypassed); a rung ending in a contact.
  Then the compiler's: 700 ms after the last edit the editor checks the text by itself (a
  quiet check, which neither marks the editor busy nor reports a failure as an error, and
  whose answer for an older text is dropped), and each diagnostic is drawn at its element and
  written under it. A plain name that is not declared is offered to be declared there, in a
  section chosen (Input, Output or Local) with a type: proposed BOOL input for a contact, BOOL
  output for a coil, the block's type in VAR for an instance, the pin's type where it is
  concrete and chosen where it is generic; Ctrl+Enter takes the operand and declares it as
  proposed.
- **Undo and redo** cover every edit, headers and declarations among them. A localId given out
  is not taken back by undo (§2.15).
- **Keys**, while the editor has the focus (TIA's for the favourites): Shift+F2 normally open
  contact, Shift+F3 normally closed contact, Shift+F7 coil, Shift+F5 empty box, Shift+F8 open
  a branch, Shift+F9 close it; Ctrl+R a network after the selected one; Alt+↑ and Alt+↓ move
  the selected network; ← and → the previous and next element, Shift+← and Shift+→ widen the
  selection along its series; ↑ and ↓ the previous and next network; Enter the first field of
  the selected element; Delete what is selected (an element, a run of them, an empty branch at
  its insertion point, or a network); Ctrl+Z undo, Ctrl+Y or Ctrl+Shift+Z redo; Ctrl+S save. In
  an operand field: ↑ and ↓ choose a suggestion, Enter takes it (or what was typed), Ctrl+Enter
  takes it and declares it as proposed, Escape goes back to the diagram without it. The
  browser's own Ctrl+R is taken only while the editor has the focus.
- Not built: moving an element by dragging it (it is deleted and put again), copy and paste,
  a box's pin wired to a second rung path (an operand is written there), editing a network kept
  as found, adding a POU to the document.

**Where the program editor's parts are** (2026-11-18, the owner's question why the work area
was crowded while the workbench's panes stood empty). As TIA's block editor: the work area holds
only the program's body and its interface (the variables with values and bindings, folded or not,
above the body). The Ladder Diagram's instructions are the **Instructions** task card (the
favourites bar stays on the editor's toolbar); the runtime's state and commands (RUN/STOP,
Unload, Release all, scan times, forces) are the **Runtime** task card; the program's properties
(language, task interval, runtime module, target, Run when loaded, Remove) are the inspector's
Properties, selected when the editor opens and again on a click in it; what the compiler said
is a panel of the inspector's **Diagnostics** tab (`host.setDiagnostics`), counted on the tab
only for the text as it is now, and brought forward after a Check the engineer asked for, and
after a Load only when the compiler refused it. The toolbar says the check in a word. Nothing is set while another
editor is active: the workbench draws only the active one.

**The Function Block Diagram view** (L3c-1; `FbdView.tsx`, `fbd.ts`). An FBD program opens in
the same editor as ST and LD, with the diagram in place of the text; the variables table,
bindings, Load, Load cold, Run, Stop, Unload, forcing and watching are unchanged. It is read-only
for everyone, an engineer included, until the FBD editor (L3c-2), and says so. The diagram is
SVG drawn from the document's `position`s (and `relPosition`s of pins where given): blocks with
their type, instance and pins (an input's expression written at it, a negated pin with the open
circle of §6.6.1.4.3 rule 9), input, output and in-out variables (a negated one with its circle),
connectors and continuations (the arrow shapes of §8.1.3), labels, jumps, returns, comments, and
every line from the output it starts at to the input it reaches; each network numbered in the
order it is evaluated (§2.13: the compiler's, a connector joining its line). An element FBD does
not have (a contact) is listed as not drawn. A diagnostic with an `element` is a mark around it,
listed as `<pou> element <localId>`. Each POU of the document is drawn; a function's or function
block's without live state. Workbench tokens only (`--c-good` for a BOOL on, solid;
`--c-text-muted` dashed for off; `--c-warn` dotted for UNKNOWN; `--c-text-faint` for not
executed or not computed); its design is the owner's to lead.

Live state, under the same conditions as LD's (the check of the text shown is current, its
version is the runtime's, in RUN, and the watch answer is of that version in RUN):

- every line carries the value its output had when its network was evaluated in the last scan
  (`values` of the check, §2.16): a BOOL drawn on or off and written TRUE or FALSE at the
  output; a number, a TIME or a STRING written at the output as the variables table writes it
  (`formatVariable`). A line from a continuation carries what its connector's line does;
- an output of a function that did not run in the last scan (its kept ENO, `ran`, is FALSE) is
  drawn as *not computed*, and its value of an earlier scan is never shown;
- a network whose flag says it did not run is drawn as not executed, its lines and outputs
  without a value; under an input, output or in-out variable its own value, and FORCED where it
  is forced; a forced output (an engineer may force a hidden variable, §2.13) is marked FORCED
  where its value is written;
- a value or a flag that cannot be read is UNKNOWN, never a value; when the runtimes or the
  values cannot be read every state is UNKNOWN; a line from a literal is drawn without a state,
  since the literal is written on its element and nothing keeps it.

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
  There are no warnings, so there is no severity. A diagnostic about a Ladder Diagram POU
  also names the element (`element`: the POU, the `localId` and position, or the variable of
  the interface, and the characters of its text), and one of a program compiled from several
  sources names its `source` (§2.15); both are absent for ST.
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
  machine running the script. Since L3a it runs the Ladder Diagrams of
  `crates/mythos-logic-compiler/tests/ladder/` as well (`oscillator.xml`, §2.15). The format lays memory out identically everywhere, so this
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
| Timers | Figure 15 (timing diagrams) | TON, TOF, TP as drawn, ET included; since L4 in ST, the same scan for scan as the native ones (§2.14) | agrees (it found our TP bug) |
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
| Writing an output, or anything inside an instance, from outside | Figure 13 rule 4b; §6.6.3.2 rule 11 (VAR is private) | refused; an internal variable cannot be read from outside either, except to look into an instance held there (`a.b.q`). The host keeps the same rule (2026-11-18): every member of an instance that is not one of its inputs is `"private": true` in the symbol table, and a tag is never bound to write one (an input binding or a setpoint is refused by the logic module and by the runtime module); it may be read (an output binding, a published tag, watching) and forced, which is an engineer's override | not compared |
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
| Accuracy of the real functions | §6.6.2.5.8: "expressed in terms of one or more Implementer specific dependencies" | measured against the host's libm (`math_against_the_host.rs`, 200,000 points each, on Linux, macOS and Windows in CI): SQRT exact; EXP, LN, SIN, COS, ATAN, ATAN2 within 1 ulp; LOG, ASIN, ACOS within 2; TAN within 2 of glibc's and 3 of Apple's (the libms differ; ours gives the same bits everywhere); EXPT within 4, and within 128 where the result nears the ends of LREAL (`e^(y ln x)` with `y ln x` near ±700). SIN, COS, TAN of any finite argument, to 10^300 | glibc's; agrees to the 1e-6 the REPL prints |
| A result that is not a real number | §6.6.2.5.8: a result outside the output's range is an error; Table 10 footnotes e and f leave infinity and NaN to the implementer | IEC 60559's: SQRT(-1) and ASIN(2) are NaN, LN(0) is -∞, EXP(1000) is +∞; the scan goes on, as for REAL division by zero | not compared |
| ABS of the most negative integer | §6.6.2.5.8: an error | wraps (ABS of SINT -128 is -128), as every result outside its type does here (Overflow, below) | not compared |
| EXPT | Table 29 footnote f: IN1 an ANY_REAL, IN2 an ANY_NUM, the result IN1's type; Table 71: `**`, precedence 7, so `-2.0 ** 2` is 4 | as written; an integer exponent by squaring (`2.0 ** 10` is 1024 exactly), 0.5 as SQRT, a negative base to a power that is not integral is NaN | agrees |
| TRUNC | Table 22 features 2b, 2c and footnote c: toward zero; `TRUNC_INT(-1.6)` is -1 | `TRUNC_<int>`, `REAL_TRUNC_<int>`, `LREAL_TRUNC_<int>`; the old TRUNC alone (2a, deprecated) is not taken | has no TRUNC_* |
| A function of reals given an integer | Figure 11: SINT and INT widen to REAL, DINT to LREAL, LINT to neither | as the figure: `SQRT(n)` of a DINT is an LREAL; of a LINT refused | not compared |
| SEL, MAX, MIN, LIMIT, MUX | Table 32; footnote e: MUX's K outside 0 … n-1 is an error | of numbers, TIMEs and BOOLs, the inputs taken to their common type as an operation's are; `LIMIT` is `MIN(MAX(IN, MN), MX)`; a K outside its inputs stops the scan (`SelectorOutOfRange`), a literal one is refused. SEL and MUX evaluate only the input chosen (a function has no effect a program sees but a fault: one in an input not chosen does not stop the scan) | agrees on what was compared; it falls through to the last input on a K out of range |
| The operators as functions | Table 29, Table 33 | ADD, MUL, GT, GE, EQ, LE, LT of two or more (a comparison of each neighbouring pair), SUB, DIV, NE of two, MOVE of one. MOD, AND, OR, XOR, NOT are keywords of ST and stay operators | agrees |
| A user POU named as a standard function | not addressed by ed. 3 | refused, naming the word, as a type's or a conversion's name is (`FUNCTION Max` would make `MAX(a, b)` mean two things). A variable may bear such a name | not compared |
| Which sections take RETAIN, NON_RETAIN | §6.5.6.1, §6.5.6.2, Table 40 and Table 47 features 4a–5b, Annex A (`Input_Decls`, `Output_Decls`, `Loc_Var_Decls`, `FB_Input_Decls`, `FB_Output_Decls`, `Global_Var_Decls`): static VAR, VAR_INPUT, VAR_OUTPUT of programs and function blocks, and VAR_GLOBAL; not VAR_IN_OUT; Annex A gives VAR_TEMP, VAR_EXTERNAL and a function's sections none | those three in a PROGRAM and a FUNCTION_BLOCK. Refused with the reason: on VAR_IN_OUT, VAR_TEMP, VAR_EXTERNAL, and on any section of a FUNCTION (it keeps nothing between calls). VAR_GLOBAL is not built, qualified or not | not compared |
| CONSTANT with RETAIN | Annex A: `Loc_Var_Decls` and `Global_Var_Decls` take one of CONSTANT, RETAIN, NON_RETAIN, never two | refused, in either order. CONSTANT alone is not built | not compared |
| A variable of an unqualified section at a warm restart | §6.5.6.1 rule 3: implementer specific | initialized, as NON_RETAIN: `retain` is absent. A cold restart initializes everything (rule 4) | not compared |
| A function block instance in a qualified section | §6.5.6.2: every variable of the instance takes the qualifier, except (a) one the type declares RETAIN or NON_RETAIN itself and (b) one that is a function block instance, which follows its own type's declarations | as the clause. An array of instances is instances: each element follows its type (b). In an unqualified section, every member follows the type's declarations. A standard block (TON, CTU, …) declares none, so its members take the instance's, its internal state included: since L4 that state (a timer's START, RUNNING, CLOCK, an edge memory) is variables of the block (§2.14), each a symbol, and a warm restart keeps it as it keeps the rest. A RETAIN timer carries on from its ET (the next row); a RETAIN edge detector or counter does not see a level held across the restart as an edge | not compared |
| A timer after a warm restart | not addressed: Figure 15 draws one clock | the scan clock starts again from zero at every start, while a RETAIN timer keeps START, taken on the clock before. A clock below the one the timer saw at its previous call is a restart: START := now − ET, so it carries on from its ET, never negative and never past PT. A restart is not seen when the timer is first called after it at a clock already past its previous call's: ET then grows by the clock less that call's, no more than the time since the restart. The time a program was stopped is not timed: no scan clock runs then (`timers.st`; `a_retained_timer_carries_on_from_its_et_when_the_clock_starts_again`, and `runtime_api.rs` through the real module) | not compared |
| A change of PT while a timer runs | Table 46 note: implementer specific | ET is the time since START, limited by the PT of the call; Q is ET ≥ PT. As the native blocks did | not compared |
| R_EDGE inputs of the counters | Table 45 declares CU and CD `BOOL R_EDGE`; §6.6.3.2 rule 13: R_EDGE implicitly declares an R_TRIG in the block | the edge memory is a BOOL variable of the block (`CU_M`, `CD_M`), the same edge: as a variable of the block it takes the instance's retention, where an implicit R_TRIG instance would follow its own type (§6.5.6.2 (b)) and lose the edge at a warm restart. R_EDGE and F_EDGE in a program's declarations are not built | not compared |
| Typed counters | Table 45 features 1a–3e: CTU_INT, …_DINT, …_LINT, …_UDINT, …_ULINT; CTU is CTU_INT | INT, DINT and LINT, and CTU, CTD, CTUD as those of INT; PVmax and PVmin the limits of the type (Table 45 note). UDINT and ULINT are not built: there are no unsigned types | not compared |
| Typed timers | Table 46 features 1b–3c; its footnote a says "features 2b and 3b shall not be used" in textual languages, but the footnote's marker stands on 2d and 3d, the graphical symbols | TP_TIME, TON_TIME, TOF_TIME, the same as TP, TON, TOF: read either way they may be given (§6.6.3.5.1: the implementer may provide additional standard blocks). LTIME (1c, 2c, 3c) is not built | not compared |
| Long input names of the bistables | Table 43 features 1b, 2b: SR(SET1, RESET), RS(SET, RESET1) | not built: a block here has one name for each input | not compared |
| A name with two underscores together, or one at its end | §6.1.2: "__LIM_SW5 and LIM__SW5 are not valid identifiers"; nor is LIM_SW5_ | refused by the lexer where it is written, with its range. A name of an intrinsic begins with two underscores, so no program can write one (§2.14) | not compared |
| A POU or a type named as a block of the standard library | Annex A: the names of the standard function blocks are not keywords | refused, naming the word, as a standard function's is: every name the library declares, used by the program or not. A variable may bear one (`ton : TON`) | not compared |
| A structure in a qualified section | §6.5.6.2: every element, nested ones too, takes the qualifier | as the clause, also inside an instance | not compared |
| An array | not addressed: §6.5.6.2 speaks of instances and structures | one symbol, retained or not as a whole. An array of instances whose members would be retained in part (`VAR RETAIN a : ARRAY [1..2] OF B` where B holds a TON, or `VAR a : ARRAY [1..2] OF B` where B declares some members RETAIN and some not) is refused at its declaration, naming its path | not compared |
| An integer to and from STRING | Table 22 footnote e: the "external representation" of the value | `INT_TO_STRING(-5)` is `'-5'`; `STRING_TO_DINT` reads an integer literal: a sign and decimal digits, or `2#`, `8#`, `16#` digits, single underscores between digits. Nothing else (no space, no type prefix): `BadNumber`. A value its type cannot hold wraps, as narrowing does | stops at the first character that is not a digit: `'16#7F'` is 16 (deliberate difference) |
| Several connections into one input of LD | §8.2.3: the vertical link is the inclusive OR of the links on its left, copied to every link on its right | the OR, also into a block's input (a BOOL); so series is AND and parallel is OR (`series_is_and_and_parallel_is_or`) | not compared |
| Transition-sensing contacts and coils | Table 75 features 3–4 and Table 76 features 8–9 say a transition is "sensed"; Table 44 defines edge detection (R_TRIG, F_TRIG), and §6.6.3.2 rule 13 realises R_EDGE and F_EDGE with them | as R_TRIG (rising) and F_TRIG (falling), on a hidden BOOL memory of the POU starting FALSE (§2.15). A contact's CLK is its variable, sensed at every evaluation whether its input is ON or not; its output is its input AND Q. A coil's CLK is its input and Q is written to its variable. So a falling contact on a variable OFF at the first evaluation after a cold start is ON then (Table 44 note), and a rising one on a variable ON (`a_falling_contact_is_on_for_one_scan_after_its_variable_goes_off`; the ST twins of `ladder_equals_st.rs` use R_TRIG and F_TRIG) | not compared |
| The order of LD networks | §8.2.7: top to bottom as they appear, except as jumps change it; §8.1.4: a network is a maximal set of connected elements, the rails excluded | by the topmost element of each, then the leftmost; the order of the document does not matter (`networks_run_top_to_bottom_whatever_the_order_of_the_document`) | not compared |
| The order within an LD network | §8.1.5.1 rules a–c: no element before its inputs; every element evaluated; nothing more | the wiring first, then position (top, then left). A value more than one statement needs is evaluated once and kept: a branch point, a coil whose output goes on (the power reaching it, sensed before it writes), an edge's output (`power_through_a_coil_is_the_power_before_it_was_written`, `a_branch_point_is_evaluated_once_for_every_branch`) | not compared |
| EN and ENO of a block in LD | §6.6.1.5 rules 1–4; Example 2 (EN evaluated outside the POU) and Example 4 | outside: EN FALSE, the block is not called, its inputs are not set, ENO is FALSE; otherwise ENO is TRUE (an error stops the scan, so none sets it FALSE). Rule 4 leaves the outputs to the implementer: a function block's keep their values; a function's result that was not computed is not written to a variable, and as power flow it is OFF (`a_function_with_en_writes_nothing_when_it_does_not_run`) | not compared |
| Jumps and returns in LD | §8.1.6, Table 73; §8.1.5.1 rule c: a network is evaluated whole "even if the network contains one of the execution control elements" | after its network is evaluated whole; of several in one network, the first in evaluation order whose condition is TRUE; a jump goes on at the first network at or below its label, back as well as forward (a loop is bounded by the instruction budget, as WHILE is) | not compared |
| Feedback in LD | §8.1.5.2 rule a: explicit loops only in FBD; rule b: through a feedback variable | a cycle of connections refused, naming the elements on it; feedback through a variable taken (a seal-in) | not compared |
| Compare contacts | Table 75 features 5a, 5b (new in ed. 3) | not built: TC6 XML v2.01 has no element for them; a comparison is a block (GT, EQ, …) | not compared |
| The order of FBD networks | §8.3.3: "the Implementer shall provide Implementer specific means by which the user may determine the order of execution of networks"; §8.1.4 as for LD, without rails | as LD's: top to bottom by the topmost element of each network, then the leftmost; the user decides the order by where a network is drawn, as TIA Portal's FBD and LD do (`networks_run_top_to_bottom_whatever_the_order_of_the_document_or_its_execution_order_ids`) | not compared |
| TC6 `executionOrderId` | not in IEC 61131-3; the schema's optional attribute, which a tool's CFC numbering uses | not read in standard FBD: the order is the rule above, whatever the document numbers. A CFC mode that orders by it is a later slice (§3) | not compared |
| The order within an FBD network | §8.1.5.1 rules a–c: no element before its inputs; every element evaluated | an element once every element it reads by a line that is not a feedback path is evaluated; of those ready, the topmost, then the leftmost (`a_block_is_evaluated_after_what_it_reads_wherever_it_is_drawn`); every element is evaluated once, its outputs kept (§2.16) | not compared |
| Lines into one input in FBD | §8.3.2: outputs "shall not be connected together"; no wired-OR, an OR block instead | two or more lines into one input refused, naming the element and the input; a line and an expression at one input refused (`two_lines_into_one_input_are_refused_there_is_no_wired_or`) | not compared |
| Connectors and continuations | §8.1.3: a line extended by a connector; no storage associated with it; an error if its name is another named element's of the POU | a continuation goes on with its connector's line, so the two are one line and join their networks. Refused, naming the element: a name that is a variable's, the POU's or a label's; two connectors of one name; a continuation no connector names; connectors that only continue one another; a connector with no line or with an expression. Read in FBD; LD still refuses them | not compared |
| Feedback in FBD | §8.1.5.2: rule a, explicit loops only in FBD; rule b, an implementer specific means for the user to determine the order in an explicit loop; rule c, a feedback variable initialized, its initial value used at the first evaluation; rule d, the new value used until the element is evaluated again | rule b's means: a line that closes a loop and runs from right to left (the element it reaches stands at or left of the one it comes from) is a feedback path, so the user decides where a loop starts by where the line back is drawn. It carries its output's value of the last evaluation; at the first, the kept variable's initial value, its type's default (§6.5.1.3 item 1, rule c). Refused, naming the elements: a line back that does not run from a block's output to a block's input (§8.1.5.2's definition of a feedback path); one from an output whose type is known only once the loop is checked (ADD, SEL, MAX, …: write it to a variable, an implicit loop, rule b's example b); a loop with no line drawn back. An implicit loop through a variable is taken as in LD (`feedback_paths_equal_their_st_twin`: example a, a function block's output, a block's own output, example b) | not compared |
| EN, ENO and a function not run, in FBD | §6.6.1.5 rules 1–4 | as LD (the row above): EN evaluated outside; a function not run computes nothing, so what reads its result is not evaluated (an output variable keeps its value, a function is not called, a function block's input is not set); its ENO is kept and is the AND of its EN and the ENO of each function it reads | not compared |
| The type of a block's output in FBD | §6.6.2.5 (overloaded functions take the type of their inputs); Figure 11 | the type the checker gives the call by its inputs alone, with nothing asked of it from where the line goes. A literal input takes the type of the input beside it, as in ST; a function of literals alone takes a literal's own (DINT, LREAL), so ADD of two literals into an INT output variable is refused as a narrowing (an input variable naming a variable gives the type) | not compared |

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
profile carries a stack depth and a call depth (§2.2, §2.3). Closed in L4: an identifier with
two underscores together, or one at its end, found while reading §6.1.2, is refused (the row
above).

The differential test lists the first three rows, INSERT's position and the integer read
from a based literal as deliberate differences from STruC++ (287 of 7,500 values); a rounding
difference must be exactly one apart to count as one.

Not built in L2c (each refused by name): WSTRING, CHAR and WCHAR, and so a string's character
by subscript (`s[2]`, §6.4.2.2); an initial value on a TYPE; conversions between STRING and
BOOL, REAL, LREAL or TIME; STRING in SEL, MAX, MIN, LIMIT, MUX. Not built in L2c-2: the bit
strings (BYTE, WORD, DWORD, LWORD) and the unsigned integers, and so the shifts of Table 30
(SHL, SHR, ROL, ROR) and the bitwise functions of Table 31 on them; TO_* conversions without
the source type; typed literals (`INT#5`, `REAL#1.5`).

### 2.14 The standard library in Structured Text, and intrinsics (L4)

**Decided by Adrian, 2026-11-18**: "semua FB standar harus diimplementasi diatas ST. expose
semua hal yang perlu sebagai intrinsic. kedepan kita akan implementasi semua OSCAT sebagai FB
standar kita." Every standard function block is implemented on top of ST; whatever ST cannot
express is exposed as an intrinsic; OSCAT is to follow as the standard library.

- **The library is source.** `crates/mythos-logic-st/src/library/`: one `.st` file per block
  or per family (`bistables.st`, `edges.st`, `counters.st`, `timers.st`), each embedded with
  `include_str!`, and an index (`library::POUS`: each POU's name, kind and file), which a test
  holds equal to what the files declare. It lives in the ST front-end because it is ST; a later
  language front-end that needs it lowers it through this one, or takes the IR it makes.
- **What a program gets.** The front-end reads the program, then adds the POUs of the library
  it uses, and those they use, each once: a type named in a declaration, a function called in
  an expression (`library::add_used`). The rest is not compiled. A library POU is then an
  ordinary POU of the unit: TON is a function block type like a user's, its body emitted once,
  each instance's state in its own variables. The IR has no catalog of standard blocks any more
  (`IR_VERSION` 5), and the backend no native call.
- **The blocks** (IEC 61131-3 ed. 3 §6.6.3.5): SR, RS (Table 43); R_TRIG, F_TRIG (Table 44,
  as written); CTU, CTD, CTUD and their variants of INT, DINT and LINT (Table 45); TP, TON, TOF
  and TP_TIME, TON_TIME, TOF_TIME (Table 46, Figure 15). What is left out and why is in §2.13
  (no unsigned types, no LTIME, the long input names of the bistables, R_EDGE in a program).
- **Equal to the native blocks.** `standard_blocks_run_as_the_native_ones.rs` runs each native
  block in a format-3 program of one `FB` instruction beside the library's, with the same
  random sequences of inputs and clock (edges, held levels, IN pulses shorter than PT, PT
  changed while timing, zero and below zero, resets and loads while counting, counts at the
  limits of INT, a clock that does not move and one that jumps past PT): every input and output
  equal after every scan, 9 blocks × 400 sequences × 150 scans. There is no intended difference.
  The timers' rule for a clock that goes back (§2.13) acts only across a restart, which the
  native blocks never saw with their state kept.
- **Intrinsics.** What ST cannot say is an intrinsic, named with two underscores at its start:
  §6.1.2 forbids that in an identifier, so the lexer refuses it in a program, with its range,
  and takes it only in the library's text; the checker takes an intrinsic only in a library POU
  as well. One so far: `__NOW()`, a TIME, the scan clock: the time since the program was put in
  RUN, the same for the whole scan, the `now` the native timers were given. In the IR it is
  `ExprKind::Intrinsic(Intrinsic::Now)`; in the bytecode the instruction `NOW` of format 4
  (§2.3).
- **Names.** Every name the library declares is refused as the name of a program's POU or
  type, used or not, with the range of the name (`TON is a name of the standard; choose
  another`): a program never changes what a library name means. Inside the library, names are
  looked up among the library's alone, so nothing a program declares reaches in: a program may
  have a function named RUNNING or a type named START, the names of a timer's own variables. A
  variable may bear a library name (`ton : TON`), as of a standard function.
- **Places.** The lines of library file `k` are counted from `(k + 1) × 2^20`, which no
  program reaches. A diagnostic about a place in the library would be a fault of the compiler,
  and is made one about the whole program that says so and where (`the standard library does
  not compile (timers.st 12:5: …); this is a fault of the compiler, not of the program`); the
  program's text is never pointed into for it.
- **What changes for a host.** Each member of a standard block's instance, its internal state
  included (a timer's RUNNING, START, CLOCK; an edge memory M, CU_M, CD_M), is a symbol, as a
  user block's members are, and takes the instance's retention (§2.13). Watch reads `t.ET`
  and a binding takes `t.Q` by the same paths as before; an instance takes more memory (a TON
  35 bytes, not 28) and a call of a standard block is a call (one more frame of call depth).
  From a program, the internal state is private (§6.6.3.2 rule 11), as a user block's is.
- **OSCAT next.** OSCAT is to follow as the standard library. It is now an Eclipse project
  under EPL-2.0 (https://eclipse.dev/oscat/); its license must be checked before any of its
  source is copied in.

### 2.15 The Ladder Diagram front-end and its format (L3a)

**Decided by the owner (2026-11-18):** ST, LD, FBD and SFC, each a front-end crate lowering to
the shared IR, so that the backend, the VM, RETAIN, watching, forcing and online change work
unchanged for every language; where implementations differ, the standard.

- **The format: PLCopen TC6 XML v2.01**, the basis of IEC 61131-10. A Ladder Diagram is stored
  and exchanged as a TC6 `project` in the namespace `http://www.plcopen.org/xml/tc6_0201`. The
  schema, `tc6_xml_v201.xsd`, is not on plcopen.org's public pages any more (2026-11-18: every
  link to it and to its zip answers 404; PLCopen handed its rights in the format to IEC, now
  IEC 61131-10). It was read from the copy kept in the Beremiz repository
  (`https://raw.githubusercontent.com/beremiz/beremiz/master/plcopen/tc6_xml_v201.xsd`, sha256
  `591b92ba65018a77c32ab9e606abf27bd810cb1f7761d972a85689531c51e20f`, which declares PLCopen's
  target namespace) for its element and attribute names only. It is not committed: the file
  states no license. No code of Beremiz was read or copied; the reader is written from the
  schema's names and IEC 61131-3 ed. 3 §8.
- **The subset taken** (everything else is refused by its name, with the element it is in):
  - `project/types/pous/pou`: `name`, `pouType` (`program`, `functionBlock`, `function`).
    `types/dataTypes` and `instances/configurations` must be empty (a type is declared in the ST
    source, a task in the logic module); `actions` and `transitions` must be empty (SFC).
  - `interface`: `returnType`; `inputVars`, `outputVars`, `localVars` (VAR_INPUT, VAR_OUTPUT,
    VAR), with `retain` or `nonretain` where ST takes RETAIN and NON_RETAIN (§2.13), refused in a
    function; `tempVars`, `inOutVars`, `externalVars`, `globalVars`, `accessVars`, `constant`,
    `persistent` and a variable's `address` refused, as ST refuses VAR_TEMP, VAR_IN_OUT,
    VAR_EXTERNAL, VAR_GLOBAL, CONSTANT and AT. A variable's `type`: an elementary type by name,
    `string` with its `length`, `array` with its `dimension`s and `baseType`, `derived` (a
    structure, a named array, a function block type); `enum`, `struct`, the subranges,
    `pointer` and the generic types refused. Its `initialValue`: `simpleValue` and `arrayValue`
    (with `repetitionValue`); `structValue` refused.
  - One `body` holding `LD`: `leftPowerRail`, `rightPowerRail`, `contact` (`negated`, `edge`
    `rising` or `falling`), `coil` (`negated`, `storage` `set` or `reset`, `edge`), `block` (a
    function or a function block: the standard library's, the project's in LD or ST; EN and ENO;
    a `negated` input or output; an input's `expression` in place of a connection),
    `inVariable`, `outVariable`, `inOutVariable` (`negated`), `label`, `jump`, `return`,
    `comment`; `connectionPointIn` with `connection` (`refLocalId`, `formalParameter`) or
    `expression`; `position`. Refused: `connector` and `continuation`, `error`, a block's
    in-out parameters, `edge` or `storage` on a block's parameter or on a variable, and a body in
    IL, ST, FBD or SFC.
- **How it is lowered.** `mythos-logic-graphic` (`mythos-logic-ld` until L3c-1) reads the document with `quick-xml` (MIT) into a
  tree, the graph into one element per `localId`, checks the graph, and lowers each network to
  statements of the ST front-end's syntax tree, built element by element: never ST text (§2.1).
  The text inside elements (a contact's variable, a variable's expression, an initial value,
  `t.Q`, `Ts[i]`) is read by the ST lexer and parser, as ST reads it. The LD POUs are then
  checked by the ST checker together with the project's ST POUs and the standard library
  (`mythos_logic_st::lower_tree`): a declaration, a type, a call, an assignment to an input,
  RETAIN, a keyword as a name are one rule in both languages. Hence the layer of §2.1: LD
  depends on the IR and on the ST front-end, and on nothing downstream. The semantics are the
  rows of §2.13 for LD: power flow (a vertical link is the OR, so series is AND and parallel is
  OR), networks top to bottom, the order within one, edges as R_TRIG and F_TRIG, EN and ENO
  evaluated outside the block, jumps and returns after their network, no feedback by wire.
- **A graph that is not well formed is refused**, each naming the element by its `localId`:
  two elements with one `localId`; a connection to an id no element has (dangling); a
  connection to an element that gives no output, or to an output a block does not show; a
  contact, coil, output variable, jump or return with no input; a cycle of connections, naming
  the elements on it; a jump to a label the body does not have, two labels of one name; a
  function block called without an instance, an instance of another type, one instance called
  twice in a network; an edge in a function (it keeps nothing between calls, §6.6.2.1).
- **Hidden variables.** What LD needs that the program did not declare is a variable of the
  POU whose name begins with two underscores, which §6.1.2 forbids in an identifier, so no
  name of the program meets it: `__EDGE_<localId>` (BOOL), the memory of an edge contact or
  coil; `__LD_<localId>` and `__LD_<localId>_<pin>`, the power of an element's output or of a
  junction (BOOL, below), or a value more than one statement needs (the declared type of a
  function of the project, of a conversion, or of the variable read); `__NET_<localId>`
  (BOOL), whether a network ran (below); `__JUMP` (DINT), when the body jumps or returns. Each is a symbol like any
  variable, so RETAIN, watching and online change take it: it is unqualified, so not retained
  in a program and retained in an instance declared RETAIN (§6.5.6.2); online change carries it
  by its path, which names the element, so an edit that keeps an element's `localId` keeps its
  memory. Each is **private**: the IR marks it `Var::hidden` (`IR_VERSION` 6) and the backend
  gives its symbol `"private": true`, so no tag is bound to write it; it may be read, watched
  and forced, as any private variable.
- **Places** (§2.8). A TC6 document's lines say little about a diagram, so a diagnostic about
  an LD POU carries `element`: `{"pou", "local_id", "x", "y", "variable", "text": {"field",
  "col", "end_col"}}`, the element's `localId` and position as the document gives them, or the
  variable of the interface, and the characters of the text the message is about (`variable`,
  `expression`, `instanceName`, `typeName`, `initialValue`, a block's input by its formal
  parameter). Its `line` and `col` are where the element's tag is in the document. A document
  that is not well-formed XML is told at its line and column, and nothing that follows from it
  is told. Inside the front-end each place is a line of the syntax tree from `2^31`, past any
  source and any file of the library, mapped back after checking.
- **LD and ST in one program.** `mythos_logic_compiler::compile_sources(&Sources { st, ld },
  target)` compiles at most one ST source (types, functions, function blocks) with any number
  of TC6 documents into one program: one POU among all of them is the PROGRAM; each POU may
  call those of the others and of the standard library; the result is the same `Compiled`
  (bytecode, symbols, types) and every check of the target profile applies.
  `compile_in(document, Language::Ld, target)` compiles a document alone. With more than one
  source, each diagnostic names its `source`.
- **Not built, and why:** a value a function gives more than one input when its type is not
  known before checking (ADD, SEL, MOVE, …: keeping it needs a variable of that type) and an
  input variable read by more than one input that is not a variable by its name: each refused
  with what to write instead. Connectors, compare contacts (§2.13), in-out parameters, edges on
  a block's parameters, types declared in a document.
- **Power for monitoring** (L3b-2a). **Decided by the owner (2026-11-18):** for online
  monitoring as TIA Portal gives it, the compiler always stores each graphical element's output
  in a hidden private variable at the moment it is evaluated, in every build: there is no
  build for monitoring. A flag per network says whether it ran in the scan, so that a network
  a jump skipped shows as not executed. The cost is accepted: about one byte and one store per
  element. FBD does the same since L3c-1 (§2.16); SFC is to in its own slice. (L3b-1 published only the
  outputs some variable happened to hold, none in a body with a jump or a return; this
  replaces it.)
  - **What is kept**: the output of every contact, coil and BOOL variable read (`inVariable`,
    `inOutVariable`) in `__LD_<localId>`; of a block, a function's BOOL result in
    `__LD_<localId>`, a function block's BOOL output in `__LD_<localId>_<formal>`, ENO in
    `__LD_<localId>_ENO`; every junction, where several links join an input (a vertical link,
    §8.2.3), in `__LD_<localId>_IN`, or `__LD_<localId>_<formal>` at a block's input; each
    network in `__NET_<localId>` of its first element in the order of evaluation. ON (the left
    rail, and what it reaches through nothing that can switch it off) is kept nowhere. An
    output that is not a BOOL (a timer's ET, an ADD's result), or a function's whose type is
    not known before checking (SEL, MAX, …), carries no power and keeps none: a BOOL output of
    a function block is known from its declaration, the project's or the standard library's.
  - **When**: where the lowering evaluates the value, so storing it changes nothing the
    program does. At once where it kept a value already (a branch point, a coil, an edge);
    otherwise by a statement of its own just before the first statement that reads the value,
    where the expression it stands for was evaluated before; only hidden variables are written
    between. An output nothing reads is stored when its network ends. A power read under an
    IF (a block's input under its EN) is stored before the IF, so it is the element's power
    whether or not the block ran. A function's result is stored only where the function runs,
    and FALSE where it does not (its EN, or the EN of a function before it, was FALSE): the
    function is never called for it there. Two things change, both as §8.1.5.1 rule c has it
    (every element is evaluated): an element that feeds nothing but the right rail, or feeds a
    block whose EN is FALSE, is now evaluated, so what it reads is read (an index out of range
    there faults where before it was never read); and a function whose BOOL result nothing
    takes is now called, under its own EN. Equal to their ST twins as before
    (`ladder_equals_st.rs`, unchanged: 1,020,000 values).
  - **Networks**: each sets its flag TRUE as it starts. In a body with a jump or a return,
    every flag is set FALSE first in the scan, so a network a jump or a return skipped reads
    FALSE, and one run again by a backward jump reads TRUE. This is exact for the PROGRAM,
    which runs every scan; an instance of an LD function block not called in a scan keeps its
    flags and power from the call before, and is not drawn live (§2.8).
  - Each is a symbol marked private (`Var::hidden`, `"private": true`): watching, RETAIN
    (unqualified: not retained in a program) and online change (carried by its path, which
    names the element) take it as any hidden variable.
  - `lower_with_probes` (and so `Compiled::probes`) names, for every output (`At::Output`),
    junction (`At::Input`) and network (`At::Network`, its elements' `localId`s in order), the
    variable keeping it, or ON. The logic module gives the PROGRAM's in `check` as `power`:
    `{local_id, output}`, `{local_id, input}` or `{local_id, network}`, each with `variable` and
    `negated`, or `on: true` (§2.6).
  - **Depth.** A series of contacts or of functions is a chain of kept values as long as the
    series. Each is worked out one element at a time, the first first, in the order the
    lowering reaches them through their inputs, so the series takes no stack for its length:
    300 AND blocks in series compile and run (`a_long_series_of_functions_compiles_and_runs`),
    where `dev` before this slice overflowed the stack of a test's thread (2 MiB, a debug build)
    at 200 (and passed 150).
  - **Cost, measured** (code and memory in bytes, `pc` profile; the oscillator `rp2040`; before
    is `dev` at `eccfbb5`; kept is the number of `__LD_`/`__NET_` variables after):

    | Program | Code before | Code after | Memory before | Memory after | Kept |
    | :--- | ---: | ---: | ---: | ---: | ---: |
    | contacts and coils (`ladder_equals_st.rs`) | 344 | 774 | 24 | 67 | 48 |
    | TON and CTU | 497 | 607 | 61 | 72 | 12 |
    | functions | 136 | 206 | 18 | 25 | 8 |
    | jumps and a return | 452 | 662 | 13 | 27 | 14 |
    | oscillator (`tests/ladder/oscillator.xml`) | 831 | 1,091 | 94 | 113 | 21 |

    Each new kept BOOL is one byte of memory (the growth in memory is the number of new ones),
    and its store ten to fifteen bytes of code (an expression or a load, and the store; a body
    with a jump also clears each flag). The VM is unchanged, so its firmware is too: ESP32-C3 39,318 bytes of code,
    RP2040 32,308 (`scripts/logic-vm-footprint.sh`, rustc 1.100.0, in the CI runner's image).
    The oscillator, now with its stores, runs on both emulated instruction sets with every scan
    equal to the host's (all 18 runs equal, QEMU 8.2.2).
- **What the editor slice (L3b-2) needs.** Editing in the workbench: placing, moving, wiring
  and deleting elements, and the text of each, writing TC6 back through `save_logic_source`.
  It must keep the `localId` of every element it does not change (online change carries
  `__LD_<localId>` and `__EDGE_<localId>` by their path, §2.5), give a new element an id no
  element of the document has had in this edit, and keep what it does not draw (the file
  header, `addData`, `documentation`) as it found it. A document the editor writes must be read
  by this front-end alone, as now; nothing of another tool's is copied. The view of L3b-1 draws
  from positions, so the editor needs a grid and a rule for where a new element goes, and the
  owner's design pass before both. Undo is the editor's own; the module keeps one version.
  Power for monitoring is the compiler's (above): the editor writes no variable for it.

### 2.16 The Function Block Diagram front-end (L3c-1)

**Decided by the owner (2026-11-18):** standard FBD first (a CFC mode, with free placement,
explicit execution order and feedback paths, is a medium-term item, §3); online monitoring as TIA
Portal gives it, so the compiler always stores each element's output (every block output, ENO
and every line's value) in a hidden private variable at the moment it is evaluated, in every
build, and a flag per network; the editor follows TIA's workflow and is the next slice (L3c-2);
where implementations differ, the standard.

- **One crate for both graphical languages.** `mythos-logic-ld` is renamed
  `mythos-logic-graphic` and reads LD and FBD; `layers.rs` gives it the layer LD had (the IR and
  the ST front-end), and still refuses a new `mythos-logic-*` crate without a layer. *Why one
  crate and not `mythos-logic-fbd` beside it:* a TC6 document is one `project` whose POUs each
  have a body in a language of their own, and a POU of one may call a POU of the other in the
  same document (`fbd_pous_are_called_from_fbd_and_from_ld_and_ld_from_fbd`): reading the
  interfaces of every POU before any body, the places of diagnostics, what a body may call
  (`Known`) and the order of networks, labels, jumps and returns (§8.1.4, §8.1.6) are one
  pass over one document. A separate FBD crate would have to depend on the LD crate and have LD
  call back into it for a document that holds both. Inside it: `graph.rs` (the elements of
  both, their reading, the checks of a whole graph, and `body`, which arranges networks, labels,
  jumps, returns and flags for either language), `ld.rs` (the LD lowering, unchanged in what it
  does: `ladder_equals_st.rs` still holds 1,020,000 values equal), `fbd.rs` (the FBD lowering),
  `read.rs`, `xml.rs`. The compiler's `Sources` names its TC6 documents `tc6` (was `ld`), and
  `Language::Fbd` compiles a document as `Language::Ld` does: the body says its language.
- **The subset taken.** A body `FBD` with: `block` (a function or a function block, the standard
  library's or the project's in LD, FBD or ST; EN and ENO; a negated input or output; an input's
  `expression` in place of a line), `inVariable`, `outVariable` (each `negated`),
  `inOutVariable` (`negatedIn`, `negatedOut`), `connector` and `continuation` (§8.1.3), `label`,
  `jump`, `return`, `comment`, connections by `refLocalId` (and `formalParameter`). Refused by
  name with the element: `leftPowerRail`, `rightPowerRail`, `contact` and `coil` (LD's), and
  anything else of the schema (`actionBlock`, `vendorElement`, …), as in LD; a block's in-out
  parameters; `edge` or `storage` on a block's parameter or on a variable. `executionOrderId` is
  not read (§2.13).
- **The semantics** are the rows of §2.13 for FBD: networks top to bottom, data flow within one,
  no wired-OR, connectors, feedback paths drawn back from right to left, EN and ENO as in LD,
  jumps and returns after their network.
- **Values kept for monitoring.** Every output an element gives is evaluated once, when the
  element is, into a hidden private variable of the POU, and every element that reads it reads
  the variable: so the value a line carries is the value kept, by construction, not a second
  computation. `__FBD_<localId>`: a function's result, an input variable read, an in-out
  variable read back; `__FBD_<localId>_<formal>`: a function block's output, read from the
  instance after its call; `__FBD_<localId>_ENO`: ENO, kept wherever the block may not run (its
  EN is connected, or it reads a function that may not run), shown or not; `__NET_<localId>`:
  the network's flag, as in LD; `__JUMP`. A negated output keeps its negation (the line carries
  it). A literal input variable is a constant and keeps nothing: its line carries the literal
  written on the element. An ENO that is always TRUE is published as ON and kept nowhere.
- **A number is kept in its own type.** The variable of a block's output has the type the
  checker gives the call: the ST front-end takes a declaration *typed by its value*
  (`ast::TypeSpecKind::OfValue`, only for a hidden variable, never from ST text), whose type is
  the type of the first value assigned to it, typed with nothing asked of it from outside. So
  ADD of two INTs keeps an INT, of two REALs a REAL, INT_TO_REAL a REAL, a timer's ET a TIME, by
  the one rule of typing there is, not a second one in the FBD front-end. A read of such a
  variable before its first assignment would be a fault of the compiler and is said to be one.
  A feedback path's source is read before it is assigned in the scan, so it is declared with its
  type, which must be known before checking (a function block output's declaration, a
  comparison, a Boolean operator, a conversion, a function of the project); otherwise the loop is
  refused (§2.13). *How a number is shown:* written at the output it comes from, as the
  variables table writes it (an INT `7`, a REAL to seven digits, a TIME in milliseconds, a
  STRING quoted), coloured as a value; a BOOL as TRUE or FALSE, its lines on or off.
- **Where a value was not computed.** A function that may not run keeps its result only where
  it runs, and its probe says under which BOOL (`Holds::Guarded { path, ran }`, `"ran"` in the
  check's JSON): where `ran` is FALSE the variable holds an earlier scan's value, which the view
  draws as *not computed*, never as a value.
- **Probes.** `lower_with_probes` (and so `Compiled::probes`) names, for every output of every
  element (`At::Output`: a block's formal parameter, `""` for a variable's or a continuation's)
  and every network (`At::Network`, its elements' `localId`s in order), the variable keeping it,
  or ON. A continuation is published with what its line comes from. The logic module gives the
  PROGRAM's in `check` as `values` (§2.6), in the shape of LD's `power`.
- **Places.** As LD's (§2.15): a diagnostic names the element, its position and the characters
  of its text. What a line brings is told at the input it reaches (an INT into a BOOL output
  variable is told at the output variable).
- **Cost, measured** (bytes, `pc` profile; kept is the number of `__FBD_`/`__NET_` variables;
  each kept value is its type's size in memory and one store of ten to fifteen bytes of code, as
  in LD; there is no FBD build without them to compare with, since FBD is new):

  | Program | Code | Memory | Kept |
  | :--- | ---: | ---: | ---: |
  | functions (`fbd_equals_st.rs`) | 409 | 57 | 18 |
  | function blocks, EN and ENO | 945 | 109 | 24 |
  | lines, jumps and a return | 755 | 44 | 20 |
  | feedback paths | 449 | 43 | 21 |
  | FBD and LD POUs called from FBD | 373 | 41 | 8 |
  | blinker (`tests/fbd/blinker.xml`) | 1,134 | 129 | 25 |

  The VM is unchanged, so its firmware is too. The blinker runs on both emulated instruction
  sets with every scan equal to the host's (all 26 runs of the corpus equal, rustc 1.100.0,
  QEMU 8.2.2, in the CI runner's image).
- **What the FBD editor (L3c-2) needs.** Placing, moving, wiring and deleting blocks,
  variables, connectors and continuations, labels, jumps and returns, as TIA's FBD editor does
  (networks with titles and comments, the instruction palette and favourites, symbolic operands
  with completion), writing TC6 back through `save_logic_source` and keeping every `localId` it
  does not change (online change carries `__FBD_<localId>` by its path) and what it does not
  draw, as LD's editor does (§2.15). It must never write a second line into one input (§8.3.2),
  and should offer an OR block where TIA would; it decides where a line back is drawn, which is
  the order of a loop (§2.13), so it should show a feedback path as such. A box's output type is
  the checker's: the editor shows the compiler's, not its own. A grid and rules for where a new
  element goes, and the owner's design pass, before both. `executionOrderId` stays the CFC
  mode's (§3).

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
  project's programs publish are offered wherever a binding names a tag), force (done: the API
  and the protocol, §2.5; the editor, the Runtimes page and the FORCED mark wherever a tag's
  value is shown, §2.6).
- C6. **Swap and online change; programs in a device's slice.** C6a (done): a device's slice
  carries its programs (`logic/`, `runtimes.json`) like the rest of its project's files, and the
  node checks the slice before installing it: a program whose runtime module it lacks is
  refused, naming it, and nothing changes (ADR-012 §2.5). C6b, online change (done, §2.5): a
  new build of a running program takes over its variables and scans on without a cold start; a
  cold load on request; the load says how each runtime started, and so does the editor.
- C7. **Hardening**: watchdog (done, §5), RETAIN variables across restarts (done: the compiler
  reads RETAIN and NON_RETAIN and each symbol says whether it is retained, §2.13, C7b-1; the
  runtime and the core keep them across a warm restart, §2.5, C7b-2).

**Track L, the language, beside it:**

- L1. **The VM's real model** (done): functions, user function blocks, arrays and structures;
  static allocation, no recursion, instance-relative addressing, a bounded call stack;
  format 1.
- L2. In three parts. **L2a** (done): a parser that recovers from errors and gives ranges, for
  the editor (§2.8); a keyword is refused as a name (§2.13). **L2b** (done): the limits the VM
  and the profiles enforce: the stack stops at the declared depth, profiles carry a stack
  depth and a call depth, a jump into the middle of an instruction is refused at load. **L2c**, in two:
  **L2c-1** (done): STRING, its literals, assignment, comparison, the functions of Table 34
  and integer conversions; bytecode format 2 (§2.3, §2.13). **L2c-2** (done): the numeric
  functions of Table 28, EXPT and `**`, TRUNC, the selection of Table 32, the operators by
  name; the VM's own real functions; bytecode format 3. Not built: the bit strings and their
  shifts (§2.13).
- L3. **LD, FBD, SFC** (not IL, §2.1), each a front-end crate and an editor kind. **L3a**
  (done, §2.15): the Ladder Diagram front-end, `mythos-logic-ld` (now `mythos-logic-graphic`), reading PLCopen TC6 XML
  v2.01; an LD program compiled with the project's ST POUs and the standard library.
  **L3b-1** (done): the logic module stores, checks and builds LD programs, each an MCP tool,
  and the workbench draws an LD program read-only with its live state (§2.6, §2.8, §2.15).
  **L3b-2a** (done): every element's output and every junction kept for monitoring in every
  build, a flag per network, and the view drawing each element and link with its power and a
  rung not run as not executed (§2.8, §2.15, power for monitoring).
  **L3b-2b** (done): an engineer edits a Ladder Diagram in the workbench, as TIA's editor works
  (the owner's decision of 2026-11-18), and the editor writes TC6 back keeping every localId,
  its header and what it does not draw (§2.8, §2.15, what the editor writes).
  **L3c-1** (done, §2.16): Function Block Diagram programs: the front-end, in the one graphical
  crate with LD and the same TC6 reader (FBD's order of networks, §8.3.3, data flow within one,
  no wired-OR, connectors, explicit feedback by a line drawn back, §8.1.5.2), every output's
  value and every network's flag kept in every build, a number in its own type; the logic
  module stores, checks and builds them (`language: "fbd"`); the workbench draws them
  read-only with their live values (§2.6, §2.8, §2.13). **L3c-2** (next): the FBD editor, as
  TIA's works (§2.16, what the FBD editor needs). Then SFC.
- L4. **The standard library in ST** (done, §2.14): the standard function blocks written in
  Structured Text, with intrinsics for what ST cannot say (the scan clock, bytecode format 4);
  their state retained like any variable's. Next: OSCAT, once its license is checked.

**Later:** ESP32 and RP2040 firmware as runtime modules on real boards; a C backend; a Linux
real-time mode. Medium term (owner, 2026-11-18): a CFC mode for the FBD editor (free placement,
explicit execution order, feedback), stored as FBD in TC6 XML, after standard FBD; see
`specs/tasks.md`, "Long-term horizons".

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
| 2.1 Crates and layers | `mythos-logic-ir`, `-st`, `-graphic` (LD since L3a, FBD since L3c-1; `-ld` until then), `-vm`, `-backend-vm`, `-compiler`; `layers.rs` guards them |
| 2.1 ST front-end | Several POUs in any order: TYPE (STRUCT, named ARRAY), FUNCTION (VAR_INPUT, VAR; positional and formal calls), FUNCTION_BLOCK (VAR_INPUT, VAR_OUTPUT, VAR; nesting; arrays of instances), one PROGRAM; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME, STRING (`STRING[n]`, `STRING(n)`, 254 by default; literals with `$` combinations; assignment that cuts; comparison; LEN, LEFT, RIGHT, MID, CONCAT, INSERT, DELETE, REPLACE, FIND; integer to and from STRING; in functions, function blocks, arrays, structures and a TYPE name); ABS, SQRT, LN, LOG, EXP, SIN, COS, TAN, ASIN, ACOS, ATAN, ATAN2, EXPT and `**`, TRUNC_*, SEL, MAX, MIN, LIMIT, MUX, and ADD, SUB, MUL, DIV, MOVE, GT, GE, EQ, LE, LT, NE by name; arrays of any dimensions with initial value lists; whole-structure and whole-array assignment; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between the elementary types; implicit widening per Figure 11; recursion refused naming the cycle; a keyword of the standard refused as a name; RETAIN and NON_RETAIN on VAR, VAR_INPUT and VAR_OUTPUT of a PROGRAM and a FUNCTION_BLOCK, each symbol saying whether it is retained (`Symbol::retain`, `"retain": true` or absent; C7b-1, §2.13; `retentive_variables.rs`), refused where nothing can be retained; kept across a warm restart by the runtime and the core (§2.5, C7b-2); the standard function blocks from a library written in ST, only those a program uses (§2.14, L4); a name with two underscores together or one at its end refused (§6.1.2); diagnostics for everything else (enumerations, subranges, WSTRING, CHAR, VAR_IN_OUT, methods, and more: each refused by name); every independent mistake of a source in one pass, each once, with the range it is about (§2.8); a body or a type nested more than 100 deep refused, a structure that contains itself told before anything follows it (§2.8) |
| 2.2 Profiles | `serde` data with a format version (4 since L4; a profile of another is refused, naming both); `pc`, `rp2040`, `esp32` stand in; refusals (type, memory, format, stack depth, call depth) proven with profiles read from JSON; a profile written before the depths existed still loads and states no limit |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; format 4 (format 1 with strings and the numeric functions, L2c, with its own real functions, §2.3, and the scan clock `NOW`, L4), and formats 1, 2 and 3 read as they were written, their native standard blocks included (a VM reads every format it extends; an instruction newer than a program's format refused at load): bodies, call frames, instance-relative and region-confined addressing; loader checks, with every jump landing on an instruction of its body, and `load_within` a host's limits; run-time checks, budget; the stack and the calls stop at the depths the program declares; faults, with `IndexOutOfRange`, `CallDepthExceeded`, `StackOverflow`, for strings `StringTooLong`, `StringArgument`, `BadString`, `BadNumber`, and `SelectorOutOfRange` |
| 2.4 Writes to field devices (C1) | Built: `IoStatusHandle::write_tag`, `PUT /api/v1/tags/<uri>/value` (operator), MCP `write_tag`; a driver declares each tag writable or not and the core refuses a read-only one without asking it; a write runs in its connection's task between polls, bounded by the poll's deadline; the answer is `written: true`, `false` or `"unknown"`. Proven by `field_write_tests.rs` (a real Modbus simulator) and through the Modbus module process in `modbus_module.rs`. Ownership of an output by a runtime (write authority) comes with the `runtime` kind |
| 2.4 The `runtime` kind (C2) | Built: `ModuleKind::Runtime`, `Call::Exchange`, `Call::Mode`, `RuntimeProgram` (`mythos-module`); `plugins/runtime-vm` (`mythos-module-runtime-vm`: the VM, a scan thread, faults, cold start); `crate::runtimes` (`RuntimeDriver`, ownership, safe values, `BusTags`). Proven by `runtime_vm.rs` (ST compiled, run in the real module process, reading one Modbus simulator and commanding another: follows the plant; an owned output refuses an operator; a setpoint is taken; stop, fault, a killed process and removal each leave the safe values; a tag that is not GOOD feeds no value) and `runtime_api.rs`. A program that publishes no tag, or is bound to nothing, runs all the same (found by the Runtimes page's end-to-end run: it was saved, reported as applied, and never started). Reading variables is built (`Call::Watch`, C5): `runtime_vm.rs` reads locals, array elements, a timer's members and a REAL from the running module process, in RUN and in STOP, and a module without `watch` says so and runs on. Forcing is built (`Call::Force`, `Call::Unforce`, C5c): `runtime_api.rs` forces an output, a setpoint and an input of the running module process over the API and watches the device follow, refuses an operator and a viewer, refuses what cannot be forced, and sees every force released by a stop and by loading the program again. The watchdog is built (C7a): a program in RUN that completes no scan within ten intervals (never less than two seconds), or whose scan thread has ended, is put in FAULT at the next exchange with the reason (`watchdog: …`), every force released, so the core writes the safe values; a scan itself is bounded by the instruction budget. Proven by the module's own tests: a healthy program runs past the limit untouched, and one whose scan thread ends is faulted and asks for no write. RETAIN is built in the runtime and the core (C7b, §2.5): `runtime_api.rs` runs a program with a RETAIN counter and proves that it goes on after a stop and run, after the program is loaded again, after its module process is killed and started again (from the values kept last, not those it was loaded with), and after a load in STOP, while the other counter starts from zero each time, and that a cold start resets it; the module's tests prove that a value is taken only into a variable of the same name, type and size. Since L4 a standard block's state is variables of its own: `runtime_api.rs` runs a RETAIN TON beside a plain one through the real module process, and after a stop and run the RETAIN one carries on from its ET (the time stopped not timed) while the plain one times again from 0, and a cold start resets both. Online change is built (C6b, §2.5): `runtime_api.rs` runs a program with a RETAIN counter, a plain counter, a TON and an output bound to a Modbus simulator, loads a build with one variable added and one removed while it runs, and proves that both counters and the timer go on without resetting, the added variable starts from its initial value, the removed one is reported as dropped, and the device's register, sampled about every millisecond through the load, never holds the safe value nor drops below what the old build commanded before the load; that a cold load resets both counters and what is kept; that a program of another name starts as before; and that an unknown `start` is refused. The module's tests prove the carry rule (same path, type and size; an array of structures or instances not carried; a value given twice taken once; what is not carried listed with why), and that a program started into RUN holds the carried values and clock before its first scan while one loaded in STOP takes none |
| 2.5 Managing runtimes | Built in the API: `GET /api/v1/runtimes`, `POST /api/v1/runtimes/:runtime/run` (a warm restart), `/cold-start`, `/stop`, `/watch`, `/force` and `/release`, `GET`/`PUT /api/v1/projects/:id/runtimes` (`?start=cold`, and `changes.runtimes` saying how each runtime started: C6b), each an MCP tool; engineer for all but reading. Built in the workbench (C4a): Settings → Runtimes (`RuntimesSettings.tsx`) reads the list about once a second and shows each runtime's module, program and version, its mode as reported with the reason given, its scan figures and its outputs (safe value, what the device last confirmed, whether all are confirmed safe); run and stop for an engineer; what is forced in each, a FORCED badge with each variable and its value (`forced UNKNOWN` when the controller does not say), and "Release all" for an engineer, asked first, with the controller's answer shown as it gave it; the runtime modules with the profile each declares. What the server did not send is `UNKNOWN` or `NOT MEASURED` (the scan figures of a runtime whose module does not answer among them: the server sends zeros there), and a read that fails or is not answered marks the rows as the last known state with their age. Proven by `runtimes.test.tsx` and, against a real server and runtime module, by `scripts/e2e.sh`. Not built: restart as a command of its own; loading a program from the page; the same for paired nodes in Fleet; data types shared in the core |
| 2.6 The logic module | Built: `plugins/logic` (programs in ST, LD or FBD, bindings, check, build; eight MCP tools, each saying which languages it takes; module version 0.4.0, its UI's the same), proven by `logic_module.rs` through the real logic, runtime and Modbus module processes, an LD program among them (created as a TC6 document, a diagnostic at its element, built, loaded, and running a Modbus simulator; an ST program saved without a language beside it, unchanged); its workbench UI `frontend/packages/logic-ui` (tree section with each program's language, program editor, the LD editor for an engineer and the read-only LD view for everyone else, both with the live state, load, load cold, how a load started, run, stop, unload), proven by its vitest suite against the workbench's real host and by `scripts/e2e.sh`, where a program typed into the editor is checked, bound, loaded and its result read live from the tag bus, and an LD program added from the tree, given rungs over the API, checked with a mistake drawn at its element, bound, loaded and run shows its contacts change state with the device's counter, and an LD program built in the editor from the keyboard and the palette is checked, saved, loaded and run, its motor coil following a start and a stop forced as a pushbutton would set them (L3b-2b). Not built: unit tests in ST, a library of POUs shared by a project's programs |
| 2.8, 2.10 | 2.8: the ST editor (2.6), with what the front-end gives it (2.1); the read-only LD view with each diagnostic at its element and its live state from watched values (L3b-1, `ladder.test.tsx`), each element, link and junction with the power the compiler kept and a rung not run as not executed (L3b-2a); the LD editor, as TIA's works (L3b-2b, `ladder-editor.test.tsx`, `ldmodel.test.ts`); the read-only FBD view with each diagnostic at its element and every line's value live, a function not run as not computed and a network not run as not executed (L3c-1, `fbd.test.tsx`); editors for FBD (L3c-2) and SFC are not built. 2.10: not built |
| 2.9 Emulators | Built: eight programs (`strings.st` since L2c-1, `library.st` since L2c-2), 60 scans each, on both emulated instruction sets, every scan equal to the host's (CI job `logic-vm-bare-metal`). A bug put into REAL addition for Arm only made the two programs that add REALs fail there and nowhere else. Since L4 the timers and counters in them are the library's, in ST: all 16 runs equal Since L3a a Ladder Diagram joins them (`tests/ladder/oscillator.xml`: two TON, a CTU, rising and falling contacts, a set coil, a jump): all 18 runs equal (rustc 1.100.0, QEMU 8.2.2, run in the CI runner's image). Since L3c-1 the ladder documents of the editor (L3b-2b) and a Function Block Diagram (`tests/fbd/blinker.xml`: two TON, a CTU reset by its own output, a block's output drawn back, a connector, a jump, a function under EN, a REAL) join them: all 26 runs equal (rustc 1.100.0, QEMU 8.2.2, in the CI runner's image with podman) |
| 2.12 Oracle | `differential_strucpp.rs`: 8 programs, 60 cycles, 9,480 values, all agree except the five deliberate rows of §2.13 (287 values: 167 of the first three, 60 each of INSERT's position and a based literal read as an integer). It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE. Unchanged by L4 (the standard blocks in ST): the same 9,480 values, the same 287 |
| 2.14 Standard library in ST (L4) | `crates/mythos-logic-st/src/library/`: SR, RS, R_TRIG, F_TRIG, CTU, CTD, CTUD and their INT, DINT and LINT variants, TP, TON, TOF and their TIME variants (22 POUs in 4 files, an index); the intrinsic `__NOW()`; names of the library refused to a program's POUs and types; the library's names resolved among its own; no diagnostic pointing into it. Proven by `standard_library.rs` (14) and `standard_blocks_run_as_the_native_ones.rs` (4: every native block against the library's, 400 sequences of 150 scans each, 540,000 scans in all) |
| 2.15 Ladder Diagram (L3a) | `crates/mythos-logic-ld` (`crates/mythos-logic-graphic` since L3c-1, §2.16): PLCopen TC6 XML v2.01 read with `quick-xml` (MIT); the subset of §2.15; the graph checked and lowered to the ST front-end's syntax tree; edges, values used twice and jumps in hidden private variables (`Var::hidden`, `IR_VERSION` 6); diagnostics naming the element (`element`) and the source (`source`); `compile_sources` and `Language::Ld` in the compiler; `layers.rs` gives LD its layer (the IR and the ST front-end) and feeds the guard an LD crate reaching the VM and an ST crate reaching LD. Proven by `ladder_runs_on_the_vm.rs` (26: every element, its table's behaviour scan by scan, EN and ENO, a function of ST and the standard functions as blocks, LD function blocks and functions, jumps both ways, a return, RETAIN and the edge memory of a RETAIN instance, 300 contacts in series and in parallel, the emulator's program), `refusals.rs` (28: every refusal of §2.15 with its message and its element, the JSON of a place, the sources named), `ladder_equals_st.rs` (4 LD programs and their ST twins, 200 random sequences of 100 scans each, every shared variable equal after every scan: 51 variables, 1,020,000 values compared) and `damaged_ladder_never_panics.rs` (3,000 damaged documents, each compiled or refused with a place). Mutation-checked in a copy of the tree (`git archive`, its own target directory; the clean copy passed before and after the restores): parallel branches made AND turned `series_is_and_and_parallel_is_or`, `a_long_series_and_a_wide_parallel_compile_and_run`, `an_ld_function_block_and_an_ld_function_are_called_from_an_ld_program` and `contacts_and_coils_equal_their_st_twin` red; a negated contact's negation dropped turned `a_normally_closed_contact_passes_power_when_its_variable_is_off`, `an_ld_function_block_and_an_ld_function_are_called_from_an_ld_program`, `the_oscillator_the_emulators_run_does_what_its_rungs_say` and `contacts_and_coils_equal_their_st_twin` red; an edge contact never writing its memory turned `a_rising_contact_is_on_for_one_scan_after_its_variable_goes_on`, `a_rising_contact_senses_its_variable_while_its_input_is_off`, `a_falling_contact_is_on_for_one_scan_after_its_variable_goes_off` and `contacts_and_coils_equal_their_st_twin` red; a dangling connection let through turned `a_dangling_connection_is_refused_naming_its_element_and_the_missing_id` and `a_damaged_ladder_document_is_compiled_or_refused_with_places_never_a_panic` red. Since L3b-1 the logic module stores, checks and builds LD programs (2.6). Since L3b-2a every element's output, every junction and every network's flag is kept in a hidden private variable in every build (`Compiled::probes`, §2.15, power for monitoring), proven by `ladder_power_probes.rs` (9) and `a_long_series_of_functions_compiles_and_runs`. Not built: editing LD in the workbench (L3b-2b) |
| 2.16 Function Block Diagram (L3c-1) | `crates/mythos-logic-graphic`: `fbd.rs`, with `graph.rs` shared with LD; the subset and the semantics of §2.16 and §2.13; every output's value kept in a hidden private variable typed by its value (`ast::TypeSpecKind::OfValue` in the ST front-end), every network's flag; `Holds::Guarded` for a function's result; `Language::Fbd`; `Sources::tc6`. Proven by `fbd_runs_on_the_vm.rs` (13), `fbd_refusals.rs` (15), `fbd_equals_st.rs` (5 FBD programs and their ST twins, 200 random sequences of 100 scans each: every shared variable and every kept value, by the element and output it is about, equal to its twin's after every scan, 3,560,000 values), `damaged_ladder_never_panics.rs` (3,000 damaged FBD documents besides the LD ones), the logic module's own tests (3) and `logic_module.rs` through the real module processes; the emulators (2.9). The workbench: `fbd.test.tsx` (11). Not built: the FBD editor (L3c-2); a CFC mode (§3) |

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
  (thumbv6m-none-eabi) 32,288 bytes of code with the real functions and formats 1 and 2 read
  (32,180 with the real functions before that, L2c-2; 23,216 with strings alone, 19,304 before them, 19,200 before L2b, 15,884 before format 1);
  **ESP32-C3** (riscv32imc-unknown-none-elf) 39,274 bytes (39,206 before the older formats were
  read, L2c-2; 27,204 with strings alone,
  22,812 before them, 22,908 before L2b, 19,146 before format 1). The real functions are most
  of L2c-2's: LREAL arithmetic in software on both. The L2b figures and the ones before it were both measured with rustc 1.99.1; the
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
- **L2c-2** (the library): 13 tests added to the five logic crates (197, from 184 with the
  reading of format 1; 192 from 179 before it):
  `math_against_the_host.rs` (the accuracy of §2.13, measured; NaN and infinity where IEC 60559
  gives them; exact where it can be), `library_runs_on_the_vm.rs` (8: every function against
  the VM's own values, the tables' examples, MUX's error, only the chosen input worked out,
  the operators by name, the refusals) and two loader tests. `library.st` joins the corpus:
  the oracle compares 1,980 more values (9,480 in all), every one agreeing; on both emulated
  instruction sets every scan's memory equals the host's (all 16 runs equal), so SIN, LN and
  EXPT in software floating point on a Cortex-M0 give the host's bits; of its 4,000 damaged
  copies 976 pass the loader and are scanned (32,000 damaged programs, 23,697 damaged
  sources). Mutation-checked with 19 mutations in a copy of the tree, each turning named
  tests red; two survived at first (a DINT given to a real function taken as REAL, and `**`
  binding below `*`), each caught by a test added for it.
- **Format versions, carried into L2c-2.** The seven corpus programs compiled by L2c-1's
  compiler (`78c84a7`, format 2) and by L2c-2's (format 3) differ only in the version field
  and the checksum; the diff of the VM adds the thirteen numeric instructions on new opcodes
  and changes nothing before them. One test added (198 in the five logic crates):
  `format_2_bytecode_runs_to_the_same_scans_as_its_source_compiled_now`, over the seven
  format-2 fixtures, committed with their hashes; the loader tests check every later
  instruction in every format from 1 to 3, and the runtime module takes a format-2 program.
  Mutation-checked, each in a copy of the tree: reading only the current version turns
  `every_format_this_vm_extends_is_read_and_runs_alike`,
  `an_instruction_newer_than_the_programs_format_is_refused_at_load`, both
  `format_N_bytecode_runs_…` tests and the runtime module's
  `a_program_built_before_an_update_of_the_vm_is_still_taken` red; reading any version turns
  `a_format_newer_than_this_vm_is_refused_not_guessed_at`,
  `a_program_of_the_format_before_this_one_is_refused_by_its_version` and the runtime
  module's test red; skipping the check of each instruction against its program's version
  turns `an_instruction_newer_than_the_programs_format_is_refused_at_load` red. The oracle
  (9,480 values, 287 deliberate) and the emulators (all 16 runs equal) are unchanged.
- **L4** (the standard library in ST, 2026-11-18): 21 tests added to the five logic crates
  (234, from 213 on `dev` at `ca35b59`; one removed, the backend's test holding the IR's
  catalog of standard blocks equal to the VM's, which no longer exists): `standard_library.rs`
  (14), `standard_blocks_run_as_the_native_ones.rs` (4), a third test in
  `older_formats_run_alike.rs` (format 3), one in the lexer (§6.1.2), two in the VM's loader
  tests (`NOW`, and the native blocks in every format); `runtime_api.rs` gained
  `a_retain_timer_keeps_its_elapsed_time_across_a_stop_and_run`. The format-3 fixtures were
  made once from `ca35b59`'s tree, with their symbols, and the format-1 and format-2 ones
  differ from them only in the version field and the checksum, which the test holds. The
  oracle is unchanged (9,480 values, 287 deliberate). The emulators run the corpus, its
  standard blocks now ST, on both instruction sets: all 16 runs equal (rustc 1.100.0, QEMU
  8.2.2, in the CI runner's image); the ESP32-C3 build of the VM is 39,318 bytes of code (44
  more, for `NOW`). Mutation-checked, each in a copy of the
  tree (`git archive`, its own target directory; the clean copy passed before and after):
  one millisecond added to TON's ET turned nine tests red, among them
  `the_timers_written_in_st_run_as_the_native_ones`, all three
  `format_N_bytecode_runs_…` and `a_retained_timer_carries_on_from_its_et_…`; R_TRIG never
  remembering CLK turned `the_edges_and_bistables_written_in_st_run_as_the_native_ones` and
  the three `format_N_…` red; a program's POU named TON let through turned
  `a_program_cannot_declare_a_pou_or_a_type_with_a_name_of_the_library` and
  `what_a_function_block_does_not_support_yet_is_refused_by_name` red; the clock-back rule
  putting START after the clock turned `a_retained_timer_carries_on_from_its_et_…` red, and
  with the guards that keep ET at zero or above removed as well, so that a negative ET comes
  out, also `a_timer_whose_start_is_more_than_a_time_can_hold_before_the_clock_is_done` and
  `the_timers_written_in_st_run_as_the_native_ones`; `NOW` taken in a format-3 program turned
  `now_pushes_the_clock_the_host_gave_the_scan` and `the_format_each_instruction_came_in_is_pinned`
  red.
- **L3b-1** (Ladder Diagram programs in the logic module and the read-only LD view,
  2026-11-18). Added: `ladder_power_probes.rs` (5) in the compiler; six tests of the logic
  module's own (LD template, check with its power and hidden variables, a diagnostic at its
  element, build, a list without a language read as ST, the date of a file header);
  `a_ladder_diagram_program_is_written_checked_built_loaded_and_runs_the_plant` and the LD
  part of `an_agent_writes_checks_and_builds_a_program_over_mcp` in `logic_module.rs`; 11
  tests in `ladder.test.tsx`, each reading rendered elements by `data-testid`. The e2e adds an
  LD scenario (an LD program added from the tree, rungs saved over the API, a mistake drawn at
  its element, bound to the simulator's counter, loaded; its contacts drawn live change state
  with the counter, the negated one always the opposite, and nothing is drawn live once it is
  stopped): 132 checks, all passed. Its two "built and loaded" checks had not been changed
  when C6b put the start sentence after that note, and failed on `dev` as well; they now
  require it. Mutation-checked in a copy of the tree (`git archive` of the tree this slice
  commits, its own target directory; the clean copy passed before and after the restores,
  each restored with `git show`): a negated contact drawn without its negation turned
  `draws each contact and coil from its variable, a link only where a variable holds its
  power`, `draws a negated contact as the negation of its variable` and `draws one value the
  runtime does not have as UNKNOWN, and the others as they are` red; an unknown value drawn
  as off turned `draws UNKNOWN, never a state, when the values cannot be read` and `draws one
  value the runtime does not have as UNKNOWN, …` red; an LD program compiled as ST turned the
  module's `a_new_ladder_diagram_is_a_tc6_document_that_compiles_to_a_program_doing_nothing`,
  `a_ladder_diagram_is_checked_with_its_variables_and_where_its_power_is_held`,
  `a_ladder_diagnostic_names_its_element_and_where_it_is_drawn`,
  `a_ladder_diagram_is_built_into_a_runtime_instance_like_any_program`, and
  `a_ladder_diagram_program_is_written_checked_built_loaded_and_runs_the_plant` and
  `an_agent_writes_checks_and_builds_a_program_over_mcp` red; a `programs.json` without a
  language refused turned `a_programs_list_written_before_languages_is_structured_text` and
  `a_ladder_diagram_program_is_written_checked_built_loaded_and_runs_the_plant` red.
- **L3b-2a** (every LD element's power kept for monitoring, and the view drawing it,
  2026-11-18). `ladder_power_probes.rs` rewritten (9, from 5): every output, junction and
  network published with the variable keeping it, each a private BOOL symbol; each kept power
  after a scan is the power the rung had, for every input of a program with a branch point, a
  timer and a junction; a seal-in rung, whose seal contact keeps what it read before its coil
  wrote (in three of eight scans that differs from the motor's value after the scan); a
  network a forward jump skipped reads as not run and keeps the last power it had; the
  networks after a return read as not run; a backward jump; an instance called by two blocks,
  each keeping its own call's output; a function with EN keeping ENO and FALSE as its result
  where it did not run. `ladder_runs_on_the_vm.rs` gained
  `a_long_series_of_functions_compiles_and_runs` (27, from 26); `ladder_equals_st.rs` is
  unchanged in what it compares and still holds 1,020,000 values equal; `logic_module.rs`
  reads `__NET_10`, `__LD_10`, `__LD_11` and `__NET_20` from the running module process. The
  view's tests (`ladder.test.tsx`, 13) read the mock `power` the compiler gives for their
  document: every element and link with its power, a contact with power off while its
  variable conducts, a junction's power apart from its branches', a rung not run drawn as not
  executed with FORCED kept, UNKNOWN when the values or one value or a rung's flag cannot be
  read. Mutation-checked in a copy of the tree (`git archive` of the commit, its own target
  directory; each file restored with `git show`; the clean copy passed before and after):
  a contact keeping its input in place of its output turned 34 tests red, among them
  `each_kept_power_after_a_scan_is_the_power_the_rung_had_in_it`,
  `a_seal_in_contact_keeps_what_it_read_before_its_coil_wrote`,
  `every_output_junction_and_network_is_published_with_the_variable_keeping_it`, the four
  `…_equal_their_st_twin` and the logic module's three LD tests (what is kept is what the next
  element reads, so a wrong store changes what the program does); the network flags never
  set FALSE turned `a_network_a_jump_skips_reads_as_not_run_and_keeps_what_it_had` and
  `the_networks_after_a_return_read_as_not_run` red; the view drawing a value it cannot read
  as not powered turned `draws UNKNOWN, never a state, when the values cannot be read` and
  `draws one value the runtime does not have as UNKNOWN, and the others as they are` red.
  The emulators: all 18 runs equal, the oscillator with its stores among them.
- **L3b-2b** (editing a Ladder Diagram in the workbench, 2026-11-18). Added: `ldmodel.test.ts`
  (15: every edit, each step checked on the TC6 it writes, that every element it did not delete
  keeps its localId and kind, that a new id was never held in the run, that the bytes before
  and after `<types>` are the document's; networks with titles, comments, labels, moved and
  deleted; every contact and coil; boxes of the library, a comparison and a block of the
  document with their pins; branches opened and closed, parallel coils, an empty branch kept as
  a wire; ids never given out twice, after a deletion, an undo, or a save and a new session; a
  document written back byte for byte when nothing changed; declarations; another tool's
  document read, edited without losing an element, its unchanged rung moved down whole and its
  network kept as found; the oscillator of the emulators read, and written back unchanged
  after an edit undone), `ladder-editor.test.tsx` (9, through the workbench's real host: a
  seal-in rung built from the keyboard and saved with Ctrl+S; the palette by click and by drag,
  a TON with its instance proposed and declared; the favourites and the palette shown or
  hidden, kept per viewer; titles, comments, moves, deletes, undo and redo; a network kept as
  found; a wrong operand told at once and then by the compiler at its element, and declared
  from where it was typed; completion by name and type; a document the editor cannot open
  drawn read-only with why; a viewer and an operator given the read-only view) and
  `ladder_from_the_editor.rs` (3) in the compiler, which compiles and runs the documents the
  editor wrote (`tests/ladder/editor_seal_in.xml`, `editor_foreign_edited.xml`, each held equal
  by `ldmodel.test.ts` to what the editor writes now): the seal-in latches on start and drops
  on stop, the timer lights the lamp 50 ms after the motor starts, with `waited` its ET, and
  another tool's document (`foreign.xml`) runs as before after the editor changed one rung,
  over 400 scans of the same inputs. Two of L3b-1's tests in `ladder.test.tsx` now sign in as a
  viewer: they hold the read-only view's positions, which an engineer no longer sees. The e2e
  adds an editing scenario (an LD program added from the tree; the palette, the favourites and
  the network list on screen by their rectangles in the real browser; a seal-in rung built with
  Ctrl+R, Shift+F2, Shift+F3, Shift+F7, Shift+F8 and Shift+F9, each operand declared with
  Ctrl+Enter; a timer rung from the palette; checked, saved with its header as the module made
  it, loaded and run; the motor coil drawn live going on with start, staying on through its
  own contact, the lamp on two seconds later, and both off with stop); two checks of L3b-1's
  scenario changed with it (an engineer's new diagram opens in the editor; rungs are numbered
  by network): 155 checks, all passed. Mutation-checked in a copy of the tree (`git archive`,
  its own `pnpm install --frozen-lockfile`; each mutation grepped as landed and the copy's
  `pnpm lint` passed before its tests; each file restored with `git show`; the clean copy
  passed before and after the restores): an insertion renumbering the other elements of its
  series turned nine tests of `ldmodel.test.ts` red, among them `writes a branch around a
  selection as parallel, never as series`, `writes the seal-in and timer rungs the compiler
  tests run (editor_seal_in.xml)` and `opens, edits and saves without losing an element, and
  leaves what it did not change as it was`; the next id taken as the highest an element has
  now, plus one, turned seven red, among them `never gives out an id twice, after a deletion,
  an undo, or a save and a new session` and `adds, titles, comments, labels, moves and deletes
  networks, each kept in TC6`; a write stamping the content header with `modificationDateTime`
  turned twelve red, every test of `ldmodel.test.ts` that edits; each branch of a parallel fed
  from the end of the one above it turned `builds a seal-in rung from the keyboard, declares its
  variables where they are typed, and saves it`, `writes a branch around a selection as
  parallel, never as series`, `opens a branch at one element and closes it after another, and
  parallels coils`, `keeps an empty branch as a wire, and says so` and `writes the seal-in and
  timer rungs the compiler tests run (editor_seal_in.xml)` red.
- **L3c-1** (Function Block Diagram programs, 2026-11-18). The crate `mythos-logic-ld` became
  `mythos-logic-graphic` (§2.16); every LD test passed unchanged through the move of its reading
  and arrangement into `graph.rs`, `ladder_equals_st.rs` among them (1,020,000 values). Added:
  `fbd_runs_on_the_vm.rs` (13), `fbd_refusals.rs` (15), `fbd_equals_st.rs` (5 programs, 3,560,000
  values), an FBD half of `damaged_ladder_never_panics.rs` (3,000 documents: 2,678 refused, 322
  compiled), three tests of the logic module's own, `a_function_block_diagram_program_is_written_checked_built_loaded_and_runs_the_plant`
  in `logic_module.rs` (the MUL's result read from the running module process as an INT, 200),
  `fbd.test.tsx` (11; its mock `values` checked against what the compiler gives for the same
  document, probe for probe and type for type), and an FBD scenario in the e2e (an FBD program
  added from the tree, networks saved over the API, a mistake drawn at its element, bound to the
  device's counter, loaded, every line drawn live: the MUL's output ten times its input, the
  continuation the same, `odd` and its negation opposite, both changing with the counter; nothing
  live once stopped): 173 checks, all passed. Writing the mock against the compiler found a
  connector named as a variable in it (refused, §8.1.3) told twice; a refused connector's
  continuations are now not told again. Mutation-checked in a copy of the tree (`git archive` of
  the commit, its own target directory; each injection grepped as landed; each file restored with
  `git show`; the clean copy passed before and after the restores; the frontend copy with its own
  `pnpm install --frozen-lockfile` and `pnpm lint` before its tests): no element waiting for what
  it reads (data flow ordered by position alone) turned 24 tests red, among them
  `a_block_is_evaluated_after_what_it_reads_wherever_it_is_drawn`, the five `…_equal_their_st_twin`,
  `the_blinker_the_emulators_run_does_what_its_networks_say`, `the_base_lowers` and the logic
  module's two FBD checks (a block before its input reads a variable not yet typed, which the
  checker calls a fault of the compiler); a negated input pin's negation dropped turned
  `a_negated_input_and_a_negated_output_are_negated_outside_the_block`,
  `functions_negation_and_numbers_equal_their_st_twin` and
  `the_blinker_the_emulators_run_does_what_its_networks_say` red; a continuation published as
  holding another line's value turned `a_connector_and_its_continuations_are_one_line` and
  `lines_jumps_and_a_return_equal_their_st_twin` red (at first only the first: the equivalence
  test mapped each probe to its twin by the variable the probe names, which cannot catch a probe
  naming the wrong one; it now maps by the element and output); the view drawing a value it
  cannot read as FALSE turned `draws UNKNOWN, never a value, when one value or all values cannot
  be read` red. The emulators: all 26 runs equal.
