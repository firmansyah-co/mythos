# ADR-004: PROFINET DCP Layer 2 Discovery Architecture and Deterministic Fieldbus Timing

## Status
**ACCEPTED**

## Context
Industrial automation control architectures require deterministic commissioning, discovery, and physical maintenance workflows. When an edge IPC hosting MythOS is deployed into an operational factory, water treatment plant, or agricultural estate, it encounters heterogeneous physical networks:
1. Brand-new out-of-the-box decentralized I/O racks (e.g., Siemens ET200SP, Phoenix Contact Axioline, WAGO I/O) that possess unconfigured IP addresses (`0.0.0.0`) or factory-default subnets that conflict with the plant LAN.
2. Inability of Layer 3 IP protocols (ICMP ping, HTTP, SSDP, mDNS) to cross subnet boundaries or communicate with unaddressed network interfaces without prior routing and gateway setups.
3. Technicians struggling to locate a specific physical module among dozens of identical chassis in high-density control cabinets.
4. RS-485 serial fieldbuses (Modbus RTU) suffering from bus contention, non-deterministic turnaround delays, unhandled framing errors, and unbounded polling jitter that degrades overall SCADA loop cycles.

Previous software architectures relied on manual IP configuration through vendor-proprietary Windows software tools (such as TIA Portal or Proneta) or slow IP sweep tools that saturated the local network with ARP broadcast storms.

MythOS requires an enterprise-grade, deterministic, vendor-neutral network discovery engine implemented directly in pure Rust.

---

## Decision Drivers
- **Zero-Configuration Physical Commissioning**: Discover all nodes on the physical local Ethernet link regardless of current IP address, subnet mask, or VLAN tag.
- **Standards Compliance**: Alignment with IEC 61158-6-10 (PROFINET DCP) and IEEE 802.3 raw frame formats.
- **Interactive Field Maintenance**: Enable software operators to flash physical LEDs on specific modules from the HMI screen (Signal Service) and push permanent IP configurations over Layer 2.
- **Routable Fallback**: Provide seamless fallback to mDNS / DNS-SD (RFC 6762/6763) for routed corporate subnets and virtualized edge gateways.
- **Deterministic Fieldbus Timing**: Strict compliance with TIA/EIA-485-A, enforcing bounded inter-character spaces ($t_{1.5}$), inter-frame silences ($t_{3.5}$), guard turnaround intervals, and cyclic polling bounds (50ms–500ms).

---

## Considered Options

### Option 1: Full IP Range ARP Scanning (Brute Force)
- *Pros*: Simple socket-based implementation.
- *Cons*: Extremely slow (takes minutes to scan a `/16` or `/24` subnet), completely fails for devices with IP `0.0.0.0`, floods the switch backplane with ARP requests, cannot set IP addresses or blink LEDs.

### Option 2: Pure mDNS / Zeroconf (RFC 6762)
- *Pros*: Works across standard UDP sockets without elevated raw-socket privileges; human-readable service records.
- *Cons*: Requires devices to already have an assigned IP address (either static or via DHCP/AutoIP link-local `169.254.x.x`); industrial field I/O racks generally do not implement mDNS; zero capability to write IP addresses over raw Ethernet.

### Option 3: Dual-Tier Architecture: PROFINET DCP (Primary Layer 2) + mDNS (Secondary Routable)
- *Pros*: 
  - PROFINET DCP operates directly over raw Ethernet frames (EtherType `0x8892`), enabling instant discovery of unconfigured hardware (`0.0.0.0`) in $< 100\,\text{ms}$.
  - Supports standard PROFINET DCP Set commands to assign permanent IP suites and execute physical LED flashing.
  - mDNS fallback accommodates IT/OT converged gateways, IPCs, and OPC UA servers on routable IP subnets.
- *Cons*: Requires raw socket privileges (`CAP_NET_RAW` on Linux) for the DCP listener service.

---

## Decision Outcome
**Adopt Option 3: Dual-Tier Architecture (PROFINET DCP Layer 2 Discovery as Primary, mDNS/Zeroconf as Secondary Fallback).**

### Key Architectural Commitments:

1. **PROFINET DCP Engine (`crates/mythos-driver-api` / `mythos-net-dcp`)**:
   - Transmit DCP Identify Multicast frames to destination MAC `FF:FF:FF:FF:FF:FF` with EtherType `0x8892` and Frame ID `0xFEFD`.
   - Parse TLV blocks: Device Station Name, MAC Address, IP Suite, Device ID, and Runtime State (`RUN`, `STOP`, `FAULT`).
   - Implement DCP Set Service (`0xFEFC`) for remote "Blink Indicator LED" (Control Option `0x05`, Sub-Option `0x04`) and "Assign IP Address" (IP Option `0x01`, Sub-Option `0x01`).

2. **Privilege & Sandboxing**:
   - The DCP discovery service runs in an isolated worker process granted strictly `CAP_NET_RAW` and `CAP_NET_BIND_SERVICE` via Linux file capabilities, avoiding full root execution.
   - Raw frame parsers are written in pure Rust using zero-copy packet slicing with strict length boundaries to guarantee memory safety.

3. **Routable Network Fallback**:
   - When no DCP responses are observed or when interfaces belong to routed corporate domains, MythOS initiates asynchronous mDNS queries over UDP port 5353 (`224.0.0.251`) for `_mythos._tcp.local` and `_opcua-tcp._tcp.local`.

4. **RS-485 Fieldbus Timing Invariants**:
   - Master transceiver timing strictly respects silent delimiters: $t_{1.5} \le 1.5\,T_{\text{char}}$ and $t_{3.5} \ge 3.5\,T_{\text{char}}$.
   - Transceiver direction turnaround guard time enforced at $t_{\text{guard}} \ge 5.0\,\text{ms}$.
   - Cyclic scan loop bounded between $50\,\text{ms}$ (fast cyclic process I/O) and $500\,\text{ms}$ (slow environmental instrumentation), with individual transaction timeouts hard-capped at $150\,\text{ms}$.

---

## Consequences

### Positive Consequences
- **Instantaneous Discovery**: Commissioning engineers can connect a laptop to any plant switch port and immediately see every connected industrial rack on the visual canvas.
- **Physical Verification**: Field technicians can confirm rack identities using the "Blink LED" action from the HMI, preventing human wiring errors.
- **Resilient Fieldbus**: Deterministic RS-485 cycle timing prevents Modbus buffer overruns, eliminates framing corruption, and delivers sub-millisecond deterministic shared memory updates.

### Negative Consequences / Trade-offs
- Deployment on Linux containers (Docker / Podman) requires `--cap-add=NET_RAW` or host network mode for Layer 2 DCP discovery.
- Non-PROFINET proprietary serial devices still require initial baud rate and parity scanning if default parameters are unknown.

---

## Compliance and Verification
- **Network Verification**: Automated CI tests replay PCAP captures of PROFINET DCP Identify, Blink LED, and IP Set frames to verify bitwise parser compliance.
- **Physical Field Testing**: Field sign-off by Dewa Bagus (OT Specialist) and Fadhil (OT Technician) on the testbench rack prior to merging to `dev`.
