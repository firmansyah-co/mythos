//! Tags of other devices of the plant, read over the pairing (ADR-012 §2.6, read only).
//!
//! A tag of another device is named with the device: `edge1:external.Boiler.U1.p`. The devices
//! are the ones of the plant this node runs (its `plant.json`, or the device table a download
//! brought, [`crate::plant::PLANT_DEVICES_FILE`]); each is followed on the node it is assigned
//! to, over HTTPS pinned to that node's certificate and with the pairing's token, by subscribing
//! to its telemetry stream.
//!
//! The honesty rule holds across the link. While a link is down, every tag last heard from it is
//! reported `BAD_COMM_FAILURE` with no value: never as its last value. A link is down when it
//! cannot be opened, when the stream ends, and when nothing has been heard on it for
//! [`SILENCE`] (the stream sends a batch every 100 ms).

use crate::api::telemetry::TagValueUpdate;
use crate::pairing::Pairings;
use mythos_pal::PlatformShm;
use mythos_shm::slot::QualityCode;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Nothing heard on a link for this long and it is down.
pub const SILENCE: Duration = Duration::from_secs(3);
const RETRY_FIRST: Duration = Duration::from_millis(500);
const RETRY_MOST: Duration = Duration::from_secs(10);
/// How often the plant's devices are read again.
const DEVICES_FRESH: Duration = Duration::from_secs(1);
/// The stream a link follows.
const STREAM_PATH: &str = "/api/v1/telemetry/stream";

/// The separator between a device and its tag.
pub const DEVICE_SEPARATOR: char = ':';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Being opened; nothing heard yet.
    Connecting,
    /// Batches arriving.
    Connected,
    /// Down: values are not current.
    Lost,
}

/// A device of the plant, followed from here.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RemoteDevice {
    pub device: String,
    pub name: String,
    pub node: String,
}

/// One link as it is shown.
#[derive(Clone, Debug, Serialize)]
pub struct LinkView {
    pub device: String,
    pub name: String,
    pub node: String,
    pub state: LinkState,
    /// Why it is not connected.
    pub reason: Option<String>,
    /// When it came to this state.
    pub since_unix_s: u64,
    /// Tags heard from it. While it is down they are reported, without values.
    pub tags: usize,
}

struct Link {
    state: LinkState,
    reason: Option<String>,
    since_unix_s: u64,
    last_heard: Option<Instant>,
    /// The latest batch, by remote URI.
    values: BTreeMap<String, TagValueUpdate>,
    stop: Arc<AtomicBool>,
}

impl Link {
    fn to(&mut self, state: LinkState, reason: Option<String>) {
        if self.state != state || self.reason != reason {
            self.state = state;
            self.reason = reason;
            self.since_unix_s = now_s();
        }
    }
}

type Links = Arc<Mutex<HashMap<String, Link>>>;

/// The links this node keeps to the other devices of its plant.
pub struct RemoteTags {
    pairings: Arc<Pairings>,
    links: Links,
    devices: Mutex<(Option<Instant>, Vec<RemoteDevice>)>,
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl RemoteTags {
    pub fn new(pairings: Arc<Pairings>) -> Self {
        RemoteTags {
            pairings,
            links: Arc::new(Mutex::new(HashMap::new())),
            devices: Mutex::new((None, Vec::new())),
        }
    }

    /// The devices to follow, read again by `resolve` at most once a second. Links to nodes no
    /// longer among them are closed; links to new ones are opened.
    pub fn devices(&self, resolve: impl FnOnce() -> Vec<RemoteDevice>) -> Vec<RemoteDevice> {
        let mut cached = self.devices.lock().unwrap_or_else(|p| p.into_inner());
        if cached.0.is_some_and(|at| at.elapsed() < DEVICES_FRESH) {
            return cached.1.clone();
        }
        let mut devices = resolve();
        devices.sort();
        devices.dedup();
        let nodes: BTreeSet<String> = devices.iter().map(|d| d.node.clone()).collect();
        {
            let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            links.retain(|node, link| {
                let keep = nodes.contains(node);
                if !keep {
                    link.stop.store(true, Ordering::SeqCst);
                }
                keep
            });
            for node in nodes {
                if !links.contains_key(&node) {
                    let stop = Arc::new(AtomicBool::new(false));
                    links.insert(
                        node.clone(),
                        Link {
                            state: LinkState::Connecting,
                            reason: None,
                            since_unix_s: now_s(),
                            last_heard: None,
                            values: BTreeMap::new(),
                            stop: Arc::clone(&stop),
                        },
                    );
                    follow(
                        Arc::clone(&self.pairings),
                        Arc::clone(&self.links),
                        node,
                        stop,
                    );
                }
            }
        }
        *cached = (Some(Instant::now()), devices.clone());
        devices
    }

    /// Every link, by device.
    pub fn links(&self, devices: &[RemoteDevice]) -> Vec<LinkView> {
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        devices
            .iter()
            .map(|d| match links.get_mut(&d.node) {
                Some(link) => {
                    mark_silent(link);
                    LinkView {
                        device: d.device.clone(),
                        name: d.name.clone(),
                        node: d.node.clone(),
                        state: link.state,
                        reason: link.reason.clone(),
                        since_unix_s: link.since_unix_s,
                        tags: link.values.len(),
                    }
                }
                None => LinkView {
                    device: d.device.clone(),
                    name: d.name.clone(),
                    node: d.node.clone(),
                    state: LinkState::Connecting,
                    reason: None,
                    since_unix_s: now_s(),
                    tags: 0,
                },
            })
            .collect()
    }

    /// Every remote tag now, named with its device. A tag of a link that is down is
    /// `BAD_COMM_FAILURE` with no value.
    pub fn updates(&self, devices: &[RemoteDevice]) -> Vec<TagValueUpdate> {
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        let mut out = Vec::new();
        for d in devices {
            let Some(link) = links.get_mut(&d.node) else {
                continue;
            };
            mark_silent(link);
            let current = link.state == LinkState::Connected;
            for (uri, sample) in &link.values {
                let mut update = sample.clone();
                update.uri = format!("{}{DEVICE_SEPARATOR}{uri}", d.device);
                update.slot_id = None;
                if !current {
                    let lost = QualityCode::BAD_COMM_FAILURE;
                    update.value = serde_json::Value::Null;
                    update.quality = lost.class_str().to_string();
                    update.quality_code = lost.0;
                    update.quality_name = Cow::Borrowed(lost.as_str());
                }
                out.push(update);
            }
        }
        out
    }
}

impl Drop for RemoteTags {
    fn drop(&mut self) {
        let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        for link in links.values() {
            link.stop.store(true, Ordering::SeqCst);
        }
    }
}

/// A connected link that has gone quiet is down, whatever its thread is doing.
fn mark_silent(link: &mut Link) {
    if link.state == LinkState::Connected && link.last_heard.is_none_or(|at| at.elapsed() > SILENCE)
    {
        link.to(
            LinkState::Lost,
            Some(format!("nothing heard for {} s", SILENCE.as_secs())),
        );
    }
}

fn with_link(links: &Links, node: &str, f: impl FnOnce(&mut Link)) -> bool {
    let mut links = links.lock().unwrap_or_else(|p| p.into_inner());
    match links.get_mut(node) {
        Some(link) => {
            f(link);
            true
        }
        None => false,
    }
}

/// One batch from a node: its samples, the quality named here from its code.
fn samples(data: &str) -> Option<Vec<TagValueUpdate>> {
    #[derive(serde::Deserialize)]
    struct Batch {
        updates: Vec<TagValueUpdate>,
    }
    let batch: Batch = serde_json::from_str(data).ok()?;
    Some(
        batch
            .updates
            .into_iter()
            .map(|mut u| {
                // The name and class are this build's for the code, as for a local tag: what
                // the other node called it is not trusted to agree with the word.
                let code = QualityCode(u.quality_code);
                u.quality = code.class_str().to_string();
                u.quality_name = Cow::Borrowed(code.as_str());
                u
            })
            .collect(),
    )
}

/// Follow `node`'s telemetry stream on a thread of its own until told to stop.
fn follow(pairings: Arc<Pairings>, links: Links, node: String, stop: Arc<AtomicBool>) {
    let label = node.chars().take(8).collect::<String>();
    std::thread::Builder::new()
        .name(format!("remote-{label}"))
        .spawn(move || {
            let mut wait = RETRY_FIRST;
            while !stop.load(Ordering::SeqCst) {
                let ended = match reach(&pairings, &node) {
                    Err(why) => why,
                    Ok((address, fingerprint, token)) => crate::tls::stream_pinned(
                        address,
                        &fingerprint,
                        STREAM_PATH,
                        Some(&token),
                        SILENCE,
                        |event, data| {
                            if stop.load(Ordering::SeqCst) {
                                return false;
                            }
                            if event != "tag_update" {
                                return true;
                            }
                            let Some(batch) = samples(data) else {
                                return true;
                            };
                            wait = RETRY_FIRST;
                            with_link(&links, &node, |link| {
                                link.values =
                                    batch.into_iter().map(|u| (u.uri.clone(), u)).collect();
                                link.last_heard = Some(Instant::now());
                                link.to(LinkState::Connected, None);
                            })
                        },
                    )
                    .err()
                    .unwrap_or_else(|| "stopped".to_string()),
                };
                if stop.load(Ordering::SeqCst)
                    || !with_link(&links, &node, |link| link.to(LinkState::Lost, Some(ended)))
                {
                    return;
                }
                let deadline = Instant::now() + wait;
                while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                wait = (wait * 2).min(RETRY_MOST);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!("cannot follow node {label}…: {e}"));
}

/// The other devices of the plant this node runs, each on a node other than this one.
pub fn devices_of_running_plant<S: PlatformShm + 'static>(
    state: &crate::api::ApiState<S>,
) -> Vec<RemoteDevice> {
    let own = state
        .node
        .as_ref()
        .map(|n| n.identity().id)
        .unwrap_or_default();
    let Ok(Some(active)) = state.storage.get_active_project() else {
        return Vec::new();
    };
    match crate::plant::plant_of_running(&state.storage, &active) {
        Ok(Some(table)) => table
            .devices
            .into_iter()
            .filter_map(|d| {
                let node = d.node.filter(|n| !n.is_empty() && *n != own)?;
                Some(RemoteDevice {
                    device: d.id,
                    name: d.name,
                    node,
                })
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!("the plant of '{active}' cannot be read: {e}");
            Vec::new()
        }
    }
}

/// Where and how to reach a paired node.
fn reach(pairings: &Pairings, node: &str) -> Result<(SocketAddr, String, String), String> {
    let peer = pairings
        .peer(node)
        .ok_or_else(|| format!("node {node} is not paired with this one"))?;
    let address = peer
        .address
        .as_deref()
        .ok_or_else(|| format!("{} has no address to reach it at", peer.name))?;
    let address = address
        .to_socket_addrs()
        .map_err(|e| format!("{address}: {e}"))?
        .next()
        .ok_or_else(|| format!("{address} resolves to nothing"))?;
    if peer.token_to_it.is_empty() {
        return Err(format!("no token to present to {}", peer.name));
    }
    Ok((address, peer.fingerprint, peer.token_to_it))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_is_named_by_this_build_from_its_code() {
        let data = r#"{"timestamp_ms":1,"batch_window_ms":100,"updates":[
            {"uri":"a","slot_id":3,"type":"REAL","value":1.5,"quality":"GOOD","quality_code":24,
             "quality_name":"GOOD","timestamp_ns":7}]}"#;
        let got = samples(data).unwrap();
        assert_eq!(got[0].quality, "BAD");
        assert_eq!(got[0].quality_name, "BAD_COMM_FAILURE");
        assert_eq!(got[0].value, serde_json::json!(1.5));
    }

    fn link(state: LinkState, heard: Option<Instant>) -> Link {
        let mut values = BTreeMap::new();
        values.insert(
            "external.p".to_string(),
            samples(
                r#"{"updates":[{"uri":"external.p","slot_id":1,"type":"REAL","value":4.25,
                "quality":"GOOD","quality_code":192,"quality_name":"GOOD","timestamp_ns":1}]}"#,
            )
            .unwrap()
            .remove(0),
        );
        Link {
            state,
            reason: None,
            since_unix_s: 0,
            last_heard: heard,
            values,
            stop: Arc::new(AtomicBool::new(true)),
        }
    }

    fn remote(node_link: Link) -> (RemoteTags, Vec<RemoteDevice>) {
        let dir = tempfile::tempdir().unwrap();
        let tags = RemoteTags::new(Arc::new(Pairings::open(dir.path()).unwrap()));
        tags.links
            .lock()
            .unwrap()
            .insert("n1".to_string(), node_link);
        let devices = vec![RemoteDevice {
            device: "edge1".into(),
            name: "Edge 1".into(),
            node: "n1".into(),
        }];
        (tags, devices)
    }

    #[test]
    fn a_connected_link_gives_its_values_named_with_the_device() {
        let (tags, devices) = remote(link(LinkState::Connected, Some(Instant::now())));
        let got = tags.updates(&devices);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].uri, "edge1:external.p");
        assert_eq!(got[0].value, serde_json::json!(4.25));
        assert_eq!(got[0].quality_name, "GOOD");
        assert_eq!(got[0].slot_id, None, "a remote tag has no slot here");
    }

    #[test]
    fn a_lost_link_gives_no_value_never_the_last_one() {
        let (tags, devices) = remote(link(LinkState::Lost, Some(Instant::now())));
        let got = tags.updates(&devices);
        assert_eq!(got[0].uri, "edge1:external.p");
        assert_eq!(got[0].value, serde_json::Value::Null);
        assert_eq!(got[0].quality, "BAD");
        assert_eq!(got[0].quality_name, "BAD_COMM_FAILURE");
    }

    #[test]
    fn a_link_that_goes_quiet_is_lost_without_its_thread_saying_so() {
        let quiet = Instant::now() - SILENCE - Duration::from_millis(10);
        let (tags, devices) = remote(link(LinkState::Connected, Some(quiet)));
        let got = tags.updates(&devices);
        assert_eq!(got[0].value, serde_json::Value::Null);
        assert_eq!(got[0].quality_name, "BAD_COMM_FAILURE");
        let view = tags.links(&devices);
        assert_eq!(view[0].state, LinkState::Lost);
        assert!(view[0].reason.as_deref().unwrap().contains("nothing heard"));
    }
}
