# ADR-003: Native STruC++ Runtime Engine Architecture & In-Process FFI Bridge

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Vulcan (VP Hardware), Scribe (VP Publication)

---

## 1. Context and Problem Statement

Industrial automation platforms frequently execute soft-logic through external PLC runtimes or standalone daemons (such as the legacy OpenPLC daemon/server runtime). These external runtimes communicate with supervisory SCADA hosts across loopback network sockets or flat shared memory segments.

Running soft-logic as an external standalone daemon introduces severe operational and engineering deficiencies:
1. **Double Context Switches and Latency**: Polling external PLC daemons across local Modbus TCP introduces 2 to 10 ms latency and network stack overhead.
2. **Coarse Lifecycle Control**: Deploying new control logic requires stopping and restarting the entire daemon process. This restart disconnects field communication drivers and causes physical I/O glitches.
3. **Fragile Process Supervision**: Crashes in an external daemon process cannot be supervised or recovered deterministically from the SCADA microkernel.
4. **Daemon Web and Process Overhead**: External runtime packages include separate web servers, background daemons, and administrative overhead unnecessary for embedded industrial controllers.
5. **Transpiler Scalability Limits**: Legacy runtime compilers generated flat, non-scalable debug variable arrays that hit 32,767-byte memory limits. They also required brittle C-linkage shims for every task and variable.

MythOS requires a deterministic, unified embedded control architecture. MythOS does not embed or depend on external OpenPLC daemons, servers, or web runtimes. Instead, MythOS integrates its own embedded Native STruC++ Runtime Engine (`crates/mythos-strucpp`). This engine consumes C++17 shared object libraries (`.so`) compiled directly by STruC++ (`strucpp`), loads them via `libloading`, and synchronizes variables directly with the MythOS Lock-Free TagBus (`/dev/shm/mythos_tagbus`).

---

## 2. Decision Drivers

- **Sub-Millisecond Execution**: Control cycles must execute reliably at 1 ms ($1000\,\mu\text{s}$) with $< 50\,\mu\text{s}$ jitter.
- **Zero I/O Interruption on Logic Update**: Uploading and activating new Structured Text logic must not drop field bus connections or glitch physical outputs.
- **Deterministic Scheduling**: Control threads must not be preempted by regular OS desktop or background services.
- **Direct Memory Bridging**: Zero-copy synchronization between soft-logic I/O memory (`%IX`, `%QX`, `%MX`) and the MythOS Shared Memory Bus (`/dev/shm/mythos_tagbus`).
- **Modern C++17 Toolchain**: Elimination of legacy C transpilers, adopting the STruC++ compiler and native C++ virtual dispatch runtime.
- **Zero Daemon Dependencies**: The soft-logic runtime must execute in-process inside the MythOS architecture without separate background processes.

---

## 3. Considered Options

1. **Option 1: External PLC Daemon with Modbus TCP Loopback Bridge**
   - *Pros*: Decoupled processes; standard Modbus protocol.
   - *Cons*: 2 to 10 ms latency jitter; Modbus register translation bottlenecks; process restart required for program upload; high CPU overhead; fragile external daemon process supervision.
2. **Option 2: Standalone Process Fork/Exec with IPC Sockets**
   - *Pros*: Process isolation; Unix domain sockets operate faster than TCP.
   - *Cons*: Serialization overhead; process restarts still drop output state; requires managing external process lifecycles.
3. **Option 3: Embedded Native STruC++ Runtime Engine (`crates/mythos-strucpp`) with In-Process FFI (`libloading`) and Linux `SCHED_FIFO` Threading** (Selected)
   - *Pros*: Compiles IEC 61131-3 logic to C++17 shared object (`.so`) via STruC++ (`strucpp compile`); exports a single entrypoint symbol (`strucpp_get_config()`); loaded directly into the MythOS process address space via `libloading`; dynamic hot-reloading during cyclic dead-time via atomic pointer swap; runtime walks native C++ class hierarchy via virtual dispatch; control loops run on dedicated threads with `SCHED_FIFO` priority; direct memory mapping to `/dev/shm/mythos_tagbus`. Eliminates external daemon processes completely.

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Embedded Native STruC++ Runtime Engine with `SCHED_FIFO`).

### 4.1 Architecture Details
- **Compilation Pipeline**: Structured Text (`.st`) compiles directly to C++17 via STruC++ (`strucpp compile`). The compiler emits:
  * `generated.cpp`: POU classes, static configuration instances, and logic solving routines.
  * `generated.hpp`: Class definitions conforming to STruC++ runtime headers (`iec_std_lib.hpp`, `iec_located.hpp`, `debug_dispatch.hpp`).
  * `generated_debug.cpp`: Split debug array tables (.rodata) supporting lazy-paged hierarchical addressing.
  * `debug-map.json`: Full variable hierarchy symbol mapping for debugging and SCADA binding.
  The C++17 artifacts and runtime shim (`runtime_v4_entry.cpp`) compile into a dynamic shared object (`libstrucpp_program_<sha256>.so`) using GCC/Clang with `-O3 -fPIC -shared -std=c++17`.
- **C-Linkage Entry & Virtual Dispatch Model**: The loaded `.so` exports exactly one configuration entrypoint symbol:
  ```cpp
  extern "C" strucpp::ConfigurationInstance* strucpp_get_config(void);
  extern "C" void strucpp_set_locks(pthread_mutex_t* image_tables_mutex, pthread_mutex_t* global_vars_mutex);
  ```
  MythOS invokes `strucpp_get_config()` and navigates the configuration hierarchy (`ConfigurationInstance` -> `ResourceInstance` -> `TaskInstance` -> `ProgramBase`) using native C++ virtual dispatch, eliminating legacy C shim functions.
- **Hierarchical Debugging & Variable Forcing**: Debugging operates over `(arr_idx, elem_idx)` hierarchical coordinates exposed via `debug_dispatch.hpp` (`STRUCPP_V4_DEBUG_EXPORTS_DEFINE`), utilizing STruC++ `IECVar<T>` forcing APIs. This completely eliminates the legacy 32,767-byte single-array limit.
- **Dynamic Hot-Reloading**: MythOS loads the `.so` using `libloading`. When a new program is compiled, pointer swapping occurs during the dead-time window between cycle executions, guaranteeing zero partial-execution states and zero output glitching.
- **Real-Time Threading**: The execution loop executes on dedicated Linux threads with `SCHED_FIFO` priority (85) and CPU core pinning (`pthread_setaffinity_np` or program-declared `CPU_AFFINITY`).
- **Zero-Copy I/O Bridging**: MythOS inspects `strucpp::locatedVars[]` directly from `iec_located.hpp`. Pointers to raw primitive values (`IECVar<T>::raw_ptr()`) for `%IX`, `%QX`, and `%MX` map directly to `/dev/shm/mythos_tagbus` offsets.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Ultra-Low Jitter**: Eliminates network stack and IPC overhead; cycle execution jitter is $< 20\,\mu\text{s}$ on PREEMPT_RT Linux kernels.
- **Glitchless Hot-Swap**: Logic updates live without terminating communication drivers or cycling physical actuator relays.
- **Single Process Deployment**: MythOS ships as a unified binary containing both the SCADA host and the native soft-logic engine. No external PLC daemons or servers required.
- **Scalable Debug Model**: Hierarchical array splitting removes arbitrary variable count limits and supports zero-copy lazy paging in memory.
- **Pure Virtual Dispatch**: Zero boilerplate shim code per project; stable runtime interfaces across user program updates.

### 5.2 Negative Consequences
- **Memory Safety Risk in Unsafe C++ Code**: Faulty user C++ blocks could cause memory corruption. Mitigated by strict STruC++ compiler generation, Clang/GCC hardening flags (`-Werror`, `-fstack-protector-strong`), and memory address bounds checks.
- **Root/CAP_SYS_NICE Requirement**: Pinned `SCHED_FIFO` real-time scheduling requires Linux capabilities (`CAP_SYS_NICE`).
