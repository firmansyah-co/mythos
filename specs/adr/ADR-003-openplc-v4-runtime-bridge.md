# ADR-003: OpenPLC Runtime v4 Embedded FFI and Dynamic Real-Time Bridge

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Vulcan (VP Hardware), Scribe (VP Publication)

---

## 1. Context and Problem Statement

OpenPLC Runtime v4 is an open-source soft-logic PLC runtime supporting IEC 61131-3 languages. In standard deployments, OpenPLC runs as a separate C++ daemon communicating with external SCADA systems via Modbus TCP loopback or shared memory blocks.

However, running OpenPLC as a standalone external daemon creates severe engineering limitations:
1. **Double Context Switches and Latency**: Polling OpenPLC over local Modbus TCP introduces 2 to 10 ms latency and network stack overhead.
2. **Coarse Lifecycle Control**: Deploying new PLC logic requires stopping and restarting the entire OpenPLC process, disconnecting field drivers and causing I/O interruptions.
3. **Weak Process Supervision**: Crashes in the OpenPLC daemon are difficult to recover gracefully from a separate SCADA orchestrator.
4. **Legacy Compiler Scalability Bottlenecks**: Previous generation PLC runtimes generated flat, non-scalable debug variable arrays hitting the 32,767-byte memory limit, lacked object-oriented IEC extensions, and required complex C-linkage shims for every task and variable.

MythOS requires a tight, deterministically scheduled integration based on the modern OpenPLC Runtime v4 STruC++ (`strucpp`) compiler pipeline that preserves real-time control cycle guarantees while allowing live hot-reloading of PLC programs.

---

## 2. Decision Drivers

- **Sub-Millisecond Execution**: Control cycles must execute reliably at 1 ms ($1000 \mu\text{s}$) with $< 50 \mu\text{s}$ jitter.
- **Zero I/O Interruption on Logic Update**: Uploading and activating new Structured Text logic must not drop field bus connections or glitch physical outputs.
- **Deterministic Scheduling**: Control threads must not be preempted by regular OS desktop or background services.
- **Direct Memory Bridging**: Zero-copy synchronization between PLC I/O memory (`%IX`, `%QX`, `%MX`) and the MythOS Shared Memory Bus (`/dev/shm/mythos_tagbus`).
- **Modern C++17 Toolchain**: Elimination of legacy C transpilers, adopting the STruC++ TypeScript compiler and native C++ virtual dispatch runtime.

---

## 3. Considered Options

1. **Option 1: External Daemon with Modbus TCP Loopback Bridge**
   - *Pros*: Completely decoupled processes; uses standard Modbus protocol.
   - *Cons*: 2-10 ms latency jitter; Modbus register translation bottlenecks; process restart required for program upload; high CPU overhead.
2. **Option 2: Process Fork/Exec with IPC Sockets**
   - *Pros*: OpenPLC remains separate; unix domain sockets are faster than TCP.
   - *Cons*: Serialization overhead; process restarts still drop output state.
3. **Option 3: Embedded STruC++ Runtime v4 FFI Bridge (`dlopen`) with Linux `SCHED_FIFO` Threading** (Selected)
   - *Pros*: Compiles IEC 61131-3 logic to C++17 shared object (`.so`) via STruC++ (`strucpp compile`); exports a single entrypoint symbol (`strucpp_get_config()`); loaded directly into the MythOS process address space via `dlopen`; dynamic hot-reloading during cyclic dead-time via atomic pointer swap; runtime walks native C++ class hierarchy via virtual dispatch; control loops run on dedicated threads with `SCHED_FIFO` priority.

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Embedded STruC++ Runtime v4 FFI Bridge with `SCHED_FIFO`).

### 4.1 Architecture Details
- **Compilation Pipeline**: Structured Text (`.st`) is compiled directly to C++17 via STruC++ (`strucpp compile`). The compiler emits:
  * `generated.cpp`: POU classes, static configuration instances, and logic solving routines.
  * `generated.hpp`: Class definitions conforming to STruC++ runtime headers (`iec_std_lib.hpp`, `iec_located.hpp`, `debug_dispatch.hpp`).
  * `generated_debug.cpp`: Split debug array tables (.rodata) supporting lazy-paged hierarchical addressing.
  * `debug-map.json`: Full variable hierarchy symbol mapping for debugging and SCADA binding.
  The C++17 artifacts and runtime shim (`runtime_v4_entry.cpp`) are compiled into a dynamic shared object (`libopenplc_program_<sha256>.so`) using GCC/Clang with `-O3 -fPIC -shared -std=c++17`.
- **C-Linkage Entry & Virtual Dispatch Model**: The loaded `.so` exports exactly one configuration entrypoint symbol:
  ```cpp
  extern "C" strucpp::ConfigurationInstance* strucpp_get_config(void);
  extern "C" void strucpp_set_locks(pthread_mutex_t* image_tables_mutex, pthread_mutex_t* global_vars_mutex);
  ```
  MythOS invokes `strucpp_get_config()` and navigates the configuration hierarchy (`ConfigurationInstance` -> `ResourceInstance` -> `TaskInstance` -> `ProgramBase`) using native C++ virtual dispatch, eliminating legacy C shim functions.
- **Hierarchical Debugging & Variable Forcing**: Debugging operates over `(arr_idx, elem_idx)` hierarchical coordinates exposed via `debug_dispatch.hpp` (`STRUCPP_V4_DEBUG_EXPORTS_DEFINE`), utilizing STruC++ `IECVar<T>` forcing APIs. This completely eliminates the legacy 32,767-byte single-array limit.
- **Dynamic Hot-Reloading**: MythOS loads the `.so` using `libloading`. When a new program is compiled, pointer swapping occurs during the dead-time window between cycle executions, guaranteeing zero partial-execution states.
- **Real-Time Threading**: The execution loop executes on dedicated Linux threads with `SCHED_FIFO` priority (85) and CPU core pinning (`pthread_setaffinity_np` or program-declared `CPU_AFFINITY`).
- **Zero-Copy I/O Bridging**: MythOS inspects `strucpp::locatedVars[]` directly from `iec_located.hpp`. Pointers to raw primitive values (`IECVar<T>::raw_ptr()`) for `%IX`, `%QX`, and `%MX` are mapped directly to `/dev/shm/mythos_tagbus` offsets.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Ultra-Low Jitter**: Eliminates network stack and IPC overhead; cycle execution jitter is $< 20 \mu\text{s}$ on PREEMPT_RT Linux kernels.
- **Glitchless Hot-Swap**: Logic can be updated live without terminating communication drivers or cycling physical actuator relays.
- **Single Process Deployment**: MythOS ships as a unified binary containing both the SCADA host and the soft-logic controller.
- **Scalable Debug Model**: Hierarchical array splitting removes arbitrary variable count limits and supports zero-copy lazy paging in memory.
- **Pure Virtual Dispatch**: Zero boilerplate shim code per project; stable runtime interfaces across user program updates.

### 5.2 Negative Consequences
- **Memory Safety Risk in Unsafe C++ Code**: Faulty user C++ blocks could cause memory corruption. Mitigated by strict STruC++ compiler generation, Clang/GCC hardening flags (`-Werror`, `-fstack-protector-strong`), and memory address bounds checks.
- **Root/CAP_SYS_NICE Requirement**: Pinned `SCHED_FIFO` real-time scheduling requires Linux capabilities (`CAP_SYS_NICE`).
