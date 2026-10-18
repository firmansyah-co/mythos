//! # Raw Socket & IEEE 802.3 Ethernet Frame Engine
//!
//! Provides zero-copy parsing and serialization for Layer 2 Ethernet frames,
//! MAC address formatting and manipulation, and an asynchronous transport abstraction
//! supporting Linux raw sockets (`AF_PACKET`, `SOCK_RAW`) and in-memory simulated loopbacks.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// PROFINET EtherType constant (IEEE 802.3 EtherType 0x8892).
pub const ETHERTYPE_PROFINET: u16 = 0x8892;

/// Standard Ethernet header length (6 octets destination + 6 octets source + 2 octets EtherType).
pub const ETHERNET_HEADER_LEN: usize = 14;

/// Minimum Ethernet frame payload size according to IEEE 802.3 (without FCS).
pub const MIN_ETHERNET_PAYLOAD_LEN: usize = 46;

/// Minimum Ethernet total frame size (without FCS).
pub const MIN_ETHERNET_FRAME_LEN: usize = ETHERNET_HEADER_LEN + MIN_ETHERNET_PAYLOAD_LEN; // 60 bytes

/// 6-byte IEEE 802.3 MAC address.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Layer 2 broadcast MAC address (`FF:FF:FF:FF:FF:FF`).
    pub const BROADCAST: Self = Self([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    /// Unspecified zero MAC address (`00:00:00:00:00:00`).
    pub const ZERO: Self = Self([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    /// PROFINET Multicast DCP MAC address (`01:0E:CF:00:00:00`).
    pub const PROFINET_MULTICAST: Self = Self([0x01, 0x0E, 0xCF, 0x00, 0x00, 0x00]);

    /// Create a new MAC address from a 6-byte array.
    #[inline]
    pub const fn new(octets: [u8; 6]) -> Self {
        Self(octets)
    }

    /// Return reference to raw octet slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Check if this MAC address is the broadcast address.
    #[inline]
    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }

    /// Check if this MAC address is a multicast address (least significant bit of first octet is 1).
    #[inline]
    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0x01) == 0x01
    }

    /// Check if this MAC address is zero.
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0 == [0x00; 6]
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl From<[u8; 6]> for MacAddress {
    #[inline]
    fn from(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }
}

impl From<MacAddress> for [u8; 6] {
    #[inline]
    fn from(mac: MacAddress) -> Self {
        mac.0
    }
}

impl FromStr for MacAddress {
    type Err = RawSocketError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let clean = s.trim();
        let parts: Vec<&str> = if clean.contains(':') {
            clean.split(':').collect()
        } else if clean.contains('-') {
            clean.split('-').collect()
        } else {
            return Err(RawSocketError::InvalidMacAddress(format!(
                "invalid delimiter in MAC '{}'",
                s
            )));
        };

        if parts.len() != 6 {
            return Err(RawSocketError::InvalidMacAddress(format!(
                "MAC must have 6 octets, got {}: '{}'",
                parts.len(),
                s
            )));
        }

        let mut octets = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            octets[i] = u8::from_str_radix(part, 16).map_err(|e| {
                RawSocketError::InvalidMacAddress(format!(
                    "failed parsing hex octet '{}' in MAC '{}': {}",
                    part, s, e
                ))
            })?;
        }

        Ok(Self(octets))
    }
}

/// Errors related to Layer 2 raw sockets and frame processing.
#[derive(Debug, thiserror::Error)]
pub enum RawSocketError {
    #[error("Ethernet frame too short: minimum {expected} bytes, received {actual} bytes")]
    FrameTooShort { expected: usize, actual: usize },

    #[error("Invalid MAC address: {0}")]
    InvalidMacAddress(String),

    #[error("Unsupported platform for native raw socket: {0}")]
    UnsupportedPlatform(String),

    #[error("Permission denied creating raw socket (requires CAP_NET_RAW / root): {0}")]
    PermissionDenied(String),

    #[error("Failed to bind raw socket to interface '{0}': {1}")]
    InterfaceBindFailed(String, String),

    #[error("Socket I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Transport channel closed: {0}")]
    TransportClosed(String),
}

/// Parsed or constructed IEEE 802.3 Ethernet frame.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EthernetFrame {
    /// Destination MAC address (6 octets).
    pub destination: MacAddress,
    /// Source MAC address (6 octets).
    pub source: MacAddress,
    /// EtherType field (e.g. 0x8892 for PROFINET).
    pub ethertype: u16,
    /// Frame payload.
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    /// Create a new Ethernet frame with given destination, source, EtherType, and payload.
    pub fn new(
        destination: MacAddress,
        source: MacAddress,
        ethertype: u16,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            destination,
            source,
            ethertype,
            payload,
        }
    }

    /// Parse an IEEE 802.3 Ethernet frame from raw bytes.
    ///
    /// Validates minimum header length (14 bytes) and returns an `EthernetFrame`.
    pub fn parse(buffer: &[u8]) -> Result<Self, RawSocketError> {
        if buffer.len() < ETHERNET_HEADER_LEN {
            return Err(RawSocketError::FrameTooShort {
                expected: ETHERNET_HEADER_LEN,
                actual: buffer.len(),
            });
        }

        let mut dest_bytes = [0u8; 6];
        dest_bytes.copy_from_slice(&buffer[0..6]);

        let mut src_bytes = [0u8; 6];
        src_bytes.copy_from_slice(&buffer[6..12]);

        let ethertype = u16::from_be_bytes([buffer[12], buffer[13]]);
        let payload = buffer[ETHERNET_HEADER_LEN..].to_vec();

        Ok(Self {
            destination: MacAddress(dest_bytes),
            source: MacAddress(src_bytes),
            ethertype,
            payload,
        })
    }

    /// Serialize Ethernet frame into raw octets, optionally padding payload to 46 bytes (60 bytes frame).
    pub fn to_bytes(&self, pad_minimum: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            ETHERNET_HEADER_LEN + self.payload.len().max(MIN_ETHERNET_PAYLOAD_LEN),
        );
        out.extend_from_slice(self.destination.as_bytes());
        out.extend_from_slice(self.source.as_bytes());
        out.extend_from_slice(&self.ethertype.to_be_bytes());
        out.extend_from_slice(&self.payload);

        if pad_minimum && out.len() < MIN_ETHERNET_FRAME_LEN {
            out.resize(MIN_ETHERNET_FRAME_LEN, 0x00);
        }

        out
    }
}

impl fmt::Debug for EthernetFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EthernetFrame")
            .field("destination", &self.destination)
            .field("source", &self.source)
            .field("ethertype", &format_args!("0x{:04X}", self.ethertype))
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// Asynchronous raw socket transport abstraction for Layer 2 communications.
#[async_trait::async_trait]
pub trait RawSocketTransport: Send + Sync {
    /// Transmit raw frame bytes over the Layer 2 link.
    async fn send_frame(&self, frame_bytes: &[u8]) -> Result<(), RawSocketError>;

    /// Receive raw frame bytes from the Layer 2 link.
    async fn recv_frame(&self) -> Result<Vec<u8>, RawSocketError>;
}

/// In-memory broadcast bus for unit tests and unprivileged simulated environments.

#[async_trait::async_trait]
impl<T: RawSocketTransport + ?Sized> RawSocketTransport for Arc<T> {
    async fn send_frame(&self, frame_bytes: &[u8]) -> Result<(), RawSocketError> {
        (**self).send_frame(frame_bytes).await
    }

    async fn recv_frame(&self) -> Result<Vec<u8>, RawSocketError> {
        (**self).recv_frame().await
    }
}

pub struct LoopbackRawBus {
    tx: broadcast::Sender<Vec<u8>>,
}

impl LoopbackRawBus {
    /// Create a new in-memory loopback bus with the specified channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Connect a participant to this loopback bus.
    pub fn connect(&self) -> LoopbackRawTransport {
        LoopbackRawTransport {
            tx: self.tx.clone(),
            rx: Arc::new(Mutex::new(self.tx.subscribe())),
        }
    }
}

/// In-memory Layer 2 transport node connected to a `LoopbackRawBus`.
#[derive(Clone)]
pub struct LoopbackRawTransport {
    tx: broadcast::Sender<Vec<u8>>,
    rx: Arc<Mutex<broadcast::Receiver<Vec<u8>>>>,
}

impl LoopbackRawTransport {
    /// Create a pair of mutually connected loopback transports (A <-> B).
    pub fn create_pair() -> (Self, Self) {
        let bus = LoopbackRawBus::new(256);
        let node1 = bus.connect();
        let node2 = bus.connect();
        (node1, node2)
    }
}

#[async_trait::async_trait]
impl RawSocketTransport for LoopbackRawTransport {
    async fn send_frame(&self, frame_bytes: &[u8]) -> Result<(), RawSocketError> {
        let _ = self.tx.send(frame_bytes.to_vec());
        Ok(())
    }

    async fn recv_frame(&self) -> Result<Vec<u8>, RawSocketError> {
        let mut rx = self.rx.lock().await;
        loop {
            match rx.recv().await {
                Ok(data) => return Ok(data),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(RawSocketError::TransportClosed(
                        "Loopback bus closed".into(),
                    ))
                }
            }
        }
    }
}

/// Native Linux Raw Socket transport using AF_PACKET and SOCK_RAW.
#[cfg(target_os = "linux")]
pub struct LinuxRawSocket {
    fd: std::os::unix::io::RawFd,
    interface_name: String,
    async_fd: tokio::io::unix::AsyncFd<std::os::unix::io::RawFd>,
}

#[cfg(target_os = "linux")]
impl LinuxRawSocket {
    /// Open a Linux raw socket bound to the specified network interface (e.g. "eth0")
    /// filtering for the specified EtherType (or `0x0003` for ETH_P_ALL).
    pub fn open(interface_name: &str, ethertype: u16) -> Result<Self, RawSocketError> {
        // Protocol in network byte order
        let proto = ethertype.to_be() as i32;
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                proto,
            )
        };

        if fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EPERM) || err.raw_os_error() == Some(libc::EACCES) {
                return Err(RawSocketError::PermissionDenied(format!(
                    "CAP_NET_RAW capability required to open raw socket on {}: {}",
                    interface_name, err
                )));
            }
            return Err(RawSocketError::Io(err));
        }

        // Resolve interface index
        let c_ifname = std::ffi::CString::new(interface_name).map_err(|e| {
            RawSocketError::InterfaceBindFailed(interface_name.to_string(), e.to_string())
        })?;

        let ifindex = unsafe { libc::if_nametoindex(c_ifname.as_ptr()) };
        if ifindex == 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(RawSocketError::InterfaceBindFailed(
                interface_name.to_string(),
                format!("interface not found: {}", err),
            ));
        }

        let mut sa: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        sa.sll_family = libc::AF_PACKET as u16;
        sa.sll_protocol = ethertype.to_be();
        sa.sll_ifindex = ifindex as i32;

        let bind_res = unsafe {
            libc::bind(
                fd,
                &sa as *const libc::sockaddr_ll as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };

        if bind_res < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(RawSocketError::InterfaceBindFailed(
                interface_name.to_string(),
                format!("failed binding to ifindex {}: {}", ifindex, err),
            ));
        }

        #[allow(deprecated)]
        let async_fd = tokio::io::unix::AsyncFd::new(fd).map_err(|e| {
            unsafe { libc::close(fd) };
            RawSocketError::Io(e)
        })?;

        Ok(Self {
            fd,
            interface_name: interface_name.to_string(),
            async_fd,
        })
    }

    /// Return the bound interface name.
    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl RawSocketTransport for LinuxRawSocket {
    async fn send_frame(&self, frame_bytes: &[u8]) -> Result<(), RawSocketError> {
        loop {
            let mut guard = self.async_fd.writable().await?;
            match guard.try_io(|inner| {
                let res = unsafe {
                    libc::send(
                        *inner.get_ref(),
                        frame_bytes.as_ptr() as *const libc::c_void,
                        frame_bytes.len(),
                        0,
                    )
                };
                if res < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(res as usize)
                }
            }) {
                Ok(Ok(_sent)) => return Ok(()),
                Ok(Err(e)) => return Err(RawSocketError::Io(e)),
                Err(_would_block) => continue,
            }
        }
    }

    async fn recv_frame(&self) -> Result<Vec<u8>, RawSocketError> {
        let mut buf = vec![0u8; 2048];
        loop {
            let mut guard = self.async_fd.readable().await?;
            match guard.try_io(|inner| {
                let res = unsafe {
                    libc::recv(
                        *inner.get_ref(),
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                        0,
                    )
                };
                if res < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(res as usize)
                }
            }) {
                Ok(Ok(n)) => {
                    buf.truncate(n);
                    return Ok(buf);
                }
                Ok(Err(e)) => return Err(RawSocketError::Io(e)),
                Err(_would_block) => continue,
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxRawSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_formatting_and_parsing() {
        let mac_str = "00:0e:cf:12:34:56";
        let mac: MacAddress = mac_str.parse().expect("failed parsing MAC");
        assert_eq!(mac.0, [0x00, 0x0E, 0xCF, 0x12, 0x34, 0x56]);
        assert_eq!(mac.to_string(), "00:0E:CF:12:34:56");
        assert!(!mac.is_broadcast());
        assert!(!mac.is_zero());
        assert!(!mac.is_multicast());

        let bcast = MacAddress::BROADCAST;
        assert!(bcast.is_broadcast());
        assert!(bcast.is_multicast());
        assert_eq!(bcast.to_string(), "FF:FF:FF:FF:FF:FF");

        let zero = MacAddress::ZERO;
        assert!(zero.is_zero());
        assert!(!zero.is_multicast());

        let pn_mcast = MacAddress::PROFINET_MULTICAST;
        assert!(pn_mcast.is_multicast());

        // Dash delimiter
        let mac_dash: MacAddress = "AA-BB-CC-DD-EE-FF".parse().expect("dash parse");
        assert_eq!(mac_dash.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        // Malformed MAC
        assert!("invalid_mac".parse::<MacAddress>().is_err());
        assert!("00:11:22".parse::<MacAddress>().is_err());
        assert!("00:11:22:33:44:ZZ".parse::<MacAddress>().is_err());
    }

    #[test]
    fn test_ethernet_frame_serialization_roundtrip() {
        let src = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        let dst = MacAddress::BROADCAST;
        let payload = vec![0xFE, 0xFD, 0x05, 0x00, 0x01, 0x02, 0x03, 0x04];

        let frame = EthernetFrame::new(dst, src, ETHERTYPE_PROFINET, payload.clone());
        let raw = frame.to_bytes(false);

        assert_eq!(raw.len(), ETHERNET_HEADER_LEN + payload.len());
        assert_eq!(&raw[0..6], dst.as_bytes());
        assert_eq!(&raw[6..12], src.as_bytes());
        assert_eq!(u16::from_be_bytes([raw[12], raw[13]]), ETHERTYPE_PROFINET);
        assert_eq!(&raw[14..], &payload[..]);

        let parsed = EthernetFrame::parse(&raw).expect("failed parsing frame");
        assert_eq!(parsed.destination, dst);
        assert_eq!(parsed.source, src);
        assert_eq!(parsed.ethertype, ETHERTYPE_PROFINET);
        assert_eq!(parsed.payload, payload);

        // Frame with padding
        let padded = frame.to_bytes(true);
        assert_eq!(padded.len(), MIN_ETHERNET_FRAME_LEN);
    }

    #[test]
    fn test_ethernet_frame_truncated() {
        let short = vec![0x00, 0x01, 0x02];
        let err = EthernetFrame::parse(&short).unwrap_err();
        match err {
            RawSocketError::FrameTooShort { expected, actual } => {
                assert_eq!(expected, ETHERNET_HEADER_LEN);
                assert_eq!(actual, 3);
            }
            _ => panic!("unexpected error: {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_loopback_transport_exchange() {
        let (client, server) = LoopbackRawTransport::create_pair();

        let frame_payload = vec![0xAA, 0xBB, 0xCC];
        let frame = EthernetFrame::new(
            MacAddress::BROADCAST,
            MacAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]),
            ETHERTYPE_PROFINET,
            frame_payload.clone(),
        );

        let serialized = frame.to_bytes(false);
        client
            .send_frame(&serialized)
            .await
            .expect("send frame failed");

        let received = server.recv_frame().await.expect("recv frame failed");
        assert_eq!(received, serialized);

        let parsed = EthernetFrame::parse(&received).expect("parse received frame");
        assert_eq!(parsed.payload, frame_payload);
    }
}
