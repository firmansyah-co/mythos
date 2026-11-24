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
 (LD, FBD, SFC) ──────┘     (the seam)         └─► (C99 later: MCUs too small for the VM)   ESP32, RP2040)

 mythos-logic-compiler: joins a front-end, the target's profile and a backend. Owns neither.
```

| Crate | May depend on | Why |
| :--- | :--- | :--- |
| `mythos-logic-ir` | nothing of the workspace | The seam. A *unit*: structures, array types and POUs (functions, function block types, one program), every name resolved to an index; no addresses, word sizes or instructions. `serde` data with a version (`IR_VERSION`), so a front-end and a backend need not share a process |
| `mythos-logic-vm` | nothing at all | Builds for a microcontroller with `core` alone |
| `mythos-logic-st` (and each later language) | the IR | A language is added without touching a target |
| `mythos-logic-graphic` (LD, FBD and SFC; `mythos-logic-ld` until L3c-1, §2.16) | the IR, and the ST front-end | A graphical language reads the common elements (declarations, types, literals), the text inside its elements and the standard library through the ST front-end, so that both have one rule for each (§2.15); it reaches no target. LD, FBD and SFC are one crate because one PLCopen TC6 document holds POUs of each, each calling the others (§2.16, §2.17) |
| `mythos-logic-backend-vm` (and each later backend) | the IR, its own target | A target kind is added without touching a language |
| `mythos-logic-compiler` | all of the above | The one call from source to a program |

`crates/mythos-logic-compiler/tests/layers.rs` reads the real dependency graph and fails on a
break, and on a new `mythos-logic-*` crate that has not declared its layer; a second test feeds
the guard each kind of break.

- **Nothing unsupported is compiled into something else.** A construct a front-end does not take
  yet is a diagnostic naming it, with its line and column.
- **LD, FBD and SFC are lowered from their graphs**, not printed as ST text: each to the ST
  front-end's syntax tree, built element by element, never through text (§2.15, §2.16, §2.17).
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
  which are the module's own and opaque to the core, and the bindings, which the core reads;
  and, from the logic module since 0.8.0, the project program it is a build of, `built_from`,
  and a mark of its making, `created`, which the core compares and a module ignores),
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
that carries the same calls to firmware over the network (§2.18: TCP, on WiFi or Ethernet). A different engine can be a
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
  seconds stops), and an answer carries the program's state and its age: `age_ms`, how long
  ago the runtime was read, and `fresh_ms`, how old an answer may be and still be given at once
  (1000 ms; an older one is waited for, up to 2.5 s). The state carries `scan_age_us`, how long
  before the read the program last completed a scan (`RuntimeState::scan_age_us`: absent before
  its first scan and from a module that does not say; optional on the wire, so a module built
  before it still loads). A program that has stopped scanning still answers, in RUN until its
  watchdog fires, with its last values and a scan age that grows (§2.8). Reading changes
  nothing in the program. A variable the program does not have, a whole array, a structure or
  an instance has an error in place of a value; a REAL that is not a finite number is said,
  not sent as null. A runtime module without `watch` answers that it gives no values.
  Each list of variables is a watch of its own (C8d-core): callers with the very same list (the
  same names in the same order) share one watch and one answer, and callers with other lists do
  not replace each other's. At its exchange the core sends one `Call::Watch` with the union of
  every live list, each name once, and gives each list the values of its own names in its own
  order, found by the name as asked. A name the reply has no value for is answered with value
  null and an error, never a value. A list is answered only from a read made after it was first
  asked for, so a list added after the union was sent waits for the next read. Each list expires
  alone, five seconds after it was last asked for. At most 256 names in the union and at most 256
  lists of one runtime are kept: a new list past either is refused with 409 and words saying why,
  and a list already watched is never refused. The 256 lists are a design limit decided with
  that slice, not a published figure, and the size of a name in bytes is not bounded. A module
  that does not answer fails every list of the runtime (502). Evidence:
  [`33-c8d-core.md`](ADR-013-evidence/33-c8d-core.md).
- **RETAIN, warm and cold restarts** (C7b; IEC 61131-3 §6.5.6). `run` from STOP or FAULT is
  a *warm restart*: a variable the compiler marks RETAIN (`"retain": true` in the symbol table)
  keeps the value it had, every other variable takes its initial value. `POST
  /api/v1/runtimes/<id>/cold-start` (engineer; MCP `cold_start_runtime`) is a *cold restart*:
  every variable takes its initial value, RETAIN ones included, and the program runs; a
  program in RUN is restarted too. The core keeps the RETAIN values of each runtime of the
  active project in `<system>/retain/<project>/<runtime>.json`, beside the controller's own
  data and not in the project, one file per runtime id and, inside it, one entry per program
  (below): it asks the module (`Call::Retained`) every two seconds and
  writes the file when they changed, and again at once after a change of mode and before an
  unload. A module process started by the core (a load, a server start, a process started
  again after a crash) is given the values kept last (`retained` in its start
  configuration), so a start into RUN is a warm restart: a restart of the server loses at most
  two seconds of them. A value is taken only into a RETAIN variable of the same path, type and
  size; a program built again from changed text takes what still fits and the module logs the
  rest. The file is an object, `{"programs": [{"name", "built_from"?, "created"?, "values"}],
  "data"}`: one entry per program of the instance, its values with the program's name taken
  off, and the data masters of retained blocks in `data`. A start gives a program only the
  entry kept for it: the same program by `built_from` and `created` when both name their
  project program, else by PROGRAM name. A runtime of several programs is given
  `<program>.<path>`, one program a bare path. Another program's entry is not given, and
  neither is one that two programs could be; each value withheld is listed in `not_given` in
  the load's answer and in the log. Every data master is given as kept. A retain file written
  before values were kept per program (a JSON array) is given once as it is, to whatever the
  runtime runs, and written again in the new form at the first keep. A program loaded in STOP reports the values it was given until it runs, so waiting
  never replaces them with its zeros. A module that keeps no RETAIN values says so and every
  start of it is cold.
- **Online change** (C6b; the owner's decisions of 2026-11-18). When a load replaces a program
  in RUN with a new build of the same programs (by `built_from` and `created` when every
  program of both names its project program, else by PROGRAM name; in the same runtime module
  and the same project), the new build takes over the running one's variables and scans on, without a cold
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
  - *When it is not.* A program that is not the same program (another project program that
    declares the same PROGRAM name, one deleted and made again under its name, any program of a
    project made active since the running one started), one in another runtime module, one that was not
    in RUN or that an engineer had asked to stop, one loaded in STOP (`"run": false`), and one
    whose module cannot give its variables or does not within 2.5 s, start as before: RETAIN
    variables from the values kept for that program, the rest from their initial values. `PUT
    /api/v1/projects/:id/runtimes?start=cold` (MCP `save_runtimes_config`, query `start`:
    `cold`) starts every runtime the load starts or replaces cold, RETAIN variables included;
    one the load leaves unchanged is not touched (`cold-start` restarts it). Another value of
    `start` is refused and nothing is saved. A query key other than `start` and `revision` is
    refused with 400 and words naming it, and nothing is saved.
  - *The revision of the read.* `GET /api/v1/projects/:id/runtimes` answers `{config, revision}`,
    the lower hex SHA-256 of the bytes of `runtimes.json` as read (of no bytes when there is no
    file; an empty file has no bytes either). `PUT ...?revision=<r>` (MCP `save_runtimes_config`,
    query `revision`) is saved only when `<r>` is the stored file's revision, and its answer adds
    the `revision` of the bytes written. Otherwise it is refused with 409 and words saying the
    runtimes changed since they were read, and nothing is saved or applied. The revision is
    compared before the module checks the programs and again under a lock that every save of this
    route takes around the compare, the write and the reload. A save without `revision` is saved
    as before and can overwrite a save made since the read. The lock does not cover a slice
    install or the reloads of other routes (`33-c8d-core.md`).
  - *What the load says.* The answer's `changes.runtimes` has, for each runtime the load
    started, `start`: `online`, `warm`, `cold`, or `unknown` (its module was given the running
    values and does not say what it took, which is never reported as online); `why` it was not
    changed online; and, changed online, `carried` (how many variables) with `dropped` (the
    running build's variables it did not take) and `initialized` (its own that took none), each
    with why; and, for a start that withheld kept RETAIN values, `not_given` (each
    `{variable, why}`). The editor's Load says the same (§2.6), except `not_given`, which the
    workbench does not show.
  - A running build and a new one that do not both name their project program are compared by
    PROGRAM name, so a running instance saved before `built_from` existed is changed online by
    a build of any program of its PROGRAM name.
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
  bindings (inputs, outputs with their safe values, published variables). A program that no
  task of a configured runtime names runs alone: a runtime of its own, named like the program,
  with one cyclic task at its interval. A configured runtime (C8c, `logic/tasks.json`, §2.19) has
  its module, its target and its tasks, and runs several programs; the logic module builds it
  as one instance.
- **Its language** (L3b-1; `fbd` since L3c-1, `sfc` since L3d-1) is `st`, `ld`, `fbd` or
  `sfc`, chosen when it is created (`create_logic_program` takes `language`, `st` by default) and
  not changed after. Structured Text is `programs/<name>.st`; a Ladder Diagram, a Function Block
  Diagram or a Sequential Function Chart is `programs/<name>.xml`, a PLCopen TC6 XML v2.01
  document holding its POUs (§2.15, §2.16, §2.17). A new SFC program is a TC6 project holding the
  PROGRAM with no variable and one initial step, `Start` (a chart has exactly one, §6.7.2), which
  compiles to a program that does nothing; `check` gives `chart` for `sfc` (§2.17). A
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
- **Each program is compiled from its own source, with the project's library, types and data
  blocks and no other program's**, through `compile_sources`. A project's programs share no POU: every source holds a PROGRAM and a unit has
  exactly one, so another program's functions and function blocks could not be compiled in
  without taking its PROGRAM out of its file; and a build that took another program's POUs
  would change, and get a new version, when that other program is edited, so a running
  program would be changed online by an edit nobody made to it. What a program calls is in
  its own source: in an LD document, function and function block POUs beside its PROGRAM, in
  LD; in ST, before it. The project's library (functions and function blocks in
  `<project>/logic/library/`, sources with no PROGRAM and no TYPE) is given to every build, and
  so are the types and data blocks the data module keeps in `<project>/data/` (D3,
  [ADR-014](ADR-014-data-blocks-and-the-data-module.md)).
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
  a hash of the code, the interval, the bindings and the layouts of the data blocks the program
  uses. A program that a configured runtime runs is not built alone: its build is refused with
  409 naming the runtime, and `build_logic_runtime` builds the runtime with all its programs
  (each compiled for the runtime's target, from one snapshot of the project), answering one
  instance with `tasks`, `replaces` (the ids the programs run under alone) and each program's
  version, which hashes the task the program is in in place of the interval. What the version of
  a configured runtime hashes is its programs' versions joined with `, `, as the core reports
  it. Every build carries `built_from`, the program's name in the project, and `created`, a mark
  given when the program was made (none for a program made before it); neither is in a version.
  `replaces` is advice to the caller: the module does not know what the core loads. The core
  refuses a save in which one program of the project is in two runtimes, when both instances
  name it as built by this module (every build since 0.8.0 does; an instance built before does
  not, and its program then runs in both until it is taken out). **A module cannot call the core**
  (ADR-011 §2.6), so the module does not load what it builds: the workbench, or an agent, saves
  the instance into the project's runtimes, and the core loads it. An edit therefore changes
  nothing that runs until it is built and loaded. A bound tag that no connection provides now
  is a warning of the build, not a refusal.
- Not built: unit tests in ST. A device's slice carries its programs (C6a, §3), LD documents as any file of `logic/`.

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
  answer the runtime is UNKNOWN; after a read fails, or while one has gone unanswered for 3 s
  since the last answer (Settings → Runtimes' rule, `RUNTIMES_STALE_AFTER_MS`; found in review
  of the stale-watch slice: a request left hanging left RUN and the last scan figures unmarked),
  what was read before is marked LAST KNOWN and Run is disabled. An answer to a read asked before
  a newer one that was already applied is dropped. Stop is never taken away, by a stale state or by a request still
  unanswered: it is the safe direction. Run asks first, as on Settings → Runtimes. Scan times are NOT MEASURED until a scan has run. A new output
  binding has no safe value, and none is written for it, until one is given.
- While a runtime runs the program, the variables table shows each variable's value in it,
  read twice a second by name with `watch` (§2.5); a STRING as Structured Text writes it,
  quoted, with `$` combinations for `$`, `'` and what is not printable. A STRING has no "Bind…"
  and says why. A variable the running version does not
  have says so; a value read from a program that is not in RUN is marked as what it stopped
  with; when the values cannot be read the cell says UNKNOWN and shows none read before, and so
  it does for values that are not current (§2.8), with why in its title.
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
- Not built: watching an element of an array from the table; the FBD editor (L3c-2); the SFC
  view (L3d-2) and editor (L3d-3) (each built since, §2.8). An SFC program's editor said the chart was not drawn yet and
  showed its TC6 document as text, read-only, each diagnostic underlined at its element's tag,
  nothing live (L3d-1); it is checked, loaded and run as any program (`sfc.test.tsx`).
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
compile, or it has not been checked yet: the two are told apart, L3d-2). A diagram open in any
editor, an engineer's or a viewer's, follows a source saved elsewhere (an agent, another
workbench) and checks it at once, quietly, as the editors' own check after an edit (not busy, a
failure only logged, an answer for an older text dropped); until L3d-2 only an engineer's editor
checked it, and a viewer's diagram stayed not live after an agent's save. Then:

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

**Only current values are drawn live** (found in review of L3d-2; `currencyOf` in
`programs.ts`). A runtime that says RUN but has stopped scanning (its scan thread hung or
fallen behind, before its watchdog's ten intervals have passed) still answers a watch, with its
last values; and an answer the editor holds while its asking gets no newer one ages in the
browser. Neither is drawn as current. The values of a watch answer are current while both of
these hold, each bound worked out from what the controller says and none a constant of the
editor's own:

- *the read is recent*: its age (`age_ms`, and the time since the request was **sent**) is
  within the route's `fresh_ms` (the oldest answer it gives at once) and one interval of the
  editor's asking (500 ms): 1.5 s today. An answer older than that should have been replaced and
  was not. Measured from the send, not the arrival, so the time an answer spent in transit counts
  as age (it is, and the send time never makes values younger than they are); and on a
  monotonic clock (`performance.now()`, `monotonicNow`), so setting the wall clock back neither
  keeps values current nor puts the re-check off;
- *in RUN, the program was scanning when it was read*: its last scan had completed
  (`state.scan_age_us`) within two of its intervals (`interval_ms` of `GET /api/v1/runtimes`)
  and its longest scan (`state.max_scan_us`), the interval taken only from a runtime of the
  version that answered: a list read before a load still names the old version's interval, and
  until it names the answer's the values are not current. The VM's scheduler makes no run that falls due while
  the last has not ended and counts it as an overrun, so a
  program scanning without overrun completes a scan at least that often (scan *k* starts within
  an interval of its due time and ends within its longest scan of starting; scan *k+1* is due
  one interval after *k*). Past it, the values are older than the program's own cycle. A host
  that holds the scan thread back by an interval or more makes the values not current for that
  moment, which is what they are. The watchdog's limit (§5, C7a) is not the bound: it is what
  this bound comes before.

What the rule needs and is not told (an answer without `age_ms` or `fresh_ms`; in RUN, without
`scan_age_us`, before a program's first scan or from a module that does not say, or without
`max_scan_us`) makes the values not current, never current. Values that are not current are
drawn as each view draws values that cannot be read: every state of a diagram UNKNOWN, the view
saying why (`data-live="unknown"`), and every value of the variables table UNKNOWN with why in
its title. In STOP only the read's age counts: the values are what the program stopped with. The
editor judges again at the instant an answer stops being current, so a request left hanging does
not leave the last values drawn. Requests overlap (each asked on time, whether the one before has
come back or not, so one left hanging never stops the next): each is numbered when asked, and an
answer to one older than the newest applied, or asked before the watch was dropped or the store
reset, is not applied, so an older answer coming back last never replaces a newer one. Settings → Runtimes keeps its own rule (a read of the list
older than 3 s is the last known state, `RUNTIMES_STALE_AFTER_MS`): it shows what the runtimes say, not a program's
values, and its fixed bound sees no stalled scan, so it was not reused here.

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
  bistables; compare; math; move; strings; the project library's functions and function blocks; the
  document's own function and function block POUs; program control) or from the favourites bar above the networks, by click or by drag:
  contacts normally open, normally closed, positive and negative edge; coils plain, negated,
  set, reset, positive and negative edge; open and close a parallel branch; a box of any
  function or function block, the standard library's (TON, TOF, TP, CTU, CTD, CTUD, R_TRIG,
  F_TRIG, SR, RS, the comparisons, ADD … MOVE, SEL, the numeric and string functions, a
  conversion `X_TO_Y` typed into an empty box), the project library's (the logic module's answer
  of `blocks` in `POST /library/check`) or the document's own, with its pins; a jump,
  the network's label, a return. A program compiles from its own source alone (§2.6), so the
  POUs a box can call are the standard library's, the project library's and the document's: an
  ST POU of the project is not among them. The favourites start as TIA's set (normally open, normally closed, coil, empty
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
  document's POUs; the module's answer for the project's library). Which pin carries the rung (§8.2.6): a function block whose first input is
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
favourites bar stays on the editor's toolbar), and so are the Function Block Diagram's, each
language with its own; the runtime's state and commands (RUN/STOP,
Unload, Release all, scan times, forces) are the **Runtime** task card; the program's properties
(language, task interval, runtime module, target, Run when loaded, Remove) are the inspector's
Properties, selected when the editor opens and again on a click in it; what the compiler said
is a panel of the inspector's **Diagnostics** tab (`host.setDiagnostics`), counted on the tab
only for the text as it is now, and brought forward after a Check the engineer asked for, and
after a Load only when the compiler refused it. The toolbar says the check in a word. Nothing is set while another
editor is active: the workbench draws only the active one.

**The Function Block Diagram view** (L3c-1; `FbdView.tsx`, `fbd.ts`). An FBD program opens in
the same editor as ST and LD, with the diagram in place of the text; the variables table,
bindings, Load, Load cold, Run, Stop, Unload, forcing and watching are unchanged. It is read-only:
since L3c-2 it is what a viewer and an operator get, and an engineer gets the FBD editor (below),
which draws each network with it. The diagram is
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

**The Function Block Diagram editor** (L3c-2; `FbdEditor.tsx` beside the view, and `fbdmodel.ts`,
`fbdedit.ts`, `fbdcatalog.ts`, `FbdInstructions.ts`; the splicing, the record of localIds and the
network headers are LD's, §2.15). The owner's decision of 2026-11-18 for LD holds: TIA Portal's
workflow and conventions as closely as they can be followed, artwork of our own (no artwork,
icon or branding of Siemens), IEC 61131-3's symbols, and no code of TIA Portal, CODESYS,
Beremiz, OpenPLC, 4diac or STruC++ read or copied. **Standard FBD first** (the owner's decision
of 2026-11-18, §2.16), as TIA's FBD editor: networks laid out by the editor, not placed freely; a CFC mode
(free placement, `executionOrderId`, lines drawn back) is a later item (§3). The visual design is
the owner's to lead; the editor uses the workbench's classes and tokens (LD's `.ld-*`).

- **Who, and what is saved**, as LD's: an engineer gets the editor, a viewer and an operator the
  view; unsaved until Save (or Ctrl+S); Load saves first. Every edit changes the model, which is
  written back into the TC6 text at once; each network is drawn by the view's `FbdPouView` from
  that text, moved to the top of its own drawing, with the view's live values (drawn on the
  PROGRAM only); the view's own network number is hidden, as the header numbers it. Over the
  drawing the editor draws the selection, its own marks of what is wrong, and a small square at
  each input that can be selected.
- **A network is one tree.** TC6 has no element for a network, and the front-end's network is a
  set of connected elements (§2.13 on §8.3.3), so two trees side by side in one network would be
  read back as two networks. *Decided on that ground* (the owner's brief said "a set of trees";
  keeping several would need a network element of our own in `addData`, which another tool and
  the compiler would not read): an editor network holds one tree, and a second goes into a
  network of its own (Ctrl+R). The tree, read left to right: a box, each input taking one operand
  (an `inVariable` wired to it; or an `expression` at the pin, as another tool may write one,
  which is kept as an expression) or the output of a nested box; a box's output goes into at most
  one further box and to any number of assignments (`outVariable`, negated or not), jumps and
  returns, as TIA branches an output; or an assignment, jump or return that reads an operand. An
  operand the editor writes is an `inVariable`, so the view draws it with its live value.
- **Kept as found.** A network another tool wrote that the tree cannot express (a connector and
  its continuations, an in-out variable or in-out parameters, a block's pin with an `edge`,
  `storage` or `hidden` attribute, which the editor would not write back, a line drawn back, an output into
  two boxes, a variable read at two inputs, a second line into one input, an element it does
  not know) is kept byte for byte: it can be moved (each of its elements by the same distance,
  only down, as far as a network above it grew), deleted, titled, labelled and commented, nothing
  is put into it, and it says why. A network's comment or label that did not change is written as
  found too, only moved.
- **One line into one input** (§8.3.2, no wired-OR). The model's input holds one source and the
  writer gives each input at most one `connection`. An operand typed at an input a box feeds is
  refused, and the editor offers what TIA would: **Join with OR** puts an OR box at that input,
  what it read into IN1, and selects IN2 for the second signal (an OR joins BOOLs; at an input of
  another type it is refused).
- **Instructions**: the Instructions task card (LD's card given the FBD's kit) and the favourites
  bar, by click or by drag. Groups: bit logic (AND `&`, OR `>=1`, XOR, NOT, assignment, negated
  assignment, one more assignment from an output, insert an input), timers (with the empty box),
  counters, edges and bistables (R_TRIG, F_TRIG, SR, RS), compare, math (MOD among them), move,
  strings, the project library's functions and function blocks, the document's own function and
  function block POUs, program control (jump, the
  network's label, return); a conversion `X_TO_Y` is typed into an empty box. **Set and reset**
  are the SR and RS boxes and an edge is R_TRIG or F_TRIG: TIA's S and R assignment boxes are not
  offered, because the front-end refuses `storage` (and `edge`) on a variable (§2.16), and the
  editor offers only what compiles. A negation is the circle at a box's input, a box's output or
  an assignment, or the NOT box. The favourites start as AND, OR, assignment, empty box, one more
  assignment and insert input, and are kept per viewer apart from LD's
  (`mythos.logic.fbd.favourites`).
- **Where an instruction goes**, as TIA's FBD puts a box into the line the selection is on: a box
  at a selected input takes that input's place, and what the input read goes into the new box's
  first input; after a selected box, that box's output goes into the new box's first input and
  the new box takes its place, the assignments of that output moving to the new box's; at an
  assignment, jump or return that reads an operand, the operand goes into the box and the
  assignment is written from the box's output; with only the network selected, as at the end of
  its tree (its last box or its assignment), or as its first element when it has none. An
  assignment, jump or return at a selected box (or one of its inputs or assignments) is one more
  written from its main output (OUT, Q, the first output that is not ENO); the "=" beside any
  output assigns that one (a timer's ET). Deleting a box gives its place to what its first input
  read; it is refused while an output other than the main one is assigned, or while the main
  one is assigned to two variables and no box takes its place, since a variable no longer
  written keeps its last value (found in review: `fbdedit.test.ts`). Deleting at an input
  empties it, and an empty input an AND, OR or XOR was given is removed, its INn inputs
  numbered again and EN left EN.
- **Boxes.** A box's pins are its declaration's (§2.14; the interface for the document's POUs; the module's
  answer for the project's library;
  AND, OR, XOR of IN1, IN2, … and NOT of IN, as the front-end takes them; an empty box `??` has
  IN1, IN2 and OUT until it is typed). EN and ENO are kept where the document has them and not
  added: an FBD box carries no rung. A new type keeps what each input read by its formal
  parameter, puts what an input it does not have read into its first free input, and is refused
  when that would lose a source. A function block gets an instance proposed as in LD.
- **A box's output type is the compiler's.** Beside each output the editor writes the type the
  check gives the variable keeping it (`values` and `variables` of the check of the text as it is
  now, §2.16), or `type UNKNOWN until the program compiles`; it never works one out. Its own
  check compares only types a declaration fixes (AND's OUT is BOOL, a TON's ET a TIME), never a
  generic one (ADD's ANY_NUM).
- **The editor's own check**, at once, written under the element's fields and marked around it,
  then the compiler's (a quiet check 700 ms after the last edit, as LD's), each at its element: a
  name not declared; a value of another family than the input or the assignment takes; an
  assignment writing an input of the POU; a function's input with no source; a box with no type
  or one nothing declares; a function block with no instance, one of another type, or one called
  twice in a network; a jump to a label no network has; an assignment, jump or return that reads
  nothing; a network whose last box is a function nothing reads. A plain name not declared is
  offered to be declared where it is typed, as in LD: a BOOL input at a BOOL input, the box's
  type for an instance, the output's type for an assignment where a declaration or the compiler
  gives it.
- **Layout**, on a grid of 10: a box's inputs are 20 apart from 30 below its top (as the view
  draws them); an operand 70 left of its input, a nested box 90 left of the box it feeds,
  assignments 90 right of their output (room for the live value the view writes at each output,
  `2000 ms`), operands and assignments 40 apart (room for the value the view writes under a
  variable), each tree's leftmost element at the body's left, networks
  stacked top to bottom. Positions, sizes and `relPosition`s are written, so other tools and the
  view read them. Within a network the front-end's order is data flow with ties taken top-left
  first (§2.16), which the stacking keeps.
- **localIds**, as LD's (§2.15): every element keeps its id while it exists (online change
  carries `__FBD_<localId>` by its path); a new one takes the first id no element of the
  document's bodies, in any language, has had, and the record in the POU's `addData`
  (`urn:mythos:logic:ld-editor`, LD's) keeps undo and a new session from giving it out again.
  The network's title is kept in its comment's `addData` under LD's name
  (`urn:mythos:logic:ld-network`), so each editor reads the other's headers.
- **Undo and redo** cover every edit, headers and declarations among them.
- **Keys**, while the editor has the focus: Shift+F2 AND, Shift+F3 OR, Shift+F7 assignment,
  Shift+F5 empty box, Shift+F8 one more assignment from the selected box's output, Shift+F9 one
  more input of an AND, OR or XOR; Ctrl+R a network after the selected one; Alt+↑ and Alt+↓ move
  the selected network; ← and → the previous and next element of the network (what a box reads
  before it, its assignments after); ↑ and ↓ the previous and next network; Enter the first field
  of the selection (an input's operand, a new box's type or instance); Delete what is selected;
  Ctrl+Z undo, Ctrl+Y or Ctrl+Shift+Z redo; Ctrl+S save; in an operand field LD's keys. *Why
  these*: TIA's own FBD keys could not be confirmed from a public source (2026-11-20: Siemens'
  keyboard reference, entry 48697265, does not list them), so none is claimed to be TIA's; the
  keys are LD's positions given their FBD meaning (Shift+F2 the series of LD, AND; Shift+F3,
  beside it, the parallel, OR; Shift+F7 the coil, an assignment; Shift+F5 the empty box; Shift+F8
  LD's branch, here a branch of an output; Shift+F9 insert input), so an engineer moving between
  the two languages finds each key where its meaning is.
- Not built: the CFC mode (free placement, `executionOrderId`, a line drawn back as a feedback
  path); connectors and continuations drawn or put by the editor (kept as found); an output into
  two boxes, two trees in one network, in-out variables and parameters; EN and ENO added to a box
  that has none; TIA's S and R assignment boxes (the front-end refuses them); moving a box by
  dragging it, copy and paste; editing a network kept as found; adding a POU to the document.

**The Sequential Function Chart view** (L3d-2; `SfcView.tsx`, `sfc.ts`). An SFC program opens in
the same editor as ST, LD and FBD, with the chart in place of the text; the variables table,
bindings, Load, Load cold, Run, Stop, Unload, forcing and watching are unchanged. It is read-only:
until L3d-3 for everyone, an engineer included (a chart was written over the API,
`save_logic_source`); since L3d-3 it is what a viewer and an operator get, and an engineer gets the
chart editor (below), which draws the chart with it. The tree adds a program in SFC as in the
other languages (until L3d-2 only the API did): it starts as the module's template, one initial
step, drawn at once. An open chart follows a source saved elsewhere and checks it at once,
quietly (above, the live state of LD), which for a chart, written only elsewhere until L3d-3, was
every save. The TC6 document stays reachable under the chart, folded, as read-only text with each
diagnostic underlined, drawn only while it is open (a browser lays out what a closed `<details>`
holds); it opens when the document cannot be read as a chart, at once or after a follow, and a
diagnostic with no element of the drawing opens it at its line.
**Decided by the owner (2026-11-20):** TIA Portal's conventions for a sequence (S7-GRAPH's view:
steps as boxes, the initial step double-framed, transitions as bars with their condition, active
steps highlighted with their time, a transition whose condition holds shown so), with IEC
61131-3's graphical forms (§6.7) and never Siemens' artwork, icons or branding; no code of TIA
Portal, CODESYS, Beremiz, OpenPLC, 4diac or STruC++ read or copied. Drawn as SVG:

- **The symbols** (§6.7, Tables 54–61, Figure 22): a step a box with its name, the initial step
  with a second frame inside the first; a transition a bar across its line, its condition written
  beside it (its ST text, cut at 40 characters with the whole in the element's title, or the name
  of the POU's transition it names; a negated one as `NOT (…)`), and its priority left of the bar
  where it counts: only on a transition after a selection divergence (Table 61 feature 2b, where
  the compiler reads it); elsewhere its title says the priority is not used; a selection divergence or convergence one horizontal line, a
  simultaneous one two; a jump an arrow down with the step it goes to; an action block beside its
  step, a row per association: the qualifier with its duration (`D T#20ms`), then the action or
  BOOL variable it names, or its body; a comment in a dotted frame; every link from the element
  it leaves to the element it reaches, a block's from the side of its step it is on. A step is
  drawn with its name and no number: TC6 gives a step a name and no number, and its `localId` is
  the document's, not the step's. An element the front-end does not read (a `macroStep`, an
  element of LD or FBD), or the second of two elements with one `localId`, is listed as not drawn.
  **An action or a condition written in LD or FBD** (L3d-1b, §2.17) is named with its language
  and not drawn yet: `(in LD)` in its row or at its transition, `Its body is in FBD …` under the
  POU's action or transition, a condition given by a network drawn in the chart as `(network in
  the chart, from <localId>)`, and each element of that network listed as not drawn (`contact 21
  (of a network of LD or FBD drawn in the chart)`); their values (`bodies` of the check) are not
  read. Drawing each body with the LD or FBD view and its `bodies` (§2.17, what the view needs) is
  the next step of the view, not built here. Each POU of the document is drawn: a chart of a
  function block without live state (each instance has its own), a body of another language
  named and not drawn; under each chart, the POU's own actions and transitions with their text.
- **Where.** At the document's positions, two and a half times their units (`SCALE`), and sizes
  where the document gives them, scaled the same; each step, transition and jump on an axis 55
  right of its x, the middle of a step's box. *Why the scale*: the steps of a chart written by
  hand are 40 apart (the corpus, the module's template), and a step's name and time do not fit
  in that; one factor keeps every relation the document draws. *Not used*: a `relPosition` of a
  connection point or of an action (rows are stacked in the document's order), so the links of
  a document written with connection points elsewhere jog to meet each element's axis; a block
  placed close to the steps (the corpus's Fill) may overlap them, as its document places it.
- **Diagnostics.** One with an `element` is a mark around it, an action of a block by the
  action's own `localId` around its row; one in a body of LD or FBD, whose `local_id` is the
  body's own element and never the chart's of that id, around what holds the body (the row or
  the transition named by `element.inline`; the POU's action or transition by name, below), and
  one about an element of a network drawn in the chart at the transition it reaches directly; one naming an action or a transition of the POU
  (`element.action`, `element.transition`) is marked at every row or transition that names it and
  written under that action's or transition's text with the line of the text; each is listed as
  `<pou> element <localId>`, or `<pou> action <name> line <n>` and `<pou> transition <name> line
  <n>`, and in a body `<pou> element <inline>, its body element <localId>` or `<pou> action <name>
  element <localId>`.

Live state, under the same conditions as LD's and FBD's (the check of the text shown is current,
its version is the runtime's, in RUN, and the watch answer is of that version in RUN; otherwise
nothing is drawn live and the view says why, in STOP that the chart is not scanned), from the
probes of `chart` (§2.17), each watched by its path (the hidden ones are not listed in the
table):

- a step whose flag X is TRUE is drawn active (its frame solid, thick and `--c-good`, filled
  `--c-good-soft`), with its time T written in it as the variables table writes a TIME
  (`T 45 ms`); a step not active is drawn plain, with no time (its T holds the time it was last
  active, which TIA does not show either);
- a transition's condition is drawn TRUE (its bar thick, `--c-good`) or FALSE (`--c-text-muted`,
  dashed) **only where its `ran` flag says the transition was enabled in the last scan**, so its
  condition was evaluated then; where it was not, *not evaluated* (faint), since its variable then
  holds an earlier scan's value, which is never shown as this one's. In the scan that activates
  a step, the transition after it is not tested yet (§2.17, a scan in order): it is drawn not
  evaluated beside a step whose time is `0 ms`;
- each association's row gets the Q of the action it names, TRUE (its action cell filled
  `--c-good-soft`) or FALSE;
- a value that cannot be read is UNKNOWN, never a state (`--c-warn`, dotted): a step whose X
  cannot be read is UNKNOWN, not inactive; an active step whose T cannot be read is active with
  its time UNKNOWN; a transition whose `ran` cannot be read (or whose probe has none) is UNKNOWN,
  whatever its condition's variable holds; a state the check names no probe for is UNKNOWN; when
  the runtimes or the values cannot be read every state is UNKNOWN; a forced value (a step's
  flag, a condition, an action's variable) is marked FORCED where it is drawn.

**The Sequential Function Chart editor** (L3d-3; `SfcEditor.tsx` beside the view, and
`sfcmodel.ts`, `sfcedit.ts`, `SfcInstructions.ts`; the splicing and the record of localIds are LD's,
§2.15). The owner's decisions hold: TIA Portal's workflow and conventions as closely as they can be
followed, for a chart S7-GRAPH's sequence editor (a step and a transition put as a pair, alternative
and simultaneous branches, jumps, the end of a sequence; a transition's condition and a step's
actions edited where they are selected; the initial step marked; online monitoring the view's);
artwork of our own (no artwork, icon or branding of Siemens), IEC 61131-3's symbols (§6.7), and no
code of TIA Portal, CODESYS, Beremiz, OpenPLC, 4diac or STruC++ read or copied. The visual design is
the owner's to lead: the editor uses the workbench's classes and tokens (LD's `.ld-*`, and a few
`.sfc-*` for layout only).

- **Who, and what is saved**, as LD's and FBD's: an engineer gets the editor, a viewer and an
  operator the view; unsaved until Save (or Ctrl+S); Load saves first. Every edit changes the model,
  which is written back into the TC6 text at once; the chart is drawn by the view's `SfcPouView`
  from that text, with the view's live state (on the PROGRAM only), the view's diagnostics and its
  list of what it does not draw, and the POU's own actions and transitions under it. Over the
  drawing the editor draws the selection and its own marks of what is wrong. A document with
  several POUs in SFC has a tab for each.
- **A chart is a sequence** (`sfcmodel.ts`): steps, transitions, selections (alternative branches,
  each starting with a transition), simultaneous constructs (branches each starting with a step)
  and jumps, the alternation of §6.7.5 kept by construction. *Decided, and why:* every structure
  operation is checked by `wellFormed` on what it leaves (steps and transitions alternate; a chart
  starts at a step and does not end at a transition, which must lead to a step; a selection follows
  a step and, where a branch meets its convergence, is followed by a step or a jump; a simultaneous
  construct follows a transition and is followed by one, every branch ending at a step that meets
  its convergence; a jump ends its sequence) and refused with the reason otherwise, so the editor
  never writes a chart the compiler would refuse for its structure, which is what Table 61 allows;
  whether a chart is safe and every step reachable (§6.7.5, Figure 26) stays the compiler's to say,
  drawn at the element it names, since only its exploration of the situations can.
- **The structure operations**, S7-GRAPH's: **a step and a transition** after the selection (after
  a step a transition and then a step; after a transition a step and then a transition); **an
  alternative branch** at a transition, beside it from the step before it to the next step (at the
  first transition of a branch, one more branch beside it; at a step, at the transition after it;
  at the last step of a sequence, two transitions meeting at a new step); **a simultaneous branch**
  at a step right after a transition, beside it up to the next transition; **a jump** after the last
  step of a sequence (with its transition) or at the last transition of an alternative branch, to
  the chart's initial step, never out of a simultaneous branch (its other branches would stay
  active, unsafe); **the end of a sequence** in place of a jump: IEC 61131-3 has no element for
  S7-GRAPH's sequence end, so it is a step no transition follows (it stays active once reached);
  **a new sequence** with its initial step beside the others. Delete takes a step or a transition
  with the element before it (the inverse of putting a pair after it), with the one after it at the
  head of a sequence or where the one before is the initial step; a jump with its transition; a
  branch whose only element it was goes with it, and a construct left with one branch becomes that
  branch; a divergence keeps its first branch that meets its convergence. A branch can be moved left
  or right: in a selection that is the order its transitions are tested. A new transition's
  condition is `FALSE`, so nothing moves on through a transition nobody has written (a safety HMI);
  a new step is `Step<n>`, as S7-GRAPH numbers them, never a name a step, action, transition or
  variable of the POU has, a jump goes to, or any text of the POU reads (found in review: a name
  freed by a deletion or a rename was given to the next new step, and a dangling jump or `Old.T`
  silently went to it). An edit that would leave a chart without its initial step is refused; a
  pair deleted with the initial step makes the chart's head initial.
- **Fields**, under the chart for the selection: a step's name (a name another step, an action, a
  transition or a variable of the POU has is refused) and whether it is the initial step (one per
  chart, §6.7.2: making one initial moves the mark; a step in a branch cannot be made initial).
  **A rename follows every reference** (found in review: a rename left `Old.T` in conditions, which
  the compiler accepted once a new step took the old name): a jump to the step, and every `S.X` and
  `S.T` of a condition or an action's statements the editor writes, rewritten token by token (an
  ST tokenizer that steps over comments, strings and typed literals, and does not take `x.Old` for
  the step); while a text the editor does not write reads the step (an action or a transition of
  the POU, a body in LD or FBD, a network drawn in the chart), the rename is refused, saying where; its **action
  table**, as S7-GRAPH's, a row per association: the qualifier (Table 59's: N, R, S, L, D, P, SD, DS,
  SL, P1, P0, with SD, SL and P0 as §2.17 decides; one Table 59 does not have, found in a document, is
  shown as refused), the
  duration where the qualifier takes one (T#1s proposed), and what it runs: an action of the POU or
  a BOOL variable with completion, declared where it is named (Ctrl+Enter, a BOOL output proposed),
  or ST statements written in the block. A transition's condition: an ST expression written in it
  (an expression has no side effect, §6.7.3; a function block call or an assignment cannot be
  written in one), or a transition of the POU by its name; negated or not. A jump's target step. A
  divergence: one more branch, or all but the first that meets deleted. Each with what is wrong,
  the editor's own word at once (a step's name, a condition with nothing written or naming a
  transition the POU does not have, a jump to a step that does not exist, an action naming nothing,
  neither an action nor a variable, or a variable that is not a BOOL, a qualifier that takes a
  duration without one or one that takes none with one, a step's `.X` or `.T` read by a name no step
  has) and then the compiler's (the quiet check
  700 ms after the last edit, as LD's), each drawn at its element, an action's at its row.
- **Bodies in LD and FBD** (§2.17, L3d-1b) are kept as found and named with their language: an
  action's or a condition's inline body is written back byte for byte, never by the editor; it can
  be replaced by an ST condition, which drops it. A network drawn in the chart that gives one
  transition its condition is written as found, moved by as much as its transition moved, and
  deleted with its transition.
- **Kept as found.** A chart another tool wrote that is not such a sequence (a link back that is
  not a jump, a step followed by two elements without a divergence, a branch that meets another
  convergence or none, a step with two action blocks, an element the editor does not know such as a
  `macroStep`, two elements with one `localId`, a selection whose priorities the compiler refuses:
  two alike, some given and some not, one not a whole number; found in review, where such a chart
  was written without them in an order of the editor's choosing, a refused document becoming an
  accepted one) is written back byte for byte, its elements not
  touched, says why, and nothing is put into it. A chart's comments are written as found, where
  they are.
- **Layout**, on a grid of 10 in the document's units (the view draws them two and a half times
  as large): each sequence in a column, its axis where its widest step fits; a step's action block
  right of it; branches side by side, a branch as wide as its widest step, action block or
  condition (with room for the state the view writes live); a step 16 high and as wide as the view
  draws its name (both written, so other tools read a step's size); then its band (the step or its
  action block, the taller) and 10 below it the transition, 20 below which the next step; charts
  side by side. Positions are written for every element, so other tools and the view read them.
  *Not written*: a selection's `priority`, dropped once the branches are ordered by it (the order
  the compiler tests them), since the editor draws the branches in that order, left to right,
  which is Table 61 feature 2a and S7-GRAPH's convention, and a branch moved left or right changes
  it. A chart the editor changes is laid out anew whole, as an FBD network is: a chart is one
  connected network.
- **localIds**, as LD's and FBD's (§2.15), and §2.17's constraint for this editor in particular:
  every element, an action of a block among them, keeps its id while it exists; a new one takes the
  first id past every `localId` anywhere in the document (an inline body's own ids included) and the
  record in the POU's `addData` (`urn:mythos:logic:ld-editor`), counted up and never back, so no id
  an element has had is given to another, after an undo, a deletion or in a new session. An inline
  body's values are carried by `__SFC_<localId>__…`, so this is what keeps one body's values, an
  edge's memory among them, out of another at an online change.
- **Online change.** A step's state is carried by its name (§2.17). When the chart's steps or
  transitions (its shape: steps by name and initial, each transition's steps before and after)
  differ from the chart as the editor opened it, the editor says that at the next Load the chart
  starts again from its initial step, every action idle; a change of an action, a condition or a
  position alone carries on where the chart was.
- **Undo and redo** cover every edit, declarations among them. A localId given out is not taken
  back by undo.
- **Keys**, while the editor has the focus: Shift+F2 a step and a transition, Shift+F3 an
  alternative branch, Shift+F8 a simultaneous branch, Shift+F9 a jump, Shift+F7 an N action of the
  selected step; ↑ and ↓ the previous and next element of the chart in the order it is drawn, ← and
  → the first element of the branch beside; Ctrl+R a new sequence; Enter the first field of the
  selection (a step's name, a condition, an action's name, a jump's target); Delete what is
  selected (an element, a row of the action table); Ctrl+Z undo, Ctrl+Y or Ctrl+Shift+Z redo; Ctrl+S
  save; in a name field LD's keys; in a condition or statements, Enter takes the text and
  Shift+Enter is a new line. *Why these*: as for FBD, S7-GRAPH's own keys could not be confirmed
  from a public source, so none is claimed to be TIA's; they are LD's positions given their SFC
  meaning (Shift+F2 the series, a step and its transition; Shift+F3 beside it the parallel, an
  alternative; Shift+F8 LD's branch, a simultaneous branch; Shift+F9 LD's close, the jump that closes
  a sequence; Shift+F7 the coil, an action). The Instructions task card holds the Sequence group (a
  step and a transition, an alternative branch, a simultaneous branch, a jump, the end of a
  sequence), the Actions group (one per qualifier offered) and the Chart group (the initial step, a
  new sequence); the favourites start as a step and a transition, both branches, a jump and an N
  action, kept per viewer apart from LD's and FBD's (`mythos.logic.sfc.favourites`).
- Not built: drawing or editing the LD and FBD bodies of actions and conditions with the LD and
  FBD editors (kept and named with their language); adding, editing or deleting the POU's own
  actions and transitions (`actions`, `transitions`: shown with their text, named from the action
  table and the condition, written as found); editing a field in the drawing itself (the fields are
  under the chart); moving an element by dragging it, copy and paste; a network drawn in the chart
  put by the editor; a chart's comments edited; editing a chart kept as found; adding a POU to the
  document.

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

Firmware for a board family is the VM plus a host for that board. What that firmware is to be,
and Espressif's Xtensa chips, are §2.18 (planned). Before a real board:

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
| What may be a chart | §6.7.1: SFC elements keep state, so only function blocks and programs; a POU partitioned into SFC is so entirely | a body `SFC` in a program or a function block; in a function refused; a chart's actions and transitions only in a POU whose body is SFC (`a_chart_in_a_function_and_actions_in_another_body_are_refused`) | not compared |
| The initial state | §6.7.2: BOOL#1 for the initial step, BOOL#0 for the others, t#0s; error 1: each SFC network has exactly one initial step | as the clause, at a cold start; each chart (a connected set of steps and transitions) with no initial step or two is refused, naming its steps (`the_initial_step_is_active_after_a_cold_start_and_no_other`, `a_chart_without_an_initial_step_is_refused`, `a_chart_with_two_initial_steps_is_refused`) | not compared |
| A step's flag and elapsed time | §6.7.2, Table 54 features 3a, 4a: `***.X`, `***.T`; T reset to t#0s when the step is activated, kept when it is deactivated; footnote a and error 2: a program assigning either is an error | read as `S.X` and `S.T` in conditions and actions; T grows by the scan clock while the step is active, from t#0s at its activation, and holds; a write to either refused at its characters, and each is private to a tag (`the_step_time_counts_from_t0_at_activation_and_holds_its_value_once_deactivated`, `the_program_reads_a_steps_flag_and_time_in_a_condition_and_in_an_action`, `a_program_writing_a_steps_flag_or_time_is_refused`) | not compared |
| Enabled, cleared, and what clearing does | §6.7.5: enabled when all preceding steps are active; crossed when enabled and the condition is TRUE; clearing deactivates all immediately preceding steps, followed by the activation of all immediately following ones | as the clause; a simultaneous divergence activates every branch, a convergence waits for every step above it (Table 61 features 4a, 4c, 4d) (`a_transition_clears_only_when_the_step_before_it_is_active_and_its_condition_is_true`, `clearing_a_transition_deactivates_every_step_before_it_and_activates_every_step_after_it`) | not compared |
| Simultaneous clearing | §6.7.5: transitions that can be cleared simultaneously shall be | every transition that clears in a scan does so at once, judged from the steps as the scan found them (`transitions_that_can_clear_together_clear_in_the_same_scan`) | not compared |
| Propagation before testing | §6.7.5: the successor transitions of an active step are not tested until the effects of its activation have propagated; a step's activity never lasts zero | a scan runs the actions with the steps as they are, then tests the transitions; a step activated in a scan runs its actions in the next before any transition after it is tested, the initial step at the first scan (`a_step_activated_in_a_scan_is_active_for_a_whole_scan_before_the_transition_after_it_is_tested`) | not compared |
| Alternation | §6.7.5: two steps never directly linked, nor two transitions | refused, naming both elements (`two_steps_linked_without_a_transition_between_them_are_refused`, `two_transitions_linked_without_a_step_between_them_are_refused`) | not compared |
| Priority in a selection | Table 61 features 2a (left to right), 2b (numbered branches, the lowest first), 2c (mutual exclusion, the user's to assure); §6.7.5: an error if non-prioritized transitions can be true at once | 2a by the transitions' positions; 2b where every transition of the divergence has a TC6 `priority` (some with and some without refused); TC6 cannot draw 2c, so every selection has a priority and the error cannot arise (`a_selection_divergence_clears_the_leftmost_transition_whose_condition_is_true`, `numbered_branches_clear_the_lowest_numbered_transition_whose_condition_is_true`) | not compared |
| Unsafe and unreachable charts | §6.7.5, Figure 26: the system shall treat them as errors | every situation reachable with any condition TRUE or FALSE explored: activating a step already active, a step never active, a transition never enabled, each refused naming them; more than 100,000 situations refused as not checked (`an_unsafe_chart_is_refused_naming_the_situation_and_the_step`, `an_unreachable_chart_is_refused_naming_the_transition_never_cleared`, `a_step_after_a_transition_that_never_clears_is_refused_as_never_active`, `a_chart_too_large_to_check_is_refused_saying_so`, `a_chart_of_exactly_the_most_situations_checked_is_taken`) | not compared |
| Action control | §6.7.4.6 rules a–e, Figure 22, Figure 23, Table 60: with or without final scan, the implementer's | without final scan (feature 2, Figure 23 b), from the library's RS, TON, R_TRIG: N, R, S, L, D, P, P1, DS. Feature 1's F_TRIG of Q is TRUE at a cold start (Table 44), which would run every action of statements then (§2.17) (`an_n_action_is_active_exactly_while_its_step_is`, `an_s_action_stays_active_after_its_step_until_an_r_association_resets_it`, `an_l_action_is_active_for_its_duration_at_most`, `a_d_action_starts_after_its_duration_and_only_while_its_step_is_active`, `a_p_action_is_active_for_one_scan_when_its_step_becomes_active`, `a_ds_action_is_stored_only_if_its_step_lasted_its_delay`, `an_action_of_statements_runs_while_it_is_active_and_its_flag_says_so`) | not compared |
| The T input of action control | §6.7.4.6 rule c: the duration of an active time-related association, t#0s if none; rule d: an error if two are active at once | as rule c; two time-related associations of one action whose steps can be active together refused (`two_time_related_associations_of_one_action_active_together_are_refused`, in one step; `two_time_related_associations_in_two_steps_active_together_are_refused`, in two simultaneous sequences) | not compared |
| SD, SL and P0 | Table 59 features 8, 10, 12; by rule c and Figure 23 an SD action whose step is left before its delay starts at once, an SL one ends at once; P0's F_TRIG is TRUE at a cold start (Table 44) | **what Table 59 names, not Figure 23 at these three edges** (the owner's decision, §2.17): each SD and SL association is stored and timed by itself with its duration kept once its step is left, so the delay and the limit run in full (`an_sd_action_starts_its_full_delay_after_it_was_stored_whether_or_not_its_step_was_left`, `an_sl_action_runs_its_full_limit_whether_or_not_its_step_was_left_and_no_longer`, `an_sd_or_sl_action_keeps_the_duration_a_variable_had_while_its_step_was_active`, `an_r_association_ends_an_sd_delay_and_the_next_one_runs_in_full`); in doubt the action is off (`an_sd_action_two_steps_store_starts_only_when_the_delay_of_each_is_over`, `an_sd_action_that_has_started_stays_active_until_r_whatever_stores_it_again`, `an_sd_step_entered_again_while_its_delay_runs_starts_the_delay_again`, `an_sl_action_ends_at_the_first_limit_that_is_over_and_stays_ended_until_r`); P0's edge remembers FALSE at a start (`a_p0_action_is_active_for_one_scan_after_its_step_was_left_and_never_at_a_start`, `a_cold_start_drops_an_sd_delay_that_was_running_and_runs_no_p0_action`) | CODESYS's help says the same of SD and SL (§2.17); P0 not compared |
| A BOOL variable as an action | Table 56 feature 1 (of VAR or VAR_OUTPUT); §6.7.4.1: an error if it is modified other than as the action | its value is Q; one of another type or section refused; a write to it in an action refused (`a_variable_that_is_an_action_written_elsewhere_is_refused`) | not compared |
| An indicator variable | §6.7.4.4: deprecated | refused (`an_indicator_variable_is_refused_as_deprecated`) | not compared |
| A step's time across a restart | not addressed | as the timers' (above): the scan clock starts again from zero, a step's time carries on from what it was, the restart not counted (`the_step_times_do_not_count_the_time_across_a_restart`) | not compared |
| A chart taken over by an online change | not addressed | carried by the steps' names when the chart's steps and transitions are the same; otherwise started again from its initial step (§2.17) | not compared |
| An action in LD or FBD | §6.7.4.1: an action can be a collection of rungs in LD or of networks in FBD; Table 56 features 2l, 2f; Table 57 feature 4, Table 58 features 4l, 4f | of the POU or in its action block, lowered by the LD or FBD front-end and run while its Q is TRUE, without final scan; a return refused, a write to a step's or an action's flag refused (§2.17) (`an_action_in_ld_runs_only_while_its_q_is_true_and_without_a_final_scan`, `an_action_in_fbd_written_in_its_action_block_runs_while_its_q_is_true`, `an_ld_action_that_returns_or_writes_a_steps_flag_is_refused`) | not compared |
| A condition in LD or FBD | §6.7.3 b, c, d, g; Table 55 features 2a, 3a, 4a–6a, 10, 11: a network adjacent to the transition, reaching it via a connector, or a TRANSITION whose network assigns the transition's name; an error if any side effect occurs during its evaluation | the network's coil or output variable of the transition's name (the POU's transition, or the `name` of the inline body) gives the condition; a network drawn in the chart gives it where it reaches the transition, through a connector too. Evaluated only where the transition is enabled. A write to anything else, a function block's call, an edge, a jump, an element not leading to the result refused (§2.17) (`a_condition_in_ld_is_the_power_reaching_the_coil_of_the_transitions_name`, `a_rung_drawn_in_the_chart_gives_the_transition_it_reaches_its_condition`, `an_fbd_network_drawn_in_the_chart_reaches_its_transition_through_a_connector`, `a_condition_network_is_evaluated_only_where_its_transition_is_enabled`, `a_condition_network_that_writes_anything_but_its_transitions_name_is_refused`) | not compared |
| An action that is a chart | §6.7.4.1, Table 56 feature 2s: SFC elements included in an action; what becomes of them when the action stops is not said | refused with the reason; a function block with an SFC body called from the action is taken (§2.17) (`an_action_in_il_or_sfc_is_refused_with_the_reason`) | not compared |

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
- **OSCAT next.** The owner decided on 2026-11-24 that OSCAT comes as a library package
  (ADR-015), not inside the compiler. It is now an Eclipse project under EPL-2.0
  (https://eclipse.dev/oscat/); its license must be checked before any of its source is
  copied in.

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
    source, a task in the logic module); `actions` and `transitions` must be empty (read since
    L3d-1 in a POU whose body is SFC, §2.17).
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
  element. FBD does the same since L3c-1 (§2.16), SFC since L3d-1 (§2.17). (L3b-1 published only the
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
- **What the FBD editor (L3c-2) needs** (built, §2.8, "The Function Block Diagram editor": a
  network is one tree laid out by the editor; connectors, continuations and a line drawn back are
  kept as found, not drawn, and are the CFC mode's). Placing, moving, wiring and deleting blocks,
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

### 2.17 The Sequential Function Chart front-end (L3d-1)

**Decided by the owner (2026-11-20):** SFC lowers, as LD and FBD do, to the ST front-end's syntax
tree; online monitoring as TIA Portal gives it, so the compiler always keeps each step's state
(its flag X and elapsed time T), each transition's condition when it is evaluated and each
action's state in hidden private variables, in every build; step `.X` and `.T` read by the
program as the standard defines them; the action qualifiers with the action control of the
standard; RETAIN and online change by the existing rules; unsafe and unreachable charts decided
by reading the standard; where implementations and the standard differ, the standard. The view
is L3d-2 and the editor L3d-3. Everything below was read in IEC 61131-3 ed. 3 §6.7 (Tables
54–61, Figures 22–26) on 2026-11-20; the TC6 names in the schema of §2.15.

- **In the one graphical crate.** `crates/mythos-logic-graphic/src/sfc.rs`, beside `ld.rs` and
  `fbd.rs`, for the reason FBD went there (§2.16): a TC6 document is one project whose POUs each
  have a body of their own language, a chart may be a function block that an LD or FBD program
  calls (`tests/sfc/doors.xml`), and the interfaces of every POU are read before any body, in
  one pass. `read.rs` takes a body `SFC` in a program or a function block (§6.7.1: the state of
  steps needs storage, so never in a function) and reads the POU's `actions` and `transitions`
  there; in a POU of another language they are refused. `Language::Sfc` compiles a document as
  `Language::Ld` does. `layers.rs` is unchanged: no crate was added.
- **The subset taken.** `step` (`name`, `initialStep`), `transition` (its condition `inline` in
  ST, LD or FBD, a `reference` to a transition of the POU, or a `connectionPointIn` from a
  network drawn in the chart, L3d-1b, below; `negated`; `priority`),
  `selectionDivergence`, `selectionConvergence`, `simultaneousDivergence`,
  `simultaneousConvergence`, `jumpStep` (`targetName`), `actionBlock` (each `action` with its
  `qualifier`, `duration`, and a `reference` to an action of the POU or to a BOOL variable, or an
  `inline` body in ST, LD or FBD), `comment`; the POU's `actions` (a body in ST: statements, or
  LD or FBD networks) and `transitions` (a body in ST: one Boolean expression, or an LD or FBD
  network writing the transition's name); the elements of LD and FBD in the chart's body, as
  networks giving transitions their conditions (L3d-1b, below). An ST text of several lines is read as
  ST reads it, each of its lines a line of the syntax tree, and a diagnostic in it gives the line
  of the text (`text.line`, below). Refused by name with the element: an action or a condition
  in IL (this compiler has no IL front-end, §2.1); an action that is a chart (SFC, Table 56
  feature 2s, below) and a condition in SFC (a condition is one Boolean value, §6.7.3);
  `macroStep` (a TC6 element that is not in ed. 3); any other element of the schema
  (`vendorElement`, …); the qualifier `DL` (not in Table 59); an indicator variable (§6.7.4.4: deprecated); a negated step or action block; RETURN
  in an action (it would end the whole chart's scan before its transitions).
- **The chart as a net.** Each transition is resolved through the divergences, convergences and
  jumps to the steps before it and the steps after it. A step, a transition, a step always
  alternate (§6.7.5): two steps or two transitions linked directly are refused, naming the
  elements, and so is a link a divergence or convergence does not take (Table 61), a step with
  two links into it (they join in a selection convergence), a step followed by two elements
  (they hang from a selection divergence), a transition that leads to no step, a jump to a step
  the body does not have. Each chart (a connected set of steps and transitions) has exactly one
  initial step (§6.7.2 error 1); steps, actions and transitions have names of their own, apart
  from the POU's variables (their scope is the POU, §6.7.2–§6.7.4).
- **Unsafe and unreachable charts** (§6.7.5: the system shall treat them as errors; Figure 26).
  Every situation (set of active steps) the chart can reach from its initial steps is explored
  as if each condition could be TRUE or FALSE in any scan. Refused, naming the elements: a
  situation where clearing a transition would activate a step that is active already and that
  it does not deactivate (unsafe, with the situation and the step); a step no situation makes
  active; a transition whose preceding steps are never active together (unreachable). The
  structure is judged, as Figure 26 judges it: a chart that only its conditions would keep safe
  is refused too. The exploration checks at most `MAX_SITUATIONS` (100,000) situations, a chart of exactly
  that many included (`a_chart_of_exactly_the_most_situations_checked_is_taken`), and refuses what it could
  not check, saying so: 18 simultaneous sequences of two steps each (3^18 situations) are
  refused; the charts of the tests and the corpus are explored whole in well under a second.
- **A scan, in order.** (1) Every active step's time grows by the scan clock since the last
  scan (none across a restart: the clock starts again at zero, as for the timers, §2.13). (2)
  Every action runs under its action control, with the steps as they are. (3) Every enabled
  transition's condition is evaluated (one that is not enabled is not evaluated: a condition
  has no effect but a fault, and it is not consulted). (4) Every transition that clears does so
  at once, judged from the steps as the scan found them: the first of a selection by priority;
  each deactivates every step before it and then activates every step after it, whose time
  starts from `T#0s`. So a step activated in a scan runs its actions in the next scan before any
  transition after it is tested, and is active for one scan at least: the rule that testing waits
  until the effects of an activation have propagated (§6.7.5), the initial step included. An
  initial step's time counts from the first scan of its chart's instance (`__SFC_STARTED`, FALSE
  until then): an instance added by an online change, or first called from a step entered an
  hour after the runtime started, does not take the hour as its initial step's time (found in
  review: `a_chart_instance_added_by_an_online_change_times_its_initial_step_from_its_first_scan`,
  `a_chart_instance_first_called_late_times_its_initial_step_from_its_first_call`); a step's
  time counts from the scan that activated it. A step whose transition leads back to itself (a
  jump step to it) is deactivated and activated in one scan: its time starts again, while its
  flag stays TRUE, so the action control, which sees only the flag (Figure 23 b), does not
  pulse a P action again nor restart an L action's limit; that is the letter of Figure 23, and
  is pinned (`a_step_that_loops_to_itself_restarts_its_time_and_keeps_its_actions_as_figure_23_draws`). Actions run in the order of the steps they are associated with (top to
  bottom, then left), each once.
- **Action control** (§6.7.4.6): the functional equivalent of ACTION_CONTROL **without final
  scan** (Table 60 feature 2, Figure 23 b), built from the standard library's RS, TON and R_TRIG,
  only the parts an action uses (rule e): S_FF, L_TMR, D_TMR, P_TRIG, DS_TMR and DS_FF, P1's
  R_TRIG; `Q := NOT R AND (N OR S_FF.Q1 OR (L AND NOT L_TMR.Q) OR D_TMR.Q OR P_TRIG.Q OR
  DS_FF.Q1 OR P1_TRIG.Q)`. An action that is a BOOL variable is Q (Table 56 feature 1); an action
  of statements runs while Q is TRUE and reads its flag as `ACTION.Q` (rule a). *Why not with
  final scan (feature 1, which the standard draws first):* its A is `Q OR F_TRIG(Q).Q`, and an
  F_TRIG is TRUE at its first execution after a cold start when its input is FALSE (Table 44 and
  its note, which §2.13 follows for F_TRIG): every action of statements would run once at a cold
  start, whatever its step, writing what it writes with no step active. Table 60 makes the two
  features the implementer's choice. The T input is the duration of the active time-related
  association, `T#0s` when there is none (rule c). Two time-related associations of one action
  whose steps can be active together are refused, from the same exploration (rule d: an error).
- **SD, SL and P0 do what Table 59 names; at three edges that is not what Figure 23 does** (the
  owner's decision, 2026-11-22; they were refused by name until it was taken, a safety question
  and not a technical one). The standard disagrees with itself there. Rule c gives T `T#0s` once
  no time-related association is active, so by Figure 23 an SD action whose step is left before
  its delay starts at once (SD_TMR's PT drops to zero while SD_FF holds IN), and an SL action
  whose step is left before its limit ends at once; and P0's pulse is an F_TRIG, which by Table
  44 is TRUE at its first execution, so a P0 action would run at a cold start whatever its step.
  Each is the dangerous direction: a delay is most often a margin (a purge before an ignition),
  and an action that runs when the controller starts has no cause in the process. Table 59 calls
  them stored and time delayed, stored and time limited, and pulse on the falling edge, and that
  is what is built:
  - **Each SD and each SL association is stored and timed by itself.** An association has a
    flip-flop set by its step and reset by R, a TON, and a duration of its own
    (`__SFC_<action>_SD_<localId>_FF`, `_TMR`, `_KEPT`; `_SL_` likewise), read each scan its
    step is active (so a variable is read while the step is active) and kept from then on. So
    the delay and the limit count in full, the step left or not, and no other association of
    the action, of any qualifier, changes them (Figure 23 has one SD_TMR and one SL_TMR on the
    one T of rule c). Rule d is unchanged and counts SD and SL: two time-related associations
    whose steps can be active together are refused.
  - **In doubt, the action is off.** Two SD or two SL associations of one action in steps
    active one after the other, or a step entered again with no R between, leave two readings
    of "the delay" and "the limit". The one with the action off is built. An SD action starts
    when no association that stored it is still in its delay, each delay counted from the last
    time its step was entered (a step entered again while its delay runs starts that delay
    again, `__SFC_<action>_SD_<localId>_WAS`; entered is the step's flag rising, so a step
    that loops to itself, whose flag never falls, is not entered again for its action control,
    for SD as for D and every qualifier
    (`a_step_that_loops_to_itself_restarts_its_time_and_keeps_its_actions_as_figure_23_draws`,
    `a_step_that_loops_to_itself_is_not_entered_again_for_an_sd_delay_or_a_p0_pulse`); once
    started it stays so until R
    (`__SFC_<action>_SD_START`), whatever stores it again and whatever a duration read from a
    variable becomes. An SL action ends at the first limit that is over, each limit counted
    from when its step first stored the action (a step entered again does not start it
    again); once ended it stays so until R (`__SFC_<action>_SL_END`). With one association,
    a constant duration and a step not entered again while stored, both are Figure 23's SD and
    SL with the duration kept.
  - **P0 remembers FALSE at a start.** The pulse is `NOT P0 AND was`, `was` the P0 input of the
    scan before, FALSE at a cold start and when the chart starts again: no step was active
    before, so none was left. The action is active for one scan, the scan after its step was
    left (as P and P1 in the scan after it was entered). Several P0 associations of one action
    are one input, their OR, as Figure 23 has them and as P and P1 are: a chart that goes from
    one such step straight into another gives no pulse until the last of them is left
    (`several_p0_associations_of_an_action_are_one_falling_edge_as_figure_23_has_them`); nor
    does a step that loops to itself, whose flag never falls.
  - **What stays Figure 23's:** R ends each at once and overrides Q; nothing of the action
    control is retained, so a cold start drops a delay that was running and the action does not
    start (`a_cold_start_drops_an_sd_delay_that_was_running_and_runs_no_p0_action`), and so
    does a chart started again by a build of another chart.
  - **Compared:** CODESYS's help (CODESYS SFC, "Qualifiers for Actions in SFC", read 2026-11-22)
    says of SD that the action starts when the delay has passed after the step became active and
    runs until a reset, and of SL that it ends when the time has passed or earlier by R, both
    running on after the step is inactive: the names' meaning. P0 was not compared.
- **Values kept for monitoring.** Each step is a variable of the POU by the step's name, a
  structure of `X` (BOOL) and `T` (TIME): `__SFC_STEP`, or `__SFC_INITIAL_STEP` whose `X`
  starts TRUE (§6.7.2: BOOL#1 for initial steps, BOOL#0 for the others, t#0s); so the program
  reads `S.X` and `S.T` as Table 54 writes them, and a host watches them by those paths. Each
  action of the POU is a variable by its name with `Q` (`__SFC_ACTION`); an action written in an
  action block keeps Q in `__SFC_<localId>_Q`; each transition keeps its condition in
  `__SFC_<localId>`, assigned where it is evaluated, and `__SFC_<localId>_EN`, TRUE where it
  was enabled; the action control's blocks are `__SFC_<action>_<part>` (`_S_FF`, `_L_TMR`, …)
  and `__SFC_<action>_T`; `__SFC_CLOCK` and `__SFC_DT` the scan clock's last value and step, `__SFC_STARTED` whether the
  instance has been scanned.
  Every one is a private symbol (`Var::hidden`): no tag is bound to write a step's flag or time,
  which Table 54 footnote a calls an error, and the front-end refuses an action's text that
  writes one (`Idle.X := TRUE` in an action is refused at its characters), or a variable that is
  an action (§6.7.4.1), or an action's flag. The two structure types have names a program cannot write (§6.1.2).
- **Probes.** `lower_with_probes` gives, for each step, `At::Step` (`S.X`) and `At::StepTime`
  (`S.T`); for each transition `At::Transition`, `Holds::Guarded { path: __SFC_<id>, ran:
  __SFC_<id>_EN }` (where `ran` is FALSE the transition was not enabled and the variable holds an
  earlier scan's value, never this one's); for each action of an action block, by the action's
  own `localId`, `At::Action`, the variable keeping the Q of the action it names. The logic
  module gives the PROGRAM's in `check` as `chart` (§2.6): `{local_id, step: "X"|"T"}`,
  `{local_id, transition: true, ran}`, `{local_id, action: "Q"}`, each with `variable` and
  `negated`. The view that draws them is L3d-2.
- **RETAIN and online change.** Each variable above is a symbol like any, so §2.13's rules
  apply unchanged: in a program, unqualified, not retained; in an instance declared RETAIN,
  retained, so the steps' states and times of a retentive function block are retentive, as
  §6.7.2 asks. Online change carries each by its path, which names the step or the element; and
  `__SFC_SHAPE`, a DINT whose initial value is a hash of the chart's steps (names, initial) and
  transitions (the steps before and after each), says whether a carried state belongs to this
  chart: a build of another chart that takes over the variables of a running one starts it again
  from its initial step, every action control idle, at its first scan; a build that changed only
  an action, a condition or a position carries on where the chart was
  (`a_build_of_another_chart_…_starts_it_again_from_its_initial_step`,
  `a_build_of_the_same_chart_…_carries_on_where_it_was`). *Why:* carried by name alone, a chart
  given a new initial step before a running one would have two active steps in one sequence,
  which no situation of the new chart is.
- **The scan clock.** A step's time needs the clock, which only the library may name
  (`__NOW()`, §2.14); the front-end writes it as `ast::ExprKind::Intrinsic(Intrinsic::Now)`, a
  node no ST text can give (as `TypeSpecKind::OfValue`, §2.16), which the checker types as the
  intrinsic. The VM is unchanged: it is `NOW` of format 4.
- **Places.** As LD's and FBD's (§2.15): a diagnostic names the element by its `localId` and
  position (an action of a block by its own `localId`), the characters of its text (`name`,
  `condition`, `duration`, `reference`, `body`), and, for an action or a transition of the
  POU, `element.action` or `element.transition` by its name; a text of several lines gives its
  line as `text.line` (from 1). Both are new fields of `mythos_logic_ir::Element` and
  `TextRange`, absent where they do not apply, so what LD and FBD give is unchanged.
- **Actions and conditions in LD and FBD (L3d-1b).** Read in IEC 61131-3 ed. 3 §6.7.3 (Table
  55) and §6.7.4.1–§6.7.4.3 (Tables 56–58) on 2026-11-20, and in the TC6 schema (§2.15): an
  action's `body` and an action block's `inline` are a `body` (IL, ST, FBD, LD or SFC); a
  transition's `condition` is a `reference`, an `inline` body with a required `name`, or a
  `connectionPointIn`; and the `SFC` element of a body may hold LD's and FBD's elements. Taken:
  - **An action in LD or FBD** (Table 56 features 2l, 2f; Table 57 feature 4 with Table 58
    features 4l, 4f), of the POU or written in its action block: its networks lowered by the LD
    or FBD front-end, unchanged (`ld::lower`, `fbd::lower`), into the chart's POU, and run as an
    action of statements is, `IF Q THEN … END_IF`, under the action control without final scan
    (above). A return is refused (it would end the whole chart's scan before its transitions); a
    jump stays inside the body. A write to a step's flag or time or to an action's flag, by a
    coil, an output variable or a block's output, is refused as in ST, at the element's text.
    An edge in an action senses from the last scan the action ran: its memory is set only where
    the body runs, as every statement of it is
    (`an_edge_in_an_ld_action_senses_from_the_last_scan_the_action_ran`, and
    `an_edge_in_an_ld_action_sees_a_rise_made_while_its_action_did_not_run`, which a memory set
    every scan fails).
  - **A condition in LD or FBD**, of the POU (Table 55 features 10, 11: TRANSITION … FROM … TO
    …, a network whose coil or output variable is the transition's name) or written in the
    transition (features 2a, 3a; TC6 `inline`, whose `name` is the name the network writes, as
    §6.7.3 g has it: the assignment of a Boolean value to the variable denoted by the transition
    name). The network writes its result to a hidden BOOL, `__SFC_<owner>__RESULT`, in place of
    the name, and the transition takes it, negated when the transition is. §6.7.3 calls a side
    effect during the evaluation of a condition an error, so refused, naming the element: a
    write to anything but the transition's name (a coil, an output or in-out variable); a call
    of a function block (it changes the instance); an edge (it keeps what it last sensed, and a
    condition is evaluated only while its transition is enabled, so it would sense from then,
    not from the last scan); a coil of the name that sets, resets or senses an edge; no coil or
    output variable of the name, or two; a label, jump or return; an element that does not lead
    to the result (a condition is one network giving one Boolean value); a feedback path (FBD's
    line drawn back, §8.1.5.2), which carries a value from one evaluation to the next exactly as
    an edge's memory does (found in review). **A result computed only under a function's EN is
    refused** (found in review: an output variable fed from a function under EN kept the value
    of an earlier enabling, so the transition cleared on it; negated, it cleared at the first
    scan on the initial FALSE of a result never computed). *Decided, and why not a reset:*
    §6.7.3 makes the condition the result of evaluating one Boolean expression, and where EN is
    FALSE the function does not run (§6.6.1.5 rule 4; §2.16: not computed). Setting the result
    FALSE first would make a negated transition clear on a value nobody computed, and a
    condition that was not computed must never clear a transition; so the network must assign
    its result in every evaluation, checked on its lowered statements (the assignment at their
    top, not only inside an `IF`), and refused at the element that takes it otherwise. A coil
    always assigns it: its power where a function did not run is OFF by §2.15's rule, a value
    of this scan, so an LD rung ending at a coil of the name is taken
    (`an_ld_condition_ending_at_a_coil_takes_no_power_where_its_function_did_not_run`), and a
    negated transition then takes that OFF as any negated contact would. An inline name is a
    name of the POU, its own as a step's or a transition's is (one that is a variable's is
    refused), and an inline body in LD or FBD with no name is refused. A transition of the POU in
    LD or FBD named by two transitions is refused: a transition name stands for one TRANSITION …
    FROM … TO … construct (feature 9a), and its network's values are kept once.
  - **A network drawn in the chart** (Table 55 features 2a–6a: adjacent to the transition, or
    reaching it via a connector; TC6 lets a chart's body hold LD and FBD elements and a condition
    be a `connectionPointIn`). Decided on reading both, which allow it: taken. Each transition
    whose condition is connected takes the network that reaches it, element by element; a
    connector and its continuations are one line first (§8.1.3, FBD's rule), so a network may
    reach its transition through a continuation (features 4a–6a). A network with a power rail, a
    contact or a coil is LD, any other FBD. Where it reaches the transition the chart adds a coil
    (LD, so several links into it are the OR of §8.2.3) or an output variable (FBD) of the
    transition's `localId`, which takes the result; the network is then checked and lowered as a
    condition's (above). Refused, naming the element: one that leads to no transition's
    condition, or to two (a network gives one transition its condition; a power rail belongs to
    each network that reaches it, §8.1.4); a connection from a step (Table 54 feature 3b: a
    step's flag is read by its name, `S.X`, feature 3a).
  - **Where each is evaluated.** An action's body where its Q is TRUE; a condition's network
    where its transition is enabled, inside `IF __SFC_<id>_EN`, so, as an ST condition, it is
    not evaluated where it is not consulted
    (`a_condition_network_is_evaluated_only_where_its_transition_is_enabled`: 100 / 0 faults
    only where the transition is enabled).
  - **Values kept for monitoring.** Each body keeps what LD and FBD keep (§2.15, §2.16), in a
    namespace of its own, since its `localId`s are its own and every body shares the chart's
    POU: `__SFC_<owner>__` in place of the two leading underscores, the owner the action's or the
    transition's name in upper case, or the `localId` of the action of the block or of the
    transition whose inline body or drawn network it is (`__LD_5` of the action Fill is
    `__SFC_FILL__LD_5`; `__NET_5`, `__EDGE_5`, `__FBD_5_OUT`, `__JUMP` likewise). No name has
    two underscores together (§6.1.2), so a namespace ends where they are and no two meet. Each
    is private, retained and carried by online change by its path as any hidden variable
    (§2.15): a build that keeps an action's name and an element's `localId` keeps its memory
    (`an_online_change_carries_an_ld_actions_edge_memory_by_its_path`). **A network of a body
    that did not run reads as not run**: every flag of a body's networks is set FALSE each scan
    before the `IF` that may run it, so a network of an action whose Q was FALSE, or of a
    condition whose transition was not enabled, never shows as executed, and its kept values are
    an earlier scan's, which the flag says
    (`a_network_of_an_action_that_did_not_run_reads_as_not_run`). So is every `ran` of the
    body (an FBD function's kept ENO, §2.16), which within a run is assigned before it is read:
    a function of a body that did not run is never shown as computed in that scan (found in
    review; `a_function_of_an_fbd_action_that_did_not_run_is_not_shown_as_computed`). The condition itself keeps the
    rule above: `__SFC_<id>`, with `ran` `__SFC_<id>_EN`.
  - **Probes.** `Probe::body` (new; `None` for an element of a POU's own body, so every LD and
    FBD probe is as it was) says which body an element is in: `BodyOf::Action(name)`,
    `Transition(name)`, `Inline(localId)`, `Wired(localId)` (a network drawn in the chart for
    that transition), with whether it is LD (power) or FBD (values). The logic module's `check`
    gives them as `bodies`, a list beside `chart`, each entry in LD's or FBD's shape with
    `body`: `{"action": name}`, `{"transition": name}`, `{"inline": id}` or `{"wired": id}`,
    and `"language": "ld"` or `"fbd"`. `chart` keeps its shape and content, so what reads it
    reads it as before. The element the chart adds where a drawn network reaches its transition
    has no probe: the transition's condition is its own. A diagnostic in an inline body names
    the action of the block or the transition by `Element::inline` (new, absent elsewhere), its
    `local_id` the element of that body; one in an action or a transition of the POU names it by
    `action` or `transition`, as in ST.
  - **Not taken, and why.** An action that is a chart (Table 56 feature 2s): the standard does
    not say what becomes of the inner chart's steps when the action's Q goes FALSE; with the
    action control without final scan they would stay where they were and the chart go on from
    there at the next activation, which no text of §6.7 describes. A function block with an SFC
    body called from the action does the same explicitly, and is taken (`doors.xml`). IL: no IL
    front-end (§2.1). A connection from a step's flag (Table 54 feature 3b). A connector in an LD
    body of the POU or of an action stays refused (§2.15); only a network drawn in the chart
    joins its connectors, as FBD does.
- **Cost, measured** (bytes; private is the number of private symbols: the chart's steps and
  actions, its hidden variables and the members of its action control's blocks; the twins are
  the hand-written ST of `sfc_equals_st.rs`, with the same behaviour and no monitoring):

  | Program | Code | Memory | Private | ST twin: code | ST twin: memory |
  | :--- | ---: | ---: | ---: | ---: | ---: |
  | selections and qualifiers (`sfc_equals_st.rs`) | 2,149 | 238 | 66 | 1,785 | 335 |
  | simultaneous sequences | 1,716 | 143 | 40 | 1,492 | 282 |
  | a chart in a function block, two instances | 931 | 184 | 44 | 719 | 234 |
  | actions and conditions in LD and FBD (L3d-1b) | 1,568 | 129 | 62 | 1,266 | 179 |
  | bottling (`tests/sfc/bottling.xml`, `rp2040`) | 2,640 | 270 | 75 | | |
  | doors (`tests/sfc/doors.xml`: two chart instances under FBD) | 1,048 | 232 | 59 | | |
  | mixer (`tests/sfc/mixer.xml`, `rp2040`: bodies in LD and FBD, L3d-1b) | 1,583 | 157 | 54 | | |

  Re-measured for L3d-1b on `dev` at `0b4a71c` and on its branch, equal for every chart that
  was there: the figures first written here were taken before the review added
  `__SFC_STARTED` (one BOOL and its test, 37 bytes of code). A body in LD or FBD costs what it
  costs in a POU of its own (§2.15, §2.16) and one store per network flag, set FALSE each scan.

  A chart is 13–24 % more code than its twin (the clock, the kept conditions and enable flags,
  the shape check) and less memory (a step's time is one TIME, where the twin needs a TON per
  step). The VM is unchanged, so its firmware is too. The bottling line and the doors run on
  both emulated instruction sets with every scan equal to the host's (all 30 runs of the corpus
  equal, rustc 1.100.0, QEMU 8.2.2, in the CI runner's image).
- **What the view (L3d-2) and the editor (L3d-3) need** (both built, §2.8: the editor keeps every
  `localId`, gives none an element has had to another, keeps a step's name, says when a change of
  the steps or transitions restarts the chart, and keeps each LD or FBD body as found, with its
  `localId`s and an inline condition's `name`). The view draws each step, transition,
  divergence, convergence, jump and action block where the document puts it, with the probes of
  `chart`: a step active or not and its time, a transition's condition only where `ran`, an
  action's Q at each of its associations; UNKNOWN where a value cannot be read, as the LD and FBD
  views do. The editor must keep every `localId` (online change carries `__SFC_<localId>` by its
  path), keep a step's name when the step is moved (its state is carried by its name), and say
  that a change of the chart's steps or transitions starts the chart again at the next online
  change. Since L3d-1b actions and conditions may be LD or FBD: the view draws each body with
  the LD or FBD view and the entries of `bodies` for it (a network not run as not executed;
  L3d-2 names each body's language and does not draw it yet, §2.8), and
  the editor keeps each body's `localId`s (online change carries `__SFC_<owner>__…` by them) and
  an inline condition's `name`. **The SFC editor must never give a `localId` an element has had
  to another element** (as the LD and FBD editors do not, §2.15, §2.16): an inline body's values
  are carried by `__SFC_<localId>__…`, the `localId` of the action of its block or of its
  transition, so an id reused for another action or transition would carry one body's values,
  an edge's memory among them, into an unrelated one at the next online change.

### 2.18 Firmware for microcontrollers (planned: nothing of it is built)

The owner's decisions of 2026-11-22 ("siapkan di ADR untuk Xtensa itu. fitur OTA ga perlu tapi
kita pakai fitur WiFi / ETH untuk update isi VM dan debug kita ya untuk semua platform"), and
what follows from them. §2.9 is what exists today: the VM built for two instruction sets and run
in emulators. No board has run it, and none of what this section describes is written.

- **A controller runs its program itself.** The firmware is the VM (§2.3) and a host for the
  board. The program is kept on the board and scanned there: a board that is powered runs what
  it holds with nothing attached. A PC is needed to compile a program and send it, to watch it,
  and for the tags it shares with the rest of a plant, never to execute it.
- **The link is the network, on every platform: WiFi or Ethernet.** Two things go over it. The
  **program** (what the VM runs: the bytecode with what it needs to start, §2.4's
  `RuntimeProgram`) is replaced over it. And **debugging** goes over it: the state, the watch
  and the force of §2.4 and §2.5, as for a program on a PC. They are the calls of the `runtime` module
  kind (§2.4), carried over TCP by a module on the server's side that speaks to the board, so
  the core, the logic module and the editors see a board as they see any runtime. A cable
  (USB or serial) is how the firmware gets onto the board and how a board is first brought up;
  it is not a second way to run one.
- **The firmware is not updated over the network (no OTA).** Only the program is. New firmware
  goes onto a board by its cable. What follows: a board keeps the bytecode format of the
  firmware it was given, and the compiler refuses a program of another format for it, naming
  both (§2.2: the board's profile says its format). A compiler that makes a newer format than a
  board runs is the case to settle when the first format change after a board exists comes:
  the compiler makes the newest format only today.
- **The platforms.** RP2040 (thumbv6m) and ESP32-C3 (riscv32imc), the two instruction sets of
  §2.9, and Espressif's Xtensa chips: ESP32, ESP32-S2, ESP32-S3.
- **Xtensa is built with Espressif's toolchain.** Rust's own releases do not yet make Xtensa
  code that can be relied on: Espressif's documentation (The Rust on ESP Book, read 2026-11-22)
  says ESP32, S2 and S3 need its fork of the compiler with its fork of LLVM, installed by
  `espup`; rustc lists the targets at tier 3. Nothing of ours is developed for it: the VM is
  `core` alone, with no assembly and no allocator, so it is expected to compile as it is. That
  is an expectation until it is built. A job in CI builds it for `xtensa-esp32-none-elf` and
  `xtensa-esp32s3-none-elf`, prints what it costs, and runs the programs of §2.9 in an emulator
  of those chips against this machine's results, as the two instruction sets of today. It is
  not a required check while its toolchain is outside Rust's releases, and the toolchain's
  version is pinned. Which emulator loads our image, and how, is found out by that work.
- **One interface between the VM and a board.** What the VM needs from a board is small: its
  inputs and outputs, a clock that only goes forward, where the program and the RETAIN
  variables are kept, the link, a watchdog, and the safe values of its outputs. It is one
  interface of ours, implemented once per family of boards, so another family is an
  implementation and no change to the VM. For Espressif's chips the implementation is to stand
  on `esp-hal`, without an operating system; that it serves both the RISC-V and the Xtensa
  chips with one interface, WiFi included, is to be checked before it is relied on. ESP-IDF
  (FreeRTOS, with WiFi, TLS and firmware update ready) is not taken: its largest gain is the
  update over the network that is not wanted, and it puts an operating system under the scan.
- **Cores.** ESP32, ESP32-S3 and RP2040 have two cores, ESP32-C3 and ESP32-S2 one. With two,
  the scan has one and the network the other; with one they share it, and what that does to a
  scan is not measured.
- **Network hardware differs by board** (from the makers' descriptions, to be checked against
  the boards chosen): every ESP32 has WiFi; a Pico has none and a Pico W has; the ESP32 has an
  Ethernet MAC that needs a PHY beside it; the others reach Ethernet through a module on SPI.

**Open, and the owner's to decide** (safety and security, not technique). Each has to be
settled before the step that needs it:

1. **What a board does when its link is lost.** Proposed: it goes on scanning with its own
   inputs and outputs, as a PLC does when its SCADA is gone; every tag it took from the server
   is bad from then on, and the program sees that; and the server treats the board as it
   treats a runtime that is silent (§2.4, §2.5: its outputs elsewhere get their safe values,
   its tags are bad and say why).
2. **Who may send a program or force a variable over the network.** A board that takes a
   program from a network without knowing who sent it must not leave a bench. This belongs to
   the foundation for IEC 62443 (`specs/tasks.md`, "Long-term horizons"); until it is decided,
   the network step below is for a bench network only.
3. **Online change on a board** (§2.5), or a cold start at every new program.
4. **RETAIN variables in flash**: how often they are written, against the wear of the flash.

**Order, when the owner schedules it** (it is after the order of §3 unless he moves it):

- F0. **Xtensa in CI**, in an emulator, not required. Leaves ESP32 and S3 at "runs in an
  emulator" where they are "not built".
- F1. **The interface and a first board**: the board's inputs and outputs, a program put on it
  by cable, and the scan time measured on it for the first time.
- F2. **Standalone**: the program in flash, started at power-up, RETAIN kept, the watchdog, safe
  values.
- F3. **The network link**: the program replaced and the program watched over WiFi or
  Ethernet, as a `runtime` module. Needs the decisions 1 and 2 above.

### 2.19 Task configuration (C8; C8a, C8b and C8c are built, C8d-1 is built, C8d-2 is not)

The owner's decisions (2026-11-18: "program bisa di bind ke runtime tertentu (cyclic / timed /
event)"; "kita ikut bahasa dan fitur siemens, tapi jangan dipisah per OB tapi dibuat settingan
di runtime manager seperti pada codesys"; then: IEC 61131-3's priorities, 0 the highest; a
higher priority preempts a lower one in the VM; a process image per task) and the design that
follows from them. Before C8 a runtime ran one program on one interval.

- **A runtime has tasks, and a task runs programs.** A runtime (one module process, one
  controller) has a list of tasks; each task has a name, a kind, a priority and the programs it
  runs, in the order written. A program is in one task of one runtime. A project made before
  C8 reads as it runs today: each program a runtime of its own with one cyclic task at its
  interval, so nothing has to be migrated by hand.
- **The kinds are Siemens' organisation blocks, set as CODESYS sets tasks.** They are a
  property of the task, chosen in a list, not a block of their own kind that a program is
  written in:
  - `cycle` (Siemens' program cycle; CODESYS' freewheeling): runs again as soon as it has
    finished, no sooner than its minimum cycle time. It is below every priority.
  - `cyclic` (a cyclic interrupt; IEC's INTERVAL): every `interval`, from an optional `phase`.
  - `startup` (OB100): once at every change to RUN, cold or warm, before any other task.
  - `event` (a hardware interrupt; IEC's SINGLE): at the rising edge of a BOOL tag. An edge is
    seen when the core hands the tag over (§2.4's exchange), not when the wire changes: it is
    as fast as an exchange, and the task says so where it is set.
  Time-of-day and time-delay tasks, and tasks that run on an error (time error, programming
  error), are not in C8: a fault puts the runtime in FAULT, as today.
- **Priorities are IEC 61131-3's: 0 is the highest**, 0 to 31 as CODESYS has them. A task that
  becomes ready while a task of lower priority runs preempts it. Tasks of one priority do not
  preempt each other: they run in the order they became ready, then in the order of the list.
- **Preemption is the VM's own, not the operating system's.** One thread schedules every task
  of a runtime. A program's run can be suspended and resumed: what `scan` keeps today in its
  local variables (where it is in the code, its stack, its calls, the instructions counted)
  becomes a value the caller holds, one per task, and the VM runs a bounded number of
  instructions at a time. Between two such pieces the scheduler looks at the clock: if a task
  of higher priority is due, the running one waits where it is. So the order of tasks and what
  a preempted program computes are the same on a PC and on a microcontroller (§2.18), with no
  thread priority asked of any operating system. What it does not give is real time: on a
  general-purpose OS the whole process can still be made to wait (§2.4: intervals of 10 ms and
  above). How long a task of the highest priority waits for a piece to end is to be measured
  when it is built, and is NOT MEASURED until then.
- **A preempted program computes what it would have computed alone.** Programs do not share
  memory: there are no global variables (§2.1: VAR_GLOBAL is refused), each program has its own
  memory, and a task's inputs are fixed for its run (below). So a run that was suspended any
  number of times leaves its memory byte for byte as an uninterrupted run from the same
  inputs and clock. That is the test of the VM's part: every program of the corpus, suspended
  at every piece's end and at random ones, against its uninterrupted run, here and on the
  emulated microcontrollers (§2.9).
- **A process image per task.** When a task starts a run, the inputs of all its programs are
  taken once from the last exchange, so the programs of one task read the same inputs and a
  suspended one does not see them change. When the run ends, the outputs and the published
  variables of its programs are copied to the task's image, and an exchange reads images: it
  never sees a program half-way, which today the lock of the single scan ensures.
- **Time.** A cyclic task's runs are due at fixed times from the change to RUN (no drift). A run
  due while the last one of the same task has not ended is not run: it is counted as an
  overrun, as today. Each task has a watchdog (the time a run may take, ten intervals and never
  less than 2 s unless set), and each run of a program its instruction budget, as today.
  Whichever is exceeded puts the runtime in FAULT.
- **A fault is the runtime's.** A fault in any program stops every task of the runtime and its
  outputs get their safe values (§2.4). A task of its own for an error is not built.
- **What a runtime reports** grows a list of tasks: for each its kind, priority and interval,
  its runs, the last and the longest run time, its overruns, how long ago a run ended, and how
  often it was preempted. A variable is named by its program in a runtime that has several.
- **Where it is set.** The task configuration is part of the project, in the logic module, as
  CODESYS has it in the device tree: a runtime with its module, its target and its tasks, and
  the programs assigned to them, with a route and an MCP tool for each change (ADR-010). As built
  (C8c), the authored runtime is in the logic module's `<project>/logic/tasks.json`, and the
  target is its own: the core's `RuntimeInstance` has no target, and the module compiles each
  program of a runtime for it. The route and tool are those of the whole runtime (`PUT`,
  `DELETE`, `GET`, check and build), not one per change. A program is in one runtime: the
  module refuses a second one (T9) and the core refuses a save in which two instances name one
  project program (`built_from`), which holds for instances built since 0.8.0.
  Settings → Runtimes goes on showing what runs and shows no tasks until C8d-2 (C8d-1 did not
  change that page).
- **Not in C8, by name:** CONFIGURATION, RESOURCE, TASK and `PROGRAM … WITH` written as text
  (the configuration is data of the project; reading and writing the standard's text for it
  can come with an importer); VAR_GLOBAL and data shared between programs, which is what makes
  preemption a question of consistency, and gets its own decision
  ([ADR-014](ADR-014-data-blocks-and-the-data-module.md)); an event from a variable;
  a real-time mode of the operating system (§3, Later).

**Slices**, each leaving `dev` working:

- C8a (done). **The VM**: a run that can be suspended and resumed, and the scheduler as a part of the
  VM's crate that needs `core` alone, with the test of equality above. No runtime uses it yet.
- C8b (done). **The runtime module and the core**: several programs and tasks in one runtime, the
  images, the state per task, the watchdog per task; a project of today runs unchanged.
- C8c (done). **The logic module**: runtimes and their task configuration as data of the
  project (`logic/tasks.json`), with six routes and MCP tools (`list_logic_runtimes`,
  `get_logic_runtime`, `save_logic_runtime`, `delete_logic_runtime`, `check_logic_runtime`,
  `build_logic_runtime`); the build of a runtime of several programs as one instance, with the
  tasks and the union of the data blocks. The module builds and does not load: it cannot call
  the core (ADR-011), so the caller saves the answered instance and removes the ids in
  `replaces`. A program that runs alone is unchanged, and so is a project with no
  `tasks.json`. Evidence: [`28-c8c.md`](ADR-013-evidence/28-c8c.md).
- C8c-core (done). **The core's part of what the review of C8c found**: a program's identity in
  the protocol (`built_from`, `created`), RETAIN values kept and given per program, an online
  change only of the same programs of one project, and the refusal of one project program in
  two runtimes when both instances name it. The logic module fills the identity (0.8.0).
  Evidence: [`29-c8c-core.md`](ADR-013-evidence/29-c8c-core.md).
- C8d-1 (done). **The workbench edits and loads the task configuration**: a Runtimes section
  and its editor in the logic module's UI, Build and Load of a configured runtime with the
  holders of its programs taken out or rebuilt in the same save, the program's Task panel, and
  the HMI tag list of a runtime of several programs. It shows no task state. Evidence:
  [`32-c8d.md`](ADR-013-evidence/32-c8d.md).
- C8d-2. **Each task's state shown**: the state per task, the watch and force of a program in a
  configured runtime and the program editor's live views, and Settings → Runtimes with its
  tasks. The slice of the core's own that it waited for is built (C8d-core): two watchers of one
  runtime with different lists no longer replace each other's list, and a save of the runtimes
  has a revision precondition. C8d-2 itself is not built, and its store must be keyed by list and
  not by runtime id, which `logic-ui` still does (`33-c8d-core.md`).

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

- C8. **Task configuration** (next; decided by the owner, designed in §2.19, not built): a
  runtime's programs run in tasks. The kinds of task are those a Siemens controller has; a
  runtime's tasks are set as CODESYS sets them, per runtime; priorities are IEC 61131-3's, 0
  the highest; a task of higher priority preempts one of lower in the VM; each task has its
  process image. In four slices: **C8a** (done) the VM (a run suspended and resumed, the scheduler),
  **C8b** (done) the runtime module and the core, **C8c** the logic module, **C8d** the workbench (C8d-1 edits and loads, C8d-2 shows the state).
  Between C8b and C8d: data blocks and user types shared between programs
  ([ADR-014](ADR-014-data-blocks-and-the-data-module.md) §4, slices D1 to D4), since the
  workbench of C8d is built once for tasks and for data.

**After L3 and C8** (the owner, 2026-11-18; `specs/tasks.md`, "Long-term horizons"): alarms and
the historian, then the foundation for IEC 62443, then an OPC UA server, each as modules.

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
  read-only with their live values (§2.6, §2.8, §2.13). **L3c-2** (done, §2.8): the FBD editor,
  as TIA's works (§2.16, what the FBD editor needs): standard FBD, each network one tree laid out
  by the editor, an input never given a second line (an OR offered), every localId kept and what
  it does not draw kept as found. **L3d-1** (done, §2.17): Sequential Function
  Chart programs: the front-end in the one graphical crate (steps, transitions, divergences and
  convergences, jumps, action blocks; actions and conditions in ST; the action qualifiers N, R,
  S, L, D, P, P1, DS with the action control of §6.7.4.6 without final scan; unsafe and
  unreachable charts refused), each step's flag and time, each transition's condition and each
  action's state kept in every build; the logic module stores, checks, builds and loads them
  (`language: "sfc"`, `chart` in `check`); the workbench says it does not draw them yet. SD, SL
  and P0 were refused until the owner decided (2026-11-22, §2.17: taken, as Table 59 names
  them). **L3d-1b** (done, §2.17): actions and transition
  conditions in LD and FBD, of the POU, written in the action block or the transition, or (a
  condition) as a network drawn in the chart, each lowered by the LD or FBD front-end in a
  namespace of its own, its values kept for the view (`bodies` in `check`), a condition's with no
  side effect. **L3d-2** (done, §2.8): the SFC view, live, read-only for everyone (IEC 61131-3's
  symbols at the document's positions; each active step with its time, a condition only where it
  was evaluated, each action's state, UNKNOWN where a value cannot be read; an action or a
  condition in LD or FBD named with its language, not drawn yet). **L3d-3** (done, §2.8): the SFC
  editor, as S7-GRAPH's sequence editor works: steps and transitions put as pairs, alternative and
  simultaneous branches and jumps, every structure operation leaving a well-formed chart; the
  action table and conditions in ST edited in place; every localId kept and none given to another;
  a chart it cannot draw and every LD or FBD body kept as found.
- L4. **The standard library in ST** (done, §2.14): the standard function blocks written in
  Structured Text, with intrinsics for what ST cannot say (the scan clock, bytecode format 4);
  their state retained like any variable's. Next: OSCAT, once its license is checked.

**Later:** firmware for RP2040 and Espressif's chips, Xtensa among them, as runtime modules on
real boards (§2.18: its decisions, what is open, and its order F0 to F3); a C backend; a Linux
real-time mode. Medium term (owner, 2026-11-18): a CFC mode for the FBD editor (free placement,
explicit execution order, feedback), stored as FBD in TC6 XML, after standard FBD; see
`specs/tasks.md`, "Long-term horizons".

Each step leaves `dev` working.

## 4. Consequences

- No toolchain is installed with MythOS or on a controller to compile logic: the compiler is Rust
  and part of the logic module.
- A controller runs bytecode, not native code: slower than native code such as STruC++'s
  output (the two were never timed against each other here), and the
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
| 2.1 Crates and layers | `mythos-logic-ir`, `-st`, `-graphic` (LD since L3a, FBD since L3c-1, SFC since L3d-1; `-ld` until L3c-1), `-vm`, `-backend-vm`, `-compiler`; `layers.rs` guards them |
| 2.1 ST front-end | Several POUs in any order: TYPE (STRUCT, named ARRAY), FUNCTION (VAR_INPUT, VAR; positional and formal calls), FUNCTION_BLOCK (VAR_INPUT, VAR_OUTPUT, VAR; nesting; arrays of instances), one PROGRAM; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME, STRING (`STRING[n]`, `STRING(n)`, 254 by default; literals with `$` combinations; assignment that cuts; comparison; LEN, LEFT, RIGHT, MID, CONCAT, INSERT, DELETE, REPLACE, FIND; integer to and from STRING; in functions, function blocks, arrays, structures and a TYPE name); ABS, SQRT, LN, LOG, EXP, SIN, COS, TAN, ASIN, ACOS, ATAN, ATAN2, EXPT and `**`, TRUNC_*, SEL, MAX, MIN, LIMIT, MUX, and ADD, SUB, MUL, DIV, MOVE, GT, GE, EQ, LE, LT, NE by name; arrays of any dimensions with initial value lists; whole-structure and whole-array assignment; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between the elementary types; implicit widening per Figure 11; recursion refused naming the cycle; a keyword of the standard refused as a name; RETAIN and NON_RETAIN on VAR, VAR_INPUT and VAR_OUTPUT of a PROGRAM and a FUNCTION_BLOCK, each symbol saying whether it is retained (`Symbol::retain`, `"retain": true` or absent; C7b-1, §2.13; `retentive_variables.rs`), refused where nothing can be retained; kept across a warm restart by the runtime and the core (§2.5, C7b-2); the standard function blocks from a library written in ST, only those a program uses (§2.14, L4); a name with two underscores together or one at its end refused (§6.1.2); diagnostics for everything else (enumerations, subranges, WSTRING, CHAR, VAR_IN_OUT, methods, and more: each refused by name); every independent mistake of a source in one pass, each once, with the range it is about (§2.8); a body or a type nested more than 100 deep refused, a structure that contains itself told before anything follows it (§2.8) |
| 2.2 Profiles | `serde` data with a format version (4 since L4; a profile of another is refused, naming both); `pc`, `rp2040`, `esp32` stand in; refusals (type, memory, format, stack depth, call depth) proven with profiles read from JSON; a profile written before the depths existed still loads and states no limit |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; format 4 (format 1 with strings and the numeric functions, L2c, with its own real functions, §2.3, and the scan clock `NOW`, L4), and formats 1, 2 and 3 read as they were written, their native standard blocks included (a VM reads every format it extends; an instruction newer than a program's format refused at load): bodies, call frames, instance-relative and region-confined addressing; loader checks, with every jump landing on an instruction of its body, and `load_within` a host's limits; run-time checks, budget; the stack and the calls stop at the depths the program declares; faults, with `IndexOutOfRange`, `CallDepthExceeded`, `StackOverflow`, for strings `StringTooLong`, `StringArgument`, `BadString`, `BadNumber`, and `SelectorOutOfRange` |
| 2.4 Writes to field devices (C1) | Built: `IoStatusHandle::write_tag`, `PUT /api/v1/tags/<uri>/value` (operator), MCP `write_tag`; a driver declares each tag writable or not and the core refuses a read-only one without asking it; a write runs in its connection's task between polls, bounded by the poll's deadline; the answer is `written: true`, `false` or `"unknown"`. Proven by `field_write_tests.rs` (a real Modbus simulator) and through the Modbus module process in `modbus_module.rs`. Ownership of an output by a runtime (write authority) comes with the `runtime` kind |
| 2.4 The `runtime` kind (C2) | Built: `ModuleKind::Runtime`, `Call::Exchange`, `Call::Mode`, `RuntimeProgram` (`mythos-module`); `plugins/runtime-vm` (`mythos-module-runtime-vm`: the VM, a scan thread, faults, cold start); `crate::runtimes` (`RuntimeDriver`, ownership, safe values, `BusTags`). Proven by `runtime_vm.rs` (ST compiled, run in the real module process, reading one Modbus simulator and commanding another: follows the plant; an owned output refuses an operator; a setpoint is taken; stop, fault, a killed process and removal each leave the safe values; a tag that is not GOOD feeds no value) and `runtime_api.rs`. A program that publishes no tag, or is bound to nothing, runs all the same (found by the Runtimes page's end-to-end run: it was saved, reported as applied, and never started). Reading variables is built (`Call::Watch`, C5): `runtime_vm.rs` reads locals, array elements, a timer's members and a REAL from the running module process, in RUN and in STOP, and a module without `watch` says so and runs on. Forcing is built (`Call::Force`, `Call::Unforce`, C5c): `runtime_api.rs` forces an output, a setpoint and an input of the running module process over the API and watches the device follow, refuses an operator and a viewer, refuses what cannot be forced, and sees every force released by a stop and by loading the program again. The watchdog is built (C7a): a program in RUN that completes no scan within ten intervals (never less than two seconds), or whose scan thread has ended, is put in FAULT at the next exchange with the reason (`watchdog: …`), every force released, so the core writes the safe values; a scan itself is bounded by the instruction budget. Proven by the module's own tests: a healthy program runs past the limit untouched, and one whose scan thread ends is faulted and asks for no write. RETAIN is built in the runtime and the core (C7b, §2.5): `runtime_api.rs` runs a program with a RETAIN counter and proves that it goes on after a stop and run, after the program is loaded again, after its module process is killed and started again (from the values kept last, not those it was loaded with), and after a load in STOP, while the other counter starts from zero each time, and that a cold start resets it; the module's tests prove that a value is taken only into a variable of the same name, type and size. Since L4 a standard block's state is variables of its own: `runtime_api.rs` runs a RETAIN TON beside a plain one through the real module process, and after a stop and run the RETAIN one carries on from its ET (the time stopped not timed) while the plain one times again from 0, and a cold start resets both. Online change is built (C6b, §2.5): `runtime_api.rs` runs a program with a RETAIN counter, a plain counter, a TON and an output bound to a Modbus simulator, loads a build with one variable added and one removed while it runs, and proves that both counters and the timer go on without resetting, the added variable starts from its initial value, the removed one is reported as dropped, and the device's register, sampled about every millisecond through the load, never holds the safe value nor drops below what the old build commanded before the load; that a cold load resets both counters and what is kept; that a program of another name starts as before; and that an unknown `start` is refused. The module's tests prove the carry rule (same path, type and size; an array of structures or instances not carried; a value given twice taken once; what is not carried listed with why), and that a program started into RUN holds the carried values and clock before its first scan while one loaded in STOP takes none |
| 2.5 Managing runtimes | Built in the API: `GET /api/v1/runtimes`, `POST /api/v1/runtimes/:runtime/run` (a warm restart), `/cold-start`, `/stop`, `/watch`, `/force` and `/release`, `GET`/`PUT /api/v1/projects/:id/runtimes` (`?start=cold`, and `changes.runtimes` saying how each runtime started: C6b), each an MCP tool; engineer for all but reading. A watch answer says how old it is (`age_ms`, `fresh_ms`) and how long before the read the program last completed a scan (`state.scan_age_us`, from the VM module; §2.5), proven by `runtime_api.rs` and the module's own test. Built in the workbench (C4a): Settings → Runtimes (`RuntimesSettings.tsx`) reads the list about once a second and shows each runtime's module, program and version, its mode as reported with the reason given, its scan figures and its outputs (safe value, what the device last confirmed, whether all are confirmed safe); run and stop for an engineer; what is forced in each, a FORCED badge with each variable and its value (`forced UNKNOWN` when the controller does not say), and "Release all" for an engineer, asked first, with the controller's answer shown as it gave it; the runtime modules with the profile each declares. What the server did not send is `UNKNOWN` or `NOT MEASURED` (the scan figures of a runtime whose module does not answer among them: the server sends zeros there), and a read that fails or is not answered marks the rows as the last known state with their age. Proven by `runtimes.test.tsx` and, against a real server and runtime module, by `scripts/e2e.sh`. Not built: restart as a command of its own; loading a program from the page; the same for paired nodes in Fleet; data types shared in the core |
| 2.6 The logic module | Built: `plugins/logic` (programs in ST, LD, FBD or SFC, bindings, check, build; twenty-four MCP tools, each saying which languages it takes; `check_logic_library` answers `blocks`, each function and function block of a library that checks with its pins, with no new route or tool; five of the tools keep library packages in a project (ADR-015, P1b: kept, listed, read and removed; programs are not compiled with them yet); module version 0.13.1, its UI's the same; an SFC program checked with `chart`, written, built, loaded and driving a Modbus simulator through the real module processes, and over MCP, since L3d-1; its actions' and conditions' LD and FBD values as `bodies` in `check` since L3d-1b, `a_chart_with_bodies_in_ld_and_fbd_gives_their_values_apart_from_its_steps`), proven by `logic_module.rs` through the real logic, runtime and Modbus module processes, an LD program among them (created as a TC6 document, a diagnostic at its element, built, loaded, and running a Modbus simulator; an ST program saved without a language beside it, unchanged); its workbench UI `frontend/packages/logic-ui` (tree section with each program's language, a Runtimes section and its editor (C8d-1: a configured runtime's tasks edited, checked, saved, deleted, built and loaded), the project library's blocks in the LD and FBD palette, each drawn with the pins the module answered (evidence 34), a Library section and its editor (D4c: the project's library sources checked before a save, programs marked as checked against an earlier library or earlier types), program editor, the LD editor for an engineer and the read-only LD view for everyone else, both with the live state, load, load cold, how a load started, run, stop, unload), proven by its vitest suite against the workbench's real host and by `scripts/e2e.sh`, where a program typed into the editor is checked, bound, loaded and its result read live from the tag bus, and an LD program added from the tree, given rungs over the API, checked with a mistake drawn at its element, bound, loaded and run shows its contacts change state with the device's counter, and an LD program built in the editor from the keyboard and the palette is checked, saved, loaded and run, its motor coil following a start and a stop forced as a pushbutton would set them (L3b-2b). Since D3 the project's library, types and data blocks are given to every build, and since C8c the module stores a project's runtimes with their tasks and builds a runtime of several programs (2.19); every build names its project program since 0.8.0 (C8c-core). Not built: unit tests in ST |
| 2.8, 2.10 | 2.8: the ST editor (2.6), which also edits the project's library sources (D4c, `library-editor.test.tsx`), with what the front-end gives it (2.1); the read-only LD view with each diagnostic at its element and its live state from watched values (L3b-1, `ladder.test.tsx`), each element, link and junction with the power the compiler kept and a rung not run as not executed (L3b-2a); the LD editor, as TIA's works (L3b-2b, `ladder-editor.test.tsx`, `ldmodel.test.ts`); the read-only FBD view with each diagnostic at its element and every line's value live, a function not run as not computed and a network not run as not executed (L3c-1, `fbd.test.tsx`); the FBD editor, as TIA's FBD works, standard FBD (L3c-2, `fbd-editor.test.tsx`, `fbdmodel.test.ts`, `fbdedit.test.ts`, `fbd_from_the_editor.rs`); the read-only SFC view (for everyone until L3d-3, a viewer's and an operator's since), with IEC 61131-3's symbols, each diagnostic at its element and the chart's live state: each active step with its time, a transition's condition only where it was evaluated in the last scan, each action's Q, UNKNOWN where a value cannot be read (L3d-2, `sfc.test.tsx`, and a chart run in `scripts/e2e.sh`); its actions and conditions in LD or FBD named with their language, not drawn yet (their values are in `bodies`, §2.17); only current values drawn live in every diagram and the variables table, values not current UNKNOWN with why (§2.8, `currencyOf`; `currency.test.ts` and a test in each of `ladder.test.tsx`, `fbd.test.tsx`, `sfc.test.tsx`, `programs.test.tsx`); the SFC editor, as S7-GRAPH's sequence editor works (L3d-3, `sfc-editor.test.tsx`, `sfcmodel.test.ts`, `sfc_from_the_editor.rs`, and a chart built in the editor, loaded and run in `scripts/e2e.sh`). 2.10: not built |
| 2.9 Emulators | Built: eight programs (`strings.st` since L2c-1, `library.st` since L2c-2), 60 scans each, on both emulated instruction sets, every scan equal to the host's (CI job `logic-vm-bare-metal`). A bug put into REAL addition for Arm only made the two programs that add REALs fail there and nowhere else. Since L4 the timers and counters in them are the library's, in ST: all 16 runs equal Since L3a a Ladder Diagram joins them (`tests/ladder/oscillator.xml`: two TON, a CTU, rising and falling contacts, a set coil, a jump): all 18 runs equal (rustc 1.100.0, QEMU 8.2.2, run in the CI runner's image). Since L3c-1 the ladder documents of the editor (L3b-2b) and a Function Block Diagram (`tests/fbd/blinker.xml`: two TON, a CTU reset by its own output, a block's output drawn back, a connector, a jump, a function under EN, a REAL) join them: all 26 runs equal (rustc 1.100.0, QEMU 8.2.2, in the CI runner's image with podman). Since L3d-1 two Sequential Function Charts join them (`crates/mythos-logic-graphic/tests/sfc/bottling.xml`: a selection with a skip, a simultaneous divergence and convergence, a jump, N, S, R, L, D, P, DS, named and inline actions, a named transition; `doors.xml`: a chart in a function block, two instances under an FBD program): all 30 runs equal (rustc 1.100.0, QEMU 8.2.2, in the CI runner's image with podman). Since L3d-1b a chart whose actions and conditions are in LD and FBD joins them (`mixer.xml`: an FBD action in its block, an LD and an FBD action of the POU, an FBD condition in its transition, a rung and an FBD network through a connector drawn in the chart): all 36 runs equal (rustc 1.100.0, QEMU 8.2.2, in the CI runner's image with podman) |
| 2.12 Oracle | `differential_strucpp.rs`: 8 programs, 60 cycles, 9,480 values, all agree except the five deliberate rows of §2.13 (287 values: 167 of the first three, 60 each of INSERT's position and a based literal read as an integer). It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE. Unchanged by L4 (the standard blocks in ST): the same 9,480 values, the same 287 |
| 2.14 Standard library in ST (L4) | `crates/mythos-logic-st/src/library/`: SR, RS, R_TRIG, F_TRIG, CTU, CTD, CTUD and their INT, DINT and LINT variants, TP, TON, TOF and their TIME variants (22 POUs in 4 files, an index); the intrinsic `__NOW()`; names of the library refused to a program's POUs and types; the library's names resolved among its own; no diagnostic pointing into it. Proven by `standard_library.rs` (14) and `standard_blocks_run_as_the_native_ones.rs` (4: every native block against the library's, 400 sequences of 150 scans each, 540,000 scans in all) |
| 2.15 Ladder Diagram (L3a) | `crates/mythos-logic-ld` (`crates/mythos-logic-graphic` since L3c-1, §2.16): PLCopen TC6 XML v2.01 read with `quick-xml` (MIT); the subset of §2.15; the graph checked and lowered to the ST front-end's syntax tree; edges, values used twice and jumps in hidden private variables (`Var::hidden`, `IR_VERSION` 6); diagnostics naming the element (`element`) and the source (`source`); `compile_sources` and `Language::Ld` in the compiler; `layers.rs` gives LD its layer (the IR and the ST front-end) and feeds the guard an LD crate reaching the VM and an ST crate reaching LD. Proven by `ladder_runs_on_the_vm.rs` (26: every element, its table's behaviour scan by scan, EN and ENO, a function of ST and the standard functions as blocks, LD function blocks and functions, jumps both ways, a return, RETAIN and the edge memory of a RETAIN instance, 300 contacts in series and in parallel, the emulator's program), `refusals.rs` (28: every refusal of §2.15 with its message and its element, the JSON of a place, the sources named), `ladder_equals_st.rs` (4 LD programs and their ST twins, 200 random sequences of 100 scans each, every shared variable equal after every scan: 51 variables, 1,020,000 values compared) and `damaged_ladder_never_panics.rs` (3,000 damaged documents, each compiled or refused with a place). Mutation-checked in a copy of the tree (`git archive`, its own target directory; the clean copy passed before and after the restores): parallel branches made AND turned `series_is_and_and_parallel_is_or`, `a_long_series_and_a_wide_parallel_compile_and_run`, `an_ld_function_block_and_an_ld_function_are_called_from_an_ld_program` and `contacts_and_coils_equal_their_st_twin` red; a negated contact's negation dropped turned `a_normally_closed_contact_passes_power_when_its_variable_is_off`, `an_ld_function_block_and_an_ld_function_are_called_from_an_ld_program`, `the_oscillator_the_emulators_run_does_what_its_rungs_say` and `contacts_and_coils_equal_their_st_twin` red; an edge contact never writing its memory turned `a_rising_contact_is_on_for_one_scan_after_its_variable_goes_on`, `a_rising_contact_senses_its_variable_while_its_input_is_off`, `a_falling_contact_is_on_for_one_scan_after_its_variable_goes_off` and `contacts_and_coils_equal_their_st_twin` red; a dangling connection let through turned `a_dangling_connection_is_refused_naming_its_element_and_the_missing_id` and `a_damaged_ladder_document_is_compiled_or_refused_with_places_never_a_panic` red. Since L3b-1 the logic module stores, checks and builds LD programs (2.6). Since L3b-2a every element's output, every junction and every network's flag is kept in a hidden private variable in every build (`Compiled::probes`, §2.15, power for monitoring), proven by `ladder_power_probes.rs` (9) and `a_long_series_of_functions_compiles_and_runs`. Not built: editing LD in the workbench (L3b-2b) |
| 2.16 Function Block Diagram (L3c-1) | `crates/mythos-logic-graphic`: `fbd.rs`, with `graph.rs` shared with LD; the subset and the semantics of §2.16 and §2.13; every output's value kept in a hidden private variable typed by its value (`ast::TypeSpecKind::OfValue` in the ST front-end), every network's flag; `Holds::Guarded` for a function's result; `Language::Fbd`; `Sources::tc6`. Proven by `fbd_runs_on_the_vm.rs` (13), `fbd_refusals.rs` (15), `fbd_equals_st.rs` (5 FBD programs and their ST twins, 200 random sequences of 100 scans each: every shared variable and every kept value, by the element and output it is about, equal to its twin's after every scan, 3,560,000 values), `damaged_ladder_never_panics.rs` (3,000 damaged FBD documents besides the LD ones), the logic module's own tests (3) and `logic_module.rs` through the real module processes; the emulators (2.9). The workbench: `fbd.test.tsx` (11; 12 since L3d-2, a viewer's diagram following a source saved elsewhere). The editor (L3c-2, §2.8): `fbdmodel.test.ts` (11), `fbd-editor.test.tsx` (10), `fbd_from_the_editor.rs` (4). Not built: a CFC mode (§3) |
| 2.17 Sequential Function Chart (L3d-1) | `crates/mythos-logic-graphic`: `sfc.rs`; the subset and the semantics of §2.17 and §2.13 (§6.7: steps, transitions, Table 61's divergences and convergences, jumps, action blocks, the action control without final scan, unsafe and unreachable charts refused); every step's flag and time, every transition's condition and enable flag and every action's state kept in private variables (`At::Step`, `At::StepTime`, `At::Transition`, `At::Action`); the scan clock as `ast::ExprKind::Intrinsic`; `Element::action`, `Element::transition`, `TextRange::line`; `Language::Sfc`. Since L3d-1b actions and conditions in LD and FBD (of the POU, in the action block or the transition, or a network drawn in the chart), lowered by `ld::lower` and `fbd::lower` in a namespace of their own, `Probe::body`, `Element::inline`, `bodies` in `check`. Proven by `sfc_runs_on_the_vm.rs` (29: a rule of §6.7 each, the action qualifiers scan by scan, a chart in a function block, a restart, an online change of the same chart and of another, the two charts of the emulators), `sfc_refusals.rs` (38 since L3d-1b: every refusal with its message and its element, a place in a text of several lines as JSON), `sfc_corpus.rs` (1), `sfc_equals_st.rs` (4 charts and their ST twins, the fourth with actions and conditions in LD and FBD, 200 random sequences of 100 scans each: every variable, step flag, step time while active, action flag, transition condition where evaluated and enable flag equal after every scan, 2,270,452 values), `sfc_graphic_bodies.rs` (14: a rule of L3d-1b each, scan by scan, and the mixer of the emulators), `damaged_ladder_never_panics.rs` (3,000 damaged charts: 2,718 refused, 282 compiled; 3,000 damaged mixers with LD and FBD bodies: 2,686 refused, 314 compiled), the logic module's own tests (4), `logic_module.rs` through the real module processes and over MCP; the emulators (2.9). The workbench: `sfc.test.tsx` (3 in L3d-1; 23 since the view, L3d-2, §2.8). The editor (L3d-3, §2.8): `sfcmodel.test.ts` (31), `sfc-editor.test.tsx` (9), `sfc_from_the_editor.rs` (6). Not built: drawing the LD and FBD bodies of actions and conditions with their views (the view names their language); an action that is a chart, IL, a connection from a step's flag (§2.17); the qualifiers SD, SL, P0 (the owner's decision, §2.17) |
| 2.18 Firmware for microcontrollers | Planned, nothing built. What exists is 2.9's: the VM built for thumbv6m and riscv32imc and run in emulators. Xtensa: not built. A real board: none has run it. |
| 2.19 Task configuration (C8) | C8a built: `mythos_logic_vm::Run` (a scan in pieces, suspended and resumed) and `mythos_logic_vm::sched` (which task runs next). C8b built: a runtime, in `RuntimeInstance` (`program` or `programs`, and `tasks`) and in the VM runtime module, runs several programs in tasks of the kinds cycle, cyclic, startup and event, with priorities, preemption by the VM and a process image per task; the module says what it runs in `profile.tasks`, and the core refuses a configuration an older module cannot run. C8c built: the logic module stores a project's runtimes and their tasks in `logic/tasks.json` (rules T1 to T9, six routes, six MCP tools) and builds a runtime of several programs as one instance, with its tasks and the union of its data blocks; a project with no `tasks.json` builds and answers as before; two programs sharing a data block are built into one runtime by the module and loaded by saving its answer, proven through the real module processes (`data_and_logic.rs`). The module does not load. C8d-1 built: the workbench edits, checks, saves, deletes, builds and loads a configured runtime and its tasks (a load takes out or rebuilds the instances that hold its programs, in the same save, after a question), shows what the core lists of it, and shows no task state; C8d-2 not built: a program of a configured runtime is still UNKNOWN in the program editor, not running, and the workbench shows no task state, watch or force of it (`32-c8d.md`); C8d-core built: each list of variables watched of a runtime is its own watch, read once for the union of at most 256 names with at most 256 lists, and the save of a project's runtimes takes a `revision` and is refused with 409 when it is no longer the stored one (`33-c8d-core.md`); C8c-core built: the core keeps RETAIN values per runtime id and per program and gives a program only the values kept for it (`not_given` lists the rest); a new build is changed online only when it is the same programs of the same project (by `built_from` and `created` when every program names its project program, else by PROGRAM name); and the core refuses a save in which one project program is in two runtimes, when both instances name it (built by the logic module since 0.8.0; an instance built before is not compared, and its program runs in both until it is taken out). A retain file written before is given once as it is and written again per program; RETAIN values and retained data blocks do not follow a program to another runtime id (`28-c8c.md`, `29-c8c-core.md`). |

The evidence of the spike (2026-11-17) and of each slice after it, with its tests, measurements and mutation checks, is in its own file in
[`ADR-013-evidence/`](ADR-013-evidence/). A new slice adds a file there and one line here, so two slices open at once do not
edit the same lines. `scripts/check-adr-evidence.sh` fails if a file is not listed here or a line here has no file.

- **Tests**: [`01-tests.md`](ADR-013-evidence/01-tests.md)
- **L2a** (the parser the editor needs): [`02-l2a.md`](ADR-013-evidence/02-l2a.md)
- **Depth** (L2a, after a stack overflow on Windows CI): [`03-depth.md`](ADR-013-evidence/03-depth.md)
- **Scan time on a PC** (`scan_time.rs`, release, i9-11980HK, Linux, not real-time, desktop load): [`04-scan-time-on-a-pc.md`](ADR-013-evidence/04-scan-time-on-a-pc.md)
- **Microcontrollers** (`scripts/logic-vm-footprint.sh`, CI job `logic-vm-bare-metal`): [`05-microcontrollers.md`](ADR-013-evidence/05-microcontrollers.md)
- **L2b** (the limits): [`06-l2b.md`](ADR-013-evidence/06-l2b.md)
- **L2c-1** (strings): [`07-l2c-1.md`](ADR-013-evidence/07-l2c-1.md)
- **Format versions** (review of L2c-1: the loader refused format 1, which every controller running a program held): [`08-format-versions.md`](ADR-013-evidence/08-format-versions.md)
- **L2c-2** (the library): [`09-l2c-2.md`](ADR-013-evidence/09-l2c-2.md)
- **Format versions, carried into L2c-2**: [`10-format-versions-carried-into-l2c-2.md`](ADR-013-evidence/10-format-versions-carried-into-l2c-2.md)
- **L4** (the standard library in ST, 2026-11-18): [`11-l4.md`](ADR-013-evidence/11-l4.md)
- **L3b-1** (Ladder Diagram programs in the logic module and the read-only LD view, 2026-11-18): [`12-l3b-1.md`](ADR-013-evidence/12-l3b-1.md)
- **L3b-2a** (every LD element's power kept for monitoring, and the view drawing it, 2026-11-18): [`13-l3b-2a.md`](ADR-013-evidence/13-l3b-2a.md)
- **L3b-2b** (editing a Ladder Diagram in the workbench, 2026-11-18): [`14-l3b-2b.md`](ADR-013-evidence/14-l3b-2b.md)
- **L3c-1** (Function Block Diagram programs, 2026-11-18): [`15-l3c-1.md`](ADR-013-evidence/15-l3c-1.md)
- **L3c-2** (editing a Function Block Diagram in the workbench, 2026-11-20): [`16-l3c-2.md`](ADR-013-evidence/16-l3c-2.md)
- **L3d-1** (Sequential Function Chart programs, 2026-11-20): [`17-l3d-1.md`](ADR-013-evidence/17-l3d-1.md)
- **L3d-1b** (a chart's actions and conditions in LD and FBD, 2026-11-22): [`18-l3d-1b.md`](ADR-013-evidence/18-l3d-1b.md)
- **L3d-2** (the Sequential Function Chart view, live, 2026-11-22): [`19-l3d-2.md`](ADR-013-evidence/19-l3d-2.md)
- **L3d-3** (the Sequential Function Chart editor, 2026-11-22): [`20-l3d-3.md`](ADR-013-evidence/20-l3d-3.md)
- **Stale watch** (only current values drawn live, found in review of L3d-2, 2026-11-22): [`21-stale-watch.md`](ADR-013-evidence/21-stale-watch.md)
- **SD, SL and P0** (the three qualifiers refused until the owner decided, 2026-11-22: taken as Table 59 names them, §2.17): [`22-sd-sl-and-p0.md`](ADR-013-evidence/22-sd-sl-and-p0.md)
- **C8a** (the VM's part of task configuration: a run suspended and resumed, the scheduler core, 2026-11-22): [`23-c8a.md`](ADR-013-evidence/23-c8a.md)
- **C8b** (several programs in several tasks in one runtime: the module protocol, the VM runner and the core, 2026-11-22): [`24-c8b.md`](ADR-013-evidence/24-c8b.md)
- **D1** (data blocks in the compiler, the first slice of ADR-014: library sources, VAR_EXTERNAL, the members a program writes, the layout hash, 2026-11-22): [`25-d1.md`](ADR-013-evidence/25-d1.md)
- **D2** (data blocks handed over between the programs of one runtime, the second slice of ADR-014: the module protocol, the runtime module and the core, 2026-11-22): [`26-d2.md`](ADR-013-evidence/26-d2.md)
- **D3** (the data module, and the logic module building with a project's types, data blocks and library: the third slice of ADR-014, 2026-11-22): [`27-d3.md`](ADR-013-evidence/27-d3.md)
- **C8c** (the logic module's runtimes and task configuration: the third slice of §2.19, 2026-11-23): [`28-c8c.md`](ADR-013-evidence/28-c8c.md)
- **C8c-core** (a program's identity, RETAIN values kept per program and one program in one runtime: what the review of C8c found in the core, 2026-11-23): [`29-c8c-core.md`](ADR-013-evidence/29-c8c-core.md)
- **D4a** (the data module's workbench UI: editors for user types and data blocks, the first part of ADR-014's D4, 2026-11-23): [`30-d4a.md`](ADR-013-evidence/30-d4a.md)
- **D4c** (the library editor in the logic module's workbench UI, the third part of ADR-014's D4, 2026-11-23): [`31-d4c.md`](ADR-013-evidence/31-d4c.md)
- **C8d-1** (the runtimes and tasks editor in the logic module's workbench UI, with Build and Load of a configured runtime, the first part of section 2.19's C8d, 2026-11-23): [`32-c8d.md`](ADR-013-evidence/32-c8d.md)
- **C8d-core** (several watch lists per runtime in the core, and a revision precondition on the save of the runtimes configuration, 2026-11-24): [`33-c8d-core.md`](ADR-013-evidence/33-c8d-core.md)
- **Library blocks in the palette** (the project's library's functions and function blocks offered and drawn with their pins in the LD and FBD editors, 2026-11-24): [`34-library-blocks-in-the-palette.md`](ADR-013-evidence/34-library-blocks-in-the-palette.md)
- **Library packages P1** (P1a: the package file read and checked by the new crate `mythos-logic-package`; P1b: the stored file and `packages.json`, five routes with their MCP tools and an event; ADR-015 step 1, 2026-11-24; P1a and P1b are built, P1c is not): [`35-library-packages-p1.md`](ADR-013-evidence/35-library-packages-p1.md)
