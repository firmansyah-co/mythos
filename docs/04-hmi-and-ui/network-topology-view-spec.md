# MythOS Devices & Networks Specification: The 3 Canonical Hardware Views (Network View, Device View, Topology View)

- **Document Identifier**: MYTHOS-HMI-004
- **Version**: 3.0.0
- **Author**: Iris (VP Design & Visual Architecture) & Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Standard Reference**: Siemens TIA Portal Parity (SYS-REQ-097), ANSI/ISA-101.01, ISO 9241-110, IEC 61158, IEC 61784, IEC 61131-3, W3C DTCG
- **Language**: ASD-STE100 Technical English

---

## 1. Executive Summary & Purpose

This specification establishes the architectural, visual, and interaction design for the MythOS **Devices & Networks** engineering workspace. Mirroring the industry-standard **Siemens TIA Portal** hardware engineering paradigm, this subsystem unifies hardware topology design, modular rack configuration, and physical cabling into **3 Canonical Hardware Views**:

1. **Network View (`Netzsicht`)**: Station-level logical fieldbus topology interconnecting controllers, distributed I/O racks, variable frequency drives (VFDs), and smart instrumentation over colored industrial buses (PROFINET, RS-485 Modbus, PROFIsafe, IO-Link).
2. **Device View (`Gerätesicht`)**: Physical slot-by-slot chassis inspection and parameterization of modular hardware racks (`Rack 0`, `Slot 0` = Power, `Slot 1` = CPU/Coupler, `Slot 2..N` = Signal Modules), including terminal strip pinouts and channel address assignment.
3. **Topology View (`Topologiesicht`)**: Physical port-to-port cable wiring (e.g. `CPU X1-P1` -> `Switch Port 3`) enabling automated neighbor discovery (LLDP / PROFINET DCP), deterministic cable discrepancy detection, and zero-programming hardware replacement.

This tripartite architecture integrates directly with the **Zone 2 Project Tree**, the **Zone 4 Hardware Catalog Task Card**, and the **Zone 5 Context-Sensitive Bottom Properties & Inspector**, providing industrial automation engineers with an immediately familiar, high-performance web engineering environment.

---

## 2. Normative References & Standards

- **ANSI/ISA-101.01-2015**: *Human Machine Interfaces for Process Automation Systems*.
- **ISO 9241-110:2020**: *Ergonomics of human-system interaction — Principles of interaction*.
- **IEC 61158 / IEC 61784**: *Industrial communication networks — Fieldbus specifications*.
- **IEC 61131-3:2013**: *Programmable controllers — Part 3: Programming languages*.
- **IEC 62443-3-3:2018**: *Security for industrial automation and control systems*.
- **IEEE 802.1AB-2016**: *Station and Media Access Control Connectivity Discovery (LLDP)*.
- **W3C Design Tokens Community Group (DTCG)**: *2025.10 Community Specification*.

---

## 3. Visual & Spatial Layout Architecture

### 3.1 5-Zone Workbench Integration

The Devices & Networks workspace executes inside **Zone 3 (Main Tabbed Canvas)** of the MythOS 5-Zone Workbench Shell, coordinated with the rest of the engineering environment:

```
+---------------------------------------------------------------------------------------------------------------+
| ZONE 1: TOP HUD BANNER (38px Fixed Height - Plant State, Node Health, Master Lock, Global Alarm Counts)       |
+----+-----------------------------+----------------------------------------------+-----------------------------+
| Z  | ZONE 2: PRIMARY EXPLORER    | ZONE 3: MAIN DOCKVIEW CANVAS                 | ZONE 4: TASK CARDS (RIGHT)  |
| O  | (240px - 360px Resizable)   | (Devices & Networks Canvas - 100% Flex)      | (384px Fixed Width)         |
| N  | ▼ Project: PlantLine_01     | +------------------------------------------+ | +-------------------------+ |
| E  |   ▼ Devices & Networks      | | [🌐 Network View] [🎛️ Device] [🔌 Topology] | | | [📦 Hardware Catalog]     | |
|    |     ├── Network View        | +------------------------------------------+ | |   ▼ Siemens S7-1500 / ET  | |
| 5  |     ├── Device View         |                                              | |     ├── CPU 1515-2 PN   | |
| :  |     └── Topology View       |   Interactive Node Graph Canvas              | |     ├── DI 16x24VDC HF  | |
| A  |   ▼ Controller [Soft-Logic] |   [PLC-Rack]======[Coupler]======[VFD]       | |     └── DQ 16x24VDC HF  | |
| C  |   ▼ Visualization [Web HMI] |        \\                                      | | [Libraries] [Layouts]   | |
| T  +-----------------------------+----------------------------------------------+-----------------------------+
| (48| ZONE 5: CONTEXT-SENSITIVE BOTTOM PROPERTIES & INSPECTOR (180px - 320px Collapsible)                      |
| px)| [Tabs: 📋 General | 🏷️ IO Addresses / Variables | 🩺 Diagnostics | 🚨 Alarms | 💻 Compiler Console]      |
|    | Properties for Selected Object: Rack_0 / Slot_2 [DI 16x24VDC HF] -> Start Address: %I0.0 ... %I1.7       |
|    +----------------------------------------------------------------------------------------------------------+
|    | ZONE 1.1: STATUS BAR (24px Fixed Height - Gateway Online, Scan Cycle: 10ms, Active Bus Drops: 0)         |
+----+----------------------------------------------------------------------------------------------------------+
```

### 3.2 View Navigation Switcher Bar

At the top of Zone 3, the Devices & Networks tab renders an industrial segmented switcher bar:
- **`[ 🌐 Network View ]`**: Hotkey `Alt+1`. Activates station-level bus interconnectivity canvas.
- **`[ 🎛️ Device View ]`**: Hotkey `Alt+2`. Activates modular chassis rack and slot configuration.
- **`[ 🔌 Topology View ]`**: Hotkey `Alt+3`. Activates port-to-port physical cabling and LLDP link inspection.

---

## 4. The 3 Canonical Hardware Engineering Views

### 4.1 Canonical View 1: Network View (`Netzsicht`)

The **Network View** displays all automated stations, standalone controllers, distributed I/O islands, drives, and network gateways mapped to their logical industrial communication subnets:

```
+----------------------------------------------------------------------------------------------------+
| ZONE 3: NETWORK VIEW (Station-Level Fieldbus Interconnections)                                     |
+----------------------------------------------------------------------------------------------------+
|  Subnet: PROFINET_IO_1 (192.168.1.0/24) --------------------------------------------------------+  |
|     ||                                                                                           |  |
|  +--++---------------+         +-----------------+         +-----------------+                   |  |
|  | PLC_RACK_01       |         | ET200SP_ISLAND  |         | VFD_PUMP_101    |                   |  |
|  | S7-1515-2 PN      |=========| IM 155-6 PN HF  |=========| Sinamics G120   |                   |  |
|  | IP: 192.168.1.10  |         | IP: 192.168.1.11|         | IP: 192.168.1.12|                   |  |
|  | PROFINET Master   |         | Device ID: 2    |         | Telegram 1      |                   |  |
|  +-------------------+         +-----------------+         +-----------------+                   |  |
|                                                                                                  |  |
|  Subnet: MODBUS_RS485_CH1 (9600-8-N-1) .........................................................+  |
|     :                                                                                           |  |
|  +--::---------------+         +-----------------+         +-----------------+                   |  |
|  | CM_PtP_RS485      |         | FLOWMETER_FT101 |         | TEMP_SENSOR_TT  |                   |  |
|  | Slot 4 Serial Mod |.........| Modbus RTU #1   |.........| Modbus RTU #2   |                   |  |
|  +-------------------+         +-----------------+         +-----------------+                   |  |
+----------------------------------------------------------------------------------------------------+
```

- **Core Capabilities**:
  * **Station Drag-and-Drop**: Dragging a controller or distributed I/O station from Zone 4 Hardware Catalog into the Network View canvas instantiates a new network node.
  * **Bus Cable Connecting**: Clicking and dragging from a station's network port connector to another station or subnet line creates a fieldbus connection.
  * **Subnet Highlighting**: Hovering over or selecting a subnet line highlights all attached stations and dims unaffiliated hardware nodes.
  * **Automatic IP Assignment**: Assigns sequential IP addresses within the subnet range based on user-defined DHCP/static pools.

---

### 4.2 Canonical View 2: Device View (`Gerätesicht`)

The **Device View** provides a realistic, slot-by-slot graphical and tabular model of a single selected station's physical chassis:

```
+----------------------------------------------------------------------------------------------------+
| ZONE 3: DEVICE VIEW (Rack 0: PLC_RACK_01 - Modular Rail DIN Chassis)                               |
+----------------------------------------------------------------------------------------------------+
|  [◀ Back to Network View]   Station: [ PLC_RACK_01 (Siemens S7-1515-2 PN) v ]   Zoom: [ 100% v ]  |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|  +------+------+------+------+------+------+------+------+------+------+------+------+              |
|  | PS   | CPU  | DI   | DQ   | AI   | AQ   | CP   | TM   | [  ] | [  ] | [  ] | [  ] |              |
|  | 24V  | 1515 | 16x  | 16x  | 8xU/I| 4xU/I| PtP  | Count| Slot | Slot | Slot | Slot |              |
|  | PM   | 2 PN | 24VDC| 24VDC| 16bit| 16bit| RS485| 24V  |  8   |  9   |  10  |  11  |              |
|  |      |      |      |      |      |      |      |      |      |      |      |      |              |
|  | Slot | Slot | Slot | Slot | Slot | Slot | Slot | Slot |      |      |      |      |              |
|  |  0   |  1   |  2   |  3   |  4   |  5   |  6   |  7   |      |      |      |      |              |
|  +------+------+------+------+------+------+------+------+------+------+------+------+              |
|                                                                                                    |
|  CHASSIS MODULE CONFIGURATION TABLE:                                                               |
|  +------+-------------------+-----------------------+-------------+-------------+---------------+  |
|  | Slot | Module Name       | Catalog Article No.   | I Address   | Q Address   | Firmware / SW |  |
|  +------+-------------------+-----------------------+-------------+-------------+---------------+  |
|  | 0    | PM 190W 24VDC     | 6EP1333-4BA00         | ---         | ---         | V1.0          |  |
|  | 1    | CPU 1515-2 PN     | 6ES7515-2AM02-0AB0    | %I0.0..%I0.7| %Q0.0..%Q0.7| V2.9.2        |  |
|  | 2    | DI 16x24VDC HF    | 6ES7521-1BH00-0AB0    | %I10..%I11  | ---         | V1.1.0        |  |
|  | 3    | DQ 16x24VDC HF    | 6ES7522-1BH01-0AB0    | ---         | %Q10..%Q11  | V1.1.0        |  |
|  | 4    | AI 8xU/I/RTD/TC HF| 6ES7531-7KF00-0AB0    | %IW20..%IW34| ---         | V2.0.0        |  |
|  | 5    | AQ 4xU/I ST       | 6ES7532-5HD00-0AB0    | ---         | %QW20..%QW26| V1.0.0        |  |
|  | 6    | CP 1541-1 RS485   | 6GK7541-1AB00-0AB0    | ---         | ---         | V1.0.0        |  |
|  | 7    | TM Count 2x24V    | 6ES7550-1AA00-0AB0    | %ID40..%ID44| %QD40..%QD44| V1.3.0        |  |
|  +------+-------------------+-----------------------+-------------+-------------+---------------+  |
+----------------------------------------------------------------------------------------------------+
```

- **Deterministic Modular Rack Indexing**:
  * **`Rack 0`**: Standard base DIN mounting rail.
  * **`Slot 0`**: System Power Supply (PS / PM).
  * **`Slot 1`**: Central Processing Unit (CPU) or Remote I/O Bus Interface Module (IM).
  * **`Slot 2..N`**: Distributed input/output signal slices, communication processors, and technology modules.
- **Terminal Strip & Channel Pinout Inspection**:
  * Clicking any module slice exposes its tactile 20-pin or 40-pin terminal block representation.
  * Individual channel LEDs reflect live operational state (`RUN` green, `ERR` red, channel bits 0..15 illuminated when high).
  * Pin-by-pin hardware tag mapping: Pin 1 = `+24VDC L+`, Pin 2 = `%I10.0 [external.e_stop_pb1]`, Pin 3 = `%I10.1 [external.safety_gate_ls]`.
- **Address Range & Filtering Controls**:
  * Automatic continuous address allocation or manual fixed address assignment.
  * Instant conflict detection: Overlapping I/O bytes trigger immediate amber warnings and block compilation.

---

### 4.3 Canonical View 3: Topology View (`Topologiesicht`)

The **Topology View** models the exact, physical port-to-port Ethernet and fieldbus wiring between hardware interfaces:

```
+----------------------------------------------------------------------------------------------------+
| ZONE 3: TOPOLOGY VIEW (Physical Port-to-Port Cabling & Neighborhood Detection)                     |
+----------------------------------------------------------------------------------------------------+
|  [◀ Back to Network View]   Topology Status: [ ✓ All Links Match Nominal Design ]  [ Auto-Route ]  |
+----------------------------------------------------------------------------------------------------+
|                                                                                                    |
|    +-----------------------+                    +-----------------------+                          |
|    | PLC_RACK_01 (CPU1515) |                    | SCALANCE_XC208        |                          |
|    | Port X1-P1 [RJ45]     |--------------------| Port 1 [RJ45]         |                          |
|    | Port X1-P2 [RJ45]     |----+               | Port 2..8 [RJ45]      |                          |
|    +-----------------------+    |               +-----------+-----------+                          |
|                                 |                           |                                      |
|         Cat6A PROFINET (12.4m)  |                           | Cat6A PROFINET (4.2m)                |
|                                 |                           |                                      |
|                                 |               +-----------+-----------+                          |
|                                 |               | ET200SP_ISLAND (IM155)|                          |
|                                 +---------------| Port X1-P1 [RJ45]     |                          |
|                                 (Redundant Ring)| Port X1-P2 [RJ45]     |                          |
|                                                 +-----------------------+                          |
|                                                                                                    |
|  PORT-TO-PORT CONNECTION MATRIX:                                                                   |
|  +------------------------+------------------------+---------------+--------------+-------------+  |
|  | Local Port             | Partner Port           | Cable Type    | Nominal Dist | Diagnostics |  |
|  +------------------------+------------------------+---------------+--------------+-------------+  |
|  | PLC_RACK_01:X1-P1      | SCALANCE_XC208:Port 1  | Cat6A Shielded| 12.4 m       | OK (100 Mbps|  |
|  | SCALANCE_XC208:Port 2  | ET200SP_ISLAND:X1-P1   | Cat6A Shielded| 4.2 m        | OK (100 Mbps|  |
|  | PLC_RACK_01:X1-P2      | ET200SP_ISLAND:X1-P2   | Cat6A Shielded| 18.1 m (MRP) | Standby Ring|  |
|  +------------------------+------------------------+---------------+--------------+-------------+  |
+----------------------------------------------------------------------------------------------------+
```

- **Port-Level Deterministic Wiring**:
  * Distinguishes exact physical ports (`X1-P1`, `X1-P2`, `X2-P1`).
  * Enforces Media Redundancy Protocol (MRP) ring visualization with primary and ring-partner roles.
- **Automated Device Replacement Without Removable Storage (DCP / LLDP)**:
  * When a physical device breaks down on the factory floor, a field technician slots in a replacement unit directly out of the box without using a programming laptop.
  * The neighboring switch or controller detects the unconfigured device on the specific port via Link Layer Discovery Protocol (LLDP IEEE 802.1AB), automatically assigns the configured PROFINET device name, and downloads its hardware parameters.
  * **Precondition — the nominal topology must already define the slot.** This replacement workflow is valid only when the project's nominal topology already names the device and its parameters for that physical slot. In that case, discovery supplies the identity of the physical unit. Discovery does not supply the configuration.
  * **Discovery does not create plant equipment.** A device that discovery finds and the nominal topology does not define is a candidate. It is not plant equipment. It becomes plant equipment only when an authorized operator commissions it (see ADR-009). No discovery response shall cause the system to create a station, an address, or a cable in the plant model. This rule prevents the class of defect removed from the Network View render path by PR #33.
  * **What the frontend enforces, and what it does not.** The render path guards the *claim*, not the plant model: since `c12b5af`, `TopologyClaim` (`ui-topology/src/provenance.tsx`) is a union in which the `DISCOVERED` arm **requires** a `commissioning` state, so "discovered, and therefore plant equipment" is not a state the type can express, and conformance is derived by `deriveConformance` rather than accepted from a caller. That prevents a surface from *presenting* a candidate as commissioned. It does not prevent a discovery response from writing a station, an address, or a cable into the plant model — that is a backend path, and **no such path exists in the merged tree today**: `NetworkTopology::add_node` (`crates/mythos-core/src/topology.rs:237`), `add_edge` (`:266`) and `add_ip_block` (`:312`) have **no production callers at all**; every caller is in `crates/mythos-core/tests/`, and there is no topology route under `crates/mythos-core/src/api/`. So the rule is normative for the whole system, the frontend guard is narrower than the rule, and neither should be read as a guarantee that a discovery response cannot reach the plant model. That guarantee is unbuilt, and saying so is the honest state.
- **Discrepancy Detection & Online Comparison**:
  * Real-time comparison between **Nominal Topology** (engineered in MythOS) and **Actual Topology** (discovered via LLDP).
  * Mismatches (e.g. cable plugged into Port 3 instead of Port 1) appear in pulsing high-contrast yellow/red dashed lines with guided re-patch instructions.

---

## 5. Industrial Cable Protocols & Visual Tokens

To prevent operator error and reflect authentic shop-floor wiring standards, cable edges strictly adhere to standardized color tokens:

| Protocol Family | Edge Hex Code | DTCG Token | Stroke Width | Line Style | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **PROFINET / Industrial Ethernet** | `#16A34A` | `color.bus.profinet` | `3px` | Solid `none` | Standard industrial green Cat5e/Cat6A industrial Ethernet |
| **RS-485 Modbus / PROFIBUS DP** | `#0284C7` | `color.bus.serial` | `2.5px` | Solid `none` | Industrial blue shielded twisted pair (PROFIBUS / Belden 3106A) |
| **Functional Safety (PROFIsafe / STO)** | `#DC2626` | `color.bus.safety` | `4px` | Solid `none` | SIL-3 / Cat 4 safety circuits, light curtains, emergency stops |
| **CANopen / DeviceNet** | `#CA8A04` | `color.bus.canopen` | `2.5px` | Dash `6,3` | Industrial amber/gold differential fieldbus |
| **IO-Link** | `#9333EA` | `color.bus.iolink` | `2px` | Solid `none` | Purple 3-wire point-to-point smart sensor connection |
| **Offline / Severed Topology Link** | `#475569` | `color.bus.offline` | `2px` | Dash `4,4` | Muted slate indicating loss of carrier signal or port fault |

---

## 6. Drag-and-Drop Gestures & Connection Verification Engine

### 6.1 Inter-Zone Interaction Flow
1. **Catalog Drag (Zone 4 -> Zone 3)**:
   - User drags a module from Zone 4 `Hardware Catalog` (e.g. `DQ 16x24VDC HF`).
   - In **Device View**, valid empty rack slots illuminate with green snap targets. Incompatible slots (e.g. attempting to drop a CPU into an I/O slice slot) dim out.
2. **Port Cabling (Zone 3 Canvas)**:
   - Dragging a line from a port handle activates `TopologyConnecting` mode.
   - Compatible destination ports pulse with their protocol highlight halo ($r = 8	ext{px}$).
3. **Inspector Sync (Zone 3 -> Zone 5)**:
   - Selecting any station, rack slot, or cable link instantly updates Zone 5 Bottom Properties & Inspector.

### 6.2 Connection Compatibility Rules
- RJ45 Ethernet cannot bridge directly to RS-485 serial without an intermediate gateway.
- IO-Link master ports only accept IO-Link compliant devices or standard discrete sensors.
- PROFIsafe modules require a safety-capable controller (F-CPU) host.

---

## 7. Context-Sensitive Bottom Properties & Inspector (Zone 5 Integration)

Selecting an element in any of the 3 Hardware Views automatically populates Zone 5 Bottom Inspector with 4 standard tabs:

```
+----------------------------------------------------------------------------------------------------+
| ZONE 5: BOTTOM PROPERTIES & INSPECTOR                                                              |
| [ 📋 General ]  [ 🏷️ IO Addresses / Variables ]  [ 🩺 Diagnostics ]  [ 🚨 Alarms ]                  |
+----------------------------------------------------------------------------------------------------+
| Selection: Rack 0 / Slot 2 (DI 16x24VDC HF)                                                        |
|                                                                                                    |
| TAB: 🏷️ IO Addresses / Variables                                                                   |
|   Inputs:                                                                                          |
|     Start Address:  [ %I10.0 ]     End Address: [ %I11.7 ]      Organization Block: [ OB1 (Cyclic)]|
|     Process Image:  [ Automatic Update v ]                                                         |
|                                                                                                    |
|   Channel Tag Binding Table:                                                                       |
|   +---------+-----------+---------+---------------------------------+---------------------------+  |
|   | Channel | Address   | Type    | Tag Name                        | Comment                   |  |
|   +---------+-----------+---------+---------------------------------+---------------------------+  |
|   | Ch 0    | %I10.0    | BOOL    | external.raw_estop_pb1          | Emergency Stop Button 1   |  |
|   | Ch 1    | %I10.1    | BOOL    | external.guard_door_interlock   | Safety Guard Interlock    |  |
|   | Ch 2    | %I10.2    | BOOL    | external.conveyor_pe_sensor     | Infeed Photoelectric Eye  |  |
|   +---------+-----------+---------+---------------------------------+---------------------------+  |
+----------------------------------------------------------------------------------------------------+
```

---

## 8. TypeScript Interface Contracts

```typescript
/**
 * MythOS Devices & Networks Schema Definitions
 * Parity with Siemens TIA Portal Hardware Architecture (SYS-REQ-097)
 */

export type HardwareViewMode = 'NETWORK' | 'DEVICE' | 'TOPOLOGY';

export type ProtocolType = 
  | 'PROFINET' 
  | 'MODBUS_TCP' 
  | 'MODBUS_RTU' 
  | 'PROFIBUS_DP' 
  | 'CANOPEN' 
  | 'IOLINK' 
  | 'SAFETY_PROFISAFE';

export interface NetworkPort {
  id: string;
  name: string; // e.g. "X1-P1", "X1-P2", "Port 1"
  protocol: ProtocolType;
  connectorType: 'RJ45' | 'M12_D_CODED' | 'DB9' | 'TERMINAL_BLOCK' | 'M12_A_CODED';
  ipAddress?: string;
  subnetMask?: string;
  macAddress?: string;
  baudRate?: number;
  slaveId?: number;
  linkStatus: 'UP' | 'DOWN' | 'DEGRADED';
  rxBytesSec: number;
  txBytesSec: number;
  crcErrors: number;
}

export interface HardwareModuleSlot {
  slotNumber: number; // 0..31
  catalogArticleNo: string; // e.g. "6ES7521-1BH00-0AB0"
  name: string; // e.g. "DI 16x24VDC HF"
  moduleCategory: 'POWER' | 'CPU' | 'DI' | 'DQ' | 'AI' | 'AQ' | 'COMM' | 'TECH';
  startAddressI?: string; // e.g. "%I10.0"
  endAddressI?: string;   // e.g. "%I11.7"
  startAddressQ?: string; // e.g. "%Q10.0"
  endAddressQ?: string;   // e.g. "%Q11.7"
  ports: NetworkPort[];
  diagnostics: {
    runLed: boolean;
    errLed: boolean;
    maintLed: boolean;
    channelBits: boolean[];
    temperatureC?: number;
  };
}

export interface HardwareStationNode {
  id: string;
  stationName: string; // e.g. "PLC_RACK_01", "ET200SP_ISLAND"
  stationType: 'PLC_RACK' | 'REMOTE_IO' | 'VFD_DRIVE' | 'FIELD_INSTRUMENT' | 'SWITCH';
  rackNumber: number;  // Default 0
  slots: HardwareModuleSlot[];
  position: { x: number; y: number };
  isLocked: boolean;
}

export interface TopologyCableEdge {
  id: string;
  sourceStationId: string;
  sourcePortId: string; // e.g. "X1-P1"
  targetStationId: string;
  targetPortId: string; // e.g. "Port 1"
  protocol: ProtocolType;
  cableLengthMeters?: number;
  nominalMatch: boolean;
  health: {
    status: 'OPTIMAL' | 'WARNING' | 'CRITICAL' | 'OFFLINE';
    latencyMs: number;
    packetLossPct: number;
    retriesPerSec: number;
  };
}

/**
 * PROVENANCE — what a topology surface is claiming, and on what basis.
 *
 * Three properties, deliberately not one enum. This section previously declared no
 * provenance contract at all; an earlier draft of the render-path work
 * (`test/topology-all-view-modes`, commit 52aa7f5) carried a single four-value state,
 * which mixed a source, a health measurement and a derived conformance claim into one
 * list. That was split before merge (`21cb9c4`), and the shape below is what landed as
 * c12b5af — it is the contract a consumer should build against.
 */

/** AXIS A — on what basis do we assert this device exists? */
export type TopologySource =
  | 'UNREAD'      // nothing has been read: no transport exists, so no claim can be made
  | 'CONFIGURED'  // nominal topology: read from the project's configured topology model
  | 'DISCOVERED'; // actual topology: seen by discovery (LLDP / DCP)

/** AXIS B — did it answer, and how well? A measurement, not project data. */
export type TopologyHealth =
  | 'UNKNOWN'      // not measured
  | 'REACHABLE'    // answered
  | 'UNREACHABLE'  // measured, and it did not answer: its state is unknown, not healthy
  | 'DEGRADED';    // answered, but not cleanly

/**
 * A property of a DISCOVERED device, not a fourth source value. Both are discovered;
 * the difference is whether a person accepted the device into the plant. A candidate is
 * not plant equipment and must never read as though it were.
 */
export type TopologyCommissioning = 'CANDIDATE' | 'COMMISSIONED';

/** DERIVED from nominal vs actual. Never an input state — see deriveConformance below. */
export type TopologyConformance = 'NOT_COMPARED' | 'MATCHES' | 'MISMATCH';

/**
 * What a view is claiming: the product of the two axes, with commissioning carried
 * alongside discovery. The union encodes the impossible pairing — nothing read, yet
 * something measured — so a consumer cannot construct a claim its own data does not
 * support.
 */
export type TopologyClaim =
  | { readonly source: 'UNREAD'; readonly health: 'UNKNOWN' }
  | { readonly source: 'CONFIGURED'; readonly health: TopologyHealth }
  | {
      readonly source: 'DISCOVERED';
      readonly health: TopologyHealth;
      readonly commissioning: TopologyCommissioning;
    };

export interface NetworkCanvasProps {
  /**
   * What these views are claiming and on what basis.
   *
   * DELIBERATELY NOT DEFAULTED to 'UNREAD'. UNREAD is true of an empty canvas and false
   * of a populated one, so omitting this while supplying stations renders NOT_STATED —
   * the surface says the provenance was not stated, instead of asserting that no claim is
   * being made while it visibly is. A default here would make a populated canvas announce
   * that it knows nothing.
   */
  claim?: TopologyClaim;
  /**
   * The two device-id sets conformance is DERIVED from. A caller may supply
   * observations; it may not supply a conclusion. There is deliberately no `conformance`
   * prop: "the caller asserted MATCHES" is not a state this type can express. Both absent
   * (today's only reachable case) derives NOT_COMPARED.
   */
  nominalDeviceIds?: readonly string[];
  actualDeviceIds?: readonly string[];
  initialStations?: HardwareStationNode[];
  initialCables?: TopologyCableEdge[];
  onSelectStation?: (station: HardwareStationNode | null) => void;
  onSelectSlot?: (stationId: string, slotNumber: number) => void;
  onSelectPort?: (stationId: string, port: NetworkPort) => void;
  className?: string;
  viewMode?: HardwareViewMode;
  onViewModeChange?: (mode: HardwareViewMode) => void;
}

export interface DevicesAndNetworksState {
  activeViewMode: HardwareViewMode;
  selectedStationId: string | null;
  selectedSlotNumber: number | null;
  selectedPortId: string | null;
  selectedCableId: string | null;
  stations: HardwareStationNode[];
  cables: TopologyCableEdge[];
  setActiveViewMode: (mode: HardwareViewMode) => void;
  selectElement: (stationId: string | null, slotNumber?: number, portId?: string) => void;
}
```

> **Provenance is carried on the view, not on each node.** `HardwareStationNode` has no
> provenance field, and that is the design rather than an omission: the question "on what
> basis do we assert this equipment exists" is a property of what the surface read, not of
> an individual station. A per-node field would allow one canvas to mix provenances and
> would put the claim where nothing derives it. Per-device health that *is* modelled lives
> on the node itself — `HardwareStationNode.status: DeviceStatus` (`'RUN' | 'STOP' | 'FAULT' |
> 'OFFLINE'`, `ui-topology/src/types.ts:18`) — and it answers a different question from
> `TopologyHealth`: the device's own operating state, not whether this surface could read it.

---

## 9. Visual Quality Control & Box-Model Verification

- **Pixel Hierarchy**: Zero double scrollbars across all view modes; canvas is hosted with `flex: 1 1 0%`, `min-h: 0`, and `overflow: hidden`.
- **Contrast Ratios**: All port, slot, and cable labels exceed WCAG 2.1 AA luminance ratio ($\ge 4.5:1$) against dark slate backgrounds (`#0F141C` and `#161E2E`).
- **Rendering Performance**: Device View SVG pinout renders instantaneously; Network and Topology views sustain 60 FPS up to 300 connected stations and 500 cable edges using WebGL/React Flow GPU acceleration.
