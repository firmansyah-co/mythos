//! # Network Discovery and Physical Device Identification Module
//!
//! Implements multi-tier industrial device auto-discovery:
//! - **PROFINET DCP** (Layer 2, IEEE 802.3 EtherType `0x8892`): Subnet-agnostic raw Ethernet discovery,
//!   identification of unconfigured field devices (`0.0.0.0`), Layer 2 IP assignment, and LED flashing.
//! - **Modbus RTU/TCP Sweep**: 3-tier heuristic bus sweep and open hardware catalog fingerprinting.
//! - **mDNS / DNS-SD** (Layer 3/4 Multicast UDP `5353`): Routable service discovery fallback for `_mythos._tcp.local`
//!   and `_opcua-tcp._tcp.local`.
//! - **Universal Discovery Coordinator**: `DiscoveryEngine` orchestrating protocol discoverers.

pub mod dcp;
pub mod engine;
pub mod mdns;
pub mod modbus_sweep;
pub mod raw_socket;
pub mod traits;

pub use dcp::{
    DcpBlock, DcpBlockPayload, DcpClient, DcpDiscoveredDevice, DcpDiscoverer, DcpError, DcpHeader,
    DcpPdu, DcpServer, DcpServerConfig, DEVICE_ROLE_IO_CONTROLLER, DEVICE_ROLE_IO_DEVICE,
    DEVICE_ROLE_PN_SUPERVISOR, FRAME_ID_DCP_GET_SET, FRAME_ID_DCP_IDENTIFY_REQ,
    FRAME_ID_DCP_IDENTIFY_RESP, SERVICE_ID_IDENTIFY, SERVICE_ID_SET, VENDOR_ID_FIRMANSYAH,
};

pub use engine::DiscoveryEngine;

pub use mdns::{
    DnsHeader, DnsPacket, DnsQuestion, DnsRecord, MdnsDiscoverer, MdnsDiscoveryClient, MdnsError,
    MdnsResponder, MdnsService, MdnsTransport, RData, SimulatedMdnsTransport, UdpMdnsTransport,
    SERVICE_TYPE_MYTHOS, SERVICE_TYPE_OPCUA,
};

pub use modbus_sweep::{MockModbusTransport, ModbusProbeTransport, ModbusSweepDiscoverer};

pub use raw_socket::{
    EthernetFrame, LoopbackRawBus, LoopbackRawTransport, MacAddress, RawSocketError,
    RawSocketTransport, ETHERNET_HEADER_LEN, ETHERTYPE_PROFINET, MIN_ETHERNET_FRAME_LEN,
};

#[cfg(target_os = "linux")]
pub use raw_socket::LinuxRawSocket;

pub use traits::{
    DiscoveredDevice, DiscoveryContext, DiscoveryError, DiscoveryMode, DiscoveryTimeoutBounds,
    ProtocolDiscoverer,
};
