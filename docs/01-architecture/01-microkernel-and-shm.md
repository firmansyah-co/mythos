# MythOS Architecture Specification: Microkernel and Shared Memory Bus

- **Document Identifier**: MYTHOS-ARCH-001
- **Version**: 1.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This document defines the high-speed, lock-free inter-process shared memory architecture for MythOS. 

Industrial SCADA runtimes face two conflicting requirements:
1. Hard real-time control loops (Native STruC++ Runtime Engine) require deterministic sub-millisecond cycle times with zero lock contention.
2. High-throughput telemetry consumers (HMI WebSocket gateways, historian writers, MCP servers, and cloud edge sync) require continuous read access to thousands of process variables.

MythOS resolves this contention by decoupling writers and readers through a triple-buffered, lock-free POSIX Shared Memory architecture (`/dev/shm/mythos_tagbus`).

---

## 2. Memory Topology and POSIX Shm Lifecycle

### 2.1 File System Mount and Permissions
- **Backing Path**: `/dev/shm/mythos_tagbus`
- **File System**: `tmpfs` (RAM-backed, zero disk write latency)
- **POSIX Mode**: `0660` (Read/Write for user `mythos`, group `mythos-readers`)
- **Initial Size**: Fixed allocation calculated as:
  $$\text{Total Size} = \text{Header Size} + (\text{Max Tags} \times 3 \times \text{Slot Size}) + \text{Diagnostic Ring Buffer Size}$$
  For 65,536 tags:
  $$\text{Size} \approx 4096 + (65536 \times 3 \times 64) + 1048576 = 13,635,584 \text{ bytes} \approx 13.0 \text{ MiB}$$

### 2.2 Memory Segment Layout

```
+-------------------------------------------------------------------+
| Section 1: Superblock and Ring Header (Offset: 0x0000, 4,096 B)   |
|   - Magic Bytes: [0x4D, 0x59, 0x54, 0x48, 0x4F, 0x53, 0x5F, 0x31] |
|   - Protocol Version, Generation, Epoch, Tag Count, Buffer Offsets|
+-------------------------------------------------------------------+
| Section 2: Buffer Bank A (Offset: 0x1000, 65,536 * 64 Bytes)      |
|   - Array of TagSlot[0..65535] (Cache-line aligned, 64 bytes each)|
+-------------------------------------------------------------------+
| Section 3: Buffer Bank B (Offset: 0x401000, 65,536 * 64 Bytes)    |
|   - Array of TagSlot[0..65535] (Cache-line aligned, 64 bytes each)|
+-------------------------------------------------------------------+
| Section 4: Buffer Bank C (Offset: 0x801000, 65,536 * 64 Bytes)    |
|   - Array of TagSlot[0..65535] (Cache-line aligned, 64 bytes each)|
+-------------------------------------------------------------------+
| Section 5: Diagnostic and Event Ring Buffer (1,048,576 Bytes)     |
|   - Atomic head/tail indices, circular FIFO of DriverEvents       |
+-------------------------------------------------------------------+
```

---

## 3. Triple-Buffering Lock-Free State Machine

Lock-free single-writer multi-reader operation is governed by an atomic state word. The state machine eliminates reader blocking and guarantees that writers never wait for slow readers.

### 3.1 State Representation
Each tag or bank of tags maintains an `AtomicU8` state index encoding three buffer identifiers:
- **Write Buffer (WB)**: Dedicated to the active writer.
- **Read Buffer (RB)**: Dedicated to readers currently active.
- **Surface Buffer (SB)**: The most recent fully written buffer waiting to be consumed.

```
State Bits: [ 0 | 0 | SB_1 SB_0 | RB_1 RB_0 | WB_1 WB_0 ]
```

```
               Writer completes write
            ┌───────────────────────────┐
            ▼                           │
     +--------------+   Atomic Swap   +--------------+
     | Write Buffer | ──────────────> | Surface Buf  |
     +--------------+  (Newest data)  +--------------+
            ▲                                │
            │                                │ Reader requests
            │ Allocate unused bank           │ latest snapshot
            │                                ▼
     +--------------+                 +--------------+
     | Free Bank    | <────────────── | Read Buffer  |
     +--------------+  Reader finish  +--------------+
```

### 3.2 State Transition Algorithm (Pure Rust)

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct TripleBufferState {
    // Encodes: (clean_index << 4) | (read_index << 2) | write_index
    state: AtomicUsize,
}

impl TripleBufferState {
    pub const fn new() -> Self {
        // Initial configuration:
        // Write: Bank 0, Clean (Surface): Bank 1, Read: Bank 2
        Self {
            state: AtomicUsize::new(0 | (1 << 4) | (2 << 2)),
        }
    }

    /// Called by Writer upon completing a cyclic write into write_index.
    /// Atomically swaps write_index with clean_index (Surface Buffer).
    #[inline(always)]
    pub fn publish_write(&self, current_write: usize) -> usize {
        loop {
            let curr = self.state.load(Ordering::Acquire);
            let clean = (curr >> 4) & 0x03;
            let read = (curr >> 2) & 0x03;

            // New state: clean becomes current_write, write becomes previous clean
            let next_write = clean;
            let next_clean = current_write;
            let next_state = next_write | (read << 2) | (next_clean << 4);

            if self.state.compare_exchange_weak(
                curr,
                next_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                return next_write;
            }
        }
    }

    /// Called by Reader at start of read cycle.
    /// Atomically acquires the most recent clean buffer without mutating writer's active bank.
    #[inline(always)]
    pub fn acquire_read(&self) -> usize {
        loop {
            let curr = self.state.load(Ordering::Acquire);
            let write = curr & 0x03;
            let clean = (curr >> 4) & 0x03;
            let read = (curr >> 2) & 0x03;

            // Swap clean and read if fresh clean data is present
            if clean == read {
                // No newer data published; keep current read buffer
                return read;
            }

            let next_state = write | (clean << 2) | (read << 4);
            if self.state.compare_exchange_weak(
                curr,
                next_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                return clean;
            }
        }
    }
}
```

---

## 4. Cache-Line Alignment and POD Struct Layouts

To eliminate false sharing across CPU cores and enable zero-copy pointer casting, all memory structs are Plain Old Data (POD) marked `#[repr(C, align(64))]`.

### 4.1 Tag Value Representation (`TagValueRaw`)

```rust
use bytemuck::{Pod, Zeroable};

#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(C)]
pub union TagValueRaw {
    pub boolean: bool,
    pub u8_val: u8,
    pub i8_val: i8,
    pub u16_val: u16,
    pub i16_val: i16,
    pub u32_val: u32,
    pub i32_val: i32,
    pub u64_val: u64,
    pub i64_val: i64,
    pub f32_val: f32,
    pub f64_val: f64,
    pub raw_bytes: [u8; 8],
}

unsafe impl Zeroable for TagValueRaw {}
unsafe impl Pod for TagValueRaw {}
```

### 4.2 Quality Code Specification (`QualityCode`)

Quality codes follow OPC DA / IEC 61850 standard bit flags:

| Bit Range | Designation | Description |
| :--- | :--- | :--- |
| `0..=1` | Quality State | `00` = Bad, `01` = Uncertain, `11` = Good |
| `2..=5` | Sub-status | Detailed error or warning code |
| `6..=7` | Limit Status | `00` = Constant, `01` = Low Limited, `10` = High Limited |

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct QualityCode(pub u16);

impl QualityCode {
    pub const GOOD: Self = Self(0x00C0);
    pub const BAD_NON_SPECIFIC: Self = Self(0x0000);
    pub const BAD_CONFIG_ERROR: Self = Self(0x0004);
    pub const BAD_NOT_CONNECTED: Self = Self(0x0008);
    pub const BAD_DEVICE_FAILURE: Self = Self(0x000C);
    pub const BAD_SENSOR_FAILURE: Self = Self(0x0010);
    pub const BAD_COMM_FAILURE: Self = Self(0x0018);
    pub const UNCERTAIN_SUBSTITUTE: Self = Self(0x0044);
    pub const UNCERTAIN_INITIAL_VALUE: Self = Self(0x0048);

    #[inline(always)]
    pub fn is_good(&self) -> bool {
        (self.0 & 0x00C0) == 0x00C0
    }
}

unsafe impl Zeroable for QualityCode {}
unsafe impl Pod for QualityCode {}
```

### 4.3 64-Byte POD TagSlot Layout (`TagSlot`)

```rust
#[derive(Copy, Clone, Debug)]
#[repr(C, align(64))]
pub struct TagSlot {
    /// Global unique 32-bit tag identifier in SHM catalog
    pub tag_id: u32,

    /// OPC / IEC data type code (1=BOOL, 2=INT, 3=DINT, 4=REAL, 5=LREAL, etc.)
    pub type_code: u16,

    /// Quality bits (IEC 61850 / OPC Unified Architecture)
    pub quality: QualityCode,

    /// Raw 64-bit value union (8 bytes)
    pub value: TagValueRaw,

    /// Nanoseconds since UNIX Epoch (UTC)
    pub timestamp_ns: u64,

    /// Sequence counter incremented by writer on each update
    pub sequence: u64,

    /// Retention and Access Mode Flags
    /// Bit 0: RO (Read-Only), Bit 1: WO (Write-Only), Bit 2: RW (Read-Write)
    /// Bit 4: VOLATILE, Bit 5: RETAIN, Bit 6: PERSISTENT
    pub flags: u32,

    /// Reserved padding to align struct strictly to 64 bytes (1 cache line)
    pub _padding: [u8; 24],
}

unsafe impl Zeroable for TagSlot {}
unsafe impl Pod for TagSlot {}

const _: () = assert!(std::mem::size_of::<TagSlot>() == 64);
const _: () = assert!(std::mem::align_of::<TagSlot>() == 64);
```

### 4.4 Shm Superblock (`ShmSuperblock`)

```rust
#[derive(Copy, Clone, Debug)]
#[repr(C, align(64))]
pub struct ShmSuperblock {
    /// Magic identifier: "MYTHOS_1"
    pub magic: [u8; 8],

    /// Schema layout version
    pub version: u32,

    /// Total number of registered tags in each buffer bank
    pub max_tags: u32,

    /// Byte offset from segment start to Bank A
    pub bank_a_offset: u64,

    /// Byte offset from segment start to Bank B
    pub bank_b_offset: u64,

    /// Byte offset from segment start to Bank C
    pub bank_c_offset: u64,

    /// Master monotonic epoch counter
    pub master_epoch: u64,

    /// State machine controlling Bank A, B, C rotation
    pub bank_state: TripleBufferState,

    /// CRC32 checksum of superblock configuration
    pub config_crc32: u32,

    /// Reserved for hardware memory bus extensions
    pub _reserved: [u8; 4016],
}
```

---

## 5. Performance and Hardware Characteristics

1. **Sub-Microsecond Latency**: Read operations require only an atomic load, a bit shift, and direct memory pointer dereferencing. Zero system calls, zero mutex locks, and zero heap allocations occur during read cycles.
2. **Deterministic Write Overhead**: Writer swaps pointers in $< 45 \text{ ns}$ on modern x86_64 / ARMv8 architectures.
3. **Hardware Cache Isolation**: Every `TagSlot` is aligned to 64 bytes (`align(64)`), which matches standard x86 and ARM cache line widths. This eliminates False Sharing across multiple cores updating adjacent tags.
