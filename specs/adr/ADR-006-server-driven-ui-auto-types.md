# ADR-006: Server-Driven UI (SDUI) Architecture and Automated Type Export

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Iris (VP Design), Scribe (VP Publication)

---

## 1. Context and Problem Statement

SCADA Human-Machine Interface (HMI) frontends are prone to architectural decay when business logic, unit conversions, alarm threshold evaluations, and permission checks are duplicated inside client-side JavaScript/React components.

This duplication causes several critical failures:
1. **Frontend Business Logic Drift**: Changes in backend scaling algorithms or alarm states are not reflected in the UI unless frontend code is independently rebuilt and redeployed.
2. **Schema Drift and Runtime Type Errors**: In manual API workflows, TypeScript interfaces desynchronize from backend Rust models, causing runtime exceptions in the browser.
3. **Heavy Client Bundle and Re-Renders**: Performing complex calculations and state machine evaluations inside React components causes unnecessary re-renders and degrades operator UI responsiveness during high-frequency alarm floods.

MythOS requires a strict architectural separation where the frontend functions as a **Pure Dumb Presentation Layer** driven by backend schemas.

---

## 2. Decision Drivers

- **Pure Dumb Frontend Invariant**: The frontend must contain zero business logic, zero regex parsing of physical tags, zero mathematical scaling, and zero hardcoded state machine rules. The Rust backend is the single, absolute Source of Truth (SSOT).
- **Compile-Time Type Synchronization**: TypeScript type definitions must be automatically generated directly from Rust structs during build time.
- **Server-Driven Dynamic Rendering**: UI layouts, widget configurations, interlock matrices, and faceplate properties are streamed as declarative JSON schemas from the backend.
- **High-Performance Streaming**: Process values and visual state changes stream over low-overhead WebSockets or Server-Sent Events (SSE) with minimal client serialization burden.

---

## 3. Considered Options

1. **Option 1: Traditional Client-Heavy Single Page Application (SPA)**
   - *Pros*: Standard web development practice; client handles routing and local caching.
   - *Cons*: Business logic leaks into frontend code; requires manual maintenance of Swagger/OpenAPI models; desynchronization between PLC logic and UI behavior.
2. **Option 2: Manual Protobuf / gRPC-Web Client**
   - *Pros*: Binary serialization efficiency; typed proto files.
   - *Cons*: High build complexity for web browsers; requires Envoy proxy for gRPC-Web; rigid schemas that do not easily support dynamic Server-Driven UI layout trees.
3. **Option 3: Server-Driven UI (SDUI) with Rust Type Export via `specta` / `ts-rs`** (Selected)
   - *Pros*: Backend defines both the schema layout and the validated data; TypeScript types are exported at Rust compile time; frontend React components are pure declarative renderers (`Dumb Presentation Components`).

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Server-Driven UI with Automated Type Export).

### 4.1 Architecture Details
- **Type Generation**: Rust structs and enums in `mythos-core` and `mythos-driver-api` derive `specta::Type` and `serde::Serialize`. Running `cargo test --package mythos-core --test export_types` generates fully typed TypeScript definitions directly into `frontend/src/types/generated.ts`.
- **Pure Dumb Frontend Contract**:
  * The frontend receives pre-scaled engineering values, formatted display strings, and explicit color/status tokens (e.g., `status: "FAULT_CRITICAL"`, `colorToken: "color-status-danger"`).
  * The frontend never calculates `(raw_val * scale) + offset` or evaluates `if (temp > 85) state = ALARM`. The backend computes and emits the finalized visual token.
- **Server-Driven Faceplate Schema**: Dynamic inspector drawers, interlock tables, and control faceplates are rendered from declarative JSON trees emitted by the backend:
  ```json
  {
    "type": "FACEPLATE_CONTAINER",
    "title": "Slurry Pump P-101",
    "statusToken": "RUNNING_NORMAL",
    "sections": [
      {
        "type": "METRIC_DISPLAY",
        "label": "Discharge Pressure",
        "value": "4.25",
        "unit": "bar",
        "quality": "GOOD"
      },
      {
        "type": "COMMAND_BUTTON_GROUP",
        "buttons": [
          { "action": "START", "label": "Start", "enabled": false },
          { "action": "STOP", "label": "Stop", "enabled": true }
        ]
      }
    ]
  }
  ```
- **Real-Time Streaming**: Live tag updates stream via binary WebSocket frames or delta-encoded JSON over SSE, directly mutating a centralized client-side store (`zustand`) that drives dumb UI components.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Zero Schema Drift**: Backend model refactoring immediately produces compile errors or updated TypeScript definitions, making breaking changes impossible to miss.
- **Instant UI Updates**: Modifying device layout, faceplates, or menu trees in the backend updates the operator UI without rebuilding or republishing the frontend bundle.
- **Impeccable Testability**: The frontend can be verified with static schema mocks; backend business logic is tested in pure Rust unit tests without browser harnesses.

### 5.2 Negative Consequences
- **Build Coupling**: The frontend build pipeline requires running the Rust type generation step prior to TypeScript compilation (`tsc`).
