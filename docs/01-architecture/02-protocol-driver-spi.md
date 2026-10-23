# MythOS Architecture Specification: Protocol Driver SPI

- **Document Identifier**: MYTHOS-ARCH-002
- **Version**: 1.2.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION — PARTIAL REWRITE PENDING
- **Language**: ASD-STE100 Technical English

> **Drift warning (2026-10-23)**: Section 3 is corrected against the current SDK (`crates/mythos-plugin-sdk/src/lib.rs`) after ADR-008. Sections 4 through 7 still describe the retired `mythos-driver-api` contract: `DriverError`, `DriverContext`, `driver_id`, and `DiscoveryCapableDriver` do not exist in code. Verify each contract against the SDK before you use it. A full rewrite of Sections 4 to 7 is pending.

---

## 1. Scope and Objective

This specification defines the Service Provider Interface (SPI) for communication protocol drivers in MythOS. 

Industrial environments contain diverse field communication protocols, including Modbus TCP/RTU, PROFINET, MQTT, Ethernet/IP, CANopen, BACnet, EtherCAT, and OPC UA. In legacy monolithic architectures, a malformed packet or slow serial transaction in one driver blocks the entire runtime. 

MythOS enforces an **Isolated Protocol Driver SPI**. Every protocol driver executes as an isolated actor. Drivers do not share memory space or execution threads with other drivers. Drivers interface with the Microkernel strictly through strongly typed asynchronous traits, opaque configuration schemas, and isolated shared memory write tokens.

Furthermore, this specification defines the **Universal Protocol Auto-Discovery SPI** (`DiscoveryCapableDriver`). Protocol drivers implement standardized, non-blocking heuristic scanning routines with cooperative cancellation and bounded progress reporting. This capability allows autonomous AI agents and commissioning technicians to discover field parameters (baud rate, parity, slave IDs, IP subnets, device profiles) with zero manual parameter guessing.

---

## 2. Driver Lifecycle State Machine

```
              ┌────────────────────────────────────────────────┐
              │                                                │
              ▼                                                │
      +---------------+   load_config() passes validation      │
      │ Uninitialized │ ─────────────────────────────────┐     │
      +---------------+                                  │     │
              ▲                                          ▼     │
              │ stop()                           +---------------+
              │                                  │  Configured   │
              │                                  +---------------+
              │                                          │
              │                                          │ start()
              │                                          ▼
      +---------------+   health_check() fails   +---------------+
      │   Degraded    │ <─────────────────────── │    Running    │
      +---------------+                          +---------------+
              │                                          │
              │ Unrecoverable error                      │ Panic / Fatal
              ▼                                          ▼
      +---------------+                          +---------------+
      │    Faulted    │ <─────────────────────── │   Restarting  │
      +---------------+                          +---------------+
```

### 2.1 State Definitions
1. **Uninitialized**: Driver struct created; no network sockets or serial ports opened.
2. **Configured**: Driver validated JSON configuration; internal buffers allocated; ready to connect.
3. **Running**: Field bus connected; cyclic polling active; tags updating into SHM.
4. **Degraded**: Frame dropouts or timeout rate exceeds threshold; driver actively retrying connection.
5. **Faulted**: Irrecoverable communication failure or configuration mismatch; driver stopped; diagnostic alert emitted.
6. **Restarting**: Supervisor executing exponential backoff reset.

---

## 3. Rust Trait Contracts (`mythos-plugin-sdk`)

The driver SPI is defined once, in `crates/mythos-plugin-sdk/src/lib.rs`. A driver implements the universal plugin lifecycle plus the driver-specific methods. The SDK exports no separate configuration-validator trait; validation is a method on `ProtocolDriver`.

### 3.1 Universal Plugin Lifecycle (`MythosPlugin`)

```rust
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use mythos_plugin_sdk::{
    PluginContext, PluginError, PluginHealthReport, PluginMetadata,
};

#[async_trait]
pub trait MythosPlugin: Send + Sync {
    /// Return the plugin manifest: id, name, version, author, type, and capabilities.
    fn metadata(&self) -> PluginMetadata;

    /// Initialize plugin state and validate dependencies before starting.
    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Apply runtime configuration and commence background tasks.
    async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError>;

    /// Optional cyclic tick hook invoked by the microkernel host scheduler.
    async fn tick(&mut self, ctx: &PluginContext, delta_ms: u32) -> Result<(), PluginError> {
        Ok(())
    }

    /// Terminate execution tasks and release resources gracefully.
    async fn stop(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Check operational health and report diagnostic status.
    async fn health_check(&self) -> Result<PluginHealthReport, PluginError>;
}
```

### 3.2 Protocol Driver SPI (`ProtocolDriver`)

```rust
#[async_trait]
pub trait ProtocolDriver: MythosPlugin {
    /// Parse and validate the driver-specific JSON configuration schema.
    fn validate_config(&self, config: &JsonValue) -> Result<(), PluginError>;

    /// Execute one bounded I/O polling cycle against configured target devices.
    /// Returns the number of field tags successfully ingested or updated.
    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<u32, PluginError>;

    /// Write an output value to a field device target tag.
    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &JsonValue,
    ) -> Result<(), PluginError>;
}
```

### 3.3 Driver Health and Diagnostics

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginHealthReport {
    pub status: HealthStatus,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Stopped,
}
```

### 3.4 Plugin Context (`PluginContext`)

`PluginContext` is the only host handle a driver receives. It carries no logger, SHM writer, or event channel. The host grants capability strings, and the driver checks them before using a privileged resource:

```rust
#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_id: String,
    pub allowed_capabilities: Arc<HashSet<String>>,
    pub working_directory: String,
}

impl PluginContext {
    /// Return true when the capability scope is permitted for this plugin.
    pub fn has_capability(&self, capability: &str) -> bool;

    /// Return PluginError::CapabilityViolation when the scope is missing.
    pub fn check_capability(&self, capability: &str) -> Result<(), PluginError>;
}
```

A driver that publishes field values declares the `tagbus:write` capability in its manifest and calls `ctx.check_capability("tagbus:write")` during `init`, `start`, and each poll cycle. The host performs the shared-memory write. Drivers do not link `mythos-shm` directly.

---

## 4. Opaque JSON Configuration Validation Pattern

To guarantee zero cross-pollution between driver configurations, each driver declares its strongly typed configuration struct and exposes an automated JSON Schema validator.

```
       Microkernel Config Loader
                   │
                   │ Raw JSON payload
                   ▼
       +───────────────────────+
       │ config_schema() Check │ (schemars validation)
       +───────────────────────+
                   │
         Pass      │      Fail
       ┌───────────┴───────────┐
       ▼                       ▼
+─────────────────+   +─────────────────────────────────────+
| from_value(raw) |   | Return ConfigValidationFailed error |
+─────────────────+   +─────────────────────────────────────+
       │
       ▼
Driver receives typed struct:
ModbusConfig { ip, port, unit_id, registers }
```

### 4.1 Concrete Example: Modbus Driver Configuration Validator

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use serde_json::Value as JsonValue;
use crate::error::DriverError;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct ModbusDriverConfig {
    /// Remote PLC IPv4 address
    pub host: String,

    /// Modbus TCP port (default: 502)
    #[serde(default = "default_port")]
    pub port: u16,

    /// Modbus Slave/Unit ID (1 - 247)
    pub unit_id: u8,

    /// Polling cycle interval in milliseconds
    #[serde(default = "default_poll_rate")]
    pub poll_rate_ms: u64,

    /// List of register scan blocks
    pub scan_blocks: Vec<ModbusScanBlock>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct ModbusScanBlock {
    pub name: String,
    pub function_code: u8, // 1=Coil, 2=Discrete Input, 3=Holding, 4=Input
    pub start_address: u16,
    pub count: u16,
    pub target_tag_base_id: u32,
}

fn default_port() -> u16 { 502 }
fn default_poll_rate() -> u64 { 50 }

pub fn validate_and_parse_config(raw: JsonValue) -> Result<ModbusDriverConfig, DriverError> {
    // 1. Structural validation via serde
    let config: ModbusDriverConfig = serde_json::from_value(raw).map_err(|e| {
        DriverError::ConfigValidationFailed(format!("JSON Schema mismatch: {e}"))
    })?;

    // 2. Semantic validation
    if config.unit_id == 0 || config.unit_id > 247 {
        return Err(DriverError::ConfigValidationFailed(format!(
            "Invalid Modbus unit_id {}: must be between 1 and 247",
            config.unit_id
        )));
    }

    if config.poll_rate_ms < 5 {
        return Err(DriverError::ConfigValidationFailed(
            "Poll rate below 5ms minimum threshold is prohibited".into()
        ));
    }

    for block in &config.scan_blocks {
        if block.count == 0 || block.count > 125 {
            return Err(DriverError::ConfigValidationFailed(format!(
                "Scan block '{}' count {} exceeds Modbus protocol limit of 125",
                block.name, block.count
            )));
        }
    }

    Ok(config)
}
```

---

## 5. Fault Isolation and Panic Containment

The Microkernel Supervisor isolates each driver inside a dedicated asynchronous tokio task pinned to a specific worker thread.

### 5.1 Panic Guard Implementation

```rust
use std::panic::AssertUnwindSafe;
use futures::FutureExt;
use tracing::{error, warn};

pub async fn run_supervised_driver<D: ProtocolDriver>(
    mut driver: D,
    mut ctx: DriverContext,
) {
    let driver_id = driver.driver_id();
    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 5;

    while retry_count < MAX_RETRIES {
        // AssertUnwindSafe wraps driver task to catch unwinding panics
        let result = AssertUnwindSafe(driver.start(&mut ctx)).catch_unwind().await;

        match result {
            Ok(Ok(())) => {
                // Driver stopped cleanly
                break;
            }
            Ok(Err(driver_err)) => {
                error!(
                    driver = driver_id,
                    error = %driver_err,
                    "Driver encountered runtime error. Entering degraded recovery."
                );
            }
            Err(panic_payload) => {
                error!(
                    driver = driver_id,
                    panic = ?panic_payload,
                    "CRITICAL: Driver crashed with an unhandled panic. Microkernel isolated failure."
                );
            }
        }

        retry_count += 1;
        let backoff = Duration::from_millis(100 * (2u64.pow(retry_count)));
        warn!(driver = driver_id, retry = retry_count, backoff = ?backoff, "Restarting driver...");
        tokio::time::sleep(backoff).await;
    }

    if retry_count >= MAX_RETRIES {
        error!(driver = driver_id, "Driver exceeded max restart attempts. Marked as FAULTED.");
        let _ = driver.stop(&mut ctx).await;
    }
}
```

### 5.2 Key Isolation Rules
1. **No Shared Locks**: A driver must never acquire a mutex or lock held by another driver or by the Native STruC++ real-time loop.
2. **Dedicated Channel**: Driver communication with the kernel uses bounded MPSC channels (`tokio::sync::mpsc`). When a channel is full, excess telemetry events drop cleanly without blocking the network loop.
3. **Dedicated Shm Slice**: Drivers receive handles to only the tag slot IDs defined in their validated configuration. A driver cannot overwrite memory owned by another driver.
---

## 6. Universal Protocol Auto-Discovery SPI (`DiscoveryCapableDriver`)

### 6.1 Architectural Rationale and Industrial Context

Field commissioning in operational brownfield environments (such as water treatment plants, palm oil mills, and multi-tenant smart facilities) frequently presents undocumented fieldbus configurations. Field instrumentation arrives from various suppliers with mismatched serial communication parameters (baud rate, parity, stop bits), unknown slave node addresses, or unassigned IP subnets. 

Traditional commissioning workflows force automation technicians to perform trial-and-error configuration with proprietary desktop tools or physical DIP switch inspection. 

To eliminate manual configuration errors and accelerate plant commissioning, MythOS provides a **Universal Protocol Auto-Discovery Engine**. Protocol drivers capable of field inspection implement the `DiscoveryCapableDriver` extension trait. This architecture standardizes:
1. **Multi-Protocol Discovery Abstraction**: A single, uniform interface across serial RS-485 (Modbus RTU), industrial Ethernet (Modbus TCP, PROFINET DCP, EtherNet/IP), automotive/fieldbus networks (CANopen), building management protocols (BACnet IP/MSTP), and motion networks (EtherCAT).
2. **Deterministic Cancellation**: Non-blocking scanning tasks bounded by `tokio_util::sync::CancellationToken`, ensuring immediate task teardown ($< 50\,\text{ms}$) upon operator cancellation or timeout without resource leaks or orphaned serial port locks.
3. **Bounded Telemetry and Backpressure**: Live progress reporting over bounded MPSC channels (`tokio::sync::mpsc`) preventing memory bloat during wide-subnet sweeps.
4. **Safety Probing Interlocks**: Strict read-only heuristic probes preventing accidental process actuation or PLC state perturbation during auto-discovery sweeps.

---

### 6.2 Universal Discovery Trait Contract (`DiscoveryCapableDriver`)

```rust
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use crate::error::DriverError;
use crate::ProtocolDriver;

/// Extension trait for drivers supporting active or passive auto-discovery
#[async_trait]
pub trait DiscoveryCapableDriver: ProtocolDriver {
    /// Return the JSON Schema describing driver-specific discovery hints.
    /// Used by frontend inspection panels and AI agents to validate scan parameters.
    fn discovery_schema(&self) -> &'static str;

    /// Execute an asynchronous non-blocking heuristic discovery sweep.
    ///
    /// # Arguments
    /// * `hints` - Scoping hints (interfaces, baud rates, address ranges, timeouts).
    /// * `progress_tx` - Bounded MPSC channel transmitting incremental progress updates.
    /// * `cancel_token` - Cooperative cancellation token allowing the kernel or user to abort the scan.
    ///
    /// # Safety and Isolation
    /// * Implementations MUST NOT execute mutating writes to target devices.
    /// * Implementations MUST yield periodically via tokio async points to prevent starvation.
    /// * Implementations MUST respect `cancel_token.is_cancelled()` across loop boundaries.
    async fn discover(
        &self,
        hints: DiscoveryHint,
        progress_tx: mpsc::Sender<ScanProgress>,
        cancel_token: CancellationToken,
    ) -> Result<ProtocolScanResult, DriverError>;
}
```

---

### 6.3 Discovery Core Data Structures

All discovery types derive `Serialize`, `Deserialize`, and `schemars::JsonSchema` to ensure direct TypeScript contract generation for the SCADA Workbench UI and native MCP tool exposure.

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use uuid::Uuid;

/// Input parameters guiding the heuristic discovery engine
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct DiscoveryHint {
    /// Unique scan session identifier
    pub scan_id: Uuid,

    /// Target physical or logical interfaces (e.g., ["/dev/ttyUSB0", "/dev/ttyS1"] or ["eth0", "192.168.1.0/24"])
    pub target_interfaces: Vec<String>,

    /// Serial-specific baud rate candidates to sweep (e.g., [9600, 19200, 38400, 57600, 115200])
    #[serde(default)]
    pub baud_rates: Option<Vec<u32>>,

    /// Serial-specific parity candidates to sweep (e.g., ["None", "Even", "Odd"])
    #[serde(default)]
    pub parities: Option<Vec<SerialParity>>,

    /// Address range or node ID range to probe (e.g., Modbus Slave ID 1..247 or CANopen Node ID 1..127)
    #[serde(default)]
    pub address_range: Option<AddressRange>,

    /// Per-probe individual timeout in milliseconds (default: 50 ms for serial, 20 ms for local LAN)
    #[serde(default = "default_probe_timeout_ms")]
    pub probe_timeout_ms: u32,

    /// Maximum concurrent probe tasks (for IP subnet sweeps, default: 64)
    #[serde(default = "default_concurrency_limit")]
    pub max_concurrency: usize,

    /// Protocol-specific arbitrary parameters (e.g., specific register probes, BACnet device instance ranges)
    #[serde(default)]
    pub protocol_options: serde_json::Value,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialParity {
    None,
    Even,
    Odd,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct AddressRange {
    pub start: u32,
    pub end: u32,
}

fn default_probe_timeout_ms() -> u32 { 50 }
fn default_concurrency_limit() -> usize { 64 }

/// Real-time progress update emitted during discovery execution
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct ScanProgress {
    /// Scan session identifier
    pub scan_id: Uuid,

    /// Target currently being probed (e.g., "/dev/ttyUSB0 @ 9600 8-N-1 (Slave 12)" or "192.168.1.45:502")
    pub current_target: String,

    /// Overall progress percentage (0.0 to 100.0)
    pub progress_percent: f32,

    /// Number of devices successfully discovered so far
    pub devices_found: usize,

    /// Current active probe phase or description
    pub current_phase: String,

    /// Elapsed time since scan start in milliseconds
    pub elapsed_ms: u64,

    /// True if scan has reached terminal state (Completed, Aborted, or Failed)
    pub is_complete: bool,
}

/// Fully qualified descriptor of a discovered field device
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct DiscoveredDevice {
    /// Unique temporary identifier for this scan result
    pub device_id: String,

    /// Driver ID that discovered this device (e.g., "driver-modbus")
    pub driver_id: String,

    /// Physical connection addressing scheme
    pub address_info: DeviceAddressInfo,

    /// Manufacturer / Vendor Name (if identified via protocol report or catalog match)
    pub vendor_name: Option<String>,

    /// Model or Product Description (e.g., "Supmea SUP-P300 Pressure Transmitter")
    pub model_name: Option<String>,

    /// Firmware or hardware revision string
    pub firmware_version: Option<String>,

    /// Serial number or unique hardware MAC / EUI identifier
    pub serial_number: Option<String>,

    /// Heuristic confidence score (0.00 to 1.00) based on response integrity and protocol conformity
    pub confidence_score: f32,

    /// Ready-to-apply configuration payload compatible with `ProtocolDriver::configure()`
    pub suggested_config: serde_json::Value,

    /// Raw protocol diagnostic attributes returned during identification probes
    pub raw_attributes: HashMap<String, String>,

    /// Matched Device Package Identifier from Open Hardware Catalog (`catalog/<vendor>/<model>.yaml`)
    pub matched_catalog_package: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(tag = "type")]
pub enum DeviceAddressInfo {
    #[serde(rename = "serial")]
    Serial {
        port: String,
        baud_rate: u32,
        data_bits: u8,
        parity: SerialParity,
        stop_bits: u8,
        slave_id: u8,
    },
    #[serde(rename = "tcp_ip")]
    TcpIp {
        ip_address: String,
        port: u16,
        unit_id: Option<u8>,
        mac_address: Option<String>,
    },
    #[serde(rename = "can")]
    Can {
        interface: String,
        node_id: u8,
        bitrate: u32,
    },
    #[serde(rename = "ethercat")]
    EtherCat {
        ring_position: u16,
        station_alias: u16,
    },
}

/// Final summary report returned upon scan completion
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct ProtocolScanResult {
    /// Scan session identifier
    pub scan_id: Uuid,

    /// Driver identifier executing the scan
    pub driver_id: String,

    /// UNIX timestamp (ms) when scan started
    pub started_at_ms: u64,

    /// UNIX timestamp (ms) when scan completed
    pub completed_at_ms: u64,

    /// Total number of discrete targets/addresses probed
    pub total_targets_probed: u32,

    /// List of successfully identified devices
    pub devices: Vec<DiscoveredDevice>,

    /// Errors encountered during the scan (non-fatal target errors)
    pub scan_errors: Vec<ScanErrorRecord>,

    /// Terminal status of the scan
    pub termination_status: ScanTerminationStatus,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct ScanErrorRecord {
    pub target: String,
    pub error_message: String,
    pub timestamp_ms: u64,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanTerminationStatus {
    Completed,
    CancelledByOperator,
    TimedOut,
    Faulted,
}
```

---

### 6.4 Asynchronous Lifecycle, Cancellation & Bounded Progress

The execution architecture separates the high-level scan orchestration from physical I/O sweeps.

```
       Microkernel Discovery Coordinator (Axum / MCP)
                              │
              spawn task with CancellationToken
                              │
                              ▼
               [DiscoveryCapableDriver::discover]
                              │
          ┌───────────────────┴───────────────────┐
          │ Probe Execution Loop                  │
          │                                       │
          │  Check cancel_token.is_cancelled()    │
          │  ├── True  ──> Abort & Clean Port     │
          │  └── False ──> Send Heuristic Probe   │
          │                                       │
          │  Yield to Tokio executor (tokio::task)│
          │  Send ScanProgress to progress_tx     │
          │  (try_send: drops if channel full)    │
          └───────────────────┬───────────────────┘
                              │
                              ▼
                    ProtocolScanResult
```

#### 6.4.1 Cooperative Cancellation Contract
- The `cancel_token` parameter is an instance of `tokio_util::sync::CancellationToken`.
- If an operator clicks "Cancel Scan" in the Workbench UI, or if an MCP client sends an abort command, the coordinator calls `cancel_token.cancel()`.
- The driver loop checks `cancel_token.is_cancelled()` before each target probe and between framing delays.
- Upon cancellation, the driver MUST immediately drop open serial port descriptors, close pending TCP connections, and construct a `ProtocolScanResult` with `ScanTerminationStatus::CancelledByOperator`.
- Teardown latency MUST NOT exceed $50\,\text{ms}$.

#### 6.4.2 Non-Blocking Progress Reporting and Backpressure
- Progress reporting uses a bounded Tokio channel (`mpsc::channel(100)`).
- Drivers MUST prioritize physical I/O over progress telemetry. If the consumer (e.g. SSE broadcaster) is slow, the driver uses `progress_tx.try_send(progress)`:
  * If the channel buffer is full, intermediate progress frames are discarded cleanly.
  * The final completion progress frame (`is_complete: true`) and device discovery frames MUST use `progress_tx.send(progress).await` to guarantee delivery.
- This design completely prevents slow UI rendering or network lag from degrading driver probe timing.

---

### 6.5 Protocol-Specific Heuristic Discovery Strategies

The `DiscoveryCapableDriver` abstraction supports dedicated heuristic search engines across industrial protocols:

```
+--------------------------------------------------------------------------------------------------+
|                            HEURISTIC DISCOVERY PROTOCOL ADAPTERS                                 |
+--------------------------------------------------------------------------------------------------+
|                                                                                                  |
|  [ Modbus RTU RS-485 ]           [ Modbus TCP / IP ]             [ CANopen Fieldbus ]            |
|  - Matrix: Baud x Parity x ID    - Subnet CIDR ping (port 502)   - LSS Fastscan (Inquire)        |
|  - Probe: FC 0x03 @ Reg 0x0000   - Concurrent TCP pool (N=64)    - Listen Heartbeat / NMT        |
|  - Fallback: FC 0x11 Report ID   - Probe: Unit ID 1..247         - Read Object 0x1008 (Name)     |
|  - Read-Only Non-Mutating        - Fast 20ms Connect Timeout     - Passive Sniffing Supported    |
|                                                                                                  |
|  [ BACnet / IP & MSTP ]          [ EtherCAT Automation ]         [ PROFINET Layer 2 ]            |
|  - Who-Is Broadcast (Port 47808) - Physical Ring Position Scan   - DCP-Identify-Req (Eth 0x8892) |
|  - I-Am Response Parsing         - SII EEPROM Vendor/Product ID  - All-Station Multicast Frame   |
|  - Object 8 (Device Identifier)  - Direct ESC State Read         - Station Name / MAC / IP Match |
+--------------------------------------------------------------------------------------------------+
```

#### 6.5.1 Modbus RTU Heuristic Sweep (Serial RS-485)
1. **Search Space Definition**:
   * Baud Rates: `[9600, 19200, 38400, 57600, 115200]` (sorted by deployment probability).
   * Parities: `[None (8-N-1), Even (8-E-1), Odd (8-O-1)]`.
   * Slave IDs: `1..247` (configurable range; defaults to `1..32` for fast commissioning).
2. **Adaptive Heuristic Probing**:
   * For each serial configuration candidate, open the serial port with exclusive OS lock (`TIOCEXCL` on Linux).
   * Emit **Read-Only Non-Mutating Probes**:
     * Primary Probe: Modbus Function Code `0x03` (Read Holding Registers), Start Address `0x0000`, Quantity `1`.
     * Secondary Probe: Modbus Function Code `0x04` (Read Input Registers), Start Address `0x0000`, Quantity `1`.
     * Identification Probe: Modbus Function Code `0x11` (Report Slave ID).
   * **Strict Non-Mutation Rule**: Under NO circumstances shall discovery emit Function Codes `0x05`, `0x06`, `0x0F`, or `0x10`. Field coils and actuator registers must never be toggled during discovery sweeps.
3. **Response Validation**:
   * Verify CRC-16 integrity.
   * If an affirmative response or a valid Modbus Exception response (e.g., `0x83 0x02` Illegal Data Address) is received, the baud rate, parity, and slave ID are confirmed.
   * Modbus Exception responses confirm that a device exists at the probed address, even if register `0x0000` is invalid.

#### 6.5.2 Modbus TCP Subnet Ping
1. Expand target CIDR (e.g., `192.168.1.0/24` -> 254 host IP addresses).
2. Launch bounded concurrent workers using `tokio::sync::Semaphore` (default concurrency: 64).
3. Attempt non-blocking TCP handshake to port 502 with a strict $20\,\text{ms}$ timeout.
4. For responsive sockets, transmit Modbus TCP MBAP frame with Unit ID sweep `[1, 255]`.
5. Capture transaction response, measuring round-trip latency ($\mu\text{s}$) and recording MAC address via local ARP table lookup.

#### 6.5.3 CANopen Fieldbus Discovery
1. Layer Setting Services (LSS - CiA 305): Execute `LSS Fastscan` algorithm to identify unconfigured nodes on the CAN bus.
2. Passive Sniffing: Monitor CAN frame IDs for cyclic NMT Node Guarding and Heartbeat messages (`0x700 + Node_ID`).
3. SDO Inquiries: Query CANopen Object Dictionary index `0x1008` (Device Name), `0x1009` (Hardware Version), and `0x1018` (Identity Object: Vendor ID, Product Code).

#### 6.5.4 BACnet / IP and MSTP Discovery
1. Transmit BACnet unconfirmed service request `Who-Is` broadcast over UDP port 47808 (BAC0).
2. Collect incoming `I-Am` responses containing:
   * Device Object Identifier (Instance Number).
   * Maximum APDU length accepted.
   * Segmentation support.
   * Vendor Identifier.
3. Query `Object_Name`, `Model_Name`, and `Firmware_Revision` via `ReadProperty` requests.

#### 6.5.5 EtherCAT Ring Topology Discovery
1. Scan the EtherCAT physical loop through the Master ESC (EtherCAT Slave Controller).
2. Enumerate slaves by physical ring position (`Auto-Increment Addressing`).
3. Read EtherCAT Slave Information (SII) EEPROM word offsets:
   * Vendor ID (`0x0008`).
   * Product Code (`0x000A`).
   * Revision Number (`0x000C`).
   * Serial Number (`0x000E`).

---

### 6.6 Hardware Catalog Integration and Auto-Provisioning

When `DiscoveryCapableDriver::discover()` returns discovered devices, the Microkernel Discovery Coordinator automatically matches them against the **MythOS Open Hardware Catalog** (`MYTHOS-CAT-001`):

```
       DiscoveredDevice
       { vendor_id, product_code, raw_attributes }
                            │
                            ▼
     +─────────────────────────────────────────────+
     | Catalog Manifest Ingestion Engine           |
     | Scans: catalog/<vendor>/<model>.yaml        |
     +─────────────────────────────────────────────+
                            │
              Match Found?  │
              ┌─────────────┴─────────────┐
        Yes   │                     No    │
              ▼                           ▼
+───────────────────────────+   +───────────────────────────+
| Auto-bind Device Package: |   | Generic Fallback:         |
| - Pre-mapped Tag Schema   |   | - Expose discovered port/ |
| - Engineering Units       |   |   baud/slave config       |
| - SVG Isometric Icon      |   | - Manual register layout  |
| - Inspector Drawer Tabs   |   |   in SCADA Workbench      |
+───────────────────────────+   +───────────────────────────+
```

1. If matching manifest signatures exist in `catalog/`, `DiscoveredDevice.matched_catalog_package` is populated.
2. The operator can commission the hardware with a single click, instantly establishing tag namespaces (`external.<namespace>.<device>.*`) and pre-allocating lock-free SHM slots without manual register configuration.


---

## 7. Per-Device Simulation Mode & Zero-Silent-Fallback Production Safety Invariant

### 7.1 Production Safety Invariant & Adrian Rule
In industrial automation, silent software fallbacks are catastrophic. When an automated control system is running a physical facility—such as a boiler, steam turbine, or chemical reactor—a severed cable or missing serial port MUST NOT cause the driver to silently switch to generating simulated values. In production, a severed communication cable must sound an immediate high-priority alarm, never pretend to be operational by injecting synthetic numbers.

**The Zero-Silent-Fallback Mandate**:
1. **Mandatory Production Mode**: All protocol drivers default strictly to `DeviceExecutionMode::PhysicalHardware`.
2. **Immediate Failure Assertion**: If an underlying communication medium fails (e.g., `/dev/ttyUSB0` missing, socket connection refused, RS-485 severed, or poll frame timeout), the driver MUST IMMEDIATELY mark the associated tags with `QualityCode::BAD_COMM_FAILURE` (`0x0008` / `0x0018`).
3. **Strict Ban on Auto-Fallback**: Under ZERO circumstances shall any driver actor, supervisor, or kernel component automatically switch a device from `PhysicalHardware` to simulation mode.
4. **Manual Mode Transition Only**: A device enters simulation mode ONLY when an authorized engineer explicitly changes the device configuration via the UI inspector or authenticated API.

---

### 7.2 The `DeviceExecutionMode` Rust Contract

```rust
use serde::{Deserialize, Serialize};

/// Operational mode governing external device communication and safety behaviors
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum DeviceExecutionMode {
    /// Strict Physical Hardware (Default Production Mode).
    /// - Physical interface (serial port, TCP socket, CAN interface) is mandatory.
    /// - Interface missing or comms timeout immediately asserts BAD_COMM_FAILURE (0x0008/0x0018).
    /// - Simulation injection endpoints are strictly locked and reject mutations.
    #[default]
    PhysicalHardware = 0,

    /// Manual Simulation Mode (Commissioning / Staging / Testbench).
    /// - Physical hardware allocation and port opening errors are safely suppressed.
    /// - Unlocks manual TagBus injection, Quick Slider setpoints, and Direct Variable Forcing.
    /// - Process values receive QualityCode::UNCERTAIN_SUBSTITUTED (0x0044).
    /// - Device and all associated tags are tagged with high-visibility amber badge [🧪 SIMULATION ACTIVE].
    ManualSimulation = 1,
}
```

---

### 7.3 Driver Execution Behavior by Mode

```
                   COMMUNICATION INITIALIZATION & CYCLIC POLL
                                       │
                         Check DeviceExecutionMode
                                       │
            ┌──────────────────────────┴──────────────────────────┐
            ▼                                                     ▼
 [ PhysicalHardware (Default) ]                          [ ManualSimulation ]
            │                                                     │
 Physical Port / Socket Available?                      Suppress Physical Port Open
       ┌────┴────┐                                                │
      Yes        No                                      Initialize Virtual Mock State
       │          │                                               │
   Poll Device  Assert BAD_COMM_FAILURE (0x0018)         Accept TagBus Manual Injection
       │        Emit High-Priority Alarm                 Apply Quality UNCERTAIN_SUBSTITUTED (0x0044)
       ▼                                                          ▼
  Publish Real Telemetry                                 Publish Injected Telemetry
  (Quality: GOOD 0x00C0)                                 (Amber [🧪 SIMULATION ACTIVE])
```

#### Detailed Driver Behavior Matrix

| Driver Phase / Condition | `PhysicalHardware` Mode | `ManualSimulation` Mode |
| :--- | :--- | :--- |
| **Port / Socket Initialization** | Fails immediately if interface unavailable. Emits `DriverError::HardwareInterfaceMissing`. | Interface acquisition bypassed. Driver transitions directly to `Running` virtual state. |
| **Physical Cable Severed / Timeout** | Frame timeout counter trips. Immediately publishes `BAD_COMM_FAILURE` (`0x0018`) to all device tag slots in SHM. | N/A (no physical line). Maintains last injected value or generator trajectory. |
| **Quality Code in SHM** | `QualityCode::GOOD` (`0x00C0`) on valid CRC; `QualityCode::BAD_COMM_FAILURE` (`0x0018`) on failure. | `QualityCode::UNCERTAIN_SUBSTITUTED` (`0x0044`) on all simulated tags. |
| **TagBus Slider Injection** | **REJECTED** (HTTP `403 Forbidden`). DriverContext returns `ErrSimulationModeDisabled`. | **ACCEPTED**. Mutates SHM tag slot and notifies subscribers over SSE/WebSocket. |
| **Visual Badge in Workbench** | Standard operational badges (Green Running / Red Fault). | Amber Badge `[🧪 SIMULATION ACTIVE]` on Network View, Tags, and HMI Faceplates. |
| **Audit Log Trail** | Standard connection logs. | Append-only audit record stamped with Operator ID, timestamp, and target URI. |

---

### 7.4 DriverContext Simulation Injection Extension

```rust
impl DriverContext {
    /// Injects a simulated value into a tag slot when the device is in ManualSimulation mode.
    ///
    /// # Safety & Invariants
    /// - Validates that the target device's configured execution mode is `ManualSimulation`.
    /// - Rejects with `DriverError::SimulationModeDisabled` if in `PhysicalHardware` mode.
    /// - Stamps the written slot with `QualityCode::UNCERTAIN_SUBSTITUTED` (0x0044).
    pub fn inject_simulated_tag(
        &mut self,
        tag_id: u32,
        value: mythos_shm::TagValueRaw,
        device_mode: DeviceExecutionMode,
    ) -> Result<(), crate::error::DriverError> {
        if device_mode != DeviceExecutionMode::ManualSimulation {
            tracing::warn!(
                tag_id = tag_id,
                "Attempted simulated injection into device operating in PhysicalHardware mode. Rejected."
            );
            return Err(crate::error::DriverError::SimulationModeDisabled(
                "Manual injection prohibited while device is in PhysicalHardware mode".into(),
            ));
        }

        // Stamped as UNCERTAIN_SUBSTITUTED (0x0044) per IEC 61850 / OPC DA
        let sim_quality = mythos_shm::QualityCode::UNCERTAIN_SUBSTITUTED;
        self.shm_writer
            .update_tag(tag_id, value, sim_quality)
            .map_err(|e| crate::error::DriverError::ShmWriteFailed(e.to_string()))
    }
}
```

---

### 7.5 Immutable Audit Trail Specification

Every transition between `PhysicalHardware` and `ManualSimulation`, as well as any direct variable forcing, is recorded in the append-only SQLite WAL audit log (`audit_events`):

```sql
CREATE TABLE IF NOT EXISTS audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,
    operator_id TEXT NOT NULL,
    action TEXT NOT NULL,              -- 'SET_DEVICE_MODE' | 'FORCE_VARIABLE' | 'CLEAR_FORCING'
    target_uri TEXT NOT NULL,           -- e.g., 'external.BoilerRoom.PT101'
    previous_state TEXT NOT NULL,       -- 'PhysicalHardware'
    new_state TEXT NOT NULL,            -- 'ManualSimulation'
    metadata_json TEXT                  -- Extended context: client IP, reason, token
);
```
