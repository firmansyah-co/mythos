# ADR-013: Control Logic: Our Own IEC 61131-3 Compiler, a Portable VM, and the Logic Module

- **Status**: PROPOSED (step 0 done: the spike below; for Adrian's review before step 1)
- **Date**: 2026-11-17
- **Requested by**: Adrian ("bagaimana planning implementasi logika kontrol ini? harus ada runtime,
  editor, dan manager kan?"; "menurutmu mending pakai struc++ atau kita bangun compiler sendiri";
  "kedepannya aku juga mau program dari compiler ST kita bisa jalan di bare metal /
  mikrokontroller dan PLC murah lain ... jadi harus kita siapkan sejak dini juga")
- **Decided with Adrian, 2026-11-17**: outputs go to a safe value when logic stops; the operator
  may not write an output logic owns; all five IEC languages if possible; the manager manages the
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
| `mythos-logic-ir` | nothing of the workspace | The seam. Resolved names, types and statements; no addresses, word sizes or instructions. `serde` data with a version (`IR_VERSION`), so a front-end and a backend need not share a process |
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
  This is a stopgap: once user function blocks exist, the standard blocks become a library
  written in the IEC languages, and adding a block no longer means new firmware.
- **IL is not planned.** Ed. 3 §7.2.1: "This language is outdated as an assembler like
  language. Therefore it is deprecated and will not be contained in the next edition of this
  standard." Four languages remain: ST, LD, FBD, SFC. (Adrian asked for all five "if possible";
  this is put to him.)

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
- **Format 0 is not stable.** It has absolute addresses only. User function blocks, arrays with a
  computed index, structs and function calls need instance-relative addressing and a call
  mechanism; that is step 1 of §3, and it will change the format. Nothing may depend on format 0
  surviving.

### 2.4 A third module kind: `runtime`

ADR-010 has `driver` (reads field devices) and ADR-011 `service` (serves routes). Neither fits a
thing that executes continuously against the tags. A **runtime** module is a process the core
starts, watches, restarts and swaps like any module, speaking the module protocol with these
calls:

- `load` a program (for the logic VM: bytecode and its symbol table), `run`, `stop`, `state`
  (RUN, STOP or FAULT and why; scan time last and max; overruns; program version);
- it **subscribes** to the tags it reads and is given their values and qualities;
- it **publishes** values for the tags it owns, and **requests writes** to field tags; the core
  performs both. The tag bus keeps one writer (ADR-002, ADR-010 §2.2);
- `read`, `write` and `force` a variable by its path, for monitoring;
- its manifest declares its **target profile** (§2.2) and what kinds of program it loads.

The protocol is not specific to logic or to our VM: it is "compute continuously from tags;
publish tags; request writes; report state". The logic VM runner on a PC
(`mythos-module-runtime-vm`) is the first. A board family (ESP32, RP2040) is another: a module
that carries the same calls to firmware over serial, USB or TCP. A different engine can be a
third without a core change. Server-side functions that also execute continuously (an alarm
engine, a historian's logger) are candidates for the same kind when they are built.

Rules the core enforces for every runtime:

- **Process image.** Inputs are latched at the start of a scan, outputs sent after it. Only
  changed values cross, so the scan time does not depend on the exchange. A runtime on the same
  machine may read the tag bus directly as a reader; outputs always go through the core.
- **Safe values.** Every output bound to a field tag has a safe value, set in the project and
  required. When the program stops, faults, misses its watchdog or its process dies, the core
  writes the safe values. A stopped runtime's own tags read `BAD_COMM_FAILURE` with no value.
- **Write authority.** An output bound to a runtime is owned by it. The operator and the HMI
  cannot write it; a setpoint is an input variable they may write, through the same path.
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

`plugins/logic` (`mythos-module-logic`), a `service` module (ADR-011 §3 step 5): programs and
tasks in `<project>/logic/`; check; compile for a target; unit tests in ST; the variable table
and each variable's binding to a tag. It asks the core to load a program into a runtime; it
never talks to a runtime itself. Every route is an MCP tool. A device's programs are part of its
slice (ADR-012 §2.5).

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

- **Run in an emulator, in CI.** QEMU has a Cortex-M0 machine (thumbv6m, the RP2040's instruction
  set) and Espressif publishes a QEMU with the ESP32-C3; RP2040-specific emulators exist
  (rp2040js from Wokwi, in Node; picoem, in Rust). This proves the same program gives the same
  values on the target's instruction set, and gives an instruction count per scan.
- **Real timing needs a real board.** An emulator counts instructions; it does not reproduce
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
it is run as a separate program and none of its code is copied. Its limits: three programs so
far, reals compared at the precision its REPL prints, and STruC++ can itself be wrong.

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

Three places where the standard leaves a choice or we go beyond it:

- **Overflow.** §7.3.2 calls a result outside its type's range an error. We wrap at the type's
  width, as STruC++ does, and do not stop the scan. Whether to fault instead is open.
- **Narrowing** a value that does not fit (Table 22's example: implementer specific): we wrap.
- **`INT_TO_BOOL` and `BOOL_TO_REAL`** families are not defined by Figure 11 ("the implementation
  may support additional conversions"). We have them: non-zero is TRUE; FALSE and TRUE are 0.0
  and 1.0.

The differential test lists the first three rows as deliberate differences from STruC++
(167 of 2,820 values); a rounding difference must be exactly one apart to count as one.

## 3. Order of work

0. **Spike** (done, §5), and the crates split along §2.1.
1. **The VM's real model**: calls, instance-relative addressing, user function blocks, arrays and
   structs; format 1. Run in an emulator in CI (§2.9).
2. **Writes to field devices** in the core: a driver's `write`, write authority, an MCP tool.
   Also what HMI setpoints need.
3. **The `runtime` module kind** in the protocol and the module host; the PC runner; safe
   values; `GET /api/v1/runtimes` and Settings → Runtimes. Proven against a real Modbus simulator
   on Linux, macOS and Windows.
4. **The logic module** (authoring) and the **ST editor**, on a parser that recovers from errors.
5. **Online**: monitor, force, runtime tags in the HMI.
6. **Swap and online change; programs in a device's slice.**
7. **Hardening**: watchdog, RETAIN variables across restarts.
8. **ESP32 and RP2040 firmware** as runtime modules, on real boards.
9. **LD, FBD, SFC** (not IL, §2.1), each a front-end crate and an editor kind; a **C backend**; a Linux
   real-time mode. The rest of the ST language and the standard library run beside steps 2–8:
   the runtime protocol does not depend on what the bytecode contains.

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
| 2.1 ST front-end | Spike: one PROGRAM; VAR, VAR_INPUT, VAR_OUTPUT; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between those types; implicit widening per Figure 11; diagnostics with line and column for everything else |
| 2.2 Profiles | `serde` data with a format version; `pc`, `rp2040`, `esp32` stand in; refusals (type, memory, format) proven with profiles read from JSON |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; loader checks, run-time checks, budget, faults |
| 2.4–2.6, 2.8, 2.10 | Not built |
| 2.9 Emulators | Not built: the VM is linked for both targets in CI, not yet run on them |
| 2.12 Oracle | `differential_strucpp.rs`: 3 programs, 60 cycles, 2,820 values, all agree except the three deliberate rows of §2.13 (167 values). It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE |

Evidence of the spike (2026-11-17):

- **Tests**: `st_runs_on_the_vm.rs` (17), `load_refuses_bad_programs.rs` (8), `layers.rs` (2),
  and unit tests (15). Mutation-checked: removing INT wrapping, the TP reset, the loader's address check, the
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
  (thumbv6m-none-eabi) 15,884 bytes of code; **ESP32-C3** (riscv32imc-unknown-none-elf)
  19,146 bytes. Run on a real board: not yet (none attached). The original ESP32 and the S3 are
  Xtensa, which needs Espressif's Rust toolchain: not built.
