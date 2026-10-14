# MythOS Architecture Specification: Cross-Platform Hardware Abstraction Layer & Containerization

- **Document Identifier**: MYTHOS-ARCH-007
- **Version**: 1.1.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification establishes the **Platform Abstraction Layer (PAL)** and containerization standards for MythOS.

While industrial field deployments predominantly utilize Linux running on bare-metal Industrial PCs (IPCs) or Raspberry Pi compute modules with real-time kernel extensions (`PREEMPT_RT`), control engineers author, simulate, and commission systems on diverse operating systems—principally **Windows 11** on field laptops, **macOS** on workstation laptops, and **Docker containers** on enterprise server clusters.

MythOS enforces an OS-Agnostic architecture: the platform runs with identical user experience, high-performance web tooling, and deterministic state logic across Windows, macOS, Linux, and Docker environments.

---

## 2. Platform Abstraction Layer (PAL) Architecture

MythOS separates operating system dependencies into the `mythos-pal` subsystem within the microkernel workspace (`crates/mythos-pal` or `crates/mythos-core/src/pal`). Operating system divergence is strictly encapsulated behind safe, unified Rust traits using compile-time conditional compilation (`#[cfg(target_os = "...")]`).

```
+----------------------------------------------------------------------------------------------------+
|                                    MYTHOS CROSS-PLATFORM PAL                                       |
|                                                                                                    |
|  +--------------------+  +--------------------+  +--------------------+  +--------------------+    |
|  |   Shared Memory    |  |  Real-Time Timing  |  |  Network Discovery |  | Logic Compilation  |    |
|  |     (TagBus)       |  |    (Scheduler)     |  |     (Scanner)      |  |     (STruC++)      |    |
|  +--------------------+  +--------------------+  +--------------------+  +--------------------+    |
|            |                       |                       |                       |               |
|            v                       v                       v                       v               |
|  +----------------------------------------------------------------------------------------------+  |
|  |                             Platform Abstraction Trait Interface                             |  |
|  |            PlatformShm             PlatformScheduler          PlatformDiscovery              |  |
|  +----------------------------------------------------------------------------------------------+  |
|            |                       |                       |                       |               |
|     +------+------+         +------+------+         +------+------+         +------+------+        |
|     |             |         |             |         |             |         |             |        |
|     v             v         v             v         v             v         v             v        |
|  [ Linux ]   [ Windows ] [ macOS ]   [ Docker ] [ Linux ]   [ Windows ] [ Linux ]   [ Windows ]    |
|  POSIX SHM    Named SHM   tmpfs mmap  Host SHM   SCHED_FIFO  REALTIME    ELF .so     PE .dll       |
+----------------------------------------------------------------------------------------------------+
```

---

## 3. Subsystem Cross-Platform Mapping Matrix

| Subsystem | Linux (Field IPC / Bare-Metal) | Windows 11 (Field Laptop) | macOS (Workstation) | Docker Container |
| :--- | :--- | :--- | :--- | :--- |
| **Shared Memory Bus** | POSIX `shm_open()` + `/dev/shm` (tmpfs) | Windows Named File Mapping (`CreateFileMappingW`) | Memory-mapped anonymous pages (`mmap`) | Host IPC bind (`--ipc=host`) or `/dev/shm` mount |
| **Real-Time Scheduling** | Linux `SCHED_FIFO` (Priority 85) via `sched_setscheduler` | `THREAD_PRIORITY_TIME_CRITICAL` via Windows API | `THREAD_TIME_CONSTRAINT_POLICY` via Mach kernel | Linux `SCHED_FIFO` via Docker `--cap-add=SYS_NICE` |
| **Thread Core Pinning** | `pthread_setaffinity_np` (Linux CPU sets) | `SetThreadAffinityMask` (Windows Win32) | macOS thread affinity tag hinting (`THREAD_AFFINITY_POLICY`) | `pthread_setaffinity_np` with host CPU access |
| **Compiled Logic Output** | Dynamic Shared Object (`.so` / ELF64) | Dynamic Link Library (`.dll` / PE32+) | Dynamic Library (`.dylib` / Mach-O) | Dynamic Shared Object (`.so` / ELF64) |
| **Layer 2 DCP Discovery** | Linux Raw Sockets (`AF_PACKET`, `SOCK_RAW`) | WinPcap / Npcap raw packet driver fallback | BPF (Berkeley Packet Filter) / mDNS fallback | Container `--net=host` + `--cap-add=NET_RAW` |
| **Layer 3 mDNS Discovery** | Standard Multicast UDP 5353 socket | Standard Multicast UDP 5353 socket | Native Apple Bonjour / UDP 5353 socket | Standard Multicast UDP 5353 socket |
| **Serial Bus Access** | Standard termios (`/dev/ttyUSB*`, `/dev/ttyS*`) | Win32 Communications API (`COM1`..`COM256`) | IOKit Serial Stream (`/dev/tty.usbserial*`) | Passthrough volume (`/dev/ttyUSB0`) |
| **Storage Root Directory** | `/var/lib/mythos/` | `%ProgramData%\MythOS\` or `%APPDATA%\MythOS\` | `~/Library/Application Support/MythOS/` | Container volume mount (`/var/lib/mythos/`) |

---

## 4. Rust PAL Trait Contracts & Implementation Specifications

### 4.1 Safe Shared Memory Trait (`PlatformShm`)

The TagBus ring-buffer requires deterministic zero-copy memory access across processes regardless of kernel primitives:

```rust
pub trait PlatformShm: Send + Sync {
    /// Allocate or map an existing shared memory segment of specified byte size.
    fn open_or_create(name: &str, size_bytes: usize) -> Result<Self, PalError>
    where
        Self: Sized;

    /// Direct raw pointer to the memory-mapped region.
    fn as_ptr(&self) -> *mut u8;

    /// Total byte length of the mapped memory segment.
    fn len(&self) -> usize;

    /// Flush pending writes to storage or backing memory if applicable.
    fn flush(&self) -> Result<(), PalError>;
}
```

#### OS-Specific Implementations:
1. **Linux (`target_os = "linux"`)**:
   - Uses `libc::shm_open` with `O_CREAT | O_RDWR`, followed by `libc::ftruncate` and `libc::mmap` (`MAP_SHARED`).
   - Clean unlinking via `libc::shm_unlink` upon shutdown. Backed by `/dev/shm` tmpfs for microsecond latency.
2. **Windows (`target_os = "windows"`)**:
   - Uses `windows_sys::Win32::System::Memory::CreateFileMappingW` with `PAGE_READWRITE` and `INVALID_HANDLE_VALUE` (pagefile-backed named shared memory).
   - Maps view via `MapViewOfFile` with `FILE_MAP_ALL_ACCESS`.
   - Windows Named Shared Memory uses kernel namespace prefix `Local\MythOS_<name>`.
3. **macOS (`target_os = "macos"`)**:
   - Uses POSIX `shm_open` or anonymous memory mapping (`mmap` with `MAP_ANON | MAP_SHARED`).
   - MacOS limits POSIX shared memory names to 31 characters (`SHM_NAME_MAX`). The PAL hashes long identifiers with BLAKE3/SHA-256 to guarantee conformance.

### 4.2 Real-Time Scheduling Trait (`PlatformScheduler`)

Deterministic industrial control loops require bounded execution jitter (< 100 microseconds for 1ms cycles):

```rust
pub trait PlatformScheduler {
    /// Elevate calling thread to deterministic real-time execution priority.
    fn set_realtime_priority(priority_level: u32) -> Result<(), PalError>;

    /// Bind calling thread to a specific hardware CPU core.
    fn pin_to_core(core_id: usize) -> Result<(), PalError>;

    /// High-resolution monotonic sleep guaranteeing minimal timer overrun.
    fn sleep_nanos(nanoseconds: u64);
}
```

#### OS-Specific Implementations:
1. **Linux (`target_os = "linux"`)**:
   - Uses `sched_setscheduler(0, SCHED_FIFO, &sched_param)` with priority 85.
   - Core affinity via `pthread_setaffinity_np` with `cpu_set_t`.
   - High-resolution timing via `clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, ...)`.
2. **Windows (`target_os = "windows"`)**:
   - Uses `SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)`.
   - Thread execution state flagged with `SetThreadExecutionState(ES_SYSTEM_REQUIRED | ES_CONTINUOUS)` to prevent power sleep.
   - Core pinning via `SetThreadAffinityMask(GetCurrentThread(), 1 << core_id)`.
   - Windows timer resolution elevated to 0.5ms via `timeBeginPeriod(1)`.
3. **macOS (`target_os = "macos"`)**:
   - Uses Mach kernel thread policies: `thread_policy_set(mach_thread_self(), THREAD_TIME_CONSTRAINT_POLICY, ...)`.
   - Configures `period`, `computation`, `constraint`, and `preemptible` parameters for hard real-time scheduling deadlines.
   - Core affinity hints set via `THREAD_AFFINITY_POLICY`.

---

## 5. Docker Architecture & First-Class Containerization

MythOS provides official multi-architecture Docker container images supporting both **x86_64** (AMD64) and **aarch64** (ARM64 for Raspberry Pi 4/5, CM4, and industrial gateways).

### 5.1 Production Multi-Stage `Dockerfile`

The container build pipeline produces a lightweight, self-contained, hardened runtime image (<= 45 MB uncompressed):

```dockerfile
# Stage 1: Build the Web Studio Frontend
FROM node:22-alpine AS frontend-builder
WORKDIR /build/frontend
RUN corepack enable && corepack prepare pnpm@latest --activate
COPY frontend/packages/ui-shell/package.json ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm --filter @mythos/ui-shell build

# Stage 2: Compile the Rust Microkernel and First-Party Plugins
FROM rust:1.82-bookworm AS backend-builder
WORKDIR /build/backend
RUN apt-get update && apt-get install -y clang libclang-dev pkg-config
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY plugins/ ./plugins/
RUN cargo build --release --workspace

# Stage 3: Minimal Production Runtime
FROM debian:bookworm-slim AS runtime
LABEL maintainer="adrianaryaputra <adrian@firmansyah.co>"
LABEL org.opencontainers.image.source="https://github.com/firmansyahco/mythos"
LABEL org.opencontainers.image.licenses="MPL-2.0"

# Install runtime dependencies: Clang/GCC toolchain for STruC++ dynamic compilation
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    clang \
    libc6-dev \
    libpthread-stubs0-dev \
    && rm -rf /var/lib/apt/lists/*

# Setup default directories
WORKDIR /opt/mythos
RUN mkdir -p /var/lib/mythos/projects /var/lib/mythos/catalog /var/log/mythos

# Copy compiled binaries and frontend bundle
COPY --from=backend-builder /build/backend/target/release/mythos-server /opt/mythos/bin/
COPY --from=frontend-builder /build/frontend/packages/ui-shell/dist /opt/mythos/web/
COPY catalog/ /var/lib/mythos/catalog/

ENV PATH="/opt/mythos/bin:${PATH}"
ENV MYTHOS_STORAGE_ROOT="/var/lib/mythos"
ENV MYTHOS_WEB_ROOT="/opt/mythos/web"
ENV MYTHOS_HTTP_PORT=8080

EXPOSE 8080 502 4840
VOLUME ["/var/lib/mythos"]

ENTRYPOINT ["mythos-server"]
CMD ["--run"]
```

### 5.2 Production Docker Compose Manifest (`compose.yaml`)

```yaml
version: "3.8"

services:
  mythos:
    image: firmansyahco/mythos:latest
    container_name: mythos-scada
    restart: unless-stopped
    
    # Required for Layer 2 DCP Ethernet Discovery and native network interfaces
    network_mode: host
    
    # Required for ultra-fast lock-free shared memory TagBus (/dev/shm)
    ipc: host
    
    # Linux capabilities for real-time SCHED_FIFO scheduling and raw sockets
    cap_add:
      - SYS_NICE       # Real-time thread scheduling priority
      - NET_RAW        # Raw Ethernet socket transmission (PROFINET DCP)
      - NET_ADMIN      # Network interface configuration
    
    # Device pass-through for serial RS-485 interfaces (optional)
    devices:
      - "/dev/ttyUSB0:/dev/ttyUSB0"
    
    # Persistent project data, recipes, and time-series historian archive
    volumes:
      - mythos_data:/var/lib/mythos
      - /etc/timezone:/etc/timezone:ro
      - /etc/localtime:/etc/localtime:ro
      
    environment:
      - RUST_LOG=info
      - MYTHOS_HTTP_PORT=8080
      - MYTHOS_STORAGE_ROOT=/var/lib/mythos

volumes:
  mythos_data:
    name: mythos_production_data
```

---

## 6. Summary of Extensibility Guarantees

1. **Zero Linux Lock-In**: Code authored on a Windows laptop or macOS workstation executes without syntax or architectural modification when deployed to a Linux industrial controller.
2. **Deterministic Fallbacks**: Where hard real-time capabilities are absent (e.g. standard Windows desktop timer granularity), MythOS activates deterministic simulation timers to ensure logic execution integrity.
3. **Container Native**: Complete Docker integration allows zero-dependency deployment in less than 10 seconds on industrial cloud-edge nodes.
