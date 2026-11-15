<p align="center">
  <img src="assets/mythos-banner.svg" width="100%" alt="MythOS Banner" />
</p>

# MythOS

An open-source SCADA platform in Rust, with a web engineering workbench. **Early development:
it can read values from a Modbus TCP device and stream them to a browser, and nothing more. It
is not fit to run a plant.**

<p align="center">
  <sub>Engineered by <a href="https://github.com/adrianaryaputra">adrianaryaputra</a> for <a href="https://firmansyah.co">firmansyahco</a></sub>
</p>

## Status

This section is the status of the project. Everything under `docs/` and `specs/` is design
intent and may describe things that do not exist.

### What runs

| Piece | Where | State |
| :--- | :--- | :--- |
| Tag bus | `crates/mythos-shm`, `crates/mythos-pal` | Triple-buffered shared-memory tag store with typed values and OPC-style quality codes. One writer: the field I/O runtime. |
| Modules | `crates/mythos-module` | Every driver is a separate executable, `mythos-module-<name>`, that the server finds beside itself (or in `MYTHOS_MODULE_PATH`), starts as a child process and talks to over JSON lines on its standard streams. Nothing is compiled into the server. A module process that dies is replaced; its tags read `BAD_COMM_FAILURE` until it is back. A new build of a module put in the directory is noticed within two seconds and swapped in for the running one without its tags stopping; a new build that cannot start leaves the old one running. See [ADR-010](specs/adr/ADR-010-modules-are-processes.md). |
| Field I/O runtime | `crates/mythos-core/src/io` | Each connection is a unit of its own: saving a configuration changes only the connections that differ, and one whose tags stay the same is swapped to its new settings without its tags stopping. Polls each configured connection and writes what the driver reports to the tag bus: a typed value with `GOOD` quality, or the last value read with a bad quality that names the fault. A tag that was never read has no value. |
| API server | `crates/mythos-core` (`mythos-server`) | HTTP API for projects, HMI screens, data types, faceplate types and each project's I/O configuration; a 100 ms SSE telemetry stream of the tag bus; `GET /api/v1/io/status` for the state of each connection. |
| Web workbench | `frontend/packages/ui-shell` | Portal view (open or create a project) and project view: project tree, devices & networks, an editor for Modbus TCP connections and their tags, live tag tables, connection diagnostics, HMI screens (text fields and I/O fields bound to tags, saved as a draft and released), an operator runtime that shows the released screens full window and follows a new release without a reload, inspector. |
| Modbus TCP driver | `plugins/drivers/driver-modbus` (module `mythos-module-modbus`) | Reads holding and input registers as `u16`, `i16`, `u32`, `i32` or `f32`, with scaling and selectable word order. Exercised end to end against a simulated device, not yet against hardware. |
| Device catalog | `catalog/`, `crates/mythos-core/src/catalog` | Manifest format and parser, with two reference device packages. |
| Desktop shell | `src-tauri` | A Tauri window that embeds the same API server. |
| Collaboration | `crates/mythos-core/src/collab` | A Yjs (`yrs`) WebSocket room. Not verified end to end. |

### What does not exist

- **Any driver other than Modbus TCP.** A Modbus RTU configuration is rejected; there is no
  MQTT driver.
- **Screen elements beyond a text field and an I/O field.** No faceplates, graphics, trends or
  alarm displays.
- **A logic editor and a network editor in the workbench.** The server still stores faceplate
  types and data types; the workbench has no editors for them.
- **A logic engine.** There is no IEC 61131-3 compiler or runtime. `POST /api/v1/control/deploy`
  answers `501` and compiles nothing.
- **Writes to field devices** from the API or the HMI.
- **Alarms, historian, recipes, device discovery, disaster recovery.**
- **Authentication and access control.** The API has none and the server listens on every
  interface. Do not expose it to a network you do not trust.
- **An AI / MCP interface.**
- **Releases and installers.** Nothing has been published. The `mythos` CLI's `update`,
  `versions` and `rollback` commands work against a built-in version list, not a release feed.

## Repository layout

| Path | Contents |
| :--- | :--- |
| `crates/mythos-pal` | Platform abstraction: shared memory, scheduling, dynamic libraries. |
| `crates/mythos-shm` | The tag bus. |
| `crates/mythos-core` | API server, field I/O runtime, project storage, tag catalog, device catalog parser, HMI registries. |
| `crates/mythos-plugin-sdk` | Plugin and protocol-driver traits. |
| `crates/mythos-module` | The module protocol, both sides of it, and module discovery. |
| `crates/mythos-module-tests` | Modules tested as real processes. |
| `crates/mythos-cli` | The `mythos` command-line tool. |
| `plugins/drivers/driver-modbus` | Modbus TCP driver, its module executable, and a device simulator (`sim` feature). |
| `src-tauri` | Desktop shell. |
| `frontend/` | Web workbench (pnpm workspace; the application is `packages/ui-shell`). |
| `catalog/` | Device packages. |
| `docs/`, `specs/` | Design documents and the requirements baseline. See [`docs/README.md`](docs/README.md). |
| `rig/` | Definition of the agent team that works on this repository. |

## Building from source

Requirements: Linux with `/dev/shm`, Rust 1.82 or newer, Node.js 22 and pnpm.

```bash
git clone https://github.com/firmansyah-co/mythos.git
cd mythos

cargo build --bin mythos-server -p driver-modbus --bin mythos-module-modbus

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
| `MYTHOS_SHM_NAME` | `mythos_tagbus` | Name of the tag bus segment. Two servers must not share one: the tag bus has a single writer. |
| `MYTHOS_MODULE_PATH` | the server's own directory | Directories to look for module executables in, separated like `PATH`. |

`GET /api/v1/modules` lists the modules the server found, the schema of the configuration each
accepts, and any executable named like a module that could not be used, with the reason.
`POST /api/v1/modules/rescan` looks again at once; the server also does that by itself when a
module executable is added, removed or replaced.

## Reading a device

Field I/O is configured per project and runs for the **active** project. The configuration is
stored as `<storage root>/<project id>/io.json`.

Save it with `PUT /api/v1/projects/<project id>/io`. The server checks it first — its own
rules, each driver's view of its connection, and that no tag is provided twice — and refuses
it whole, naming every problem, if anything is wrong. A saved configuration of the active
project is applied at once, and only what differs changes: a connection that is unchanged keeps
running, one whose tags stay the same is swapped to the new settings without a gap, one whose
tags change is restarted, one that is gone is stopped and its tags removed. The answer's
`changes` lists which connection had which. Activating another project switches field I/O to that project the same way.
`GET /api/v1/projects/<project id>/io` reads it back and `GET /api/v1/io/drivers` lists the
drivers it may name. A file written by hand is picked up when the server starts or the project
is activated.

```json
{
  "connections": [
    {
      "id": "boiler-plc",
      "driver": "modbus",
      "poll_interval_ms": 500,
      "config": {
        "transport": "tcp",
        "host": "192.0.2.10",
        "port": 502,
        "slave_id": 1,
        "timeout_ms": 1000,
        "mappings": [
          { "uri": "external.BoilerRoom.PT101.pressure", "address": 100, "function_code": 3, "scale": 0.1 },
          { "uri": "external.BoilerRoom.TT101.temperature", "address": 102, "function_code": 3, "data_type": "f32" }
        ]
      }
    }
  ]
}
```

| Field | Meaning |
| :--- | :--- |
| `poll_interval_ms` | Required. How often the connection is polled. |
| `cycle_timeout_ms` | Optional. Longest a poll cycle may run before every tag of the connection is marked `BAD_COMM_FAILURE` and the driver is restarted. Default: five poll intervals, at least 2000. |
| `function_code` | `3` holding registers, `4` input registers. |
| `data_type` | `u16` (default), `i16`, `u32`, `i32`, `f32`. |
| `word_order` | For two-register types: `high_first` (default) or `low_first`. |
| `scale` | Multiplier applied to the decoded value. Any value other than `1` makes the tag a 64-bit float. |

`GET /api/v1/io/status` reports each connection, and `GET /api/v1/telemetry/stream` carries the
values. A connection that cannot be started is listed with the reason and provides no tags.

Without hardware, a simulated Modbus TCP device can stand in. **Its values are synthetic**; it is
a separate process that is only read when a connection is pointed at it:

```bash
cargo run -p driver-modbus --features sim --example modbus_sim -- 127.0.0.1:1502
```

## Verifying a change

```bash
./scripts/check.sh    # cargo fmt, clippy, test; pnpm lint, test, build
./scripts/e2e.sh      # a real server, a simulated device, and the workbench in a real browser
```

`e2e.sh` needs Chromium and is not run in CI. It adds a connection through the workbench's form,
has the controller refuse a bad one, saves a good one, reads live values off the page, then
draws an HMI screen, binds a field to a tag, drags an element, saves and releases it, and
opens the operator runtime on it.

See [`CLAUDE.md`](CLAUDE.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md) for the working rules.

## License

Mozilla Public License 2.0. See [LICENSE.md](LICENSE.md).

```
Copyright (c) 2026 firmansyahco
Author: adrianaryaputra <adrian@firmansyah.co>
```
