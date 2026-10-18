//! # PROFINET Discovery and Configuration Protocol (DCP) Engine
//!
//! Implements IEC 61158-6-10 compliant Layer 2 PROFINET DCP protocol parsing,
//! serialization, and a Dual-Role engine:
//!
//! 1. **DcpClient** (Engineer Laptop Mode):
//!    - Transmits L2 broadcast Identify requests (`0x8892`, Frame ID `0xFEFD`, Service `0x05`).
//!    - Identifies field devices (including unconfigured `0.0.0.0`) within $\le 2.5\,\text{s}$.
//!    - Issues remote Layer 2 `Set-IP` (`0xFEFC`, Service `0x04`) and `Signal-LED` commands.
//!
//! 2. **DcpServer** (Edge Controller Mode):
//!    - Listens for DCP Identify queries and responds with Station Name, Device/Vendor ID,
//!      Device Role, and IP Suite (Siemens PRONETA and TIA Portal compatible).
//!    - Accepts Layer 2 `Set-IP` commands to reconfigure controller networking without IP routing.
//!    - Reacts to `Signal-LED` flashing commands.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::discovery::raw_socket::{
    EthernetFrame, MacAddress, RawSocketError, RawSocketTransport, ETHERTYPE_PROFINET,
};
use crate::discovery::traits::{
    DiscoveredDevice as UniversalDiscoveredDevice, DiscoveryContext, DiscoveryError,
    ProtocolDiscoverer,
};

/// Frame ID for DCP Get/Set Request and Response (`0xFEFC`).
pub const FRAME_ID_DCP_GET_SET: u16 = 0xFEFC;
/// Frame ID for DCP Identify Multicast Request (`0xFEFD`).
pub const FRAME_ID_DCP_IDENTIFY_REQ: u16 = 0xFEFD;
/// Frame ID for DCP Identify Unicast Response (`0xFEFE`).
pub const FRAME_ID_DCP_IDENTIFY_RESP: u16 = 0xFEFE;
/// Frame ID for DCP Hello Request (`0xFEFB`).
pub const FRAME_ID_DCP_HELLO: u16 = 0xFEFB;

/// Service ID for DCP Get (`0x03`).
pub const SERVICE_ID_GET: u8 = 0x03;
/// Service ID for DCP Set (`0x04`).
pub const SERVICE_ID_SET: u8 = 0x04;
/// Service ID for DCP Identify (`0x05`).
pub const SERVICE_ID_IDENTIFY: u8 = 0x05;
/// Service ID for DCP Hello (`0x06`).
pub const SERVICE_ID_HELLO: u8 = 0x06;

/// Service Type: Request (`0x00`).
pub const SERVICE_TYPE_REQUEST: u8 = 0x00;
/// Service Type: Response Success (`0x01`).
pub const SERVICE_TYPE_RESPONSE_SUCCESS: u8 = 0x01;
/// Service Type: Response Error (`0x05`).
pub const SERVICE_TYPE_RESPONSE_ERROR: u8 = 0x05;

/// DCP Option: IP Parameter (`0x01`).
pub const OPTION_IP: u8 = 0x01;
/// DCP Sub-option: IP Parameter / Suite (`0x02`).
pub const SUBOPTION_IP_PARAMETER: u8 = 0x02;

/// DCP Option: Device Properties (`0x02`).
pub const OPTION_DEVICE_PROPERTIES: u8 = 0x02;
/// DCP Sub-option: Manufacturer specific / Type of Station (`0x01`).
pub const SUBOPTION_DEV_MANUFACTURER_SPECIFIC: u8 = 0x01;
/// DCP Sub-option: Name of Station (`0x02`).
pub const SUBOPTION_DEV_NAME_OF_STATION: u8 = 0x02;
/// DCP Sub-option: Device ID (`0x03`).
pub const SUBOPTION_DEV_DEVICE_ID: u8 = 0x03;
/// DCP Sub-option: Device Role (`0x04`).
pub const SUBOPTION_DEV_DEVICE_ROLE: u8 = 0x04;
/// DCP Sub-option: Device Instance (`0x07`).
pub const SUBOPTION_DEV_DEVICE_INSTANCE: u8 = 0x07;

/// DCP Option: Control (`0x05`).
pub const OPTION_CONTROL: u8 = 0x05;
/// DCP Sub-option: Signal LED / Flash (`0x04`).
pub const SUBOPTION_CTRL_SIGNAL: u8 = 0x04;
/// DCP Sub-option: Control Response (`0x05`).
pub const SUBOPTION_CTRL_RESPONSE: u8 = 0x05;
/// DCP Sub-option: Factory Reset (`0x06`).
pub const SUBOPTION_CTRL_FACTORY_RESET: u8 = 0x06;

/// DCP Option: All Selector (`0xFF`).
pub const OPTION_ALL_SELECTOR: u8 = 0xFF;
/// DCP Sub-option: All (`0xFF`).
pub const SUBOPTION_ALL: u8 = 0xFF;

/// BlockQualifier: Permanent IP configuration (stored in remanent NVRAM).
pub const BLOCK_QUALIFIER_PERMANENT: u16 = 0x0000;
/// BlockQualifier: Temporary IP configuration (active until reboot).
pub const BLOCK_QUALIFIER_TEMPORARY: u16 = 0x0001;

/// Signal Value: Flash LED once.
pub const SIGNAL_VALUE_FLASH_ONCE: u16 = 0x0100;

/// Default Vendor ID for Firmansyah Co (`0x04D2`).
pub const VENDOR_ID_FIRMANSYAH: u16 = 0x04D2;
/// Default Device ID for MythOS Edge Controller (`0x0001`).
pub const DEVICE_ID_MYTHOS_EDGE: u16 = 0x0001;

/// Device Role Flags (IEC 61158-6-10).
pub const DEVICE_ROLE_IO_DEVICE: u8 = 0x01;
pub const DEVICE_ROLE_IO_CONTROLLER: u8 = 0x02;
pub const DEVICE_ROLE_IO_MULTIDEVICE: u8 = 0x04;
pub const DEVICE_ROLE_PN_SUPERVISOR: u8 = 0x08;

/// Minimum DCP header length on top of Ethernet payload:
/// Frame ID (2) + Service ID (1) + Service Type (1) + XID (4) + Response Delay (2) + Data Length (2) = 12 bytes.
pub const DCP_HEADER_LEN: usize = 12;

/// Errors arising from PROFINET DCP protocol parsing, validation, and operations.
#[derive(Debug, thiserror::Error)]
pub enum DcpError {
    #[error("Packet too short for DCP header: expected at least {expected} bytes, got {actual}")]
    HeaderTooShort { expected: usize, actual: usize },

    #[error("Invalid DCP frame ID: 0x{0:04X}")]
    InvalidFrameId(u16),

    #[error("Data length mismatch: declared {declared} bytes, but only {actual} bytes remaining")]
    DataLengthMismatch { declared: usize, actual: usize },

    #[error("TLV block length error: option={option}, suboption={suboption}, declared={declared}, available={available}")]
    BlockLengthError {
        option: u8,
        suboption: u8,
        declared: usize,
        available: usize,
    },

    #[error("Malformed block content: {0}")]
    MalformedBlock(String),

    #[error("Invalid UTF-8 station name: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("Underlying Layer 2 raw socket error: {0}")]
    RawSocket(#[from] RawSocketError),

    #[error("DCP operation timed out after {0:?}")]
    Timeout(Duration),

    #[error("Target device returned error response: result_code=0x{0:02X}")]
    DeviceErrorResponse(u8),
}

/// Fixed DCP PDU header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DcpHeader {
    pub frame_id: u16,
    pub service_id: u8,
    pub service_type: u8,
    pub xid: u32,
    pub response_delay: u16,
    pub data_length: u16,
}

/// Parsed or constructed PROFINET DCP TLV block payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DcpBlockPayload {
    /// Option 0x01, Sub-option 0x02: IPv4 Suite (IP, Subnet, Gateway).
    IpParameter {
        qualifier: u16,
        ip: Ipv4Addr,
        subnet: Ipv4Addr,
        gateway: Ipv4Addr,
    },
    /// Option 0x02, Sub-option 0x02: Name of Station (hostname / identifier).
    NameOfStation { qualifier: u16, name: String },
    /// Option 0x02, Sub-option 0x03: Device & Vendor ID.
    DeviceId {
        qualifier: u16,
        vendor_id: u16,
        device_id: u16,
    },
    /// Option 0x02, Sub-option 0x04: Device Role (IO Device, Controller, Supervisor).
    DeviceRole {
        qualifier: u16,
        role: u8,
        reserved: u8,
    },
    /// Option 0x05, Sub-option 0x04: Signal LED / Flash.
    ControlSignal { qualifier: u16, signal_value: u16 },
    /// Option 0x05, Sub-option 0x05: Control Response.
    ControlResponse {
        response_option: u8,
        response_suboption: u8,
        result_error: u8,
    },
    /// Option 0xFF, Sub-option 0xFF: All Selector.
    AllSelector,
    /// Unhandled or vendor-specific raw block payload.
    Raw {
        option: u8,
        suboption: u8,
        data: Vec<u8>,
    },
}

/// Full TLV block with option, suboption, and payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DcpBlock {
    pub option: u8,
    pub suboption: u8,
    pub payload: DcpBlockPayload,
}

impl DcpBlock {
    /// Create an IP Parameter TLV block.
    pub fn ip_parameter(
        ip: Ipv4Addr,
        subnet: Ipv4Addr,
        gateway: Ipv4Addr,
        permanent: bool,
    ) -> Self {
        let qualifier = if permanent {
            BLOCK_QUALIFIER_PERMANENT
        } else {
            BLOCK_QUALIFIER_TEMPORARY
        };
        Self {
            option: OPTION_IP,
            suboption: SUBOPTION_IP_PARAMETER,
            payload: DcpBlockPayload::IpParameter {
                qualifier,
                ip,
                subnet,
                gateway,
            },
        }
    }

    /// Create a NameOfStation TLV block.
    pub fn name_of_station(name: impl Into<String>) -> Self {
        Self {
            option: OPTION_DEVICE_PROPERTIES,
            suboption: SUBOPTION_DEV_NAME_OF_STATION,
            payload: DcpBlockPayload::NameOfStation {
                qualifier: 0x0000,
                name: name.into(),
            },
        }
    }

    /// Create a DeviceId TLV block.
    pub fn device_id(vendor_id: u16, device_id: u16) -> Self {
        Self {
            option: OPTION_DEVICE_PROPERTIES,
            suboption: SUBOPTION_DEV_DEVICE_ID,
            payload: DcpBlockPayload::DeviceId {
                qualifier: 0x0000,
                vendor_id,
                device_id,
            },
        }
    }

    /// Create a DeviceRole TLV block.
    pub fn device_role(role: u8) -> Self {
        Self {
            option: OPTION_DEVICE_PROPERTIES,
            suboption: SUBOPTION_DEV_DEVICE_ROLE,
            payload: DcpBlockPayload::DeviceRole {
                qualifier: 0x0000,
                role,
                reserved: 0x00,
            },
        }
    }

    /// Create a Signal LED TLV block.
    pub fn signal_led() -> Self {
        Self {
            option: OPTION_CONTROL,
            suboption: SUBOPTION_CTRL_SIGNAL,
            payload: DcpBlockPayload::ControlSignal {
                qualifier: 0x0000,
                signal_value: SIGNAL_VALUE_FLASH_ONCE,
            },
        }
    }

    /// Create a Control Response TLV block.
    pub fn control_response(option: u8, suboption: u8, result_error: u8) -> Self {
        Self {
            option: OPTION_CONTROL,
            suboption: SUBOPTION_CTRL_RESPONSE,
            payload: DcpBlockPayload::ControlResponse {
                response_option: option,
                response_suboption: suboption,
                result_error,
            },
        }
    }

    /// Create an All Selector block.
    pub fn all_selector() -> Self {
        Self {
            option: OPTION_ALL_SELECTOR,
            suboption: SUBOPTION_ALL,
            payload: DcpBlockPayload::AllSelector,
        }
    }

    /// Serialize this TLV block into bytes, including the 2-octet padding when length is odd.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(self.option);
        out.push(self.suboption);

        let data = match &self.payload {
            DcpBlockPayload::IpParameter {
                qualifier,
                ip,
                subnet,
                gateway,
            } => {
                let mut b = Vec::with_capacity(14);
                b.extend_from_slice(&qualifier.to_be_bytes());
                b.extend_from_slice(&ip.octets());
                b.extend_from_slice(&subnet.octets());
                b.extend_from_slice(&gateway.octets());
                b
            }
            DcpBlockPayload::NameOfStation { qualifier, name } => {
                let mut b = Vec::with_capacity(2 + name.len());
                b.extend_from_slice(&qualifier.to_be_bytes());
                b.extend_from_slice(name.as_bytes());
                b
            }
            DcpBlockPayload::DeviceId {
                qualifier,
                vendor_id,
                device_id,
            } => {
                let mut b = Vec::with_capacity(6);
                b.extend_from_slice(&qualifier.to_be_bytes());
                b.extend_from_slice(&vendor_id.to_be_bytes());
                b.extend_from_slice(&device_id.to_be_bytes());
                b
            }
            DcpBlockPayload::DeviceRole {
                qualifier,
                role,
                reserved,
            } => {
                let mut b = Vec::with_capacity(4);
                b.extend_from_slice(&qualifier.to_be_bytes());
                b.push(*role);
                b.push(*reserved);
                b
            }
            DcpBlockPayload::ControlSignal {
                qualifier,
                signal_value,
            } => {
                let mut b = Vec::with_capacity(4);
                b.extend_from_slice(&qualifier.to_be_bytes());
                b.extend_from_slice(&signal_value.to_be_bytes());
                b
            }
            DcpBlockPayload::ControlResponse {
                response_option,
                response_suboption,
                result_error,
            } => {
                vec![*response_option, *response_suboption, *result_error]
            }
            DcpBlockPayload::AllSelector => Vec::new(),
            DcpBlockPayload::Raw { data, .. } => data.clone(),
        };

        let len = data.len() as u16;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&data);

        // IEEE / IEC 61158-6-10 padding rule: pad byte when length is odd
        if data.len() % 2 != 0 {
            out.push(0x00);
        }

        out
    }

    /// Parse a single TLV block from a byte slice. Returns `(DcpBlock, bytes_consumed)`.
    pub fn parse(buffer: &[u8]) -> Result<(Self, usize), DcpError> {
        if buffer.len() < 4 {
            return Err(DcpError::HeaderTooShort {
                expected: 4,
                actual: buffer.len(),
            });
        }

        let option = buffer[0];
        let suboption = buffer[1];
        let block_len = u16::from_be_bytes([buffer[2], buffer[3]]) as usize;

        // Total wire bytes consumed including optional alignment padding
        let pad_len = block_len % 2;
        let wire_len = 4 + block_len + pad_len;

        if buffer.len() < 4 + block_len {
            return Err(DcpError::BlockLengthError {
                option,
                suboption,
                declared: block_len,
                available: buffer.len() - 4,
            });
        }

        let data = &buffer[4..4 + block_len];

        let payload = match (option, suboption) {
            (OPTION_ALL_SELECTOR, SUBOPTION_ALL) => DcpBlockPayload::AllSelector,
            (OPTION_IP, SUBOPTION_IP_PARAMETER) => {
                if data.len() < 14 {
                    return Err(DcpError::MalformedBlock(format!(
                        "IP parameter block too short: expected 14 bytes, got {}",
                        data.len()
                    )));
                }
                let qualifier = u16::from_be_bytes([data[0], data[1]]);
                let ip = Ipv4Addr::new(data[2], data[3], data[4], data[5]);
                let subnet = Ipv4Addr::new(data[6], data[7], data[8], data[9]);
                let gateway = Ipv4Addr::new(data[10], data[11], data[12], data[13]);
                DcpBlockPayload::IpParameter {
                    qualifier,
                    ip,
                    subnet,
                    gateway,
                }
            }
            (OPTION_DEVICE_PROPERTIES, SUBOPTION_DEV_NAME_OF_STATION) => {
                if data.len() < 2 {
                    return Err(DcpError::MalformedBlock(
                        "NameOfStation block shorter than 2 bytes".into(),
                    ));
                }
                let qualifier = u16::from_be_bytes([data[0], data[1]]);
                let name = String::from_utf8(data[2..].to_vec())?;
                DcpBlockPayload::NameOfStation { qualifier, name }
            }
            (OPTION_DEVICE_PROPERTIES, SUBOPTION_DEV_DEVICE_ID) => {
                if data.len() < 6 {
                    return Err(DcpError::MalformedBlock(
                        "DeviceId block shorter than 6 bytes".into(),
                    ));
                }
                let qualifier = u16::from_be_bytes([data[0], data[1]]);
                let vendor_id = u16::from_be_bytes([data[2], data[3]]);
                let device_id = u16::from_be_bytes([data[4], data[5]]);
                DcpBlockPayload::DeviceId {
                    qualifier,
                    vendor_id,
                    device_id,
                }
            }
            (OPTION_DEVICE_PROPERTIES, SUBOPTION_DEV_DEVICE_ROLE) => {
                if data.len() < 4 {
                    return Err(DcpError::MalformedBlock(
                        "DeviceRole block shorter than 4 bytes".into(),
                    ));
                }
                let qualifier = u16::from_be_bytes([data[0], data[1]]);
                let role = data[2];
                let reserved = data[3];
                DcpBlockPayload::DeviceRole {
                    qualifier,
                    role,
                    reserved,
                }
            }
            (OPTION_CONTROL, SUBOPTION_CTRL_SIGNAL) => {
                if data.len() < 4 {
                    return Err(DcpError::MalformedBlock(
                        "ControlSignal block shorter than 4 bytes".into(),
                    ));
                }
                let qualifier = u16::from_be_bytes([data[0], data[1]]);
                let signal_value = u16::from_be_bytes([data[2], data[3]]);
                DcpBlockPayload::ControlSignal {
                    qualifier,
                    signal_value,
                }
            }
            (OPTION_CONTROL, SUBOPTION_CTRL_RESPONSE) => {
                if data.len() < 3 {
                    return Err(DcpError::MalformedBlock(
                        "ControlResponse block shorter than 3 bytes".into(),
                    ));
                }
                let response_option = data[0];
                let response_suboption = data[1];
                let result_error = data[2];
                DcpBlockPayload::ControlResponse {
                    response_option,
                    response_suboption,
                    result_error,
                }
            }
            _ => DcpBlockPayload::Raw {
                option,
                suboption,
                data: data.to_vec(),
            },
        };

        // Determine consumed bytes on buffer: if buffer contains padding, consume wire_len up to buffer.len()
        let consumed = wire_len.min(buffer.len());

        Ok((
            Self {
                option,
                suboption,
                payload,
            },
            consumed,
        ))
    }
}

/// A parsed or constructed PROFINET DCP Protocol Data Unit (PDU).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DcpPdu {
    pub header: DcpHeader,
    pub blocks: Vec<DcpBlock>,
}

impl DcpPdu {
    /// Create a new DCP PDU.
    pub fn new(header: DcpHeader, blocks: Vec<DcpBlock>) -> Self {
        Self { header, blocks }
    }

    /// Serialize the DCP PDU into raw bytes (ready to be encapsulated in an EthernetFrame payload).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut blocks_bytes = Vec::new();
        for block in &self.blocks {
            blocks_bytes.extend_from_slice(&block.to_bytes());
        }

        let mut out = Vec::with_capacity(DCP_HEADER_LEN + blocks_bytes.len());
        out.extend_from_slice(&self.header.frame_id.to_be_bytes());
        out.push(self.header.service_id);
        out.push(self.header.service_type);
        out.extend_from_slice(&self.header.xid.to_be_bytes());
        out.extend_from_slice(&self.header.response_delay.to_be_bytes());

        // Update data length with actual serialized blocks length
        let data_len = blocks_bytes.len() as u16;
        out.extend_from_slice(&data_len.to_be_bytes());
        out.extend_from_slice(&blocks_bytes);

        out
    }

    /// Parse a DCP PDU from raw payload bytes.
    pub fn parse(buffer: &[u8]) -> Result<Self, DcpError> {
        if buffer.len() < DCP_HEADER_LEN {
            return Err(DcpError::HeaderTooShort {
                expected: DCP_HEADER_LEN,
                actual: buffer.len(),
            });
        }

        let frame_id = u16::from_be_bytes([buffer[0], buffer[1]]);
        let service_id = buffer[2];
        let service_type = buffer[3];
        let xid = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
        let response_delay = u16::from_be_bytes([buffer[8], buffer[9]]);
        let declared_data_len = u16::from_be_bytes([buffer[10], buffer[11]]) as usize;

        let available_data = buffer.len() - DCP_HEADER_LEN;
        if available_data < declared_data_len {
            return Err(DcpError::DataLengthMismatch {
                declared: declared_data_len,
                actual: available_data,
            });
        }

        let mut blocks = Vec::new();
        let mut cursor = DCP_HEADER_LEN;
        let data_end = DCP_HEADER_LEN + declared_data_len;

        while cursor < data_end {
            let slice = &buffer[cursor..data_end];
            let (block, consumed) = DcpBlock::parse(slice)?;
            blocks.push(block);
            if consumed == 0 {
                break;
            }
            cursor += consumed;
        }

        Ok(Self {
            header: DcpHeader {
                frame_id,
                service_id,
                service_type,
                xid,
                response_delay,
                data_length: declared_data_len as u16,
            },
            blocks,
        })
    }
}

/// Information model of a field device discovered via PROFINET DCP.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DcpDiscoveredDevice {
    /// MAC address of the device.
    pub mac: MacAddress,
    /// PROFINET Station Name (e.g. `mythos-edge-01` or `et200sp-rack01`).
    pub station_name: String,
    /// Currently configured IPv4 address (`0.0.0.0` for factory-fresh unconfigured devices).
    pub ip: Ipv4Addr,
    /// Subnet mask.
    pub subnet_mask: Ipv4Addr,
    /// Default gateway.
    pub gateway: Ipv4Addr,
    /// PROFINET Vendor ID (e.g. 0x04D2 for Firmansyah Co, 0x002A for Siemens).
    pub vendor_id: u16,
    /// PROFINET Device ID.
    pub device_id: u16,
    /// PROFINET Device Role bitmask.
    pub device_role: u8,
    /// Whether the device has a valid non-zero IP configuration.
    pub is_configured: bool,
}

impl DcpDiscoveredDevice {
    /// Extract device information from an Identify Response Ethernet frame.
    pub fn from_identify_frame(frame: &EthernetFrame) -> Result<Self, DcpError> {
        let pdu = DcpPdu::parse(&frame.payload)?;

        if pdu.header.frame_id != FRAME_ID_DCP_IDENTIFY_RESP
            || pdu.header.service_id != SERVICE_ID_IDENTIFY
            || pdu.header.service_type != SERVICE_TYPE_RESPONSE_SUCCESS
        {
            return Err(DcpError::InvalidFrameId(pdu.header.frame_id));
        }

        let mut station_name = String::new();
        let mut ip = Ipv4Addr::UNSPECIFIED;
        let mut subnet_mask = Ipv4Addr::UNSPECIFIED;
        let mut gateway = Ipv4Addr::UNSPECIFIED;
        let mut vendor_id = 0u16;
        let mut device_id = 0u16;
        let mut device_role = DEVICE_ROLE_IO_DEVICE;

        for block in pdu.blocks {
            match block.payload {
                DcpBlockPayload::NameOfStation { name, .. } => {
                    station_name = name;
                }
                DcpBlockPayload::IpParameter {
                    ip: b_ip,
                    subnet: b_sub,
                    gateway: b_gw,
                    ..
                } => {
                    ip = b_ip;
                    subnet_mask = b_sub;
                    gateway = b_gw;
                }
                DcpBlockPayload::DeviceId {
                    vendor_id: v,
                    device_id: d,
                    ..
                } => {
                    vendor_id = v;
                    device_id = d;
                }
                DcpBlockPayload::DeviceRole { role, .. } => {
                    device_role = role;
                }
                _ => {}
            }
        }

        let is_configured = !ip.is_unspecified();

        Ok(Self {
            mac: frame.source,
            station_name,
            ip,
            subnet_mask,
            gateway,
            vendor_id,
            device_id,
            device_role,
            is_configured,
        })
    }

    /// Convert into the universal `DiscoveredDevice` model used across the coordinator engine.
    pub fn into_universal(self) -> UniversalDiscoveredDevice {
        let mut properties = HashMap::new();
        properties.insert("vendor_id".to_string(), format!("0x{:04X}", self.vendor_id));
        properties.insert("device_id".to_string(), format!("0x{:04X}", self.device_id));
        properties.insert(
            "device_role".to_string(),
            format!("0x{:02X}", self.device_role),
        );
        properties.insert("subnet_mask".to_string(), self.subnet_mask.to_string());
        properties.insert("gateway".to_string(), self.gateway.to_string());
        properties.insert("configured".to_string(), self.is_configured.to_string());

        let dev_id = if !self.station_name.is_empty() {
            self.station_name.clone()
        } else {
            self.mac.to_string()
        };

        UniversalDiscoveredDevice {
            device_id: dev_id,
            protocol: "profinet-dcp".to_string(),
            station_name: Some(self.station_name),
            mac_address: Some(self.mac.to_string()),
            ip_address: Some(self.ip.to_string()),
            modbus_slave_id: None,
            fingerprint_model: None,
            vendor: Some(format!("0x{:04X}", self.vendor_id)),
            confidence: 1.0,
            properties,
        }
    }
}

// ---------------------------------------------------------------------------
// DUAL-ROLE ENGINE: ROLE 1 — DCP CLIENT / SCANNER (Engineer Laptop Mode)
// ---------------------------------------------------------------------------

/// PROFINET DCP Client (Role 1: Scanner / Engineering Tool).
pub struct DcpClient {
    source_mac: MacAddress,
    xid_seq: AtomicU32,
}

impl DcpClient {
    /// Create a new DCP Client with the specified local source MAC address.
    pub fn new(source_mac: MacAddress) -> Self {
        Self {
            source_mac,
            xid_seq: AtomicU32::new(1),
        }
    }

    /// Generate next transaction ID (XID).
    pub fn next_xid(&self) -> u32 {
        self.xid_seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Build a Layer 2 PROFINET DCP Identify broadcast frame.
    ///
    /// If `station_name_filter` is None, an `AllSelector` block is attached to discover all devices.
    /// Otherwise, a filter block is attached to match only the specified station name.
    pub fn build_identify_request(
        &self,
        xid: u32,
        station_name_filter: Option<&str>,
    ) -> EthernetFrame {
        let block = match station_name_filter {
            None => DcpBlock::all_selector(),
            Some(name) => DcpBlock::name_of_station(name),
        };

        let pdu = DcpPdu::new(
            DcpHeader {
                frame_id: FRAME_ID_DCP_IDENTIFY_REQ,
                service_id: SERVICE_ID_IDENTIFY,
                service_type: SERVICE_TYPE_REQUEST,
                xid,
                response_delay: 0x0001, // 1 * 100ms
                data_length: 0,
            },
            vec![block],
        );

        EthernetFrame::new(
            MacAddress::BROADCAST,
            self.source_mac,
            ETHERTYPE_PROFINET,
            pdu.to_bytes(),
        )
    }

    /// Build a Layer 2 PROFINET DCP Set-IP request frame.
    pub fn build_set_ip_request(
        &self,
        target_mac: MacAddress,
        xid: u32,
        ip: Ipv4Addr,
        subnet: Ipv4Addr,
        gateway: Ipv4Addr,
        permanent: bool,
    ) -> EthernetFrame {
        let block = DcpBlock::ip_parameter(ip, subnet, gateway, permanent);
        let pdu = DcpPdu::new(
            DcpHeader {
                frame_id: FRAME_ID_DCP_GET_SET,
                service_id: SERVICE_ID_SET,
                service_type: SERVICE_TYPE_REQUEST,
                xid,
                response_delay: 0x0000,
                data_length: 0,
            },
            vec![block],
        );

        EthernetFrame::new(
            target_mac,
            self.source_mac,
            ETHERTYPE_PROFINET,
            pdu.to_bytes(),
        )
    }

    /// Build a Layer 2 PROFINET DCP Signal-LED (Blink) request frame.
    pub fn build_signal_led_request(&self, target_mac: MacAddress, xid: u32) -> EthernetFrame {
        let block = DcpBlock::signal_led();
        let pdu = DcpPdu::new(
            DcpHeader {
                frame_id: FRAME_ID_DCP_GET_SET,
                service_id: SERVICE_ID_SET,
                service_type: SERVICE_TYPE_REQUEST,
                xid,
                response_delay: 0x0000,
                data_length: 0,
            },
            vec![block],
        );

        EthernetFrame::new(
            target_mac,
            self.source_mac,
            ETHERTYPE_PROFINET,
            pdu.to_bytes(),
        )
    }

    /// Execute a network-wide DCP Identify sweep over the provided transport.
    ///
    /// Identifies all responsive field devices within the specified timeout (SYS-REQ-040: <= 2.5s).
    pub async fn identify_sweep(
        &self,
        transport: &(dyn RawSocketTransport + Send + Sync),
        station_name_filter: Option<&str>,
        timeout: Duration,
    ) -> Result<Vec<DcpDiscoveredDevice>, DcpError> {
        let xid = self.next_xid();
        let req_frame = self.build_identify_request(xid, station_name_filter);
        let req_bytes = req_frame.to_bytes(true);

        transport.send_frame(&req_bytes).await?;

        let mut devices = Vec::new();
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, transport.recv_frame()).await {
                Ok(Ok(raw_resp)) => {
                    if let Ok(frame) = EthernetFrame::parse(&raw_resp) {
                        if frame.ethertype == ETHERTYPE_PROFINET {
                            if let Ok(dev) = DcpDiscoveredDevice::from_identify_frame(&frame) {
                                // Prevent duplicate entries from multiple responses
                                if !devices
                                    .iter()
                                    .any(|d: &DcpDiscoveredDevice| d.mac == dev.mac)
                                {
                                    debug!(mac = %dev.mac, station = %dev.station_name, ip = %dev.ip, "Discovered device via PROFINET DCP");
                                    devices.push(dev);
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => return Err(DcpError::RawSocket(e)),
                Err(_) => break, // Timeout elapsed
            }
        }

        Ok(devices)
    }

    /// Send a Set-IP command to a target device and wait for DCP acknowledgement.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_ip(
        &self,
        transport: &(dyn RawSocketTransport + Send + Sync),
        target_mac: MacAddress,
        ip: Ipv4Addr,
        subnet: Ipv4Addr,
        gateway: Ipv4Addr,
        permanent: bool,
        timeout: Duration,
    ) -> Result<(), DcpError> {
        let xid = self.next_xid();
        let req_frame = self.build_set_ip_request(target_mac, xid, ip, subnet, gateway, permanent);
        let req_bytes = req_frame.to_bytes(true);

        transport.send_frame(&req_bytes).await?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, transport.recv_frame()).await {
                Ok(Ok(raw_resp)) => {
                    if let Ok(frame) = EthernetFrame::parse(&raw_resp) {
                        if frame.ethertype == ETHERTYPE_PROFINET && frame.source == target_mac {
                            if let Ok(pdu) = DcpPdu::parse(&frame.payload) {
                                if pdu.header.frame_id == FRAME_ID_DCP_GET_SET
                                    && pdu.header.service_id == SERVICE_ID_SET
                                    && pdu.header.xid == xid
                                {
                                    for block in &pdu.blocks {
                                        if let DcpBlockPayload::ControlResponse {
                                            result_error,
                                            ..
                                        } = block.payload
                                        {
                                            if result_error != 0x00 {
                                                return Err(DcpError::DeviceErrorResponse(
                                                    result_error,
                                                ));
                                            }
                                        }
                                    }
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => return Err(DcpError::RawSocket(e)),
                Err(_) => return Err(DcpError::Timeout(timeout)),
            }
        }

        Err(DcpError::Timeout(timeout))
    }

    /// Send a Signal-LED (Blink) command to a target device and wait for DCP acknowledgement.
    pub async fn signal_led(
        &self,
        transport: &(dyn RawSocketTransport + Send + Sync),
        target_mac: MacAddress,
        timeout: Duration,
    ) -> Result<(), DcpError> {
        let xid = self.next_xid();
        let req_frame = self.build_signal_led_request(target_mac, xid);
        let req_bytes = req_frame.to_bytes(true);

        transport.send_frame(&req_bytes).await?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, transport.recv_frame()).await {
                Ok(Ok(raw_resp)) => {
                    if let Ok(frame) = EthernetFrame::parse(&raw_resp) {
                        if frame.ethertype == ETHERTYPE_PROFINET && frame.source == target_mac {
                            if let Ok(pdu) = DcpPdu::parse(&frame.payload) {
                                if pdu.header.frame_id == FRAME_ID_DCP_GET_SET
                                    && pdu.header.service_id == SERVICE_ID_SET
                                    && pdu.header.xid == xid
                                {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => return Err(DcpError::RawSocket(e)),
                Err(_) => return Err(DcpError::Timeout(timeout)),
            }
        }

        Err(DcpError::Timeout(timeout))
    }
}

// ---------------------------------------------------------------------------
// DUAL-ROLE ENGINE: ROLE 2 — DCP SERVER / RESPONDER (Edge Controller Mode)
// ---------------------------------------------------------------------------

/// Configuration parameters for the DCP Server / Responder Daemon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DcpServerConfig {
    pub mac: MacAddress,
    pub station_name: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub device_role: u8,
    pub ip: Ipv4Addr,
    pub subnet: Ipv4Addr,
    pub gateway: Ipv4Addr,
}

impl Default for DcpServerConfig {
    fn default() -> Self {
        Self {
            mac: MacAddress::new([0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]),
            station_name: "mythos-edge-01".to_string(),
            vendor_id: VENDOR_ID_FIRMANSYAH,
            device_id: DEVICE_ID_MYTHOS_EDGE,
            device_role: DEVICE_ROLE_IO_DEVICE | DEVICE_ROLE_IO_CONTROLLER,
            ip: Ipv4Addr::new(0, 0, 0, 0),
            subnet: Ipv4Addr::new(255, 255, 255, 0),
            gateway: Ipv4Addr::new(0, 0, 0, 0),
        }
    }
}

/// Dynamic runtime state of the DCP Server.
struct DcpServerState {
    config: DcpServerConfig,
    blink_counter: u64,
}

/// PROFINET DCP Server / Responder Daemon (Role 2: Edge Controller Mode).
///
/// Answers DCP Identify requests and handles remote `Set-IP` and `Signal-LED` commands.
pub struct DcpServer {
    state: Mutex<DcpServerState>,
}

impl DcpServer {
    /// Create a new DCP Server with given configuration.
    pub fn new(config: DcpServerConfig) -> Self {
        Self {
            state: Mutex::new(DcpServerState {
                config,
                blink_counter: 0,
            }),
        }
    }

    /// Get a snapshot of current configuration.
    pub async fn config(&self) -> DcpServerConfig {
        let state = self.state.lock().await;
        state.config.clone()
    }

    /// Get total number of Signal-LED blink requests received.
    pub async fn blink_count(&self) -> u64 {
        let state = self.state.lock().await;
        state.blink_counter
    }

    /// Process an incoming Ethernet frame.
    ///
    /// If the frame is a relevant PROFINET DCP request (Identify or Set), generates
    /// and returns the corresponding response `EthernetFrame`.
    pub async fn handle_frame(
        &self,
        frame: &EthernetFrame,
    ) -> Result<Option<EthernetFrame>, DcpError> {
        if frame.ethertype != ETHERTYPE_PROFINET {
            return Ok(None);
        }

        let mut state = self.state.lock().await;

        // Check if addressed to us, broadcast, or PROFINET multicast
        let is_addressed = frame.destination == state.config.mac
            || frame.destination.is_broadcast()
            || frame.destination == MacAddress::PROFINET_MULTICAST;

        if !is_addressed {
            return Ok(None);
        }

        let pdu = match DcpPdu::parse(&frame.payload) {
            Ok(p) => p,
            Err(e) => {
                debug!("Failed parsing DCP PDU: {}", e);
                return Ok(None);
            }
        };

        match (
            pdu.header.frame_id,
            pdu.header.service_id,
            pdu.header.service_type,
        ) {
            (FRAME_ID_DCP_IDENTIFY_REQ, SERVICE_ID_IDENTIFY, SERVICE_TYPE_REQUEST) => {
                // Check station name filter
                let mut should_respond = false;
                for block in &pdu.blocks {
                    match &block.payload {
                        DcpBlockPayload::AllSelector => {
                            should_respond = true;
                            break;
                        }
                        DcpBlockPayload::NameOfStation { name, .. }
                            if name.eq_ignore_ascii_case(&state.config.station_name) =>
                        {
                            should_respond = true;
                            break;
                        }
                        _ => {}
                    }
                }

                if !should_respond {
                    return Ok(None);
                }

                info!(
                    requester = %frame.source,
                    station = %state.config.station_name,
                    "Responding to PROFINET DCP Identify request"
                );

                // Build Identify Response PDU
                let resp_blocks = vec![
                    DcpBlock::name_of_station(&state.config.station_name),
                    DcpBlock::device_id(state.config.vendor_id, state.config.device_id),
                    DcpBlock::device_role(state.config.device_role),
                    DcpBlock::ip_parameter(
                        state.config.ip,
                        state.config.subnet,
                        state.config.gateway,
                        true,
                    ),
                ];

                let resp_pdu = DcpPdu::new(
                    DcpHeader {
                        frame_id: FRAME_ID_DCP_IDENTIFY_RESP,
                        service_id: SERVICE_ID_IDENTIFY,
                        service_type: SERVICE_TYPE_RESPONSE_SUCCESS,
                        xid: pdu.header.xid,
                        response_delay: 0x0000,
                        data_length: 0,
                    },
                    resp_blocks,
                );

                // Identify response is sent unicast back to the scanner's source MAC
                let resp_frame = EthernetFrame::new(
                    frame.source,
                    state.config.mac,
                    ETHERTYPE_PROFINET,
                    resp_pdu.to_bytes(),
                );

                Ok(Some(resp_frame))
            }
            (FRAME_ID_DCP_GET_SET, SERVICE_ID_SET, SERVICE_TYPE_REQUEST) => {
                // Set command must be addressed to our MAC (or broadcast)
                if frame.destination != state.config.mac && !frame.destination.is_broadcast() {
                    return Ok(None);
                }

                let mut resp_blocks = Vec::new();

                for block in &pdu.blocks {
                    match &block.payload {
                        DcpBlockPayload::IpParameter {
                            ip,
                            subnet,
                            gateway,
                            ..
                        } => {
                            info!(
                                old_ip = %state.config.ip,
                                new_ip = %ip,
                                subnet = %subnet,
                                gateway = %gateway,
                                "Applying remote Layer 2 DCP Set-IP configuration"
                            );
                            state.config.ip = *ip;
                            state.config.subnet = *subnet;
                            state.config.gateway = *gateway;

                            resp_blocks.push(DcpBlock::control_response(
                                OPTION_IP,
                                SUBOPTION_IP_PARAMETER,
                                0x00, // OK
                            ));
                        }
                        DcpBlockPayload::ControlSignal { .. } => {
                            state.blink_counter += 1;
                            info!(
                                blink_count = state.blink_counter,
                                "Received DCP Signal-LED request: flashing physical LED"
                            );

                            resp_blocks.push(DcpBlock::control_response(
                                OPTION_CONTROL,
                                SUBOPTION_CTRL_SIGNAL,
                                0x00, // OK
                            ));
                        }
                        _ => {}
                    }
                }

                let resp_pdu = DcpPdu::new(
                    DcpHeader {
                        frame_id: FRAME_ID_DCP_GET_SET,
                        service_id: SERVICE_ID_SET,
                        service_type: SERVICE_TYPE_RESPONSE_SUCCESS,
                        xid: pdu.header.xid,
                        response_delay: 0x0000,
                        data_length: 0,
                    },
                    resp_blocks,
                );

                let resp_frame = EthernetFrame::new(
                    frame.source,
                    state.config.mac,
                    ETHERTYPE_PROFINET,
                    resp_pdu.to_bytes(),
                );

                Ok(Some(resp_frame))
            }
            _ => Ok(None),
        }
    }

    /// Background listener loop driving the responder over a raw socket transport.
    pub async fn run_responder(
        self: Arc<Self>,
        transport: Arc<dyn RawSocketTransport + Send + Sync>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) {
        debug!("DCP Responder background task started");
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        debug!("DCP Responder received shutdown signal");
                        break;
                    }
                }
                recv_res = transport.recv_frame() => {
                    match recv_res {
                        Ok(raw_frame) => {
                            if let Ok(frame) = EthernetFrame::parse(&raw_frame) {
                                match self.handle_frame(&frame).await {
                                    Ok(Some(resp_frame)) => {
                                        let bytes = resp_frame.to_bytes(true);
                                        if let Err(e) = transport.send_frame(&bytes).await {
                                            warn!("Failed sending DCP response: {}", e);
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        warn!("Error processing DCP frame: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error receiving frame in DCP responder: {}", e);
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }
                }
            }
        }
    }
}

/// Universal Protocol Discoverer adapter for PROFINET DCP.
pub struct DcpDiscoverer {
    client: DcpClient,
    transport: Arc<dyn RawSocketTransport + Send + Sync>,
}

impl DcpDiscoverer {
    pub fn new(
        source_mac: MacAddress,
        transport: Arc<dyn RawSocketTransport + Send + Sync>,
    ) -> Self {
        Self {
            client: DcpClient::new(source_mac),
            transport,
        }
    }
}

#[async_trait::async_trait]
impl ProtocolDiscoverer for DcpDiscoverer {
    fn protocol_id(&self) -> &str {
        "profinet-dcp"
    }

    async fn discover(
        &self,
        ctx: &DiscoveryContext,
    ) -> Result<Vec<UniversalDiscoveredDevice>, DiscoveryError> {
        let timeout = ctx.timeout_bounds.fast_probe_timeout;
        let dcp_devices = self
            .client
            .identify_sweep(&*self.transport, None, timeout)
            .await
            .map_err(|e| match e {
                DcpError::Timeout(d) => DiscoveryError::Timeout(d),
                DcpError::RawSocket(re) => {
                    DiscoveryError::InterfaceError(ctx.target_interface.clone(), re.to_string())
                }
                other => DiscoveryError::ProtocolError(other.to_string()),
            })?;

        Ok(dcp_devices
            .into_iter()
            .map(|d| d.into_universal())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::raw_socket::LoopbackRawTransport;

    #[test]
    fn test_dcp_pdu_serialization_and_parsing() {
        let pdu = DcpPdu::new(
            DcpHeader {
                frame_id: FRAME_ID_DCP_IDENTIFY_REQ,
                service_id: SERVICE_ID_IDENTIFY,
                service_type: SERVICE_TYPE_REQUEST,
                xid: 0x12345678,
                response_delay: 1,
                data_length: 0,
            },
            vec![DcpBlock::all_selector()],
        );

        let bytes = pdu.to_bytes();
        let parsed = DcpPdu::parse(&bytes).expect("parse failed");

        assert_eq!(parsed.header.frame_id, FRAME_ID_DCP_IDENTIFY_REQ);
        assert_eq!(parsed.header.service_id, SERVICE_ID_IDENTIFY);
        assert_eq!(parsed.header.xid, 0x12345678);
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].payload, DcpBlockPayload::AllSelector);
    }

    #[test]
    fn test_tlv_odd_length_padding() {
        // NameOfStation with odd length: "abc" (3 bytes) + 2 qualifier = 5 bytes -> odd -> 1 pad byte
        let block = DcpBlock::name_of_station("abc");
        let bytes = block.to_bytes();

        // 4 header + 5 payload + 1 pad = 10 bytes
        assert_eq!(bytes.len(), 10);
        let (parsed_block, consumed) = DcpBlock::parse(&bytes).expect("parse block");
        assert_eq!(consumed, 10);
        match parsed_block.payload {
            DcpBlockPayload::NameOfStation { name, .. } => assert_eq!(name, "abc"),
            _ => panic!("unexpected payload"),
        }
    }

    #[test]
    fn test_dcp_identify_response_parsing() {
        let src_mac = MacAddress::new([0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);
        let resp_blocks = vec![
            DcpBlock::name_of_station("mythos-controller-01"),
            DcpBlock::device_id(VENDOR_ID_FIRMANSYAH, DEVICE_ID_MYTHOS_EDGE),
            DcpBlock::device_role(DEVICE_ROLE_IO_CONTROLLER),
            DcpBlock::ip_parameter(
                Ipv4Addr::new(192, 168, 1, 50),
                Ipv4Addr::new(255, 255, 255, 0),
                Ipv4Addr::new(192, 168, 1, 1),
                true,
            ),
        ];

        let pdu = DcpPdu::new(
            DcpHeader {
                frame_id: FRAME_ID_DCP_IDENTIFY_RESP,
                service_id: SERVICE_ID_IDENTIFY,
                service_type: SERVICE_TYPE_RESPONSE_SUCCESS,
                xid: 0x42,
                response_delay: 0,
                data_length: 0,
            },
            resp_blocks,
        );

        let frame = EthernetFrame::new(
            MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
            src_mac,
            ETHERTYPE_PROFINET,
            pdu.to_bytes(),
        );

        let device = DcpDiscoveredDevice::from_identify_frame(&frame).expect("parse device");
        assert_eq!(device.mac, src_mac);
        assert_eq!(device.station_name, "mythos-controller-01");
        assert_eq!(device.ip, Ipv4Addr::new(192, 168, 1, 50));
        assert_eq!(device.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(device.gateway, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(device.vendor_id, VENDOR_ID_FIRMANSYAH);
        assert_eq!(device.device_id, DEVICE_ID_MYTHOS_EDGE);
        assert!(device.is_configured);

        let universal = device.into_universal();
        assert_eq!(universal.device_id, "mythos-controller-01");
        assert_eq!(universal.protocol, "profinet-dcp");
        assert_eq!(universal.mac_address, Some("00:1A:2B:3C:4D:5E".to_string()));
    }

    #[tokio::test]
    async fn test_dual_role_dcp_identify_exchange() {
        let (client_transport, server_transport) = LoopbackRawTransport::create_pair();

        let server_config = DcpServerConfig {
            mac: MacAddress::new([0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02]),
            station_name: "mythos-unconfigured-rack".to_string(),
            vendor_id: VENDOR_ID_FIRMANSYAH,
            device_id: DEVICE_ID_MYTHOS_EDGE,
            device_role: DEVICE_ROLE_IO_DEVICE,
            ip: Ipv4Addr::UNSPECIFIED, // 0.0.0.0 (unconfigured field device)
            subnet: Ipv4Addr::new(255, 255, 255, 0),
            gateway: Ipv4Addr::UNSPECIFIED,
        };

        let server = Arc::new(DcpServer::new(server_config));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let srv_clone = server.clone();
        let srv_handle = tokio::spawn(async move {
            srv_clone
                .run_responder(Arc::new(server_transport), shutdown_rx)
                .await;
        });

        let client = DcpClient::new(MacAddress::new([0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]));

        // Sweep for devices (timeout bounded to 500ms for test)
        let devices = client
            .identify_sweep(&client_transport, None, Duration::from_millis(500))
            .await
            .expect("identify sweep failed");

        assert_eq!(devices.len(), 1);
        let dev = &devices[0];
        assert_eq!(
            dev.mac,
            MacAddress::new([0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02])
        );
        assert_eq!(dev.station_name, "mythos-unconfigured-rack");
        assert_eq!(dev.ip, Ipv4Addr::UNSPECIFIED);
        assert!(!dev.is_configured);

        // Terminate server
        let _ = shutdown_tx.send(true);
        let _ = srv_handle.await;
    }

    #[tokio::test]
    async fn test_dual_role_dcp_set_ip_and_signal_led() {
        let (client_transport, server_transport) = LoopbackRawTransport::create_pair();

        let server_config = DcpServerConfig {
            mac: MacAddress::new([0x10, 0x20, 0x30, 0x40, 0x50, 0x60]),
            station_name: "mythos-node-10".to_string(),
            vendor_id: VENDOR_ID_FIRMANSYAH,
            device_id: DEVICE_ID_MYTHOS_EDGE,
            device_role: DEVICE_ROLE_IO_DEVICE,
            ip: Ipv4Addr::UNSPECIFIED,
            subnet: Ipv4Addr::new(255, 255, 255, 0),
            gateway: Ipv4Addr::UNSPECIFIED,
        };

        let server = Arc::new(DcpServer::new(server_config));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let srv_clone = server.clone();
        let srv_handle = tokio::spawn(async move {
            srv_clone
                .run_responder(Arc::new(server_transport), shutdown_rx)
                .await;
        });

        let client = DcpClient::new(MacAddress::new([0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]));
        let target_mac = MacAddress::new([0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);

        // 1. Issue Set-IP over Layer 2
        let new_ip = Ipv4Addr::new(10, 0, 1, 99);
        let new_subnet = Ipv4Addr::new(255, 255, 0, 0);
        let new_gateway = Ipv4Addr::new(10, 0, 1, 1);

        client
            .set_ip(
                &client_transport,
                target_mac,
                new_ip,
                new_subnet,
                new_gateway,
                true,
                Duration::from_millis(500),
            )
            .await
            .expect("set-ip failed");

        // Verify server's state was updated
        let updated_cfg = server.config().await;
        assert_eq!(updated_cfg.ip, new_ip);
        assert_eq!(updated_cfg.subnet, new_subnet);
        assert_eq!(updated_cfg.gateway, new_gateway);

        // 2. Issue Signal-LED command
        client
            .signal_led(&client_transport, target_mac, Duration::from_millis(500))
            .await
            .expect("signal-led failed");

        assert_eq!(server.blink_count().await, 1);

        let _ = shutdown_tx.send(true);
        let _ = srv_handle.await;
    }
}
