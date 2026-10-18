//! Device Network Topology Data Model & Validation Engine.
//!
//! Provides the graph data model for physical and logical network architectures,
//! including modular racks, Ethernet port-to-port cabling, RS-485 daisy-chaining,
//! and subnet address validation according to SYS-REQ-052/053 and MYTHOS-HMI-004.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Errors occurring during topology manipulation and validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TopologyError {
    #[error("Node with ID '{0}' already exists in topology")]
    DuplicateNodeId(String),

    #[error("Node with ID '{0}' not found")]
    NodeNotFound(String),

    #[error("Edge from '{from}' to '{to}' references non-existent node")]
    InvalidEdgeEndpoint { from: String, to: String },

    #[error("Port '{port}' not found on node '{node_id}'")]
    PortNotFound { node_id: String, port: String },

    #[error("Subnet block '{0}' exhausted or invalid")]
    IpAllocationError(String),
}

/// Topology validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyValidationError {
    pub severity: ValidationSeverity,
    pub rule: String,
    pub message: String,
    pub affected_nodes: Vec<String>,
}

/// Severity level for topology validation findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

/// Node classification types representing industrial devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Controller,
    RemoteIo,
    Sensor,
    Switch,
    Inverter,
}

/// Physical or logical port on a network node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDefinition {
    pub id: String,
    pub name: String,
    pub connector_type: String,
}

/// A node in the network topology graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyNode {
    pub id: String,
    pub station_name: String,
    pub node_type: NodeType,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub subnet_mask: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortDefinition>,
    #[serde(default)]
    pub parent_rack: Option<String>,
    #[serde(default)]
    pub rack_slot: Option<u32>,
    #[serde(default)]
    pub modbus_slave_id: Option<u8>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl TopologyNode {
    /// Create a new topology node.
    pub fn new(
        id: impl Into<String>,
        station_name: impl Into<String>,
        node_type: NodeType,
    ) -> Self {
        Self {
            id: id.into(),
            station_name: station_name.into(),
            node_type,
            vendor: None,
            model: None,
            mac_address: None,
            ip_address: None,
            subnet_mask: None,
            gateway: None,
            ports: Vec::new(),
            parent_rack: None,
            rack_slot: None,
            modbus_slave_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a port to the node.
    pub fn with_port(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        connector: impl Into<String>,
    ) -> Self {
        self.ports.push(PortDefinition {
            id: id.into(),
            name: name.into(),
            connector_type: connector.into(),
        });
        self
    }

    /// Set IP networking suite.
    pub fn with_ip(
        mut self,
        ip: impl Into<String>,
        subnet: impl Into<String>,
        gw: impl Into<String>,
    ) -> Self {
        self.ip_address = Some(ip.into());
        self.subnet_mask = Some(subnet.into());
        self.gateway = Some(gw.into());
        self
    }
}

/// Industrial Ethernet bus topology structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EthernetBusTopology {
    Star,
    Ring,
    Line,
    ProfinetIo,
}

/// Topology edge representing a physical cable or multi-drop fieldbus link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TopologyEdge {
    EthernetLink {
        id: String,
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
        topology: EthernetBusTopology,
        speed_mbps: Option<u32>,
    },
    SerialBus {
        id: String,
        from_node: String,
        to_node: String,
        channel: String,
        baud_rate: u32,
        parity: String,
        is_daisy_chain: bool,
    },
}

impl TopologyEdge {
    pub fn id(&self) -> &str {
        match self {
            Self::EthernetLink { id, .. } => id,
            Self::SerialBus { id, .. } => id,
        }
    }

    pub fn from_node(&self) -> &str {
        match self {
            Self::EthernetLink { from_node, .. } => from_node,
            Self::SerialBus { from_node, .. } => from_node,
        }
    }

    pub fn to_node(&self) -> &str {
        match self {
            Self::EthernetLink { to_node, .. } => to_node,
            Self::SerialBus { to_node, .. } => to_node,
        }
    }
}

/// Assignable IP address range block for DHCP / automatic allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpBlock {
    pub subnet: String,
    pub netmask: String,
    pub gateway: String,
    pub pool_start: String,
    pub pool_end: String,
}

/// Complete Device Network Topology representation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub nodes: HashMap<String, TopologyNode>,
    pub edges: Vec<TopologyEdge>,
    pub ip_blocks: Vec<IpBlock>,
}

impl NetworkTopology {
    /// Create a new empty network topology.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            ip_blocks: Vec::new(),
        }
    }

    /// Add a node to the topology.
    pub fn add_node(&mut self, node: TopologyNode) -> Result<(), TopologyError> {
        if self.nodes.contains_key(&node.id) {
            return Err(TopologyError::DuplicateNodeId(node.id));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Retrieve a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&TopologyNode> {
        self.nodes.get(id)
    }

    /// Retrieve a mutable reference to a node.
    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut TopologyNode> {
        self.nodes.get_mut(id)
    }

    /// Remove a node and its incident edges.
    pub fn remove_node(&mut self, id: &str) -> Option<TopologyNode> {
        let removed = self.nodes.remove(id);
        if removed.is_some() {
            self.edges
                .retain(|e| e.from_node() != id && e.to_node() != id);
        }
        removed
    }

    /// Add a physical or bus connection edge.
    pub fn add_edge(&mut self, edge: TopologyEdge) -> Result<(), TopologyError> {
        let from = edge.from_node();
        let to = edge.to_node();

        let from_node = self
            .nodes
            .get(from)
            .ok_or_else(|| TopologyError::InvalidEdgeEndpoint {
                from: from.to_string(),
                to: to.to_string(),
            })?;

        let to_node = self
            .nodes
            .get(to)
            .ok_or_else(|| TopologyError::InvalidEdgeEndpoint {
                from: from.to_string(),
                to: to.to_string(),
            })?;

        // If EthernetLink, verify ports exist
        if let TopologyEdge::EthernetLink {
            ref from_port,
            ref to_port,
            ..
        } = edge
        {
            if !from_node.ports.iter().any(|p| &p.id == from_port) {
                return Err(TopologyError::PortNotFound {
                    node_id: from.to_string(),
                    port: from_port.clone(),
                });
            }
            if !to_node.ports.iter().any(|p| &p.id == to_port) {
                return Err(TopologyError::PortNotFound {
                    node_id: to.to_string(),
                    port: to_port.clone(),
                });
            }
        }

        self.edges.push(edge);
        Ok(())
    }

    /// Add an assignable IP block.
    pub fn add_ip_block(&mut self, block: IpBlock) {
        self.ip_blocks.push(block);
    }

    /// Validate network topology integrity against industrial engineering invariants.
    pub fn validate(&self) -> Vec<TopologyValidationError> {
        let mut errors = Vec::new();

        // 1. Check Station Name Duplicates
        let mut station_names: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in self.nodes.values() {
            station_names
                .entry(node.station_name.as_str())
                .or_default()
                .push(node.id.as_str());
        }
        for (name, node_ids) in station_names {
            if node_ids.len() > 1 {
                errors.push(TopologyValidationError {
                    severity: ValidationSeverity::Error,
                    rule: "SYS-REQ-TOPOLOGY-001".to_string(),
                    message: format!(
                        "Duplicate station name '{}' detected across multiple nodes",
                        name
                    ),
                    affected_nodes: node_ids.into_iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        // 2. Check IP Address Conflicts
        let mut ip_map: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in self.nodes.values() {
            if let Some(ref ip) = node.ip_address {
                let ip_trimmed = ip.trim();
                if !ip_trimmed.is_empty() && ip_trimmed != "0.0.0.0" {
                    ip_map.entry(ip_trimmed).or_default().push(node.id.as_str());
                }
            }
        }
        for (ip, node_ids) in ip_map {
            if node_ids.len() > 1 {
                errors.push(TopologyValidationError {
                    severity: ValidationSeverity::Error,
                    rule: "SYS-REQ-TOPOLOGY-002".to_string(),
                    message: format!(
                        "IP address collision detected: '{}' is assigned to multiple devices",
                        ip
                    ),
                    affected_nodes: node_ids.into_iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        // 3. Check Unassigned IP Devices (Ethernet-based nodes: Controller, RemoteIO, Switch)
        for node in self.nodes.values() {
            let requires_ip = matches!(
                node.node_type,
                NodeType::Controller | NodeType::RemoteIo | NodeType::Switch
            );
            if requires_ip {
                let is_unassigned = match &node.ip_address {
                    None => true,
                    Some(ip) => ip.trim().is_empty() || ip.trim() == "0.0.0.0",
                };
                if is_unassigned {
                    errors.push(TopologyValidationError {
                        severity: ValidationSeverity::Warning,
                        rule: "SYS-REQ-TOPOLOGY-003".to_string(),
                        message: format!(
                            "Device '{}' ({:?}) has no assigned IP address (0.0.0.0 or unconfigured)",
                            node.station_name, node.node_type
                        ),
                        affected_nodes: vec![node.id.clone()],
                    });
                }
            }
        }

        // 4. Check Modbus Slave ID collisions on SerialBus edges
        let mut serial_buses: HashMap<&str, HashMap<u8, Vec<&str>>> = HashMap::new();
        for edge in &self.edges {
            if let TopologyEdge::SerialBus { ref channel, .. } = edge {
                if let Some(node) = self.nodes.get(edge.from_node()) {
                    if let Some(slave_id) = node.modbus_slave_id {
                        serial_buses
                            .entry(channel.as_str())
                            .or_default()
                            .entry(slave_id)
                            .or_default()
                            .push(node.id.as_str());
                    }
                }
                if let Some(node) = self.nodes.get(edge.to_node()) {
                    if let Some(slave_id) = node.modbus_slave_id {
                        serial_buses
                            .entry(channel.as_str())
                            .or_default()
                            .entry(slave_id)
                            .or_default()
                            .push(node.id.as_str());
                    }
                }
            }
        }
        for (channel, id_map) in serial_buses {
            for (slave_id, node_ids) in id_map {
                let mut unique_ids: Vec<&str> = node_ids.clone();
                unique_ids.sort();
                unique_ids.dedup();
                if unique_ids.len() > 1 {
                    errors.push(TopologyValidationError {
                        severity: ValidationSeverity::Error,
                        rule: "SYS-REQ-TOPOLOGY-004".to_string(),
                        message: format!(
                            "Modbus Slave ID collision on serial bus '{}': ID {} assigned to multiple devices",
                            channel, slave_id
                        ),
                        affected_nodes: unique_ids.into_iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }

        errors
    }

    /// Allocate the next available IPv4 address from an IP block.
    pub fn allocate_next_ip(&self, subnet_cidr: &str) -> Result<String, TopologyError> {
        let block = self
            .ip_blocks
            .iter()
            .find(|b| b.subnet == subnet_cidr)
            .ok_or_else(|| {
                TopologyError::IpAllocationError(format!("Subnet {} not found", subnet_cidr))
            })?;

        // Collect existing assigned IPs
        let assigned_ips: HashSet<&str> = self
            .nodes
            .values()
            .filter_map(|n| n.ip_address.as_deref())
            .collect();

        // Simple IPv4 sequential scan within pool_start .. pool_end
        let start_octets = parse_ipv4(&block.pool_start)
            .ok_or_else(|| TopologyError::IpAllocationError("Invalid pool_start IP".into()))?;
        let end_octets = parse_ipv4(&block.pool_end)
            .ok_or_else(|| TopologyError::IpAllocationError("Invalid pool_end IP".into()))?;

        let start_u32 = u32::from_be_bytes(start_octets);
        let end_u32 = u32::from_be_bytes(end_octets);

        for candidate in start_u32..=end_u32 {
            let candidate_str = format_ipv4(candidate.to_be_bytes());
            if !assigned_ips.contains(candidate_str.as_str()) && candidate_str != block.gateway {
                return Ok(candidate_str);
            }
        }

        Err(TopologyError::IpAllocationError(format!(
            "Subnet {} pool exhausted",
            subnet_cidr
        )))
    }
}

fn parse_ipv4(ip: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = ip.trim().split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        octets[i] = p.parse().ok()?;
    }
    Some(octets)
}

fn format_ipv4(octets: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}
