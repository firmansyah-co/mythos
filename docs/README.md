# MythOS documentation

**These are design documents, not a status report.** Most were written before the code they
describe, and a `SHALL` or an "is implemented" in one of them is not evidence that anything
ships. What actually runs is stated in the [root README](../README.md); when a document here
disagrees with the code, the code is right and the document is stale.

Handbooks for subsystems that have no code in the tree (the logic engine bridge, the MCP
subsystem, Layer 2 DCP discovery, the alarm engine, the recipe model, the historian, the RBAC
matrix and the disaster-recovery package) were removed together with the unwired code they
described. They remain in git history.

## Requirements and decisions (`specs/`)

- [System specification](../specs/spec.md) — the requirements baseline. It describes the
  intended product, most of which is not built.
- [Work breakdown](../specs/tasks.md) — the original sprint plan. It carries no completion
  status and still names deliverables that have been removed.
- [Architectural decision records](../specs/adr/) — ADR-001 to ADR-010. ADR-003, ADR-004,
  ADR-005 and ADR-007 record decisions about subsystems that are not in the tree.
  [ADR-010](../specs/adr/ADR-010-modules-are-processes.md) is how MythOS is assembled: every
  driver is a module process.

## Architecture (`docs/01-architecture/`)

- [00 Cargo workspace and crates](01-architecture/00-cargo-workspace-and-crates-design.md) —
  stale: lists crates that have been removed.
- [01 Microkernel and shared memory](01-architecture/01-microkernel-and-shm.md) — the tag bus
  in `crates/mythos-shm`.
- [02 Protocol driver SPI](01-architecture/02-protocol-driver-spi.md) — stale: predates the
  contract in `crates/mythos-plugin-sdk`, in which a driver returns samples and never writes the
  tag bus itself.
- [05 HTTP, SSE and WebSocket API](01-architecture/05-http-sse-and-websocket-api.md) — describes
  more endpoints than `crates/mythos-core/src/api` serves.
- [06 Universal plugin harness](01-architecture/06-universal-plugin-harness-and-extensibility.md)
  — superseded by [ADR-010](../specs/adr/ADR-010-modules-are-processes.md): modules are
  processes, not loaded libraries.
- [07 Cross-platform HAL and containerization](01-architecture/07-cross-platform-hal-and-containerization.md)
  — `crates/mythos-pal`.
- [08 Installers and distribution](01-architecture/08-one-click-installers-and-distribution.md)
  — a design; no release has been published.

## Data and addressing (`docs/02-data-and-addressing/`)

- [Tag namespace rules](02-data-and-addressing/tag-namespace-rules.md)
- [Access mode matrix](02-data-and-addressing/access-mode-matrix.md)
- [UDT and memory retention](02-data-and-addressing/udt-and-memory-retention.md) — the
  retention tiers are a design; the NVRAM engine is not in the tree.
- [Project file and directory structure](02-data-and-addressing/project-file-and-directory-structure.md)

## Device catalog (`docs/03-network-and-catalog/`)

- [Device package schema](03-network-and-catalog/device-package-schema.md) — the manifest
  format parsed by `crates/mythos-core/src/catalog`. Reference packages:
  `catalog/devices/supmea-p300/` and `catalog/devices/sentec-rainfall/`.

## HMI (`docs/04-hmi-and-ui/`)

These describe the previous workbench, which has been replaced. The current one
(`frontend/packages/ui-shell`) follows a portal view / project view layout and is described by
its own source.

- [Frontend application architecture](04-hmi-and-ui/00-frontend-application-architecture.md)
- [Design tokens](04-hmi-and-ui/design-tokens-dtcg.json)
- [Network topology view](04-hmi-and-ui/network-topology-view-spec.md)
- [Faceplate binding](04-hmi-and-ui/faceplate-binding-spec.md)
- [Inspector drawer layout](04-hmi-and-ui/inspector-drawer-layout.md)
- [Operator setpoint reconcile rule](04-hmi-and-ui/operator-setpoint-reconcile-rule.md)
