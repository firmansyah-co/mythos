# MythOS Workspace Task Cards & Context-Sensitive Inspector Architecture

- **Document Identifier**: MYTHOS-HMI-002
- **Version**: 3.0.0
- **Author**: Iris (VP Design & Visual Architecture) & Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Standard Reference**: Siemens TIA Portal Parity (SYS-REQ-097), ANSI/ISA-101.01, ISO 9241-110, W3C DTCG
- **Language**: ASD-STE100 Technical English

---

## 1. Executive Summary & Purpose

This document specifies the architectural, visual, and behavioral standards for the MythOS **Far-Right Collapsible Task Cards (Zone 4)**, the **Context-Sensitive Bottom Properties & Inspector (Zone 5)**, and the **Multi-Mode View Architecture** across the unified 5-Zone Industrial SCADA Workbench Shell.

Directly mirroring canonical **Siemens TIA Portal** engineering paradigms:
1. **Zone 4 (Far-Right Dock)** provides collapsible vertical **Task Cards** (`Instructions`, `Hardware Catalog`, `Libraries`, `Layout & Widgets`) for rapid, searchable asset palette extraction into the canvas.
2. **Zone 5 (Bottom Dock)** provides the **Context-Sensitive Bottom Properties & Inspector** tab deck (`General`, `IO Addresses / Variables`, `Diagnostics`, `Alarms`), automatically dynamically synchronized to whichever logic block, hardware module, network cable, or graphic element is currently selected in Zone 3.
3. Equipment control and diagnostics bridge seamlessly into **4 Distinct Display Archetypes** (P&ID Mode, Interactive Mode, Popup Mode, and Sidebar Mode) with strict constant-width ergonomics (**384px fixed width**, 2-column modular grid).

---

## 2. Normative References & Box-Model Constraints

- **ANSI/ISA-101.01-2015**: *Human Machine Interfaces for Process Automation Systems*.
- **ISO 9241-110:2020**: *Ergonomics of human-system interaction — Principles of interaction*.
- **Siemens TIA Portal Parity (SYS-REQ-097)**: Workspace layout, task card tabs, and bottom properties parity.
- **W3C Design Tokens Community Group (DTCG)**: Token syntax and design primitives.

### Box-Model Constraints
1. **Zero Double Scrollbars**: The root shell hosts `overflow: hidden`. Every panel enforces `flex: 1 1 0%`, `min-h: 0`, and explicit scroll containment (`overflow-y: auto`, `overflow-x: hidden`).
2. **Deterministic Spatial Dimensions**:
   - Zone 1 Top HUD: Exactly **`38px`** height (`h-[38px]`).
   - Zone 1.1 Status Bar: Exactly **`24px`** height (`h-[24px]`).
   - Zone 2 Primary Explorer: Resizable **`240px - 360px`** width.
   - Zone 4 Far-Right Task Cards: Exactly **`384px`** fixed width (`w-[384px] min-w-[384px] max-w-[384px]`).
   - Zone 5 Bottom Properties & Inspector: Collapsible **`180px - 320px`** height.

---

## 3. 5-Zone Workbench Layout Architecture

The MythOS Workbench Shell arranges the display zones using high-performance Flexbox and `@hexact/dockview`:

```
+-------------------------------------------------------------------------------------------------------------------+
| ZONE 1: TOP INDUSTRIAL HUD BANNER (38px Fixed Height)                                                             |
| [SYS: ONLINE]  [NODE: lycan-edge-01 (192.168.1.50)]  [CYCLE: 10ms]  [ACTIVE ALARMS: 0 CRIT | 2 WARN]           [🔒]|
+----+----------------------------------+------------------------------------+--------------------------------------+
| Z  | ZONE 2: PRIMARY EXPLORER        | ZONE 3: MAIN DOCKVIEW CANVAS       | ZONE 4: FAR-RIGHT TASK CARDS         |
| O  | (240px - 360px Resizable)        | (100% Flex Multi-Tab Viewport)     | (Fixed 384px Constant Width)         |
| N  | ▼ Project: BottlingPlant         |                                    | +----------------------------------+ |
| E  |   ▼ Devices & Networks           | +--------------------------------+ | | TABS: [Instr | Cat | Lib | Lay]  | |
|    |     ├── Network View             | | Tab 1: P&ID  | Tab 2: Devices  | | +----------------------------------+ |
| 5  |     ├── Device View              | +--------------------------------+ | | Active Card: [📦 Hardware Catalog]| |
| :  |     └── Topology View            | |                                | |   ▼ Siemens S7-1500 / ET200SP      | |
| A  |   ▼ Controller [Soft-Logic]      | |  Bounded Process Canvas        | |     ├── CPU 1515-2 PN            | |
| C  |   ▼ Visualization [Web HMI]      | |  (Grid Snap: 8px / 16px)       | |     ├── DI 16x24VDC HF           | |
| T  |     ├── HMI Screens              | |  [Imported SVG / JPEG / P&ID]  | |     └── DQ 16x24VDC HF           | |
| I  |     └── Faceplate Types          | |                                | | [≡] Drag module into rack/canvas | |
| V  +----------------------------------+------------------------------------+--------------------------------------+
| I  | ZONE 5: CONTEXT-SENSITIVE BOTTOM PROPERTIES & INSPECTOR (180px - 320px Collapsible)                          |
| T  | [Tabs: 📋 General | 🏷️ IO Addresses / Variables | 🩺 Diagnostics | 🚨 Alarms | 💻 Compiler Output]           |
| Y  | Selection: Rack_0 / Slot_2 [DI 16x24VDC HF] -> Start Address: %I10.0 ... %I11.7                             |
| (48+--------------------------------------------------------------------------------------------------------------+
| px)| ZONE 1.1: STATUS BAR (24px Fixed Height)                                                                     |
|    | [Gateway: RUNNING] [Memory: 42MB] [TagBus: 14,200 tags] [MQTT: CONNECTED] [Time: 2026-10-14 09:00:00 UTC]    |
+----+--------------------------------------------------------------------------------------------------------------+
```

---

## 4. Zone 4 Far-Right Collapsible Task Cards (TIA Portal Parity)

Zone 4 occupies the right dock of the workbench shell, constrained to a strict constant width of **384px**. It features 4 vertical tabs mirroring Siemens TIA Portal Task Cards:

```
+--------------------------------------------------------------------+
| ZONE 4: TASK CARDS                                          [⤢] [X]|
| [ ⚡ Instructions ] [ 📦 HW Catalog ] [ 📚 Libraries ] [ 🎨 Layout ]|
+--------------------------------------------------------------------+
```

### 4.1 Task Card 1: `Instructions` (`InstructionsTaskCard.tsx`)
Hosts standard IEC 61131-3 logic operations and domain-specific algorithm blocks for drag-and-drop insertion into Ladder diagrams or Function Block Diagrams:
- **Bit Logic Operations**: Normally Open Contact (`--| |--`), Normally Closed Contact (`--|/|--`), Output Coil (`--( )--`), Set/Reset Coils, Positive/Negative Edge Evaluators (`P_TRIG`, `N_TRIG`).
- **Timers**: Pulse Timer (`TP`), On-Delay Timer (`TON`), Off-Delay Timer (`TOF`), Time Accumulator (`TONR`).
- **Counters**: Up-Counter (`CTU`), Down-Counter (`CTD`), Up-Down Counter (`CTUD`).
- **Math & Comparators**: `ADD`, `SUB`, `MUL`, `DIV`, `CMP ==`, `CMP >=`, `MIN`, `MAX`, `LIMIT`.
- **Conversion & Movement**: `MOVE`, `CONVERT`, `SCALE_X`, `NORM_X`.
- **Custom Function Blocks**: User-authored FBs extracted from the active project.

### 4.2 Task Card 2: `Hardware Catalog` (`HardwareCatalogTaskCard.tsx`)
Hosts a comprehensive, searchable tree of industrial automation hardware modules:
- **Search & Filter**: Search by article number (e.g. `6ES7521-1BH00-0AB0`), device category, or vendor name.
- **Hierarchical Taxonomy**:
  * `Controllers`: Modular PLCs (Siemens S7-1500, S7-1200, Beckhoff CX, MythOS Soft-PLC).
  * `Distributed I/O`: Bus couplers and slice families (ET 200SP, ET 200MP, Axioline, WAGO 750).
  * `Drives & Starters`: Sinamics G120, V20, Rockwell PowerFlex, Soft Starters.
  * `Field Devices & Sensors`: IO-Link Masters, RTD Transmitters, Pressure Sensors.
  * `Network Infrastructure`: Industrial managed switches (Scalance XC208, Moxa EDS), Fieldbus gateways.
- **Drag-and-Drop Targets**: Dragging an item from the catalog directly into Zone 3 snaps into valid slots in **Device View** or creates interconnected stations in **Network View**.

### 4.3 Task Card 3: `Libraries` (`LibrariesTaskCard.tsx`)
Mirrors the dual-library structure of TIA Portal:
- **Project Library**:
  * `Master Copies`: Project-specific graphic blocks, sub-canvases, and pre-configured hardware templates.
  * `Types`: Formally versioned Faceplate Types (`FP_Motor_VFD`, `FP_Valve_Pneumatic`) and Project UDTs.
- **Global Libraries**:
  * Vendor read-only libraries, corporate corporate design standard libraries, and ISA-101 compliant component sets.
  * Version comparison and type updating utilities (`Update Library Types`).

### 4.4 Task Card 4: `Layout & Widgets` / Sidebar Inspector (`LayoutWidgetsTaskCard.tsx`)
Hosts the iOS-style modular widget deck, P&ID graphic symbol library, and equipment properties for active screen authoring:
- **P&ID Vector Symbols**: ISA-5.1 compliant pumps, valves, compressors, heat exchangers, and tanks.
- **Modular Widget Palette**: Standard S/M/L widgets for live control and monitoring decks.
- **Property Inspector Deck**: When authoring screens, displays layout dimensions, transform coordinates, and visual styling properties.

---

## 5. Zone 5 Context-Sensitive Bottom Properties & Inspector (TIA Portal Parity)

Zone 5 occupies the bottom collapsible dock (180px - 320px). It continuously tracks the active selection in Zone 3 and displays detailed properties across 4 standardized tabs:

```
+----------------------------------------------------------------------------------------------------+
| ZONE 5: BOTTOM PROPERTIES & INSPECTOR                                                    [─] [⤢] [X]|
| [ 📋 General ]  [ 🏷️ IO Addresses / Variables ]  [ 🩺 Diagnostics ]  [ 🚨 Alarms ]  [ 💻 Console ]  |
+----------------------------------------------------------------------------------------------------+
```

### 5.1 Tab 1: `General` (`PropertiesGeneralTab.tsx`)
Displays metadata, naming, and engineering identification:
- **Identification**: Name, Author, Version, Comment, Location in plant hierarchy (ISA-95 Area/Unit).
- **Module / Block Type**: Exact catalog article number, firmware version, execution cycle assignment (OB1 Cyclic, OB35 Periodic Interrupt 100ms).
- **Documentation**: Multilingual description strings, operator manual links, and change log history.

### 5.2 Tab 2: `IO Addresses / Variables` (`PropertiesIoAddressesTab.tsx`)
Displays address boundaries, process image partitions, and variable bindings:
- **Hardware Address Boundaries**: Start address and end address (e.g. `%I10.0` to `%I11.7`, `%Q20.0` to `%Q21.7`).
- **Process Image Organization**: Organization block assignment, cyclic update vs direct peripheral addressing (`%IW20:P`).
- **Channel Tag Binding Matrix**: Channel-by-channel table mapping physical terminals to semantic system tags (`external.feed_pump_run_fdbk`), channel inversion, wire break detection, and 4-20mA scaling limits ($0.0 - 100.0\%$).

### 5.3 Tab 3: `Diagnostics` (`PropertiesDiagnosticsTab.tsx`)
Provides real-time online diagnostics and device health:
- **Hardware Status**: Online RUN/STOP state, power supply rail voltage ($24.1	ext{V}$), module temperature ($41.5^\circ	ext{C}$).
- **Communication Quality**: Cycle time jitter, CRC packet error counters, dropped frames, buffer fill percentage.
- **Diagnostic Buffer**: Standardized ring buffer recording timestamped events (e.g. `Event ID 16# 02:4421: Wire break channel 3`, `Event ID 16# 02:3952: PROFINET station disconnected`).

### 5.4 Tab 4: `Alarms` (`PropertiesAlarmsTab.tsx`)
Displays ISA-18.2 alarm configurations and contextual alarm state:
- **Active Alarms Banner**: Displays active unacknowledged and acknowledged alarms bound to the selected device.
- **Threshold Parameterization**: High-High (HH), High (H), Low (L), Low-Low (LL) trip setpoints and deadband tolerances.
- **Sequence-of-Events (SOE) Log**: Millisecond-accurate timestamped alarm state transitions for root-cause analysis.

---

## 6. The 4 Display Archetypes & Screen-Role Mapping

To prevent cognitive clutter and support varied operational personas, MythOS decouples presentation into **4 Distinct Display Archetypes**:

```
+-------------------------------------------------------------------------------------------------------+
|                                  THE 4 DISPLAY ARCHETYPES & SCREEN ROLES                              |
+---------------------+-------------------------+---------------------------+---------------------------+
| 1. P&ID MODE        | 2. INTERACTIVE MODE     | 3. POPUP MODE             | 4. SIDEBAR MODE           |
| (Engineering Screen)| (Operator Screen)       | (Diagnostic Modal Window) | (Modular Widget Deck)     |
+---------------------+-------------------------+---------------------------+---------------------------+
| Target: Engineering | Target: Daily Operator  | Target: Contextual Action | Target: Reusable Widgets  |
| - Schematic P&ID    | - High-density mimic    | - Triggered by clicking   | - iOS-style standard cards|
| - Piping & valves   | - Big tactile buttons   |   P&ID symbol or          | - Constant 384px width    |
| - Complete loops    | - Auto/Manual toggles   |   Interactive setting     | - Standard sizes (S/M/L)  |
| - Bounded 2D Grid   | - Setpoint sliders      | - Floating modal or       | - Stackable & scrollable  |
| - ISA-101 Monochrome| - Zero visual clutter   |   multi-monitor window    | - User drag-drop arrange  |
+---------------------+-------------------------+---------------------------+---------------------------+
```

### 6.1 Archetype 1: P&ID Mode (Dedicated Engineering Screen)
- **Primary Host**: Engineering Screens (Level 1 Plant Overview & Level 2 Detailed Process Schematics).
- **Presentation**: Complete mechanical and instrumentation topology on a bounded grid canvas adhering to ISA-101 neutral styling. Single-click opens Zone 4 Sidebar Widget Deck; double-click launches Archetype 3 (Popup Mode).

### 6.2 Archetype 2: Interactive Mode (Dedicated Operator Screen)
- **Primary Host**: Day-to-day production floor terminals and panel PCs.
- **Presentation**: High-contrast mimic cards, large tactile pushbuttons (`START`, `STOP`), setpoint sliders, and monospaced readouts (`JetBrains Mono` 28px).

### 6.3 Archetype 3: Popup Mode (Contextual Diagnostic Window)
- **Primary Trigger**: Clicking an instrument in P&ID Mode or clicking the `[⚙ Settings]` trigger on an Interactive Mimic Card.
- **Presentation**: Detachable floating modal dialog or popped-out OS window via React Portals, engineered for multi-monitor setups and deep PID tuning.

### 6.4 Archetype 4: Sidebar Mode (iOS-Style Modular Widget Deck)
- **Ergonomic Constraint (Adrian Constant-Width Rule)**:
  * Strict constant width of **`384px`** (`width: 384px; min-width: 384px; max-width: 384px; flex: 0 0 384px`).
- **iOS-Style Modular Grid System (2-Column Standard)**:
  Partitioned into two 184px sub-columns with a 16px central gutter:
  * **`Size S-Half` (`184px × 96px`)**: Single dial, RUN/TRIP pill, quick toggle switch.
  * **`Size S-Full` (`384px × 96px`)**: Full-width KPI strip, odometer totalizer.
  * **`Size M-Half` (`184px × 192px`)**: Slim vertical tank level bar.
  * **`Size M-Full` (`384px × 192px`)**: Tactile actuator control deck, Sparkline trend.
  * **`Size L-Full` (`384px × 384px`)**: Cause-and-Effect Boolean Interlock Matrix, PID tuning suite.

---

## 7. Bounded Grid Canvas Engine

### 7.1 Spatial Grid Limits & Snapping
- **Bounded Resolutions**: Full HD ($1920 	imes 1080$), QHD ($2560 	imes 1440$), 4K UHD ($3840 	imes 2160$).
- **Grid Snapping**: Deterministic snap-to-grid at 8px and 16px intervals.

### 7.2 Constant Grid Matrix Resolution Independence
The canvas uses an offscreen virtual coordinate system normalized to design-time viewport units, scaling dynamically via CSS transforms while preserving crisp 1px borders.

---

## 8. React Component Hierarchy & TypeScript Contracts

### 8.1 Component Directory Structure
```
src/components/workbench/
├── WorkbenchShell.tsx                  # Root 5-zone flexbox container
├── Zone1TopHud/
│   ├── TopHudBanner.tsx                # Fixed 38px HUD header
│   └── TopHudAlarms.tsx
├── Zone2PrimaryExplorer/
│   ├── PrimaryExplorer.tsx             # 240px-360px resizable tree
│   ├── ScreenHierarchyTree.tsx         # HMI screens & sub-screens tree
│   └── DevicesNetworksTree.tsx         # Hardware views (Network, Device, Topology)
├── Zone3MainCanvas/
│   ├── MainTabbedCanvas.tsx            # Multi-document tab host (Dockview)
│   ├── BoundedGridCanvas.tsx           # Bounded grid engine
│   ├── ProcessPidCanvas.tsx            # Interactive P&ID schematic
│   └── DevicesNetworksCanvas.tsx       # 3 Hardware views canvas host
├── Zone4TaskCards/
│   ├── TaskCardsDock.tsx               # Fixed 384px constant-width right dock
│   ├── TaskCardsTabBar.tsx             # Vertical/horizontal task cards switcher
│   ├── InstructionsTaskCard.tsx        # Standard IEC 61131-3 logic operations
│   ├── HardwareCatalogTaskCard.tsx     # OEM modules, drives, sensors catalog
│   ├── LibrariesTaskCard.tsx           # Project Library & Global Libraries
│   ├── LayoutWidgetsTaskCard.tsx       # Modular widget deck & P&ID symbol palette
│   └── cards/                          # Modular widget deck cards
│       ├── ControlDeckCard.tsx
│       ├── InterlockMatrixCard.tsx
│       ├── MiniTrendCard.tsx
│       └── SimulationDeckCard.tsx
├── Zone5BottomProperties/
│   ├── BottomPropertiesInspector.tsx   # 180px-320px collapsible bottom dock
│   ├── PropertiesTabBar.tsx            # General, IO, Diagnostics, Alarms tabs
│   ├── PropertiesGeneralTab.tsx        # Metadata, author, naming, comments
│   ├── PropertiesIoAddressesTab.tsx    # Address bounds, process image, channels
│   ├── PropertiesDiagnosticsTab.tsx    # Online health, CRC counters, buffer
│   └── PropertiesAlarmsTab.tsx         # ISA-18.2 thresholds & active alarms
└── Zone1StatusBar/
    └── WorkbenchStatusBar.tsx          # Fixed 24px industrial status bar
```

### 8.2 Zustand Store Architecture (`workbenchStore.ts` & `inspectorStore.ts`)

```typescript
import { create } from 'zustand';

export type DisplayArchetype = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';
export type TaskCardTab = 'INSTRUCTIONS' | 'HARDWARE_CATALOG' | 'LIBRARIES' | 'LAYOUT_WIDGETS';
export type InspectorBottomTab = 'GENERAL' | 'IO_ADDRESSES' | 'DIAGNOSTICS' | 'ALARMS' | 'CONSOLE';

export interface InspectorState {
  // Zone 4 Task Cards
  isTaskCardOpen: boolean;
  activeTaskCard: TaskCardTab;
  readonly fixedSidebarWidth: 384;
  setActiveTaskCard: (tab: TaskCardTab) => void;
  toggleTaskCard: () => void;

  // Zone 5 Context-Sensitive Bottom Inspector
  isBottomInspectorOpen: boolean;
  activeBottomTab: InspectorBottomTab;
  selectedElementId: string | null;
  selectedElementType: 'DEVICE' | 'SLOT' | 'PORT' | 'CABLE' | 'BLOCK' | 'WIDGET' | null;
  setActiveBottomTab: (tab: InspectorBottomTab) => void;
  selectElement: (id: string | null, type?: InspectorState['selectedElementType']) => void;

  // Multi-Mode Display Archetypes
  activeArchetype: DisplayArchetype;
  setDisplayArchetype: (mode: DisplayArchetype) => void;
}
```

---

## 9. Visual Quality Control & Box-Model Verification

- **Spatial Invariants**: Strictly adheres to Zone 1 (38px), Zone 1.1 (24px), Zone 4 (384px), Zone 5 (180px-320px).
- **Accessibility & Contrast**: Conforms to ISO 9241-110 and WCAG 2.1 AA ($\ge 4.5:1$ contrast against Obsidian dark palette `#0A0D12` / `#161E2E`).
- **Responsive Overflow**: All docks enforce explicit scroll containment; zero horizontal scroll leak across viewport transitions.


---

## 7. Bounded Grid Canvas Engine

### 7.1 Spatial Grid Limits & Snapping
- **Bounded Resolutions**: Full HD (1920 x 1080), QHD (2560 x 1440), 4K UHD (3840 x 2160).
- **Grid Snapping**: Deterministic snap-to-grid at 8px and 16px intervals.

### 7.2 Constant Grid Matrix Resolution Independence (Multi-Device Viewport Scaling)
The canvas operates on an absolute internal logical resolution independent of client physical monitor pixels. It uses an offscreen virtual coordinate system normalized to design-time viewport units, scaling dynamically via CSS transforms while preserving crisp 1px borders.

---

## 8. Industrial Multi-Format Asset Importer

Industrial control screens require diverse visual assets, ranging from engineering vector CAD diagrams to photographic facility overviews and dynamic physical equipment states. MythOS includes a native **Multi-Format Asset Importer**:

```
+----------------------------------------------------------------------------------------------------+
|                                INDUSTRIAL ASSET IMPORTER PIPELINE                                  |
|                                                                                                    |
|  [ SVG Vector ]       ===> Clean DOM Node Insertion ==> Dynamic CSS Class Injections (ISA Alarms)  |
|  [ JPEG Photo ]       ===> Raster Background Plane  ===> Non-Interactive High-DPI Facility Layout  |
|  [ Animated GIF/WEBP] ===> State-Synchronized Loop  ===> Motor Rotation / Mixer / Burner Dynamics  |
+----------------------------------------------------------------------------------------------------+
```

### 8.1 Vector Graphics (SVG)
- **Primary Use**: Equipment symbols (pumps, valves, heat exchangers, instrumentation bubbles) and process flow lines.
- **Engine Processing**:
  * Automatic XML sanitization removing script tags and malicious payloads.
  * Preservation of SVG `viewBox` attributes for lossless infinite scaling.
  * CSS Variable Injection: SVG stroke and fill attributes are mapped directly to W3C DTCG color tokens (`fill="var(--color-isa101-process-vessel)"`).
  * Dynamic Alarm Binding: Injects animated SVG pulse halos when bound alarm tags trip.

### 8.2 Raster Photographic Backdrops (JPEG)
- **Primary Use**: Architectural plant layouts, aerial drone photos of tank farms, satellite maps, and physical control cabinet backplates.
- **Engine Processing**:
  * Renders on the base canvas layer below the 16px interactive grid.
  * Optimized image buffering with progressive WebP/JPEG decoding.
  * Configurable canvas opacity (`20%` to `100%`) ensuring engineering overlays and piping schematics remain visually distinct over photorealistic backgrounds.

### 8.3 Animated Equipment State Decoders (GIF / WebP)
- **Primary Use**: Intuitive physical state animations (rotating impeller on centrifugal pumps, agitator blades in slurry tanks, conveyor belt movement, active burner flame loops).
- **Engine Processing**:
  * Frame-rate throttling to prevent browser GPU overload (synchronized to plant display refresh rates).
  * **Tag-Driven State Gating**:
    * When `Pump.bRunning == TRUE`: Animated GIF loop plays.
    * When `Pump.bRunning == FALSE`: Engine freezes frame on static neutral index 0 (`pipe-inactive` tone).
    * When `Pump.bTripped == TRUE`: Engine overlays pulsing red ISA alarm glyph (`#DC2626`).

---

## 9. Hierarchical Reusable Sub-Canvases (Nesting & Drag-and-Drop)

A core innovation in MythOS is **Canvas Nesting**. Any collection of symbols, piping, logic bindings, and interactive cards can be authored on a bounded canvas and saved as a **Named Reusable Sub-Canvas**:

```
+----------------------------------------------------------------------------------------------------+
|                             HIERARCHICAL SUB-CANVAS NESTING ENGINE                                 |
|                                                                                                    |
|  1. AUTHORING (Sub-Canvas Editor)             2. PROJECT ASSET CATALOG      3. PARENT CANVAS DROP  |
|  +------------------------------+             +-----------------------+     +--------------------+ |
|  | Sub-Canvas: [Chamber01_Skid] |             | Named Sub-Canvases:   |     | Main Process Screen| |
|  | - Feed Pump (VFD)            | ===SAVE===> | - Chamber01_Skid      | ==> | +----------------+ | |
|  | - Inlet Valve (XV-101)       |             | - Decanter_Centrifuge |     | |Chamber01_Skid| | |
|  | - Flow Meter (FT-101)        |             | - Boiler_Loop_Sub     |     | | (Nested Unit)  | | |
|  | - Local Interlock Checklist  |             |                       |     | +----------------+ | |
|  +------------------------------+             +-----------------------+     +--------------------+ |
+----------------------------------------------------------------------------------------------------+
```

### 9.1 Nesting Architecture & Encapsulation
- **Scoped Tag Aliasing**: A Sub-Canvas defines formal input parameters (e.g. `$PUMP_TAG`, `$VALVE_TAG`, `$AREA_ID`). When dragged onto a parent canvas, an **Instance Parameter Dialog** binds the alias to concrete TagBus paths.
- **Composition Levels**:
  * Level 0 (Primitive): Single vector symbol or basic slider widget.
  * Level 1 (Sub-Canvas Assembly): Motor skid with isolation valves, bypass line, and differential pressure transmitter.
  * Level 2 (Parent Unit Canvas): Evaporator unit containing 4 nested Sub-Canvas assemblies.
  * Level 3 (Plant Area Canvas): Multi-screen plant overview hosting nested unit screens.
- **Portability**: All sub-canvases are serialized into standardized JSON structures stored in `assets/subcanvases/<name>.subcanvas.json`.

---

## 10. Drag-and-Drop Widget Registry & Layout Customization

The user-customizable presentation engine uses `@dnd-kit` to allow operators and engineers to reorganize interface widgets effortlessly.

```
+----------------------------------------------------------------------------------------------------+
|                               DRAG-AND-DROP WIDGET REGISTRY ARCHITECTURE                           |
|                                                                                                    |
|  AVAILABLE WIDGET PALETTE               DROP TARGET CONTAINER (Sidebar / Canvas / Popup)           |
|  +----------------------------+         +--------------------------------------------------------+ |
|  | [::] Actuator Control Deck | ======> | [≡] Interlock Matrix Checklist Card        [X] [^] [v] | |
|  | [::] Interlock Matrix Card |         +--------------------------------------------------------+ |
|  | [::] Mini-Trend Sparkline  | ======> | [≡] Actuator Control Deck (Start/Stop/Jog) [X] [^] [v] | |
|  | [::] PID Tuning Workspace  |         +--------------------------------------------------------+ |
|  | [::] Diagnostic Fault Log  | ======> | [≡] Mini-Trend Sparkline (PV/SP/CV 1m)     [X] [^] [v] | |
|  | [::] Asset Odometer Badge  |         +--------------------------------------------------------+ |
|  +----------------------------+         | + Drag new widget from palette here...                 | |
|                                         +--------------------------------------------------------+ |
+----------------------------------------------------------------------------------------------------+
```

### 10.1 The Standard Modular Widget Library
1. **Actuator Control Deck Card (`widget.control-deck`)**: Operating Mode Selector (`AUTO`, `MANUAL`, `LOCAL`), large digital readout (`JetBrains Mono` 28px) with units, tactile pushbuttons.
2. **Interlock Matrix Checklist Card (`widget.interlock-matrix`)**: Cause-and-Effect Boolean validation checklist with First-Out Trip latch highlighter.
3. **High-Resolution Mini-Trend Card (`widget.mini-trend`)**: Real-time Canvas/WebGL sparkline with selectable time windows, PV/SP/CV channels, and danger envelopes.
4. **Maintenance & PID Tuning Card (`widget.maintenance-tuning`)**: Runtime odometer, PID loop tuning ($K_p$, $T_i$, $T_d$), and bypass lock.
5. **Raw Register & Diagnostics Log Card (`widget.diagnostics-log`)**: Hexadecimal PLC memory viewer and communication packet counters.
6. **Simulation & Tag Injection Deck Card (`widget.simulation-deck`)**:
   - **Explicit Per-Device Operational Mode Toggle**: Segmented switch: `[ 🔌 Physical Hardware ]` (Production Default) vs `[ 🧪 Simulation / Manual Test ]` (Staging). Requires authenticated confirmation and logs to audit trail.
   - **High-Visibility Safety Stamping**: Pulsing amber indicator badge `[🧪 SIMULATION ACTIVE]` (`#D97706` / `#F59E0B`) stamped on Inspector drawer header, Network View node, Tag Browser, and SCADA faceplates.
   - **Quick Input Slider & Variable Forcing Interface**: Continuous slider (`0.0` to `100.0%`) injecting into TagBus feedback slot, direct numeric forcing box, and OPC DA quality `UNCERTAIN_SUBSTITUTED` (`0x0044`).
   - **Physical Hardware Safety Lockout**: In `PhysicalHardware` mode, slider and force controls are hard-disabled and locked with padlock icon. Communication loss emits immediate `BAD_COMM_FAILURE` (`0x0008`) with zero silent fallback.

---

## 11. React Component Hierarchy & TypeScript Contracts

### 11.1 Component Directory Structure
```
src/components/workbench/
├── WorkbenchShell.tsx                  # Root 5-zone flexbox container
├── Zone1TopHud/
│   ├── TopHudBanner.tsx                # Fixed 38px HUD header
│   └── TopHudAlarms.tsx
├── Zone2PrimaryExplorer/
│   ├── PrimaryExplorer.tsx             # 240px-360px resizable tree
│   ├── ScreenHierarchyTree.tsx         # HMI screens & sub-screens tree
│   └── DevicesNetworksTree.tsx         # Hardware views (Network, Device, Topology)
├── Zone3MainCanvas/
│   ├── MainTabbedCanvas.tsx            # Multi-document tab host (Dockview)
│   ├── BoundedGridCanvas.tsx           # Bounded grid engine
│   ├── ProcessPidCanvas.tsx            # Interactive P&ID schematic
│   └── DevicesNetworksCanvas.tsx       # 3 Hardware views canvas host
├── Zone4TaskCards/
│   ├── TaskCardsDock.tsx               # Fixed 384px constant-width right dock
│   ├── TaskCardsTabBar.tsx             # Vertical/horizontal task cards switcher
│   ├── InstructionsTaskCard.tsx        # Standard IEC 61131-3 logic operations
│   ├── HardwareCatalogTaskCard.tsx     # OEM modules, drives, sensors catalog
│   ├── LibrariesTaskCard.tsx           # Project Library & Global Libraries
│   ├── LayoutWidgetsTaskCard.tsx       # Modular widget deck & P&ID symbol palette
│   └── cards/                          # Modular widget deck cards
│       ├── ControlDeckCard.tsx
│       ├── InterlockMatrixCard.tsx
│       ├── MiniTrendCard.tsx
│       └── SimulationDeckCard.tsx
├── Zone5BottomProperties/
│   ├── BottomPropertiesInspector.tsx   # 180px-320px collapsible bottom dock
│   ├── PropertiesTabBar.tsx            # General, IO, Diagnostics, Alarms tabs
│   ├── PropertiesGeneralTab.tsx        # Metadata, author, naming, comments
│   ├── PropertiesIoAddressesTab.tsx    # Address bounds, process image, channels
│   ├── PropertiesDiagnosticsTab.tsx    # Online health, CRC counters, buffer
│   └── PropertiesAlarmsTab.tsx         # ISA-18.2 thresholds & active alarms
└── Zone1StatusBar/
    └── WorkbenchStatusBar.tsx          # Fixed 24px industrial status bar
```

### 11.2 Zustand Store Architecture (`inspectorStore.ts`)

```typescript
import { create } from 'zustand';

export type DisplayArchetype = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';
export type DeviceExecutionMode = 'PhysicalHardware' | 'ManualSimulation';
export type TaskCardTab = 'INSTRUCTIONS' | 'HARDWARE_CATALOG' | 'LIBRARIES' | 'LAYOUT_WIDGETS';
export type InspectorBottomTab = 'GENERAL' | 'IO_ADDRESSES' | 'DIAGNOSTICS' | 'ALARMS' | 'CONSOLE';

export interface ModularCardConfig {
  instanceId: string;
  widgetId: string;
  title: string;
  collapsed: boolean;
  orderIndex: number;
  customProps?: Record<string, unknown>;
}

export interface InspectorState {
  // Zone 4 Task Cards
  isTaskCardOpen: boolean;
  activeTaskCard: TaskCardTab;
  readonly fixedSidebarWidth: 384; // Fixed constant width per Adrian Ergonomic Invariant
  setActiveTaskCard: (tab: TaskCardTab) => void;
  toggleTaskCard: () => void;

  // Zone 5 Context-Sensitive Bottom Inspector
  isBottomInspectorOpen: boolean;
  activeBottomTab: InspectorBottomTab;
  selectedElementId: string | null;
  selectedElementType: 'DEVICE' | 'SLOT' | 'PORT' | 'CABLE' | 'BLOCK' | 'WIDGET' | null;
  setActiveBottomTab: (tab: InspectorBottomTab) => void;
  selectElement: (id: string | null, type?: InspectorState['selectedElementType']) => void;

  // Multi-Mode Display Archetypes & Selection
  activeArchetype: DisplayArchetype;
  selectedTagPrefix: string | null;
  selectedUdtType: string | null;
  selectedDeviceLabel: string | null;
  
  // Per-Device Execution Mode & Simulation State
  selectedDeviceExecutionMode: DeviceExecutionMode;
  isSimulationActive: boolean;
  forcedVariables: Record<string, number | boolean | string>;

  cardStack: ModularCardConfig[];

  // Actions
  openInspector: (tagPrefix: string, udtType: string, label: string, mode?: DisplayArchetype) => void;
  closeInspector: () => void;
  setDisplayArchetype: (archetype: DisplayArchetype) => void;
  setDeviceExecutionMode: (deviceUri: string, mode: DeviceExecutionMode, operatorId: string) => Promise<void>;
  injectQuickSliderValue: (tagUri: string, value: number) => Promise<void>;
  forceVariable: (tagUri: string, value: number | boolean | string) => Promise<void>;
  reorderCardStack: (activeId: string, overId: string) => void;
}
```

---

## 12. Accessibility & Keyboard Navigation (ISO 9241-110)

- **Keyboard Focus Management**:
  * `Alt+1`, `Alt+2`, `Alt+3`: Quick switcher between Network, Device, and Topology views in Zone 3.
  * `Ctrl+B`: Toggle Zone 2 Primary Explorer.
  * `Ctrl+J`: Toggle Zone 5 Bottom Properties & Inspector.
  * `Ctrl+K`: Focus Task Card search field in Zone 4.
- **High-Contrast Focus Rings**: All interactive cards, tabs, and buttons feature high-contrast `2px solid var(--color-focus-ring)` indicator rings adhering to WCAG 2.1 AA.
- **Screen Reader Announcements**: State changes (alarm triggers, simulation activations, and connection errors) emit `aria-live="assertive"` announcements to assistive technologies.
