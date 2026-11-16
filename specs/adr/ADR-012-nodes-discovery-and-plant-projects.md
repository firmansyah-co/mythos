# ADR-012: Nodes, Discovery, Pairing and Plant Projects

- **Status**: ACCEPTED
- **Date**: 2026-11-16
- **Requested by**: Adrian ("mythos ini kan bisa jadi running di komputer engineer, di edge device,
  atau di server. gimana caranya user bisa pilih bakal jalan di runtime apa dan gimana fitur
  auto discoverablenya?"; "dalam satu plant bisa ada banyak device dengan mythos, apakah
  project kita bisa handle semuanya all in one atau harus satu-satu? kalau siemens kan bisa
  langsung semua tuh"; order: this before STruC++)
- **Written by**: Claude, implementing the request

Builds on ADR-009 (discovery yields candidates), ADR-010 (modules are processes) and ADR-011
(the core is small; everything else is a module).

---

## 1. Context

On 2026-11-16 a MythOS installation knows nothing about itself or about any other:

- `mythos-server` binds `0.0.0.0:<port>` over plain HTTP; the desktop app runs the same server
  embedded on `127.0.0.1:9090`. Neither has an identity, a name or a role.
- A **project is one controller's configuration**: its I/O connections, data types and the HMI
  module's screens. A server runs one active project. A plant with three MythOS devices is three
  unrelated projects; nothing relates a tag on one to a screen on another, nothing deploys to a
  device other than the one the workbench is connected to, and nothing compares what a device
  runs with what the engineer has.
- An engineer finds a controller by typing its address. Nothing announces one, and nothing
  establishes that the controller answering is the one meant: there is no TLS and no pairing.

The same software runs on an engineer's laptop, on an edge device in a panel, and on a server.
What differs is what each should do by default and who may reach it.

## 2. Decision

### 2.1 A node has an identity and a role

Every installation is a **node**. On first start it writes `<system>/node.json`:

| Field | Meaning |
| :--- | :--- |
| `id` | A random UUID, never changed. What pairing and plant projects refer to. |
| `name` | Human name, defaulting to the host name; changeable. |
| `role` | `workstation`, `edge` or `server` (below). |
| `site` | Optional free text (plant, area). |

`GET /api/v1/node` returns it to anyone signed in; changing name, site or role is an
administrator's, through Settings → Node and the matching MCP tools.

A role is **a preset, not a different product**. The binary is the same; the role chooses the
defaults below, and every default can be changed on its own:

| | `workstation` | `edge` | `server` |
| :--- | :--- | :--- | :--- |
| Where | an engineer's computer (the desktop app) | IPC, gateway, panel PC | on premises or cloud |
| Listens on | `127.0.0.1` | every interface | every interface |
| Field I/O (driver modules) | for simulation | yes | off |
| Announces itself (§2.2) | no | yes | no |
| Runs as | the desktop app | a system service | a system service |
| Holds | plant projects being engineered | the slice of a plant project deployed to it | plant projects (shared), the fleet (§2.6) |

The role is chosen when installing (`mythos install --role edge`, the installers' choice, the
desktop app being a workstation) and can be changed in Settings → Node. Changing what a node
listens on takes effect on restart, and the page says so.

### 2.2 Discovery: a module, yielding candidates

Discovery is a service module, `org.mythos.discovery` (ADR-011: not the core). It:

- **announces** the node over mDNS/DNS-SD as `_mythos._tcp`, with TXT records `id`, `name`,
  `role`, `version`, `protocol`, `port` and the TLS certificate's SHA-256 fingerprint. No secret
  is announced. It announces only when the node's role or its setting says so, on the
  interfaces chosen.
- **browses** for other nodes and lists them at `GET /api/v1/m/discovery/nodes` (an MCP tool),
  shown in the Portal as "MythOS on this network".

It uses a pure-Rust mDNS implementation, so it works the same on Linux, macOS and Windows with
no Avahi or Bonjour installed.

As ADR-009 holds for field devices, **a discovered node is a candidate**. Nothing connects to it,
trusts it or deploys to it because it was found; an engineer pairs it (§2.3). Across subnets,
NAT or the internet, mDNS does not reach: an address is typed, or the edge calls home to a
server (§2.6).

### 2.3 Pairing and TLS

Every node serves HTTPS with a certificate it generates on first start (self-signed, its
fingerprint in `node.json`); plain HTTP stays available on the loopback interface only.

**Pairing** makes two nodes, or a workbench and a node, trust each other:

1. The engineer chooses a node (discovered or typed) and is shown its fingerprint, to compare
   with the one the node shows on its own console or Settings → Node.
2. The engineer gives the node's pairing code (shown on the node; one-time, short-lived, the
   same way as the setup code) or signs in as one of its administrators.
3. The node issues a **pairing token** bound to the pairing node's id, with a role. Both keep
   the other's id and pinned fingerprint. A node whose certificate changes is refused until it
   is paired again, and says why.

A pairing can be listed and revoked on either side. Every request between nodes carries its
pairing token, and the receiving node applies the token's role exactly as for a user.

### 2.4 Plant projects: one project, many devices

A project describes **a plant**, not one controller:

```
project "Boiler House"
├── device edge-boiler-1     → node 7f3c… (paired)   I/O, logic, HMI screens
├── device edge-boiler-2     → not assigned yet
├── device control-room-hmi  → node a91e… (paired)   HMI screens only
└── shared: data types, faceplate types, the tag library
```

- A **device** is a slot in the project: a name and what that node is to run (its I/O
  configuration, the HMI module's screens, later its logic, and which modules it needs).
- A device is **assigned** to a node by its id, explicitly, from a paired node. Nothing is
  assigned because it was discovered.
- Shared definitions (data types, faceplate types) belong to the project and go to every device
  that uses them.
- The project lives on the workstation, or on a server for a team. A device holds only its own
  slice, as deployed.
- **Existing projects are not lost**: a project from before this decision opens as a plant
  project with one device, "this controller", assigned to the node it is on.

### 2.5 Download, download all, compare

- **Download** sends one device's slice to its node. The node stages it, checks that every
  module the slice needs is present and enabled at a compatible protocol, activates it
  atomically, and keeps the previous slice to return to. A download that fails leaves the node
  running what it ran, and says why.
- **Download all** downloads every assigned device and reports each one: done, refused (and
  why), or unreachable. It never stops halfway silently.
- A download to a node that is running asks for explicit confirmation naming the node and what
  changes. It is never implicit and never triggered by discovery.
- **Compare**: each slice has a content hash. The workbench shows, per device, whether the node
  runs exactly what the project says, something else, or cannot be reached. The latter two are
  never shown as "the same".

### 2.6 Tags across devices, and the fleet

- A tag of another device is named with its device: `edge-boiler-1:external.Boiler.U1.steam_temp`.
- A node reads another node's tags by subscribing to its telemetry stream over the pairing, read
  only. A write to another device's tag is declared in the project and goes through that node's
  own write path and policy.
- The honesty rule holds across the link: when the link is down, a remote value is shown as not
  current (`CONNECTION LOST`), never as its last value.
- A `server` keeps the **fleet**: the nodes paired to it, their state and what they run. An edge
  that a server cannot reach (another subnet, NAT) **calls home**: it opens the connection to the
  server, with an enrollment token, and the server reaches it over that connection.
- *As built (step 8):* the enrollment is the pairing itself. The node that cannot be reached can
  reach the server, so it pairs with it the ordinary way (a code made on the server, its
  fingerprint compared) and is marked to call it home; there is no second kind of credential.
  The connection is a long poll over the same pinned HTTPS, not a socket of its own: it passes
  every proxy that passes HTTPS, and needs nothing beyond what pairing already trusts.

## 3. Order of work

1. **Node identity and role**: `node.json`, `GET /api/v1/node`, Settings → Node, MCP tools; the
   desktop app is a workstation, `mythos-server` defaults to edge.
2. **Role presets and services**: what each role listens on and enables; `mythos install
   --role`, the system service on Linux, macOS and Windows.
3. **Discovery module**: announce and browse; the Portal's "MythOS on this network".
4. **TLS and pairing**: certificates, fingerprints, pairing codes and tokens, revocation.
5. **Plant projects**: devices in a project, assignment, migration of existing projects, the
   workbench per device.
6. **Download, download all, compare.**
7. **Tags across devices**, read only first.
8. **Fleet and call home** on a server.
9. Then the logic module (STruC++, ADR-011 step 5), whose programs belong to a device and are
   downloaded with it.

Each step leaves `dev` working: a single controller, as today, keeps working at every step.

## 4. Consequences

- One project engineers a whole plant, the way an engineer expects from TIA Portal: one place
  for every device, one download for all of them, and a comparison that says which one differs.
- Every connection between nodes is authenticated and encrypted; a node announcing itself on the
  network grants nothing.
- A node's identity outlives its address: re-addressing a device does not break the project.
- The tag URI grows a device prefix for remote tags; a tag without one is the node's own, so
  nothing existing changes meaning.
- mDNS works only within a broadcast domain; plants with routed OT networks rely on typed
  addresses or call home.

## 5. Status of implementation

| Section | Built |
| :--- | :--- |
| 2.1 Identity and role | Steps 1 and 2 built. `crate::node` (`node.json`: id, name, role, site, and `field_io`/`announce` of its own over the role's), `GET`/`PUT /api/v1/node` with MCP tools, Settings → Node; the role decides where the server listens (`MYTHOS_BIND` overrides) and whether it reads field devices (a server does not; a change applies at once, and the I/O status says why there are no connections). `mythos service install|uninstall|status` runs `mythos-server` (with `--storage-root`, `--port`) under systemd, launchd or Task Scheduler (as SYSTEM, restarted on failure); `--dry-run` prints it. Proven by `node_api_tests.rs`, `node_field_io_tests.rs` (a Modbus simulator), `server_binary_tests.rs`, `service.rs` tests for every platform, `service_roundtrip.rs` (a real install: CI on Windows and macOS, by hand on Linux), `node.test.tsx`, e2e. `announce` is read by the discovery module (step 3) |
| 2.2 Discovery | Step 3 built: `plugins/discovery` (`mythos-module-discovery`, always on: `always_on` in the manifest makes the server start it when found and again when it stops; told its node by `Call::Node` after hello and on every change), announcing `_mythos._tcp` when the node says so and listing others at `GET /api/v1/m/discovery/nodes` (MCP `list_network_nodes`), each a candidate; the Portal's "MythOS on this network" (`portalSection` in the SDK). Proven by its unit tests, `crates/mythos-module-tests/tests/discovery_module.rs` (two nodes hear each other over real multicast where `MYTHOS_TEST_MDNS=1`: CI's Windows and macOS runners, and by hand), e2e, release-smoke |
| 2.3 Pairing and TLS | TLS built (step 4a): `crate::tls` makes each node's self-signed ECDSA P-256 certificate on first start (`<system>/tls/`, key readable by its owner only), its SHA-256 fingerprint is shown in Settings → Node, in `GET /api/v1/node` and announced by discovery; `mythos-server` answers HTTPS on the role's address (`--https-port`, `MYTHOS_HTTPS_PORT`, default 8443) and plain HTTP on this computer only unless `http_on_network`; `tls::request_pinned` reaches another node only if it presents the expected certificate. Proven by `tls.rs` tests, `server_binary_tests.rs` (`https_presents_the_node_certificate`, a real HTTPS request pinned to the fingerprint read from the certificate file; another fingerprint is refused), e2e. Pairing built (step 4b): `crate::pairing` (one-use code, ten minutes, granting a role, used up after five wrong guesses; each node keeps the other's id, name, address, pinned fingerprint, the token it presents and only the SHA-256 of the token it accepts, in `pairings.json` readable by its owner only), `api/pairing.rs` (`POST /api/v1/pairing/code`, `POST /api/v1/pairing/accept` with the code as the only credential, `GET/POST /api/v1/pairings`, `GET /api/v1/pairings/:node/status`, `DELETE /api/v1/pairings/:node` which tells the other node), a paired node's `mythos_pair_` token acts with the role granted; Settings → Pairing, and "Pair…" in the Portal's network page. Proven by `pairing.rs` tests and `pairing_tests.rs`: two real servers pair over pinned HTTPS (a wrong fingerprint is refused before the code leaves; a code pairs once; each reaches the other as itself; unpairing reaches both), `pairing.test.tsx`, e2e |
| 2.4 Plant projects | Built (step 5): `crate::plant` (`plant.json`: devices, each a project of its own, `<plant>--<device>`, assigned explicitly to this node or a paired one; a project without one is the one device "this controller"), `GET /api/v1/projects/:id/plant`, `PUT`/`DELETE …/plant/devices/:device`, MCP tools; the Plant editor in the project tree. Shared definitions between devices (data types, faceplate types) are not yet carried across: each device's project holds its own |
| 2.5 Download and compare | Built (step 6): a device's slice is its project's files with a SHA-256 over path and content; `PUT /api/v1/slices/:project` on the node checks every path stays inside, checks the hash, stages, keeps the previous project and makes the new one current, or runs the previous again; `…/download` and `…/plant/download` need `confirm: true` and report each device (done, refused, unreachable, skipped); `…/compare` says same, differs, not_running, never_downloaded or unreachable, never the same when it cannot ask. Proven by `plant.rs` tests and `pairing_tests.rs::a_plant_downloads_a_device_to_its_paired_node_and_compares_it` (two real servers: never_downloaded, refused without confirm, done and running on the node, same, differs after a change, download all, unreachable when the node is gone), `plant.test.tsx`, e2e |
| 2.6 Tags across devices, fleet | Tags across devices built, read only (step 7): `crate::remote` follows the telemetry stream of each other device of the plant this node runs (from its `plant.json`, or from `plant-devices.json`, the plant's device table a device's slice now carries, so an assignment change shows as `differs` on every device), over HTTPS pinned to that node and with the pairing's token; their tags join this node's in the stream and in `GET /api/v1/tags/values` as `<device>:<uri>`, without a slot. A link is down when it cannot be opened, ends, or is silent for 3 s; then every tag heard from it is `BAD_COMM_FAILURE` with `value: null`, its name and class this build's for the code (what the other node called it is not trusted). `GET /api/v1/remote` (MCP `list_remote_devices`) and the Plant editor's "Its tags here" say connected, connecting or lost and why. Proven by `remote.rs` and `plant.rs` tests, `pairing_tests.rs::a_device_reads_another_devices_tags_and_never_shows_a_lost_value_as_current` (two real servers, a real Modbus device: the value arrives named with the device, a change on the field arrives, and when the other node dies the tag stays with no value and BAD), `plant.test.tsx`, e2e. Writing another device's tag (declared in the project, through its own write path) is not built. Fleet and call home built (step 8): `GET /api/v1/fleet` (MCP `list_fleet`, Settings → Fleet) asks every paired node now, all at once, how it is reached (`direct`, `call_home` or `none`), who it says it is, and what it runs (project and slice hash), and says why one does not answer, never what it said before. `crate::callhome`: a pairing marked `call_home` (`POST /api/v1/pairings` or `PUT /api/v1/pairings/:node`, MCP `set_call_home`, Settings → Pairing) makes this node long-poll `POST /api/v1/callhome` on that node, as itself; that node sends its requests over the poll, carrying its own pairing token so they are checked there exactly as if direct, and every node-to-node request (`ask`: pairing status, unpair, compare, download, fleet) goes over the call-home connection while one is live (a poll within 12 s), directly otherwise. A request whose asker gave up (timed out, or dropped) is never sent late, so a download reported as not done does not happen afterwards. Proven by `callhome.rs` tests and `pairing_tests.rs::a_node_that_cannot_be_reached_calls_home_and_is_reached_over_it` (two real servers; the edge is a workstation, so the server has no address for it: the fleet reaches it by its call home, a device is downloaded and compared `same` over it, only a paired node may poll; when it stops, the fleet says `none` and compare `unreachable`), `fleet.test.tsx`, `pairing.test.tsx`, e2e. Reading tags from a node that only calls home is not built: §2.6 tags follow a node's stream directly |
