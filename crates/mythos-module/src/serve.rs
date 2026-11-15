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
    Call, ErrorKind, ModuleManifest, Request, Response, WireDeclaration, WireError, WireSample,
    MANIFEST_FLAG, SERVE_FLAG,
};
use mythos_plugin_sdk::{PluginContext, PluginError, ProtocolDriver};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::process::ExitCode;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// Entry point of a driver module executable.
///
/// `--manifest` prints the manifest as JSON and exits. `--serve` serves the protocol on stdin
/// and stdout until stdin closes. Anything else is a usage error.
pub fn run_driver_module<D, F>(manifest: ModuleManifest, factory: F) -> ExitCode
where
    D: ProtocolDriver + 'static,
    F: Fn() -> D,
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
            runtime.block_on(serve_driver(
                manifest,
                factory,
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
                match handle(&manifest, &factory, &mut driver, &mut ctx, request.call).await {
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

async fn handle<D, F>(
    manifest: &ModuleManifest,
    factory: &F,
    driver: &mut D,
    ctx: &mut Option<PluginContext>,
    call: Call,
) -> Result<Value, WireError>
where
    D: ProtocolDriver,
    F: Fn() -> D,
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
    }
}
