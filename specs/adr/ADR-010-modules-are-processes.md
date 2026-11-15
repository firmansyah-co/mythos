# ADR-010: Everything Is a Module, and a Module Is a Process

- **Status**: ACCEPTED
- **Date**: 2026-11-16
- **Requested by**: Adrian ("pastikan semua program yang kita buat menggunakan prinsip everything
  is a module dan hotswappable"; "pastikan semua fitur aplikasi ini ada MCPnya")
- **Written by**: Claude, implementing the request

Supersedes the loading model of ADR-008 (a plugin harness that was never built: nothing in the
tree loaded a plugin dynamically).

---

## 1. Context

On 2026-11-16 every driver was compiled into `mythos-server` (`DriverRegistry::builtin()`), the
workbench carried a form written for Modbus alone, HMI element kinds were a fixed `switch`, and
saving an I/O configuration stopped and restarted every connection. Adding or upgrading a driver
meant rebuilding and restarting the server, and a driver that panicked or hung took field I/O
down with it.

## 2. Decision

### 2.1 A module is a separate executable

Every unit of functionality that can be added, removed or upgraded independently is a
**module**: an executable named `mythos-module-<name>`, started by the server, speaking a
line-oriented JSON protocol on its standard streams (`crates/mythos-module`).

- `<exe> --manifest` prints what the module is: id, version, kind, the name it provides, the
  capabilities it needs, and the JSON Schema of its configuration.
- `<exe> --serve` serves requests (`hello`, `describe`, `start`, `poll`, `write`, `health`,
  `stop`) until stdin closes.

The server finds modules by reading the directories in `MYTHOS_MODULE_PATH`, or the directory
it runs from. Nothing is compiled in.

**Why a process and not a shared library.** Rust has no stable ABI: a `dyn ProtocolDriver`
loaded from a `.so` is valid only for the exact compiler and dependency versions of the host,
and unloading one (`dlclose`) is unsound while any of its code, statics or thread-locals are
live. A process can be stopped, killed and replaced at any time; it cannot corrupt the
server's memory; and it can be written in any language that reads and writes lines of JSON.
The cost is a copy per poll cycle, which at field-bus rates (milliseconds, tens to thousands of
tags) is negligible.

### 2.2 The tag bus keeps one writer

Modules never write the tag bus. A driver module answers `poll` with samples; the server's
scanner writes them. This keeps the invariant of ADR-002 and the quality rules of the read path
in one place: a module cannot publish to a slot it does not own, and cannot mark a value GOOD.

### 2.3 A module that dies is replaced; the server is not affected

When a module process exits or stops answering, every tag it feeds becomes `BAD_COMM_FAILURE`
(the last value stays, marked bad), and the host starts a new process with the same
configuration, no more often than every two seconds. A replacement that declares different
tags than the one it replaces is refused.

### 2.4 Hot-swap is per unit

- Changing one connection's configuration restarts that connection only.
- Replacing a module executable with a new build swaps the running instances of that module
  for new ones; a new instance that cannot start leaves the old one running.
- HMI screens are released as data and followed by the runtime without a reload.

### 2.5 Every feature is reachable over MCP

Every HTTP route of the API has a Model Context Protocol tool, or an entry in an explicit list
of routes that cannot be one (streams, the WebSocket collaboration room, static files), with
the reason. A test fails when a route has neither. A tool calls its route through the same
router, so an agent and the workbench cannot get different behaviour.

### 2.6 The workbench draws module configuration from the manifest

A driver's connection form is drawn from the `config_schema` of its manifest. Adding a driver
module adds its form without a workbench change.

## 3. Consequences

- A driver crate ships a module executable (`driver-modbus` → `mythos-module-modbus`). Its
  in-process type remains, for tests and for embedding.
- A server installation is the server executable plus the module executables beside it. The
  desktop shell must bundle them as sidecars.
- The API has no authentication (see the root README). MCP access is as open as HTTP access;
  neither should be exposed to an untrusted network.

## 4. Status of implementation

The root README's status table is the record of what runs. When this ADR was accepted:

| Section | Built |
| :--- | :--- |
| 2.1 Modules are executables, discovered | Yes: `crates/mythos-module`, `mythos-module-modbus` |
| 2.2 One tag bus writer | Yes (unchanged) |
| 2.3 A dead module is replaced | Yes: `ProcessDriver` |
| 2.4 Hot-swap per unit | Yes: `IoRuntime::apply`, `IoManager::rescan` and its watcher |
| 2.5 MCP for every route | Yes: `crates/mythos-core/src/mcp.rs`, enforced by `every_route_of_the_api_is_a_tool_or_says_why_not` |
| 2.6 Forms from the manifest | Yes: `frontend/packages/ui-shell/src/data/schemaForm.ts` |
