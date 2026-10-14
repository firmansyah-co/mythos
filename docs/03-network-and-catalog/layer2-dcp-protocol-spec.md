# MythOS Network Specification: Layer 2 Discovery and Fieldbus Timing Specification

- **Document Identifier**: MYTHOS-NET-001
- **Version**: 1.0.0
- **Author**: Vulcan (VP Hardware & Robotics Engineering)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Executive Summary and Scope

This document specifies the Layer 2 network discovery protocols and deterministic fieldbus timing architectures for MythOS. 

Industrial automation environments contain uncommissioned edge nodes, unmanaged switches, and multi-drop fieldbuses. Devices frequently arrive from factories without valid IP addresses or subnets. Standard Layer 3 protocols (such as ICMP ping, HTTP, or mDNS) fail to discover or configure these devices across misconfigured subnets or unassigned network interfaces.

MythOS implements two discovery layers:
1. **Primary Layer 2 Industrial Discovery**: PROFINET Discovery and basic Configuration Protocol (DCP) over raw Ethernet frames. This protocol operates without IP configuration, penetrates across subnets on local Layer 2 broadcast domains, and provides deterministic hardware identification, remote LED flashing, and IP address assignment.
2. **Secondary Routable Discovery (Fallback)**: Multicast DNS (mDNS) / Zeroconf operating over UDP port 5353 for routable Layer 3 subnets and software-defined networks.

Additionally, this document specifies deterministic bus timings, multi-drop termination rules, and scan bounds for RS-485 serial fieldbuses (Modbus RTU).

---

## 2. Normative References and Standards

- **IEC 61158-6-10**: *Industrial communication networks — Fieldbus specifications — Part 6-10: Application layer protocol specification — Type 10 elements (PROFINET)*.
- **IEC 61784-2**: *Industrial communication networks — Profiles — Part 2: Additional fieldbus profiles for real-time networks based on ISO/IEC/IEEE 8802-3*.
- **IEEE 802.3**: *Standard for Ethernet*.
- **RFC 6762**: *Multicast DNS*.
- **RFC 6763**: *DNS-Based Service Discovery*.
- **TIA/EIA-485-A**: *Electrical Characteristics of Generators and Receivers for Use in Balanced Digital Multipoint Systems*.

---

## 3. PROFINET DCP Layer 2 Discovery Architecture

### 3.1 Raw Ethernet Frame Topology

PROFINET DCP bypasses the IP protocol stack (Layer 3) and User Datagram Protocol (Layer 4). All DCP service requests and responses encapsulate directly inside IEEE 802.3 Ethernet frames using the dedicated EtherType `0x8892`.

```
+-----------------------------------------------------------------------+
|                         IEEE 802.3 Ethernet Header                    |
+------------------------------------+----------------------------------+
| Destination MAC (6 Octets)         | FF:FF:FF:FF:FF:FF (Broadcast) or |
|                                    | Target Station Unicast MAC       |
+------------------------------------+----------------------------------+
| Source MAC (6 Octets)              | MythOS Host Interface MAC        |
+------------------------------------+----------------------------------+
| EtherType (2 Octets)               | 0x8892 (PROFINET Protocol)       |
+------------------------------------+----------------------------------+
|                        PROFINET DCP Protocol Header                   |
+------------------------------------+----------------------------------+
| Frame ID (2 Octets)                | 0xFEFD (DCP Identify Request)    |
|                                    | 0xFEFE (DCP Identify Response)   |
|                                    | 0xFEFC (DCP Get/Set Request)     |
|                                    | 0xFEFB (DCP Get/Set Response)    |
+------------------------------------+----------------------------------+
| Service ID (1 Octet)               | 0x05 (Identify), 0x03 (Get),     |
|                                    | 0x04 (Set)                       |
+------------------------------------+----------------------------------+
| Service Type (1 Octet)             | 0x00 (Request), 0x01 (Response   |
|                                    | Success), 0x05 (Response Error)  |
+------------------------------------+----------------------------------+
| XID (Transaction ID) (4 Octets)    | 32-bit Monotonic Sequence Number |
+------------------------------------+----------------------------------+
| Response Delay (2 Octets)          | Maximum delay in 10ms ticks      |
+------------------------------------+----------------------------------+
| DCP Data Length (2 Octets)         | Payload length in bytes          |
+------------------------------------+----------------------------------+
| DCP Blocks (Variable Octets)       | Chained Type-Length-Value Blocks |
+-----------------------------------------------------------------------+
```

### 3.2 EtherType and Frame ID Allocation

MythOS kernel filters raw sockets using strict Berkeley Packet Filter (BPF) expressions:

| Protocol Parameter | Value | Hexadecimal | Definition |
| :--- | :--- | :--- | :--- |
| **EtherType** | 34962 | `0x8892` | PROFINET Real-Time / DCP |
| **Frame ID: Identify Multicast Request** | 65277 | `0xFEFD` | Broadcast request to locate all DCP stations |
| **Frame ID: Identify Response** | 65278 | `0xFEFE` | Unicast response from detected station to controller |
| **Frame ID: Get/Set Multicast Request** | 65276 | `0xFEFC` | Request to query or write parameters |
| **Frame ID: Get/Set Response** | 65275 | `0xFEFB` | Acknowledgment or response to query/write |

### 3.3 Discovery Across Subnets and Unassigned IP Devices

Standard TCP/IP communications fail when a device:
1. Possesses an unconfigured IP address (`0.0.0.0`).
2. Operates on a mismatched subnet (e.g., Device on `192.168.1.50/24`, Controller on `10.10.10.10/24`).
3. Has duplicate IP allocations causing ARP conflicts.

Because PROFINET DCP operates on Data Link Layer 2, frames pass through industrial Layer 2 switches regardless of VLAN configuration or IP allocation. A MythOS node discovers every connected PROFINET device by transmitting an Identify Request frame with destination MAC `FF:FF:FF:FF:FF:FF`.

The Identify Request can broadcast wildcard queries or filter by:
- **Device Family**: Generic, Drives, I/O, IPC.
- **Station Name**: Exact NameOfStation string match.

---

## 4. DCP Block Encodings and Service Payloads

All DCP parameters use a structured TLV (Type, Length, Value) encoding:

```
+----------------------------------------------------+
| Block Option (1 Octet)                             |
+----------------------------------------------------+
| Block Sub-Option (1 Octet)                         |
+----------------------------------------------------+
| Block Length (2 Octets, Big-Endian)                |
+----------------------------------------------------+
| Block Status / Info (2 Octets, optional in set/get)|
+----------------------------------------------------+
| Block Payload (N Octets)                           |
+----------------------------------------------------+
| Padding (1 Octet if Block Length is Odd)           |
+----------------------------------------------------+
```

### 4.1 Supported Options and Sub-Options

| Option | Sub-Option | Name | Description |
| :--- | :--- | :--- | :--- |
| `0x01` (IP) | `0x01` | IP Parameter | IP Address, Subnet Mask, Default Gateway |
| `0x01` (IP) | `0x02` | IP Suite Full | IP parameters including DHCP status |
| `0x02` (Device) | `0x01` | Manufacturer Specific | Vendor ID and Device ID |
| `0x02` (Device) | `0x02` | Name of Station | UTF-8 DNS-compatible station identifier |
| `0x02` (Device) | `0x03` | Device ID | Vendor ID (2 octets) + Device ID (2 octets) |
| `0x02` (Device) | `0x04` | Device Role | Supervisor (0x01), Controller (0x02), Device (0x04) |
| `0x02` (Device) | `0x05` | Device Options | Supported DCP options bitmap |
| `0x02` (Device) | `0x07` | Station OEM ID | OEM Vendor ID and Device ID |
| `0x05` (Control) | `0x04` | Signal / Flash LED | Blink diagnostic LEDs on physical hardware |
| `0x05` (Control) | `0x05` | Response Reset | Factory reset device parameters |
| `0x05` (Control) | `0x06` | Reset to Factory | Complete NVRAM purge |

### 4.2 Service Response Payload Fields

Upon receiving an Identify Request, every compliant device transmits an Identify Response (`0xFEFE`) back to the MythOS host unicast MAC. The payload contains:
1. **Device Station Name (Option 0x02, Sub 0x02)**: e.g., `et200sp-rack-01`, `supmea-p300-gw`.
2. **Hardware MAC Address**: Extracted from source MAC of the Ethernet frame.
3. **Current IP Suite (Option 0x01, Sub 0x01)**:
   - 4 Octets: IP Address.
   - 4 Octets: Subnet Mask.
   - 4 Octets: Gateway Address.
4. **Device Identification (Option 0x02, Sub 0x03)**:
   - 2 Octets: Vendor ID (e.g., `0x002A` for Siemens AG).
   - 2 Octets: Device ID (e.g., `0x0313` for ET200SP).
5. **Device Role (Option 0x02, Sub 0x04)**: Bitmask identifying IO-Device, IO-Controller, or IO-Supervisor.
6. **Runtime State**: Extracted via Status and Diagnostic blocks:
   - `RUN`: Controller or IO device active and cyclic data exchanging.
   - `STOP`: Device halted or unconfigured.
   - `FAULT`: Hardware failure, bus fault (BF), or system fault (SF) active.

---

## 5. Remote Operations: Blink LED and IP Assignment

### 5.1 Remote "Blink Indicator LED" Command (Signal Service)

In dense industrial control cabinets with dozens of identical decentralized I/O racks, field technicians must locate the exact physical module mapped in software. 

MythOS emits a DCP Set Request (`0xFEFC`) containing the Signal sub-option:
- **Service ID**: `0x04` (Set Request).
- **Service Type**: `0x00`.
- **Block Option**: `0x05` (Control).
- **Block Sub-Option**: `0x04` (Signal).
- **Length**: `0x0004` (4 octets).
- **Payload**:
  - `0x0100` (Signal Value: Flash Once / Blink active).
  - `0x0000` (Reserved).

Upon receipt, the target hardware physically flashes its `RUN`, `ERR`, and `MAINT` LEDs at a frequency of 2 Hz for a duration of 3 to 10 seconds.

### 5.2 Remote "Assign IP Address" Layer 2 Frames

When commissioning bare-metal hardware or replacing a failed unit:
1. MythOS discovers the unconfigured device MAC via DCP Identify Request.
2. The operator assigns target IP `192.168.10.50`, Subnet `255.255.255.0`, Gateway `192.168.10.1`.
3. MythOS constructs a DCP Set Request frame:
   - **Destination MAC**: Unicast MAC of the target device.
   - **Block Option**: `0x01` (IP Parameter).
   - **Block Sub-Option**: `0x01` (IP).
   - **Block Length**: `0x000E` (14 octets).
   - **Block Qualifier**:
     - `0x0000`: Temporary assignment (lost on reboot).
     - `0x0001`: Permanent assignment (saved to NVRAM/EEPROM).
   - **Block Payload**:
     - 4 Octets: IP Address (`0xC0, 0xA8, 0x0A, 0x32`).
     - 4 Octets: Subnet Mask (`0xFF, 0xFF, 0xFF, 0x00`).
     - 4 Octets: Gateway Address (`0xC0, 0xA8, 0x0A, 0x01`).
4. The device validates the address, writes NVRAM, and responds with DCP Set Response (`0xFEFB`, Result `0x0000` OK).

---

## 6. Secondary Fallback: mDNS / Zeroconf Architecture

For non-PROFINET hardware, managed layer-3 edge gateways, and software IPC instances, MythOS provides an automatic fallback to Multicast DNS (RFC 6762) and DNS-SD (RFC 6763).

### 6.1 Multicast Group and Port Configuration

- **IPv4 Multicast Address**: `224.0.0.251`
- **IPv6 Multicast Address**: `FF02::FB`
- **UDP Port**: `5353`
- **Service Type Registered**: `_mythos._tcp.local`, `_opcua-tcp._tcp.local`, `_modbus-tcp._tcp.local`.

### 6.2 Service Registration Record Schema

MythOS nodes publish DNS PTR, SRV, and TXT records:
```
PTR: _mythos._tcp.local -> edge-controller-01._mythos._tcp.local
SRV: edge-controller-01._mythos._tcp.local -> priority: 0, weight: 0, port: 4840, target: edge-controller-01.local
TXT: 
  v=1
  role=runtime-microkernel
  state=RUN
  mac=70:B3:D5:E2:81:4A
  schema=1.0.0
```

### 6.3 Discovery Selection Algorithm

```
+-------------------------------------------------------+
| Trigger Network Discovery Scan                       |
+-------------------------------------------------------+
                           |
                           v
        [ Transmit PROFINET DCP Broadcast Frame ]
        [ EtherType 0x8892, Dst FF:FF:FF:FF:FF:FF]
                           |
            +--------------+--------------+
            |                             |
      Responses Received           Timeout (500ms)
            |                             |
            v                             v
[ Parse PROFINET Devices ]      [ Send mDNS UDP 5353 Query ]
[ Extract MAC, IP, State ]      [ Query _mythos._tcp.local ]
            |                             |
            +--------------+--------------+
                           |
                           v
   [ Merge Topologies into System Discovery Map ]
   [ Deduplicate by Physical MAC Address        ]
```

---

## 7. Deterministic RS-485 Serial Fieldbus Timing and Invariants

RS-485 (TIA/EIA-485-A) differential serial interfaces govern Modbus RTU communications for smart instruments (e.g., pressure transmitters, rainfall sensors, flowmeters). Because RS-485 is half-duplex and non-collision-detecting, the MythOS runtime strictly enforces physical layer invariants and deterministic timings.

### 7.1 Electrical Specifications and Termination

1. **Topology**: Pure daisy-chain (bus) topology. Star, ring, or tree topologies are strictly prohibited.
2. **Termination Resistors**: Exactly two 120-Ohm 1/4-Watt terminating resistors must be installed:
   - One resistor across Line A (+) and Line B (-) at the physical start of the bus (MythOS Master).
   - One resistor across Line A (+) and Line B (-) at the last physical multi-drop slave.
   - All intermediate drops must leave termination DIP switches OPEN.
3. **Fail-Safe Biasing**: When the bus is idle, receivers float into an indeterminate logic state. The MythOS master interface enforces fail-safe biasing resistors:
   - Pull-up resistor (560 Ohm to 1k Ohm) from Line B (+) to +5VDC.
   - Pull-down resistor (560 Ohm to 1k Ohm) from Line A (-) to GND.
   - Generates a default differential voltage $V_{AB} > +200\,\text{mV}$ during idle state.
4. **Maximum Cable Length**:
   - At 9600 bps: Maximum 1200 meters (Shielded Twisted Pair, 24 AWG, Belden 9841 equivalent).
   - At 115200 bps: Maximum 300 meters.
5. **Stub Length**: Maximum stub drop length from the main trunk line must not exceed 0.3 meters.

### 7.2 Multi-Drop Baud Rate Scanning and Parity Invariants

When an unconfigured sensor is attached to the serial trunk, MythOS executes a bounded automated baud rate and parity search.

#### Parity Invariant
Per the Modbus Application Protocol Specification V1.1b:
- **Default Framing**: 1 Start bit, 8 Data bits, Even Parity, 1 Stop bit (`8-E-1`).
- **No-Parity Mandate**: When Parity is NONE, the standard dictates **2 Stop bits** (`8-N-2`) to maintain an 11-bit character envelope.
- **Field Tolerance**: In Asian industrial instruments (e.g., Supmea, SenTec, Haiwell), manufacturers widely implement `8-N-1` (10-bit envelope). MythOS supports configurable stop bits and verifies framing error flags ($FE$) in the UART driver.

#### Automated Scan Sequence Matrix
```
For each Slave ID in Candidate List [1..32]:
  For Baud in [9600, 19200, 4800, 115200, 38400]:
    For Framing in [8-N-1, 8-E-1, 8-N-2]:
      1. Configure UART parameters.
      2. Clear UART RX FIFO buffer.
      3. Transmit Modbus Function Code 0x03 (Read Holding Register 0x0000, 1 word).
      4. Await response within timeout (T_response = 120ms).
      5. If valid CRC-16 received:
         Record Device { slave_id, baud, framing }
         Break to next Slave ID.
```

### 7.3 Inter-Character and Inter-Frame Bounded Timings ($t_{1.5}$ and $t_{3.5}$)

Modbus RTU uses silent physical line intervals to demarcate message frames:

1. **Character Frame Duration ($T_{\text{char}}$)**:
   For 11-bit framing (`8-E-1` or `8-N-2`):
   $$T_{\text{char}} = \frac{11}{\text{Baudrate}}$$
   At 9600 bps:
   $$T_{\text{char}} = \frac{11}{9600} \approx 1.1458\,\text{ms}$$

2. **Inter-Character Space ($t_{1.5}$)**:
   The silent pause between characters inside a single frame must not exceed $1.5 \times T_{\text{char}}$ ($1.718\,\text{ms}$ at 9600 bps). If a silent interval exceeding $1.5\,\text{char}$ occurs, the receiver marks the frame as incomplete and resets.
   *(For baud rates $> 19200\,\text{bps}$, fixed standard value $t_{1.5} = 750\,\mu\text{s}$ applies).*

3. **Inter-Frame Silence ($t_{3.5}$)**:
   A message frame must start and end with a silent transmission interval of at least $3.5 \times T_{\text{char}}$ ($4.01\,\text{ms}$ at 9600 bps).
   *(For baud rates $> 19200\,\text{bps}$, fixed standard value $t_{3.5} = 1.750\,\text{ms}$ applies).*

4. **Turnaround Delay ($t_{\text{turnaround}}$)**:
   Before a master transmits a new query to another slave on the same RS-485 bus, it must wait a minimum guard time of $t_{\text{guard}} = 5\,\text{ms}$ after receiving the previous slave response. This allows the previous slave's RS-485 transceiver driver to release the differential bus into high-impedance mode (Tri-state).

---

## 8. Deterministic Cycle-Time Bounds for Fieldbus Acquisition

MythOS enforces bounded execution periods for all polling cycles to prevent thread starvation and shared memory latency spikes:

```
+--------------------------------------------------------------------------------+
|                         Fieldbus Acquisition Bounds                            |
+-------------------+-----------------+-------------------+----------------------+
| Fieldbus Class    | Scan Interval   | Worst-Case Delay  | Recovery Mechanism   |
+-------------------+-----------------+-------------------+----------------------+
| Fast Cyclic I/O   | 50 ms           | 75 ms             | Token Timeout Alert  |
| Process Sensors   | 100 ms - 200 ms | 350 ms            | Fallback to Last OK  |
| Environmental/Slow| 500 ms          | 1200 ms           | Quality Flag = BAD   |
+-------------------+-----------------+-------------------+----------------------+
```

### 8.1 Cycle-Time Calculation Formula

For an RS-485 Modbus RTU bus running at 9600 bps with $N$ slave devices, each polled for 10 holding registers:

$$\text{Query Bytes} = 8\,\text{bytes (Slave + FC + AddrHi + AddrLo + QtyHi + QtyLo + CRCLo + CRCHi)}$$
$$\text{Response Bytes} = 5 + (2 \times 10) = 25\,\text{bytes}$$
$$\text{Total Raw Bytes per Transaction} = 33\,\text{bytes}$$
$$T_{\text{wire}} = 33 \times 1.146\,\text{ms} \approx 37.82\,\text{ms}$$
$$T_{\text{device\_response}} \approx 20.00\,\text{ms}\quad (\text{slave processing time})$$
$$T_{\text{guard}} = 5.00\,\text{ms}$$
$$T_{\text{transaction}} = T_{\text{wire}} + T_{\text{device\_response}} + T_{\text{guard}} \approx 62.82\,\text{ms}$$

For $N = 4$ sensors:
$$T_{\text{total\_scan}} = 4 \times 62.82\,\text{ms} = 251.28\,\text{ms}$$

### 8.2 Cycle Invariants
1. **Bounded Polling**: Master loop interval must be configured with at least $20\%$ headroom above $T_{\text{total\_scan}}$. For a 4-sensor bus, nominal cycle is bounded to **300ms**.
2. **Deterministic Timeout**: Slave timeout is capped at $150\,\text{ms}$. If no byte arrives within $150\,\text{ms}$, the transaction aborts, the tag quality status transitions to `COMM_FAILURE`, and the scheduler immediately moves to the next slave.
3. **Jitter Control**: Polling jitter must not exceed $\pm 2.5\,\text{ms}$ under real-time Linux (`PREEMPT_RT`) or Tokio high-resolution timers.

---

## 9. Hardware Diagnostic Indicators and Verification Procedures

| LED Symbol | Color | Behavior | Diagnostic State |
| :--- | :--- | :--- | :--- |
| **BF** (Bus Fault) | Red | Steady ON | Physical link disconnected or no carrier signal |
| **BF** | Red | Flashing (2 Hz) | Layer 2 discovery active; no cyclic data connection |
| **SF** (System Fault)| Red | Steady ON | Hardware fault, address collision, or NVRAM error |
| **RUN** | Green | Steady ON | Cyclic communication active; valid process values |
| **MAINT** | Yellow| Steady ON | Degraded line quality, framing error rate $> 1\%$ |
| **FLASH** | All | Sync Blink (2 Hz)| Remote DCP "Blink LED" command actively received |

Verification of the Layer 2 discovery and RS-485 timings is performed using Wireshark on Linux interfaces (`raw_socket:eth0`) and digital logic analyzers on the serial lines (Saleae Logic measuring $t_{1.5}$ and $t_{3.5}$ waveforms).
