//! Service modules (ADR-011): both sides of the protocol for a module that serves API routes.
//!
//! A service module's binary is one line:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     mythos_module::run_service_module(manifest(), MyService::new())
//! }
//! ```
//!
//! Unlike a driver, a service answers many requests at once: each one is handled on its own
//! task, and answers are written as they are ready. It may also write events at any time.

use crate::host::{is_text_file_busy, ModuleBinary, BUSY_RETRIES};
use crate::protocol::{
    Call, ErrorKind, HttpCall, HttpReply, ModuleManifest, Outgoing, Request, Response, WireError,
    WireEvent, MANIFEST_FLAG, SERVE_FLAG,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};

// -----------------------------------------------------------------------------
// The module side
// -----------------------------------------------------------------------------

/// What a service module does with each request to one of its routes.
#[async_trait::async_trait]
pub trait Service: Send + Sync + 'static {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply;

    /// The node this service runs on, told after `hello` and whenever it changes
    /// (ADR-012). Most services do not need it.
    async fn node(&self, _node: crate::protocol::WireNode, _events: &Events) {}
}

/// Sends events to the server, which relays them to every open workbench.
#[derive(Clone)]
pub struct Events {
    lines: mpsc::UnboundedSender<String>,
}

impl Events {
    pub fn send(&self, name: impl Into<String>, data: Value) {
        let line = Outgoing::Event {
            event: WireEvent {
                name: name.into(),
                data,
            },
        };
        if let Ok(text) = serde_json::to_string(&line) {
            let _ = self.lines.send(text);
        }
    }
}

/// The `main` of a service module: `--manifest` prints the manifest, `--serve` serves.
pub fn run_service_module<S: Service>(manifest: ModuleManifest, service: S) -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some(MANIFEST_FLAG) => match serde_json::to_string(&manifest) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("manifest cannot be encoded: {e}");
                ExitCode::FAILURE
            }
        },
        Some(SERVE_FLAG) => {
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .try_init();
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    eprintln!("module runtime cannot start: {e}");
                    return ExitCode::FAILURE;
                }
            };
            runtime.block_on(serve_service(
                manifest,
                Arc::new(service),
                tokio::io::stdin(),
                tokio::io::stdout(),
            ));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "{} is a MythOS module. It is started by the MythOS server.\nusage: {MANIFEST_FLAG} | {SERVE_FLAG}",
                manifest.id
            );
            ExitCode::from(2)
        }
    }
}

/// Serve requests from `input` until it closes or `stop` is asked, writing to `output`.
pub async fn serve_service<S, R, W>(manifest: ModuleManifest, service: Arc<S>, input: R, output: W)
where
    S: Service,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (lines_tx, mut lines_rx) = mpsc::unbounded_channel::<String>();
    // Told when the input has ended: the writer then writes what is queued and stops. It cannot
    // wait for every sender to be dropped instead, because a service may keep an `Events` on a
    // thread of its own for as long as the process lives (discovery's listener does), and then a
    // module whose server died or stopped it would never exit.
    let (closing, mut closed) = oneshot::channel::<()>();
    let writer = tokio::spawn(async move {
        let mut output = output;
        loop {
            let line = tokio::select! {
                biased;
                line = lines_rx.recv() => line,
                _ = &mut closed => {
                    while let Ok(line) = lines_rx.try_recv() {
                        if write_line(&mut output, line).await.is_err() {
                            break;
                        }
                    }
                    break;
                }
            };
            let Some(line) = line else { break };
            if write_line(&mut output, line).await.is_err() {
                break;
            }
        }
    });
    let events = Events {
        lines: lines_tx.clone(),
    };
    let answer = |lines: &mpsc::UnboundedSender<String>, response: Response| {
        if let Ok(text) = serde_json::to_string(&response) {
            let _ = lines.send(text);
        }
    };

    let mut input = BufReader::new(input).lines();
    let mut tasks = Vec::new();
    while let Ok(Some(line)) = input.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(e) => {
                answer(
                    &lines_tx,
                    Response::err(
                        0,
                        WireError {
                            kind: ErrorKind::Protocol,
                            message: format!("unreadable request: {e}"),
                        },
                    ),
                );
                continue;
            }
        };
        let id = request.id;
        match request.call {
            Call::Hello { .. } => answer(
                &lines_tx,
                Response::ok(id, serde_json::to_value(&manifest).unwrap_or(Value::Null)),
            ),
            Call::Health => answer(&lines_tx, Response::ok(id, json!({ "healthy": true }))),
            Call::Node { node } => {
                service.node(node, &events).await;
                answer(&lines_tx, Response::ok(id, Value::Null));
            }
            Call::Http(call) => {
                let service = Arc::clone(&service);
                let events = events.clone();
                let lines = lines_tx.clone();
                tasks.push(tokio::spawn(async move {
                    let reply = service.handle(call, &events).await;
                    let response =
                        Response::ok(id, serde_json::to_value(reply).unwrap_or(Value::Null));
                    if let Ok(text) = serde_json::to_string(&response) {
                        let _ = lines.send(text);
                    }
                }));
            }
            Call::Stop => {
                for task in tasks.drain(..) {
                    let _ = task.await;
                }
                answer(&lines_tx, Response::ok(id, Value::Null));
                break;
            }
            other => answer(
                &lines_tx,
                Response::err(
                    id,
                    WireError {
                        kind: ErrorKind::Protocol,
                        message: format!(
                            "{} is a service; it does not answer {:?}",
                            manifest.id,
                            serde_json::to_value(&other)
                                .ok()
                                .and_then(|v| v.get("method").cloned())
                                .unwrap_or(Value::Null)
                        ),
                    },
                ),
            ),
        }
    }
    for task in tasks {
        let _ = task.await;
    }
    drop(events);
    drop(lines_tx);
    let _ = closing.send(());
    let _ = writer.await;
}

async fn write_line<W: AsyncWrite + Unpin>(
    output: &mut W,
    mut line: String,
) -> std::io::Result<()> {
    line.push('\n');
    output.write_all(line.as_bytes()).await?;
    output.flush().await
}

// -----------------------------------------------------------------------------
// The host side
// -----------------------------------------------------------------------------

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

/// One running service module process. Calls may overlap.
pub struct ServiceSession {
    label: String,
    child: Mutex<Child>,
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Pending,
    ended: Arc<Mutex<Option<String>>>,
    closing: Arc<AtomicBool>,
    next_id: AtomicU64,
    reader: JoinHandle<()>,
    stderr: JoinHandle<()>,
}

/// How long a service may take to answer a request.
pub const SERVICE_CALL_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE_STOP_TIMEOUT: Duration = Duration::from_secs(5);

impl ServiceSession {
    /// Start `module` and greet it. Its events are sent on `events`.
    pub async fn start(
        module: &ModuleBinary,
        events: mpsc::UnboundedSender<WireEvent>,
    ) -> Result<Self, String> {
        let label = format!("{} ({})", module.manifest.id, module.path.display());
        let mut attempt = 0;
        let spawned = loop {
            let result = Command::new(&module.path)
                .arg(SERVE_FLAG)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
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
        let mut child = spawned.map_err(|e| format!("{label} could not be started: {e}"))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(format!("{label} has no standard streams"));
        };

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let ended: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let closing = Arc::new(AtomicBool::new(false));

        let reader = {
            let (pending, ended, closing, label) = (
                Arc::clone(&pending),
                Arc::clone(&ended),
                Arc::clone(&closing),
                label.clone(),
            );
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                let reason = loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => match serde_json::from_str::<Outgoing>(&line) {
                            Ok(Outgoing::Event { event }) => {
                                let _ = events.send(event);
                            }
                            Ok(Outgoing::Response(response)) => {
                                let waiter = pending
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .remove(&response.id);
                                if let Some(waiter) = waiter {
                                    let _ = waiter.send(response);
                                }
                            }
                            Err(e) => {
                                break format!("the module wrote something that is neither a response nor an event: {e}")
                            }
                        },
                        Ok(None) => break "the module process closed its output".to_string(),
                        Err(e) => break format!("the module output could not be read: {e}"),
                    }
                };
                if closing.load(Ordering::SeqCst) {
                    tracing::debug!("module {label}: {reason}");
                } else {
                    warn!("module {label}: {reason}");
                }
                *ended.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
                pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
            })
        };
        let stderr = {
            let label = label.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    info!("module {label}: {line}");
                }
            })
        };

        let session = Self {
            label,
            child: Mutex::new(child),
            stdin: tokio::sync::Mutex::new(stdin),
            pending,
            ended,
            closing,
            next_id: AtomicU64::new(1),
            reader,
            stderr,
        };
        let greeting = session
            .call(
                Call::Hello {
                    context: crate::protocol::WireContext {
                        plugin_id: module.manifest.id.clone(),
                        capabilities: module.manifest.capabilities.clone(),
                    },
                },
                Duration::from_secs(15),
            )
            .await?;
        let said: ModuleManifest = serde_json::from_value(greeting)
            .map_err(|e| format!("{}: its greeting is not a manifest: {e}", session.label))?;
        if said.id != module.manifest.id {
            return Err(format!(
                "{}: it greets as '{}', not as the module it was found as",
                session.label, said.id
            ));
        }
        Ok(session)
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).id()
    }

    /// Whether the process is running and answering.
    pub fn alive(&self) -> bool {
        self.ended
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
            && matches!(
                self.child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .try_wait(),
                Ok(None)
            )
    }

    fn end_reason(&self) -> String {
        self.ended
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| "the module process is not answering".to_string())
    }

    pub async fn call(&self, call: Call, deadline: Duration) -> Result<Value, String> {
        if !self.alive() {
            return Err(self.end_reason());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        let mut line = serde_json::to_string(&Request { id, call })
            .map_err(|e| format!("request cannot be encoded: {e}"))?;
        line.push('\n');
        {
            let mut stdin = self.stdin.lock().await;
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                return Err(format!("the module process cannot be written to: {e}"));
            }
            let _ = stdin.flush().await;
        }
        let response = match tokio::time::timeout(deadline, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(self.end_reason()),
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                return Err(format!(
                    "the module did not answer within {} s",
                    deadline.as_secs()
                ));
            }
        };
        match (response.result, response.error) {
            (_, Some(error)) => Err(error.message),
            (Some(result), None) => Ok(result),
            (None, None) => Ok(Value::Null),
        }
    }

    /// Forward a request to one of the module's routes.
    pub async fn http(&self, call: HttpCall) -> Result<HttpReply, String> {
        let value = self.call(Call::Http(call), SERVICE_CALL_TIMEOUT).await?;
        serde_json::from_value(value)
            .map_err(|e| format!("the module's answer is not a reply: {e}"))
    }

    /// Tell the module which node it runs on.
    pub async fn node(&self, node: crate::protocol::WireNode) -> Result<(), String> {
        self.call(Call::Node { node }, SERVICE_CALL_TIMEOUT)
            .await
            .map(|_| ())
    }

    /// Ask the module to stop (it finishes the requests it has), then close it.
    pub async fn close(self) {
        if self.alive() {
            let _ = self.call(Call::Stop, SERVICE_STOP_TIMEOUT).await;
        }
        self.closing.store(true, Ordering::SeqCst);
        let mut child = self.child.into_inner().unwrap_or_else(|e| e.into_inner());
        drop(self.stdin);
        if tokio::time::timeout(SERVICE_STOP_TIMEOUT, child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
        self.reader.abort();
        self.stderr.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A service that keeps an `Events` on a thread of its own, as discovery's listener does.
    struct Holding;

    #[async_trait::async_trait]
    impl Service for Holding {
        async fn handle(&self, _call: HttpCall, _events: &Events) -> HttpReply {
            unreachable!("no route in this test")
        }

        async fn node(&self, _node: crate::protocol::WireNode, events: &Events) {
            let events = events.clone();
            std::thread::spawn(move || loop {
                events.send("tick", Value::Null);
                std::thread::sleep(std::time::Duration::from_millis(20));
            });
        }
    }

    fn manifest() -> ModuleManifest {
        serde_json::from_value(json!({
            "protocol": 1, "id": "org.test.holding", "name": "Holding", "version": "0.1.0",
            "kind": "service", "provides": "holding", "description": "keeps an Events forever"
        }))
        .unwrap()
    }

    const NODE: &str = r#"{"id":1,"call":{"method":"node","params":{"node":{"id":"n","name":"n","role":"edge","announce":false,"version":"0","protocol":1}}}}"#;

    /// Serve until `input` ends or asks to stop; how long it took to return, and what it wrote.
    async fn serve_with(
        requests: &str,
        keep_open: bool,
    ) -> (Result<(), tokio::time::error::Elapsed>, Vec<String>) {
        let (mut to_module, input) = tokio::io::duplex(1 << 16);
        let (output, from_module) = tokio::io::duplex(1 << 20);
        to_module.write_all(requests.as_bytes()).await.unwrap();
        let _held = if keep_open {
            Some(to_module)
        } else {
            drop(to_module);
            None
        };
        let served = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            serve_service(manifest(), Arc::new(Holding), input, output),
        )
        .await;
        let mut lines = Vec::new();
        let mut reader = BufReader::new(from_module).lines();
        while let Ok(Ok(Some(line))) =
            tokio::time::timeout(std::time::Duration::from_millis(200), reader.next_line()).await
        {
            lines.push(line);
        }
        (served, lines)
    }

    #[tokio::test]
    async fn a_module_whose_server_is_gone_stops_though_its_service_keeps_an_events() {
        let (served, lines) = serve_with(&format!("{NODE}\n"), false).await;
        assert!(served.is_ok(), "the input ended, so the module stops");
        assert!(
            lines.iter().any(|l| l.contains(r#""id":1"#)),
            "the node call was answered: {lines:?}"
        );
    }

    #[tokio::test]
    async fn a_module_told_to_stop_answers_and_stops_though_its_service_keeps_an_events() {
        let (served, lines) = serve_with(
            &format!("{NODE}\n{{\"id\":2,\"call\":{{\"method\":\"stop\"}}}}\n"),
            true,
        )
        .await;
        assert!(served.is_ok(), "asked to stop, the module stops");
        assert!(
            lines.iter().any(|l| l.contains(r#""id":2"#)),
            "and answered the stop first: {lines:?}"
        );
    }
}
