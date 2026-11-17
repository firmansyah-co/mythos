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

A target declares what it can run: the bytecode format its VM reads, types it refuses, and the
variable memory and code a program may use. The compiler checks the IR and its output against
the profile, so a program that does not fit is refused at compile time with the reason, never on
the controller. A profile is `serde` data: **a runtime module carries its own in its manifest**
(§2.4), so a new board brings its profile without a compiler change. Until runtime modules exist,
three stand in (`pc`, `rp2040`, `esp32`); their microcontroller limits are budgets chosen for a
program, not the chips' sizes.

### 2.3 The logic VM

`crates/mythos-logic-vm`. The same code runs in the PC runner and in microcontroller firmware:

- **`no_std`, no allocation, no `unsafe`.** The host gives it the program's bytes, a memory and
  a stack of the sizes the program declares. Clock, I/O and communication belong to the host.
- **A portable format.** Little-endian on every target, versioned (a program of another version
  is refused, not guessed at), CRC-32 checked. Variables are packed bytes with no alignment.
- **Checked twice.** At load, every instruction is decoded and every address, jump and function
  block instance checked. At run time, every memory and stack access is checked again; a program
  that would go outside them stops with a fault.
- **A bounded scan.** An instruction budget stops an endless loop with `BudgetExceeded`.
  Division by zero is a fault, not a value.
- **Format 1** (functions, user function blocks, arrays, structures). Format 0 is refused by
  its version.
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
    `CallDepthExceeded`. A jump into the middle of an instruction is not refused at load; the
    run-time checks still hold. `damaged_programs_never_panic.rs` damages every compiled
    program of the corpus at random, thousands of times, and requires each to be refused or to
    fault, never to panic.

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
- The write path to field devices, the safe values and the write authority of §2.4.
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
  with the diagnostics (line, column, message), or with the variables and every binding that
  names a variable the program does not have or one that cannot be bound (an array, a TIME).
- `build` compiles the saved source and answers with the runtime instance of §2.4, its version
  a hash of the code, the interval and the bindings. **A module cannot call the core**
  (ADR-011 §2.6), so the module does not load what it builds: the workbench, or an agent, saves
  the instance into the project's runtimes, and the core loads it. An edit therefore changes
  nothing that runs until it is built and loaded. A bound tag that no connection provides now
  is a warning of the build, not a refusal.
- Not built: unit tests in ST, and programs in a device's slice (C6).

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
language brings its editor as an editor kind. The spike's parser stops at the first error and
records only where a construct starts; an editor needs several errors at once and ranges, so the
ST front-end's parser is rewritten for that before the editor is built.

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
it is run as a separate program and none of its code is copied. Its limits: six programs so
far, reals compared at the precision its REPL prints, only scalar variables of the program
compared, and STruC++ can itself be wrong.

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

Three places where the standard leaves a choice or we go beyond it:

- **Overflow.** §7.3.2 calls a result outside its type's range an error. We wrap at the type's
  width, as STruC++ does, and do not stop the scan. Whether to fault instead is open.
- **Narrowing** a value that does not fit (Table 22's example: implementer specific): we wrap.
- **`INT_TO_BOOL` and `BOOL_TO_REAL`** families are not defined by Figure 11 ("the implementation
  may support additional conversions"). We have them: non-zero is TRUE; FALSE and TRUE are 0.0
  and 1.0.

Known gaps, to close in L2: a reserved word is still accepted as a name (`at : INT`
compiles); the evaluation stack uses whatever the host gives instead of stopping at the depth
the program declares, as the call frames do; a target profile does not yet carry a stack depth
or a call depth, so a program too deep for a firmware is refused at its start and not at
compile time.

The differential test lists the first three rows as deliberate differences from STruC++
(167 of 6,120 values); a rounding difference must be exactly one apart to count as one.

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
- C4. **The ST editor** in the workbench.
  - C4a. **The Runtimes page** (done): Settings → Runtimes shows every runtime of this node as
    `GET /api/v1/runtimes` reports it, with run and stop for an engineer (§2.5, §5).
- C5. **Online**: monitor, force, runtime tags in the HMI.
- C6. **Swap and online change; programs in a device's slice.**
- C7. **Hardening**: watchdog, RETAIN variables across restarts.

**Track L, the language, beside it:**

- L1. **The VM's real model** (done): functions, user function blocks, arrays and structures;
  static allocation, no recursion, instance-relative addressing, a bounded call stack;
  format 1.
- L2. A parser that recovers from errors and gives ranges (for the editor); more of the
  standard library; strings.
- L3. **LD, FBD, SFC** (not IL, §2.1), each a front-end crate and an editor kind.

**Later:** ESP32 and RP2040 firmware as runtime modules on real boards; a C backend; a Linux
real-time mode.

Each step leaves `dev` working.

## 4. Consequences

- No toolchain is installed with MythOS or on a controller to compile logic: the compiler is Rust
  and part of the logic module.
- A controller runs bytecode, not native code: slower than STruC++'s native output (§5), and the
  price of memory safety, one format for every target, and variables enumerable by name.
- The language coverage is ours to build. STruC++ covers far more today (OOP, pointers, strings,
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
| 2.1 ST front-end | Several POUs in any order: TYPE (STRUCT, named ARRAY), FUNCTION (VAR_INPUT, VAR; positional and formal calls), FUNCTION_BLOCK (VAR_INPUT, VAR_OUTPUT, VAR; nesting; arrays of instances), one PROGRAM; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME; arrays of any dimensions with initial value lists; whole-structure and whole-array assignment; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between the elementary types; implicit widening per Figure 11; recursion refused naming the cycle; diagnostics with line and column for everything else (enumerations, subranges, strings, VAR_IN_OUT, methods, and more: each refused by name) |
| 2.2 Profiles | `serde` data with a format version; `pc`, `rp2040`, `esp32` stand in; refusals (type, memory, format) proven with profiles read from JSON |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; format 1: bodies, call frames, instance-relative and region-confined addressing; loader checks, run-time checks, budget; faults, with `IndexOutOfRange` and `CallDepthExceeded` |
| 2.4 Writes to field devices (C1) | Built: `IoStatusHandle::write_tag`, `PUT /api/v1/tags/<uri>/value` (operator), MCP `write_tag`; a driver declares each tag writable or not and the core refuses a read-only one without asking it; a write runs in its connection's task between polls, bounded by the poll's deadline; the answer is `written: true`, `false` or `"unknown"`. Proven by `field_write_tests.rs` (a real Modbus simulator) and through the Modbus module process in `modbus_module.rs`. Ownership of an output by a runtime (write authority) comes with the `runtime` kind |
| 2.4 The `runtime` kind (C2) | Built: `ModuleKind::Runtime`, `Call::Exchange`, `Call::Mode`, `RuntimeProgram` (`mythos-module`); `plugins/runtime-vm` (`mythos-module-runtime-vm`: the VM, a scan thread, faults, cold start); `crate::runtimes` (`RuntimeDriver`, ownership, safe values, `BusTags`). Proven by `runtime_vm.rs` (ST compiled, run in the real module process, reading one Modbus simulator and commanding another: follows the plant; an owned output refuses an operator; a setpoint is taken; stop, fault, a killed process and removal each leave the safe values; a tag that is not GOOD feeds no value) and `runtime_api.rs`. A program that publishes no tag, or is bound to nothing, runs all the same (found by the Runtimes page's end-to-end run: it was saved, reported as applied, and never started). Not built: reading or forcing variables, state carried across a replacement, a watchdog on the scan thread, RETAIN |
| 2.5 Managing runtimes | Built in the API: `GET /api/v1/runtimes`, `POST /api/v1/runtimes/:id/run` and `/stop`, `GET`/`PUT /api/v1/projects/:id/runtimes`, each an MCP tool; engineer for all but reading. Built in the workbench (C4a): Settings → Runtimes (`RuntimesSettings.tsx`) reads the list about once a second and shows each runtime's module, program and version, its mode as reported with the reason given, its scan figures and its outputs (safe value, what the device last confirmed, whether all are confirmed safe); run and stop for an engineer; the runtime modules with the profile each declares. What the server did not send is `UNKNOWN` or `NOT MEASURED` (the scan figures of a runtime whose module does not answer among them: the server sends zeros there), and a read that fails or is not answered marks the rows as the last known state with their age. Proven by `runtimes.test.tsx` and, against a real server and runtime module, by `scripts/e2e.sh`. Not built: restart as a command of its own; loading a program from the page; the same for paired nodes in Fleet; data types shared in the core |
| 2.6 The logic module | Built: `plugins/logic` (programs, bindings, check, build; eight MCP tools), proven by `logic_module.rs` through the real logic, runtime and Modbus module processes. Not built: its editor in the workbench, unit tests in ST, programs in a device's slice |
| 2.8, 2.10 | Not built |
| 2.9 Emulators | Built: six programs, 60 scans each, on both emulated instruction sets, every scan equal to the host's (CI job `logic-vm-bare-metal`). A bug put into REAL addition for Arm only made the two programs that add REALs fail there and nowhere else |
| 2.12 Oracle | `differential_strucpp.rs`: 6 programs, 60 cycles, 6,120 values, all agree except the three deliberate rows of §2.13 (167 values). It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE |

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
- **Scan time on a PC** (`scan_time.rs`, release, i9-11980HK, Linux, not real-time, desktop
  load): a program shaped like a small plant (CASE sequence, 8 TON, CTU, R_TRIG, a PI loop in
  REAL, a 50-turn FOR) is 884 bytes of code and 280 of memory, 970 instructions per scan:
  over 200,000 scans the mean was between 7.3 and 13.1 µs in six runs of the same code (the
  machine was in use; the fastest run: p99 9.3 µs, max 100 µs; the slowest: p99 22.5 µs; the
  worst single scan seen in any run: 336 µs). The VM has not been
  optimised. Windows and macOS: not measured.
- **Microcontrollers** (`scripts/logic-vm-footprint.sh`, CI job `logic-vm-bare-metal`): the VM
  with its loader and scan loop, `opt-level=s`, LTO, linked with `core` alone: **RP2040**
  (thumbv6m-none-eabi) 19,200 bytes of code (15,884 before format 1); **ESP32-C3**
  (riscv32imc-unknown-none-elf) 22,908 bytes (19,146 before). Run on a real board: not yet (none attached). The original ESP32 and the S3 are
  Xtensa, which needs Espressif's Rust toolchain: not built.
