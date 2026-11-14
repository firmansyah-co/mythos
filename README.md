<p align="center">
  <img src="assets/mythos-banner.svg" width="100%" alt="MythOS Banner" />
</p>

# MythOS

An open-source SCADA platform in Rust, with a web engineering workbench. **Early development:
it does not yet read a value from a field device, and it is not fit to run a plant.**

<p align="center">
  <sub>Engineered by <a href="https://github.com/adrianaryaputra">adrianaryaputra</a> for <a href="https://firmansyah.co">firmansyahco</a></sub>
</p>

## Status

This section is the status of the project. Everything under `docs/` and `specs/` is design
intent and may describe things that do not exist.

### What runs

| Piece | Where | State |
| :--- | :--- | :--- |
| Tag bus | `crates/mythos-shm`, `crates/mythos-pal` | Triple-buffered shared-memory tag store with typed values and OPC-style quality codes. Tested. Nothing writes process values to it yet. |
| API server | `crates/mythos-core` (`mythos-server`) | HTTP API for projects, HMI screens, data types and faceplate types; a 100 ms SSE telemetry stream that reports whatever the tag bus holds. |
| Web workbench | `frontend/` | Project portal, HMI canvas, faceplates, logic and topology editors. Being rebuilt. |
| Modbus TCP driver | `plugins/drivers/driver-modbus` | Encodes and decodes Modbus TCP frames and polls registers. Not yet connected to the tag bus. |
| Device catalog | `catalog/`, `crates/mythos-core/src/catalog` | Manifest format and parser, with two reference device packages. |
| Desktop shell | `src-tauri` | A Tauri window that embeds the same API server. |
| Collaboration | `crates/mythos-core/src/collab` | A Yjs (`yrs`) WebSocket room. Not verified end to end. |

### What does not exist

- **A field data path.** No driver delivers values to the tag bus, so every telemetry batch is
  empty and every HMI value is `NOT BOUND` or bad quality.
- **A logic engine.** There is no IEC 61131-3 compiler or runtime. `POST /api/v1/control/deploy`
  answers `501` and compiles nothing.
- **Writes to field devices** from the API or the HMI.
- **Alarms, historian, recipes, device discovery, disaster recovery.**
- **Authentication and access control.** The API has none and the server listens on every
  interface. Do not expose it to a network you do not trust.
- **An AI / MCP interface.**
- **Modbus RTU and MQTT.** The Modbus driver's RTU transport is a stub that answers a
  constant, not field data.
- **Releases and installers.** Nothing has been published. The `mythos` CLI's `update`,
  `versions` and `rollback` commands work against a built-in version list, not a release feed.

## Repository layout

| Path | Contents |
| :--- | :--- |
| `crates/mythos-pal` | Platform abstraction: shared memory, scheduling, dynamic libraries. |
| `crates/mythos-shm` | The tag bus. |
| `crates/mythos-core` | API server, project storage, tag catalog, device catalog parser, HMI registries. |
| `crates/mythos-plugin-sdk` | Plugin and protocol-driver traits. |
| `crates/mythos-cli` | The `mythos` command-line tool. |
| `plugins/drivers/driver-modbus` | Modbus driver. |
| `src-tauri` | Desktop shell. |
| `frontend/` | Web workbench (pnpm workspace). |
| `catalog/` | Device packages. |
| `docs/`, `specs/` | Design documents and the requirements baseline. See [`docs/README.md`](docs/README.md). |
| `rig/` | Definition of the agent team that works on this repository. |

## Building from source

Requirements: Linux with `/dev/shm`, Rust 1.82 or newer, Node.js 22 and pnpm.

```bash
git clone https://github.com/firmansyah-co/mythos.git
cd mythos

cargo build --bin mythos-server

cd frontend
pnpm install --frozen-lockfile
pnpm build
cd ..
```

Run the server and open `http://localhost:8080`:

```bash
MYTHOS_STORAGE_ROOT="$PWD/.mythos-data" \
MYTHOS_STATIC_DIR="$PWD/frontend/packages/ui-shell/dist" \
  ./target/debug/mythos-server
```

| Variable | Default | Meaning |
| :--- | :--- | :--- |
| `MYTHOS_STORAGE_ROOT` | `/var/lib/mythos/projects` | Where projects are stored. |
| `MYTHOS_HTTP_PORT` | `8080` | Listening port. |
| `MYTHOS_STATIC_DIR` | unset | Directory of the built web workbench to serve. |

## Verifying a change

```bash
./scripts/check.sh    # cargo fmt, clippy, test; pnpm lint, test, build
```

See [`CLAUDE.md`](CLAUDE.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md) for the working rules.

## License

Mozilla Public License 2.0. See [LICENSE.md](LICENSE.md).

```
Copyright (c) 2026 firmansyahco
Author: adrianaryaputra <adrian@firmansyah.co>
```
