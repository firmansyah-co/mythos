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

On 2026-11-16, before this decision, `crates/mythos-core` held, beside the core:

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

A module may ship a `ui/` directory with an ES module entry, `entry.js`. The core serves it at
`/modules/<id>/ui/…` to anyone signed in, only while the module is enabled, and lists it as the
module's `ui` in `GET /api/v1/modules`. The workbench shell imports the entries of the enabled
modules after sign-in and takes them down at sign-out.

An entry's default export is a `WorkbenchModule` (`@mythos/workbench-sdk`): an id, which must
be the module's, and `activate(host)`. Through the host it registers project-tree sections,
editor kinds, and full-window views (the operator runtime), and sets inspector panels and task
cards while an editor of its own wants them. The host gives it the open project, the signed-in
user, requests to the controller as that user, the live tag values and the named events of the
telemetry stream (its own included), and the session log.

Every contribution is a **mount**: the shell gives it an element and gets back what takes it
down. A module brings whatever it draws with (React, plain DOM, …); no code of the shell is
shared with it at run time, so a module keeps working across rebuilds of the shell and need not
use the shell's React version. The shell's CSS is shared: a module that uses its class names
looks like the rest of the workbench. The shell knows no HMI or logic code of its own (once
step 4 is done).

A module cannot take a name the shell or another module has: the shell's own editor kinds, the
address words `project`, `settings` and `portal`, or an editor kind, view or tree section another
module registered. A module whose entry fails to load or to activate is reported in the session
log and takes nothing else down with it.

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
| 2.1 Core boundary | HMI out of the core and the shell (step 4); data types and the deploy route are still in the core until step 5 |
| 2.2 Service modules | Yes: `mythos_module::service` (both sides), `crate::services` (host, forwarding, roles, data directories, events, swap), MCP tools from manifests, `needs: ["tags"]` for a route that checks against the tag catalog; proven by `crates/mythos-module-tests/tests/service_module.rs` with a test module. The HMI is the first real one |
| 2.3 Module UI | Yes: `api/modules.rs` (`ui_file`, `ModuleRow.ui`), `frontend/packages/workbench-sdk`, `ui-shell/src/modules` (loader, registries, mounts in the tree, work area, inspector, task cards and a full-window view; stream events by any name). Proven by `module_packages.rs` (serving), `moduleUi.test.tsx`, and the e2e scenario with the notes test module's UI (`crates/mythos-module-tests/ui/entry.js`) and the HMI's |
| 2.4 Enable/disable, packages | Yes: `crate::modules`, `api/modules.rs`, Settings → Modules, `mythos module pack`; `.mythos-plugin` retired. Proven by `crates/mythos-module-tests/tests/module_packages.rs` |
| 2.5 Library | No |
| §3 step 4, HMI module | Yes: `plugins/hmi` (`mythos-module-hmi`: screens and faceplates at `/api/v1/projects/:id/hmi/…`, data in `<project>/hmi/`, earlier files copied in on first use, `SCREENS_RELEASED` and `FACEPLATE_PROPAGATED` sent as module events, faceplate bindings checked against the tag catalog through `needs: ["tags"]`) and `frontend/packages/hmi-ui` (tree section, screen editor with toolbox and element properties, operator runtime at `#/runtime/<project>`). Shipped in every release bundle (`bin/`, `modules-ui/org.mythos.hmi/`). The screen URLs are unchanged; the faceplate routes moved from `/api/v1/projects/:id/faceplates/…` to `…/hmi/faceplates/…`, because a module's routes live under its own name (§2.2); nothing but MCP called them, and the MCP tool names are unchanged. Proven by `crates/mythos-module-tests/tests/hmi_screens.rs` and `hmi_faceplates.rs` (the core's former tests, run against the module process), the HMI UI tests run against the workbench's own host, and the e2e scenario unchanged |
