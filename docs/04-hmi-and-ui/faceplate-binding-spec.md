# MythOS Reusable Function Block Faceplate Binding Specification

## 1. Executive Summary & Purpose
This document specifies the architecture, data binding engine, and visual rendering models for reusable industrial SCADA Faceplates in MythOS. 

In traditional SCADA architectures (WinCC, InTouch, FactoryTalk), binding an industrial graphic to PLC logic requires tediously mapping dozens of individual discrete tags (e.g., Process Value, Setpoint, Output, Auto/Manual, Run Feedback, Fault Bit, Interlock Matrix, Alarm Flags) one by one. This approach is error-prone, violates software engineering DRY principles, and results in catastrophic maintenance overhead.

MythOS introduces the **Unified UDT-to-Faceplate Binding Model**. A single IEC 61131-3 User-Defined Data Type (UDT) or Function Block (FB) instance in the PLC or TagBus is bound as a single atomic unit to a composite Faceplate component. When an engineer drags a Function Block instance (e.g., `Motor_FeedPump_01` of type `FB_Motor_VFD`) from the logic tree onto the SCADA canvas, MythOS automatically binds all telemetry, control registers, diagnostics, and alarm states with **zero manual wire binding**.

Furthermore, MythOS implements the **4-Mode Presentation Mapping Model**, harmonizing faceplate personas directly into the **4 Display Archetypes**:
1. **P&ID Vector Symbol**: ISA-101 compliant 2D schematic symbol on bounded grid.
2. **Interactive Mimic Widget**: High-density interactive control card with setpoint adjusters and actuation decks.
3. **Popup Diagnostic Modal**: Popped-out floating dialog or multi-monitor OS window with deep tuning suites.
4. **Sidebar Inspector**: Fixed 384px docked slide-over drawer with user-customizable, drag-and-drop stackable cards.

In addition, faceplates can be authored as **Reusable Nested Sub-Canvases** on bounded grids (up to 4K max bounds) incorporating vector SVGs, photographic JPEGs, and state-gated animated GIFs, and subsequently dragged and dropped into any parent screen.

---

## 2. Normative References
- **ANSI/ISA-101.01-2015**: *Human Machine Interfaces for Process Automation Systems*.
- **ANSI/ISA-18.2-2016**: *Management of Alarm Systems for the Process Industries*.
- **IEC 61131-3:2013**: *Programmable controllers — Part 3: Programming languages (Function Blocks & UDTs)*.
- **ISO 9241-110:2020**: *Ergonomics of human-system interaction — Principles of interaction*.
- **W3C Design Tokens Community Group (DTCG)**: *2025.10 Specification (Dual Theme: Dark Obsidian & Light Clean Slate)*.

---

## 3. The 4-Mode Faceplate Presentation Mapping Model & Screen Roles

In accordance with Adrian's architectural mandate, the 4 Faceplate presentation modes decouple into dedicated screen contexts and modular iOS-style widget standards:

```
+---------------------------------------------------------------------------------------------------+
|                                4-MODE PRESENTATION MAPPING MODEL & SCREEN ROLES                   |
+---------------------+---------------------+-----------------------+-------------------------------+
| 1. P&ID SYMBOL      | 2. INTERACTIVE      | 3. POPUP MODAL        | 4. SIDEBAR WIDGET DECK        |
| (Engineering Screen)| (Operator Screen)   | (Contextual Dialog)   | (iOS-Style Modular Deck)      |
+---------------------+---------------------+-----------------------+-------------------------------+
| - Target: Engineers | - Target: Operators | - Target: Diagnostics | - Target: Companion Widgets   |
| - Full P&ID schematic- High-density mimic | - Triggered on click  | - Constant 384px fixed width  |
| - ISA-101 Muted SVG | - Big Start/Stop    | - Detachable floating | - Standard S / M / L sizes    |
| - Piping & instrument - Direct SP sliders |   window / multi-mon  | - Smart Widget Stacking       |
| - Bounded 2D Grid   | - Zero clutter deck | - PID tuning & hex log| - Vertical drag-drop sort     |
+---------------------+---------------------+-----------------------+-------------------------------+
```

### 3.1 Mode 1: P&ID Vector Symbol
- **Target Context**: Level 1 (Plant Overview) and Level 2 (Unit Overview) SCADA screens.
- **Visual Design**:
  * Ultra-clean SVG vector iconography adhering strictly to ISA-101 monochrome guidelines.
  * Vessels, heat exchangers, valves, and centrifugal pumps render with slate neutral outlines (`#64748B` in Dark Mode, `#94A3B8` in Light Mode) and muted structural backgrounds (`#1E293B` dark, `#E2E8F0` light).
  * **Zero Color In Quiescent Nominal State**: An energized motor or open valve does not shine bright saturated green; it displays an elevated neutral tone or subdued cyan (`#0284C7`), preserving bright red (`#DC2626`) and orange (`#EA580C` dark / `#D97706` light) strictly for active alarms.
- **Alarm Annunciation**:
  * If the bound FB has an active alarm, a prominent 2px pulsing border and an ISA-18.2 priority symbol (Square for Critical, Triangle for Major, Inverted Triangle for Minor) renders adjacent to the symbol.
- **Interaction**: Single-click selects the device and routes details to the Zone 4 Sidebar Inspector; double-click opens the floating Mode 3 Popup Modal.

### 3.2 Mode 2: Interactive Mimic Widget
- **Target Context**: Level 2 (Unit Control) and Level 3 (Subsystem Detail) screens.
- **Visual Design**:
  * Compact card container (`240px` to `360px` width) with elevated surface (`#161E2E` dark, `#F1F5F9` light) and crisp border (`#334155` dark, `#CBD5E1` light).
  * Header displays Tag ID (`P-101A`), Equipment Description (`Slurry Feed Pump A`), and Operating Mode Badge (`AUTO` / `MANUAL` / `LOCAL`).
  * Live process values: Digital bar graph showing current capacity %, actual RPM, motor current (`A`), and bearing temperature (`°C`).
  * Primary operator action triggers: Quick Start, Stop, and Setpoint adjustment input.
- **Canvas Portability**: Designed to be authored on a bounded sub-canvas and dragged into any parent SCADA screen as a reusable nested element.

### 3.3 Mode 3: Popup Diagnostic Modal
- **Target Context**: Level 3 & Level 4 (Diagnostic, Tuning & Troubleshooting) screens, especially multi-monitor workstations.
- **Visual Design**:
  * Comprehensive deep-dive interface hosted inside a detachable floating window via React Portal (`window.open`).
  * Unconstrained by sidebar boundaries; allows simultaneous side-by-side view of real-time trending (100 Hz), full Cause-and-Effect Boolean checklist, PID loop tuning controls, and raw PLC register dumps.
  * Operators can pin multiple popup faceplates across secondary monitors while keeping the primary P&ID process canvas undisturbed.

### 3.4 Mode 4: Sidebar Inspector Drawer
- **Target Context**: Standard operational parameter review and equipment adjustment in Zone 4.
- **Ergonomic Constraint (Adrian Invariant)**:
  * **Fixed Constant Width of 384px**: Zero horizontal resizing across all plant screens, eliminating disruptive layout jumps.
  * **Modular Stackable Cards**: Card deck (Control, Interlocks, Trends, Maintenance) reorderable via `@dnd-kit` drag-and-drop.
  * **Dynamic Vertical Scroll**: Dynamically adjusts height to fit available screen space without double scrollbars.

---

## 4. Reusable Nested Sub-Canvases & Asset Importer

MythOS faceplates are not restricted to monolithic hardcoded components; they leverage the **Reusable Bounded Sub-Canvas Architecture**:

```
+----------------------------------------------------------------------------------------------------+
|                               REUSABLE FACEPLATE SUB-CANVAS ENGINE                                 |
|                                                                                                    |
|  [ SUB-CANVAS DEFINITION ]                      [ ASSET IMPORTERS ]        [ PARENT SCREEN DROP ]  |
|  Name: "DecanterCentrifuge_Faceplate"           - SVG Vector Lineart       Main Canvas (1920x1080) |
|  Grid: 48x32 cells (768x512px) @ 16px snap      - JPEG Physical Photo  ==> +---------------------+|
|  Formal Tags:                                   - Animated GIF Rotor Loop  | Skid A (Sub-Canvas) ||
|    $DRIVE, $BOWL, $VIB_SENSOR                                              | Skid B (Sub-Canvas) ||
|                                                                            +---------------------+|
+----------------------------------------------------------------------------------------------------+
```

### 4.1 Bounded Grid Canvas Authoring
- **Canvas Boundaries**:
  * Faceplate sub-canvases are created inside bounded coordinate grids (8px fine snap, 16px standard snap).
  * Full plant screens adhere to standard FHD `1920px × 1080px` or 4K ceiling `3840px × 2160px`.
  * Sub-canvases define explicit bounding boxes (e.g. `384px × 512px` or `640px × 480px`).
- **Asset Importer Support**:
  * **SVG**: Clean vector equipment paths with automatic theme token binding.
  * **JPEG**: High-resolution photorealistic cabinet backplates or field equipment photographs.
  * **GIF / WebP**: State-synchronized animation loops (e.g., rotating pump impellers, spinning centrifuges, flickering burner flames) gated by live Boolean telemetry tags (`bRunning`).

### 4.2 Hierarchical Canvas Nesting
- Once saved to the project catalog (e.g., `assets/subcanvases/Centrifuge_Unit.subcanvas.json`), the entire composite faceplate assembly can be dragged and dropped into any parent screen.
- An instance parameter mapping sheet maps the formal template variables (`$DRIVE`, `$VALVE`, `$PID`) to active TagBus paths without writing code.

---

## 5. Atomic UDT-to-Faceplate Binding Architecture

### 5.1 Zero-Manual-Wire Architecture
When an IEC 61131-3 PLC program defines a Function Block instance:

```pascal
VAR_GLOBAL
    Pump_Slurry_P101 : FB_Motor_VFD;
    Valve_Inlet_XV101 : FB_Digital_Valve;
    Loop_Reactor_TIC101 : FB_PID_Controller;
END_VAR
```

MythOS parses the UDT schema via the PLC Symbol Table / ADS / OPC-UA / Modbus Register Map. The SCADA binding engine maps the entire composite structure to the corresponding Faceplate component in a single atomic registration.

```
+----------------------------------------------------------------------------------------------------+
|                                ATOMIC UDT BINDING ARCHITECTURE                                     |
|                                                                                                    |
|   PLC IEC 61131-3 UDT INSTANCE                 MYTHOS BINDING ENGINE       MYTHOS FACEPLATE MODES  |
|   +--------------------------+                 +---------------------+     +---------------------+ |
|   | Pump_Slurry_P101         |                 | Zero-Wire Auto-Map  |     | [P&ID Symbol]       | |
|   | .rSpeedFeedback (REAL)   | ====[SHM Tag]=> | Resolves schema     | ==> | [Interactive Widget]| |
|   | .rSpeedSetpoint (REAL)   | ====[Bus Event] | Standardized names  |     | [Popup Modal]       | |
|   | .bRunning (BOOL)         |                 | Injects live state  |     | [Sidebar 384px]     | |
|   | .bTripped (BOOL)         |                 | Subscribes SSE      |     |                     | |
|   | .dwFaultCode (DWORD)     |                 |                     |     +---------------------+ |
|   | .stInterlocks (STRUCT)   |                 |                     |                             |
|   +--------------------------+                 +---------------------+                             |
+----------------------------------------------------------------------------------------------------+
```

### 5.2 Standard Canonical UDT Signatures
MythOS ships with standard, pre-certified Function Block schemas for common industrial field equipment:

#### A. `FB_Motor_VFD` (Variable Frequency Drive / Motor)
- **Inputs / Feedback**:
  * `rSpeedFeedback`: Motor speed in RPM / Hz (REAL).
  * `rCurrentAmps`: Motor phase current in Amperes (REAL).
  * `rTorquePercent`: Motor output torque percentage (REAL).
  * `bRunning`: Run feedback confirmation bit (BOOL).
  * `bTripped`: VFD overload or internal fault trip bit (BOOL).
  * `bRemoteMode`: Local/Remote control selector status (BOOL).
  * `dwFaultCode`: Numerical diagnostic alarm register (DWORD).
- **Commands / Outputs**:
  * `bStartCommand`: Operator start command (BOOL).
  * `bStopCommand`: Operator stop command (BOOL).
  * `bFaultReset`: Reset trip circuit latch (BOOL).
  * `rSpeedSetpoint`: Target speed command (REAL).
  * `bManualMode`: Auto / Manual mode command (BOOL).
- **Safety & Interlocks**:
  * `stInterlocks`: Array of Boolean permissives (e.g. `EmergencyStopOK`, `LubeOilPressureOK`, `SuctionValveOpen`).

#### B. `FB_PID_Controller` (Analog Closed-Loop Control)
- **Process Telemetry**:
  * `rPV`: Process Variable (REAL).
  * `rSP`: Active Setpoint (REAL).
  * `rCV`: Controller Output % (REAL).
  * `eMode`: Operating mode (`MANUAL`, `AUTO`, `CASCADE`).
- **Loop Tuning**:
  * `rKp`: Proportional Gain (REAL).
  * `rTn`: Integral Action Time in seconds (REAL).
  * `rTv`: Derivative Action Time in seconds (REAL).
  * `rDeadband`: Operational deadband threshold (REAL).
- **Alarm Limits**:
  * `rHighHighLimit`, `rHighLimit`, `rLowLimit`, `rLowLowLimit`.

#### C. `FB_Digital_Valve` (On/Off Isolation & Control Valve)
- **Feedback**:
  * `bOpenLimit`: Valve full-open limit switch (BOOL).
  * `bClosedLimit`: Valve full-closed limit switch (BOOL).
  * `bTransitFault`: Valve failed to reach target within travel timeout (BOOL).
- **Commands**:
  * `bOpenCommand`, `bCloseCommand`, `bInterlockBypass`.

---

## 6. TypeScript Contract & Schema Specification

```typescript
/**
 * MythOS Faceplate Binding & Multi-Mode Presentation Schema
 */

export type FaceplatePresentationMode = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';

export interface UdtFieldBinding {
  /** Property key expected by the React Faceplate component */
  componentKey: string;
  /** Relative tag member inside the PLC UDT (e.g. ".rSpeedFeedback") */
  udtMemberPath: string;
  /** Engineering unit (e.g. "RPM", "bar", "°C", "%") */
  unit?: string;
  /** Display format mask (e.g. "0.0", "0.00") */
  formatMask?: string;
  /** Directionality */
  access: 'READ_ONLY' | 'READ_WRITE';
}

export interface FaceplateArchetypeDefinition {
  /** Unique archetype identifier (e.g. "archetype.motor.vfd") */
  id: string;
  name: string;
  compatibleUdtTypes: string[]; // e.g. ["FB_Motor_VFD", "ST_Siemens_G120", "VFD_Generic"]
  description: string;
  bindings: UdtFieldBinding[];
  supportedModes: FaceplatePresentationMode[];
  subCanvasAssetRef?: string;
  defaultDimensions: {
    pid: { width: number; height: number };
    interactive: { width: number; height: number };
    popup: { width: number; height: number };
    sidebar: { width: 384; height: number }; // Constant 384px width
  };
}

export interface BoundFaceplateInstance {
  instanceId: string;
  archetypeId: string;
  tagPrefix: string; // e.g. "Plant1.Area2.P101A"
  label: string;      // e.g. "Slurry Feed Pump A"
  currentMode: FaceplatePresentationMode;
  position: { x: number; y: number };
  gridSnap: 8 | 16;
  telemetrySubscriptions: string[]; // Fully qualified Tag IDs subscribed to SSE
  writeAccessGranted: boolean;
}
```

---

## 7. Drag-and-Drop Canvas Instantiation Workflow

1. **Catalog / Explorer Selection**:
   - Operator expands the **Function Block Instances** category in Zone 2 Primary Explorer.
   - The tree displays active runtime logic instances parsed from the PLC symbol map.
2. **Canvas Drop Gesture & Mode Selector**:
   - Dragging an item (e.g., `Pump_Slurry_P101`) over Zone 3 highlights the 16px snap grid boundaries.
   - Upon release, the binding engine detects the PLC UDT type and presents a mode selector:
     `[ Render As: (•) P&ID Symbol  ( ) Interactive Mimic  ( ) Dock in Sidebar (384px)  ( ) Popped-Out Dialog ]`
3. **Automated Tag Subscription**:
   - The React component instantiates and registers a composite SSE topic:
     `/api/v1/telemetry/stream?prefix=Plant1.Area2.Pump_Slurry_P101.*`
   - All child fields begin streaming updates at 100ms cycle time with zero manual wiring.

---

## 8. Security, Interlocks & Command Authorization

To prevent unauthorized tampering on mission-critical plant equipment:
- **RBAC Command Guard**: Control buttons (Start, Stop, Setpoint Ramp, Fault Reset) are disabled unless the active operator session possesses role `OPERATOR_EXECUTE` or `COMMISSIONING_LEAD`.
- **Two-Stage Arm-and-Fire Dialog**:
  * Toggling critical actuators (e.g. Emergency Isolation Valve or Motor Jog in Manual) triggers an inline 2-second confirmation slide:
    `[ Slide to Confirm Actuation: P-101A START ]`
- **Audit Trail Logging**:
  * All parameter writes are recorded to the immutable audit historian with:
    `{ timestamp, operatorId, tagPath, previousValue, newValue, clientIp }`

---

## 9. Faceplate Library Architecture & Master Type Authoring (TIA Portal Library Pattern)

To answer the fundamental engineering question: *"Where do reusable faceplates get authored and maintained?"*, MythOS establishes the **Faceplate Library** within Zone 2 of the workbench, paired with the dedicated **Faceplate Master Designer** tab in Zone 3.

```
+───────────────────────────────────────────────────────────────────────────+
| ZONE 2: PRIMARY EXPLORER                                                  |
+───────────────────────────────────────────────────────────────────────────+
| ▼ Project: BottlingPlant_Line01                                           |
|   ├── ⚙️ Hardware Racks & Fieldbus (PROFINET)                             |
|   ├── 📜 PLC Logic Blocks (MainBottling.st)                               |
|   │                                                                       |
|   ▼ 🖥️ HMI Screens (Plant Operational Screens)                            |
|     ├── Screen_01_PlantOverview                                           |
|     └── Screen_02_BoilerSystem                                            |
|                                                                           |
|   ▼ 📦 Faceplate Library (Master Reusable Component Types)  <-- DESIGN    |
|     ├── [➕ Create New Faceplate Type]                                    |
|     ├── 📦 FP_Motor_VFD (Master Pump / VFD Motor)                         |
|     ├── 📦 FP_Pneumatic_Valve (Master Discrete Valve)                     |
|     └── 📦 FP_Analog_Transmitter (Master 4-20mA Sensor)                   |
+───────────────────────────────────────────────────────────────────────────+
```

### 9.1 Faceplate Master Designer Workspace (Zone 3 Canvas)
Double-clicking any Master Faceplate Type in the Zone 2 library opens a dedicated **Faceplate Master Designer** document tab in Zone 3.

```
+───────────────────────────────────────────────────────────────────────────────────────────────+
| Zone 3 Tab: [📦 FP_Motor_VFD (Master Designer v1.0.0)]           [💾 Save Type] [🚀 Bump v1.1.0]|
+───────────────────────────────────────────────────────────────────────────────────────────────+
| Persona Views: [ (•) P&ID Symbol View | ( ) Interactive Card | ( ) Popup Modal | ( ) Alarm Row] |
+───────────────────────────────────────────────────────────────────────────────────────────────+
| FORMAL UDT INTERFACE MATRIX (Decoupled from Concrete Tags):                                   |
| - UDT.bRunning       : BOOL   (Direction: In   | Unit: -   | Role: Run Status)                |
| - UDT.bFault         : BOOL   (Direction: In   | Unit: -   | Role: Trip Status)               |
| - UDT.rSpeedActual   : REAL   (Direction: In   | Unit: RPM | Role: Telemetry Feedback)        |
| - UDT.rSpeedSetpoint : REAL   (Direction: Out  | Unit: RPM | Role: Command Setpoint)          |
| - UDT.bStartCmd      : BOOL   (Direction: Out  | Unit: -   | Role: Start Pulse)               |
| - UDT.bStopCmd       : BOOL   (Direction: Out  | Unit: -   | Role: Stop Pulse)                |
+───────────────────────────────────────────────────────────────────────────────────────────────+
| PERSONA CANVAS: (P&ID Vector Symbol Editor - 16px Grid Snap)                                  |
|                                                                                               |
|       [ SVG Motor Circle: dynamic fill = UDT.bFault ? Red : UDT.bRunning ? Cyan : Slate ]     |
|       [ Text Label: {UDT.rSpeedActual} RPM ]                                                  |
|                                                                                               |
+───────────────────────────────────────────────────────────────────────────────────────────────+
```

### 9.2 Decoupled Formal UDT Interface Authoring
Master Faceplates are never bound to concrete physical addresses (e.g. `%QX0.0` or `Line1.Pump01`) during type authoring. Instead, they bind exclusively to **Formal UDT Interface Properties**:
- `UDT.<propertyName>` represents an abstract typed slot.
- Types conform to standard IEC 61131-3 data types: `BOOL`, `REAL`, `INT`, `DINT`, `DWORD`, `STRING`, or nested `STRUCT`.
- Directionality is enforced at authoring time:
  * `INPUT` (Read-only process telemetry from controller).
  * `OUTPUT` (Command registers dispatched to controller).
  * `IN_OUT` (Bidirectional parameters, such as recipe setpoints or tuning gains).

### 9.3 Multi-Persona Canvas Tabs
The Faceplate Master Designer exposes 4 dedicated authoring tabs corresponding to the 4 standardized industrial display archetypes:
1. **[ P&ID Symbol View ]**: ISA-101 compliant 2D vector schematic icon (64px x 64px default).
2. **[ Interactive Card View ]**: Modular mimic control card with Start/Stop buttons and Setpoint sliders (320px x 240px default).
3. **[ Diagnostic Popup View ]**: Contextual tuning modal featuring live PID trend curves, interlock permissives matrix, and raw hexadecimal fault registers (640px x 480px default).
4. **[ Alarm Row View ]**: High-density table annunciator row rendering status pill, acknowledgment button, and trip timestamps (384px x 48px).

---

## 10. Instantiation & Global Version Propagation Engine (TIA Portal Library Pattern)

### 10.1 Screen Drag-and-Drop & Instant UDT Instance Binding
When an automation engineer constructs an operational plant screen (e.g. `Screen_02_BoilerSystem`):
1. The engineer opens `Screen_02_BoilerSystem` in Zone 3 drafting canvas.
2. The engineer drags `FP_Motor_VFD` from the Faceplate Library into the canvas grid.
3. MythOS displays the **Target Instance Assignment Dialog**:
   ```
   +─────────────────────────────────────────────────────────────────+
   | ASSIGN TARGET PLC FUNCTION BLOCK / UDT INSTANCE                  |
   +─────────────────────────────────────────────────────────────────+
   | Master Faceplate Type: FP_Motor_VFD (v1.0.0)                    |
   | Compatible PLC Types : FB_Motor_VFD, ST_Siemens_G120            |
   |                                                                 |
   | Select Active Instance:                                         |
   | [ Line1.Boiler.FeedWater_Pump_01                       ▼ ]      |
   +─────────────────────────────────────────────────────────────────+
   | Automated Bindings Preview:                                     |
   |  - UDT.bRunning       ──► Line1.Boiler.FeedWater_Pump_01.bRunning |
   |  - UDT.bFault         ──► Line1.Boiler.FeedWater_Pump_01.bFault   |
   |  - UDT.rSpeedActual   ──► Line1.Boiler.FeedWater_Pump_01.rSpeed   |
   |  - UDT.rSpeedSetpoint ──► Line1.Boiler.FeedWater_Pump_01.rSP      |
   +─────────────────────────────────────────────────────────────────+
   |                                          [ Cancel ] [ Confirm ] |
   +─────────────────────────────────────────────────────────────────+
   ```
4. Upon clicking `[ Confirm ]`, all 4 presentation modes (P&ID, Interactive, Popup, Sidebar) bind atomically to `Line1.Boiler.FeedWater_Pump_01` without manual wiring.

### 10.2 Global Version Propagation Engine
In large industrial facilities, a single Faceplate Master Type may be instantiated hundreds of times across dozens of plant screens. Updating graphics or adding a diagnostic field must not require manual re-editing of each screen.

MythOS implements the **TIA Portal Library Propagation Pattern**:
1. **Authoring Update**: The lead automation engineer modifies `FP_Motor_VFD` in the Faceplate Master Designer (e.g. adding an emergency stop permissive LED and optimizing the high-temperature alarm threshold).
2. **Version Bump Trigger**: The engineer clicks **[🚀 Bump v1.1.0 & Propagate]**.
3. **Impact Analysis & Pre-Flight Validation**:
   - The backend engine scans all screen schemas stored in `screens/draft/*.json`.
   - It identifies all instances linked to `FP_Motor_VFD` with version `< v1.1.0`.
   - It generates an impact summary:
     `"Detected 28 instances across 4 screens: Screen_01_PlantOverview (8), Screen_02_BoilerSystem (12), Screen_03_OperatorDeck (8)."`
4. **Atomic In-Memory & Draft WAL Update**:
   - The engine upgrades the component layout schemas to `v1.1.0`.
   - **Preserved Instance Attributes**: Individual canvas coordinates `(x, y)`, screen-specific device labels (`"Feedwater Pump 101A"`), and target PLC instance bindings (`Line1.Boiler.FeedWater_Pump_01`) remain 100% untouched.
   - **Propagated Attributes**: Updated SVG paths, color token thresholds, new formal UDT member bindings, and button styles propagate instantaneously.
5. **Staging Buffer Isolation**: All updates apply strictly to `screens/draft/`. Active production screens (`screens/active/`) continue executing unmodified until the draft buffer is verified in **Preview Mode** and committed via **Deploy to Runtime HMI**.
