//! This installation as a node (ADR-012 §2.1): who it is, and what role it plays.
//!
//! Written once to `<system dir>/node.json` on first start. The id never changes: pairing and
//! plant projects refer to a node by it, so re-addressing or renaming a device breaks nothing.
//! The role is a preset of defaults (where the server listens, and from ADR-012 step 2 what it
//! enables), never a different product.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Where this node is meant to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    /// An engineer's computer: the desktop app. Reachable from this computer only.
    Workstation,
    /// A device in the plant (IPC, gateway, panel PC): reads the field, reachable on its networks.
    Edge,
    /// On premises or in the cloud: holds plant projects and the fleet.
    Server,
}

impl NodeRole {
    pub const ALL: [NodeRole; 3] = [NodeRole::Workstation, NodeRole::Edge, NodeRole::Server];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeRole::Workstation => "workstation",
            NodeRole::Edge => "edge",
            NodeRole::Server => "server",
        }
    }

    pub fn parse(text: &str) -> Option<NodeRole> {
        NodeRole::ALL
            .into_iter()
            .find(|r| r.as_str() == text.trim())
    }

    /// The address the role listens on: a workstation on its own loopback only, every other
    /// role on every interface.
    pub fn listen_ip(self) -> IpAddr {
        match self {
            NodeRole::Workstation => IpAddr::V4(Ipv4Addr::LOCALHOST),
            NodeRole::Edge | NodeRole::Server => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        }
    }
}

/// `node.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub id: String,
    pub name: String,
    pub role: NodeRole,
    #[serde(default)]
    pub site: String,
}

/// Chooses the role of a node on its first start, when `node.json` does not exist yet.
pub const ROLE_ENV: &str = "MYTHOS_ROLE";
/// Overrides the address the server listens on (`127.0.0.1`, `0.0.0.0`, an interface's address),
/// whatever the role says.
pub const BIND_ENV: &str = "MYTHOS_BIND";

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("{0}")]
    Invalid(String),
    #[error("node.json: {0}")]
    Io(#[from] std::io::Error),
    #[error("node.json is not readable: {0}")]
    Json(#[from] serde_json::Error),
}

/// The node, its file, and the address this run of the server listens on.
pub struct Node {
    path: PathBuf,
    identity: RwLock<NodeIdentity>,
    listening: RwLock<Option<SocketAddr>>,
}

/// A random version-4 UUID, without a dependency for it.
fn new_id() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The computer's name, as the `hostname` command (on Linux, macOS and Windows alike) or the
/// environment says; `None` when neither does.
fn host_name() -> Option<String> {
    let from_command = std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());
    from_command
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|name| name.trim().to_string())
        .filter(|name| valid_name(name).is_ok())
}

fn valid_name(name: &str) -> Result<(), NodeError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NodeError::Invalid("a node needs a name".into()));
    }
    if name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(NodeError::Invalid(
            "a node's name is at most 64 characters, without control characters".into(),
        ));
    }
    Ok(())
}

fn valid_site(site: &str) -> Result<(), NodeError> {
    if site.chars().count() > 128 || site.chars().any(char::is_control) {
        return Err(NodeError::Invalid(
            "a site is at most 128 characters, without control characters".into(),
        ));
    }
    Ok(())
}

/// What may be changed: each field that is present.
#[derive(Debug, Default, Deserialize)]
pub struct NodeChange {
    pub name: Option<String>,
    pub site: Option<String>,
    pub role: Option<String>,
}

impl Node {
    /// Read `node.json` from `system_dir`, or write it: a new id, the computer's name, and
    /// `first_role` (or `MYTHOS_ROLE`, when it names a role).
    pub fn open(system_dir: &Path, first_role: NodeRole) -> Result<Node, NodeError> {
        let path = system_dir.join("node.json");
        let identity = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<NodeIdentity>(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let role = match std::env::var(ROLE_ENV) {
                    Ok(text) => NodeRole::parse(&text).ok_or_else(|| {
                        NodeError::Invalid(format!(
                            "{ROLE_ENV}={text} names no role; use workstation, edge or server"
                        ))
                    })?,
                    Err(_) => first_role,
                };
                let id = new_id();
                let identity = NodeIdentity {
                    name: host_name().unwrap_or_else(|| format!("mythos-{}", &id[..8])),
                    id,
                    role,
                    site: String::new(),
                };
                write(&path, &identity)?;
                identity
            }
            Err(e) => return Err(e.into()),
        };
        Ok(Node {
            path,
            identity: RwLock::new(identity),
            listening: RwLock::new(None),
        })
    }

    pub fn identity(&self) -> NodeIdentity {
        self.identity.read().unwrap().clone()
    }

    /// Change what `change` names; nothing is written unless all of it is valid.
    pub fn update(&self, change: NodeChange) -> Result<NodeIdentity, NodeError> {
        let mut next = self.identity();
        if let Some(name) = change.name {
            valid_name(&name)?;
            next.name = name.trim().to_string();
        }
        if let Some(site) = change.site {
            valid_site(&site)?;
            next.site = site.trim().to_string();
        }
        if let Some(role) = change.role {
            next.role = NodeRole::parse(&role).ok_or_else(|| {
                NodeError::Invalid(format!(
                    "'{role}' is not a role; use workstation, edge or server"
                ))
            })?;
        }
        let mut current = self.identity.write().unwrap();
        write(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }

    /// Where the server should listen: `MYTHOS_BIND` when set, otherwise the role's address.
    pub fn listen_ip(&self) -> Result<IpAddr, NodeError> {
        match std::env::var(BIND_ENV) {
            Ok(text) if !text.trim().is_empty() => text
                .trim()
                .parse()
                .map_err(|_| NodeError::Invalid(format!("{BIND_ENV}={text} is not an IP address"))),
            _ => Ok(self.identity().role.listen_ip()),
        }
    }

    /// Record where this run of the server listens, for `GET /api/v1/node`.
    pub fn set_listening(&self, address: SocketAddr) {
        *self.listening.write().unwrap() = Some(address);
    }

    pub fn listening(&self) -> Option<SocketAddr> {
        *self.listening.read().unwrap()
    }
}

fn write(path: &Path, identity: &NodeIdentity) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(identity)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_is_made_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let first = Node::open(dir.path(), NodeRole::Edge).unwrap().identity();
        assert_eq!(first.role, NodeRole::Edge);
        assert_eq!(first.id.len(), 36);
        assert_eq!(&first.id[14..15], "4", "a version-4 UUID: {}", first.id);
        assert!(!first.name.is_empty());

        // Opened again, with another first role: the file wins.
        let again = Node::open(dir.path(), NodeRole::Workstation).unwrap();
        assert_eq!(again.identity(), first);

        let changed = again
            .update(NodeChange {
                name: Some(" Boiler edge 1 ".into()),
                site: Some("Boiler house".into()),
                role: Some("server".into()),
            })
            .unwrap();
        assert_eq!(
            (changed.name.as_str(), changed.site.as_str(), changed.role),
            ("Boiler edge 1", "Boiler house", NodeRole::Server)
        );
        assert_eq!(changed.id, first.id, "the id never changes");
        assert_eq!(
            Node::open(dir.path(), NodeRole::Edge).unwrap().identity(),
            changed
        );
    }

    #[test]
    fn an_invalid_change_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let node = Node::open(dir.path(), NodeRole::Edge).unwrap();
        let before = node.identity();
        for change in [
            NodeChange {
                name: Some("  ".into()),
                ..Default::default()
            },
            NodeChange {
                name: Some("a\u{7}b".into()),
                ..Default::default()
            },
            NodeChange {
                name: Some("ok".into()),
                role: Some("plc".into()),
                ..Default::default()
            },
            NodeChange {
                site: Some("x".repeat(129)),
                ..Default::default()
            },
        ] {
            assert!(matches!(node.update(change), Err(NodeError::Invalid(_))));
        }
        assert_eq!(node.identity(), before);
        assert_eq!(
            Node::open(dir.path(), NodeRole::Edge).unwrap().identity(),
            before
        );
    }

    #[test]
    fn a_workstation_listens_on_its_own_computer_only() {
        assert_eq!(NodeRole::Workstation.listen_ip().to_string(), "127.0.0.1");
        assert_eq!(NodeRole::Edge.listen_ip().to_string(), "0.0.0.0");
        assert_eq!(NodeRole::Server.listen_ip().to_string(), "0.0.0.0");
    }
}
