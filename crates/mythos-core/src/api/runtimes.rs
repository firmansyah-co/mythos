//! Runtimes (ADR-013 §2.5): what this node executes, a project's runtimes configuration, and
//! the commands to run and stop a program.

use crate::api::state::ApiState;
use crate::io::ConfigProblem;
use crate::runtimes::{ForceAsk, ForceRefusal, RuntimesConfig, RUNTIMES_FILE, WATCH_LIMIT};
use crate::storage::StorageError;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use mythos_pal::PlatformShm;
use serde_json::{json, Value};
use std::path::PathBuf;

type ApiError = (StatusCode, Json<Value>);

fn failure(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

/// The path of an existing project's `runtimes.json`.
fn config_path<S: PlatformShm>(state: &ApiState<S>, project: &str) -> Result<PathBuf, ApiError> {
    let dir = state
        .storage
        .project_dir(project)
        .map_err(|e| failure(StatusCode::BAD_REQUEST, e))?;
    state.storage.get_project(project).map_err(|e| match e {
        StorageError::InvalidProjectId(_) => failure(StatusCode::BAD_REQUEST, e),
        _ => failure(
            StatusCode::NOT_FOUND,
            format!("Project '{project}' not found"),
        ),
    })?;
    Ok(dir.join(RUNTIMES_FILE))
}

/// `GET /api/v1/runtimes` — every runtime of this node and what it is doing, the runtime
/// modules that could run one (each with its target profile), and why the active project's
/// runtimes are not as configured, when they are not.
pub async fn list<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Json<Value> {
    let Some(io) = &state.io_manager else {
        return Json(json!({
            "runtimes": [],
            "modules": [],
            "problem": "this server has no field I/O manager, so it runs no program",
        }));
    };
    let modules: Vec<Value> = io
        .runtime_modules()
        .iter()
        .map(|m| {
            json!({
                "id": m.manifest.id,
                "provides": m.manifest.provides,
                "name": m.manifest.name,
                "version": m.manifest.version,
                "profile": m.manifest.profile,
            })
        })
        .collect();
    let runtimes = io.runtimes();
    Json(json!({
        "runtimes": runtimes.snapshot(),
        "modules": modules,
        "problem": runtimes.problem(),
    }))
}

async fn command<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    id: &str,
    run: bool,
    cold: bool,
) -> Result<Json<Value>, ApiError> {
    let known = state.io_manager.as_ref().is_some_and(|io| {
        if cold {
            io.runtimes().cold_start(id)
        } else {
            io.runtimes().command(id, run)
        }
    });
    if !known {
        return Err(failure(
            StatusCode::NOT_FOUND,
            format!("no runtime '{id}' is loaded on this node"),
        ));
    }
    Ok(Json(json!({
        "id": id,
        "asked": if cold { "cold start" } else if run { "run" } else { "stop" },
        "note": "it takes effect at the runtime's next exchange; read GET /api/v1/runtimes for what it is doing",
    })))
}

/// `POST /api/v1/runtimes/:id/run` — put a loaded program in RUN. From STOP or FAULT that is
/// a warm restart (IEC 61131-3 §6.5.6): its RETAIN variables keep the values they had, the
/// others take their initial values.
pub async fn run<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    command(&state, &id, true, false).await
}

/// `POST /api/v1/runtimes/:id/cold-start` — start a program again from its initial values,
/// RETAIN variables included, and run it: a cold restart. A program in RUN is restarted too.
pub async fn cold_start<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    command(&state, &id, true, true).await
}

/// `POST /api/v1/runtimes/:id/stop` — put a program in STOP. Its outputs are written their
/// safe values and its published tags stop being current.
pub async fn stop<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    command(&state, &id, false, false).await
}

/// An answer no older than this is given at once; an older one is waited out. Sent with every
/// answer as `fresh_ms`: a client holding an answer longer than this past its `age_ms` holds
/// one this route would no longer give (ADR-013 §2.8).
const WATCH_FRESH: std::time::Duration = std::time::Duration::from_millis(1_000);
/// How long a watch waits for the runtime's next exchange.
const WATCH_WAIT: std::time::Duration = std::time::Duration::from_millis(2_500);

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchBody {
    variables: Vec<String>,
}

/// `POST /api/v1/runtimes/:runtime/watch` — the values of a program's variables as its last
/// scan left them, any variable, bound to a tag or not. It changes nothing in the program.
///
/// The runtime is asked at its next exchange, so the first answer takes up to one; asking
/// again within a few seconds keeps it reading, and gives what it last read at once.
/// `age_ms` says how long ago the runtime was read, `fresh_ms` how old an answer may be and
/// still be given at once, and `state.scan_age_us` how long before that read the program last
/// completed a scan (absent before its first scan, and from a module that does not say): a
/// program that has stopped scanning still answers, with its last values and a growing scan
/// age. A variable the program does not have, or one that has no single value (a whole array, a
/// structure), has an `error` instead of a `value`.
///
/// Each list of names is a watch of its own: callers with other lists do not disturb each
/// other, and callers with the same list share its answer. The runtime is read for every name
/// of every list watched of it, each once, at most [`WATCH_LIMIT`] names and at most
/// [`WATCH_LIMIT`] lists: a new list past either is refused with 409 and words, and nothing is
/// watched. A list nobody asks for again within [`crate::runtimes::WATCH_KEPT`] is let go.
pub async fn watch<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
    Json(body): Json<WatchBody>,
) -> Result<Json<Value>, ApiError> {
    if body.variables.is_empty() || body.variables.len() > WATCH_LIMIT {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            format!("a watch names between 1 and {WATCH_LIMIT} variables"),
        ));
    }
    let Some(io) = &state.io_manager else {
        return Err(failure(
            StatusCode::NOT_FOUND,
            format!("no runtime '{id}' is loaded on this node"),
        ));
    };
    let runtimes = io.runtimes();
    let answered = runtimes.answered();
    let deadline = tokio::time::Instant::now() + WATCH_WAIT;
    loop {
        // Registered before the state is read, so an answer between the two is not missed.
        let woken = answered.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        // Asked again on every turn: the instance may have been unloaded while waiting. Asking
        // again only renews a list already watched, which is never refused.
        match runtimes.watch(&id, &body.variables) {
            Ok(true) => {}
            Ok(false) => {
                return Err(failure(
                    StatusCode::NOT_FOUND,
                    format!("no runtime '{id}' is loaded on this node"),
                ))
            }
            Err(why) => return Err(failure(StatusCode::CONFLICT, why)),
        }
        if let Some((age, answer)) = runtimes.watched(&id, &body.variables) {
            if age <= WATCH_FRESH {
                return match answer {
                    Ok(reply) => Ok(Json(json!({
                        "id": id,
                        "state": reply.state,
                        "values": reply.values,
                        "age_ms": age.as_millis() as u64,
                        "fresh_ms": WATCH_FRESH.as_millis() as u64,
                    }))),
                    Err(why) => Err(failure(StatusCode::BAD_GATEWAY, why)),
                };
            }
        }
        if tokio::time::timeout_at(deadline, woken).await.is_err() {
            return Err(failure(
                StatusCode::GATEWAY_TIMEOUT,
                format!(
                    "runtime '{id}' gave no values within {} ms",
                    WATCH_WAIT.as_millis()
                ),
            ));
        }
    }
}

/// How long a force or a release waits for the runtime's next exchange.
const FORCE_WAIT: std::time::Duration = std::time::Duration::from_millis(2_500);

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForceBody {
    variable: String,
    value: Value,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ReleaseBody {
    #[serde(default)]
    variable: Option<String>,
}

fn who(identity: Option<Extension<crate::auth::Identity>>) -> String {
    identity
        .map(|Extension(i)| format!("{} ({})", i.username, i.via))
        .unwrap_or_else(|| "an unauthenticated caller".to_string())
}

/// Carry a force or a release to the runtime at its next exchange, and give its answer.
async fn forcing<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    id: &str,
    ask: ForceAsk,
    who: &str,
) -> Result<Json<Value>, ApiError> {
    let what = match &ask {
        ForceAsk::Force { variable, value } => format!("force {variable} to {value}"),
        ForceAsk::Release { variable: Some(v) } => format!("release the force of {v}"),
        ForceAsk::Release { variable: None } => "release every force".to_string(),
    };
    let refuse = |status: StatusCode, why: String| {
        tracing::warn!(target: "mythos::audit", "{who} could not {what} in runtime '{id}': {why}");
        failure(status, why)
    };
    let Some(answer) = state
        .io_manager
        .as_ref()
        .and_then(|io| io.runtimes().force(id, ask.clone()))
    else {
        return Err(refuse(
            StatusCode::NOT_FOUND,
            format!("no runtime '{id}' is loaded on this node"),
        ));
    };
    // Past the wait the receiver is dropped, and an ask nobody waits for is not carried out.
    match tokio::time::timeout(FORCE_WAIT, answer).await {
        Ok(Ok(Ok(runtime))) => {
            tracing::info!(target: "mythos::audit", "{who} did {what} in runtime '{id}'");
            Ok(Json(json!({ "id": id, "state": runtime })))
        }
        Ok(Ok(Err(ForceRefusal::Refused(why)))) => Err(refuse(StatusCode::CONFLICT, why)),
        Ok(Ok(Err(ForceRefusal::Unreachable(why)))) => Err(refuse(StatusCode::BAD_GATEWAY, why)),
        Ok(Ok(Err(ForceRefusal::Unloaded))) | Ok(Err(_)) => Err(refuse(
            StatusCode::NOT_FOUND,
            format!("runtime '{id}' was unloaded before it was asked"),
        )),
        Err(_) => Err(refuse(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "runtime '{id}' had no exchange within {} ms, so it was not asked and nothing changed",
                FORCE_WAIT.as_millis()
            ),
        )),
    }
}

/// `POST /api/v1/runtimes/:runtime/force` — hold a variable of a running program at a value.
/// Whatever its inputs and the program say, it reads and is written as that value: an output
/// bound to it is written it, a published tag gives it, an operator's setpoint to it is
/// refused. It holds until released, until the program leaves RUN (stopped, faulted), or until
/// a program is loaded again. Only a BOOL or a number can be forced.
pub async fn force<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
    identity: Option<Extension<crate::auth::Identity>>,
    Json(body): Json<ForceBody>,
) -> Result<Json<Value>, ApiError> {
    if !(body.value.is_number() || body.value.is_boolean()) {
        return Err(failure(
            StatusCode::UNPROCESSABLE_ENTITY,
            "nothing was forced: the value must be a number or a boolean",
        ));
    }
    let ask = ForceAsk::Force {
        variable: body.variable,
        value: body.value,
    };
    forcing(&state, &id, ask, &who(identity)).await
}

/// `POST /api/v1/runtimes/:runtime/release` — release one forced variable (`variable`), or
/// every one (no `variable`). The program decides its value again from its next scan.
pub async fn release<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(id): Path<String>,
    identity: Option<Extension<crate::auth::Identity>>,
    Json(body): Json<ReleaseBody>,
) -> Result<Json<Value>, ApiError> {
    let ask = ForceAsk::Release {
        variable: body.variable,
    };
    forcing(&state, &id, ask, &who(identity)).await
}

/// `GET /api/v1/projects/:id/runtimes` — the project's runtimes, or `null` when it has none.
pub async fn get_config<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let path = config_path(&state, &project)?;
    let config =
        RuntimesConfig::load(&path).map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "config": config })))
}

/// How `PUT /api/v1/projects/:id/runtimes` starts what it loads.
#[derive(serde::Deserialize)]
pub struct LoadQuery {
    /// `cold`: every runtime it starts or replaces starts from its initial values, RETAIN
    /// variables included. Left out, a new build of a running program is changed online.
    start: Option<String>,
}

/// `PUT /api/v1/projects/:id/runtimes` — check, save and, for the active project, apply.
///
/// Each instance is checked by the runtime module it names, which loads the program without
/// running it: a program that does not load, or a binding to a variable it does not have, is
/// refused here and nothing is written. For the active project the runtimes are then made to
/// match: an instance whose published tags stay the same is replaced in place. A new build of
/// a program in RUN takes over its variables (changed online) unless `?start=cold`; how each
/// runtime started is in `changes.runtimes`.
pub async fn put_config<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(project): Path<String>,
    Query(query): Query<LoadQuery>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let cold = match query.start.as_deref() {
        None => false,
        Some("cold") => true,
        Some(other) => {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                format!(
                    "start={other} is not a way to start: 'cold' starts every program it loads from its initial values; leave it out to change a running program online"
                ),
            ))
        }
    };
    let path = config_path(&state, &project)?;
    let config: RuntimesConfig = serde_json::from_value(body).map_err(|e| {
        failure(
            StatusCode::BAD_REQUEST,
            format!("not a runtimes configuration: {e}"),
        )
    })?;
    config
        .validate()
        .map_err(|e| failure(StatusCode::BAD_REQUEST, e))?;

    let Some(io) = &state.io_manager else {
        return Err(failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server has no field I/O manager, so it can check and run no program",
        ));
    };
    let problems: Vec<ConfigProblem> = io.check_runtimes(&config).await;
    if !problems.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "the runtimes configuration has problems and was not saved",
                "problems": problems,
            })),
        ));
    }

    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state
        .storage
        .atomic_write_file(&path, &bytes)
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let is_active = matches!(
        state.storage.get_active_project(),
        Ok(Some(active)) if active == project
    );
    let (applied, changes) = if is_active {
        (true, io.reload_with(cold).await)
    } else {
        (false, None)
    };
    Ok(Json(
        json!({ "config": config, "applied": applied, "changes": changes }),
    ))
}
