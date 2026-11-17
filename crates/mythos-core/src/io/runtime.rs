//! The I/O runtime: driver tasks, the scanner that owns the tag bus writer, and their status.
//!
//! Connections are independent units. [`IoRuntime::apply`] takes a new configuration and
//! changes only what differs from what is running: a connection whose configuration and driver
//! are unchanged keeps running untouched; one whose tags stay the same is swapped in place, the
//! new driver started before the old one is stopped, so its tags never stop updating; one whose
//! tags change is stopped and started again; one that is gone is stopped. A new driver that
//! cannot start leaves the one it was meant to replace running.

use crate::io::config::{ConnectionConfig, IoConfig};
use crate::io::registry::DriverRegistry;
use crate::tag::{AccessMode, TagCatalog, TagMetadata};
use mythos_pal::PlatformShm;
use mythos_plugin_sdk::{
    PluginContext, PluginError, ProtocolDriver, SampleFault, SampleValue, TagDeclaration, TagSample,
};
use mythos_shm::{QualityCode, ShmWriterHandle, TagDataType, TagValueRaw};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::{error, info, warn};

/// The capability granted to every protocol driver: it may open network connections.
const DRIVER_CAPABILITY: &str = "network:client";

// -----------------------------------------------------------------------------
// Status
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionState {
    /// The driver is being polled.
    Running,
    /// The connection could not be started. `detail` says why. None of its tags exist.
    ConfigError,
    /// The driver task has ended. Its tags carry a bad quality.
    Stopped,
}

/// The reason one tag had no reading in the last poll cycle.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TagFaultStatus {
    pub uri: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub id: String,
    pub driver: String,
    pub state: ConnectionState,
    /// Why the connection is not running, when it is not.
    pub detail: Option<String>,
    /// URIs of the tags this connection feeds.
    pub tags: Vec<String>,
    pub poll_interval_ms: u64,
    pub cycle_timeout_ms: u64,
    pub cycles_completed: u64,
    /// When the last poll cycle finished. `null` until one has.
    pub last_cycle_unix_ms: Option<u64>,
    /// Set when the last cycle failed as a whole (the driver errored or overran its deadline).
    pub last_cycle_error: Option<String>,
    /// How many tags had a reading in the last cycle. `null` until one has finished.
    pub tags_good: Option<usize>,
    /// The tags that had no reading in the last cycle, and why.
    pub faults: Vec<TagFaultStatus>,
}

/// What the I/O runtime is doing, for `GET /api/v1/io/status`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct IoStatus {
    /// The configuration file in effect, when there is one.
    pub source: Option<String>,
    /// Why no I/O is running, when none is.
    pub note: Option<String>,
    pub connections: Vec<ConnectionStatus>,
}

/// The shared, cheaply cloned handle to the field I/O of one server: what it is doing
/// ([`IoStatusHandle::snapshot`]) and the way to write through it
/// ([`IoStatusHandle::write_tag`]). One handle outlives every reload of the configuration, so
/// whoever holds it always reaches what is running now.
#[derive(Clone, Default)]
pub struct IoStatusHandle {
    status: Arc<StdRwLock<IoStatus>>,
    /// Every tag a running connection provides, and how to reach that connection's task.
    writes: Arc<StdRwLock<HashMap<String, WriteTarget>>>,
    /// The tags a runtime owns as its outputs (ADR-013 §2.4): who, and under which claim.
    owners: Arc<StdRwLock<HashMap<String, (String, u64)>>>,
    next_claim: Arc<std::sync::atomic::AtomicU64>,
}

/// Where a write to one tag goes.
#[derive(Clone)]
struct WriteTarget {
    /// The task of the connection that provides the tag. Polls and writes of one connection
    /// run in that one task, one at a time: a driver has one conversation with its device.
    connection: mpsc::Sender<WriteRequest>,
    /// What the driver declared.
    writable: bool,
}

struct WriteRequest {
    uri: String,
    value: serde_json::Value,
    done: oneshot::Sender<Result<(), WriteError>>,
}

/// Why a value was not written, or is not known to have been.
///
/// The cases are kept apart because what the caller may conclude differs: after the first
/// four nothing reached the device; after `NotConfirmed` it is not known whether the device
/// took the value, and saying "not written" would be a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// No running connection provides this tag.
    UnknownTag,
    /// The driver declares the tag read-only.
    ReadOnly,
    /// The connection that provides the tag is not running.
    Stopped,
    /// The driver refused before sending anything: the value does not fit, or is not a value.
    Refused(String),
    /// A request was or may have been sent and no confirmation came back.
    NotConfirmed(String),
    /// The tag is an output of the named runtime: only that runtime writes it. An operator
    /// changes what it does through a setpoint the program publishes.
    Owned(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::UnknownTag => write!(f, "no running connection provides this tag"),
            WriteError::ReadOnly => {
                write!(f, "the tag is read-only: its driver cannot write it")
            }
            WriteError::Stopped => write!(
                f,
                "the connection that provides this tag is not running; nothing was written"
            ),
            WriteError::Refused(why) => write!(f, "nothing was written: {why}"),
            WriteError::Owned(by) => write!(
                f,
                "nothing was written: the tag is an output of {by}, which alone writes it; change a setpoint of its program instead"
            ),
            WriteError::NotConfirmed(why) => write!(
                f,
                "the device did not confirm the write, so it is not known whether it took the value: {why}"
            ),
        }
    }
}

impl WriteError {
    /// What a driver's refusal means. A driver that says the tag or the value is wrong sent
    /// nothing; anything else happened during or after the conversation with the device.
    fn from_driver(e: PluginError) -> Self {
        match e {
            PluginError::InvalidTag(m) | PluginError::InvalidConfiguration(m) => {
                WriteError::Refused(m)
            }
            other => WriteError::NotConfirmed(other.to_string()),
        }
    }
}

impl IoStatusHandle {
    /// A status for a server on which no I/O runtime is running, stating why.
    pub fn not_running(note: impl Into<String>) -> Self {
        let handle = Self::default();
        handle.update(|status| status.note = Some(note.into()));
        handle
    }

    pub fn snapshot(&self) -> IoStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Write `value` to the field device behind `uri`, and wait for the device's answer.
    ///
    /// `Ok` means the driver had the device confirm the write. The tag's value on the tag bus
    /// is not touched: it changes when the next poll reads the device, so what is shown is
    /// always what was read, never what was asked for.
    pub async fn write_tag(&self, uri: &str, value: serde_json::Value) -> Result<(), WriteError> {
        self.write_tag_as(uri, value, None).await
    }

    /// As [`Self::write_tag`], by `owner`: a runtime writing one of its outputs. A tag that
    /// has an owner is written by that owner only, and by nobody else.
    pub async fn write_tag_as(
        &self,
        uri: &str,
        value: serde_json::Value,
        owner: Option<&str>,
    ) -> Result<(), WriteError> {
        let owned_by = self
            .owners
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(uri)
            .map(|(who, _)| who.clone());
        if let Some(who) = owned_by {
            if owner != Some(who.as_str()) {
                return Err(WriteError::Owned(who));
            }
        }
        let target = self
            .writes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(uri)
            .cloned();
        let target = target.ok_or(WriteError::UnknownTag)?;
        if !target.writable {
            return Err(WriteError::ReadOnly);
        }
        let (done, answer) = oneshot::channel();
        let request = WriteRequest {
            uri: uri.to_string(),
            value,
            done,
        };
        // Not delivered: the task is gone, and nothing was sent to the device.
        target
            .connection
            .send(request)
            .await
            .map_err(|_| WriteError::Stopped)?;
        // Delivered and then dropped unanswered: the task ended with the request in hand.
        answer.await.unwrap_or_else(|_| {
            Err(WriteError::NotConfirmed(
                "the connection stopped before answering".to_string(),
            ))
        })
    }

    /// Make `owner` the only writer of `tags`. Refused, changing nothing, when another owner
    /// has one of them. An owner claiming again (its replacement, started before it stops)
    /// takes its tags over under a new claim. Returns the claim, for [`Self::release`].
    pub fn claim(&self, owner: &str, tags: &[String]) -> Result<u64, String> {
        let mut owners = self.owners.write().unwrap_or_else(|e| e.into_inner());
        for tag in tags {
            if let Some((who, _)) = owners.get(tag) {
                if who != owner {
                    return Err(format!("{tag} is already an output of {who}"));
                }
            }
        }
        let claim = self
            .next_claim
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        for tag in tags {
            owners.insert(tag.clone(), (owner.to_string(), claim));
        }
        Ok(claim)
    }

    /// Whether `tag` is still held under `claim`: false once a later claim took it over.
    pub fn holds(&self, tag: &str, claim: u64) -> bool {
        self.owners
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(tag)
            .is_some_and(|(_, c)| *c == claim)
    }

    /// Give up what is still held under `claim`. Tags a later claim took over are not touched.
    pub fn release(&self, claim: u64) {
        self.owners
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, (_, c)| *c != claim);
    }

    /// Who owns `tag` as an output, if a runtime does.
    pub fn owner_of(&self, tag: &str) -> Option<String> {
        self.owners
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(tag)
            .map(|(who, _)| who.clone())
    }

    fn set_write_targets(&self, tags: &[(String, bool)], connection: &mpsc::Sender<WriteRequest>) {
        let mut writes = self.writes.write().unwrap_or_else(|e| e.into_inner());
        for (uri, writable) in tags {
            writes.insert(
                uri.clone(),
                WriteTarget {
                    connection: connection.clone(),
                    writable: *writable,
                },
            );
        }
    }

    fn drop_write_targets<'a>(&self, uris: impl Iterator<Item = &'a String>) {
        let mut writes = self.writes.write().unwrap_or_else(|e| e.into_inner());
        for uri in uris {
            writes.remove(uri);
        }
    }

    /// Report that nothing is running, and why. Replaces whatever was reported before.
    pub(crate) fn set_not_running(&self, note: impl Into<String>) {
        let note = note.into();
        self.update(|status| {
            *status = IoStatus {
                source: None,
                note: Some(note),
                connections: Vec::new(),
            };
        });
    }

    fn update(&self, change: impl FnOnce(&mut IoStatus)) {
        change(&mut self.status.write().unwrap_or_else(|e| e.into_inner()));
    }

    fn update_connection(&self, id: &str, change: impl FnOnce(&mut ConnectionStatus)) {
        self.update(|status| {
            if let Some(connection) = status.connections.iter_mut().find(|c| c.id == id) {
                change(connection);
            }
        });
    }

    /// Insert or replace the entry of one connection.
    fn upsert_connection(&self, entry: ConnectionStatus) {
        self.update(
            |status| match status.connections.iter_mut().find(|c| c.id == entry.id) {
                Some(existing) => *existing = entry,
                None => status.connections.push(entry),
            },
        );
    }
}

// -----------------------------------------------------------------------------
// Slot image
// -----------------------------------------------------------------------------

/// What the scanner will write to one slot on the next publish.
#[derive(Clone, Copy)]
struct SlotImage {
    slot_id: usize,
    /// `0` until a value has been read: the slot then has no type and its value means nothing.
    type_code: u16,
    value: TagValueRaw,
    quality: QualityCode,
    /// When the value was read from the device. `0` until one has been.
    timestamp_ns: u64,
}

impl SlotImage {
    fn unread(slot_id: usize) -> Self {
        Self {
            slot_id,
            type_code: 0,
            value: TagValueRaw::from_u64(0),
            quality: QualityCode::BAD_NOT_CONNECTED,
            timestamp_ns: 0,
        }
    }

    fn apply(&mut self, reading: &Result<SampleValue, SampleFault>, at_ns: u64) {
        match reading {
            Ok(value) => {
                let (data_type, raw) = encode(*value);
                self.type_code = data_type.code();
                self.value = raw;
                self.quality = QualityCode::GOOD;
                self.timestamp_ns = at_ns;
            }
            // The last value read from the device, its type and its timestamp stay as they
            // are. Only the quality changes: a reader sees a stale value marked bad, with the
            // time it was last true, and never a number made up for the gap.
            Err(fault) => self.quality = quality_for(fault),
        }
    }
}

fn encode(value: SampleValue) -> (TagDataType, TagValueRaw) {
    match value {
        SampleValue::Bool(v) => (TagDataType::Bool, TagValueRaw::from_bool(v)),
        SampleValue::U8(v) => (TagDataType::Byte, TagValueRaw::from_u8(v)),
        SampleValue::I16(v) => (TagDataType::Int16, TagValueRaw::from_i16(v)),
        SampleValue::U16(v) => (TagDataType::Uint16, TagValueRaw::from_u16(v)),
        SampleValue::I32(v) => (TagDataType::Int32, TagValueRaw::from_i32(v)),
        SampleValue::U32(v) => (TagDataType::Uint32, TagValueRaw::from_u32(v)),
        SampleValue::I64(v) => (TagDataType::Int64, TagValueRaw::from_i64(v)),
        SampleValue::U64(v) => (TagDataType::Uint64, TagValueRaw::from_u64(v)),
        SampleValue::F32(v) => (TagDataType::Float32, TagValueRaw::from_f32(v)),
        SampleValue::F64(v) => (TagDataType::Float64, TagValueRaw::from_f64(v)),
    }
}

fn quality_for(fault: &SampleFault) -> QualityCode {
    match fault {
        SampleFault::CommFailure(_) => QualityCode::BAD_COMM_FAILURE,
        SampleFault::DeviceFailure(_) => QualityCode::BAD_DEVICE_FAILURE,
        SampleFault::ConfigError(_) => QualityCode::BAD_CONFIG_ERROR,
    }
}

fn now_unix_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Write every slot of the image and publish.
///
/// The whole image is written each time because the triple buffer hands the writer a bank that
/// is two publishes old: a slot left untouched would roll back to a stale value.
fn flush<S: PlatformShm>(writer: &mut ShmWriterHandle<S>, image: &BTreeMap<usize, SlotImage>) {
    for slot in image.values() {
        if let Err(e) = writer.write_tag(
            slot.slot_id,
            slot.type_code,
            slot.quality,
            slot.value,
            slot.timestamp_ns,
            0,
        ) {
            error!("tag bus write failed for slot {}: {}", slot.slot_id, e);
        }
    }
    writer.publish();
}

// -----------------------------------------------------------------------------
// Scanner
// -----------------------------------------------------------------------------

/// What the scanner is asked to do. It is the only task that writes the tag bus.
enum ScanMsg {
    /// One poll cycle's outcome: slot and reading.
    Batch {
        at_ns: u64,
        updates: Vec<(usize, Result<SampleValue, SampleFault>)>,
    },
    /// Slots about to be given to new tags: publish them as never read before anyone can see
    /// the tags. A slot that held an older tag's value must not show it under a new name.
    Attach {
        slots: Vec<usize>,
        done: oneshot::Sender<()>,
    },
    /// Slots whose tags are going away: publish them as not connected.
    Detach {
        slots: Vec<usize>,
        done: oneshot::Sender<()>,
    },
}

async fn run_scanner<S: PlatformShm>(
    mut writer: ShmWriterHandle<S>,
    mut rx: mpsc::Receiver<ScanMsg>,
) {
    // Every slot this runtime has ever written. A detached slot stays here, not connected,
    // and is written on every publish: the triple buffer would otherwise bring back whatever
    // an older bank held for it.
    let mut image: BTreeMap<usize, SlotImage> = BTreeMap::new();

    while let Some(message) = rx.recv().await {
        match message {
            ScanMsg::Batch { at_ns, updates } => {
                for (slot, reading) in &updates {
                    if let Some(image) = image.get_mut(slot) {
                        image.apply(reading, at_ns);
                    }
                }
                flush(&mut writer, &image);
            }
            ScanMsg::Attach { slots, done } => {
                for slot in slots {
                    image.insert(slot, SlotImage::unread(slot));
                }
                flush(&mut writer, &image);
                let _ = done.send(());
            }
            ScanMsg::Detach { slots, done } => {
                for slot in slots {
                    if let Some(image) = image.get_mut(&slot) {
                        image.quality = QualityCode::BAD_NOT_CONNECTED;
                    }
                }
                flush(&mut writer, &image);
                let _ = done.send(());
            }
        }
    }

    // Nothing will refresh these slots again. Leaving them GOOD would present the last reading
    // as current for as long as the segment exists.
    for slot in image.values_mut() {
        slot.quality = QualityCode::BAD_NOT_CONNECTED;
    }
    flush(&mut writer, &image);
}

// -----------------------------------------------------------------------------
// Runtime
// -----------------------------------------------------------------------------

/// A driver that has been started with a connection's configuration and is ready to poll.
struct Prepared {
    driver: Box<dyn ProtocolDriver>,
    ctx: PluginContext,
    driver_config: serde_json::Value,
    poll_interval: Duration,
    cycle_timeout: Duration,
    declared: Vec<TagDeclaration>,
}

/// A replacement driver for a running connection whose tags stay the same.
struct Swap {
    prepared: Prepared,
    done: oneshot::Sender<()>,
}

struct Running {
    config: ConnectionConfig,
    /// Which build of which driver runs it. See [`DriverRegistry::identity`].
    identity: String,
    /// Tag URI and slot, in declaration order.
    tags: Vec<(String, usize)>,
    swap: mpsc::Sender<Swap>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

/// What [`IoRuntime::apply`] did to each connection.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ApplyReport {
    /// Unchanged and left running.
    pub kept: Vec<String>,
    /// Same tags, new configuration or driver: replaced in place without a gap.
    pub swapped: Vec<String>,
    /// Different tags: stopped and started again.
    pub restarted: Vec<String>,
    /// New connections that started.
    pub started: Vec<String>,
    /// Connections that are no longer configured.
    pub stopped: Vec<String>,
    /// Connections that could not be started or replaced, and why. A connection listed here
    /// that was running before keeps running as it was.
    pub failed: Vec<(String, String)>,
}

/// The running field I/O of one server.
pub struct IoRuntime {
    status: IoStatusHandle,
    catalog: Arc<RwLock<TagCatalog>>,
    capacity: usize,
    scanner: Option<mpsc::Sender<ScanMsg>>,
    scanner_task: Option<JoinHandle<()>>,
    connections: HashMap<String, Running>,
}

impl IoRuntime {
    /// Start every connection of `config` and begin writing to the tag bus.
    ///
    /// A connection that cannot be started is reported in the status with the reason and
    /// contributes no tags; the others still run. Tags are registered in `catalog` only after
    /// their slots have been published as never read, so a reader can never see a slot of this
    /// runtime holding data left over from an earlier process or tag.
    pub async fn start<S: PlatformShm + 'static>(
        config: IoConfig,
        source: Option<String>,
        registry: &DriverRegistry,
        catalog: Arc<RwLock<TagCatalog>>,
        writer: ShmWriterHandle<S>,
        status: IoStatusHandle,
    ) -> Self {
        Self::start_with_report(config, source, registry, catalog, writer, status)
            .await
            .0
    }

    /// [`Self::start`], also saying which connections started and which could not.
    pub async fn start_with_report<S: PlatformShm + 'static>(
        config: IoConfig,
        source: Option<String>,
        registry: &DriverRegistry,
        catalog: Arc<RwLock<TagCatalog>>,
        writer: ShmWriterHandle<S>,
        status: IoStatusHandle,
    ) -> (Self, ApplyReport) {
        let capacity = writer.max_tags();
        let (tx, rx) = mpsc::channel::<ScanMsg>(256);
        let scanner_task = tokio::spawn(run_scanner(writer, rx));
        status.update(|s| *s = IoStatus::default());
        let mut runtime = Self {
            status,
            catalog,
            capacity,
            scanner: Some(tx),
            scanner_task: Some(scanner_task),
            connections: HashMap::new(),
        };
        let report = runtime.apply(config, source, registry).await;
        (runtime, report)
    }

    pub fn status(&self) -> IoStatusHandle {
        self.status.clone()
    }

    /// The configuration of the connections that are running, in no particular order.
    pub fn running_configs(&self) -> Vec<ConnectionConfig> {
        self.connections
            .values()
            .map(|r| r.config.clone())
            .collect()
    }

    /// Make the running connections match `config`, changing only what differs.
    pub async fn apply(
        &mut self,
        config: IoConfig,
        source: Option<String>,
        registry: &DriverRegistry,
    ) -> ApplyReport {
        let mut report = ApplyReport::default();
        let wanted: HashSet<&str> = config.connections.iter().map(|c| c.id.as_str()).collect();

        // Connections that are gone go first, so their tag names are free for the others.
        let mut gone: Vec<String> = self
            .connections
            .keys()
            .filter(|id| !wanted.contains(id.as_str()))
            .cloned()
            .collect();
        // Runtimes before field connections: a runtime that stops writes its outputs' safe
        // values, and needs the connection behind them to still be there.
        gone.sort_by_key(|id| (!crate::runtimes::is_runtime_connection(id), id.clone()));
        for id in gone {
            self.remove(&id).await;
            report.stopped.push(id);
        }

        // Connections whose tags change are stopped before any new one is started, for the same
        // reason; their replacements are started below with the new ones.
        let mut pending: Vec<(ConnectionConfig, Prepared, String)> = Vec::new();
        let mut not_started: Vec<(ConnectionConfig, String)> = Vec::new();

        for connection in &config.connections {
            // A driver switched off in Settings → Modules stops its connections and says why.
            // (A module file that is merely missing for a moment, while it is being replaced,
            // leaves the running connection alone: see `prepare` failing below.)
            if let Some(module) = registry.disabled_driver(&connection.driver) {
                if self.connections.contains_key(&connection.id) {
                    self.remove(&connection.id).await;
                }
                not_started.push((
                    connection.clone(),
                    format!(
                        "driver '{}' is provided by module '{}', which is disabled in Settings → Modules",
                        connection.driver, module.manifest.id
                    ),
                ));
                continue;
            }
            let identity = registry.identity(&connection.driver);
            if let Some(running) = self.connections.get(&connection.id) {
                let unchanged =
                    running.config == *connection && Some(&running.identity) == identity.as_ref();
                if unchanged {
                    report.kept.push(connection.id.clone());
                    continue;
                }
                let prepared = match prepare(connection, registry).await {
                    Ok(prepared) => prepared,
                    Err(reason) => {
                        error!(
                            "I/O connection '{}' not replaced, the running one is kept: {}",
                            connection.id, reason
                        );
                        self.status.update_connection(&connection.id, |s| {
                            s.detail = Some(format!(
                                "the new configuration could not be started, so the previous \
                                 one is still running: {reason}"
                            ));
                        });
                        report.failed.push((connection.id.clone(), reason));
                        continue;
                    }
                };
                let same_tags = running
                    .tags
                    .iter()
                    .map(|(uri, _)| uri.as_str())
                    .eq(prepared.declared.iter().map(|d| d.uri.as_str()));
                let identity = identity.unwrap_or_default();
                if same_tags {
                    self.swap(connection, prepared, identity).await;
                    report.swapped.push(connection.id.clone());
                } else {
                    self.remove(&connection.id).await;
                    pending.push((connection.clone(), prepared, identity));
                    report.restarted.push(connection.id.clone());
                }
            } else {
                match prepare(connection, registry).await {
                    Ok(prepared) => {
                        pending.push((connection.clone(), prepared, identity.unwrap_or_default()));
                        report.started.push(connection.id.clone());
                    }
                    Err(reason) => not_started.push((connection.clone(), reason)),
                }
            }
        }

        for (connection, prepared, identity) in pending {
            if let Err(reason) = self.add(&connection, prepared, identity).await {
                report.started.retain(|id| id != &connection.id);
                report.restarted.retain(|id| id != &connection.id);
                not_started.push((connection, reason));
            }
        }

        for (connection, reason) in not_started {
            error!("I/O connection '{}' not started: {}", connection.id, reason);
            self.status.upsert_connection(ConnectionStatus {
                state: ConnectionState::ConfigError,
                detail: Some(reason.clone()),
                ..new_status(&connection)
            });
            report.failed.push((connection.id.clone(), reason));
        }

        // The status lists the configured connections, in the configuration's order.
        let order: Vec<String> = config.connections.iter().map(|c| c.id.clone()).collect();
        self.status.update(|s| {
            s.source = source;
            s.note = if order.is_empty() {
                Some("the I/O configuration has no connections".to_string())
            } else {
                None
            };
            let mut by_id: HashMap<String, ConnectionStatus> =
                s.connections.drain(..).map(|c| (c.id.clone(), c)).collect();
            s.connections = order.iter().filter_map(|id| by_id.remove(id)).collect();
        });

        report
    }

    /// Start a prepared connection: claim slots, publish them unread, register the tags, poll.
    async fn add(
        &mut self,
        connection: &ConnectionConfig,
        mut prepared: Prepared,
        identity: String,
    ) -> Result<(), String> {
        let Some(scanner) = self.scanner.clone() else {
            let _ = prepared.driver.stop(&prepared.ctx).await;
            return Err("the I/O runtime has been shut down".to_string());
        };

        let mut catalog = self.catalog.write().await;

        // Stage every tag. Nothing is committed until all of them are valid, so a rejected
        // connection leaves no partial set of tags behind.
        let mut metas: Vec<TagMetadata> = Vec::new();
        let mut slots: Vec<usize> = Vec::new();
        let mut taken: HashSet<u32> = HashSet::new();
        let mut probe: u32 = 0;
        let mut rejection: Option<String> = None;
        for declaration in &prepared.declared {
            if catalog.get_by_uri(&declaration.uri).is_some() {
                rejection = Some(format!(
                    "tag '{}' is already provided by another source",
                    declaration.uri
                ));
                break;
            }
            while catalog.get_by_slot(probe).is_some() || taken.contains(&probe) {
                probe += 1;
            }
            if probe as usize >= self.capacity {
                rejection = Some(format!(
                    "the tag bus is full ({} slots): no slot for tag '{}'",
                    self.capacity, declaration.uri
                ));
                break;
            }
            match TagMetadata::new(declaration.uri.clone(), probe) {
                Ok(meta) => {
                    metas.push(
                        meta.with_description(declaration.description.clone())
                            .with_access(if declaration.writable {
                                AccessMode::ReadWrite
                            } else {
                                AccessMode::ReadOnly
                            }),
                    );
                    slots.push(probe as usize);
                    taken.insert(probe);
                    probe += 1;
                }
                Err(e) => {
                    rejection = Some(format!("tag '{}' is not valid: {}", declaration.uri, e));
                    break;
                }
            }
        }

        if let Some(reason) = rejection {
            drop(catalog);
            let _ = prepared.driver.stop(&prepared.ctx).await;
            return Err(reason);
        }

        let (done, attached) = oneshot::channel();
        if scanner
            .send(ScanMsg::Attach {
                slots: slots.clone(),
                done,
            })
            .await
            .is_err()
            || attached.await.is_err()
        {
            drop(catalog);
            let _ = prepared.driver.stop(&prepared.ctx).await;
            return Err("the tag bus scanner has stopped".to_string());
        }
        for meta in metas {
            let uri = meta.uri.clone();
            if let Err(e) = catalog.register(meta) {
                error!("tag '{}' could not be registered: {}", uri, e);
            }
        }
        drop(catalog);

        let tags: Vec<(String, usize)> = prepared
            .declared
            .iter()
            .map(|d| d.uri.clone())
            .zip(slots)
            .collect();

        self.status.upsert_connection(ConnectionStatus {
            state: ConnectionState::Running,
            tags: tags.iter().map(|(uri, _)| uri.clone()).collect(),
            ..new_status(connection)
        });

        let (shutdown, shutdown_rx) = watch::channel(false);
        let (swap, swap_rx) = mpsc::channel::<Swap>(1);
        let (writes, writes_rx) = mpsc::channel::<WriteRequest>(16);
        let targets: Vec<(String, bool)> = prepared
            .declared
            .iter()
            .map(|d| (d.uri.clone(), d.writable))
            .collect();
        info!(
            "I/O connection '{}' started with {} tag(s)",
            connection.id,
            tags.len()
        );
        let task = tokio::spawn(run_connection(
            connection.id.clone(),
            prepared,
            tags.clone(),
            scanner,
            Inbox {
                shutdown: shutdown_rx,
                swaps: swap_rx,
                writes: writes_rx,
            },
            self.status.clone(),
        ));
        // Only now can a write find the connection: its task exists and its tags are registered.
        self.status.set_write_targets(&targets, &writes);

        self.connections.insert(
            connection.id.clone(),
            Running {
                config: connection.clone(),
                identity,
                tags,
                swap,
                shutdown,
                task,
            },
        );
        Ok(())
    }

    /// Hand a running connection a new driver for the same tags. The new driver is already
    /// started; the connection's task stops the old one between two poll cycles.
    async fn swap(&mut self, connection: &ConnectionConfig, prepared: Prepared, identity: String) {
        let Some(running) = self.connections.get_mut(&connection.id) else {
            return;
        };
        let (done, swapped) = oneshot::channel();
        let sent = running.swap.send(Swap { prepared, done }).await;
        if sent.is_ok() && swapped.await.is_ok() {
            running.config = connection.clone();
            running.identity = identity;
            let interval_ms = connection.poll_interval_ms;
            let timeout_ms = connection.effective_cycle_timeout_ms();
            self.status.update_connection(&connection.id, |s| {
                s.driver = connection.driver.clone();
                s.poll_interval_ms = interval_ms;
                s.cycle_timeout_ms = timeout_ms;
                s.detail = None;
            });
            info!("I/O connection '{}' swapped to a new driver", connection.id);
        } else if let Err(mpsc::error::SendError(swap)) = sent {
            // The task has already ended; the new driver was never used.
            let mut prepared = swap.prepared;
            let _ = prepared.driver.stop(&prepared.ctx).await;
        }
    }

    /// Stop one connection, publish its slots as not connected, and remove its tags.
    async fn remove(&mut self, id: &str) {
        let Some(running) = self.connections.remove(id) else {
            return;
        };
        // First: no new write may find a connection that is about to stop.
        self.status
            .drop_write_targets(running.tags.iter().map(|(uri, _)| uri));
        let _ = running.shutdown.send(true);
        let _ = running.task.await;

        let slots: Vec<usize> = running.tags.iter().map(|(_, slot)| *slot).collect();
        if let Some(scanner) = &self.scanner {
            let (done, detached) = oneshot::channel();
            if scanner.send(ScanMsg::Detach { slots, done }).await.is_ok() {
                let _ = detached.await;
            }
        }
        // Unregistered only now: there is no moment at which a registered tag reads as GOOD
        // with nothing feeding it.
        let mut catalog = self.catalog.write().await;
        for (uri, _) in &running.tags {
            catalog.unregister(uri);
        }
        drop(catalog);
        self.status.update(|s| s.connections.retain(|c| c.id != id));
        info!("I/O connection '{}' stopped", id);
    }

    /// Stop every driver, mark every tag "not connected", remove the tags, and wait for the
    /// tasks to end.
    pub async fn shutdown(mut self) {
        let mut tags: Vec<String> = Vec::new();
        // Runtimes first, for their safe values (see `apply`).
        let mut all: Vec<(String, Running)> = self.connections.drain().collect();
        all.sort_by_key(|(id, _)| (!crate::runtimes::is_runtime_connection(id), id.clone()));
        for (id, running) in all {
            self.status
                .drop_write_targets(running.tags.iter().map(|(uri, _)| uri));
            let _ = running.shutdown.send(true);
            let _ = running.task.await;
            tags.extend(running.tags.into_iter().map(|(uri, _)| uri));
            let detail = Some("shut down".to_string());
            self.status.update_connection(&id, |s| {
                s.state = ConnectionState::Stopped;
                s.detail = detail;
            });
        }
        // The scanner marks every slot not connected when its last sender is gone.
        self.scanner.take();
        if let Some(task) = self.scanner_task.take() {
            let _ = task.await;
        }
        let mut catalog = self.catalog.write().await;
        for uri in &tags {
            catalog.unregister(uri);
        }
    }
}

fn new_status(connection: &ConnectionConfig) -> ConnectionStatus {
    ConnectionStatus {
        id: connection.id.clone(),
        driver: connection.driver.clone(),
        state: ConnectionState::Running,
        detail: None,
        tags: Vec::new(),
        poll_interval_ms: connection.poll_interval_ms,
        cycle_timeout_ms: connection.effective_cycle_timeout_ms(),
        cycles_completed: 0,
        last_cycle_unix_ms: None,
        last_cycle_error: None,
        tags_good: None,
        faults: Vec::new(),
    }
}

/// Create, initialise and start a driver for `connection`, and read the tags it declares.
async fn prepare(
    connection: &ConnectionConfig,
    registry: &DriverRegistry,
) -> Result<Prepared, String> {
    let mut driver = registry.create(&connection.driver).ok_or_else(|| {
        format!(
            "unknown driver '{}' (available: {})",
            connection.driver,
            registry.names().join(", ")
        )
    })?;

    let mut capabilities = HashSet::new();
    capabilities.insert(DRIVER_CAPABILITY.to_string());
    let ctx = PluginContext::new(format!("io.{}", connection.id), capabilities, String::new());

    if let Err(e) = driver.init(&ctx).await {
        let _ = driver.stop(&ctx).await;
        return Err(e.to_string());
    }
    if let Err(e) = driver.start(&ctx, &connection.config).await {
        let _ = driver.stop(&ctx).await;
        return Err(e.to_string());
    }

    let declared = driver.declared_tags();
    let mut seen = HashSet::new();
    let problem = if declared.is_empty() {
        Some("the connection declares no tags".to_string())
    } else {
        declared
            .iter()
            .find(|d| !seen.insert(d.uri.as_str()))
            .map(|d| format!("tag '{}' is declared twice", d.uri))
    };
    if let Some(problem) = problem {
        let _ = driver.stop(&ctx).await;
        return Err(problem);
    }

    Ok(Prepared {
        driver,
        ctx,
        driver_config: connection.config.clone(),
        poll_interval: Duration::from_millis(connection.poll_interval_ms),
        cycle_timeout: Duration::from_millis(connection.effective_cycle_timeout_ms()),
        declared,
    })
}

fn now_unix_ms() -> u64 {
    now_unix_ns() / 1_000_000
}

fn ticker_for(period: Duration) -> tokio::time::Interval {
    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticker
}

/// What a connection's task is told from outside, between two poll cycles.
struct Inbox {
    shutdown: watch::Receiver<bool>,
    swaps: mpsc::Receiver<Swap>,
    writes: mpsc::Receiver<WriteRequest>,
}

async fn run_connection(
    id: String,
    mut current: Prepared,
    tags: Vec<(String, usize)>,
    scanner: mpsc::Sender<ScanMsg>,
    inbox: Inbox,
    status: IoStatusHandle,
) {
    let Inbox {
        mut shutdown,
        mut swaps,
        mut writes,
    } = inbox;
    let by_uri: HashMap<&str, usize> = tags
        .iter()
        .map(|(uri, slot)| (uri.as_str(), *slot))
        .collect();

    let mut ticker = ticker_for(current.poll_interval);
    let mut stop_reason: Option<String> = None;

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            Some(swap) = swaps.recv() => {
                let mut old = std::mem::replace(&mut current, swap.prepared);
                let _ = old.driver.stop(&old.ctx).await;
                if old.poll_interval != current.poll_interval {
                    ticker = ticker_for(current.poll_interval);
                }
                let _ = swap.done.send(());
                continue;
            }
            Some(request) = writes.recv() => {
                // Between two poll cycles, never during one: the driver has one conversation
                // with its device. Bounded like a poll, and for the same reason.
                let outcome = timeout(
                    current.cycle_timeout,
                    current.driver.write_tag(&current.ctx, &request.uri, &request.value),
                )
                .await;
                let (answer, timed_out) = match outcome {
                    Ok(Ok(())) => (Ok(()), false),
                    Ok(Err(e)) => (Err(WriteError::from_driver(e)), false),
                    Err(_) => (
                        Err(WriteError::NotConfirmed(format!(
                            "no answer within {} ms",
                            current.cycle_timeout.as_millis()
                        ))),
                        true,
                    ),
                };
                match &answer {
                    Ok(()) => info!("I/O connection '{}': wrote '{}'", id, request.uri),
                    Err(e) => warn!("I/O connection '{}': write to '{}': {}", id, request.uri, e),
                }
                let _ = request.done.send(answer);
                if timed_out {
                    // As after a poll that overran: the request was cancelled mid-flight, so a
                    // late answer must not be read as the reply to the next request.
                    let _ = current.driver.stop(&current.ctx).await;
                    if let Err(e) = current
                        .driver
                        .start(&current.ctx, &current.driver_config)
                        .await
                    {
                        stop_reason = Some(format!("driver could not be restarted: {e}"));
                        break;
                    }
                }
                continue;
            }
            _ = ticker.tick() => {}
        }

        let outcome = timeout(
            current.cycle_timeout,
            current.driver.poll_cycle(&current.ctx),
        )
        .await;

        let mut cycle_error: Option<String> = None;
        let mut overran = false;
        let samples: Vec<TagSample> = match outcome {
            Ok(Ok(samples)) => samples,
            Ok(Err(e)) => {
                cycle_error = Some(e.to_string());
                Vec::new()
            }
            Err(_) => {
                overran = true;
                cycle_error = Some(format!(
                    "poll cycle exceeded its {} ms deadline",
                    current.cycle_timeout.as_millis()
                ));
                Vec::new()
            }
        };

        // One outcome per declared tag. A tag the driver did not report on is a fault, never
        // "unchanged": silence would leave its last value standing as GOOD.
        let mut readings: HashMap<usize, Result<SampleValue, SampleFault>> = HashMap::new();
        for sample in samples {
            match by_uri.get(sample.uri.as_str()) {
                Some(slot) => {
                    readings.insert(*slot, sample.reading);
                }
                None => warn!(
                    "I/O connection '{}': driver reported undeclared tag '{}'",
                    id, sample.uri
                ),
            }
        }

        let mut updates = Vec::with_capacity(tags.len());
        let mut faults = Vec::new();
        let mut good = 0usize;
        for (uri, slot) in &tags {
            let reading = readings.remove(slot).unwrap_or_else(|| {
                Err(SampleFault::CommFailure(match &cycle_error {
                    Some(reason) => reason.clone(),
                    None => "the driver returned no sample for this tag".to_string(),
                }))
            });
            match &reading {
                Ok(_) => good += 1,
                Err(fault) => faults.push(TagFaultStatus {
                    uri: uri.clone(),
                    reason: fault.to_string(),
                }),
            }
            updates.push((*slot, reading));
        }

        if scanner
            .send(ScanMsg::Batch {
                at_ns: now_unix_ns(),
                updates,
            })
            .await
            .is_err()
        {
            stop_reason = Some("the tag bus scanner has stopped".to_string());
            break;
        }

        status.update_connection(&id, |s| {
            s.cycles_completed += 1;
            s.last_cycle_unix_ms = Some(now_unix_ms());
            s.last_cycle_error = cycle_error.clone();
            s.tags_good = Some(good);
            s.faults = faults;
        });

        if overran {
            // The cycle was cancelled mid-request, so the driver's connection is in an unknown
            // state. Restart it rather than read a late answer as the reply to a new request.
            let _ = current.driver.stop(&current.ctx).await;
            if let Err(e) = current
                .driver
                .start(&current.ctx, &current.driver_config)
                .await
            {
                stop_reason = Some(format!("driver could not be restarted: {e}"));
                break;
            }
        }
    }

    let _ = current.driver.stop(&current.ctx).await;

    // If the task ends for any reason other than an orderly shutdown, its tags must not stay
    // as they were: mark them, then report the connection stopped.
    if let Some(reason) = &stop_reason {
        error!("I/O connection '{}' stopped: {}", id, reason);
        let updates = tags
            .iter()
            .map(|(_, slot)| (*slot, Err(SampleFault::CommFailure(reason.clone()))))
            .collect();
        let _ = scanner
            .send(ScanMsg::Batch {
                at_ns: now_unix_ns(),
                updates,
            })
            .await;
        status.update_connection(&id, |s| {
            s.state = ConnectionState::Stopped;
            s.detail = Some(reason.clone());
        });
    }
}
