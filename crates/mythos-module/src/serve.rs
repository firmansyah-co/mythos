//! The module side of the protocol: turn a [`ProtocolDriver`] into a module executable.
//!
//! A driver crate's module binary is one line:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     mythos_module::run_driver_module(manifest(), MyDriver::new)
//! }
//! ```
//!
//! One process serves one driver instance, which is one connection. Requests are handled in
//! the order they arrive, one at a time.

use crate::protocol::{
    Call, ErrorKind, ExchangeReply, ModuleManifest, Request, Response, RuntimeState,
    WireDeclaration, WireError, WireInput, WireSample, MANIFEST_FLAG, SERVE_FLAG,
};
use mythos_plugin_sdk::{async_trait, PluginContext, PluginError, ProtocolDriver};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::ExitCode;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// A runtime module's engine (ADR-013 §2.4): what executes a program against the tags.
///
/// It is a [`ProtocolDriver`] for everything a driver does, so the host runs it with the
/// machinery it runs drivers with: `start` loads the program it is configured with (a
/// [`crate::protocol::RuntimeProgram`] and whether to run it), `declared_tags` are the tags it
/// publishes, `write_tag` writes a published variable, `stop` unloads. `poll_cycle` is never
/// called: the host exchanges instead.
#[async_trait]
pub trait RuntimeEngine: ProtocolDriver {
    /// Take the tags the program reads, as they are now; give what it publishes and what it
    /// wants written. Must not wait for a scan: it reports the last one.
    async fn exchange(
        &mut self,
        ctx: &PluginContext,
        inputs: Vec<WireInput>,
    ) -> Result<ExchangeReply, PluginError>;

    /// Put the program in RUN (from a cold start unless it already runs) or in STOP.
    async fn set_mode(
        &mut self,
        ctx: &PluginContext,
        run: bool,
    ) -> Result<RuntimeState, PluginError>;
}

/// The calls a module answers beyond a driver's.
#[async_trait]
pub trait ExtraCalls<D>: Send + Sync {
    async fn handle(
        &self,
        manifest: &ModuleManifest,
        driver: &mut D,
        ctx: Option<&PluginContext>,
        call: Call,
    ) -> Result<Value, WireError>;
}

/// A driver answers none.
pub struct DriverOnly;

#[async_trait]
impl<D: ProtocolDriver> ExtraCalls<D> for DriverOnly {
    async fn handle(
        &self,
        manifest: &ModuleManifest,
        _driver: &mut D,
        _ctx: Option<&PluginContext>,
        _call: Call,
    ) -> Result<Value, WireError> {
        Err(WireError {
            kind: ErrorKind::Protocol,
            message: format!("{} is a driver; it runs no program", manifest.id),
        })
    }
}

/// A runtime module answers `exchange` and `mode`.
pub struct RuntimeCalls;

#[async_trait]
impl<D: RuntimeEngine> ExtraCalls<D> for RuntimeCalls {
    async fn handle(
        &self,
        _manifest: &ModuleManifest,
        driver: &mut D,
        ctx: Option<&PluginContext>,
        call: Call,
    ) -> Result<Value, WireError> {
        let ctx = ctx.ok_or_else(not_initialised)?;
        let fail = |e: PluginError| WireError::from(&e);
        match call {
            Call::Exchange { inputs } => encode(driver.exchange(ctx, inputs).await.map_err(fail)?),
            Call::Mode { run } => encode(driver.set_mode(ctx, run).await.map_err(fail)?),
            _ => Err(WireError {
                kind: ErrorKind::Protocol,
                message: "not a runtime call".to_string(),
            }),
        }
    }
}

/// Entry point of a driver module executable.
///
/// `--manifest` prints the manifest as JSON and exits. `--serve` serves the protocol on stdin
/// and stdout until stdin closes. Anything else is a usage error.
pub fn run_driver_module<D, F>(manifest: ModuleManifest, factory: F) -> ExitCode
where
    D: ProtocolDriver + 'static,
    F: Fn() -> D,
{
    run_module(manifest, factory, DriverOnly)
}

/// Entry point of a runtime module executable. As [`run_driver_module`], for a
/// [`RuntimeEngine`].
pub fn run_runtime_module<D, F>(manifest: ModuleManifest, factory: F) -> ExitCode
where
    D: RuntimeEngine + 'static,
    F: Fn() -> D,
{
    run_module(manifest, factory, RuntimeCalls)
}

fn run_module<D, F, X>(manifest: ModuleManifest, factory: F, extra: X) -> ExitCode
where
    D: ProtocolDriver + 'static,
    F: Fn() -> D,
    X: ExtraCalls<D>,
{
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
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
            // stdout is the protocol channel, so every log line goes to stderr.
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
            runtime.block_on(serve_module(
                manifest,
                factory,
                extra,
                tokio::io::stdin(),
                tokio::io::stdout(),
            ));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "{} is a MythOS module. It is started by the MythOS server.\n\
                 usage: {} {MANIFEST_FLAG} | {SERVE_FLAG}",
                manifest.id,
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "module".to_string())
            );
            ExitCode::from(2)
        }
    }
}

/// Serve the protocol for one driver instance until `input` closes.
pub async fn serve_driver<D, F, R, W>(manifest: ModuleManifest, factory: F, input: R, output: W)
where
    D: ProtocolDriver,
    F: Fn() -> D,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    serve_module(manifest, factory, DriverOnly, input, output).await
}

/// Serve the protocol for one runtime instance until `input` closes.
pub async fn serve_runtime<D, F, R, W>(manifest: ModuleManifest, factory: F, input: R, output: W)
where
    D: RuntimeEngine,
    F: Fn() -> D,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    serve_module(manifest, factory, RuntimeCalls, input, output).await
}

async fn serve_module<D, F, X, R, W>(
    manifest: ModuleManifest,
    factory: F,
    extra: X,
    input: R,
    output: W,
) where
    D: ProtocolDriver,
    F: Fn() -> D,
    X: ExtraCalls<D>,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(input).lines();
    let mut output = output;
    let mut driver = factory();
    let mut ctx: Option<PluginContext> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let id = request.id;
                match handle(
                    &manifest,
                    &factory,
                    &extra,
                    &mut driver,
                    &mut ctx,
                    request.call,
                )
                .await
                {
                    Ok(result) => Response::ok(id, result),
                    Err(error) => Response::err(id, error),
                }
            }
            // Without a readable id there is nothing to answer to; id 0 is never used by a host.
            Err(e) => Response::err(
                0,
                WireError {
                    kind: ErrorKind::Protocol,
                    message: format!("unreadable request: {e}"),
                },
            ),
        };

        let mut text = match serde_json::to_string(&response) {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("response cannot be encoded: {e}");
                continue;
            }
        };
        text.push('\n');
        if output.write_all(text.as_bytes()).await.is_err() || output.flush().await.is_err() {
            break;
        }
    }

    if let Some(ctx) = &ctx {
        let _ = driver.stop(ctx).await;
    }
}

fn not_initialised() -> WireError {
    WireError {
        kind: ErrorKind::Protocol,
        message: "the module has not been initialised with 'hello'".to_string(),
    }
}

fn encode(value: impl serde::Serialize) -> Result<Value, WireError> {
    serde_json::to_value(value).map_err(|e| WireError {
        kind: ErrorKind::Protocol,
        message: format!("result cannot be encoded: {e}"),
    })
}

async fn handle<D, F, X>(
    manifest: &ModuleManifest,
    factory: &F,
    extra: &X,
    driver: &mut D,
    ctx: &mut Option<PluginContext>,
    call: Call,
) -> Result<Value, WireError>
where
    D: ProtocolDriver,
    F: Fn() -> D,
    X: ExtraCalls<D>,
{
    let fail = |e: PluginError| WireError::from(&e);
    match call {
        Call::Hello { context } => {
            let capabilities: HashSet<String> = context.capabilities.into_iter().collect();
            let new_ctx = PluginContext::new(context.plugin_id, capabilities, String::new());
            driver.init(&new_ctx).await.map_err(fail)?;
            *ctx = Some(new_ctx);
            encode(manifest)
        }
        Call::Describe { config } => {
            // A fresh instance: describing a configuration must not depend on, or change, the
            // state of the one that may be running.
            let declarations = factory().describe_config(&config).await.map_err(fail)?;
            encode(
                declarations
                    .iter()
                    .map(WireDeclaration::from)
                    .collect::<Vec<_>>(),
            )
        }
        Call::Start { config } => {
            let ctx = ctx.as_ref().ok_or_else(not_initialised)?;
            driver.start(ctx, &config).await.map_err(fail)?;
            encode(
                driver
                    .declared_tags()
                    .iter()
                    .map(WireDeclaration::from)
                    .collect::<Vec<_>>(),
            )
        }
        Call::Poll => {
            let ctx = ctx.as_ref().ok_or_else(not_initialised)?;
            let samples = driver.poll_cycle(ctx).await.map_err(fail)?;
            encode(samples.iter().map(WireSample::from).collect::<Vec<_>>())
        }
        Call::Write { uri, value } => {
            let ctx = ctx.as_ref().ok_or_else(not_initialised)?;
            driver.write_tag(ctx, &uri, &value).await.map_err(fail)?;
            Ok(json!(null))
        }
        Call::Health => encode(driver.health_check().await.map_err(fail)?),
        Call::Stop => {
            if let Some(ctx) = ctx.as_ref() {
                driver.stop(ctx).await.map_err(fail)?;
            }
            Ok(json!(null))
        }
        Call::Http(_) => Err(WireError {
            kind: ErrorKind::Protocol,
            message: format!("{} is a driver; it serves no routes", manifest.id),
        }),
        // A driver does not need to know the node; it is told nothing it must act on.
        Call::Node { .. } => Ok(json!(null)),
        call @ (Call::Exchange { .. } | Call::Mode { .. }) => {
            extra.handle(manifest, driver, ctx.as_ref(), call).await
        }
    }
}
