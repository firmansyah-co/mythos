//! The I/O runtime: driver tasks, the scanner that owns the tag bus writer, and their status.

use crate::io::config::{ConnectionConfig, IoConfig};
use crate::tag::{AccessMode, TagCatalog, TagMetadata};
use mythos_pal::PlatformShm;
use mythos_plugin_sdk::{PluginContext, ProtocolDriver, SampleFault, SampleValue, TagSample};
use mythos_shm::{QualityCode, ShmWriterHandle, TagDataType, TagValueRaw};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::{error, info, warn};

/// The capability granted to every protocol driver: it may open network connections.
const DRIVER_CAPABILITY: &str = "network:client";

// -----------------------------------------------------------------------------
// Driver registry
// -----------------------------------------------------------------------------

/// Constructs a fresh, unconfigured driver instance.
pub type DriverFactory = fn() -> Box<dyn ProtocolDriver>;

/// The drivers a server can instantiate, by the name used in `io.json`.
///
/// Drivers are compiled in. There is no dynamic loading.
pub struct DriverRegistry {
    factories: HashMap<String, DriverFactory>,
}

impl DriverRegistry {
    /// A registry with no drivers.
    pub fn empty() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// The drivers that ship with the server.
    pub fn builtin() -> Self {
        let mut registry = Self::empty();
        registry.register("modbus", || Box::new(driver_modbus::ModbusDriver::new()));
        registry
    }

    pub fn register(&mut self, name: impl Into<String>, factory: DriverFactory) {
        self.factories.insert(name.into(), factory);
    }

    pub fn create(&self, name: &str) -> Option<Box<dyn ProtocolDriver>> {
        self.factories.get(name).map(|factory| factory())
    }

    /// Registered driver names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.factories.keys().cloned().collect();
        names.sort();
        names
    }
}

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

/// Shared, cheaply cloned view of the runtime's status.
#[derive(Clone, Default)]
pub struct IoStatusHandle(Arc<StdRwLock<IoStatus>>);

impl IoStatusHandle {
    /// A status for a server on which no I/O runtime is running, stating why.
    pub fn not_running(note: impl Into<String>) -> Self {
        let handle = Self::default();
        handle.update(|status| status.note = Some(note.into()));
        handle
    }

    pub fn snapshot(&self) -> IoStatus {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, change: impl FnOnce(&mut IoStatus)) {
        change(&mut self.0.write().unwrap_or_else(|e| e.into_inner()));
    }

    fn update_connection(&self, index: usize, change: impl FnOnce(&mut ConnectionStatus)) {
        self.update(|status| {
            if let Some(connection) = status.connections.get_mut(index) {
                change(connection);
            }
        });
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
fn flush<S: PlatformShm>(writer: &mut ShmWriterHandle<S>, image: &[SlotImage]) {
    for slot in image {
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
// Runtime
// -----------------------------------------------------------------------------

/// One poll cycle's outcome for one connection: image index and reading.
struct Batch {
    at_ns: u64,
    updates: Vec<(usize, Result<SampleValue, SampleFault>)>,
}

/// A connection that started and is ready to be polled.
struct Prepared {
    index: usize,
    id: String,
    driver: Box<dyn ProtocolDriver>,
    ctx: PluginContext,
    driver_config: serde_json::Value,
    poll_interval: Duration,
    cycle_timeout: Duration,
    /// Declared tag URI and its index in the scanner's image, in declaration order.
    tags: Vec<(String, usize)>,
}

/// The running field I/O of one server.
pub struct IoRuntime {
    status: IoStatusHandle,
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl IoRuntime {
    /// Start every connection of `config` and begin writing to the tag bus.
    ///
    /// A connection that cannot be started is reported in the status with the reason and
    /// contributes no tags; the others still run. Tags are registered in `catalog` only after
    /// their slots have been published as "not connected", so a reader can never see a slot of
    /// this runtime holding data left over from an earlier process.
    pub async fn start<S: PlatformShm + 'static>(
        config: IoConfig,
        source: Option<String>,
        registry: &DriverRegistry,
        catalog: Arc<RwLock<TagCatalog>>,
        mut writer: ShmWriterHandle<S>,
        status: IoStatusHandle,
    ) -> Self {
        let capacity = writer.max_tags();
        let mut image: Vec<SlotImage> = Vec::new();
        let mut staged: Vec<TagMetadata> = Vec::new();
        let mut staged_uris: HashSet<String> = HashSet::new();
        let mut staged_slots: HashSet<u32> = HashSet::new();
        let mut next_slot: u32 = 0;
        let mut prepared: Vec<Prepared> = Vec::new();
        let mut statuses: Vec<ConnectionStatus> = Vec::new();

        let mut catalog_guard = catalog.write().await;

        for (index, connection) in config.connections.iter().enumerate() {
            let mut connection_status = ConnectionStatus {
                id: connection.id.clone(),
                driver: connection.driver.clone(),
                state: ConnectionState::ConfigError,
                detail: None,
                tags: Vec::new(),
                poll_interval_ms: connection.poll_interval_ms,
                cycle_timeout_ms: connection.effective_cycle_timeout_ms(),
                cycles_completed: 0,
                last_cycle_unix_ms: None,
                last_cycle_error: None,
                tags_good: None,
                faults: Vec::new(),
            };

            let (driver, ctx) = match start_driver(connection, registry).await {
                Ok(started) => started,
                Err(reason) => {
                    error!("I/O connection '{}' not started: {}", connection.id, reason);
                    connection_status.detail = Some(reason);
                    statuses.push(connection_status);
                    continue;
                }
            };

            // Stage this connection's tags. Nothing is committed until all of them are valid,
            // so a rejected connection leaves no partial set of tags behind.
            let mut local_meta: Vec<TagMetadata> = Vec::new();
            let mut local_uris: HashSet<String> = HashSet::new();
            let mut local_slots: Vec<u32> = Vec::new();
            let mut probe_slot = next_slot;
            let mut rejection: Option<String> = None;

            for declaration in driver.declared_tags() {
                if catalog_guard.get_by_uri(&declaration.uri).is_some()
                    || staged_uris.contains(&declaration.uri)
                    || !local_uris.insert(declaration.uri.clone())
                {
                    rejection = Some(format!(
                        "tag '{}' is already provided by another source",
                        declaration.uri
                    ));
                    break;
                }

                while catalog_guard.get_by_slot(probe_slot).is_some()
                    || staged_slots.contains(&probe_slot)
                {
                    probe_slot += 1;
                }
                if probe_slot as usize >= capacity {
                    rejection = Some(format!(
                        "the tag bus is full ({} slots): no slot for tag '{}'",
                        capacity, declaration.uri
                    ));
                    break;
                }

                match TagMetadata::new(declaration.uri.clone(), probe_slot) {
                    Ok(meta) => {
                        local_meta.push(
                            meta.with_description(declaration.description)
                                .with_access(AccessMode::ReadOnly),
                        );
                        local_slots.push(probe_slot);
                        probe_slot += 1;
                    }
                    Err(e) => {
                        rejection = Some(format!("tag '{}' is not valid: {}", declaration.uri, e));
                        break;
                    }
                }
            }

            if rejection.is_none() && local_meta.is_empty() {
                rejection = Some("the connection declares no tags".to_string());
            }

            if let Some(reason) = rejection {
                error!("I/O connection '{}' not started: {}", connection.id, reason);
                let mut driver = driver;
                let _ = driver.stop(&ctx).await;
                connection_status.detail = Some(reason);
                statuses.push(connection_status);
                continue;
            }

            let mut tags = Vec::with_capacity(local_meta.len());
            for (meta, slot) in local_meta.into_iter().zip(local_slots) {
                tags.push((meta.uri.clone(), image.len()));
                image.push(SlotImage::unread(slot as usize));
                staged_uris.insert(meta.uri.clone());
                staged_slots.insert(slot);
                staged.push(meta);
            }
            next_slot = probe_slot;

            connection_status.state = ConnectionState::Running;
            connection_status.tags = tags.iter().map(|(uri, _)| uri.clone()).collect();
            statuses.push(connection_status);

            prepared.push(Prepared {
                index,
                id: connection.id.clone(),
                driver,
                ctx,
                driver_config: connection.config.clone(),
                poll_interval: Duration::from_millis(connection.poll_interval_ms),
                cycle_timeout: Duration::from_millis(connection.effective_cycle_timeout_ms()),
                tags,
            });
        }

        // Publish "not connected" for every slot before any of them becomes visible.
        flush(&mut writer, &image);
        for meta in staged {
            let uri = meta.uri.clone();
            if let Err(e) = catalog_guard.register(meta) {
                error!("tag '{}' could not be registered: {}", uri, e);
            }
        }
        drop(catalog_guard);

        status.update(|s| {
            s.source = source;
            s.note = if statuses.is_empty() {
                Some("the I/O configuration has no connections".to_string())
            } else {
                None
            };
            s.connections = statuses;
        });

        let (shutdown, shutdown_rx) = watch::channel(false);
        let (tx, rx) = mpsc::channel::<Batch>(64);
        let mut tasks = Vec::with_capacity(prepared.len() + 1);

        tasks.push(tokio::spawn(run_scanner(writer, image, rx)));
        for connection in prepared {
            info!(
                "I/O connection '{}' started with {} tag(s)",
                connection.id,
                connection.tags.len()
            );
            tasks.push(tokio::spawn(run_connection(
                connection,
                tx.clone(),
                shutdown_rx.clone(),
                status.clone(),
            )));
        }
        // The scanner ends when the last driver task drops its sender.
        drop(tx);

        Self {
            status,
            shutdown,
            tasks,
        }
    }

    pub fn status(&self) -> IoStatusHandle {
        self.status.clone()
    }

    /// Stop every driver, mark every tag "not connected", and wait for the tasks to end.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

async fn start_driver(
    connection: &ConnectionConfig,
    registry: &DriverRegistry,
) -> Result<(Box<dyn ProtocolDriver>, PluginContext), String> {
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

    driver.init(&ctx).await.map_err(|e| e.to_string())?;
    driver
        .start(&ctx, &connection.config)
        .await
        .map_err(|e| e.to_string())?;
    Ok((driver, ctx))
}

/// The only task that writes the tag bus.
async fn run_scanner<S: PlatformShm>(
    mut writer: ShmWriterHandle<S>,
    mut image: Vec<SlotImage>,
    mut rx: mpsc::Receiver<Batch>,
) {
    while let Some(batch) = rx.recv().await {
        for (index, reading) in &batch.updates {
            if let Some(slot) = image.get_mut(*index) {
                slot.apply(reading, batch.at_ns);
            }
        }
        flush(&mut writer, &image);
    }

    // Every driver task has ended, so nothing will refresh these slots again. Leaving them
    // GOOD would present the last reading as current for as long as the segment exists.
    for slot in &mut image {
        slot.quality = QualityCode::BAD_NOT_CONNECTED;
    }
    flush(&mut writer, &image);
}

fn now_unix_ms() -> u64 {
    now_unix_ns() / 1_000_000
}

async fn run_connection(
    mut connection: Prepared,
    tx: mpsc::Sender<Batch>,
    mut shutdown: watch::Receiver<bool>,
    status: IoStatusHandle,
) {
    let by_uri: HashMap<&str, usize> = connection
        .tags
        .iter()
        .map(|(uri, index)| (uri.as_str(), *index))
        .collect();

    let mut ticker = interval(connection.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut stop_reason: Option<String> = None;

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = ticker.tick() => {}
        }

        let outcome = timeout(
            connection.cycle_timeout,
            connection.driver.poll_cycle(&connection.ctx),
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
                    connection.cycle_timeout.as_millis()
                ));
                Vec::new()
            }
        };

        // One outcome per declared tag. A tag the driver did not report on is a fault, never
        // "unchanged": silence would leave its last value standing as GOOD.
        let mut readings: HashMap<usize, Result<SampleValue, SampleFault>> = HashMap::new();
        for sample in samples {
            match by_uri.get(sample.uri.as_str()) {
                Some(index) => {
                    readings.insert(*index, sample.reading);
                }
                None => warn!(
                    "I/O connection '{}': driver reported undeclared tag '{}'",
                    connection.id, sample.uri
                ),
            }
        }

        let mut updates = Vec::with_capacity(connection.tags.len());
        let mut faults = Vec::new();
        let mut good = 0usize;
        for (uri, index) in &connection.tags {
            let reading = readings.remove(index).unwrap_or_else(|| {
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
            updates.push((*index, reading));
        }

        if tx
            .send(Batch {
                at_ns: now_unix_ns(),
                updates,
            })
            .await
            .is_err()
        {
            stop_reason = Some("the tag bus scanner has stopped".to_string());
            break;
        }

        status.update_connection(connection.index, |s| {
            s.cycles_completed += 1;
            s.last_cycle_unix_ms = Some(now_unix_ms());
            s.last_cycle_error = cycle_error.clone();
            s.tags_good = Some(good);
            s.faults = faults;
        });

        if overran {
            // The cycle was cancelled mid-request, so the driver's connection is in an unknown
            // state. Restart it rather than read a late answer as the reply to a new request.
            let _ = connection.driver.stop(&connection.ctx).await;
            if let Err(e) = connection
                .driver
                .start(&connection.ctx, &connection.driver_config)
                .await
            {
                stop_reason = Some(format!("driver could not be restarted: {e}"));
                break;
            }
        }
    }

    let _ = connection.driver.stop(&connection.ctx).await;

    // If the task ends for any reason other than an orderly shutdown, its tags must not stay
    // as they were: mark them, then report the connection stopped.
    if let Some(reason) = &stop_reason {
        error!("I/O connection '{}' stopped: {}", connection.id, reason);
        let updates = connection
            .tags
            .iter()
            .map(|(_, index)| (*index, Err(SampleFault::CommFailure(reason.clone()))))
            .collect();
        let _ = tx
            .send(Batch {
                at_ns: now_unix_ns(),
                updates,
            })
            .await;
    }
    status.update_connection(connection.index, |s| {
        s.state = ConnectionState::Stopped;
        s.detail = Some(stop_reason.unwrap_or_else(|| "shut down".to_string()));
    });
}
