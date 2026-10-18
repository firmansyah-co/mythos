# MythOS Architecture Specification: Native STruC++ Runtime Engine & In-Process FFI Bridge

- **Document Identifier**: MYTHOS-ARCH-003
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification details the dynamic Foreign Function Interface (FFI) integration between the MythOS Pure Rust Microkernel and the Native STruC++ Runtime Engine (`crates/mythos-strucpp`).

The Native STruC++ Runtime Engine executes soft-logic control programs specified under IEC 61131-3 (Structured Text, Ladder Diagram, Function Block Diagram, Instruction List, and Sequential Function Chart). MythOS integrates STruC++ as an embedded real-time engine rather than an external standalone PLC daemon.

This integration delivers four core capabilities:
1. **Automated STruC++ Compilation Pipeline**: Compiling IEC 61131-3 logic directly into C++17 shared object libraries (`.so`) using the STruC++ (`strucpp`) compiler.
2. **C++ Virtual Dispatch Execution Model**: Executing control programs via a single C-linkage configuration entrypoint (`strucpp_get_config()`) and native C++ virtual dispatch over `ConfigurationInstance`, `ResourceInstance`, `TaskInstance`, and `ProgramBase`.
3. **Deterministic Linux `SCHED_FIFO` Thread-Per-Task Scheduling**: Guaranteeing sub-millisecond control cycles with core pinning (`CPU_AFFINITY`) and cycle overrun protection.
4. **Hierarchical Debugging and Direct TagBus Memory Bridging**: Eliminating legacy array bounds limits via hierarchical variable addressing (`debug_dispatch.hpp`), `IECVar<T>` forcing, and zero-copy `LocatedVar` pointer bridging into `/dev/shm/mythos_tagbus`.

---

## 2. Compilation and Code Generation Pipeline

The Native STruC++ Runtime Engine utilizes the STruC++ (`strucpp`) compiler pipeline. STruC++ compiles Structured Text (`.st`) directly to C++17 source files and structural metadata.

```
IEC 61131-3 Logic
(.st / .xml / .pou)
       │
       ▼
+───────────────────────────+
|     STruC++ Compiler      | (strucpp compile)
+───────────────────────────+
       │
       ├─► generated.cpp         (POU classes, Configuration, Resource, Task instances)
       ├─► generated.hpp         (Type declarations, class definitions, LocatedVar arrays)
       ├─► generated_debug.cpp   (Hierarchical debug array tables in .rodata)
       └─► debug-map.json        (Hierarchical symbol map: path -> [arr_idx, elem_idx])
       │
       ▼
+───────────────────────────+
| STruC++ Runtime Headers   | (iec_std_lib.hpp, iec_located.hpp, debug_dispatch.hpp, ...)
+───────────────────────────+
       │
       ▼
+───────────────────────────+
| Runtime Shim (Static)     | (runtime_v4_entry.cpp: exports strucpp_get_config)
+───────────────────────────+
       │
       ▼
+───────────────────────────+
| Clang / GCC Toolchain     | (g++ -std=c++17 -O3 -fPIC -shared -Wl,-z,now)
+───────────────────────────+
       │
       ▼
libstrucpp_program_<sha256>.so
       │
       ▼
Loaded into MythOS via dlopen()
```

### 2.1 Toolchain Invariants
- **Compiler**: `strucpp compile` (STruC++ IEC 61131-3 to C++17 compiler).
- **Emitted Artifacts**:
  * `generated.cpp`: POU program implementations and static configuration structures.
  * `generated.hpp`: Typed POU declarations, variable members, and located variable declarations.
  * `generated_debug.cpp`: Split hierarchical debug tables placed in `.rodata`.
  * `debug-map.json`: Full variable hierarchy and type descriptors for engineering tools and SCADA binding.
- **Runtime Header Set**: Version-locked headers supplied by STruC++:
  * `iec_std_lib.hpp`: Standard IEC function blocks and mathematical operations.
  * `iec_located.hpp`: Located variable descriptor (`LocatedVar`) definitions and arrays.
  * `debug_dispatch.hpp`: Hierarchical debug dispatcher macros and PDU helpers.
  * `iec_var.hpp`: `IECVar<T>` template wrapper providing atomic variable forcing and raw value access.
  * `iec_threading.hpp`: Mutex definitions and lock guards (`IMAGE_TABLES_LOCK_GUARD`).
- **Binary Toolchain**: G++ or Clang with required flags:
  `-std=c++17 -O3 -march=native -fPIC -shared -Wall -Wextra -Werror -Wl,-z,relro,-z,now`
- **Output Artifact**: `/var/lib/mythos/runtimes/libstrucpp_program_<sha256>.so`

---

## 3. C-Linkage Entry Symbol and Virtual Dispatch Model

### 3.1 Single Entrypoint Symbol Contract

The compiled shared object (`.so`) exposes a strictly minimized C-linkage export surface. MythOS does not require project-specific wrapper functions or legacy procedural shims.

The `.so` exports exactly one configuration entrypoint, one mutex setter, and the hierarchical debug exports:

```cpp
// Static runtime entry shim: core/strucpp_runtime/runtime_v4_entry.cpp
#define STRUCPP_V4_DEBUG_EXPORTS_DEFINE
#include "debug_dispatch.hpp"
#include "iec_threading.hpp"
#include "generated.hpp"

#include <pthread.h>

namespace strucpp {
    pthread_mutex_t* g_image_tables_mutex_ptr = nullptr;
    pthread_mutex_t* g_global_vars_mutex_ptr  = nullptr;
}

static strucpp::Configuration_CONFIG0 g_config;

extern "C" {
    // 1. Single configuration instance entrypoint
    strucpp::ConfigurationInstance* strucpp_get_config(void) {
        return &g_config;
    }

    // 2. Lock plumbing for multi-threaded safety
    void strucpp_set_locks(pthread_mutex_t* image_tables_mutex,
                           pthread_mutex_t* global_vars_mutex) {
        strucpp::g_image_tables_mutex_ptr = image_tables_mutex;
        strucpp::g_global_vars_mutex_ptr  = global_vars_mutex;
    }

    // 3. Hierarchical debug exports (generated by debug_dispatch.hpp)
    uint8_t  strucpp_debug_array_count(void);
    uint16_t strucpp_debug_elem_count(uint8_t arr);
    uint16_t strucpp_debug_size(uint8_t arr, uint16_t elem);
    uint8_t  strucpp_debug_set(uint8_t arr, uint16_t elem, bool forcing,
                               const uint8_t *bytes, uint16_t len);
    uint16_t strucpp_debug_read(uint8_t arr, uint16_t elem, uint8_t *dest);

    // 4. Informational metadata
    extern unsigned long long common_ticktime__;
    extern const char *plc_program_md5;
}
```

### 3.2 Native C++ Virtual Dispatch Hierarchy

MythOS walks the configuration structure directly across DSO boundaries through STruC++ base class vtables.

```cpp
// STruC++ Base Class Hierarchy (defined in iec_std_lib.hpp)

struct ProgramBase {
    virtual void run() = 0;
    virtual ~ProgramBase() = default;
};

struct TaskInstance {
    const char* name;
    uint64_t interval_ns;
    uint32_t priority;
    uint32_t cpu_affinity_mask;
    ProgramBase** programs;
    size_t program_count;
};

struct ResourceInstance {
    const char* name;
    TaskInstance* tasks;
    size_t task_count;
};

struct ConfigurationInstance {
    virtual const char* get_name() const = 0;
    virtual ResourceInstance* get_resources() = 0;
    virtual size_t get_resource_count() const = 0;
    virtual ~ConfigurationInstance() = default;
};
```

### 3.3 Dynamic Dispatch Traversal
Upon dynamic loading, the runtime traverses the loaded hierarchy:
```cpp
// Runtime execution traversal
strucpp::ConfigurationInstance* cfg = strucpp_get_config();
size_t resource_count = cfg->get_resource_count();
strucpp::ResourceInstance* resources = cfg->get_resources();

for (size_t r = 0; r < resource_count; ++r) {
    for (size_t t = 0; t < resources[r].task_count; ++t) {
        auto& task = resources[r].tasks[t];
        // Read metadata: task.name, task.interval_ns, task.priority, task.cpu_affinity_mask
        for (size_t p = 0; p < task.program_count; ++p) {
            // Virtual dispatch directly into compiled logic
            task.programs[p]->run();
        }
    }
}
```

---

## 4. Real-Time Thread-Per-Task Architecture and Linux SCHED_FIFO

Industrial automation requires hard real-time scheduling. Standard Linux kernel scheduling (`SCHED_OTHER`) introduces non-deterministic thread preemption. The Native STruC++ Runtime Engine deploys a dedicated POSIX thread per IEC task under `SCHED_FIFO`.

### 4.1 Thread Configuration Parameters
- **Scheduling Policy**: `SCHED_FIFO` (POSIX real-time First-In, First-Out).
- **Static Priority**: Derived from IEC task priority (mapped to range 80..98; below kernel interrupts).
- **CPU Affinity**: Configured per task using `task.cpu_affinity_mask` (from `CPU_AFFINITY` in Structured Text task definitions) via `pthread_setaffinity_np`.
- **Clock Source**: `CLOCK_MONOTONIC_RAW` via `clock_nanosleep` to avoid NTP time-jump disruptions.

### 4.2 Real-Time Task Host Implementation (Rust / libc)

```rust
use std::os::raw::{c_int, c_void};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub struct RealtimeTaskConfig {
    pub task_name: String,
    pub cycle_time_us: u64,       // e.g., 1000 us (1 ms)
    pub sched_priority: i32,       // e.g., 85
    pub cpu_core_id: usize,        // CPU core affinity mask
    pub overrun_threshold_us: u64, // e.g., 800 us
}

pub struct RealtimeTaskHost {
    config: RealtimeTaskConfig,
    stop_signal: std::sync::atomic::AtomicBool,
}

impl RealtimeTaskHost {
    pub fn new(config: RealtimeTaskConfig) -> Self {
        Self {
            config,
            stop_signal: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Spawns the dedicated Linux OS thread with SCHED_FIFO and CPU pinning
    pub fn spawn<F>(self: std::sync::Arc<Self>, cycle_fn: F) -> std::thread::JoinHandle<()>
    where
        F: FnMut() + Send + 'static,
    {
        std::thread::Builder::new()
            .name(self.config.task_name.clone())
            .spawn(move || {
                self.configure_realtime_thread();
                self.run_cyclic_loop(cycle_fn);
            })
            .expect("Failed to spawn real-time task thread")
    }

    fn configure_realtime_thread(&self) {
        unsafe {
            // 1. Set CPU Core Affinity if configured
            if self.config.cpu_core_id > 0 {
                let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_SET(self.config.cpu_core_id, &mut cpuset);
                let thread_id = libc::pthread_self();
                let aff_res = libc::pthread_setaffinity_np(
                    thread_id,
                    std::mem::size_of::<libc::cpu_set_t>(),
                    &cpuset,
                );
                if aff_res != 0 {
                    warn!("Unable to pin thread to CPU core {}: error {}", self.config.cpu_core_id, aff_res);
                }
            }

            // 2. Set SCHED_FIFO Priority
            let param = libc::sched_param {
                sched_priority: self.config.sched_priority,
            };
            let thread_id = libc::pthread_self();
            let sched_res = libc::pthread_setschedparam(thread_id, libc::SCHED_FIFO, &param);
            if sched_res != 0 {
                error!("CRITICAL: Failed to set SCHED_FIFO priority {}: error {}", self.config.sched_priority, sched_res);
            } else {
                info!("Realtime thread configured: SCHED_FIFO priority {}", self.config.sched_priority);
            }
        }
    }

    fn run_cyclic_loop<F>(&self, mut cycle_fn: F)
    where
        F: FnMut(),
    {
        let cycle_duration = Duration::from_micros(self.config.cycle_time_us);
        let mut next_activation = Instant::now() + cycle_duration;

        while !self.stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
            let cycle_start = Instant::now();

            // Execute the compiled task programs via virtual dispatch
            cycle_fn();

            let cycle_elapsed = cycle_start.elapsed();
            let elapsed_us = cycle_elapsed.as_micros() as u64;

            // Cycle Overrun Detection
            if elapsed_us > self.config.overrun_threshold_us {
                warn!(
                    elapsed_us = elapsed_us,
                    budget_us = self.config.cycle_time_us,
                    "Realtime cycle overrun warning: execution used > {}% of cycle budget",
                    (self.config.overrun_threshold_us * 100) / self.config.cycle_time_us
                );
            }

            // Deterministic sleep until next period using absolute monotonic clock
            let now = Instant::now();
            if next_activation > now {
                let sleep_time = next_activation - now;
                std::thread::sleep(sleep_time);
            } else {
                // Hard cycle slip detected
                warn!("Realtime cycle slip: skipped period interval");
                next_activation = now;
            }

            next_activation += cycle_duration;
        }
    }
}
```


### 4.3 Task Scheduler Staging vs Live Runtime Execution

To preserve operational safety and determinism in industrial environments, MythOS strictly decouples the **Engineering Edit View** from the **Live Production Runtime View** for task scheduling parameters (task intervals, priorities, watchdog limits, CPU core affinities, and program bindings).

Modifying task scheduling parameters in the MythOS Engineering IDE or via the REST API operates exclusively on the staging document (`tasks.draft.json`). The live Linux `SCHED_FIFO` real-time threads running in the Native STruC++ Engine execute against the immutable active descriptor (`tasks.active.json`) and remain completely isolated from editing actions until explicitly promoted.

```
+─────────────────────────────────────────────────────────────────────────────+
|                        ENGINEERING EDIT VIEW (STAGING)                      |
|                                                                             |
|  Engineer changes Task_Fast: interval 100 ms -> 20 ms, CPU Affinity Core 2  |
|  - Saved to: /var/lib/mythos/projects/<id>/tasks.draft.json                 |
|  - Validated by AST & Resource Limits Checker                               |
|  - Runtime State: UNCHANGED (Zero impact on field bus or cyclic loops)      |
+──────────────────────────────────────┬──────────────────────────────────────+
                                       │
                                       │ HTTP POST /api/v1/control/draft/commit
                                       ▼
+─────────────────────────────────────────────────────────────────────────────+
|                     ATOMIC PROMOTION & SCHEDULER RE-ARM                     |
|                                                                             |
|  1. Parse & validate tasks.draft.json against system resource quotas        |
|  2. Compile associated STruC++ logic if code changed -> libstrucpp_prog.so  |
|  3. Scan Cycle Dead-Time Synchronization Barrier (< 50 ms dead-time)        |
|  4. Atomic File Promotion: tasks.draft.json -> tasks.active.json            |
|  5. Reconfigure running SCHED_FIFO pthreads or hot-spawn new task threads   |
|  6. Re-bind CPU affinity (pthread_setaffinity_np) & priorities              |
+──────────────────────────────────────┬──────────────────────────────────────+
                                       │
                                       ▼
+─────────────────────────────────────────────────────────────────────────────+
|                       LIVE RUNTIME EXECUTION (ACTIVE)                       |
|                                                                             |
|  Live Task_Fast running at 20 ms cycle (SCHED_FIFO priority 90, CPU core 2) |
|  - Descriptor: /var/lib/mythos/projects/<id>/tasks.active.json              |
|  - Direct I/O Memory Bridge /dev/shm/mythos_tagbus active                   |
+─────────────────────────────────────────────────────────────────────────────+
```

#### 4.3.1 Task Staging Isolation Contract
1. **Zero Runtime Glitch during Drafting**: Modifications to task cycle intervals (e.g., from 100 ms down to 20 ms, or changing task priority from 80 to 92) do NOT affect the running POSIX threads. The real-time loop continues executing at the active rate without jitter or dropped cycles.
2. **Persistence Boundary**:
   - `tasks.draft.json`: Holds staged configuration under active development or simulation preview.
   - `tasks.active.json`: Read-only configuration descriptor executed by the real-time runtime host.
3. **Dirty State Detection**: The controller tracks cryptographic hashes (`sha256`) of `tasks.draft.json` and `tasks.active.json`. When `draft_hash != active_hash`, the system flags the task subsystem as `DIRTY`.

#### 4.3.2 Atomic Task Scheduler Promotion Sequence (< 50ms Dead-Time)
When the engineer or supervisor issues `POST /api/v1/control/draft/commit`, the controller executes the following atomic sequence:

1. **Pre-Flight Validation**:
   - Verify `tasks.draft.json` syntax and schema integrity.
   - Enforce scheduling feasibility: sum of task cycle utilizations $\sum rac{C_i}{T_i} < 0.85$ to prevent CPU starvation.
   - Verify core affinity masks against physical CPU topology (`/sys/devices/system/cpu/online`).
2. **Sync Barrier at Scan Dead-Time**:
   - The runtime identifies the inter-cycle quiescence window across all active task threads.
   - Soft-logic execution pauses at the completion of the current cycle ($t_{\text{dead-time}}$).
3. **Atomic File Promotion**:
   - Atomically updates on-disk descriptors using POSIX atomic file replacement:
     `rename("/var/lib/mythos/projects/<id>/tasks.draft.json.staged", "/var/lib/mythos/projects/<id>/tasks.active.json")`.
4. **Dynamic Thread Reconfiguration**:
   - Existing tasks with updated cycle times have their `cycle_duration` updated atomically via `AtomicU64` in `RealtimeTaskHost`.
   - Tasks with modified CPU affinity call `pthread_setaffinity_np` immediately without destroying the thread.
   - Tasks with modified priority call `pthread_setschedparam` with updated `sched_priority`.
   - Newly added tasks are spawned with `RealtimeTaskHost::spawn()`.
   - Removed tasks receive `stop_signal.store(true, Ordering::Release)` and are joined cleanly.
5. **Total Quiescence Guarantee**:
   - The entire reconfiguration takes $< 50\,\text{ms}$, well within industrial watchdog fail-safe timeout thresholds ($T_{\text{watchdog}} = 250\,\text{ms}$).
   - TagBus pointers in `/dev/shm/mythos_tagbus` remain valid and locked across the transition, guaranteeing zero actuator glitches or spurious field outputs.

---

## 5. FFI Dynamic Library Hot-Reload Architecture

Hot-reload replaces control logic while maintaining continuous field bus communication.

```
1. Compile new logic via STruC++ -> libstrucpp_program_v2.so
2. Verify C-linkage export: strucpp_get_config()
3. Instantiate and inspect ConfigurationInstance
4. Re-bind LocatedVar pointers to /dev/shm/mythos_tagbus
5. Wait for current cycle dead-time window
6. Atomic Pointer Swap: swap active ConfigurationInstance reference
7. Safely dlclose() previous library in background worker thread
```

### 5.1 Safe Dynamic Library Loader (`DynamicRuntimeLoader`)

```rust
use libloading::{Library, Symbol};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use crate::error::StrucppError;

#[repr(C)]
pub struct ConfigurationInstanceOpaque {
    _private: [u8; 0],
}

type GetConfigFn = unsafe extern "C" fn() -> *mut ConfigurationInstanceOpaque;
type SetLocksFn = unsafe extern "C" fn(*mut libc::pthread_mutex_t, *mut libc::pthread_mutex_t);

pub struct LoadedProgram {
    _lib: Library,
    pub config_ptr: *mut ConfigurationInstanceOpaque,
    pub set_locks: SetLocksFn,
}

pub struct DynamicRuntimeLoader {
    active_program: Arc<AtomicPtr<LoadedProgram>>,
}

impl DynamicRuntimeLoader {
    pub fn new() -> Self {
        Self {
            active_program: Arc::new(AtomicPtr::new(std::ptr::null_mut())),
        }
    }

    /// Load and inspect a newly compiled shared library
    pub unsafe fn load_library(&self, path: &std::path::Path) -> Result<Box<LoadedProgram>, StrucppError> {
        let lib = Library::new(path).map_err(|e| StrucppError::DlOpenFailed(e.to_string()))?;

        let get_config: Symbol<GetConfigFn> = lib.get(b"strucpp_get_config")
            .map_err(|e| StrucppError::SymbolNotFound { symbol: "strucpp_get_config".into(), source: e.to_string() })?;
        let set_locks: Symbol<SetLocksFn> = lib.get(b"strucpp_set_locks")
            .map_err(|e| StrucppError::SymbolNotFound { symbol: "strucpp_set_locks".into(), source: e.to_string() })?;

        let config_ptr = get_config();
        if config_ptr.is_null() {
            return Err(StrucppError::NullConfigPointer);
        }

        Ok(Box::new(LoadedProgram {
            config_ptr,
            set_locks: *set_locks,
            _lib: lib,
        }))
    }

    /// Atomically swap active program pointer during cyclic dead-time
    pub fn hot_swap(&self, new_program: Box<LoadedProgram>) -> Option<Box<LoadedProgram>> {
        let new_ptr = Box::into_raw(new_program);
        let old_ptr = self.active_program.swap(new_ptr, Ordering::AcqRel);

        if old_ptr.is_null() {
            None
        } else {
            // Reconstruct Box to trigger drop/dlclose safely outside real-time thread
            Some(unsafe { Box::from_raw(old_ptr) })
        }
    }
}
```

---

## 6. Hierarchical Debugging and Variable Forcing Architecture

### 6.1 Elimination of the Legacy Single-Array Limit

Legacy PLC compilers generated a single flat `debug_vars[]` array that expanded every composite array element and structure leaf into a monolithic structure. In industrial projects with over 30,000 variable leaves, this caused severe failure modes:
- AVR / GCC object limit exceeded: `error: size of array is too large` (32,767-byte ceiling).
- 16-bit index overflow (`uint16_t` ceiling at 65,535).
- Heavy RAM bloat and slow linear search times.

### 6.2 STruC++ Hierarchical Debugging Model

The Native STruC++ Engine solves this through hierarchical debug addressing:
1. **Multiple Array Partitioning**: `generated_debug.cpp` splits debug entries across multiple arrays, each capped at 8,000 entries.
2. **Read-Only Memory Mapping with Lazy Paging**: On Linux targets, debug arrays reside in `.rodata` of the `.so`. The Linux kernel maps these pages read-only on demand. Scoping a few variables incurs zero RAM overhead for unused arrays.
3. **PDU Coordinate Shims**: Variables are identified by `(arr_idx, elem_idx)` tuples:
   * Function Code 0x41 (`DEBUG_INFO`): Queries array counts and element capacities.
   * Function Code 0x42 (`DEBUG_SET`): Forces or unforces a variable using `IECVar<T>::force(val)` / `unforce()`.
   * Function Code 0x43 (`DEBUG_GET`): Reads current variable value from `IECVar<T>::value_`.
   * Function Code 0x44 (`DEBUG_GET_LIST`): Batch reads multiple hierarchical variables.
   * Function Code 0x45 (`DEBUG_GET_MD5`): Verifies program hash and endianness.
4. **`IECVar<T>` Wrapper**: Every elementary variable in STruC++ is wrapped in `IECVar<T>`:
   ```cpp
   template <typename T>
   class IECVar {
   public:
       T value_;
       T forced_value_;
       bool forced_{false};

       inline T* raw_ptr() { return &value_; }
       inline void force(T val) { forced_value_ = val; value_ = val; forced_ = true; }
       inline void unforce() { forced_ = false; }
   };
   ```

---

## 7. Direct I/O Memory Bridge to MythOS Shared Memory (`/dev/shm/mythos_tagbus`)

### 7.1 `LocatedVar` Descriptor Model

The Native STruC++ Engine exposes located variables through `strucpp::locatedVars[]` defined in `iec_located.hpp`:

```cpp
namespace strucpp {
    enum class LocatedArea : uint8_t {
        Input  = 0,  // %I
        Output = 1,  // %Q
        Memory = 2   // %M
    };

    enum class LocatedSize : uint8_t {
        Bit   = 0,   // %IX, %QX, %MX
        Byte  = 1,   // %IB, %QB, %MB
        Word  = 2,   // %IW, %QW, %MW
        DWord = 3,   // %ID, %QD, %MD
        LWord = 4    // %IL, %QL, %ML
    };

    struct LocatedVar {
        LocatedArea area;
        LocatedSize size;
        uint16_t byte_index;
        uint8_t bit_index;
        uint8_t _reserved[3];
        void* pointer; // Direct pointer to IECVar<T>::value_ via raw_ptr()
    };

    extern LocatedVar locatedVars[];
    extern uint32_t locatedVarsCount;
}
```

### 7.2 Zero-Copy TagBus Bridging

MythOS iterates `strucpp::locatedVars[]` directly during runtime initialization and dynamic hot-swap:

```
+───────────────────────────────────────────────────────────+
| STruC++ Located Variables (strucpp::locatedVars[])        |
|   %IX0.0 ... %IX127.7   -> pointer = IECVar<bool>::raw_ptr()
|   %IW0   ... %IW1023    -> pointer = IECVar<uint16_t>::raw_ptr()
|   %QX0.0 ... %QX127.7   -> pointer = IECVar<bool>::raw_ptr()
|   %QW0   ... %QW1023    -> pointer = IECVar<uint16_t>::raw_ptr()
+───────────────────────────────────────────────────────────+
                             │
                             │ Direct pointer registration
                             ▼
+───────────────────────────────────────────────────────────+
| MythOS Shared Memory Bus (/dev/shm/mythos_tagbus)         |
|   TagSlot[0..N] (64-byte aligned, lock-free triple buffer)|
+───────────────────────────────────────────────────────────+
```

### 7.3 Cyclic I/O Synchronization Sequence
1. **Cycle Start ($t = 0\,\mu\text{s}$)**:
   - Microkernel / protocol drivers update process inputs in `/dev/shm/mythos_tagbus`.
   - `IoBufferBridge` transfers TagBus values directly into `LocatedArea::Input` pointers via `raw_ptr()`.
2. **Logic Execution ($0\,\mu\text{s} < t < 250\,\mu\text{s}$)**:
   - Dedicated `SCHED_FIFO` task thread executes `task.programs[p]->run()` via virtual dispatch.
   - POU logic evaluates equations against `IECVar<T>` memory blocks.
3. **Cycle Commit ($t = 250\,\mu\text{s}$)**:
   - `IoBufferBridge` reads `LocatedArea::Output` pointers.
   - Outputs are committed to the `/dev/shm/mythos_tagbus` write buffer, and the clean buffer pointer is atomically swapped.
   - Outbound field protocol drivers (Modbus, PROFINET, MQTT) dispatch fresh actuator outputs to physical devices.
