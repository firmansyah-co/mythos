# MythOS Data Specification: UDT Layout and Memory Retention

- **Document Identifier**: MYTHOS-DATA-002
- **Version**: 1.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification details the binary data layout for User-Defined Data Types (UDT / IEC 61131-3 STRUCT) and defines the persistence engine governing memory retention in MythOS.

Industrial processes rely on machine state variables, calibration parameters, totalizers, and batch recipes that must survive power cycles, unexpected process restarts, or full system reboots. MythOS categorizes all tags into three deterministic retention classes (`VOLATILE`, `RETAIN`, `PERSISTENT`) and implements a storage-backed Non-Volatile RAM (NVRAM) Write-Ahead Log (WAL) engine (`/var/lib/mythos/nvram.dat`).

---

## 2. User-Defined Type (UDT) Binary Layout

User-Defined Types represent compound industrial objects (e.g., motor drives, PID loops, valve actuators, analog transmitters). MythOS maps UDT instances into contiguous slices of Shared Memory Tag Slots while maintaining deterministic field alignments.

### 2.1 Standard Motor Drive UDT (`UDT_MotorDrive`)

```rust
use bytemuck::{Pod, Zeroable};

/// Binary memory layout of a standard industrial motor drive faceplate object
#[derive(Copy, Clone, Debug)]
#[repr(C, align(8))]
pub struct UdtMotorDrive {
    // Commands (Control Word)
    pub cmd_start: bool,        // Offset 0
    pub cmd_stop: bool,         // Offset 1
    pub cmd_reset_fault: bool,  // Offset 2
    pub cmd_jog: bool,          // Offset 3
    pub mode_auto: bool,        // Offset 4: 1=Auto (PLC), 0=Manual (HMI)
    pub _pad_bools: [u8; 3],    // Offset 5..7: 3-byte alignment padding

    // Setpoints
    pub speed_setpoint_rpm: f32,// Offset 8..11 (4 bytes)
    pub accel_time_sec: f32,    // Offset 12..15 (4 bytes)

    // Status (Feedback Word)
    pub status_running: bool,   // Offset 16
    pub status_tripped: bool,   // Offset 17
    pub status_ready: bool,     // Offset 18
    pub status_warning: bool,   // Offset 19
    pub fault_code: u16,        // Offset 20..21 (2 bytes)
    pub _pad_status: [u8; 2],   // Offset 22..23 (2 bytes)

    // Process Feedback
    pub speed_feedback_rpm: f32,// Offset 24..27 (4 bytes)
    pub motor_current_amps: f32,// Offset 28..31 (4 bytes)
    pub motor_temp_celsius: f32,// Offset 32..35 (4 bytes)
    pub operating_hours: f32,   // Offset 36..39 (4 bytes)
}

unsafe impl Zeroable for UdtMotorDrive {}
unsafe impl Pod for UdtMotorDrive {}

const _: () = assert!(std::mem::size_of::<UdtMotorDrive>() == 40);
```

### 2.2 UDT Tag Namespace Flattening
When registered in the MythOS system catalog, UDT instances expose child subfields mapped directly to individual `TagSlot` entries following the canonical hierarchy `external.<namespace>.<device>.<variable>[.<subfield>]`:
```
external.BoilerRoom.FeedPump01.cmd_start      -> TagSlot #412 (BOOL, WO)
external.BoilerRoom.FeedPump01.speed.sp       -> TagSlot #413 (REAL, RW)
external.BoilerRoom.FeedPump01.speed.pv       -> TagSlot #414 (REAL, RO)
external.BoilerRoom.FeedPump01.fault_code     -> TagSlot #415 (UINT, RO)
```
- `external`: Physical external field device origin.
- `BoilerRoom`: Plant process area / machine group installation namespace.
- `FeedPump01`: Physical equipment / instrument tag.
- `speed`, `cmd_start`, `fault_code`: Measured variable or command register.
- `.sp`, `.pv`: Subfield qualifiers for setpoint and process value.

---

## 3. Memory Retention Classifications

Every tag declares an explicit memory retention tier:

```
+─────────────────+─────────────────────────────────────────────────────────────+
| Retention Tier  | Behavioral Invariant                                        |
+─────────────────+─────────────────────────────────────────────────────────────+
| VOLATILE        | Initialized to factory default upon any restart.            |
|                 | Memory cleared during power loss or process restart.        |
|                 | Examples: Instantaneous sensor values, raw communication    |
|                 | telemetry, live cycle timers, dynamic animation flags.      |
+─────────────────+─────────────────────────────────────────────────────────────+
| RETAIN          | Value survives unexpected process crashes, warm reboots,    |
|                 | and controlled software restarts. Restored automatically    |
|                 | by the WAL recovery manager during bootstrap.               |
|                 | Examples: Operating hour meters, production piece counters, |
|                 | current batch step sequence, alarm trip history.            |
+─────────────────+─────────────────────────────────────────────────────────────+
| PERSISTENT      | Value survives cold power cycles, kernel updates, and       |
|                 | complete program re-flashes. Values are preserved unless an |
|                 | explicit factory wipe command is executed.                  |
|                 | Examples: PID tuning parameters (Kp, Ti, Td), calibration   |
|                 | zero/span offsets, recipe formulas, network IP settings.   |
+─────────────────+─────────────────────────────────────────────────────────────+
```

---

## 4. NVRAM WAL Engine Architecture (`/var/lib/mythos/nvram.dat`)

To eliminate reliance on expensive proprietary battery-backed SRAM hardware, MythOS implements an industrial-grade **Asynchronous Write-Ahead Log (WAL)** engine.

```
+───────────────────────────────────────────────────────────────────────────+
| Tag Update Event (Tag with RETAIN or PERSISTENT flag modified)            |
+───────────────────────────────────────────────────────────────────────────+
                                     │
                                     ▼
+───────────────────────────────────────────────────────────────────────────+
| Bounded In-Memory WAL Ring Buffer (Lock-Free MPSC)                        |
+───────────────────────────────────────────────────────────────────────────+
                                     │
                                     ▼ Cyclic Flush (Every 50 ms or 100 ops)
+───────────────────────────────────────────────────────────────────────────+
| Append-Only Storage WAL File (`/var/lib/mythos/nvram.dat`)                 |
|   [WAL Record Header | Tag ID | Value Union | Timestamp | CRC32]          |
+───────────────────────────────────────────────────────────────────────────+
                                     │
                                     ▼ Periodic Compaction (Hourly)
+───────────────────────────────────────────────────────────────────────────+
| Atomic Snapshot Compaction (`/var/lib/mythos/nvram.snapshot`)              |
|   Atomic rename to replace WAL file and reclaim disk sectors              |
+───────────────────────────────────────────────────────────────────────────+
```

### 4.1 WAL Record Binary Format

```rust
use bytemuck::{Pod, Zeroable};

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct WalRecordHeader {
    /// Magic byte signature: 0x5741 (ASCII "WA")
    pub magic: u16,

    /// Monotonic log sequence number (LSN)
    pub lsn: u64,

    /// Unique tag ID matching SHM catalog
    pub tag_id: u32,

    /// Timestamp of write in nanoseconds
    pub timestamp_ns: u64,

    /// 64-bit value payload
    pub value_raw: [u8; 8],

    /// CRC32 checksum over the preceding bytes of this record
    pub record_crc32: u32,
}

unsafe impl Zeroable for WalRecordHeader {}
unsafe impl Pod for WalRecordHeader {}

const _: () = assert!(std::mem::size_of::<WalRecordHeader>() == 34);
```

### 4.2 Crash Recovery Algorithm (Pure Rust)

```rust
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use tracing::{error, info, warn};
use crc32fast::Hasher;
use crate::error::NvramError;

pub struct NvramWalEngine {
    file_path: std::path::PathBuf,
}

impl NvramWalEngine {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self { file_path: path.as_ref().to_path_buf() }
    }

    /// Executed on MythOS startup before real-time control cycle commences
    pub fn recover_into_shm(
        &self,
        shm_writer: &mut mythos_shm::ShmWriterHandle,
    ) -> Result<usize, NvramError> {
        if !self.file_path.exists() {
            info!("No existing NVRAM WAL file found. Starting clean slate.");
            return Ok(0);
        }

        let file = File::open(&self.file_path)?;
        let mut reader = BufReader::new(file);
        let mut recovered_count = 0;
        let mut buffer = [0u8; std::mem::size_of::<WalRecordHeader>()];

        loop {
            match reader.read_exact(&mut buffer) {
                Ok(()) => {
                    let record: WalRecordHeader = bytemuck::cast(buffer);

                    // 1. Verify Magic
                    if record.magic != 0x5741 {
                        warn!("Invalid WAL record magic encountered. Terminating replay at clean boundary.");
                        break;
                    }

                    // 2. Verify CRC32
                    let mut hasher = Hasher::new();
                    hasher.update(&buffer[..30]); // Preceding bytes excluding CRC
                    let calculated_crc = hasher.finalize();

                    if calculated_crc != record.record_crc32 {
                        error!("Corrupt WAL record detected! Checksum failure at LSN {}", record.lsn);
                        return Err(NvramError::ChecksumMismatch {
                            epoch: 0,
                            offset: record.lsn,
                        });
                    }

                    // 3. Restore into Shared Memory
                    let val_raw = mythos_shm::TagValueRaw { raw_bytes: record.value_raw };
                    shm_writer.restore_retained_tag(record.tag_id, val_raw);
                    recovered_count += 1;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Reached end of WAL log cleanly
                    break;
                }
                Err(e) => return Err(NvramError::FileError(e)),
            }
        }

        info!("NVRAM recovery successful: restored {} retained tags into SHM.", recovered_count);
        Ok(recovered_count)
    }
}
```

---

## 5. Compaction and Flash Wear Leveling

To avoid flash storage exhaustion and unbounded WAL file growth on industrial embedded edge controllers (Raspberry Pi CM4, Advantech UNO, Siemens IOT2050):
1. **Periodic Snapshot**: Every hour or upon accumulating 50,000 log entries, the engine writes an atomic snapshot file (`nvram.snapshot`) containing the latest unique value for each retained tag.
2. **Atomic Swap**: The engine uses POSIX `rename()` to atomically replace `nvram.dat` with `nvram.snapshot`.
3. **Hardware Storage Wear Minimization**: Flushes to disk utilize buffered I/O with explicit `fdatasync()` calls restricted to a 50 ms coalescing window, reducing flash write amplification.
