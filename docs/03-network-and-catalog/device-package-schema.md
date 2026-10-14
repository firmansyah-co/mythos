# MythOS Open Hardware Catalog: Device Package Schema and Standards

- **Document Identifier**: MYTHOS-CAT-001
- **Version**: 1.0.0
- **Author**: Vulcan (VP Hardware & Robotics Engineering)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Executive Summary and Scope

The MythOS Open Hardware Catalog defines a vendor-neutral, machine-readable packaging format for industrial instrumentation, remote I/O modules, motor drives, and edge devices. 

In traditional SCADA and PLC engineering, adding field instrumentation requires manual typing of Modbus register addresses, engineering scale conversions, bitwise masks, and parity parameters into proprietary engineering tools. This manual process causes configuration errors, silent byte swapping bugs, and costly on-site downtime.

The MythOS Device Package Specification solves this challenge by standardizing:
1. **Device Manifest Schema**: A declarative YAML and JSON schema specifying hardware identity, physical port bindings, protocol configurations, register maps, byte endianness, and mathematical transformation functions.
2. **Universal Device Discovery & Fingerprinting**: Automated multi-drop bus sweep heuristics (3-Tier Heuristic Sweep: Fast Probe, Expanding Sweep, Full Blind Sweep) and Passive Bus Sniffing for non-disruptive zero-configuration telemetry.
3. **SVG Thumbnail Standard**: Strict visual guidelines for isometric and 2D vector hardware symbols rendered across the MythOS Network Topology View and HMI Inspector Drawer.
4. **Reference Implementations**: Two field-tested gold standard device packages:
   - **Supmea SUP-P300**: Industrial Piezoresistive Pressure Transmitter (Modbus RTU).
   - **SenTec Industrial Rainfall Sensor**: Tipping Bucket Precipitation Gauge (Modbus RTU).

---

## 2. Normative References and Schema Formats

- **JSON Schema Draft 2020-12**: *Specification for JSON Schema Validation*.
- **W3C Scalable Vector Graphics (SVG) 2.0**: *W3C Recommendation*.
- **Modbus Application Protocol Specification V1.1b**: *Modbus Organization*.
- **IEEE 754-2019**: *Standard for Floating-Point Arithmetic*.
- **ANSI/ISA-5.1-2009**: *Instrumentation Symbols and Identification*.

---

## 3. Package Directory Structure

Every device package is an isolated directory within the catalog root (`catalog/devices/<device-identifier>/`).

```
catalog/devices/supmea-p300/
├── manifest.yaml             # Human-authored primary hardware specification
├── manifest.json             # Canonical machine-validated schema representation
├── icon.svg                  # Vector thumbnail for UI canvas & inspector drawer
├── wiring-diagram.svg        # 2-wire / 4-wire terminal connection schematic
└── README.md                 # Field commissioning notes and hardware manual
```

---

## 4. Formal Device Package Schema Specification

A valid device package manifest must strictly conform to the following schema definition.

### 4.1 Schema Definition (JSON Schema Draft 2020-12)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://mythos.firmansyah.co/schemas/device-package-v1.json",
  "title": "MythOSDevicePackage",
  "type": "object",
  "required": [
    "schemaVersion",
    "metadata",
    "physicalInterface",
    "protocol",
    "registers",
    "ui"
  ],
  "properties": {
    "schemaVersion": {
      "type": "string",
      "enum": [
        "1.0.0"
      ]
    },
    "metadata": {
      "type": "object",
      "required": [
        "id",
        "vendor",
        "model",
        "category",
        "description",
        "revision"
      ],
      "properties": {
        "id": {
          "type": "string",
          "pattern": "^[a-z0-9-]+$"
        },
        "vendor": {
          "type": "string"
        },
        "vendorId": {
          "type": "string"
        },
        "model": {
          "type": "string"
        },
        "category": {
          "type": "string",
          "enum": [
            "sensor",
            "actuator",
            "drive",
            "remote-io",
            "gateway",
            "power-meter"
          ]
        },
        "description": {
          "type": "string"
        },
        "revision": {
          "type": "string"
        },
        "certifications": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      }
    },
    "physicalInterface": {
      "type": "object",
      "required": [
        "connectorType",
        "signaling",
        "supplyVoltage"
      ],
      "properties": {
        "connectorType": {
          "type": "string",
          "enum": [
            "terminal-block",
            "m12-a-coded",
            "m12-d-coded",
            "rj45",
            "db9"
          ]
        },
        "signaling": {
          "type": "string",
          "enum": [
            "rs485-2wire",
            "rs485-4wire",
            "rs232",
            "ethernet-100basetx"
          ]
        },
        "supplyVoltage": {
          "type": "object",
          "required": [
            "nominalVdc",
            "minVdc",
            "maxVdc",
            "powerWatts"
          ],
          "properties": {
            "nominalVdc": {
              "type": "number"
            },
            "minVdc": {
              "type": "number"
            },
            "maxVdc": {
              "type": "number"
            },
            "powerWatts": {
              "type": "number"
            }
          }
        },
        "pinout": {
          "type": "array",
          "items": {
            "type": "object",
            "required": [
              "pin",
              "label",
              "function"
            ],
            "properties": {
              "pin": {
                "type": "string"
              },
              "label": {
                "type": "string"
              },
              "function": {
                "type": "string"
              }
            }
          }
        }
      }
    },
    "protocol": {
      "type": "object",
      "required": [
        "type",
        "modbusRtu"
      ],
      "properties": {
        "type": {
          "type": "string",
          "enum": [
            "modbus-rtu",
            "modbus-tcp",
            "profinet",
            "opcua"
          ]
        },
        "modbusRtu": {
          "type": "object",
          "required": [
            "defaultSlaveId",
            "defaultBaudRate",
            "supportedBaudRates",
            "defaultFraming",
            "endianness"
          ],
          "properties": {
            "defaultSlaveId": {
              "type": "integer",
              "minimum": 1,
              "maximum": 247
            },
            "defaultBaudRate": {
              "type": "integer"
            },
            "supportedBaudRates": {
              "type": "array",
              "items": {
                "type": "integer"
              }
            },
            "defaultFraming": {
              "type": "string",
              "enum": [
                "8-N-1",
                "8-E-1",
                "8-O-1",
                "8-N-2"
              ]
            },
            "endianness": {
              "type": "string",
              "enum": [
                "big-endian",
                "little-endian",
                "byte-swap",
                "word-swap",
                "mid-big-endian"
              ],
              "description": "Standard 16-bit and 32-bit endianness configuration"
            },
            "minPollIntervalMs": {
              "type": "integer",
              "minimum": 10
            },
            "timeoutMs": {
              "type": "integer",
              "minimum": 20
            }
          }
        }
      }
    },
    "registers": {
      "type": "array",
      "items": {
        "$ref": "#/$defs/RegisterDefinition"
      }
    },
    "ui": {
      "type": "object",
      "required": [
        "iconPath",
        "colorAccent",
        "rackWidthUnits"
      ],
      "properties": {
        "iconPath": {
          "type": "string"
        },
        "colorAccent": {
          "type": "string"
        },
        "rackWidthUnits": {
          "type": "integer"
        }
      }
    },
    "discovery": {
      "$ref": "#/$defs/DiscoveryDefinition"
    }
  },
  "$defs": {
    "RegisterDefinition": {
      "type": "object",
      "required": [
        "name",
        "tagSuffix",
        "addressHex",
        "addressDec",
        "functionCode",
        "dataType",
        "access",
        "unit"
      ],
      "properties": {
        "name": {
          "type": "string"
        },
        "tagSuffix": {
          "type": "string",
          "pattern": "^[a-zA-Z0-9_]+$"
        },
        "addressHex": {
          "type": "string"
        },
        "addressDec": {
          "type": "integer",
          "minimum": 0,
          "maximum": 65535
        },
        "functionCode": {
          "type": "string",
          "enum": [
            "0x01",
            "0x02",
            "0x03",
            "0x04",
            "0x05",
            "0x06",
            "0x10"
          ]
        },
        "dataType": {
          "type": "string",
          "enum": [
            "BOOL",
            "INT16",
            "UINT16",
            "INT32",
            "UINT32",
            "FLOAT32",
            "BCD16",
            "RAW_BYTES"
          ]
        },
        "access": {
          "type": "string",
          "enum": [
            "RO",
            "WO",
            "RW"
          ]
        },
        "unit": {
          "type": "string"
        },
        "scaling": {
          "type": "object",
          "required": [
            "type"
          ],
          "properties": {
            "type": {
              "type": "string",
              "enum": [
                "linear",
                "divisor",
                "lookup",
                "none"
              ]
            },
            "multiplier": {
              "type": "number"
            },
            "divisor": {
              "type": "number"
            },
            "offset": {
              "type": "number"
            },
            "rawMin": {
              "type": "number"
            },
            "rawMax": {
              "type": "number"
            },
            "scaledMin": {
              "type": "number"
            },
            "scaledMax": {
              "type": "number"
            }
          }
        },
        "enumMapping": {
          "type": "object",
          "additionalProperties": {
            "type": "string"
          }
        },
        "description": {
          "type": "string"
        }
      }
    },
    "DiscoveryDefinition": {
      "type": "object",
      "description": "Heuristic discovery and active bus fingerprinting parameters",
      "required": [
        "probe_register",
        "expected_function_code",
        "priority_baudrates",
        "priority_parities",
        "default_id_range"
      ],
      "properties": {
        "probe_register": {
          "oneOf": [
            {
              "type": "integer",
              "minimum": 0,
              "maximum": 65535,
              "description": "Standard 1-based PLC register address (e.g. 40001) or 0-based offset"
            },
            {
              "type": "string",
              "pattern": "^(0x[0-9a-fA-F]+|[0-9]+)$",
              "description": "Register address formatted as hex string (e.g. 0x0000) or decimal string"
            }
          ],
          "description": "Modbus register address probed during heuristic identification"
        },
        "expected_function_code": {
          "type": "string",
          "enum": [
            "0x01",
            "0x02",
            "0x03",
            "0x04"
          ],
          "description": "Non-mutating Modbus read function code used for bus verification"
        },
        "expected_fc": {
          "type": "string",
          "enum": [
            "0x01",
            "0x02",
            "0x03",
            "0x04"
          ],
          "description": "Convenience alias for expected_function_code"
        },
        "priority_baudrates": {
          "type": "array",
          "items": {
            "type": "integer"
          },
          "description": "Ranked serial transmission speeds (baud) tested during fast heuristic sweeps"
        },
        "priority_parities": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "None",
              "Even",
              "Odd"
            ]
          },
          "description": "Ranked serial UART parity modes (None, Even, Odd)"
        },
        "default_id_range": {
          "type": "array",
          "items": {
            "type": "integer",
            "minimum": 1,
            "maximum": 247
          },
          "minItems": 2,
          "maxItems": 2,
          "description": "Candidate Modbus slave address interval [min_id, max_id]"
        },
        "probe_timeout_ms": {
          "type": "integer",
          "minimum": 10,
          "maximum": 5000,
          "description": "Per-frame query timeout in milliseconds"
        }
      }
    }
  }
}
```

---

## 5. Universal Device Discovery & Fingerprinting Specification

Field deployment in operational facilities often encounters unlabelled, unmapped, or legacy serial instruments. The MythOS runtime provides an automated, non-invasive discovery engine to detect, identify, and configure multi-drop devices without requiring manual baud rate matching or slave address lookup.

### 5.1 Discovery Manifest Schema Specification

To participate in automated discovery and fingerprinting, a device manifest MAY define a top-level `discovery` configuration block conforming to the JSON/YAML schema below:

```yaml
discovery:
  probe_register: 40001
  expected_function_code: "0x03"
  priority_baudrates:
    - 9600
    - 19200
    - 4800
  priority_parities:
    - "None"
    - "Even"
  default_id_range:
    - 1
    - 32
  probe_timeout_ms: 50
```

#### JSON Schema Definition (`$defs/DiscoveryDefinition`)
```json
{
  "DiscoveryDefinition": {
    "type": "object",
    "description": "Heuristic discovery and active bus fingerprinting parameters",
    "required": [
      "probe_register",
      "expected_function_code",
      "priority_baudrates",
      "priority_parities",
      "default_id_range",
      "probe_timeout_ms"
    ],
    "properties": {
      "probe_register": {
        "oneOf": [
          {
            "type": "integer",
            "minimum": 0,
            "maximum": 65535,
            "description": "Standard 1-based PLC register (e.g. 40001) or 0-based offset"
          },
          {
            "type": "string",
            "pattern": "^(0x[0-9a-fA-F]+|[0-9]+)$",
            "description": "Register address formatted as hex string (e.g. 0x0000) or decimal string"
          }
        ],
        "description": "Modbus register address probed during heuristic identification"
      },
      "expected_function_code": {
        "type": "string",
        "enum": ["0x01", "0x02", "0x03", "0x04"],
        "description": "Non-mutating Modbus read function code used for bus verification"
      },
      "expected_fc": {
        "type": "string",
        "enum": ["0x01", "0x02", "0x03", "0x04"],
        "description": "Convenience alias for expected_function_code"
      },
      "priority_baudrates": {
        "type": "array",
        "items": { "type": "integer" },
        "description": "Ranked serial transmission speeds (baud) tested during fast heuristic sweeps"
      },
      "priority_parities": {
        "type": "array",
        "items": {
          "type": "string",
          "enum": ["None", "Even", "Odd"]
        },
        "description": "Ranked serial UART parity modes (None, Even, Odd)"
      },
      "default_id_range": {
        "type": "array",
        "items": { "type": "integer", "minimum": 1, "maximum": 247 },
        "minItems": 2,
        "maxItems": 2,
        "description": "Candidate Modbus slave address interval [min_id, max_id]"
      },
      "probe_timeout_ms": {
        "type": "integer",
        "minimum": 10,
        "maximum": 5000,
        "description": "Per-frame query timeout in milliseconds"
      }
    }
  }
}
```

### 5.2 Discovery Configuration Properties Reference

| Property Name | Type | Required | Default Value | Description |
| :--- | :--- | :--- | :--- | :--- |
| `probe_register` | `integer` \| `string` | **Yes** | - | Register address read during fingerprinting (e.g. `40001` or `0x0000`). |
| `expected_function_code` | `string` | **Yes** | `"0x03"` | Non-mutating Modbus function code (`0x01`, `0x02`, `0x03`, `0x04`). |
| `expected_fc` | `string` | No | `"0x03"` | Convenience shorthand alias for `expected_function_code`. |
| `priority_baudrates` | `array[integer]` | **Yes** | `[9600, 19200, 4800]` | Ranked baud rate sequence tested during Tier 1 and Tier 2 scans. |
| `priority_parities` | `array[string]` | **Yes** | `["None", "Even"]` | Ranked framing parities (`None` for 8-N-1/8-N-2, `Even` for 8-E-1, `Odd` for 8-O-1). |
| `default_id_range` | `[min, max]` | **Yes** | `[1, 32]` | Target slave ID boundaries for fast heuristic sweeps. |
| `probe_timeout_ms` | `integer` | **Yes** | `50` | Maximum wait duration per probe query in milliseconds before timing out. |

---

### 5.3 3-Tier Heuristic Sweep Architecture

RS-485 is a half-duplex, single-master bus without collision detection. Brute-force scanning of all 247 slave addresses across 8 baud rates and 3 parity combinations requires up to $247 \times 8 \times 3 \times 150\,\text{ms} \approx 889\,\text{seconds}$ (~15 minutes), during which process control may be blocked.

To guarantee bounded discovery times without bus disruption, MythOS implements a **3-Tier Heuristic Sweep** algorithm:

```
+-----------------------------------------------------------------------------------+
|                           MYTHOS 3-TIER BUS SWEEP ENGINE                          |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|  [ TIER 1: FAST PROBE ]                                                           |
|    - Default Catalog Parameters (e.g. 9600 bps, 8-N-1)                            |
|    - Common Industrial Addresses (IDs: 1, 2, 3, ... up to 16)                     |
|    - Budget: t <= 1.0 second                                                      |
|         |                                                                         |
|         v (If no response or additional unassigned nodes expected)                |
|  [ TIER 2: EXPANDING HEURISTIC SWEEP ]                                            |
|    - Priority Baudrates: [9600, 19200, 4800, 115200]                             |
|    - Priority Parities: [None, Even]                                              |
|    - Address Range: [1 .. 32]                                                     |
|    - Budget: 2.0s <= t <= 4.0s                                                    |
|         |                                                                         |
|         v (Operator-initiated full audit or commissioning recovery)               |
|  [ TIER 3: FULL BLIND SWEEP ]                                                     |
|    - All Supported Baudrates (1200 bps to 115200 bps)                             |
|    - All Parities (None, Even, Odd)                                               |
|    - Full Modbus Range: [1 .. 247]                                                |
|    - Incremental background task with cooperative cancellation                    |
|                                                                                   |
+-----------------------------------------------------------------------------------+
```

#### 1. Tier 1: Fast Probe ($\le 1.0\,\text{s}$)
- **Target**: Rapid detection of standard factory out-of-the-box instruments.
- **Search Space**: Factory default baud rate (typically `9600` bps), default framing (`8-N-1`), and common factory slave IDs (`1` to `16`).
- **Timing Budget**: Total elapsed execution duration is constrained to $\le 1.0\,\text{s}$ ($\le 20$ probes at $50\,\text{ms}$ timeout).
- **Execution Flow**: Transmits single-word read query to `probe_register`. If a valid response or standard Modbus Exception is received, the node is immediately fingerprinted and logged.

#### 2. Tier 2: Expanding Sweep ($2.0\,\text{s} - 4.0\,\text{s}$)
- **Target**: Identification of field instruments configured with non-default plant speeds or addressing offsets.
- **Search Space**: Priority baud rates specified in catalog manifest (e.g., `[9600, 19200, 4800]`), common parities (`["None", "Even"]`), and extended candidate IDs (`1` to `32`).
- **Timing Budget**: Interleaved bounded scan executing within $2.0\,\text{s}$ to $4.0\,\text{s}$.
- **Optimizations**:
  - Automatically skips baud rates that trigger physical UART framing errors across all initial probe attempts.
  - Aborts baud rate probe if electrical bus noise ($V_{AB} < 200\,\text{mV}$) or constant buffer overflow is detected.

#### 3. Tier 3: Full Blind Sweep (Comprehensive Recovery)
- **Target**: Deep hardware recovery of lost, misconfigured, or unknown legacy multi-drop devices.
- **Search Space**: Exhaustive matrix encompassing all baud rates (`1200` to `115200` bps), all framing options (`8-N-1`, `8-E-1`, `8-O-1`, `8-N-2`), and full Modbus address space (`1` to `247`).
- **Timing & Safety Guarantees**:
  - Never executed automatically during critical process control loops. Must be triggered via Engineer role or MCP AI diagnostic agent.
  - Emits real-time progress updates over bounded channels (`tokio::sync::mpsc::Sender<ScanProgress>`).
  - Implements cooperative cancellation checks (`cancel_token.is_cancelled()`) between each probed register transaction and after each inter-frame silence duration ($t_{3.5}$).

---

### 5.4 Passive Bus Sniffing (Zero-Disruption Telemetry)

Active query probing on a live multi-drop serial bus carries the inherent risk of bus contention if an existing master is actively interrogating field slaves. To eliminate disruption on active plant networks, MythOS incorporates **Passive Bus Sniffing**:

```
+-----------------------------------------------------------------------------------+
|                        PASSIVE BUS SNIFFING ARCHITECTURE                          |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|      Active Master                   RS-485 Differential Bus                      |
|     [ Haiwell PLC ] <==============[ A (+) / B (-) ]==============+               |
|                                                                   |               |
|                                                                   v (High-Z Tap)  |
|                                                         +----------------------+  |
|                                                         |  MythOS Sniffer Node |  |
|                                                         |  (RX Only, TX Tri-Z) |  |
|                                                         +----------------------+  |
|                                                                   |               |
|               +---------------------------------------------------+               |
|               |                                                                   |
|               v                                                                   v
|     [ Frame Boundary Detector ]                                        [ Device Registry ]
|     Demarcates frames via t_1.5 & t_3.5                                Maps ID -> Catalog
|     Extracts Slave ID, FC, and Payload                                 Match vendor signature
|                                                                                   |
+-----------------------------------------------------------------------------------+
```

1. **High-Impedance Listen-Only Mode**:
   - The MythOS RS-485 transceiver driver enters high-impedance (Tri-state) receiver mode ($\text{DE} = 0, \overline{\text{RE}} = 0$).
   - The transmitter driver is physically disabled, guaranteeing zero transmissions or collisions on the differential pair.

2. **Frame Demarcation & Silent Interval Timing**:
   - The passive sniffer monitors line idle silence intervals:
     $$t_{1.5} \le 1.5 \times T_{\text{char}}, \quad t_{3.5} \ge 3.5 \times T_{\text{char}}$$
   - Packets are bounded by $t_{3.5}$ idle line intervals without transmitting query frames.

3. **Autonomous Protocol Dissection & Catalog Fingerprinting**:
   - Incoming master-request and slave-response frames are captured and validated via 16-bit CRC checks ($G(x) = x^{16} + x^{15} + x^2 + 1$).
   - Valid frames reveal active Slave IDs, accessed function codes (`0x03`, `0x04`, etc.), and returned register payloads.
   - The sniffer matches register response lengths, bit structures, and telemetry values against the Open Hardware Catalog fingerprint database, automatically identifying connected hardware models without injecting a single bit onto the wire.

---

### 5.5 Safety Invariants During Auto-Discovery

1. **Strict Read-Only Probing**: Auto-discovery SHALL NEVER transmit Modbus Function Codes `0x05` (Write Single Coil), `0x06` (Write Single Holding Register), `0x0F` (Write Multiple Coils), or `0x10` (Write Multiple Holding Registers). Active probing is strictly confined to read-only FC `0x01`, `0x02`, `0x03`, and `0x04`.
2. **Exception Handling as Device Affirmation**: Receipt of Modbus Exception Codes (`0x01` Illegal Function, `0x02` Illegal Data Address) positively confirms that a device exists and is operational at that Slave ID. The engine marks the device as active and flags it for secondary register interrogation.
3. **Turnaround Silence Enforcement**: Between consecutive discovery probes, the engine strictly waits for the minimum turnaround guard delay $t_{\text{guard}} \ge 5\,\text{ms}$ plus inter-frame silence $t_{3.5}$ before asserting the transmitter driver, preventing RS-485 transceiver contention.

---

## 6. Endianness and Word-Order Mathematics

Modbus transmissions transfer data in 16-bit register blocks (2 bytes: High Byte $B_H$, Low Byte $B_L$). When transmitting 32-bit integers (`INT32`, `UINT32`) or IEEE 754 floating-point numbers (`FLOAT32`), two 16-bit registers ($R_1, R_2$) representing 4 bytes ($B_0, B_1, B_2, B_3$) are transmitted over the wire.

Manufacturers disagree on byte and word serialization. MythOS standardizes the mathematical transformations into five distinct modes:

```
Consider 32-bit Hexadecimal Value 0x12345678:
  Byte 0 = 0x12 (MSB)
  Byte 1 = 0x34
  Byte 2 = 0x56
  Byte 3 = 0x78 (LSB)
```

| Endianness Mode | Wire Byte Sequence | Word Order | Register 1 ($R_1$) | Register 2 ($R_2$) | Typical Manufacturers |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Big-Endian** (ABCD) | `B0, B1, B2, B3` | High Word First | `0x1234` | `0x5678` | Siemens, Yokogawa, SenTec |
| **Word-Swap** (CDAB / Mid-Little) | `B2, B3, B0, B1` | Low Word First | `0x5678` | `0x1234` | Modicon, Schneider Electric |
| **Byte-Swap** (BADC) | `B1, B0, B3, B2` | High Word, Swapped| `0x3412` | `0x7856` | Advantech, Supmea |
| **Little-Endian** (DCBA)| `B3, B2, B1, B0` | Pure Reversed | `0x7856` | `0x3412` | Beckhoff, Rockwell Automation |

MythOS runtime deserializes raw buffers using compile-time Rust vector transmutations without heap allocation:

```rust
pub fn decode_f32(raw_words: &[u16; 2], endianness: ModbusEndianness) -> f32 {
    let bytes: [u8; 4] = match endianness {
        ModbusEndianness::BigEndian => [
            (raw_words[0] >> 8) as u8, (raw_words[0] & 0xFF) as u8,
            (raw_words[1] >> 8) as u8, (raw_words[1] & 0xFF) as u8,
        ],
        ModbusEndianness::WordSwap => [
            (raw_words[1] >> 8) as u8, (raw_words[1] & 0xFF) as u8,
            (raw_words[0] >> 8) as u8, (raw_words[0] & 0xFF) as u8,
        ],
        ModbusEndianness::ByteSwap => [
            (raw_words[0] & 0xFF) as u8, (raw_words[0] >> 8) as u8,
            (raw_words[1] & 0xFF) as u8, (raw_words[1] >> 8) as u8,
        ],
        ModbusEndianness::LittleEndian => [
            (raw_words[1] & 0xFF) as u8, (raw_words[1] >> 8) as u8,
            (raw_words[0] & 0xFF) as u8, (raw_words[0] >> 8) as u8,
        ],
    };
    f32::from_be_bytes(bytes)
}
```

---

## 7. SVG Thumbnail Vector Visual Standards

To guarantee visual clarity on high-DPI operator panels and WebGL node graphs, all catalog thumbnails (`icon.svg`) must strictly adhere to the following rules:

1. **Canvas Dimensions**: ViewBox must be strictly `0 0 120 120` pixels.
2. **Color Palette Alignment**: Colors must reference MythOS Design Tokens:
   - Primary Chassis: Dark Slate (`#1E293B`, token: `color.surface.chassis`).
   - Terminal Blocks: Industrial Green (`#16A34A`, token: `color.bus.ethernet`) or Slate (`#334155`).
   - Status LEDs:
     - Running: Emerald (`#10B981`)
     - Fault / Error: Amber / Red (`#EF4444`)
     - Power: Electric Blue (`#3B82F6`)
   - Accent Lines & Highlights: `#64748B`.
3. **Geometry & Grids**:
   - Stroke width: `2px` or `1.5px` with rounded caps (`stroke-linecap="round"`).
   - Zero inline JavaScript or foreignObject elements.
   - All text rendered as converted path outlines or standard SVG `<text>` using system sans-serif (`font-family="system-ui, -apple-system, sans-serif"`).
   - Crisp rendering via `shape-rendering="geometricPrecision"`.

---

## 8. Gold Standard Reference Device 1: Supmea SUP-P300 Pressure Transmitter

The **Supmea SUP-P300** is an industrial piezoresistive gauge/absolute pressure transmitter featuring an isolated 316L stainless steel diaphragm, integrated 24VDC reverse-polarity protection, and an RS-485 Modbus RTU interface.

```
                    +-----------------------------+
                    |    SUPMEA SUP-P300          |
                    |  Pressure Transmitter       |
                    +-----------------------------+
                                  |
               +------------------+------------------+
               |                  |                  |
               v                  v                  v
        [ Power 24VDC ]    [ RS485 A (+) ]    [ RS485 B (-) ]
        Terminal Pin 1     Terminal Pin 3     Terminal Pin 4
```

### 8.1 Electrical and Environmental Specifications
- **Supply Voltage**: 12–36 VDC (Nominal: 24 VDC).
- **Power Consumption**: $< 0.5\,\text{W}$.
- **Measurement Span**: 0 to 1.0 MPa (0 to 10.000 bar).
- **Accuracy Class**: $\pm 0.5\%\,\text{FS}$.
- **Electrical Connection**: Hirschmann DIN 43650A or IP67 M12 connector.
- **Factory Default Comm**: Slave ID `1`, 9600 bps, `8-N-1`, Big-Endian.

### 8.2 Modbus Register Map
- **Function Code**: `0x03` (Read Holding Registers), `0x06` (Write Single Register).

| Reg Hex | Reg Dec | Parameter Name | Data Type | Access | Units | Scaling / Formula | Description |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `0x0000` | 0 | Slave Address | UINT16 | RW | - | Raw integer | Device ID (1..247) |
| `0x0001` | 1 | Baud Rate Code | UINT16 | RW | - | Enum (0..7) | `0`: 1200, `1`: 2400, `2`: 4800, `3`: 9600, `4`: 19200 |
| `0x0002` | 2 | Engineering Unit | UINT16 | RO | - | Enum (0..8) | `0`: MPa, `1`: kPa, `3`: Bar, `7`: mH2O, `8`: mmH2O |
| `0x0003` | 3 | Decimal Places | UINT16 | RO | - | Integer (0..3) | Number of decimal places |
| `0x0004` | 4 | Measured Pressure Value | INT16 | RO | Configured | $P = \frac{\text{Raw}}{10^{\text{Dec}}}$ | Process measurement value |
| `0x0005` | 5 | Calibrated Zero | INT16 | RO | Configured | Zero offset | Calibration lower bound |
| `0x0006` | 6 | Calibrated Full Scale | INT16 | RO | Configured | Full range | Calibration upper bound |

---

### 8.3 Discovery and Fingerprinting Configuration
```yaml
discovery:
  probe_register: 40001
  expected_function_code: "0x03"
  expected_fc: "0x03"
  priority_baudrates: [9600, 19200, 4800]
  priority_parities: ["None", "Even"]
  default_id_range: [1, 32]
  probe_timeout_ms: 150
```
- **Probed Register**: 40001 (`0x0000`, Slave ID configuration register).
- **Behavior**: Fast probe interrogates Slave ID `1` at 9600 8-N-1 with 150 ms timeout.

---

## 9. Gold Standard Reference Device 2: SenTec Industrial Rainfall Sensor

The **SenTec Tipping Bucket Rainfall Sensor** is a meteorological-grade physical instrument. Rain collected by an aerodynamic funnel enters a precision balanced tipping bucket mechanism. Each tip of the bucket triggers a magnetic reed switch pulse corresponding to exactly $0.1\,\text{mm}$ of precipitation. An integrated MCU counts pulses, computes real-time rain intensity, maintains 24-hour historians, and exposes values over RS-485 Modbus RTU.

```
                    +-----------------------------+
                    |    SENTEC RAINFALL GAUGE    |
                    | Tipping Bucket Mechanism   |
                    +-----------------------------+
                                  |
               +------------------+------------------+
               |                  |                  |
               v                  v                  v
        [ Power 12-24VDC ] [ RS485 A (+) ]    [ RS485 B (-) ]
          Brown Wire          Yellow Wire        Blue Wire
```

### 9.1 Electrical and Environmental Specifications
- **Operating Voltage**: 5–24 VDC (Nominal: 12 VDC or 24 VDC).
- **Power Consumption**: $< 0.15\,\text{W}$ at 24 VDC.
- **Funnel Diameter**: 200 mm (Standard WMO meteorological aperture).
- **Resolution**: $0.1\,\text{mm}$ per pulse.
- **Factory Default Comm**: Slave ID `1`, 4800 bps, `8-N-1` (Configured: Slave ID `2`, 9600 bps).

### 9.2 Modbus Register Map
- **Function Code**: `0x03` (Read Holding Registers), `0x06` (Write Single Register).

| Reg Hex | Reg Dec | Parameter Name | Data Type | Access | Units | Scaling / Formula | Description |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `0x0000` | 0 | Daily Rainfall Accumulation | UINT16 | RO | mm | $\text{Daily} = \frac{\text{Raw}}{10.0}$ | Rain accumulated today (00:00 to now) |
| `0x0001` | 1 | Instantaneous Rainfall Rate | UINT16 | RO | mm/min | $\text{Rate} = \frac{\text{Raw}}{10.0}$ | Precipitation rate between polls |
| `0x0002` | 2 | Yesterday Total Rainfall | UINT16 | RO | mm | $\text{Yest} = \frac{\text{Raw}}{10.0}$ | 24-hour total for previous day |
| `0x0003` | 3 | Lifetime Total Rainfall | UINT16 | RO | mm | $\text{Life} = \frac{\text{Raw}}{10.0}$ | Total precipitation since MCU power-up |
| `0x0004` | 4 | Current Hour Rainfall | UINT16 | RO | mm | $\text{Hour} = \frac{\text{Raw}}{10.0}$ | Accumulation during active hour |
| `0x0005` | 5 | Previous Hour Rainfall | UINT16 | RO | mm | $\text{PrevHour} = \frac{\text{Raw}}{10.0}$| Total precipitation in previous 60 min |
| `0x0006` | 6 | 24-Hour Peak Rainfall Rate | UINT16 | RO | mm/hr | $\text{Peak} = \frac{\text{Raw}}{10.0}$ | Maximum rainfall recorded in 24 hrs |
| `0x0007` | 7 | 24-Hour Peak Timestamp | BCD16 | RO | - | BCD Encoded | Time of maximum rainfall occurrence |
| `0x07D0` | 2000 | Modbus Slave ID | UINT16 | RW | - | Raw integer | Slave address (1..247) |
| `0x07D1` | 2001 | Serial Baud Rate Code | UINT16 | RW | - | Enum (0..2) | `0`: 2400, `1`: 4800, `2`: 9600 bps |

### 9.3 Discovery and Fingerprinting Configuration
```yaml
discovery:
  probe_register: 40001
  expected_function_code: "0x03"
  expected_fc: "0x03"
  priority_baudrates: [9600, 4800, 19200]
  priority_parities: ["None"]
  default_id_range: [1, 32]
  probe_timeout_ms: 200
```
- **Probed Register**: 40001 (`0x0000`, Daily Rainfall Accumulation register).
- **Behavior**: Fast probe interrogates candidate IDs across [1..32] with 200 ms timeout per transaction.
