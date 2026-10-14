# MythOS Network Topology View Specification

## 1. Executive Summary & Purpose
This document specifies the architectural, visual, and interaction design for the MythOS Interactive Network Topology View. Inspired by Siemens TIA Portal Network View and Schneider EcoStruxure Automation Expert, this subsystem provides an interactive, node-graph canvas where automation engineers, commissioning technicians, and control room operators can visually discover, lay out, configure, and diagnose industrial hardware racks, remote I/O islands, smart sensors, and variable frequency drives (VFDs).

The Network Topology View integrates directly with the MythOS Hardware Catalog and Runtime Microkernel, bridging low-level protocol drivers (PROFINET, Modbus TCP, Modbus RTU, EtherCAT, and IO-Link) with high-performance WebGL/SVG graphics running in modern Chromium-based edge HMIs.

---

## 2. Normative References & Standards
- **ANSI/ISA-101.01-2015**: *Human Machine Interfaces for Process Automation Systems*.
- **ISO 9241-110:2020**: *Ergonomics of human-system interaction — Principles of interaction*.
- **IEC 61158 / IEC 61784**: *Industrial communication networks — Fieldbus specifications*.
- **IEC 62443-3-3**: *Industrial communication networks — Network and system security*.
- **W3C Design Tokens Community Group (DTCG)**: *2025.10 Community Specification*.

---

## 3. Visual & Spatial Layout Architecture

### 3.1 5-Zone Workbench Integration
The Network Topology View renders inside Zone 3 (Main Tabbed Canvas) of the MythOS 5-Zone Workbench Shell. It strictly honors the spatial box-model invariants enforced by the autonomous enterprise design governance:

```
+---------------------------------------------------------------------------------------------------+
| ZONE 1: TOP HUD BANNER (38px Fixed Height - Node ID, Health, Master Lock, Global Alarms)          |
+----+----------------------------------+----------------------------------+------------------------+
| Z  | ZONE 2: PRIMARY EXPLORER        | ZONE 3: MAIN DOCKVIEW CANVAS     | ZONE 4: SECONDARY      |
| O  | (240px - 320px Resizable)        | (Interactive Network Topology    | INSPECTOR DRAWER       |
| N  | - Physical Hardware Tree         |  xyflow Canvas - 100% flex)      | (360px - 420px)        |
| E  | - Modbus / PROFINET Channels     |                                  |                        |
|    | - Tag Namespace Explorer         | [PLC-Rack]======[Coupler]======[VFD] | - Port Addresses       |
| 5  |                                  |      \\                          | - Cable Diagnostics    |
| :  |                                  |       \---[RS485]---[Sensor]     | - Comm Packet Counters |
| A  +----------------------------------+----------------------------------+ - Hardware UDT Binds   |
| C  | ZONE 5: BOTTOM INDUSTRIAL CONSOLE (160px - 280px Collapsible)                                |
| T  | - Active ISA-18.2 Alarms | Comm Packet Sniffer | Telemetry Stream | Diagnostics Terminal     |
| (48+----------------------------------------------------------------------------------------------+
| px)| ZONE 1.1: STATUS BAR (24px Fixed Height - Gateway Lifecycle, Cycle Time 10ms, Packet Queue)  |
+----+----------------------------------------------------------------------------------------------+
```

### 3.2 Canvas Engine (xyflow / ReactFlow v12)
The canvas operates on top of `@xyflow/react` (v12.x) with custom hardware-accelerated nodes and edges.
- **Background**: Hex `#0A0D12` (Base Obsidian) with a subtle dot grid pattern `#1E293B` spaced at 20px intervals.
- **Viewport Controls**: Infinite zoom range ($0.15\times$ to $2.5\times$), double-click auto-fit, and mini-map inset located at the bottom-right of Zone 3.
- **Grid Snapping**: Deterministic snap-to-grid grid at 10px intervals to ensure absolute horizontal and vertical cable alignment.

---

## 4. Hardware Node Taxonomy & Component Models

Every physical and virtual device in the MythOS topology is categorized into one of four deterministic node archetypes:

```
+-------------------------------------------------------------------------------------+
|                               HARDWARE NODE TAXONOMY                                |
+-----------------------+-----------------------+------------------+------------------+
| 1. PLC RACK           | 2. REMOTE I/O         | 3. INVERTER / VFD| 4. FIELD SENSOR  |
| (Modular Base Chassis)| (Bus Coupler + Slices)| (Motor Controller| (Smart End-Point)|
| - Rail DIN Mount      | - Header Station      | - Drive Profile  | - IO-Link Master |
| - Slot 0: Power / CPU | - 4/8/16 Slot Slices  | - Power In/Out   | - 4-20mA / HART  |
| - Slot 1..N: I/O & Comm| - Compact Footprint  | - Fieldbus Comms | - RS-485 Modbus  |
+-----------------------+-----------------------+------------------+------------------+
```

### 4.1 PLC Rack Node (`PlcRackNode.tsx`)
Represents modular PLC hardware such as Siemens S7-1500, Allen-Bradley ControlLogix, or the MythOS Micro-PLC.
- **Chassis Representation**:
  * Scalable DIN-rail graphic with fixed slot indexing (Slot 0 to Slot 11).
  * Slot 0: Power Supply (PS) & Central Processing Unit (CPU) with dual RJ45 Ethernet ports (`X1-P1`, `X1-P2`).
  * Slot 1–N: Digital Input (`DI`), Digital Output (`DO`), Analog Input (`AI`), Analog Output (`AO`), and Comm Processors (`CP-RS485`, `CP-CAN`).
- **Interactive Slots**:
  * Dragging an I/O module from the Primary Explorer catalog directly into an empty slot snaps and instantiates the module.
  * Right-clicking a slot opens the module diagnostic contextual menu.
  * Each slot renders active LED indicators: `RUN` (Green), `ERR` (Amber/Red), `MAINT` (Yellow), and active I/O channel bit indicators (8 or 16 LEDs).

### 4.2 Remote I/O Node (`RemoteIoNode.tsx`)
Represents distributed modular island systems such as Siemens ET 200SP or Phoenix Contact Axioline.
- **Header Station / Bus Coupler**:
  * Houses upstream communication ports (Dual RJ45 for PROFINET Daisy Chain or Single RS-485 DB9).
  * Displays station network address (e.g. `192.168.1.51` or `Slave #4`).
- **Slice Array**:
  * Compact horizontal strip showing individual plug-in slice modules (`8xDI`, `8xDO`, `4xRTD`, `IO-Link Master`).
  * Dynamic expandability: Slices can be dragged to reorder; address recalculations propagate instantly to the TagBus.

### 4.3 Inverter / Variable Frequency Drive Node (`InverterNode.tsx`)
Represents motor control devices such as Siemens Sinamics G120, Rockwell PowerFlex 525, or Danfoss VLT.
- **Visual Design**:
  * Industrial enclosure silhouette with heatsink ribbing.
  * Built-in dynamic mini-keypad showing output frequency (`Hz`), motor current (`A`), and DC bus voltage (`V`).
- **Communication Ports**:
  * Port `ETH1` / `ETH2` for industrial Ethernet protocols (PROFINET / Modbus TCP).
  * Port `TERM_RS485` for Modbus RTU daisy chaining.

### 4.4 Field Sensor Node (`FieldSensorNode.tsx`)
Represents instrumentation devices (pressure transmitters, flowmeters, pyrometers, photoelectric barriers).
- **Visual Design**:
  * Circular gauge or compact sensor capsule icon.
  * Live process value readout pill (e.g., `104.2 °C`, `4.82 bar`) updated in real-time via Server-Sent Events (SSE).
  * Built-in ISA-101 alarm indicator pip (Critical, Major, Minor, Normal).

---

## 5. Industrial Cable Routing & Color-Coded Protocols

### 5.1 Protocol Color Mapping
To eliminate wiring ambiguity during engineering and maintenance, MythOS strictly standardizes edge cable colors per international industrial conventions:

| Protocol Family | Edge Hex Code | DTCG Token | Stroke Width | Dash Pattern | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Ethernet / Modbus TCP / PROFINET** | `#16A34A` | `color.bus.ethernet` | `3px` | Solid `none` | Standard industrial green Cat6A/PROFINET industrial Ethernet |
| **Serial RS-485 / Modbus RTU / PROFIBUS** | `#0284C7` | `color.bus.serial` | `2.5px` | Solid `none` | Industrial blue shielded twisted pair (PROFIBUS DP / Belden 3106A) |
| **CANopen / DeviceNet** | `#CA8A04` | `color.bus.can` | `2.5px` | Dash `6,3` | Industrial amber/gold differential fieldbus |
| **SIL-3 Safety Bus / PROFIsafe** | `#DC2626` | `color.bus.safety` | `4px` | Solid `none` | High-integrity red functional safety bus (E-Stop, Light Curtains) |
| **IO-Link** | `#9333EA` | `color.bus.io-link` | `2px` | Solid `none` | Purple 3-wire point-to-point smart sensor connection |
| **Offline / Severed Link** | `#475569` | `color.bus.offline` | `2px` | Dash `4,4` | Muted gray indicating loss of carrier signal or timeout |

### 5.2 Orthogonal & Smart Bezier Cable Routing
- **Routing Engine**: Built-in A* orthogonal pathfinding router that hugs 90-degree industrial raceways, preventing diagonal wires crossing directly through hardware racks.
- **Cable Jumps / Bridges**: When two distinct bus cables intersect, an automatic semi-circular bridge arc ($r = 4\text{px}$) is rendered on the horizontal cable to maintain visual schematic clarity.
- **Interactive Flow Animation**:
  * During active communications, subtle particle dash animations (`stroke-dashoffset`) traverse along the cable in the direction of primary data flow.
  * Particle velocity is proportionally modulated by current network traffic utilization (10% to 100%).

---

## 6. Drag-and-Drop Gestures & Connection Verification Engine

### 6.1 Gesture Lifecycle
1. **Source Port Drag**:
   - Operator clicks and drags from any device communication port handle (`Handle` component).
   - Canvas enters `TopologyConnecting` state.
2. **Valid Target Highlighting**:
   - While dragging, all incompatible ports dim to opacity `0.2`.
   - Compatible ports expand with a pulsing target halo ($r = 8\text{px}$) in the protocol's signature color.
3. **Drop & Compatibility Validation**:
   - Releasing the connection triggers the `TopologyVerificationEngine`.

### 6.2 Connection Compatibility Matrix

```
+---------------------------------------------------------------------------------------------------------+
|                                    CONNECTION COMPATIBILITY MATRIX                                      |
+---------------------+-------------------+-------------------+-------------------+-----------------------+
| SOURCE / TARGET     | RJ45 Ethernet     | RS-485 Serial DB9 | IO-Link Master    | Terminal AI/AO (4-20) |
+---------------------+-------------------+-------------------+-------------------+-----------------------+
| RJ45 Ethernet       | VALID (PROFINET/  | INVALID           | INVALID           | INVALID               |
|                     | Modbus TCP)       | (Requires Gateway)| (Requires Gateway)| (Requires Modbus/IO)  |
+---------------------+-------------------+-------------------+-------------------+-----------------------+
| RS-485 Serial DB9   | INVALID           | VALID (Modbus RTU | INVALID           | INVALID               |
|                     | (Requires Gateway)| / PROFIBUS DP)    |                   |                       |
+---------------------+-------------------+-------------------+-------------------+-----------------------+
| IO-Link Port        | INVALID           | INVALID           | VALID (IO-Link    | INVALID               |
|                     |                   |                   | Sensor Link)      |                       |
+---------------------+-------------------+-------------------+-------------------+-----------------------+
| Terminal AI/AO      | INVALID           | INVALID           | INVALID           | VALID (Analog Channel)|
+---------------------+-------------------+-------------------+-------------------+-----------------------+
```

If an operator attempts an illegal connection (e.g. dragging an RS-485 serial cable directly to an RJ45 Ethernet port), the canvas displays an inline toast:
`[!] Invalid Topology Connection: Cannot bridge RS-485 Serial (Physical Layer TIA-485) directly to IEEE 802.3 Ethernet without an intermediate gateway node (e.g. MythOS Edge Gateway or Moxa MB3180).`

---

## 7. Contextual Popovers & Diagnostic Overlays

### 7.1 Port Configuration Popover (`PortConfigPopover.tsx`)
Clicking on any port handle opens a lightweight, non-blocking popover providing immediate network addressing controls:

```
+-------------------------------------------------------------+
| Port: CPU Slot 0 [X1-P1] (PROFINET / Modbus TCP)         [X]|
+-------------------------------------------------------------+
| IP Address:    [ 192 . 168 .   1 .  10 ]                    |
| Subnet Mask:   [ 255 . 255 . 255 .   0 ]                    |
| Gateway:       [ 192 . 168 .   1 .   1 ]                    |
| PROFINET Name: [ plc-rack-01.plant.local ]                  |
| Cycle Time:    [ 10 ms                 v]                   |
| Status:        (•) Link Up (100 Mbps Full Duplex)            |
+-------------------------------------------------------------+
| [ Apply Parameters ]                   [ Ping Test / Trace ]|
+-------------------------------------------------------------+
```

### 7.2 Diagnostic Live Heatmap Overlay
Operators can toggle the **Diagnostic Overlay** via the top canvas toolbar or keyboard shortcut `Ctrl+D`.
When activated:
- Cable lines change stroke color based on packet loss and error counters:
  * Green: 0% packet loss, latency $< 5\text{ms}$.
  * Yellow: $0.1\% - 2\%$ packet loss or retry rate $> 5\text{ retries/sec}$.
  * Red: $> 2\%$ packet loss, cyclic timeout, or CRC checksum errors.
- Device nodes display floating status badges indicating CPU load %, Free RAM, and Temperature.

---

## 8. TypeScript Interface Contracts

```typescript
/**
 * MythOS Network Topology Schema Definitions
 */

export type ProtocolType = 
  | 'ETHERNET_IP' 
  | 'MODBUS_TCP' 
  | 'PROFINET' 
  | 'MODBUS_RTU' 
  | 'PROFIBUS_DP' 
  | 'CANOPEN' 
  | 'IOLINK' 
  | 'SAFETY_PROFISAFE';

export interface NetworkPort {
  id: string;
  name: string; // e.g. "X1-P1", "Port A", "CH1"
  protocol: ProtocolType;
  connectorType: 'RJ45' | 'M12_D_CODED' | 'DB9' | 'TERMINAL_BLOCK' | 'M12_A_CODED';
  ipAddress?: string;
  subnetMask?: string;
  macAddress?: string;
  baudRate?: number; // For serial/CAN
  slaveId?: number;  // For Modbus RTU
  linkStatus: 'UP' | 'DOWN' | 'DEGRADED';
  rxBytesSec: number;
  txBytesSec: number;
  crcErrors: number;
}

export interface HardwareModuleSlot {
  slotNumber: number; // 0..11
  catalogNumber: string; // e.g. "MYTH-CPU-01", "MYTH-DI16-24VDC"
  name: string;
  moduleType: 'CPU' | 'DI' | 'DO' | 'AI' | 'AO' | 'COMM' | 'POWER';
  ports: NetworkPort[];
  diagnostics: {
    runLed: boolean;
    errLed: boolean;
    maintLed: boolean;
    temperatureC?: number;
  };
}

export interface HardwareRackNodeData {
  nodeId: string;
  rackName: string;
  rackType: 'PLC_RACK' | 'REMOTE_IO' | 'INVERTER' | 'FIELD_SENSOR';
  location: string;
  slots: HardwareModuleSlot[];
  isLocked: boolean;
}

export interface TopologyCableEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string;
  targetNodeId: string;
  targetPortId: string;
  protocol: ProtocolType;
  cableLengthMeters?: number;
  health: {
    status: 'OPTIMAL' | 'WARNING' | 'CRITICAL' | 'OFFLINE';
    latencyMs: number;
    packetLossPct: number;
    retriesPerSec: number;
  };
}
```

---

## 9. Visual Quality Control & Box-Model Verification
- **Pixel Density**: All nodes use crisp vector SVG symbols with 1px hairline inner borders to prevent blurring on high-DPI industrial touchscreens.
- **Color Contrast**: All port labels and node texts exceed WCAG 2.1 AA luminance ratio ($\ge 4.5:1$) against Obsidian surfaces `#0F141C` and `#161E2E`.
- **Performance Budget**: Canvas renders up to 250 connected nodes and 400 edges at a continuous 60 FPS using offscreen SVG layer caching and WebGL line instancing.
