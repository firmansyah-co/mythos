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
| Modules | `crates/mythos-module` | Every driver is a separate executable, `mythos-module-<name>`, that the server finds beside itself (or in `MYTHOS_MODULE_PATH`), starts as a child process and talks to over JSON lines on its standard streams. Nothing is compiled into the server. A module process that dies is replaced; its tags read `BAD_COMM_FAILURE` until it is back. A new build of a module put in the directory is noticed within two to four seconds (a change is acted on once it has held still for one look, so a half-copied file is never started) and swapped in for the running one without its tags stopping; a new build that cannot start, or a module that goes missing, leaves the old one running. See [ADR-010](specs/adr/ADR-010-modules-are-processes.md). |
| Field I/O runtime | `crates/mythos-core/src/io` | Each connection is a unit of its own: saving a configuration changes only the connections that differ, and one whose tags stay the same is swapped to its new settings without its tags stopping. Polls each configured connection and writes what the driver reports to the tag bus: a typed value with `GOOD` quality, or the last value read with a bad quality that names the fault. A tag that was never read has no value. |
| API server | `crates/mythos-core` (`mythos-server`) | HTTP API for projects, HMI screens, data types, faceplate types and each project's I/O configuration; a 100 ms SSE telemetry stream of the tag bus; `GET /api/v1/io/status` for the state of each connection. |
| Web workbench | `frontend/packages/ui-shell` | Portal view (open or create a project) and project view: project tree, devices & networks, a connection editor whose driver form is drawn from the driver module's JSON Schema (a new driver module brings its own form; a configuration without a drawable schema is shown and saved back as stored), live tag tables, connection diagnostics, HMI screens (text fields and I/O fields bound to tags, saved as a draft and released; element kinds come from one registry that the toolbox, canvas, runtime and inspector all read), an operator runtime that shows the released screens full window and follows a new release without a reload, inspector. |
| Modbus TCP driver | `plugins/drivers/driver-modbus` (module `mythos-module-modbus`) | Reads holding and input registers as `u16`, `i16`, `u32`, `i32` or `f32`, with scaling and selectable word order. Exercised end to end against a simulated device, not yet against hardware. |
| MCP endpoint | `crates/mythos-core/src/mcp.rs` | `POST /mcp`, the Model Context Protocol over Streamable HTTP. Every API route is a tool (35), or is listed with the reason it is not (the telemetry stream, the collaboration WebSocket, the 501 deploy route, static files); a test reads the router and fails on a route that is neither. A tool sends its route's own request through the same router. Checked with the official MCP TypeScript SDK client. |
| Platforms | `crates/mythos-pal`, CI | Linux, macOS and Windows. CI builds every Rust crate and runs the whole Rust test suite on each, the module and tag bus tests included. The tag bus is POSIX shared memory on Linux and macOS and a paging-file mapping on Windows. Real-time thread priority and core pinning exist on Linux and Windows; on macOS they report that they are not implemented. |
| Accounts | `crates/mythos-core/src/auth`, Settings in the workbench | Every route requires an account with enough of a role: viewer (reads), operator (reserved for writing setpoints, which do not exist yet), engineer (changes projects), admin (manages accounts). The workbench signs in with a session cookie; programs and MCP agents use personal access tokens. A new controller prints a one-time setup code in its log for creating the first administrator. Passwords are Argon2id hashes; sessions live in memory, so a restart signs everyone out. |
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
- **Encrypted transport.** The server speaks plain HTTP and listens on every interface: a
  password or token crosses the network readable. Put it behind TLS (a reverse proxy) before
  exposing it to a network you do not trust.
- **A published release.** Nothing has been published yet, so `mythos update` has nothing to
  install and says so. The one-line installers (`scripts/install.sh`, `scripts/install.ps1`)
  still name download hosts that do not exist.

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

Requirements: Linux, macOS or Windows; Rust 1.82 or newer, Node.js 22 and pnpm. On Windows the
executables end in `.exe` (`target\debug\mythos-server.exe`) and the variables below are set with
`$env:NAME = "value"` in PowerShell.

```bash
git clone https://github.com/firmansyah-co/mythos.git
cd mythos

cargo build -p mythos-core -p driver-modbus --bin mythos-server --bin mythos-module-modbus

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
| `MYTHOS_STORAGE_ROOT` | Linux: `/var/lib/mythos/projects`; macOS: `/Library/Application Support/MythOS/projects`; Windows: `%ProgramData%\MythOS\projects` | Where projects are stored. |
| `MYTHOS_HTTP_PORT` | `8080` | Listening port. |
| `MYTHOS_STATIC_DIR` | unset | Directory of the built web workbench to serve. |
| `MYTHOS_SHM_NAME` | `mythos_tagbus` | Name of the tag bus segment. Two servers must not share one: the tag bus has a single writer. |
| `MYTHOS_MODULE_PATH` | the server's own directory | Directories to look for module executables in, separated like `PATH`. |

`GET /api/v1/modules` lists the modules the server found, the schema of the configuration each
accepts, and any executable named like a module that could not be used, with the reason.
`POST /api/v1/modules/rescan` looks again at once; the server also does that by itself when a
module executable is added, removed or replaced.

To replace a module, put the new build beside it under another name and rename it over the old
one. Windows does not let a running executable be replaced: rename the old build away first (to
any name that is not a module's, such as `mythos-module-modbus.old`), then move the new one in.
The connections using the module keep running while it is missing.

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

## Installing and updating

A release publishes one bundle per platform, `mythos-<version>-<target>.zip`, and a
`SHA256SUMS` file. The bundles are built for Linux (x86-64 and arm64), macOS (Apple silicon) and
Windows (x86-64) by `.github/workflows/release-assets.yml`, which also installs each one with
its own CLI and starts its server before attaching it to the release. A bundle holds `bin/`
(`mythos`, `mythos-server`, the driver modules) and `web/` (the workbench, which the server
serves from there when `MYTHOS_STATIC_DIR` is not set).

```bash
mythos update                    # newest release on the configured channel
mythos update --version 1.2.0    # a given version
mythos versions --remote         # what has been published, and whether this platform has a bundle
mythos versions                  # what is installed
mythos rollback                  # back to the previous version, which is still installed
mythos channel switch rc         # follow release candidates too (or dev for everything)
```

`update` reads GitHub's release list for this repository (`MYTHOS_RELEASE_FEED` or
`remote_url` in `~/.mythos/config.json` point it elsewhere). It refuses a bundle whose SHA-256
does not match `SHA256SUMS`, an archive with an entry outside its directory, and a bundle whose
`mythos` does not run on this machine; only then does it move `~/.mythos/current` (a symlink,
or a junction on Windows) to `~/.mythos/versions/<version>`. Put `~/.mythos/current/bin` on
`PATH`. No executable is ever overwritten, so updating while the server runs is safe; the
running server keeps its version until it is restarted from `current`. The checksum guards
against a damaged download, not against a compromised release: bundles are not signed yet.

## Connecting an AI agent

The server answers the Model Context Protocol at `/mcp`. Create an access token in the
workbench (Settings → Account → Access tokens), then in Claude Code:

```bash
claude mcp add --transport http mythos http://localhost:8080/mcp \
  --header "Authorization: Bearer mythos_pat_..."
```

The agent then has a tool for every route: projects, field I/O configuration and status, tag
values with their quality, modules, HMI screens, data types, faceplates, and accounts. It acts
with the token owner's role: a tool the role does not allow answers 403. It refuses requests
whose `Origin` is a web page on another host.

## First start

A controller starts with no account. Its log says:

```text
No account exists yet. Open the workbench and create the first administrator with setup code …
```

Open the workbench, enter that code, and choose the administrator's username and password.
Further accounts are made in Settings → Users. `MYTHOS_SETUP_CODE` sets the code instead of a
random one, for automated installs.

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
