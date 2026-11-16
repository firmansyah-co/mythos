//! The MythOS discovery module (ADR-012 §2.2): announces this node on the network over mDNS
//! (`_mythos._tcp.local.`) when its role or its own setting says to, and lists the other MythOS
//! nodes it hears.
//!
//! What it lists are **candidates** (ADR-009): a node is never trusted, connected to or deployed
//! to because it was heard. Nothing secret is announced: the node's id, name, role, site,
//! version, module protocol and port.
//!
//! It is an always-on service (the server starts it when it finds it), told which node it runs
//! on through `Call::Node`. mDNS is the pure-Rust `mdns-sd`, so it works the same on Linux, macOS
//! and Windows with no Avahi or Bonjour installed.

use async_trait::async_trait;
use mythos_module::protocol::{
    HttpCall, HttpReply, ModuleKind, ModuleManifest, RouteDecl, RouteScope, WireNode,
    PROTOCOL_VERSION,
};
use mythos_module::{Events, Service};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MODULE_ID: &str = "org.mythos.discovery";
pub const PROVIDES: &str = "discovery";
/// The DNS-SD service type MythOS nodes announce.
pub const SERVICE_TYPE: &str = "_mythos._tcp.local.";
/// Sent on the telemetry stream when the list of nodes heard changes.
pub const NODES_CHANGED_EVENT: &str = "NODES_CHANGED";

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.into(),
        name: "Discovery".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: ModuleKind::Service,
        provides: PROVIDES.into(),
        description: "Announces this node and lists the MythOS nodes on the network (mDNS)".into(),
        capabilities: vec![],
        config_schema: serde_json::Value::Null,
        routes: vec![RouteDecl {
            method: "GET".into(),
            path: "/nodes".into(),
            scope: RouteScope::Global,
            role: Some("viewer".into()),
            tool: "list_network_nodes".into(),
            summary: "The MythOS nodes heard on this network (mDNS), each a candidate to pair with, never trusted because it was heard; and whether this node announces itself.".into(),
            body: None,
            query: vec![],
            needs: vec![],
        }],
        always_on: true,
    }
}

/// A node heard on the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeardNode {
    pub id: String,
    pub name: String,
    pub role: String,
    pub site: String,
    pub version: String,
    pub protocol: Option<u32>,
    pub port: u16,
    pub addresses: Vec<String>,
    /// The mDNS instance it was heard as.
    pub instance: String,
    pub last_heard_ms: u64,
    /// Always true: heard is not trusted (ADR-009).
    pub candidate: bool,
}

/// What a resolved announcement says, before it is accepted as a node.
#[derive(Debug, Clone, Default)]
pub struct Announcement {
    pub instance: String,
    pub port: u16,
    pub addresses: Vec<String>,
    /// TXT properties.
    pub txt: BTreeMap<String, String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The nodes heard, by id. Kept apart from the network so that what it accepts and drops is
/// tested without multicast.
#[derive(Debug, Default)]
pub struct Heard {
    nodes: BTreeMap<String, HeardNode>,
}

impl Heard {
    /// Take in an announcement. One without an `id` is not a MythOS node and is ignored; one
    /// from `own_id` is this node. Returns whether the list changed.
    pub fn resolved(&mut self, a: &Announcement, own_id: Option<&str>) -> bool {
        let Some(id) = a.txt.get("id").filter(|id| !id.is_empty()) else {
            return false;
        };
        if Some(id.as_str()) == own_id {
            return false;
        }
        // An address is heard once per interface it arrives on: list each once.
        let mut addresses = a.addresses.clone();
        addresses.sort();
        addresses.dedup();
        let node = HeardNode {
            id: id.clone(),
            name: a.txt.get("name").cloned().unwrap_or_default(),
            role: a.txt.get("role").cloned().unwrap_or_default(),
            site: a.txt.get("site").cloned().unwrap_or_default(),
            version: a.txt.get("version").cloned().unwrap_or_default(),
            protocol: a.txt.get("protocol").and_then(|p| p.parse().ok()),
            port: a.port,
            addresses,
            instance: a.instance.clone(),
            last_heard_ms: now_ms(),
            candidate: true,
        };
        let changed = self.nodes.get(id).is_none_or(|old| {
            HeardNode {
                last_heard_ms: 0,
                ..old.clone()
            } != HeardNode {
                last_heard_ms: 0,
                ..node.clone()
            }
        });
        self.nodes.insert(id.clone(), node);
        changed
    }

    /// An instance said goodbye (or expired): forget the node heard as it.
    pub fn removed(&mut self, instance: &str) -> bool {
        let before = self.nodes.len();
        self.nodes.retain(|_, node| node.instance != instance);
        self.nodes.len() != before
    }

    pub fn list(&self) -> Vec<HeardNode> {
        self.nodes.values().cloned().collect()
    }
}

/// The TXT properties this node announces. Nothing secret: no token, no setup code.
pub fn txt_of(node: &WireNode) -> Vec<(String, String)> {
    vec![
        ("id".into(), node.id.clone()),
        ("name".into(), node.name.clone()),
        ("role".into(), node.role.clone()),
        ("site".into(), node.site.clone()),
        ("version".into(), node.version.clone()),
        ("protocol".into(), node.protocol.to_string()),
    ]
}

/// The mDNS instance name: the node's name, made unique by the start of its id.
pub fn instance_of(node: &WireNode) -> String {
    let short: String = node.id.chars().take(8).collect();
    let name: String = node
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == ' ')
        .take(40)
        .collect();
    format!("{} ({short})", name.trim())
}

struct State {
    daemon: Option<mdns_sd::ServiceDaemon>,
    error: Option<String>,
    own: Option<WireNode>,
    announced: Option<String>,
}

/// The discovery service.
pub struct Discovery {
    state: Mutex<State>,
    heard: Arc<Mutex<Heard>>,
}

impl Default for Discovery {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                daemon: None,
                error: None,
                own: None,
                announced: None,
            }),
            heard: Arc::new(Mutex::new(Heard::default())),
        }
    }
}

impl Discovery {
    /// Start listening, once: browse for MythOS nodes on a thread of its own.
    fn listen(&self, state: &mut State, events: &Events) {
        if state.daemon.is_some() || state.error.is_some() {
            return;
        }
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                state.error = Some(format!("mDNS cannot start on this machine: {e}"));
                return;
            }
        };
        let receiver = match daemon.browse(SERVICE_TYPE) {
            Ok(receiver) => receiver,
            Err(e) => {
                state.error = Some(format!("mDNS cannot browse: {e}"));
                return;
            }
        };
        let heard = Arc::clone(&self.heard);
        let own = state.own.as_ref().map(|n| n.id.clone());
        let own = Arc::new(Mutex::new(own));
        let events = events.clone();
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                let changed = match event {
                    mdns_sd::ServiceEvent::ServiceResolved(resolved) => {
                        let txt = resolved
                            .get_properties()
                            .iter()
                            .map(|p| (p.key().to_string(), p.val_str().to_string()))
                            .collect();
                        let announcement = Announcement {
                            instance: resolved.get_fullname().to_string(),
                            port: resolved.get_port(),
                            addresses: resolved
                                .get_addresses()
                                .iter()
                                .map(|a| a.to_ip_addr().to_string())
                                .collect(),
                            txt,
                        };
                        let own = own.lock().unwrap().clone();
                        heard
                            .lock()
                            .unwrap()
                            .resolved(&announcement, own.as_deref())
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => {
                        heard.lock().unwrap().removed(&fullname)
                    }
                    _ => false,
                };
                if changed {
                    events.send(NODES_CHANGED_EVENT, json!({}));
                }
            }
        });
        state.daemon = Some(daemon);
    }

    /// Announce `node`, or stop announcing, as it says.
    fn announce(&self, state: &mut State) {
        let Some(daemon) = state.daemon.as_ref() else {
            return;
        };
        if let Some(fullname) = state.announced.take() {
            let _ = daemon.unregister(&fullname);
        }
        let Some(node) = state.own.clone() else {
            return;
        };
        let Some(port) = node.port.filter(|_| node.announce) else {
            return;
        };
        let short: String = node.id.chars().take(8).collect();
        let info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance_of(&node),
            &format!("mythos-{short}.local."),
            "",
            port,
            &txt_of(&node)[..],
        )
        .map(|info| info.enable_addr_auto());
        match info {
            Ok(info) => {
                let fullname = info.get_fullname().to_string();
                match daemon.register(info) {
                    Ok(()) => state.announced = Some(fullname),
                    Err(e) => state.error = Some(format!("cannot announce: {e}")),
                }
            }
            Err(e) => state.error = Some(format!("cannot announce: {e}")),
        }
    }
}

#[async_trait]
impl Service for Discovery {
    async fn node(&self, node: WireNode, events: &Events) {
        let mut state = self.state.lock().unwrap();
        state.own = Some(node);
        self.listen(&mut state, events);
        self.announce(&mut state);
    }

    async fn handle(&self, call: HttpCall, _events: &Events) -> HttpReply {
        match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/nodes") => {
                let state = self.state.lock().unwrap();
                let own = state.own.as_ref().map(|n| n.id.clone());
                let nodes: Vec<HeardNode> = self
                    .heard
                    .lock()
                    .unwrap()
                    .list()
                    .into_iter()
                    .filter(|n| Some(&n.id) != own.as_ref())
                    .collect();
                HttpReply::json(
                    200,
                    json!({
                        "self": own,
                        "listening": state.daemon.is_some(),
                        "announcing": state.announced.is_some(),
                        "error": state.error,
                        "nodes": nodes,
                    }),
                )
            }
            _ => HttpReply::error(404, "not a route of the discovery module"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard_as(id: &str, instance: &str) -> Announcement {
        Announcement {
            instance: instance.into(),
            port: 8080,
            addresses: vec![
                "192.168.10.21".into(),
                "10.0.0.5".into(),
                "fe80::1".into(),
                "192.168.10.21".into(),
                "fe80::1".into(),
            ],
            txt: [
                ("id", id),
                ("name", "boiler-edge-1"),
                ("role", "edge"),
                ("version", "0.1.0"),
                ("protocol", "1"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        }
    }

    #[test]
    fn a_node_heard_is_listed_as_a_candidate_and_forgotten_when_it_leaves() {
        let mut heard = Heard::default();
        assert!(heard.resolved(
            &heard_as("7f3c", "boiler (7f3c)._mythos._tcp.local."),
            Some("self")
        ));
        let list = heard.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "boiler-edge-1");
        assert_eq!(list[0].protocol, Some(1));
        assert_eq!(
            list[0].addresses,
            vec!["10.0.0.5", "192.168.10.21", "fe80::1"],
            "each address once, however many interfaces it was heard on"
        );
        assert!(list[0].candidate, "heard is never trusted");

        // Heard again unchanged: nothing to tell.
        assert!(!heard.resolved(
            &heard_as("7f3c", "boiler (7f3c)._mythos._tcp.local."),
            Some("self")
        ));
        assert!(heard.removed("boiler (7f3c)._mythos._tcp.local."));
        assert!(heard.list().is_empty());
    }

    #[test]
    fn itself_and_announcements_without_an_id_are_not_nodes() {
        let mut heard = Heard::default();
        assert!(!heard.resolved(&heard_as("self", "me._mythos._tcp.local."), Some("self")));
        let mut anonymous = heard_as("x", "printer._mythos._tcp.local.");
        anonymous.txt.remove("id");
        assert!(!heard.resolved(&anonymous, Some("self")));
        assert!(heard.list().is_empty());
    }

    #[test]
    fn what_is_announced_holds_no_secret() {
        let node = WireNode {
            id: "7f3c2a10-5b9e-4c1d-9a2f-0e6b8d4c1a77".into(),
            name: "Boiler edge 1".into(),
            role: "edge".into(),
            site: "Boiler house".into(),
            announce: true,
            port: Some(8080),
            version: "0.1.0".into(),
            protocol: 1,
        };
        let keys: Vec<String> = txt_of(&node).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["id", "name", "role", "site", "version", "protocol"]);
        assert_eq!(instance_of(&node), "Boiler edge 1 (7f3c2a10)");
    }

    #[test]
    fn it_runs_always_and_serves_one_route_to_anyone_signed_in() {
        let m = manifest();
        assert!(m.always_on);
        assert_eq!(m.routes.len(), 1);
        assert_eq!(m.routes[0].role.as_deref(), Some("viewer"));
        assert_eq!(m.routes[0].scope, RouteScope::Global);
    }
}
