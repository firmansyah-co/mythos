//! # mDNS / Zeroconf Routable Fallback Discovery Engine
//!
//! Provides RFC 6762 (Multicast DNS) and RFC 6763 (DNS-SD) service discovery over routed IP networks.
//! Acts as the secondary discovery tier (SYS-REQ-043) when industrial devices or controller nodes
//! communicate across routed subnets or virtualized environments where Layer 2 PROFINET DCP broadcasts
//! cannot penetrate.
//!
//! Discovers standard services such as `_mythos._tcp.local` and `_opcua-tcp._tcp.local`.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

/// Standard IPv4 Multicast DNS address (`224.0.0.251`).
pub const MDNS_IPV4_MULTICAST: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

/// Standard Multicast DNS UDP port (`5353`).
pub const MDNS_PORT: u16 = 5353;

/// Primary MythOS daemon service record type.
pub const SERVICE_TYPE_MYTHOS: &str = "_mythos._tcp.local";

/// Industrial OPC UA TCP server service record type.
pub const SERVICE_TYPE_OPCUA: &str = "_opcua-tcp._tcp.local";

/// DNS Resource Record Types (RFC 1035 / RFC 2782).
pub const TYPE_A: u16 = 1;
pub const TYPE_PTR: u16 = 12;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_SRV: u16 = 33;
pub const TYPE_ANY: u16 = 255;

/// DNS Class IN (Internet).
pub const CLASS_IN: u16 = 1;

/// Maximum label length in standard DNS.
pub const MAX_LABEL_LEN: usize = 63;
/// Maximum domain name length.
pub const MAX_NAME_LEN: usize = 255;
/// Maximum recursion depth when resolving DNS compression pointers (prevents infinite loops).
pub const MAX_POINTER_RECURSION: usize = 8;

/// Errors arising from mDNS packet encoding, decoding, and network communication.
#[derive(Debug, thiserror::Error)]
pub enum MdnsError {
    #[error("Packet buffer too short: expected at least {expected} bytes, got {actual}")]
    BufferTooShort { expected: usize, actual: usize },

    #[error("Invalid DNS name encoding: {0}")]
    InvalidName(String),

    #[error("Compression pointer loops or exceeded maximum recursion depth")]
    PointerLoop,

    #[error("Malformed resource record: {0}")]
    MalformedRecord(String),

    #[error("I/O error during mDNS operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Discovery timed out after {0:?}")]
    Timeout(Duration),
}

/// Discovered network service via mDNS / DNS-SD.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MdnsService {
    /// Service instance name (e.g. "MythOS Primary Edge Controller").
    pub instance_name: String,
    /// Service registration type (e.g. `_mythos._tcp.local`).
    pub service_type: String,
    /// Target host domain name (e.g. `edge-01.local`).
    pub host: String,
    /// Target IPv4 address if resolved from an A record.
    pub ip: Option<Ipv4Addr>,
    /// Port number hosting the service.
    pub port: u16,
    /// Key-value properties extracted from TXT record.
    pub txt_records: HashMap<String, String>,
}

/// Parsed or constructed DNS Header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl DnsHeader {
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..2].copy_from_slice(&self.id.to_be_bytes());
        b[2..4].copy_from_slice(&self.flags.to_be_bytes());
        b[4..6].copy_from_slice(&self.qdcount.to_be_bytes());
        b[6..8].copy_from_slice(&self.ancount.to_be_bytes());
        b[8..10].copy_from_slice(&self.nscount.to_be_bytes());
        b[10..12].copy_from_slice(&self.arcount.to_be_bytes());
        b
    }

    pub fn parse(buf: &[u8]) -> Result<Self, MdnsError> {
        if buf.len() < 12 {
            return Err(MdnsError::BufferTooShort {
                expected: 12,
                actual: buf.len(),
            });
        }
        Ok(Self {
            id: u16::from_be_bytes([buf[0], buf[1]]),
            flags: u16::from_be_bytes([buf[2], buf[3]]),
            qdcount: u16::from_be_bytes([buf[4], buf[5]]),
            ancount: u16::from_be_bytes([buf[6], buf[7]]),
            nscount: u16::from_be_bytes([buf[8], buf[9]]),
            arcount: u16::from_be_bytes([buf[10], buf[11]]),
        })
    }
}

/// DNS Question structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

impl DnsQuestion {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = encode_domain_name(&self.name);
        out.extend_from_slice(&self.qtype.to_be_bytes());
        out.extend_from_slice(&self.qclass.to_be_bytes());
        out
    }
}

/// Parsed DNS Resource Record Data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RData {
    A(Ipv4Addr),
    Ptr(String),
    Srv {
        priority: u16,
        weight: u16,
        port: u16,
        target: String,
    },
    Txt(HashMap<String, String>),
    Raw(Vec<u8>),
}

/// DNS Resource Record (Answer / Additional).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    pub name: String,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,
    pub rdata: RData,
}

impl DnsRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = encode_domain_name(&self.name);
        out.extend_from_slice(&self.rtype.to_be_bytes());
        out.extend_from_slice(&self.rclass.to_be_bytes());
        out.extend_from_slice(&self.ttl.to_be_bytes());

        let rdata_bytes = match &self.rdata {
            RData::A(ip) => ip.octets().to_vec(),
            RData::Ptr(target) => encode_domain_name(target),
            RData::Srv {
                priority,
                weight,
                port,
                target,
            } => {
                let mut b = Vec::new();
                b.extend_from_slice(&priority.to_be_bytes());
                b.extend_from_slice(&weight.to_be_bytes());
                b.extend_from_slice(&port.to_be_bytes());
                b.extend_from_slice(&encode_domain_name(target));
                b
            }
            RData::Txt(map) => {
                let mut b = Vec::new();
                for (k, v) in map {
                    let entry = format!("{}={}", k, v);
                    let len = entry.len().min(255) as u8;
                    b.push(len);
                    b.extend_from_slice(&entry.as_bytes()[..len as usize]);
                }
                if b.is_empty() {
                    b.push(0x00);
                }
                b
            }
            RData::Raw(bytes) => bytes.clone(),
        };

        let rdlength = rdata_bytes.len() as u16;
        out.extend_from_slice(&rdlength.to_be_bytes());
        out.extend_from_slice(&rdata_bytes);
        out
    }
}

/// Complete DNS / mDNS Packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsPacket {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsRecord>,
    pub additionals: Vec<DnsRecord>,
}

impl DnsPacket {
    pub fn new_query(service_type: &str) -> Self {
        Self {
            header: DnsHeader {
                id: 0,
                flags: 0x0000, // Standard query
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: vec![DnsQuestion {
                name: service_type.to_string(),
                qtype: TYPE_PTR,
                qclass: CLASS_IN,
            }],
            answers: Vec::new(),
            additionals: Vec::new(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = self.header;
        header.qdcount = self.questions.len() as u16;
        header.ancount = self.answers.len() as u16;
        header.arcount = self.additionals.len() as u16;

        let mut out = Vec::new();
        out.extend_from_slice(&header.to_bytes());

        for q in &self.questions {
            out.extend_from_slice(&q.to_bytes());
        }

        for a in &self.answers {
            out.extend_from_slice(&a.to_bytes());
        }

        for add in &self.additionals {
            out.extend_from_slice(&add.to_bytes());
        }

        out
    }

    pub fn parse(buf: &[u8]) -> Result<Self, MdnsError> {
        let header = DnsHeader::parse(buf)?;
        let mut offset = 12;

        let mut questions = Vec::with_capacity(header.qdcount as usize);
        for _ in 0..header.qdcount {
            let (name, consumed) = decode_domain_name(buf, offset, 0)?;
            offset += consumed;

            if offset + 4 > buf.len() {
                return Err(MdnsError::BufferTooShort {
                    expected: offset + 4,
                    actual: buf.len(),
                });
            }

            let qtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
            let qclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
            offset += 4;

            questions.push(DnsQuestion {
                name,
                qtype,
                qclass: qclass & 0x7FFF, // Strip unicast-response bit
            });
        }

        let total_records =
            header.ancount as usize + header.nscount as usize + header.arcount as usize;
        let mut all_records = Vec::with_capacity(total_records);

        for _ in 0..total_records {
            if offset >= buf.len() {
                break;
            }

            let (name, consumed) = decode_domain_name(buf, offset, 0)?;
            offset += consumed;

            if offset + 10 > buf.len() {
                return Err(MdnsError::BufferTooShort {
                    expected: offset + 10,
                    actual: buf.len(),
                });
            }

            let rtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
            let rclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) & 0x7FFF;
            let ttl = u32::from_be_bytes([
                buf[offset + 4],
                buf[offset + 5],
                buf[offset + 6],
                buf[offset + 7],
            ]);
            let rdlength = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
            offset += 10;

            if offset + rdlength > buf.len() {
                return Err(MdnsError::BufferTooShort {
                    expected: offset + rdlength,
                    actual: buf.len(),
                });
            }

            let rdata_slice = &buf[offset..offset + rdlength];
            let rdata = match rtype {
                TYPE_A => {
                    if rdlength == 4 {
                        RData::A(Ipv4Addr::new(
                            rdata_slice[0],
                            rdata_slice[1],
                            rdata_slice[2],
                            rdata_slice[3],
                        ))
                    } else {
                        RData::Raw(rdata_slice.to_vec())
                    }
                }
                TYPE_PTR => {
                    let (target, _) = decode_domain_name(buf, offset, 0)?;
                    RData::Ptr(target)
                }
                TYPE_SRV => {
                    if rdlength >= 6 {
                        let priority = u16::from_be_bytes([rdata_slice[0], rdata_slice[1]]);
                        let weight = u16::from_be_bytes([rdata_slice[2], rdata_slice[3]]);
                        let port = u16::from_be_bytes([rdata_slice[4], rdata_slice[5]]);
                        let (target, _) = decode_domain_name(buf, offset + 6, 0)?;
                        RData::Srv {
                            priority,
                            weight,
                            port,
                            target,
                        }
                    } else {
                        RData::Raw(rdata_slice.to_vec())
                    }
                }
                TYPE_TXT => {
                    let mut map = HashMap::new();
                    let mut txt_cursor = 0;
                    while txt_cursor < rdlength {
                        let len = rdata_slice[txt_cursor] as usize;
                        txt_cursor += 1;
                        if txt_cursor + len <= rdlength {
                            if let Ok(entry) =
                                std::str::from_utf8(&rdata_slice[txt_cursor..txt_cursor + len])
                            {
                                if let Some((k, v)) = entry.split_once('=') {
                                    map.insert(k.to_string(), v.to_string());
                                } else {
                                    map.insert(entry.to_string(), "".to_string());
                                }
                            }
                        }
                        txt_cursor += len;
                    }
                    RData::Txt(map)
                }
                _ => RData::Raw(rdata_slice.to_vec()),
            };

            offset += rdlength;

            all_records.push(DnsRecord {
                name,
                rtype,
                rclass,
                ttl,
                rdata,
            });
        }

        let answers_count = (header.ancount as usize).min(all_records.len());
        let answers = all_records[..answers_count].to_vec();
        let additionals = all_records[answers_count..].to_vec();

        Ok(Self {
            header,
            questions,
            answers,
            additionals,
        })
    }
}

/// Encode a dot-separated domain name into DNS wire format (`mythoslocal `).
pub fn encode_domain_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let trimmed = name.trim().trim_end_matches('.');
    if !trimmed.is_empty() {
        for label in trimmed.split('.') {
            let bytes = label.as_bytes();
            let len = bytes.len().min(MAX_LABEL_LEN) as u8;
            out.push(len);
            out.extend_from_slice(&bytes[..len as usize]);
        }
    }
    out.push(0x00); // Null terminator
    out
}

/// Decode a domain name from a DNS buffer supporting compression pointers (`0xC0XX`).
#[allow(unused_assignments, unused_mut)]
pub fn decode_domain_name(
    buf: &[u8],
    mut offset: usize,
    recursion: usize,
) -> Result<(String, usize), MdnsError> {
    if recursion > MAX_POINTER_RECURSION {
        return Err(MdnsError::PointerLoop);
    }

    let initial_offset = offset;
    let mut labels = Vec::new();
    let mut bytes_consumed = 0;
    let mut reached_end = false;

    while offset < buf.len() {
        let len = buf[offset];

        // Terminator
        if len == 0 {
            if !reached_end {
                bytes_consumed = (offset + 1) - initial_offset;
            }
            break;
        }

        // Pointer (top 2 bits 11)
        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= buf.len() {
                return Err(MdnsError::BufferTooShort {
                    expected: offset + 2,
                    actual: buf.len(),
                });
            }
            let pointer_offset = (((len & 0x3F) as usize) << 8) | (buf[offset + 1] as usize);

            if !reached_end {
                bytes_consumed = (offset + 2) - initial_offset;
                reached_end = true;
            }

            if pointer_offset >= buf.len() {
                return Err(MdnsError::InvalidName(format!(
                    "Pointer offset {} beyond buffer len {}",
                    pointer_offset,
                    buf.len()
                )));
            }

            let (suffix, _) = decode_domain_name(buf, pointer_offset, recursion + 1)?;
            if !suffix.is_empty() {
                labels.push(suffix);
            }
            break;
        }

        // Regular label
        let label_len = len as usize;
        offset += 1;

        if offset + label_len > buf.len() {
            return Err(MdnsError::BufferTooShort {
                expected: offset + label_len,
                actual: buf.len(),
            });
        }

        let label = std::str::from_utf8(&buf[offset..offset + label_len])
            .map_err(|e| MdnsError::InvalidName(e.to_string()))?;
        labels.push(label.to_string());

        offset += label_len;
        if !reached_end {
            bytes_consumed = offset - initial_offset;
        }
    }

    Ok((labels.join("."), bytes_consumed))
}

#[async_trait::async_trait]
pub trait MdnsTransport: Send + Sync {
    async fn send_multicast(&self, packet: &[u8]) -> Result<(), MdnsError>;
    async fn recv_multicast(&self) -> Result<(Vec<u8>, SocketAddr), MdnsError>;
}

/// Live UDP Multicast transport for routable mDNS discovery.

#[async_trait::async_trait]
impl<T: MdnsTransport + ?Sized> MdnsTransport for Arc<T> {
    async fn send_multicast(&self, packet: &[u8]) -> Result<(), MdnsError> {
        (**self).send_multicast(packet).await
    }

    async fn recv_multicast(&self) -> Result<(Vec<u8>, SocketAddr), MdnsError> {
        (**self).recv_multicast().await
    }
}

pub struct UdpMdnsTransport {
    socket: Arc<UdpSocket>,
}

impl UdpMdnsTransport {
    /// Bind to mDNS multicast group. If port 5353 is already held, attempts ephemeral port binding.
    pub async fn bind() -> Result<Self, MdnsError> {
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
        let socket = UdpSocket::bind(addr).await?;
        let _ = socket.join_multicast_v4(MDNS_IPV4_MULTICAST, Ipv4Addr::UNSPECIFIED);

        Ok(Self {
            socket: Arc::new(socket),
        })
    }
}

#[async_trait::async_trait]
impl MdnsTransport for UdpMdnsTransport {
    async fn send_multicast(&self, packet: &[u8]) -> Result<(), MdnsError> {
        let target = SocketAddr::V4(SocketAddrV4::new(MDNS_IPV4_MULTICAST, MDNS_PORT));
        self.socket.send_to(packet, target).await?;
        Ok(())
    }

    async fn recv_multicast(&self) -> Result<(Vec<u8>, SocketAddr), MdnsError> {
        let mut buf = vec![0u8; 4096];
        let (n, peer) = self.socket.recv_from(&mut buf).await?;
        buf.truncate(n);
        Ok((buf, peer))
    }
}

/// Type alias for simulated mDNS broadcast receiver to avoid type complexity warnings.
pub type MdnsBroadcastReceiver =
    Arc<Mutex<tokio::sync::broadcast::Receiver<(Vec<u8>, SocketAddr)>>>;

/// In-memory simulated mDNS transport for deterministic testing and sandbox fallbacks.
#[derive(Clone)]
pub struct SimulatedMdnsTransport {
    tx: tokio::sync::broadcast::Sender<(Vec<u8>, SocketAddr)>,
    rx: MdnsBroadcastReceiver,
    local_addr: SocketAddr,
}

impl SimulatedMdnsTransport {
    pub fn new(
        capacity: usize,
        local_addr: SocketAddr,
    ) -> (Self, tokio::sync::broadcast::Sender<(Vec<u8>, SocketAddr)>) {
        let (tx, _) = tokio::sync::broadcast::channel(capacity);
        let rx = Arc::new(Mutex::new(tx.subscribe()));
        let transport = Self {
            tx: tx.clone(),
            rx,
            local_addr,
        };
        (transport, tx)
    }

    pub fn create_paired_network() -> (Self, Self) {
        let (tx, _) = tokio::sync::broadcast::channel(128);
        let node1 = Self {
            tx: tx.clone(),
            rx: Arc::new(Mutex::new(tx.subscribe())),
            local_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 10), 5353)),
        };
        let node2 = Self {
            tx: tx.clone(),
            rx: Arc::new(Mutex::new(tx.subscribe())),
            local_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 20), 5353)),
        };
        (node1, node2)
    }
}

#[async_trait::async_trait]
impl MdnsTransport for SimulatedMdnsTransport {
    async fn send_multicast(&self, packet: &[u8]) -> Result<(), MdnsError> {
        let _ = self.tx.send((packet.to_vec(), self.local_addr));
        Ok(())
    }

    async fn recv_multicast(&self) -> Result<(Vec<u8>, SocketAddr), MdnsError> {
        let mut rx = self.rx.lock().await;
        loop {
            match rx.recv().await {
                Ok(item) => return Ok(item),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(MdnsError::Io(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "Simulated channel closed",
                    )))
                }
            }
        }
    }
}

/// High-level mDNS Discovery Client and Resolver.
pub struct MdnsDiscoveryClient {
    transport: Arc<dyn MdnsTransport>,
}

impl MdnsDiscoveryClient {
    pub fn new(transport: Arc<dyn MdnsTransport>) -> Self {
        Self { transport }
    }

    /// Query the network for services matching `service_type` (e.g. `_mythos._tcp.local`).
    pub async fn query_services(
        &self,
        service_type: &str,
        timeout: Duration,
    ) -> Result<Vec<MdnsService>, MdnsError> {
        let query_packet = DnsPacket::new_query(service_type);
        let query_bytes = query_packet.to_bytes();

        self.transport.send_multicast(&query_bytes).await?;

        let mut services: HashMap<String, MdnsService> = HashMap::new();
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, self.transport.recv_multicast()).await {
                Ok(Ok((data, peer))) => {
                    if let Ok(packet) = DnsPacket::parse(&data) {
                        // Extract PTR answers pointing to instance names
                        for ans in &packet.answers {
                            if ans.rtype == TYPE_PTR && ans.name.eq_ignore_ascii_case(service_type)
                            {
                                if let RData::Ptr(instance_name) = &ans.rdata {
                                    let entry = services
                                        .entry(instance_name.clone())
                                        .or_insert_with(|| MdnsService {
                                            instance_name: instance_name.clone(),
                                            service_type: service_type.to_string(),
                                            host: "".to_string(),
                                            ip: match peer {
                                                SocketAddr::V4(v4) => Some(*v4.ip()),
                                                _ => None,
                                            },
                                            port: 0,
                                            txt_records: HashMap::new(),
                                        });

                                    // Check additional records for SRV, TXT, A
                                    for add in &packet.additionals {
                                        if add.name.eq_ignore_ascii_case(instance_name) {
                                            match &add.rdata {
                                                RData::Srv { port, target, .. } => {
                                                    entry.port = *port;
                                                    entry.host = target.clone();
                                                }
                                                RData::Txt(map) => {
                                                    entry.txt_records.extend(map.clone());
                                                }
                                                _ => {}
                                            }
                                        } else if !entry.host.is_empty()
                                            && add.name.eq_ignore_ascii_case(&entry.host)
                                        {
                                            if let RData::A(ip) = add.rdata {
                                                entry.ip = Some(ip);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => break, // Timeout elapsed
            }
        }

        Ok(services.into_values().collect())
    }
}

/// Simulated mDNS Responder for handling incoming PTR queries in tests or edge environments.
pub struct MdnsResponder {
    service: MdnsService,
}

impl MdnsResponder {
    pub fn new(service: MdnsService) -> Self {
        Self { service }
    }

    /// Process an incoming mDNS packet and generate a response if a question matches our service.
    pub fn handle_packet(&self, packet: &DnsPacket) -> Option<DnsPacket> {
        let mut matched = false;
        for q in &packet.questions {
            if q.name.eq_ignore_ascii_case(&self.service.service_type) {
                matched = true;
                break;
            }
        }

        if !matched {
            return None;
        }

        let ptr_record = DnsRecord {
            name: self.service.service_type.clone(),
            rtype: TYPE_PTR,
            rclass: CLASS_IN,
            ttl: 120,
            rdata: RData::Ptr(self.service.instance_name.clone()),
        };

        let srv_record = DnsRecord {
            name: self.service.instance_name.clone(),
            rtype: TYPE_SRV,
            rclass: CLASS_IN,
            ttl: 120,
            rdata: RData::Srv {
                priority: 0,
                weight: 0,
                port: self.service.port,
                target: self.service.host.clone(),
            },
        };

        let txt_record = DnsRecord {
            name: self.service.instance_name.clone(),
            rtype: TYPE_TXT,
            rclass: CLASS_IN,
            ttl: 120,
            rdata: RData::Txt(self.service.txt_records.clone()),
        };

        let mut additionals = vec![srv_record, txt_record];

        if let Some(ip) = self.service.ip {
            additionals.push(DnsRecord {
                name: self.service.host.clone(),
                rtype: TYPE_A,
                rclass: CLASS_IN,
                ttl: 120,
                rdata: RData::A(ip),
            });
        }

        Some(DnsPacket {
            header: DnsHeader {
                id: packet.header.id,
                flags: 0x8400, // Authoritative response
                qdcount: 0,
                ancount: 1,
                nscount: 0,
                arcount: additionals.len() as u16,
            },
            questions: Vec::new(),
            answers: vec![ptr_record],
            additionals,
        })
    }
}

use crate::discovery::traits::{
    DiscoveredDevice as UniversalDiscoveredDevice, DiscoveryContext, DiscoveryError,
    ProtocolDiscoverer,
};

/// Universal Protocol Discoverer adapter for mDNS / DNS-SD.
pub struct MdnsDiscoverer {
    client: MdnsDiscoveryClient,
}

impl MdnsDiscoverer {
    pub fn new(transport: Arc<dyn MdnsTransport>) -> Self {
        Self {
            client: MdnsDiscoveryClient::new(transport),
        }
    }
}

#[async_trait::async_trait]
impl ProtocolDiscoverer for MdnsDiscoverer {
    fn protocol_id(&self) -> &str {
        "mdns"
    }

    async fn discover(
        &self,
        ctx: &DiscoveryContext,
    ) -> Result<Vec<UniversalDiscoveredDevice>, DiscoveryError> {
        let timeout = ctx.timeout_bounds.fast_probe_timeout;
        let mut results = Vec::new();

        for service_type in &[SERVICE_TYPE_MYTHOS, SERVICE_TYPE_OPCUA] {
            if ctx.cancel_token.is_cancelled() {
                return Err(DiscoveryError::Cancelled);
            }
            match self.client.query_services(service_type, timeout).await {
                Ok(services) => {
                    for s in services {
                        let mut properties = s.txt_records.clone();
                        properties.insert("port".to_string(), s.port.to_string());
                        properties.insert("host".to_string(), s.host.clone());

                        results.push(UniversalDiscoveredDevice {
                            device_id: s.instance_name.clone(),
                            protocol: "mdns".to_string(),
                            station_name: Some(s.instance_name),
                            mac_address: None,
                            ip_address: s.ip.map(|ip| ip.to_string()),
                            modbus_slave_id: None,
                            fingerprint_model: None,
                            vendor: None,
                            confidence: 0.9,
                            properties,
                        });
                    }
                }
                Err(MdnsError::Timeout(d)) => return Err(DiscoveryError::Timeout(d)),
                Err(e) => {
                    tracing::warn!("mDNS query for {} encountered error: {}", service_type, e);
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_name_encoding_and_decoding() {
        let name = "_mythos._tcp.local";
        let encoded = encode_domain_name(name);

        assert_eq!(encoded[0], 7);
        assert_eq!(&encoded[1..8], b"_mythos");
        assert_eq!(encoded[8], 4);
        assert_eq!(&encoded[9..13], b"_tcp");
        assert_eq!(encoded[13], 5);
        assert_eq!(&encoded[14..19], b"local");
        assert_eq!(encoded[19], 0x00);

        let (decoded, consumed) = decode_domain_name(&encoded, 0, 0).expect("decode domain name");
        assert_eq!(decoded, name);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_dns_packet_query_roundtrip() {
        let packet = DnsPacket::new_query(SERVICE_TYPE_OPCUA);
        let bytes = packet.to_bytes();

        let parsed = DnsPacket::parse(&bytes).expect("parse dns packet");
        assert_eq!(parsed.questions.len(), 1);
        assert_eq!(parsed.questions[0].name, SERVICE_TYPE_OPCUA);
        assert_eq!(parsed.questions[0].qtype, TYPE_PTR);
        assert_eq!(parsed.questions[0].qclass, CLASS_IN);
    }

    #[tokio::test]
    async fn test_mdns_discovery_simulated_exchange() {
        let (client_transport, server_transport) = SimulatedMdnsTransport::create_paired_network();

        let mut txt = HashMap::new();
        txt.insert("model".to_string(), "MythOS-IPC-100".to_string());
        txt.insert("version".to_string(), "0.1.0".to_string());

        let service = MdnsService {
            instance_name: "MythOS Edge 01".to_string(),
            service_type: SERVICE_TYPE_MYTHOS.to_string(),
            host: "mythos-01.local".to_string(),
            ip: Some(Ipv4Addr::new(192, 168, 1, 20)),
            port: 8080,
            txt_records: txt,
        };

        let responder = Arc::new(MdnsResponder::new(service));

        // Background server task
        let srv_transport = Arc::new(server_transport);
        let resp_clone = responder.clone();
        let srv_clone = srv_transport.clone();
        tokio::spawn(async move {
            if let Ok((data, _)) = srv_clone.recv_multicast().await {
                if let Ok(pkt) = DnsPacket::parse(&data) {
                    if let Some(resp) = resp_clone.handle_packet(&pkt) {
                        let _ = srv_clone.send_multicast(&resp.to_bytes()).await;
                    }
                }
            }
        });

        let client = MdnsDiscoveryClient::new(Arc::new(client_transport));
        let discovered = client
            .query_services(SERVICE_TYPE_MYTHOS, Duration::from_millis(500))
            .await
            .expect("mDNS query failed");

        assert_eq!(discovered.len(), 1);
        let dev = &discovered[0];
        assert_eq!(dev.instance_name, "MythOS Edge 01");
        assert_eq!(dev.service_type, SERVICE_TYPE_MYTHOS);
        assert_eq!(dev.port, 8080);
        assert_eq!(dev.ip, Some(Ipv4Addr::new(192, 168, 1, 20)));
        assert_eq!(
            dev.txt_records.get("model").map(|s| s.as_str()),
            Some("MythOS-IPC-100")
        );
    }
}
