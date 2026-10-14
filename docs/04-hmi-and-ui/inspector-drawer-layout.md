# MythOS Modular Drag-and-Drop Inspector & Multi-Mode View Architecture

## 1. Executive Summary & Purpose
This document specifies the architectural, visual, and behavioral standards for the MythOS Modular Drag-and-Drop Inspector and Multi-Mode View Architecture within the unified 5-Zone SCADA Workbench Canvas.

In modern mission-critical industrial plants, fixed, rigid operator interfaces fail to accommodate diverse operational personas—ranging from field commissioning technicians on rugged tablets to central control room operators monitoring multi-monitor video walls. Traditional SCADA HMIs force hardcoded panels on users. MythOS provides a **100% user-editable, modular drag-and-drop presentation engine** inspired by modern industrial suites (Siemens TIA Portal, Inductive Automation Ignition, and AVEVA InTouch).

The system unifies all equipment inspection, actuation, diagnostics, and process schematics across **4 Distinct Display Archetypes**:
1. **P&ID Mode**: Process & Instrumentation Diagram vector symbol and schematic view rendered on a bounded grid canvas.
2. **Interactive Mode**: Dynamic widget and mimic control deck equipped with live command triggers, sliders, jog buttons, and setpoint adjusters.
3. **Popup Mode**: Detachable, floating modal window or multi-monitor dialog engineered for dedicated diagnostic deep dives.
4. **Sidebar Mode**: Docked, slide-over inspector drawer constrained to a **fixed constant width of 384px** with a dynamically scrollable, user-customizable vertical card stack.

---

## 2. Normative References & Box-Model Constraints
- **ANSI/ISA-101.01-2015**: *Human Machine Interfaces for Process Automation Systems*.
- **ANSI/ISA-18.2-2016**: *Management of Alarm Systems for the Process Industries*.
- **ISO 9241-110:2020**: *Ergonomics of human-system interaction — Principles of interaction*.
- **W3C Design Tokens Community Group (DTCG)**: *2025.10 Specification (Dual Theme: Dark Obsidian & Light Clean Slate)*.
- **Adrian Box-Model & Layout Invariants (Design Division Standard)**:
  * **Top Industrial HUD Banner**: Fixed `38px` height.
  * **Activity Bar**: Fixed `48px` width.
  * **Industrial Status Bar**: Fixed `24px` height.
  * **Primary Explorer Tree**: `240px` to `480px` resizable width (default `280px`).
  * **Standardized Sidebar Drawer**: **Fixed constant width of 384px** (`min-width: 384px; max-width: 384px`). Zero horizontal resizing across all plant screens for visual uniformity.
  * **Bottom Industrial Console**: `160px` to `280px` collapsible height (default `220px`).
  * **Bounded Canvas Dimensions**: Standard FHD `1920px × 1080px` (120 × 67.5 cells @ 16px grid); 4K Video Wall ceiling `3840px × 2160px` (240 × 135 cells @ 16px grid).
  * **Predictable Flexbox & Overflow Rules**: Explicit `flex: 1 1 0%`, `min-height: 0`, and `overflow: hidden`. Zero double scrollbars.

---

## 3. 5-Zone Workbench Layout Architecture

The MythOS Workbench Shell arranges the display zones using high-performance Flexbox and `@hexact/dockview`:

```
+---------------------------------------------------------------------------------------------------------+
| ZONE 1: TOP INDUSTRIAL HUD BANNER (38px Fixed Height)                                                   |
| [SYS: ONLINE]  [NODE: lycan-edge-01 (192.168.1.50)]  [CYCLE: 10ms]  [ACTIVE ALARMS: 0 CRIT | 2 WARN] [🔒]|
+----+----------------------------------+------------------------------------+----------------------------+
| Z  | ZONE 2: PRIMARY EXPLORER        | ZONE 3: MAIN DOCKVIEW CANVAS       | ZONE 4: SECONDARY          |
| O  | (240px - 480px Resizable)        | (100% Flex Multi-Tab Viewport)     | SIDEBAR INSPECTOR          |
| N  | - Physical Hardware Topology     |                                    | (Fixed 384px Constant)     |
| E  | - Tag Namespaces Tree            | +--------------------------------+ |                            |
|    | - Function Block Instances       | | Tab 1: P&ID  | Tab 2: Topology | | [X] Close   [⤢] Undock     |
| 5  | - Recipe Catalog (ISA-88)        | +--------------------------------+ | -------------------------- |
| :  | - Sub-Canvas Asset Library       | |                                | | Dynamic Vertical Card    |
| A  |                                  | |  Bounded Process Canvas        | | Stack (@dnd-kit reorder) |
| C  |                                  | |  (Grid Snap: 8px / 16px)       | | [≡] Control Deck Card    |
| T  |                                  | |  [Imported SVG / JPEG / GIF]   | | [≡] Interlock Matrix Card|
| I  |                                  | |                                | | [≡] Sparkline Trend Card |
| V  +----------------------------------+------------------------------------+----------------------------+
| I  | ZONE 5: BOTTOM INDUSTRIAL CONSOLE (160px - 280px Collapsible)                                      |
| T  | [Tabs: ISA-18.2 Alarms | Packet Sniffer | Telemetry Events | Historian Query | Sub-Canvas Editor]   |
| Y  +----------------------------------------------------------------------------------------------------+
| (48| ZONE 1.1: STATUS BAR (24px Fixed Height)                                                           |
| px)| [Gateway: RUNNING] [Memory: 42MB] [TagBus: 14,200 tags] [MQTT: CONNECTED] [Time: 2025-10-13 17:30] |
+----+----------------------------------------------------------------------------------------------------+
```

---

## 4. The 4 Display Archetypes & Screen-Role Mapping

In accordance with Adrian's architectural mandate, the 4 display archetypes are decoupled and targeted to dedicated operational screens to prevent cognitive clutter:

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

### 4.1 Archetype 1: P&ID Mode (Dedicated Engineering Screen)
- **Primary Host**: **Engineering Screens** (Level 1 Plant Overview & Level 2 Detailed Process Schematics).
- **Presentation**:
  * Rendered directly onto the bounded grid canvas for engineers, process technologists, and automation leads.
  * Displays complete mechanical and instrumentation topology: piping lines with flow arrows, isolation valves, instrument measurement bubbles, and equipment vessels.
  * Strict adherence to ISA-101 monochrome design: neutral slate borders and vessels in normal state, illuminating exclusively during active ISA-18.2 alarm conditions.
- **Interaction**: Single-click selects instrument and opens the Zone 4 Sidebar Widget Deck; double-click or clicking setting gear launches Archetype 3 (Popup Mode).

### 4.2 Archetype 2: Interactive Mode (Dedicated Operator Screen)
- **Primary Host**: **Operator Screens** (Day-to-day production floor terminals, panel PCs, and operator workstations).
- **Presentation**:
  * Streamlined, high-contrast mimic cards and actuation decks engineered specifically for plant floor operators.
  * Strips away engineering schematics and complex piping, presenting only essential control affordances:
    * Segmented mode selector pills: `[ AUTO ]  [ MANUAL ]  [ LOCAL ]`.
    * Large high-visibility tactile pushbuttons (`START`, `STOP`, `PAUSE`).
    * Direct setpoint adjustment sliders and rapid step buttons (`[-10] [-1] [+1] [+10]`).
    * Monospaced live readouts (`JetBrains Mono` 28px) with engineering units and high-contrast status badges.
- **Sub-Canvas Portability**: Interactive mimic decks are authored on bounded sub-canvases and placed onto operator screens via simple drag-and-drop.

### 4.3 Archetype 3: Popup Mode (Contextual Diagnostic Window)
- **Primary Trigger**: Contextual popover launched when an operator or engineer clicks an instrument in P&ID Mode or clicks the `[⚙ Settings]` trigger on an Interactive Mimic Card.
- **Presentation**:
  * Detachable floating modal dialog or popped-out OS window via `window.open` and React Portals.
  * Engineered for deep troubleshooting, PID loop tuning, and multi-monitor control room setups (enabling engineers to pin a vessel diagnostic popup on monitor 2 while observing schematics on monitor 1).
  * Houses full parameter calibration, raw register bitwise inspection, 100 Hz high-speed trends, and alarm threshold configuration.

### 4.4 Archetype 4: Sidebar Mode (iOS-Style Modular Widget Deck)
- **Primary Role**: Reusable, customizable companion widget deck docked on the right side of the screen.
- **Ergonomic Constraint (Adrian Constant-Width Rule)**:
  * **Strict Constant Width**: Fixed exactly at **`384px`** (`width: 384px; min-width: 384px; max-width: 384px; flex: 0 0 384px`).
  * Horizontal resizing is strictly prohibited across all screens to guarantee enterprise-wide visual stability.
- **iOS-Style Modular Widget Grid System (1/2-Width & 1-Width Column Standards)**:
  To eliminate arbitrary card sizing and deliver an authentic iOS-style customizable widget deck, the 384px fixed sidebar is partitioned into a **2-column modular grid** (two 184px sub-columns with a 16px central gutter). Widgets conform to standardized width and height matrices:

  ```
  +-------------------------------------------------------------+
  |              SIDEBAR MODULAR WIDGET GRID (384px)            |
  |  [ Column 1: 184px ]  (16px gutter)  [ Column 2: 184px ]    |
  +-----------------------------+-------------------------------+
  | [1/2W x 1H: 184px x 96px]   | [1/2W x 1H: 184px x 96px]     | <-- Side-by-side S widgets
  | (Pump P-101 Status Dial)    | (Valve XV-101 Open/Close)     |
  +-----------------------------+-------------------------------+
  | [1W x 1H: 384px x 96px Full-Width Horizontal Strip]         | <-- 1-Width S strip
  | (Plant Boiler Totalizer Odometer & Steam Production Rate)   |
  +-------------------------------------------------------------+
  | [1W x 2H: 384px x 192px Full-Width Medium Widget]           | <-- 1-Width M widget
  | (Actuator Deck: Start/Stop Buttons + VFD Speed Jog Slider)  |
  +-------------------------------------------------------------+
  | [1/2W x 2H: 184px x 192px]  | [1/2W x 2H: 184px x 192px]    | <-- Side-by-side M cards
  | (Vertical Tank Level Bar)   | (Temperature Gauge Dial)      |
  +-----------------------------+-------------------------------+
  | [1W x 4H: 384px x 384px Full-Width Large Square Widget]     | <-- 1-Width L widget
  | (Cause-and-Effect Interlock Matrix Checklist [✓] / [✗])     |
  +-------------------------------------------------------------+
  ```

  1. **Standard Form Factor Matrix**:
     * **`Size S-Half` (1/2 Width × 1 Height = `184px × 96px`)**:
       - Compact single-metric gauge dial, digital RUN/TRIP status pill, quick toggle switch.
       - Fits two widgets side-by-side across the 384px sidebar deck.
     * **`Size S-Full` (1 Full Width × 1 Height = `384px × 96px`)**:
       - Horizontal KPI strip, production counter odometer, dual-meter status bar.
     * **`Size M-Half` (1/2 Width × 2 Height = `184px × 192px`)**:
       - Vertical tank level column, slim bar graph with high/low threshold markers.
     * **`Size M-Full` (1 Full Width × 2 Height = `384px × 192px`)**:
       - Actuator control deck (tactile Start/Stop + jog slider), real-time Sparkline mini-trend (PV vs SP).
     * **`Size L-Full` (1 Full Width × 4 Height = `384px × 384px`)**:
       - Complete square card: Cause-and-Effect Boolean Interlock Matrix ([✓] / [✗]), multi-pen trend graph, PID parameter tuning suite ($K_p$, $T_i$, $T_d$).
- **Smart Widget Stacking (iOS Smart Stack Paradigm)**:
  * Operators can stack multiple widgets of the same size (e.g. stacking Motor Bearing Temperature and Motor Current Trend within a single Size M slot).
  * Vertical swipe or carousel arrows allow flipping between stacked widgets without occupying additional vertical sidebar space.
- **User Customization via Drag-and-Drop (`@dnd-kit`)**:
  * Operators can drag widgets from an available palette, reorder their vertical stack, or remove cards to tailor the sidebar to their specific shift responsibilities.

---

## 5. Bounded Grid Canvas Engine

To deliver deterministic layout fidelity matching industrial CAD tools (Siemens TIA Portal Graphics Designer, AutoCAD Electrical, Ignition Vision), MythOS implements the **Bounded Grid Canvas Engine**.

```
+----------------------------------------------------------------------------------------------------+
|                                 BOUNDED GRID CANVAS ENGINE                                         |
|                                                                                                    |
|  [0, 0]                                                                   [1920, 0] / [3840, 0]   |
|     +-----------------------------------------------------------------------------+                |
|     | · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · |                |
|     | · · [Grid: 16px Primary / 8px Secondary]  · · · · · · · · · · · · · · · · · |                |
|     | · ·                                       · · · · · · · · · · · · · · · · · |                |
|     | · ·   +--------------------+              · · ·   +--------------------+  · |                |
|     | · ·   | Sub-Canvas Asset   |              · · ·   | Animated Valve     |  · |                |
|     | · ·   | (P-101 Skid Group) |==============·===·==>| (GIF / SVG Loop)   |  · |                |
|     | · ·   +--------------------+              · · ·   +--------------------+  · |                |
|     | · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · |                |
|     | · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · |                |
|     +-----------------------------------------------------------------------------+                |
|  [0, 1080] / [0, 2160]                                                    [1920, 1080] / [3840, 2160]|
+----------------------------------------------------------------------------------------------------+
```

### 5.1 Spatial Grid Limits & Snapping
- **Grid Snap Granularity**:
  * **Primary Snap (16px)**: Standard alignment step for equipment symbols, vessels, pipe junctions, and container cards (120 × 67.5 grid cells on FHD).
  * **Secondary Fine Snap (8px)**: Micro-alignment step for wiring lines, bus cables, tag badges, and terminal connection pins.
- **Max Bounded Dimensions**:
  * **Standard Full HD (FHD)**: Native raster boundary of `1920px × 1080px`.
  * **Ultra-High Definition (4K Video Wall)**: Configurable ceiling boundary up to `3840px × 2160px` (240 × 135 grid cells).
  * Elements cannot be dragged beyond defined canvas coordinate limits, eliminating lost-offscreen graphical assets.
- **Coordinate System**:
  * Integer-based Cartesian plane ($X \in [0, X_{max}], Y \in [0, Y_{max}]$) with zoom levels constrained between `25%` and `400%`.
  * Renders via CSS transforms (`transform: translate3d(x, y, 0) scale(zoom)`) backed by hardware GPU rasterization.

### 5.2 Constant Grid Matrix Resolution Independence (Multi-Device Viewport Scaling)
- **Problem Statement**:
  Industrial plant displays exhibit radical resolution disparities—from 4K central control room video walls (`3840×2160`) to standard engineering laptops (`1920×1080`), rugged maintenance tablets (`1280×800`), and handheld mobile devices (`390×844`). Absolute pixel layouts break, truncate telemetry labels, or produce illegible shrinking.
- **The Constant Grid Principle (Adrian Mandate)**:
  * The **number of logical grid cells across the screen is held constant** across all client devices (default profile: `120 columns × 67.5 rows` of 16px logical units).
  * Rather than adding or removing grid columns when opening screens on different hardware, **the physical pixel dimension of each grid cell scales adaptively** to fit the target display viewport:
    $$	ext{CellPixelSize} = \min\left( rac{	ext{ViewportWidth}}{	ext{GridCols}}, rac{	ext{ViewportHeight}}{	ext{GridRows}} 
ight)$$
  * **Concrete Scaling Examples**:
    * **4K Control Room Wall (3840×2160)**: `1 Grid Cell = 32.0 px`. Layout renders with massive, crisp clarity across the entire video wall without dead margin space.
    * **FHD Engineering Laptop (1920×1080)**: `1 Grid Cell = 16.0 px`. Standard native industrial viewing baseline.
    * **Rugged Field Tablet (1280×720 viewport)**: `1 Grid Cell = 10.66 px`. Identical geometric alignment, zero clipped text, zero broken pipe lines.
    * **Mobile Device / Phone (390×844 viewport)**: The screen preserves the exact relative coordinate hierarchy with viewport-adaptive scaling or switches to the Archetype 4 (Sidebar stack / Mobile card view) without spatial asset collision.
- **Zero Layout Distortion Guarantee**:
  Because every symbol, vessel, and pipe is authored strictly in integer grid units (e.g. Pump = `6 cells wide × 4 cells high`), the relative spatial composition is mathematically invariant across all physical resolutions.

---

## 6. Industrial Multi-Format Asset Importer

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

### 6.1 Vector Graphics (SVG)
- **Primary Use**: Equipment symbols (pumps, valves, heat exchangers, instrumentation bubbles) and process flow lines.
- **Engine Processing**:
  * Automatic XML sanitization removing script tags and malicious payloads.
  * Preservation of SVG `viewBox` attributes for lossless infinite scaling.
  * CSS Variable Injection: SVG stroke and fill attributes are mapped directly to W3C DTCG color tokens (`fill="var(--color-isa101-process-vessel)"`).
  * Dynamic Alarm Binding: Injects animated SVG pulse halos when bound alarm tags trip.

### 6.2 Raster Photographic Backdrops (JPEG)
- **Primary Use**: Architectural plant layouts, aerial drone photos of tank farms, satellite maps, and physical control cabinet backplates.
- **Engine Processing**:
  * Renders on the base canvas layer below the 16px interactive grid.
  * Optimized image buffering with progressive WebP/JPEG decoding.
  * Configurable canvas opacity (`20%` to `100%`) ensuring engineering overlays and piping schematics remain visually distinct over photorealistic backgrounds.

### 6.3 Animated Equipment State Decoders (GIF / WebP)
- **Primary Use**: Intuitive physical state animations (rotating impeller on centrifugal pumps, agitator blades in slurry tanks, conveyor belt movement, active burner flame loops).
- **Engine Processing**:
  * Frame-rate throttling to prevent browser GPU overload (synchronized to plant display refresh rates).
  * **Tag-Driven State Gating**:
    * When `Pump.bRunning == TRUE`: Animated GIF loop plays.
    * When `Pump.bRunning == FALSE`: Engine freezes frame on static neutral index 0 (`pipe-inactive` tone).
    * When `Pump.bTripped == TRUE`: Engine overlays pulsing red ISA alarm glyph (`#DC2626`).

---

## 7. Hierarchical Reusable Sub-Canvases (Nesting & Drag-and-Drop)

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

### 7.1 Nesting Architecture & Encapsulation
- **Scoped Tag Aliasing**:
  * A Sub-Canvas defines formal input parameters (e.g. `$PUMP_TAG`, `$VALVE_TAG`, `$AREA_ID`).
  * When dragged and dropped onto a parent canvas, an **Instance Parameter Dialog** binds the alias to concrete TagBus paths (e.g. `$PUMP_TAG -> Plant1.Area2.P101A`).
- **Composition Levels**:
  * **Level 0 (Primitive)**: Single vector symbol or basic slider widget.
  * **Level 1 (Sub-Canvas Assembly)**: Motor skid with isolation valves, bypass line, and differential pressure transmitter.
  * **Level 2 (Parent Unit Canvas)**: Evaporator unit containing 4 nested Sub-Canvas assemblies.
  * **Level 3 (Plant Area Canvas)**: Multi-screen plant overview hosting nested unit screens.
- **Portability**: All sub-canvases are serialized into standardized JSON structures stored in the project repository (`assets/subcanvases/<name>.subcanvas.json`), allowing seamless version control via Git.

---

## 8. Drag-and-Drop Widget Registry & Layout Customization

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

### 8.1 The Standard Modular Widget Library
1. **Actuator Control Deck Card (`widget.control-deck`)**:
   - Operating Mode Selector (`AUTO`, `MANUAL`, `LOCAL`).
   - Large digital readout (`JetBrains Mono` 28px) with engineering units (`RPM`, `bar`, `°C`).
   - Pushbuttons: Start (`#16A34A`), Stop (`#475569`), Jog Forward (Momentary), Fault Reset (`#CA8A04`).
2. **Interlock Matrix Checklist Card (`widget.interlock-matrix`)**:
   - Cause-and-Effect Boolean validation checklist.
   - Status indicators: `[✓]` Satisfied (`#16A34A`), `[✗]` Pulsing Trip (`#DC2626`), `[BYP]` Authorized Bypass (`#CA8A04`).
   - First-Out Trip latch highlighter identifying the root-cause failure with microsecond accuracy.
3. **High-Resolution Mini-Trend Card (`widget.mini-trend`)**:
   - Real-time Canvas/WebGL sparkline with selectable time windows (`30s`, `1m`, `5m`, `15m`, `1h`).
   - Traced channels: PV (Cyan `#38BDF8`), SP (Dashed Yellow `#FACC15`), CV (Dotted Magenta `#E879F9`).
   - Shaded danger envelopes for High-High and Low-Low alarm thresholds.
4. **Maintenance & PID Tuning Card (`widget.maintenance-tuning`)**:
   - Runtime odometer (Total hours, cycle count, service due countdown).
   - Closed-loop PID parameters ($K_p$, $T_i$, $T_d$, Deadband).
   - Supervisor interlock bypass lock with PIN authentication.
5. **Raw Register & Diagnostics Log Card (`widget.diagnostics-log`)**:
   - Hexadecimal PLC memory viewer and communication packet drop counters.
6. **Simulation & Tag Injection Deck Card (`widget.simulation-deck`)**:
   - **Explicit Per-Device Operational Mode Toggle**:
     * Segmented radio switch: `[ 🔌 Physical Hardware ]` (Default Production Mode) vs `[ 🧪 Simulation / Manual Test ]` (Staging / Development Mode).
     * Mode transitions require authenticated operator confirmation and log immediately to the immutable audit trail (`[OPERATOR: <name>] [ACTION: Enable Simulation Mode] [TARGET: external.<Namespace>.<Device>]`).
   - **High-Visibility Safety Stamping**:
     * When `ManualSimulation` is selected, a pulsing high-visibility amber indicator badge `[🧪 SIMULATION ACTIVE]` (`#D97706` / `#F59E0B`) is stamped on:
       1. The Inspector drawer header and device title.
       2. The Network View topology node for this device.
       3. All associated tags in the Tag Browser and Inspector table.
       4. SCADA Canvas mimic faceplates and symbol bindings.
   - **Quick Input Slider & Variable Forcing Interface**:
     * **Interactive Slider**: Continuous tactile slider (`0.0` to `100.0%` or calibrated engineering unit span) injecting values directly into the Microkernel TagBus feedback slot (`.pv`).
     * **Direct Variable Forcing Box**: Numeric / Boolean input with `[ Force Override ]` and `[ Release Override ]` actions.
     * **Quality Code Assertion**: Injected values automatically receive OPC DA quality `UNCERTAIN_SUBSTITUTED` (`0x0044`), ensuring downstream safety loops detect non-field data.
   - **Physical Hardware Safety Lockout**:
     * When configured in `PhysicalHardware` mode (production default), the slider and force controls are **hard-disabled and locked** with a safety padlock icon and tooltip (*"Physical Hardware Active: Manual TagBus injection disabled to protect field operations"*).
     * Physical communication loss or severed cables emit immediate `BAD_COMM_FAILURE` (`0x0008`/`0x0018`) alarms with ZERO silent fallback.

---

## 9. Layout Persistence Schema (`project.json` & User Presets)

Customized layouts are serialized into human-readable JSON structures, allowing role-based persistence across shifts, machines, and operator logins:

```json
{
  "$schema": "https://mythos.firmansyah.co/schemas/layout-preset.v2.json",
  "presetId": "preset-digestion-operator-dark",
  "name": "Digestion Area Lead Operator Layout",
  "theme": "dark",
  "sidebar": {
    "width": 384,
    "collapsed": false,
    "cards": [
      {
        "instanceId": "card-interlock-01",
        "widgetId": "widget.interlock-matrix",
        "title": "Safety & Permissive Matrix",
        "collapsed": false,
        "pinned": true,
        "props": { "showBypassControls": false }
      },
      {
        "instanceId": "card-control-01",
        "widgetId": "widget.control-deck",
        "title": "Actuation & Manual Jog",
        "collapsed": false,
        "pinned": false,
        "props": { "jogStep": 50 }
      },
      {
        "instanceId": "card-trend-01",
        "widgetId": "widget.mini-trend",
        "title": "1-Minute Dynamic Response",
        "collapsed": false,
        "pinned": false,
        "props": { "timeWindowSec": 60, "sampleRateHz": 10 }
      }
    ]
  },
  "subCanvases": [
    {
      "subCanvasAssetId": "Chamber01_PID_SubCanvas",
      "position": { "x": 128, "y": 96 },
      "dimensions": { "width": 640, "height": 480 },
      "parameterBindings": {
        "$PUMP_TAG": "Plant1.Digestion.Pump_P101A",
        "$VALVE_TAG": "Plant1.Digestion.Valve_XV101"
      }
    }
  ]
}
```

---

## 10. React Component Hierarchy & TypeScript Contracts

### 10.1 Component Directory Structure
```
src/components/workbench/
├── WorkbenchShell.tsx                  # Root 5-zone flexbox container
├── Zone1TopHud/
│   ├── TopHudBanner.tsx                # Fixed 38px HUD header
│   └── TopHudAlarms.tsx
├── Zone2PrimaryExplorer/
│   ├── PrimaryExplorer.tsx             # 240px-480px resizable tree
│   ├── SubCanvasAssetCatalog.tsx       # Reusable sub-canvas library tree
│   └── FunctionBlockTree.tsx
├── Zone3MainCanvas/
│   ├── MainTabbedCanvas.tsx            # Multi-document tab host (Dockview)
│   ├── BoundedGridCanvas.tsx           # 1920x1080 / 3840x2160 bounded grid engine
│   ├── CanvasAssetLayer.tsx            # SVG, JPEG backdrop & animated GIF renderer
│   ├── NestedSubCanvasHost.tsx         # Hierarchical sub-canvas container
│   └── ProcessPidCanvas.tsx            # Interactive P&ID schematic
├── Zone4SecondaryInspector/
│   ├── SecondaryInspector.tsx          # Fixed 384px constant-width slide-over drawer
│   ├── InspectorHeader.tsx             # Equipment title, undock trigger, close button
│   ├── ModularCardStack.tsx            # @dnd-kit vertical draggable card container
│   ├── cards/
│   │   ├── ControlDeckCard.tsx         # Mode selector, pushbuttons, jog deck
│   │   ├── InterlockMatrixCard.tsx     # Cause-and-Effect Boolean checklist
│   │   ├── MiniTrendCard.tsx           # High-speed Canvas/WebGL sparkline
│   │   ├── MaintenanceTuningCard.tsx   # Odometer, PID loop tuning
│   │   ├── DiagnosticLogCard.tsx       # Hex registers, comm diagnostics
│   │   └── SimulationDeckCard.tsx      # Per-device toggle, Quick Slider, Variable Forcing
├── Zone5IndustrialConsole/
│   ├── BottomIndustrialConsole.tsx     # 160px-280px collapsible tray
│   ├── Isa182AlarmTable.tsx
│   └── SubCanvasAuthoringConsole.tsx   # Visual nested canvas builder & testbench
└── Zone1StatusBar/
    └── WorkbenchStatusBar.tsx          # Fixed 24px industrial status bar
```

### 10.2 Zustand Store Architecture (`inspectorStore.ts`)

```typescript
import { create } from 'zustand';

export type DisplayArchetype = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';
export type DeviceExecutionMode = 'PhysicalHardware' | 'ManualSimulation';

export interface ModularCardConfig {
  instanceId: string;
  widgetId: string;
  title: string;
  collapsed: boolean;
  orderIndex: number;
  customProps?: Record<string, unknown>;
}

export interface InspectorState {
  isOpen: boolean;
  isUndocked: boolean;
  activeArchetype: DisplayArchetype;
  readonly fixedWidth: 384; // Fixed constant width per Adrian Ergonomic Invariant
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
  setUndocked: (undocked: boolean) => void;
  setDisplayArchetype: (archetype: DisplayArchetype) => void;
  setDeviceExecutionMode: (deviceUri: string, mode: DeviceExecutionMode, operatorId: string) => Promise<void>;
  injectQuickSliderValue: (tagUri: string, value: number) => Promise<void>;
  forceVariable: (tagUri: string, value: number | boolean | string) => Promise<void>;
  releaseVariableForce: (tagUri: string) => Promise<void>;
  reorderCards: (startIndex: number, endIndex: number) => void;
  toggleCardCollapse: (instanceId: string) => void;
  removeCard: (instanceId: string) => void;
  addCard: (widgetId: string, title: string) => void;
  resetToDefaultLayout: () => void;
}

export const useInspectorStore = create<InspectorState>((set) => ({
  isOpen: false,
  isUndocked: false,
  activeArchetype: 'SIDEBAR',
  fixedWidth: 384, // Enforced constant
  selectedTagPrefix: null,
  selectedUdtType: null,
  selectedDeviceLabel: null,
  cardStack: [
    { instanceId: 'c1', widgetId: 'widget.interlock-matrix', title: 'Interlock Checklist', collapsed: false, orderIndex: 0 },
    { instanceId: 'c2', widgetId: 'widget.control-deck', title: 'Manual Actuation', collapsed: false, orderIndex: 1 },
    { instanceId: 'c3', widgetId: 'widget.mini-trend', title: 'Process Mini-Trend', collapsed: false, orderIndex: 2 },
    { instanceId: 'c4', widgetId: 'widget.maintenance-tuning', title: 'Maintenance & Tuning', collapsed: true, orderIndex: 3 },
  ],

  openInspector: (tagPrefix, udtType, label, mode = 'SIDEBAR') =>
    set({
      isOpen: true,
      selectedTagPrefix: tagPrefix,
      selectedUdtType: udtType,
      selectedDeviceLabel: label,
      activeArchetype: mode,
    }),

  closeInspector: () => set({ isOpen: false }),
  setUndocked: (isUndocked) => set({ isUndocked }),
  setDisplayArchetype: (activeArchetype) => set({ activeArchetype }),

  reorderCards: (startIndex, endIndex) =>
    set((state) => {
      const updated = Array.from(state.cardStack);
      const [removed] = updated.splice(startIndex, 1);
      updated.splice(endIndex, 0, removed);
      return { cardStack: updated.map((c, i) => ({ ...c, orderIndex: i })) };
    }),

  toggleCardCollapse: (instanceId) =>
    set((state) => ({
      cardStack: state.cardStack.map((c) =>
        c.instanceId === instanceId ? { ...c, collapsed: !c.collapsed } : c
      ),
    })),

  removeCard: (instanceId) =>
    set((state) => ({
      cardStack: state.cardStack.filter((c) => c.instanceId !== instanceId),
    })),

  addCard: (widgetId, title) =>
    set((state) => ({
      cardStack: [
        ...state.cardStack,
        {
          instanceId: `card-${Date.now()}`,
          widgetId,
          title,
          collapsed: false,
          orderIndex: state.cardStack.length,
        },
      ],
    })),

  resetToDefaultLayout: () =>
    set({
      cardStack: [
        { instanceId: 'c1', widgetId: 'widget.interlock-matrix', title: 'Interlock Checklist', collapsed: false, orderIndex: 0 },
        { instanceId: 'c2', widgetId: 'widget.control-deck', title: 'Manual Actuation', collapsed: false, orderIndex: 1 },
        { instanceId: 'c3', widgetId: 'widget.mini-trend', title: 'Process Mini-Trend', collapsed: false, orderIndex: 2 },
        { instanceId: 'c4', widgetId: 'widget.maintenance-tuning', title: 'Maintenance & Tuning', collapsed: true, orderIndex: 3 },
      ],
    }),
}));
```

---

## 11. Accessibility & Keyboard Navigation (ISO 9241-110)
- **Escape Key (`Esc`)**: Closes the Sidebar Inspector and returns focus cleanly to the active canvas element.
- **Tab Navigation**: All interactive buttons, card headers, drag handles, and numeric inputs feature high-visibility `#38BDF8` (Dark) / `#0284C7` (Light) focus rings (`outline: 2px solid var(--color-border-focus); outline-offset: 2px`).
- **Shortcuts**:
  * `Ctrl+1`: Focus Control Deck Card.
  * `Ctrl+2`: Focus Interlock Matrix Card.
  * `Ctrl+3`: Focus Mini-Trend Card.
  * `Ctrl+4`: Focus Maintenance Card.
  * `Ctrl+U`: Toggle Undocked Window (Popup Mode).
  * `Ctrl+G`: Toggle Grid Snapping Overlay (16px / 8px).
