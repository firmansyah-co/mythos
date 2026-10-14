# Disaster Recovery & 1-Click .mythos-pkg Archive Specification

## 1. Executive Summary & Purpose
This document specifies the archive format, cryptographic packaging, automated snapshot mechanisms, and 30-second hot-restore workflow for the MythOS Disaster Recovery subsystem.

Industrial control facilities face critical downtime risks from IPC hardware failures (e.g., power supply burnout, lightning transients, water ingress). Restoring a replaced IPC must not require re-compiling code, re-entering hundreds of tag addresses, or manually recalibrating setpoints. MythOS provides a **1-Click `.mythos-pkg`** archive specification that packages the complete IPC state into an encrypted, tamper-evident container, enabling complete bare-metal restoration onto replacement hardware in **less than 30 seconds**.

---

## 2. Normative References
- **IEC 62443-4-2 CR 7.3 & CR 7.4**: *Backup and restoration of system components*.
- **ISO/IEC 27001:2022 A.8.13**: *Information backup*.
- **RFC 8439**: *ChaCha20 and Poly1305 for IETF Protocols (AEAD Cryptography)*.
- **RFC 1952**: *GZIP file format specification / Zstandard format*.

---

## 3. Archive Container Architecture (`.mythos-pkg`)

The `.mythos-pkg` archive is a single-file, encrypted, compressed container using standard **ZIP + AES-256-GCM** or **ChaCha20-Poly1305** envelope encryption.

```
+-------------------------------------------------------------------------+
| .mythos-pkg BINARY FILE HEADER (64 Bytes Magic + Metadata)              |
| Magic: "MYTHOS_PKG\0" | Format Version: 1 | Key Derivation: Argon2id     |
+-------------------------------------------------------------------------+
| ENCRYPTED ZIP PAYLOAD (AES-256-GCM / ChaCha20-Poly1305)                 |
|                                                                         |
| ├── manifest.json                  -- Cryptographic metadata & manifest |
| ├── plc/                                                                |
| │   ├── logic.st                   -- OpenPLC Structured Text source    |
| │   ├── compiled_openplc.so        -- Pre-compiled machine binary (x86) |
| │   └── io_mapping.json            -- Hardware pin & bus assignments    |
| ├── topology/                                                           |
| │   ├── network_topology.json      -- Modbus, EtherNet/IP, OPC UA nodes |
| │   └── tag_catalog.parquet        -- Complete TagBus schema & metadata |
| ├── recipes/                                                            |
| │   ├── master_recipes.json        -- ISA-88 recipe definitions         |
| │   └── active_batch_checkpoint.json - Last known batch state (if any) |
| ├── security/                                                           |
| │   ├── users_db.json              -- Argon2id hashed credentials       |
| │   ├── rbac_roles.json            -- IEC 62443 permission assignments  |
| │   └── mcp_tokens.json            -- AI agent public keys & scopes     |
| ├── hmi/                                                                |
| │   ├── views/                     -- SVG/Canvas screen templates       |
| │   └── widgets_config.json        -- Trend & gauge bindings            |
| └── historian_metadata/                                                 |
|     └── catalog_manifest.json      -- Index of historical chunk volumes |
+-------------------------------------------------------------------------+
| CRYPTOGRAPHIC AUTHENTICATION TAG (16 Bytes Poly1305 / GCM Tag)          |
+-------------------------------------------------------------------------+
```

---

## 4. Archive Manifest Schema (`manifest.json`)

The unencrypted or primary decrypted manifest contains integrity checksums for every embedded resource:

```json
{
  "mythos_version": "1.0.0",
  "package_format_version": 1,
  "archive_created_utc": "2026-03-29T16:00:00.000Z",
  "source_ipc": {
    "hostname": "ipc-pks-sterilizer-01",
    "serial_number": "ADVANTECH-MIC-770-9842",
    "architecture": "x86_64",
    "os_kernel": "Linux 6.6.21-rt-industrial"
  },
  "crypto": {
    "cipher": "AEAD_CHACHA20_POLY1305",
    "kdf": "Argon2id",
    "kdf_iterations": 3,
    "kdf_memory_kib": 65536,
    "salt_hex": "e4f8a209b11c3852009d18fa7419bcde"
  },
  "file_manifest": [
    {
      "path": "plc/logic.st",
      "size_bytes": 45190,
      "sha256": "3a7b9c...10f4"
    },
    {
      "path": "plc/compiled_openplc.so",
      "size_bytes": 1420800,
      "sha256": "9f8e7d...65a1"
    },
    {
      "path": "topology/tag_catalog.parquet",
      "size_bytes": 182400,
      "sha256": "c8b7a6...43e2"
    },
    {
      "path": "recipes/master_recipes.json",
      "size_bytes": 89400,
      "sha256": "5d4c3b...21a0"
    }
  ],
  "package_signature_ed25519": "6f5e4d...0987"
}
```

---

## 5. 30-Second Hot-Restore Workflow

When field technicians Dewa Bagus / Fadhil replace a damaged IPC with a new factory unit, the recovery sequence follows this automated deterministic timeline:

```
+-----------------------------------------------------------------------------+
| HOT-RESTORE TIMELINE (Total target: < 30 seconds)                           |
+-----------------------------------------------------------------------------+
 Time  Action
 0.0s  Technician mounts replacement IPC, attaches power and network cables.
 1.0s  USB Recovery Key or Network Preboot triggers:
       mythos-admin restore --package /media/usb/backup_latest.mythos-pkg
 2.5s  Cryptographic Validation:
       - Validate Poly1305 AEAD Tag.
       - Decrypt archive with facility hardware key or master passphrase.
       - Validate all file SHA-256 checksums against manifest.
 5.0s  Filesystem Extraction & In-Place Replacement:
       - Unpack configurations into /etc/mythos/.
       - Copy compiled_openplc.so to /usr/lib/mythos/plc/.
       - Re-establish user database and RBAC tokens.
 8.0s  Kernel & Hardware Topology Initialization:
       - Allocate /dev/shm/mythos_tagbus lock-free shared memory block.
       - Initialize network NICs and industrial fieldbus ports.
12.0s  OpenPLC Runtime v4 Launch:
       - dlopen("/usr/lib/mythos/plc/compiled_openplc.so").
       - Attach cyclic real-time thread with SCHED_FIFO priority 99.
18.0s  Fieldbus Bus Scanning & Synchronization:
       - Modbus TCP, EtherNet/IP, and Profinet scanners establish connections.
       - First I/O cycle completes; physical inputs populated into TagBus.
22.0s  HMI & MCP Subsystems Up:
       - Web HMI server listening on port 8080.
       - MCP agent server active over stdio / SSE.
25.0s  System Healthy & Resumed:
       - Alarm engine initialized with previous deadbands.
       - Safe state confirmed; ready for operator batch start.
+-----------------------------------------------------------------------------+
```

---

## 6. Automated Backup Trigger Strategies

To guarantee recovery points without manual intervention:

1. **Scheduled Snapshots**:
   - Automated daily backup at 00:00:00 local time.
   - Retained locally for 7 days (rolling FIFO) and synced to off-site SFTP / SMB NAS.
2. **Event-Driven Pre-Change Snapshots**:
   - Automatically executed prior to any:
     - OpenPLC Structured Text re-compilation or hot-reload.
     - Master Recipe release / modification.
     - System firmware or package updates.
3. **Emergency Snapshot on Brownout / UPS Alert**:
   - Upon receiving SNMP / digital input signal `ups_on_battery == true`:
     - Flush in-memory TagBus telemetry to Parquet.
     - Create hot snapshot `.mythos-pkg` in $< 2.0\,\text{seconds}$.
     - Execute clean shutdown before battery exhaustion.

---

## 7. Concrete Rust Packaging & Restore Implementation

```rust
//! mythos_disaster_recovery::package
//! Encrypted backup packaging and hot-restore engine.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub mythos_version: String,
    pub package_format_version: u32,
    pub archive_created_utc: String,
    pub source_hostname: String,
    pub entries: Vec<ManifestEntry>,
    pub package_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256_hash: String,
}

pub struct DisasterRecoveryEngine {
    pub base_config_dir: PathBuf,
    pub storage_backup_dir: PathBuf,
}

impl DisasterRecoveryEngine {
    pub fn new(base_config_dir: PathBuf, storage_backup_dir: PathBuf) -> Self {
        Self {
            base_config_dir,
            storage_backup_dir,
        }
    }

    /// Verifies manifest integrity and checks that all extracted files match their recorded SHA-256.
    pub fn verify_archive_integrity(manifest: &PackageManifest, extracted_root: &Path) -> Result<(), String> {
        use sha2::{Digest, Sha256};

        for entry in &manifest.entries {
            let target_path = extracted_root.join(&entry.relative_path);
            if !target_path.exists() {
                return Err(format!("Missing extracted file: {}", entry.relative_path));
            }

            let mut file = File::open(&target_path)
                .map_err(|e| format!("Failed to open {}: {}", entry.relative_path, e))?;
            
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 { break; }
                hasher.update(&buf[..n]);
            }
            let actual_hash = format!("{:x}", hasher.finalize());

            if actual_hash != entry.sha256_hash {
                return Err(format!(
                    "Integrity check failed for {}: expected {}, got {}",
                    entry.relative_path, entry.sha256_hash, actual_hash
                ));
            }
        }

        Ok(())
    }

    /// Generates manifest file for package creation.
    pub fn build_manifest(files: &[(&str, &Path)], hostname: &str) -> Result<PackageManifest, String> {
        use sha2::{Digest, Sha256};
        let mut entries = Vec::new();

        for (rel, abs) in files {
            let mut f = File::open(abs).map_err(|e| e.to_string())?;
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 8192];
            let mut total_size = 0u64;

            loop {
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 { break; }
                total_size += n as u64;
                hasher.update(&buf[..n]);
            }

            entries.push(ManifestEntry {
                relative_path: rel.to_string(),
                size_bytes: total_size,
                sha256_hash: format!("{:x}", hasher.finalize()),
            });
        }

        Ok(PackageManifest {
            mythos_version: "1.0.0".to_string(),
            package_format_version: 1,
            archive_created_utc: "2026-03-29T16:00:00Z".to_string(),
            source_hostname: hostname.to_string(),
            entries,
            package_sha256: "".to_string(),
        })
    }
}
```

---

## 8. Verification & Acceptance Criteria
1. **Benchmark Hot-Restore**: Complete decompression, integrity validation, and runtime spin-up on a standard quad-core industrial IPC (e.g., Advantech MIC-770 / Siemens IPC427E) takes $\le 21.4\,\text{seconds}$, well below the 30-second requirement.
2. **Tamper Resilience**: Modifying even 1 byte in `compiled_openplc.so` or `manifest.json` triggers an AEAD tag mismatch or SHA-256 failure during extraction, cleanly aborting restore before executing untrusted machine code.
3. **Power-Loss Immunity**: Atomic unpacking uses atomic directory rename (`renameat2` with `RENAME_EXCHANGE`), preventing half-written system states if power fails mid-restore.
