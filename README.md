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
| Service modules | `crates/mythos-module/src/service.rs`, `crates/mythos-core/src/services.rs` | A module of kind `service` (ADR-011) declares its routes in its manifest, each with the least role it needs. The server mounts them under `/api/v1/projects/<id>/<module>/…` or `/api/v1/m/<module>/…`, refuses a caller without the role before the module sees the request, gives the module its own data directory per project (`<project>/<module>/`), relays the events it sends on the telemetry stream, and makes every declared route an MCP tool. Its process is started on first use, started again when it dies, and replaced by a new build without dropping requests. A route that must check against the tag catalog declares `needs: ["tags"]` and is given the registered tag URIs with each call. The HMI is one (`plugins/hmi`). |
| Node | `crates/mythos-core/src/node.rs`, Settings → Node | Every installation is a node (ADR-012): `<storage root>/.mythos-system/node.json` holds an id that never changes, a name, a site and a role — `workstation` (the desktop app: listens on this computer only), `edge` (`mythos-server`'s default: every interface) or `server`. `MYTHOS_ROLE` chooses the role on first start, `MYTHOS_BIND` overrides where the server listens. The network uses HTTPS with the node's own certificate (`--https-port`, `MYTHOS_HTTPS_PORT`, default 8443): a browser on another computer opens `https://<node>:8443` and accepts the certificate once, after comparing its fingerprint with the one Settings → Node shows. Plain HTTP (`--port`, default 8080) is answered on the node's own computer only, unless `http_on_network` is set. A server reads no field devices unless told to (`field_io`), and says so in the I/O status. `GET/PUT /api/v1/node`; a new role's listening address applies on the next start, and the page says so. `mythos service install [--role edge] [--port 8080] [--storage DIR] [--user] [--dry-run]` runs `mythos-server` at boot under systemd (Linux), launchd (macOS) or Task Scheduler as SYSTEM (Windows), restarted when it stops; `mythos service uninstall` removes it and keeps its storage; `mythos service status` asks the operating system. |
| Discovery | `plugins/discovery` (`mythos-module-discovery`) | An always-on service module (ADR-012 §2.2): announces this node over mDNS (`_mythos._tcp`, no Avahi or Bonjour needed) when its role or setting says so — an edge does — and lists the MythOS nodes it hears at `GET /api/v1/m/discovery/nodes`, shown in the Portal as "MythOS on this network". A node heard is a candidate: nothing connects to it or trusts it because it was heard. Nothing secret is announced. |
| Pairing | `crates/mythos-core/src/pairing.rs`, Settings → Pairing | Two nodes trust each other only once an administrator pairs them (ADR-012 §2.3): one makes a one-use pairing code (ten minutes, granting a role); the other gives that node's address, its certificate fingerprint (compared by a person) and the code, and reaches it over TLS pinned to that fingerprint, so a node presenting another certificate is refused before the code leaves. Each then acts on the other with the role it was granted (`mythos_pair_` tokens, stored only as hashes on the side that accepts them). Unpairing on either side reaches both. |
| Plant projects | `crates/mythos-core/src/plant.rs`, the Plant editor | One project engineers a plant of MythOS devices (ADR-012 §2.4): each device is a project of its own, assigned to this node or a paired one. Download sends a device's files to its node over the pairing; the node checks them, keeps what it ran, and runs the new ones, or runs the old ones again. Download all reports each device. Compare says whether a node runs exactly what the project says, and never says so when it cannot ask. A download needs `confirm: true`. |
| Tags across devices | `crates/mythos-core/src/remote.rs`, the Plant editor | A node running a plant reads the tags of its other devices over the pairing (ADR-012 §2.6, read only), named with the device: `edge1:external.Boiler.U1.p`, in the telemetry stream and `GET /api/v1/tags/values` like its own. While a link is down they are `BAD_COMM_FAILURE` with no value, never their last value. `GET /api/v1/remote` (MCP `list_remote_devices`) and the Plant editor say whether each link is live. |
| Fleet and call home | `crates/mythos-core/src/callhome.rs`, Settings → Fleet | Every paired node asked now: how it is reached, its version, what it runs (ADR-012 §2.6). A node the others cannot reach (another subnet, NAT, a workstation) is marked on its pairing to call home: it long-polls the node it pairs with over pinned HTTPS and is reached over that, for status, compare and download. A request given up on is never run late. |
| Settings → Modules | `crates/mythos-core/src/modules.rs`, the workbench | Every module found, with its state. An administrator can disable a module without deleting it (a disabled driver's connections stop with `CONFIG_ERROR` naming why; a disabled service answers 503), enable it again, install one from a package (a zip with `module.json`, a SHA-256 of every file and an executable per platform, built by `mythos module pack <dir>`), and remove an installed one. A package is refused unless every file matches its checksum, no entry lies outside its directory, it has an executable for this platform, and that executable answers `--manifest` with the package's manifest. Packages are installed into `<storage root>/.mythos-system/modules/`, which updates of MythOS leave alone. |
| Module UI | `frontend/packages/workbench-sdk`, `frontend/packages/ui-shell/src/modules` | A module may ship a workbench UI (`ui/entry.js`, ADR-011 §2.3). The server serves it at `/modules/<id>/ui/…` while the module is enabled; the workbench imports it after sign-in, and the module adds project-tree sections, editors, full-window views, inspector panels and task cards, each drawn with whatever the module bundles. A UI that fails to load is named in the session log; the rest of the workbench works without it. |
| Writes to field devices | `crates/mythos-core/src/io/runtime.rs`, `PUT /api/v1/tags/<uri>/value` | An operator writes a number or a boolean to a tag whose driver declares it writable (`access_mode` `READ_WRITE`; for Modbus: a holding register of one register). The write runs in the connection's own task, between two poll cycles, with the poll's deadline. The answer says what is known: `written: true` when the device confirmed, `false` when nothing reached it (unknown tag, read-only tag, a value that does not fit), `"unknown"` when a request went out and no confirmation came back. A write never changes what a tag reads: the next poll does. Each write is logged with who made it. MCP tool `write_tag`. |
| Runtimes | `crates/mythos-core/src/runtimes.rs`, `plugins/runtime-vm` (`mythos-module-runtime-vm`), Settings → Runtimes | A project's `runtimes.json` says which control programs this node executes (ADR-013 §2.4). Each runs in a runtime module process: the VM runtime scans a program compiled to logic VM bytecode on its own clock. The server gives it the tags it reads, puts what it publishes on the tag bus as `internal.` tags, and writes its outputs to field devices. An output belongs to its program: an operator's write to it is refused, and a setpoint is a published variable marked writable. Whenever a program is not running (stopped, faulted, its process dead or silent, or unloaded) every output is written its safe value until the device confirms, and its tags are bad and say why. A program that dies is started again from a cold start. A program built before an update of the runtime module is still taken after it: the VM reads every bytecode format it extends (ADR-013 §2.3). `GET /api/v1/runtimes`, `POST /api/v1/runtimes/<id>/run` and `/stop`, `GET`/`PUT /api/v1/projects/<id>/runtimes`, each an MCP tool. `POST /api/v1/runtimes/<id>/watch` (any signed-in role; MCP `watch_runtime`) reads the values of a program's variables as its last scan left them, bound to a tag or not, one element of an array as `name[2]`, a STRING as its characters: all of one scan, with the program's state and the values' age, and an error in place of a value for a variable the program does not have. It changes nothing in the program. Settings → Runtimes in the workbench reads that list about once a second and shows, for every runtime, its module, program and version, its mode as reported with the reason given, its scan figures, and each output with its safe value and what the device last confirmed; an engineer runs and stops a program there (Run asks first: it is a cold start), and the page lists the runtime modules installed with the profile each declares. A value the server did not send is `UNKNOWN` or `NOT MEASURED`, and when a read fails or is not answered the rows are marked as the last known state with their age. Loading a program is not on the page: it is the API's. This is not a safety system: if the server itself stops, nothing writes anything. |
| Logic module | `plugins/logic` (`mythos-module-logic`), `frontend/packages/logic-ui` | Authoring of control programs, as a service module (ADR-013 §2.6) under `/api/v1/projects/<id>/logic/…`: a project's programs in Structured Text (`<project>/logic/programs/<name>.st`), each with its interval, its runtime and target, and its bindings of variables to tags (`<project>/logic/programs.json`). `check` compiles a program, or a text not yet saved, and answers with the compiler's diagnostics (line and column) or the program's variables and what its bindings get wrong. `build` compiles the saved source and packs it as a runtime instance; saving that instance into the project's runtimes (`PUT /api/v1/projects/<id>/runtimes`) is what loads it, so nothing that runs changes until then. A build's version is a hash of its code and bindings. Every route is an MCP tool. Its workbench UI adds Programs to the project tree and the program editor: Structured Text with the compiler's diagnostics marked at their line and column, the task settings, a table of the program's variables with their bindings to tags (an output has no safe value until one is given; a STRING cannot be bound, as a tag holds no text) and each bound tag's live value, what the controller's runtime of the program is doing, and each variable's value in the running program (a value read from a program that is not in RUN is marked as what it stopped with). Load saves, checks and builds the program and puts it into the project's runtimes; Run, Stop and Unload act on the runtime. When the runtimes cannot be read the editor says UNKNOWN, or marks what it last read as last known. |
| Field I/O runtime | `crates/mythos-core/src/io` | Each connection is a unit of its own: saving a configuration changes only the connections that differ, and one whose tags stay the same is swapped to its new settings without its tags stopping. Polls each configured connection and writes what the driver reports to the tag bus: a typed value with `GOOD` quality, or the last value read with a bad quality that names the fault. A tag that was never read has no value. |
| HMI module | `plugins/hmi` (`mythos-module-hmi`), `frontend/packages/hmi-ui` | HMI screens (drafts, release, sub-screens) and faceplate types and instances, as a service module (ADR-011 step 4) at the URLs they always had, `/api/v1/projects/<id>/hmi/…`, with its data in `<project>/hmi/`. A project whose screens and faceplates were saved by an earlier version keeps them: the module copies them in the first time it serves the project and leaves the originals. Its workbench UI is the screen editor, its toolbox and element properties, the project tree's HMI screens, and the operator runtime (`#/runtime/<project>`). Disabled, the workbench has no HMI and the routes answer 503. |
| API server | `crates/mythos-core` (`mythos-server`) | HTTP API for projects, data types and each project's I/O configuration, and the routes of service modules; a 100 ms SSE telemetry stream of the tag bus; `GET /api/v1/io/status` for the state of each connection. |
| Web workbench | `frontend/packages/ui-shell` | Portal view (open or create a project) and project view: project tree, devices & networks, a connection editor whose driver form is drawn from the driver module's JSON Schema (a new driver module brings its own form; a configuration without a drawable schema is shown and saved back as stored), live tag tables, connection diagnostics, inspector, Settings, and whatever the modules' UIs add (the HMI module: HMI screens with text fields and I/O fields bound to tags, saved as a draft and released, and an operator runtime that shows the released screens full window and follows a new release without a reload). |
| Modbus TCP driver | `plugins/drivers/driver-modbus` (module `mythos-module-modbus`) | Reads holding and input registers as `u16`, `i16`, `u32`, `i32` or `f32`, with scaling and selectable word order. Exercised end to end against a simulated device, not yet against hardware. |
| MCP endpoint | `crates/mythos-core/src/mcp.rs` | `POST /mcp`, the Model Context Protocol over Streamable HTTP. Every API route is a tool (61), or is listed with the reason it is not (the telemetry stream, static files); a test reads the router and fails on a route that is neither. A tool sends its route's own request through the same router. Checked with the official MCP TypeScript SDK client. |
| Platforms | `crates/mythos-pal`, CI | Linux, macOS and Windows. CI builds every Rust crate and runs the whole Rust test suite on each, the module and tag bus tests included. The tag bus is POSIX shared memory on Linux and macOS and a paging-file mapping on Windows. Real-time thread priority and core pinning exist on Linux and Windows; on macOS they report that they are not implemented. |
| Accounts | `crates/mythos-core/src/auth`, Settings in the workbench | Every route requires an account with enough of a role: viewer (reads), operator (also writes values to field devices), engineer (changes projects), admin (manages accounts). The workbench signs in with a session cookie; programs and MCP agents use personal access tokens. A new controller prints a one-time setup code in its log for creating the first administrator. Passwords are Argon2id hashes; sessions live in memory, so a restart signs everyone out. |
| Device catalog | `catalog/`, `crates/mythos-core/src/catalog` | Manifest format and parser, with two reference device packages. |
| Desktop shell | `src-tauri` | A Tauri window that embeds the same API server. |

### What does not exist

- **Any driver other than Modbus TCP.** A Modbus RTU configuration is rejected; there is no
  MQTT driver.
- **Screen elements beyond a text field and an I/O field.** No faceplates, graphics, trends or
  alarm displays.
- **Editors for faceplate types, data types and networks in the workbench.** The server still
  stores faceplate types and data types; the workbench has no editors for them.
- **A function block diagram editor and sequential function charts.** A program is written, checked,
  bound to tags and loaded in the program editor (see Logic module above) and watched, stopped
  and run again in Settings → Runtimes, in a subset of Structured Text. A Ladder Diagram is
  edited and run too; a function block diagram runs and is watched live but is not yet edited
  in the workbench; sequential function chart is not built.
- **Writes to field devices from the HMI.** The API has one (`PUT /api/v1/tags/<uri>/value`,
  see below); no screen element writes yet.
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
| `crates/mythos-core` | API server, field I/O runtime, project storage, tag catalog, device catalog parser, data-type registry, accounts, module host. |
| `crates/mythos-plugin-sdk` | Plugin and protocol-driver traits. |
| `crates/mythos-module` | The module protocol, both sides of it, and module discovery. |
| `crates/mythos-module-tests` | Modules tested as real processes. |
| `crates/mythos-cli` | The `mythos` command-line tool. |
| `crates/mythos-logic-ir` | The intermediate representation of control logic: what every language produces and every target consumes (ADR-013). |
| `crates/mythos-logic-st` | The Structured Text front-end (a subset so far; STRING, the string, numeric and selection functions of the standard included): ST to the IR, or every independent mistake of the source, each with the range it is about. |
| `crates/mythos-logic-backend-vm` | The VM backend: the IR to bytecode. |
| `crates/mythos-logic-vm` | The VM that runs compiled logic: `no_std`, no allocation, for the PC runner and microcontrollers; it computes SQRT, LN, SIN and the other real functions itself, the same bits on every target. |
| `crates/mythos-logic-compiler` | Joins a front-end, a target profile and a backend. |
| `plugins/drivers/driver-modbus` | Modbus TCP driver, its module executable, and a device simulator (`sim` feature). |
| `plugins/hmi` | The HMI service module: screens and faceplates. Its workbench UI is `frontend/packages/hmi-ui`. |
| `plugins/runtime-vm` | The logic runtime module for a PC: runs compiled programs on the VM. |
| `plugins/logic` | The logic authoring module: programs, bindings, check and build. Its workbench UI is `frontend/packages/logic-ui`. |
| `src-tauri` | Desktop shell. |
| `frontend/` | Web workbench (pnpm workspace): the shell `packages/ui-shell`, the module contract `packages/workbench-sdk`, the HMI module's UI `packages/hmi-ui`, and the logic module's UI `packages/logic-ui`. |
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
mythos update                    # the pinned version, else the newest on the channel
mythos update --version 1.2.0    # a given version, once
mythos pin 1.2.0                 # stay on 1.2.0 (mythos unpin to follow the channel again)
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

The same is in the workbench, Settings → Updates: the version the server runs and the
installation it runs from (`<home>/versions/<version>/bin`, or `MYTHOS_HOME`), every published
version with its channel, date and release notes, and — for an administrator — following a
channel or staying on one version, installing, and rolling back. A server built from source runs
from no installation; it lists the releases and says it has nowhere to install them.

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
opens the operator runtime on it. It also compiles a small control program
(`frontend/e2e/fan.st`), loads it into the VM runtime over the API, and on Settings → Runtimes
sees it run, takes the browser off the network to see the page mark it as the last known
state, stops it and sees its output confirmed at its safe value, and runs it again.

See [`CLAUDE.md`](CLAUDE.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md) for the working rules.

## License

Mozilla Public License 2.0. See [LICENSE.md](LICENSE.md).

```
Copyright (c) 2026 firmansyahco
Author: adrianaryaputra <adrian@firmansyah.co>
```
