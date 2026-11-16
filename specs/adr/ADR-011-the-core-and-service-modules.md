# ADR-011: The Core Is Small; HMI, Logic and Everything Else Are Service Modules

- **Status**: ACCEPTED
- **Date**: 2026-11-16
- **Requested by**: Adrian ("HMI itu harus modul terpisah. hanya API auth storage dan bagian core
  yang masuk core"; "apakah kita udah punya setting untuk load unload modul dan kedepannya
  persiapan untuk module library?")
- **Written by**: Claude, implementing the request

Extends ADR-010. ADR-010 made every *driver* a module process; everything else (HMI screens and
faceplates, data types, the deploy route) stayed compiled into `mythos-server`, and a module
could only feed tags.

---

## 1. Context

On 2026-11-16, `crates/mythos-core` holds, beside the core:

| Part | Where | Lines |
| :--- | :--- | ---: |
| HMI screens: drafts, release, sub-screens | `api/screens.rs` | 235 |
| Faceplate types and instances | `faceplate/`, `api/faceplates.rs` | 1257 |
| Data types (DUT/UDT, for logic) | `datatype/`, `api/datatypes.rs` | 658 |
| Logic deployment (answers 501) | `sdui/`, `api/mod.rs` | ~200 |

and the workbench bundle holds the screen editor, its element registry, the operator runtime
and the stores behind them. None of it can be added, removed, upgraded or replaced without
rebuilding and restarting the server and redeploying the workbench. There is no way to switch a
module off without deleting its file, no package to install one from, and no catalog to find
one in. The CLI's `.mythos-plugin` packages are installed into `~/.mythos/plugins`, which nothing
reads.

## 2. Decision

### 2.1 What the core is

The core is what every installation needs whatever it does, and what modules need to exist:

| Core part | Owns |
| :--- | :--- |
| API gateway | HTTP routing, the telemetry stream, serving the workbench shell |
| Accounts | sign-in, sessions, tokens, roles, the route policy (`auth`) |
| Storage | projects, the active project, atomic writes, each module's data directory |
| Tag bus | the shared-memory tag store and the tag catalog |
| Field I/O runtime | the scanner, the single tag-bus writer (ADR-010 §2.2) |
| Module host | discovery, processes, enable/disable, packages, hot swap |
| MCP gateway | the tools of every core route and every module route |
| Updates | versions, releases, installing (`updates`) |
| Workbench shell | sign-in, project tree, editors frame, Settings, the module UI loader |

Everything else is a module: drivers (ADR-010), **HMI**, **logic** (the STruC++ engine, data
types, the language editors), and whatever comes after.

### 2.2 A service module

A module's manifest says its `kind`. Beside `driver` there is `service`. A service module is a
process speaking the ADR-010 protocol, with these additions:

- **Routes.** The manifest declares each route it serves: method, path, the least role it
  needs, a summary, and the JSON Schema of its body. The core mounts them, checks the caller's
  role against what the manifest declares (a route that declares none is administrators-only),
  and forwards the request — method, path parameters, query, body and the caller's identity —
  as a protocol call; the module answers with a status and a body. Each declared route is an
  MCP tool, generated from the manifest, so ADR-010 §2.5 holds for modules without a core
  change. A module's routes live under its own name (`/api/v1/projects/:id/<name>/…` for project
  data, `/api/v1/<name>/…` otherwise); it cannot shadow a core route.
- **Data.** The core gives a service module, per call, the directory that holds its data for
  that project (`<storage root>/<project>/<name>/`) after validating the project id. The module
  owns the files in it; the core owns the directory's place. A module never opens another
  module's directory or the core's.
- **Events.** A module may send events (a screen released, a definition changed); the core
  relays them on the telemetry stream to every open workbench and operator station.
- **No tag-bus writes.** As for drivers, a service module never writes tags. Reading live values
  happens in the browser, from the core's telemetry stream.
- **Hot swap.** A new build of a service module is started beside the old one; when it answers
  `hello`, requests are routed to it and the old one is stopped. Its data is in files, so nothing
  is lost. Open workbenches are told the module changed and reload its UI.

### 2.3 Module UI

A module may ship a `ui/` directory with an ES module entry. The core serves it at
`/modules/<id>/ui/…`, and the workbench shell imports the entries of the enabled modules at
sign-in. An entry is built against `@mythos/workbench-sdk`, which gives it the shell's React and
the extension points: project-tree sections, editor kinds, toolbox items, inspector panels,
Settings sections, and full-window views (the operator runtime). The shell knows no HMI or
logic code of its own.

### 2.4 Switching modules on and off; installing them

Settings → Modules (administrators; every action is also an MCP tool) lists every module found:
manifest, version, platform, process, restarts, what uses it, and problems. A module can be
**disabled** without deleting it: the core stops its process and unmounts its routes and UI,
and whatever used it says so (a connection whose driver is disabled is `CONFIG_ERROR` naming
the reason; an HMI that is disabled shows no screens, not empty ones). A module is **installed**
from a package and **removed** from there.

A **module package** is a zip: `module.json` (the manifest), `bin/<target>/` with one
executable per platform, `ui/` when there is one, and a SHA-256 of each file. The core installs
the executable for its own platform into its module directory and checks that it answers
`--manifest` with the manifest of the package before enabling it. This replaces `.mythos-plugin`.

### 2.5 Module library (prepared, not built in the first step)

A library is a feed of module packages, in the shape of the release feed: id, versions,
platforms, protocol version, the least server version, checksums. Settings → Modules browses
it, installs and updates from it. Packages will be signed before a public library exists; until
then only packages from a feed the administrator configures are offered.

## 3. Order of work

1. **Service module host:** the `service` kind; routes with roles, forwarded calls, data
   directories, events, MCP tools from manifests; a small test service module that proves them on
   Linux, macOS and Windows.
2. **Settings → Modules:** list, disable/enable, install and remove packages; the package format;
   `.mythos-plugin` retired.
3. **Workbench SDK and module UI loading.**
4. **HMI module** (`plugins/hmi`): screens, faceplates and the operator runtime move out of the
   core and the shell, behind the same URLs and the same e2e checks.
5. **Logic module** (STruC++, data types, editors), built as a service module from the start.
6. **Module library.**

Each step leaves `dev` working: the e2e scenario (screens, release, runtime) passes before and
after step 4 unchanged.

## 4. Consequences

- The server binary shrinks to the core; an installation without an HMI (a headless gateway)
  is the core plus its drivers.
- A module's route is as protected as a core route: the policy is enforced by the core, from
  the manifest, before the module sees the request.
- A request to a module route costs a JSON round trip to its process. Screens and faceplates are
  edited at human speed; nothing on the field I/O path goes through it.
- A module that is not trusted is still a process with the server's rights: manifest
  capabilities are not enforced by an operating-system sandbox. A public library needs signed
  packages and, later, sandboxing.

## 5. Status of implementation

| Section | Built |
| :--- | :--- |
| 2.1 Core boundary | Decided; HMI, data types and deploy are still in the core |
| 2.2 Service modules | Yes: `mythos_module::service` (both sides), `crate::services` (host, forwarding, roles, data directories, events, swap), MCP tools from manifests; proven by `crates/mythos-module-tests/tests/service_module.rs` with a test module. No real service module yet |
| 2.3 Module UI | No |
| 2.4 Enable/disable, packages | Yes: `crate::modules`, `api/modules.rs`, Settings → Modules, `mythos module pack`; `.mythos-plugin` retired. Proven by `crates/mythos-module-tests/tests/module_packages.rs` |
| 2.5 Library | No |
