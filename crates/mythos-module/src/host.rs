//! The host side: find module executables, and drive one as a [`ProtocolDriver`].
//!
//! A [`ProcessDriver`] is an ordinary driver to the I/O runtime. Behind it is a child process
//! running the module. If the process dies, the driver reports every tag as a communication
//! failure and starts a new process with the same configuration, no more often than
//! [`RESPAWN_BACKOFF`]. The server itself is never taken down by a module.

use crate::protocol::{
    Call, ModuleKind, ModuleManifest, Request, Response, WireContext, WireDeclaration, WireSample,
    MANIFEST_FLAG, MODULE_FILE_PREFIX, PROTOCOL_VERSION, SERVE_FLAG,
};
use mythos_plugin_sdk::{
    async_trait, MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata,
    PluginType, ProtocolDriver, TagDeclaration, TagSample,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Longest a module may take to print its manifest.
pub const MANIFEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest a module may take to answer `hello`, `describe` or `start`.
pub const SETUP_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest a module may take to answer `stop` before it is killed.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(2);
/// Shortest time between two starts of a module process that keeps dying.
pub const RESPAWN_BACKOFF: Duration = Duration::from_secs(2);

/// Identifies one build of a module file: replacing the file changes it.
///
/// Size and modification time alone are not enough: on macOS `cp` and `std::fs::copy` clone a
/// file with its modification time, so reinstalling a build leaves both unchanged. Which file it
/// is settles that: a file installed by writing it beside the old one and renaming it over is a
/// new file, whatever its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Fingerprint {
    pub len: u64,
    pub modified_unix_ns: u128,
    /// Which file this is: device and inode on Unix, volume serial and file index on Windows.
    /// (Not the creation time on Windows: NTFS gives a file created under a name that was just
    /// renamed away the old file's creation time, which is exactly how a module is replaced.)
    ///
    /// Written to JSON as hexadecimal text: device and inode together do not fit the 64 bits
    /// a JSON number can carry, and serialising it as a number failed every request that listed
    /// the modules.
    #[serde(serialize_with = "hex_text")]
    pub file_id: u128,
}

fn hex_text<S: serde::Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{value:x}"))
}

impl Fingerprint {
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let unix_ns = |t: SystemTime| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        };
        #[cfg(unix)]
        let file_id = {
            use std::os::unix::fs::MetadataExt;
            ((meta.dev() as u128) << 64) | meta.ino() as u128
        };
        #[cfg(windows)]
        let file_id = windows_file_id(path)?;
        Ok(Self {
            len: meta.len(),
            modified_unix_ns: unix_ns(meta.modified()?),
            file_id,
        })
    }
}

/// The volume serial number and file index of `path`: NTFS's equivalent of device and inode.
#[cfg(windows)]
fn windows_file_id(path: &Path) -> std::io::Result<u128> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let file = std::fs::File::open(path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(((info.dwVolumeSerialNumber as u128) << 64)
        | ((info.nFileIndexHigh as u128) << 32)
        | info.nFileIndexLow as u128)
}

/// A module executable whose manifest has been read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleBinary {
    pub path: PathBuf,
    pub manifest: ModuleManifest,
    pub fingerprint: Fingerprint,
}

/// An executable that looked like a module and could not be used, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleProblem {
    pub path: PathBuf,
    pub message: String,
}

/// What a search of the module directories found.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Discovery {
    pub modules: Vec<ModuleBinary>,
    pub problems: Vec<ModuleProblem>,
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    meta.is_file()
}

/// Whether a file name is one a module executable may have on this platform.
fn is_module_file_name(name: &str) -> bool {
    is_module_file_name_on(name, cfg!(windows))
}

/// Whether `name` is a module executable's name: `mythos-module-<name>`, plus `.exe` on
/// Windows, and nothing else.
///
/// Build tools and installers leave other files beside an executable (`mythos-module-x.d`,
/// `mythos-module-x.pdb`, a renamed old build): none of them is a module. On Windows only an
/// `.exe` can be started at all, so a file without that extension is not one either.
fn is_module_file_name_on(name: &str, windows: bool) -> bool {
    let path = Path::new(name);
    let ext = path.extension().and_then(|e| e.to_str());
    let stem = match (windows, ext) {
        (true, Some(ext)) if ext.eq_ignore_ascii_case("exe") => {
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
        }
        (false, None) => name,
        _ => return false,
    };
    stem.strip_prefix(MODULE_FILE_PREFIX)
        .is_some_and(|rest| !rest.is_empty())
}

/// Linux refuses to execute a file that some process holds open for writing (`ETXTBSY`). That
/// happens briefly while a module is being installed, and whenever another thread of this
/// process forks while a file is being written. It passes; anything else does not.
pub(crate) fn is_text_file_busy(e: &std::io::Error) -> bool {
    cfg!(target_os = "linux") && e.raw_os_error() == Some(26)
}

pub(crate) const BUSY_RETRIES: u32 = 10;

/// Run `path --manifest` and read what it prints.
pub async fn read_manifest(path: &Path) -> Result<ModuleManifest, String> {
    let mut attempt = 0;
    let run = loop {
        let result = Command::new(path)
            .arg(MANIFEST_FLAG)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        match result {
            Err(e) if is_text_file_busy(&e) && attempt < BUSY_RETRIES => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(20 * attempt as u64)).await;
            }
            other => break other,
        }
    };
    let child = run.map_err(|e| format!("it could not be run: {e}"))?;
    let output = tokio::time::timeout(MANIFEST_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            format!(
                "it did not print a manifest within {} s",
                MANIFEST_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("it could not be run: {e}"))?;

    if !output.status.success() {
        return Err(format!("'{MANIFEST_FLAG}' exited with {}", output.status));
    }
    let manifest: ModuleManifest = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("its manifest is not valid: {e}"))?;
    if manifest.protocol != PROTOCOL_VERSION {
        return Err(format!(
            "it speaks protocol {}, this server speaks {}",
            manifest.protocol, PROTOCOL_VERSION
        ));
    }
    if manifest.provides.trim().is_empty() {
        return Err("its manifest names nothing it provides".to_string());
    }
    Ok(manifest)
}

/// The executables in `dir` named like modules, sorted by path.
fn candidates_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(is_module_file_name)
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .filter(|path| {
            std::fs::metadata(path)
                .map(|m| is_executable(&m))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    paths
}

/// What is installed in `dirs`, cheaply: each module-named executable and its fingerprint.
///
/// Comparing two snapshots tells whether a module was added, removed or replaced, without
/// running anything. [`discover`] is what reads the manifests.
pub fn snapshot(dirs: &[PathBuf]) -> Vec<(PathBuf, Option<Fingerprint>)> {
    dirs.iter()
        .flat_map(|dir| candidates_in(dir))
        .map(|path| {
            let fingerprint = Fingerprint::of(&path).ok();
            (path, fingerprint)
        })
        .collect()
}

/// Find module executables in `dirs`, in order, and read their manifests.
///
/// A directory that does not exist is skipped. When two modules provide the same name, the
/// first one found is used and the other is reported as a problem.
pub async fn discover(dirs: &[PathBuf]) -> Discovery {
    let mut discovery = Discovery::default();
    let mut provided: HashMap<(ModuleKind, String), PathBuf> = HashMap::new();

    for dir in dirs {
        let paths = candidates_in(dir);
        for path in paths {
            let fingerprint = match Fingerprint::of(&path) {
                Ok(fingerprint) => fingerprint,
                Err(e) => {
                    discovery.problems.push(ModuleProblem {
                        path,
                        message: format!("it cannot be read: {e}"),
                    });
                    continue;
                }
            };
            match read_manifest(&path).await {
                Ok(manifest) => {
                    let key = (manifest.kind, manifest.provides.clone());
                    if let Some(first) = provided.get(&key) {
                        discovery.problems.push(ModuleProblem {
                            message: format!(
                                "'{}' is already provided by {}",
                                manifest.provides,
                                first.display()
                            ),
                            path,
                        });
                        continue;
                    }
                    provided.insert(key, path.clone());
                    discovery.modules.push(ModuleBinary {
                        path,
                        manifest,
                        fingerprint,
                    });
                }
                Err(message) => discovery.problems.push(ModuleProblem { path, message }),
            }
        }
    }
    discovery
}

// -----------------------------------------------------------------------------
// One running module process
// -----------------------------------------------------------------------------

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

struct Session {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
    /// Why the process stopped answering, once it has.
    ended: Arc<Mutex<Option<String>>>,
    /// Set when the host closes the session, so the end of its output is not reported as a
    /// failure.
    closing: Arc<std::sync::atomic::AtomicBool>,
    next_id: u64,
    /// Set while a request is outstanding. Still set at the start of the next call means the
    /// previous one was abandoned mid-way, and the stream can no longer be trusted.
    in_flight: bool,
    reader: JoinHandle<()>,
    stderr: JoinHandle<()>,
}

impl Session {
    async fn spawn(module: &ModuleBinary, label: &str) -> Result<Self, PluginError> {
        let mut attempt = 0;
        let spawned = loop {
            let result = Command::new(&module.path)
                .arg(SERVE_FLAG)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn();
            match result {
                Err(e) if is_text_file_busy(&e) && attempt < BUSY_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(20 * attempt as u64)).await;
                }
                other => break other,
            }
        };
        let mut child = spawned.map_err(|e| {
            PluginError::StartFailed(format!(
                "module {} could not be started: {e}",
                module.path.display()
            ))
        })?;

        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(PluginError::StartFailed(
                "module process has no standard streams".to_string(),
            ));
        };

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let ended: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let reader = {
            let pending = Arc::clone(&pending);
            let ended = Arc::clone(&ended);
            let closing = Arc::clone(&closing);
            let label = label.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                let reason = loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => match serde_json::from_str::<Response>(&line) {
                            Ok(response) => {
                                let waiter = pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .remove(&response.id);
                                if let Some(waiter) = waiter {
                                    let _ = waiter.send(response);
                                }
                            }
                            Err(e) => {
                                break format!(
                                    "the module wrote something that is not a response: {e}"
                                )
                            }
                        },
                        Ok(None) => break "the module process closed its output".to_string(),
                        Err(e) => break format!("the module output could not be read: {e}"),
                    }
                };
                if closing.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::debug!("module {label}: {reason}");
                } else {
                    warn!("module {label}: {reason}");
                }
                *ended.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
                // Dropping the waiters wakes every caller with an error.
                pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
            })
        };

        let stderr = {
            let label = label.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    info!("module {label}: {line}");
                }
            })
        };

        Ok(Self {
            child,
            stdin,
            pending,
            ended,
            closing,
            next_id: 1,
            in_flight: false,
            reader,
            stderr,
        })
    }

    fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Whether the process is running and its stream can still be trusted.
    fn usable(&mut self) -> bool {
        !self.in_flight
            && self
                .ended
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_none()
            && matches!(self.child.try_wait(), Ok(None))
    }

    fn end_reason(&mut self) -> String {
        if self.in_flight {
            return "a request to the module was abandoned before it answered".to_string();
        }
        if let Some(reason) = self.ended.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return reason;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => format!("the module process exited ({status})"),
            _ => "the module process is not answering".to_string(),
        }
    }

    async fn call(&mut self, call: Call, deadline: Option<Duration>) -> Result<Value, PluginError> {
        if !self.usable() {
            return Err(PluginError::CommunicationError(self.end_reason()));
        }
        let id = self.next_id;
        self.next_id += 1;

        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);

        let mut line = serde_json::to_string(&Request { id, call })
            .map_err(|e| PluginError::RuntimeError(format!("request cannot be encoded: {e}")))?;
        line.push('\n');

        self.in_flight = true;
        if let Err(e) = self.stdin.write_all(line.as_bytes()).await {
            return Err(PluginError::CommunicationError(format!(
                "the module process cannot be written to: {e}"
            )));
        }
        let _ = self.stdin.flush().await;

        let answer = match deadline {
            Some(limit) => match tokio::time::timeout(limit, rx).await {
                Ok(answer) => answer,
                Err(_) => {
                    return Err(PluginError::Timeout(format!(
                        "the module did not answer within {} ms",
                        limit.as_millis()
                    )))
                }
            },
            None => rx.await,
        };
        self.in_flight = false;

        let response = answer.map_err(|_| PluginError::CommunicationError(self.end_reason()))?;
        match (response.result, response.error) {
            (_, Some(error)) => Err(error.into()),
            (Some(result), None) => Ok(result),
            (None, None) => Ok(Value::Null),
        }
    }

    /// Ask the module to stop, then close it. Kills it if it does not go.
    async fn close(mut self, polite: bool) {
        if polite && self.usable() {
            let _ = self.call(Call::Stop, Some(STOP_TIMEOUT)).await;
        }
        self.closing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        drop(self.stdin);
        if tokio::time::timeout(STOP_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
        self.reader.abort();
        self.stderr.abort();
    }
}

// -----------------------------------------------------------------------------
// ProcessDriver
// -----------------------------------------------------------------------------

/// A protocol driver whose implementation runs in a module process.
pub struct ProcessDriver {
    module: Arc<ModuleBinary>,
    session: Option<Session>,
    context: Option<WireContext>,
    config: Option<Value>,
    declared: Vec<TagDeclaration>,
    last_spawn: Option<Instant>,
    restarts: u64,
}

impl ProcessDriver {
    pub fn new(module: Arc<ModuleBinary>) -> Self {
        Self {
            module,
            session: None,
            context: None,
            config: None,
            declared: Vec::new(),
            last_spawn: None,
            restarts: 0,
        }
    }

    pub fn module(&self) -> &ModuleBinary {
        &self.module
    }

    /// The process id of the running module process, if there is one.
    pub fn pid(&self) -> Option<u32> {
        self.session.as_ref().and_then(Session::pid)
    }

    /// How many times a module process that ended was started again.
    pub fn restarts(&self) -> u64 {
        self.restarts
    }

    /// Send the running module a call that is not one of a driver's (a runtime module's
    /// `exchange` or `mode`), reviving its process first as a poll does.
    pub async fn call(&mut self, call: Call, deadline: Duration) -> Result<Value, PluginError> {
        self.revive().await?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| PluginError::CommunicationError("no module process".to_string()))?;
        session.call(call, Some(deadline)).await
    }

    fn label(&self) -> String {
        match &self.context {
            Some(context) => format!("{} ({})", self.module.manifest.id, context.plugin_id),
            None => self.module.manifest.id.clone(),
        }
    }

    /// Start a process and say hello. The manifest it answers with must be the one discovered:
    /// a file replaced by a different module is not silently adopted.
    async fn open(&mut self) -> Result<(), PluginError> {
        let context = self.context.clone().ok_or_else(|| {
            PluginError::InitializationFailed("the driver has not been initialised".to_string())
        })?;
        self.last_spawn = Some(Instant::now());
        let mut session = Session::spawn(&self.module, &self.label()).await?;
        let answer = session
            .call(Call::Hello { context }, Some(SETUP_TIMEOUT))
            .await;
        let manifest: ModuleManifest = match answer.and_then(|value| {
            serde_json::from_value(value).map_err(|e| {
                PluginError::InitializationFailed(format!("module manifest is not valid: {e}"))
            })
        }) {
            Ok(manifest) => manifest,
            Err(e) => {
                session.close(false).await;
                return Err(e);
            }
        };
        if manifest.id != self.module.manifest.id || manifest.protocol != PROTOCOL_VERSION {
            session.close(false).await;
            return Err(PluginError::InitializationFailed(format!(
                "{} is now module '{}' (protocol {}), not the '{}' that was discovered",
                self.module.path.display(),
                manifest.id,
                manifest.protocol,
                self.module.manifest.id
            )));
        }
        self.session = Some(session);
        Ok(())
    }

    async fn start_session(&mut self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| PluginError::StartFailed("no module process".to_string()))?;
        let value = session
            .call(
                Call::Start {
                    config: config.clone(),
                },
                Some(SETUP_TIMEOUT),
            )
            .await?;
        let declarations: Vec<WireDeclaration> = serde_json::from_value(value).map_err(|e| {
            PluginError::StartFailed(format!("module answered start with no tag list: {e}"))
        })?;
        Ok(declarations.into_iter().map(Into::into).collect())
    }

    /// Bring a module process that ended back, with the configuration it was running.
    async fn revive(&mut self) -> Result<(), PluginError> {
        let reason = match self.session.as_mut() {
            Some(session) => {
                if session.usable() {
                    return Ok(());
                }
                session.end_reason()
            }
            None => "the module process is not running".to_string(),
        };
        let Some(config) = self.config.clone() else {
            return Err(PluginError::RuntimeError(
                "the driver has not been started".to_string(),
            ));
        };
        if let Some(old) = self.session.take() {
            old.close(false).await;
        }
        if let Some(at) = self.last_spawn {
            if at.elapsed() < RESPAWN_BACKOFF {
                return Err(PluginError::CommunicationError(format!(
                    "{reason}; it will be started again"
                )));
            }
        }

        warn!("module {}: {reason}; starting it again", self.label());
        self.restarts += 1;
        self.open().await?;
        let declared = match self.start_session(&config).await {
            Ok(declared) => declared,
            Err(e) => {
                if let Some(session) = self.session.take() {
                    session.close(false).await;
                }
                return Err(e);
            }
        };
        if declared != self.declared {
            // The tags were registered from the first answer. A process that now declares
            // others would have its samples land on the wrong slots or none.
            if let Some(session) = self.session.take() {
                session.close(false).await;
            }
            return Err(PluginError::RuntimeError(
                "the restarted module declares different tags than before".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl MythosPlugin for ProcessDriver {
    fn metadata(&self) -> PluginMetadata {
        let manifest = &self.module.manifest;
        PluginMetadata {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            author: String::new(),
            description: manifest.description.clone(),
            plugin_type: match manifest.kind {
                ModuleKind::Driver => PluginType::ProtocolDriver,
                ModuleKind::Service => PluginType::Extension,
                ModuleKind::Runtime => PluginType::ProcessEngine,
            },
            capabilities: manifest.capabilities.clone(),
        }
    }

    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        let mut capabilities: Vec<String> = ctx.allowed_capabilities.iter().cloned().collect();
        capabilities.sort();
        self.context = Some(WireContext {
            plugin_id: ctx.plugin_id.clone(),
            capabilities,
        });
        if let Some(old) = self.session.take() {
            old.close(true).await;
        }
        self.open().await
    }

    async fn start(&mut self, _ctx: &PluginContext, config: &Value) -> Result<(), PluginError> {
        let usable = match self.session.as_mut() {
            Some(session) => session.usable(),
            None => false,
        };
        if !usable {
            if let Some(old) = self.session.take() {
                old.close(false).await;
            }
            self.open().await?;
        }
        let declared = self.start_session(config).await?;
        self.config = Some(config.clone());
        self.declared = declared;
        Ok(())
    }

    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        if let Some(session) = self.session.take() {
            session.close(true).await;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        Ok(match &self.session {
            Some(session) => match session.pid() {
                Some(pid) => PluginHealthReport::healthy(format!("module process {pid} running")),
                None => PluginHealthReport::unhealthy("module process has exited"),
            },
            None => PluginHealthReport::stopped("no module process"),
        })
    }
}

#[async_trait]
impl ProtocolDriver for ProcessDriver {
    async fn describe_config(&self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        // A short-lived process of its own, so checking a configuration never disturbs one
        // that is running.
        let mut session = Session::spawn(&self.module, &self.module.manifest.id).await?;
        let answer = session
            .call(
                Call::Describe {
                    config: config.clone(),
                },
                Some(SETUP_TIMEOUT),
            )
            .await;
        session.close(false).await;
        let declarations: Vec<WireDeclaration> = serde_json::from_value(answer?).map_err(|e| {
            PluginError::RuntimeError(format!("module answered describe with no tag list: {e}"))
        })?;
        Ok(declarations.into_iter().map(Into::into).collect())
    }

    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.declared.clone()
    }

    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        self.revive().await?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| PluginError::CommunicationError("no module process".to_string()))?;
        let value = session.call(Call::Poll, None).await?;
        let samples: Vec<WireSample> = serde_json::from_value(value).map_err(|e| {
            PluginError::CommunicationError(format!("module answered poll with no samples: {e}"))
        })?;
        Ok(samples.into_iter().map(Into::into).collect())
    }

    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        tag_uri: &str,
        value: &Value,
    ) -> Result<(), PluginError> {
        self.revive().await?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| PluginError::CommunicationError("no module process".to_string()))?;
        session
            .call(
                Call::Write {
                    uri: tag_uri.to_string(),
                    value: value.clone(),
                },
                Some(SETUP_TIMEOUT),
            )
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS copies a file with its modification time: a reinstalled build has the same size
    /// and time as the one it replaces, and must still count as a new build.
    #[test]
    fn a_file_moved_in_over_another_is_a_new_build_even_with_the_same_size_and_time() {
        let dir = std::env::temp_dir().join(format!("mythos-fingerprint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let installed = dir.join("mythos-module-x");
        let staged = dir.join(".staged");
        std::fs::write(&installed, b"build").unwrap();
        let before = Fingerprint::of(&installed).unwrap();

        std::fs::write(&staged, b"build").unwrap();
        let mtime = std::fs::metadata(&installed).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&staged)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        std::fs::rename(&staged, &installed).unwrap();
        let after = Fingerprint::of(&installed).unwrap();

        assert_eq!(
            (after.len, after.modified_unix_ns),
            (before.len, before.modified_unix_ns)
        );
        assert_ne!(
            after, before,
            "the replaced file was not seen as a new build"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// GET /api/v1/modules (and its MCP tool) writes every module's fingerprint as JSON.
    #[test]
    fn a_fingerprint_of_any_file_can_be_written_as_json() {
        let largest = Fingerprint {
            len: u64::MAX,
            modified_unix_ns: u64::MAX as u128,
            file_id: u128::MAX,
        };
        let json = serde_json::to_value(largest).expect("a fingerprint serialises");
        assert_eq!(json["file_id"], "ffffffffffffffffffffffffffffffff");

        let real = Fingerprint::of(&std::env::current_exe().unwrap()).unwrap();
        serde_json::to_value(real).expect("this test's own executable serialises");
    }

    #[test]
    fn only_executables_named_like_modules_are_considered() {
        for windows in [false, true] {
            assert!(!is_module_file_name_on("mythos-module-", windows));
            assert!(!is_module_file_name_on("mythos-module-.exe", windows));
            assert!(!is_module_file_name_on("mythos-module-modbus.d", windows));
            assert!(!is_module_file_name_on("mythos-module-modbus.pdb", windows));
            assert!(!is_module_file_name_on("mythos-module-modbus.old", windows));
            assert!(!is_module_file_name_on("mythos-server", windows));
            assert!(!is_module_file_name_on("mythos-server.exe", windows));
            assert!(!is_module_file_name_on("libmythos_module.rlib", windows));
        }
        // Unix: no extension at all.
        assert!(is_module_file_name_on("mythos-module-modbus", false));
        assert!(!is_module_file_name_on("mythos-module-modbus.exe", false));
        // Windows: an .exe, and nothing without that extension can be started.
        assert!(is_module_file_name_on("mythos-module-modbus.exe", true));
        assert!(is_module_file_name_on("mythos-module-modbus.EXE", true));
        assert!(!is_module_file_name_on("mythos-module-modbus", true));
    }
}
