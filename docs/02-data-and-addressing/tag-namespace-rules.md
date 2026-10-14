# MythOS Data and Addressing Specification: Tag Namespace Rules

- **Document Identifier**: MYTHOS-DATA-001
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification establishes the canonical addressing rules and hierarchical Uniform Resource Identifier (URI) nomenclature for process variables (tags) in MythOS.

In legacy SCADA platforms, variables are addressed through raw register numbers (e.g., `40001`, `DB1.DBX0.0`) or flat tag names without plant hierarchy. These legacy schemes cause:
- High configuration error rates during plant commissioning.
- Broken tag bindings during PLC logic refactoring.
- Zero spatial or physical context for human operators and AI agents.
- Flawed transport-coupling where engineering tags depend on protocol names (e.g., Modbus or PROFINET).

MythOS enforces a **Physical Plant Hierarchy URI Convention**:
```
external.<namespace>.<device>.<variable>[.<subfield>]
```
All system components—including the OpenPLC FFI bridge, protocol drivers, HMI visual widgets, time-series historians, and MCP tools—address process data through this unified semantic format.

---

## 2. Namespace URI Hierarchy and Architecture

The tag URI reflects physical plant topology and instrument functional designations rather than communication transport protocols. Communication protocols (Modbus RTU/TCP, PROFINET, EtherNet/IP, MQTT) are transport details managed by protocol drivers; they **NEVER** appear in the semantic tag namespace.

### 2.1 External vs Internal Tag Classification

MythOS categorizes all process variables into two distinct domains:

1. **External Process Variables (`external.*`)**:
   - The prefix `external` is **STRICTLY RESERVED** for data originating from or dispatched to physical external field devices and field instruments (e.g., Supmea P200 pressure transmitters, Schneider Modicon PLCs, VFD motor drives, RTD temperature sensors, and remote I/O drops).
   - Variables with the `external` root prefix are synchronized across the lock-free Shared Memory TagBus (`/dev/shm/mythos_tagbus`) by protocol drivers.

2. **Internal PLC and Microkernel Variables (Non-`external`)**:
   - Internal PLC program variables, local intermediate calculations, state machine timers, and accumulator registers **DO NOT** use the `external` prefix.
   - Internal PLC variables use the domain prefix:
     `internal.<namespace>.<program_or_block>.<variable>`
     *(e.g., `internal.BoilerRoom.SeqController.step_timer` or `internal.BoilerRoom.SteamCalculation.total_mass_flow`)*.
   - Microkernel diagnostics, host OS statistics, and runtime performance metrics use the reserved root domain:
     `kernel.<subsystem>.<metric>`
     *(e.g., `kernel.system.cycle_time_us`, `kernel.shm.buffer_swap_count`)*.

---

## 3. Structural Segments of External Process Tags

The canonical URI for external field process data contains four mandatory dot-separated tokens, followed by optional structural subfields:

```
┌──────────┬────────────────────────────┬────────────────────────────┬───────────────────────┬──────────────────────┐
│  Domain  │    Namespace (Location)    │    Device (Instrument)     │   Variable (Metric)   │  Subfield (UDT/Flag) │
├──────────┼────────────────────────────┼────────────────────────────┼───────────────────────┼──────────────────────┤
│ external │ BoilerRoom                 │ PT101                      │ pressure              │ pv                   │
│ external │ BoilerRoom                 │ PT101                      │ pressure              │ raw                  │
│ external │ BoilerRoom                 │ FeedPump01                 │ speed                 │ sp                   │
│ external │ BoilerRoom                 │ FeedPump01                 │ speed                 │ pv                   │
│ external │ Line1                      │ Conveyor01                 │ motor_status          │ fault                │
│ external │ Sterilizer01               │ TT204                      │ temperature           │ pv                   │
│ external │ PKS.Station2               │ Supmea_P200                │ pressure              │ pv                   │
└──────────┴────────────────────────────┴────────────────────────────┴───────────────────────┴──────────────────────┘
```

### 3.1 Segment Definitions and Constraints

1. **Root Domain (`external`)**:
   - Fixed identifier indicating a process variable bound to an external physical device or field bus interface.
   - Designates that the value is acquired from or transmitted to a field instrument.

2. **Namespace (`<namespace>`)**:
   - Identifies the physical plant installation location, process area, or machine group.
   - Must conform to ISA-88 / ISA-95 physical asset models.
   - Permitted values represent real plant locations or process areas:
     * `BoilerRoom`: Utility and steam generation area.
     * `Line1`: Packaging or bottling production line 1.
     * `Sterilizer01`: Palm oil sterilization vessel unit 1.
     * `PKS.Station2`: Palm oil mill (PKS) station 2 area hierarchy.
   - Character set: `[A-Za-z0-9_]`. May contain dots (`.`) when declaring explicit hierarchical area nesting (such as `PKS.Station2`). Maximum length: 64 characters.

3. **Device Identifier (`<device>`)**:
   - Unique alphanumeric string identifying the physical machine, instrument tag, or logical field controller instance.
   - Conforms to ISA-5.1 instrumentation tagging conventions where applicable.
   - Examples:
     * `PT101`: Pressure Transmitter 101 (e.g., Supmea P200 gauge pressure sensor).
     * `TT204`: Temperature Transmitter 204 (e.g., PT100 RTD sensor).
     * `Supmea_P200`: Field instrument model designation.
     * `FeedPump01`: Boiler feedwater pump unit 1.
   - Character set: `[A-Za-z0-9_]`. Maximum length: 32 characters.

4. **Variable Name (`<variable>`)**:
   - Semantic name of the physical metric, measured process variable, setpoint, or command register.
   - Examples:
     * `pressure`: Fluid or steam pressure measurement.
     * `temperature`: Thermal measurement.
     * `flow_rate`: Volumetric or mass flow rate.
     * `speed`: Rotational velocity (RPM) or linear speed.
     * `motor_status`: Multi-bit status word.
   - Character set: `[a-z0-9_]`. Maximum length: 48 characters.

5. **Subfield (`<subfield>`) (Optional)**:
   - For User-Defined Types (UDT), structs, or bitfield flags, specifies the exact member element.
   - Standard industrial subfield qualifiers:
     * `.pv`: Process Value (actual measured reading confirmed by field device).
     * `.sp`: Setpoint (commanded target value from operator or batch recipe).
     * `.raw`: Unscaled raw integer register count from ADC/hardware.
     * `.fault`: Boolean trip or alarm condition.
     * `.mode`: Operating mode (Auto/Manual).
     * `.warn_hi`: High warning threshold.

---

## 4. Concrete Industrial Gold Standard Examples

### 4.1 Supmea P200 Pressure Transmitter in Boiler Room
- **Physical Device**: Supmea P200 Gauge Pressure Transmitter installed at Boiler 1 Steam Header.
- **Hardware Bus**: Modbus RTU RS-485 (Polled by `driver-modbus`).
- **Canonical MythOS Tag (Process Value)**:
  ```
  external.BoilerRoom.PT101.pressure.pv
  ```
- **Dissected URI Architecture**:
  * `external`: Originates from external physical field device.
  * `BoilerRoom`: Installation location and plant process area.
  * `PT101`: Instrument tag identifying the Supmea P200 unit.
  * `pressure`: Physical variable being measured.
  * `pv`: Process Value scaled into engineering units (`bar`).
- **Raw Modbus Register Tag**:
  ```
  external.BoilerRoom.PT101.pressure.raw
  ```

### 4.2 Sterilizer Vessel Temperature Transmitter
- **Physical Device**: PT100 RTD sensor with 4-20mA loop transmitter on Sterilizer Vessel 01.
- **Canonical MythOS Tag**:
  ```
  external.Sterilizer01.TT204.temperature.pv
  ```

### 4.3 Boiler Feedwater Pump Drive (Command vs Feedback)
- **Physical Device**: Variable Frequency Drive (VFD) controlling Feed Pump 01.
- **Commanded Speed Setpoint (Write-Only / Shadow)**:
  ```
  external.BoilerRoom.FeedPump01.speed.sp
  ```
- **Actual Measured Feedback Speed (Read-Only)**:
  ```
  external.BoilerRoom.FeedPump01.speed.pv
  ```
- **Drive Tripped Interlock**:
  ```
  external.BoilerRoom.FeedPump01.status.fault
  ```

---

## 5. Formal Grammar (EBNF)

```ebnf
TagUri        ::= ExternalUri | InternalUri | KernelUri ;

ExternalUri   ::= "external." PlantArea "." DeviceId "." VariableId ( "." MemberId )* ;
InternalUri   ::= "internal." PlantArea "." BlockId "." VariableId ( "." MemberId )* ;
KernelUri     ::= "kernel." SubsystemId "." MetricId ;

PlantArea     ::= AreaToken ( "." AreaToken )* ;
AreaToken     ::= [A-Za-z0-9_]{1,32} ;
DeviceId      ::= [A-Za-z0-9_]{1,32} ;
BlockId       ::= [A-Za-z0-9_]{1,32} ;
VariableId    ::= [a-z0-9_]{1,48} ;
MemberId      ::= [a-z0-9_]{1,32} ;
SubsystemId   ::= [a-z0-9_]{1,32} ;
MetricId      ::= [a-z0-9_]{1,48} ;
```

---

## 6. Metadata Attributes and Quality Propagation

Every registered tag slot carries standardized industrial metadata attributes:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TagMetadata {
    /// Canonical dot-separated URI (e.g., "external.BoilerRoom.PT101.pressure.pv")
    pub uri: String,

    /// Unique numeric index into Shared Memory array
    pub slot_id: u32,

    /// Human-readable description (e.g., "Boiler 1 Main Steam Header Pressure")
    pub description: String,

    /// Engineering units (e.g., "bar", "°C", "RPM", "m³/h", "%")
    pub engineering_units: String,

    /// Lower physical limit of sensor scale
    pub scale_min: f64,

    /// Upper physical limit of sensor scale
    pub scale_max: f64,

    /// Raw register integer lower bound (e.g., 0)
    pub raw_min: i64,

    /// Raw register integer upper bound (e.g., 65535)
    pub raw_max: i64,

    /// Deadband filter percentage (0.0 to 100.0%)
    pub deadband_percent: f64,

    /// Historical retention policy
    pub retention_mode: RetentionMode,

    /// Access Mode permissions (ReadOnly, WriteOnly, ReadWrite)
    pub access_mode: AccessMode,
}
```

### 6.1 Quality Codes (IEC 61850 / OPC DA Standard)

Quality flags communicate operational integrity across the system. When a communication link drops, quality updates immediately to `BAD_COMM_FAILURE` without zeroing the last known value, preventing spurious alarms or false zero readings:

```
Good Quality (0x00C0):
  - Normal operational reading verified by driver CRC.

Bad Quality:
  - 0x0004: Configuration error (scale invalid, type mismatch).
  - 0x0008: Field device disconnected / socket unreachable.
  - 0x0010: Hardware sensor fault (4-20mA loop wire break < 3.6mA).
  - 0x0018: Communication frame timeout.

Uncertain Quality:
  - 0x0044: Substituted manual value (operator forced override).
  - 0x0048: Initial startup default (prior to first valid field poll).
```

### 6.2 High-Resolution Timestamping
- All timestamps use **64-bit unsigned integers representing nanoseconds since UNIX Epoch (UTC)**.
- Generated by the protocol driver at packet ingestion using `CLOCK_REALTIME` or hardware PHY timestamping (IEEE 1588 PTP where available).
