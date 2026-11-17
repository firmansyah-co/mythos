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
- **Written by**: Claude, implementing the request

**Supersedes ADR-003** (an in-process STruC++ `.so` under `SCHED_FIFO`). That design was never
built into anything that ran (`crates/mythos-strucpp` was removed by #75 because no binary used
it), and it contradicts three later requirements: modules are processes (ADR-010), MythOS runs on
every OS, and logic must run on microcontrollers.

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

### 2.1 One compiler, a target-neutral IR, several backends

```text
 ST ─┐
 LD ─┤                                    ┌─► VM bytecode ──► logic VM (PC runner, ESP32, RP2040)
 FBD ┼─► parse ─► check ─► IR ─► profile ─┤
 SFC ┤                    check           └─► C99 (later: MCUs too small for the VM)
 IL ─┘
```

- `crates/mythos-logic-compiler` (Rust, `std`): lexer, parser, type checker, IR, backends.
- **The IR is the seam.** It holds resolved names, types and statements, and nothing a target
  decides: no addresses, no word sizes, no instructions. Every language lowers into it and every
  backend starts from it. LD and FBD are lowered from their graphs, not printed as ST text.
- **Nothing unsupported is compiled into something else.** A construct the compiler does not
  take yet is a diagnostic naming it, with its line and column.
- IEC 61131-3 ed. 3 is the reference. Where an implementation we compare against differs from
  the standard's text, we follow the text and record the difference (§2.10).
- IL is deprecated in ed. 3. It is the last language.

### 2.2 Target profiles

A target declares what it can run: types it refuses, and the variable memory and code a program
may use. The compiler checks the IR and its output against the target, so a program that does
not fit is refused at compile time with the reason ("LREAL is not supported on the target …",
"the program needs 70 KB of variable memory; the target allows 64 KB"), never on the controller.
Profiles today: `pc`, `rp2040`, `esp32`. The microcontroller limits are budgets chosen for a
program, leaving the rest of the chip to the firmware; they are not the chips' sizes.

### 2.3 The logic VM

`crates/mythos-logic-vm`. The same code runs in the PC runner and in microcontroller firmware:

- **`no_std`, no allocation, no `unsafe`.** The host gives it the program's bytes, a memory and
  a stack of the sizes the program declares. Clock, I/O and communication belong to the host.
- **A portable format.** Little-endian on every target, versioned (a program of another version
  is refused, not guessed at), CRC-32 checked. Variables are packed bytes with no alignment, so a
  layout never depends on the target.
- **Checked twice.** At load, every instruction is decoded and every address, jump and function
  block instance checked against the program's memory and code. At run time, every memory and
  stack access is checked again; a program that would go outside them stops with a fault.
- **A bounded scan.** The host gives an instruction budget; a scan that exceeds it (an endless
  `WHILE`) stops with `BudgetExceeded`. Division by zero is a fault, not a value.
- **Standard function blocks** (TON, TOF, TP, CTU, CTD, R_TRIG, F_TRIG, SR, RS) are built in. Their
  instance layouts are defined once in the VM and read by the compiler.
- Integer arithmetic wraps at the width of its type, inside expressions too. REAL to integer
  conversions round to nearest, halves away from zero, and saturate.

### 2.4 The runtime on a PC: a runner process per program instance

A **logic runner** is a process (ADR-010: a crash cannot take the server down, and it can be
replaced at any time) that embeds the VM and runs one device's program: its tasks, each with an
interval, at its own clock.

- **Process image.** Inputs are latched at the start of a scan, outputs sent after it. Runner and
  core exchange **changed values**, not a round trip per scan, so the scan time does not depend
  on the exchange.
- **One tag bus writer** (ADR-002, ADR-010 §2.2). The runner never writes the tag bus. The core
  writes logic variables published as tags (`internal.<namespace>.<program>.<variable>`, per the
  tag namespace rules) and forwards logic outputs bound to field tags to the driver's `write`.
- **Safe values.** Every output bound to a field tag has a safe value, set in the project and
  required. When the program stops, faults, misses its watchdog or its runner dies, the core
  writes the safe values. A stopped program's own tags read `BAD_COMM_FAILURE` with no value.
- **Write authority.** An output bound to logic is owned by logic. The operator and the HMI cannot
  write it; a setpoint is a logic input variable they may write, through the same path.
- **Hot swap.** A new build starts beside the old one; variables whose path and type match are
  copied over at a scan boundary; then the I/O switches and the old runner stops. A build that
  cannot start leaves the old one running.
- **Timing is claimed only as measured.** On a general-purpose OS the target is task intervals of
  10 ms and above. A real-time mode (Linux, PREEMPT_RT, `SCHED_FIFO`) comes later, with its own
  measurements. ADR-003's 1 ms / 50 µs figures are withdrawn: nothing measured them.

### 2.5 The manager: the logic module

`plugins/logic` (`mythos-module-logic`), a service module (ADR-011 §3 step 5):

- Programs, data types (moved from the core) and tasks in `<project>/logic/`.
- Check, compile for a target, and run unit tests in ST.
- **The runtimes on this PC**: every runner, possibly several (one per device of a plant
  simulated here, or one per project open), with its state (RUN, STOP, FAULT and why), scan time
  (last, max, overruns) and program version; start, stop, restart.
- Monitor, force and write variables; online change.
- Every route is an MCP tool (ADR-010 §2.5), so an agent can write, test and deploy logic.
- A device's programs are part of its slice (ADR-012 §2.5): downloaded, compared and run again on
  a failed download with the rest of the device.

### 2.6 The editor

The logic module's workbench UI (ADR-011 §2.3): a POU tree, the ST editor with diagnostics from
the compiler, the variable table with each variable's binding to a tag, task settings, check and
test buttons, and live values beside the code. LD, FBD and SFC editors follow, each lowering to
the IR.

### 2.7 Microcontrollers

Firmware for a board family is the VM plus a host for that board (clock, pins, a transport).
The same protocol as the PC runner (download, run, stop, monitor, force), over serial, USB or
TCP, lets the manager treat a board as one more runtime. A board family is a target module.

### 2.8 Closed PLCs

Not now. They run only their vendor's programs, from their vendor's software, over proprietary
download protocols. Exporting through a backend (PLCopen XML for Codesys-based PLCs, a vendor's
dialect) stays possible because the IR is neutral; it needs research per vendor first. Today a
cheap PLC can serve as remote I/O over Modbus TCP.

### 2.9 STruC++ as an oracle

STruC++ is not used to build or run anything. A differential test (by hand, it needs Node, g++
and STruC++) runs each program in `crates/mythos-logic-compiler/tests/differential/` on STruC++
and on our VM for 60 cycles and compares every variable after every cycle. STruC++ is GPL-3.0; it
is run as a separate program and none of its code is copied.

### 2.10 Deliberate differences from STruC++

| Block | IEC 61131-3 ed. 3, Table 45 | STruC++ (as OpenPLC) | Ours |
| :--- | :--- | :--- | :--- |
| CTU | counts while `CV < PVmax` | stops at `PV` | the standard |
| CTD | counts while `CV > PVmin` | stops at `0` | the standard |

A program migrated from OpenPLC that relies on a counter stopping at PV behaves differently here.

## 3. Order of work

0. **Spike** (done, §5): compiler and VM for a subset of ST; measured on a PC and linked for two
   microcontrollers; compared with STruC++.
1. **Writes to field devices**: the core calls a driver's `write`, with the authority rule of
   §2.4 and an MCP tool. Also what HMI setpoints need.
2. **Logic module and runner**: one cyclic task, bindings, RUN/STOP, status, safe values; proven
   against a real Modbus simulator on Linux, macOS and Windows.
3. **ST editor**.
4. **Online**: monitor, force, logic tags in the HMI.
5. **Hot swap and online change; programs in a device's slice**.
6. **Hardening**: watchdog, RETAIN variables across restarts.
7. **ESP32 and RP2040 firmware** on real boards.
8. **More of the language** (functions, user function blocks, arrays, structs, strings, the
   standard library), then **LD, FBD, SFC, IL**; a **C backend**; a Linux real-time mode.

Each step leaves `dev` working.

## 4. Consequences

- No toolchain is installed with MythOS or on a controller to compile logic: the compiler is Rust
  and part of the logic module.
- A controller runs bytecode, not native code: slower than STruC++'s native output (§5), and the
  price of memory safety, one format for every target, and variables enumerable by name.
- The language coverage is ours to build. STruC++ covers far more today (OOP, pointers, strings,
  80+ functions). Until §3 step 8 is far along, a program using those is refused by name.
- Integer and real semantics are pinned by our tests and by the oracle, not by a C++ compiler.

## 5. Status of implementation

| Part | Built |
| :--- | :--- |
| 2.1 Compiler | Spike: one PROGRAM; VAR, VAR_INPUT, VAR_OUTPUT; BOOL, SINT, INT, DINT, LINT, REAL, LREAL, TIME; IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison, BOOL logic; `*_TO_*` between those types; implicit widening per Table 11; diagnostics with line and column for everything else |
| 2.2 Profiles | `pc`, `rp2040`, `esp32`; refusals proven with a test profile |
| 2.3 VM | `no_std`, no allocation, `#![forbid(unsafe_code)]`; loader checks, run-time checks, budget, faults |
| 2.4–2.8 | Not built |
| 2.9 Oracle | `differential_strucpp.rs`: 3 programs, 60 cycles, 2,820 values, all agree except §2.10. It found one bug, fixed: TP kept ET at PT when the pulse ended with IN already FALSE |

Evidence of the spike (2026-11-17):

- **Tests**: `st_runs_on_the_vm.rs` (13), `load_refuses_bad_programs.rs` (8), and unit tests
  (13). Mutation-checked: removing INT wrapping, the TP reset, the loader's address check, the
  division-by-zero fault, or counting CTU only to PV each turns a named test red.
- **Scan time on a PC** (`scan_time.rs`, release, i9-11980HK, Linux, not real-time, desktop
  load): a program shaped like a small plant (CASE sequence, 8 TON, CTU, R_TRIG, a PI loop in
  REAL, a 50-turn FOR) is 884 bytes of code and 280 of memory, 970 instructions per scan:
  mean 8.3 µs, p99 12.3 µs, p99.9 20.5 µs, max 336 µs over 200,000 scans. The VM has not been
  optimised. Windows and macOS: not measured.
- **Microcontrollers** (`scripts/logic-vm-footprint.sh`, CI job `logic-vm-bare-metal`): the VM
  with its loader and scan loop, `opt-level=s`, LTO, linked with `core` alone: **RP2040**
  (thumbv6m-none-eabi) 15,648 bytes of code; **ESP32-C3** (riscv32imc-unknown-none-elf)
  18,950 bytes. Run on a real board: not yet (none attached). The original ESP32 and the S3 are
  Xtensa, which needs Espressif's Rust toolchain: not built.
