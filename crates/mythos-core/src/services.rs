//! Service modules (ADR-011): running them, and forwarding requests to their routes.
//!
//! A service module is found like a driver (ADR-010), by its manifest's `kind`. The host keeps
//! one process per service, started on first use, started again when it has died (no more often
//! than every two seconds), and replaced when a new build of it is installed: the new process is
//! started and greeted first, and only then takes the requests; the old one finishes the
//! requests it has and is stopped.
//!
//! What a module may be asked is decided here, before it sees anything: the route must be one
//! its manifest declares, the caller must have the role it declares (none declared is
//! administrators only), and a path parameter can never name another directory.

use crate::api::state::HmiEvent;
use crate::auth::Role;
use mythos_module::protocol::{HttpCall, HttpReply, RouteDecl, RouteScope, WireEvent};
use mythos_module::{ModuleBinary, ServiceSession};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{info, warn};

/// The least time between two starts of a service that keeps dying.
pub const RESTART_BACKOFF: Duration = Duration::from_secs(2);

/// What tells a service which node it runs on (ADR-012): read each time it is told.
pub type NodeSource = Arc<dyn Fn() -> Option<mythos_module::protocol::WireNode> + Send + Sync>;
type SharedNode = Arc<RwLock<Option<NodeSource>>>;

/// One service module and its process.
pub struct RunningService {
    pub module: Arc<ModuleBinary>,
    session: Mutex<Slot>,
    events: mpsc::UnboundedSender<WireEvent>,
    node: SharedNode,
    /// Set when it is replaced or removed: an always-on service is not started again then.
    stopped: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Slot {
    session: Option<Arc<ServiceSession>>,
    last_start: Option<Instant>,
    starts: u32,
    last_error: Option<String>,
}

/// What `GET /api/v1/modules` says about a service.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub provides: String,
    pub id: String,
    pub version: String,
    pub path: String,
    pub pid: Option<u32>,
    /// Processes started so far, the first included.
    pub starts: u32,
    pub last_error: Option<String>,
    pub routes: Vec<RouteDecl>,
}

impl RunningService {
    fn new(
        module: Arc<ModuleBinary>,
        hmi: broadcast::Sender<HmiEvent>,
        node: SharedNode,
    ) -> Arc<Self> {
        let (events, mut rx) = mpsc::unbounded_channel::<WireEvent>();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let _ = hmi.send(HmiEvent::new(event.name, event.data));
            }
        });
        Arc::new(Self {
            module,
            session: Mutex::new(Slot::default()),
            events,
            node,
            stopped: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn current_node(&self) -> Option<mythos_module::protocol::WireNode> {
        let source = self.node.read().unwrap_or_else(|e| e.into_inner()).clone();
        source.and_then(|source| source())
    }

    /// Tell the running process which node it runs on, if it is running.
    async fn tell_node(&self) {
        let session = self.session.lock().await.session.clone();
        if let (Some(session), Some(node)) = (session.filter(|s| s.alive()), self.current_node()) {
            if let Err(e) = session.node(node).await {
                warn!(
                    "service module '{}' was not told the node: {e}",
                    self.module.manifest.provides
                );
            }
        }
    }

    /// Keep an always-on service running: start it now, and again whenever it has stopped,
    /// until it is replaced or removed.
    fn keep_running(self: &Arc<Self>) {
        let service = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(service) = service.upgrade() else {
                    break;
                };
                if service.stopped.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = service.session().await {
                    warn!(
                        "always-on service module '{}' cannot start: {e}",
                        service.module.manifest.provides
                    );
                }
                drop(service);
                tokio::time::sleep(RESTART_BACKOFF).await;
            }
        });
    }

    /// The running process, started now if there is none or it has died.
    async fn session(&self) -> Result<Arc<ServiceSession>, String> {
        let mut slot = self.session.lock().await;
        if let Some(session) = &slot.session {
            if session.alive() {
                return Ok(Arc::clone(session));
            }
        }
        if let Some(last) = slot.last_start {
            let since = last.elapsed();
            if since < RESTART_BACKOFF {
                tokio::time::sleep(RESTART_BACKOFF - since).await;
            }
        }
        slot.last_start = Some(Instant::now());
        slot.starts += 1;
        match ServiceSession::start(&self.module, self.events.clone()).await {
            Ok(session) => {
                info!(
                    "service module '{}' {} started (pid {:?})",
                    self.module.manifest.provides,
                    self.module.manifest.version,
                    session.pid()
                );
                let session = Arc::new(session);
                // Every new process is told the node before it is asked anything.
                if let Some(node) = self.current_node() {
                    if let Err(e) = session.node(node).await {
                        warn!(
                            "service module '{}' was not told the node: {e}",
                            self.module.manifest.provides
                        );
                    }
                }
                slot.session = Some(Arc::clone(&session));
                slot.last_error = None;
                Ok(session)
            }
            Err(e) => {
                slot.last_error = Some(e.clone());
                Err(e)
            }
        }
    }

    async fn status(&self) -> ServiceStatus {
        let slot = self.session.lock().await;
        let m = &self.module.manifest;
        ServiceStatus {
            provides: m.provides.clone(),
            id: m.id.clone(),
            version: m.version.clone(),
            path: self.module.path.display().to_string(),
            pid: slot
                .session
                .as_ref()
                .filter(|s| s.alive())
                .and_then(|s| s.pid()),
            starts: slot.starts,
            last_error: slot.last_error.clone(),
            routes: m.routes.clone(),
        }
    }

    /// Stop the process, letting it finish what it has.
    async fn stop(&self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let session = self.session.lock().await.session.take();
        if let Some(session) = session {
            // Requests still in flight hold it; the process is killed (kill_on_drop) when the
            // last of them is done with it.
            if let Ok(session) = Arc::try_unwrap(session) {
                session.close().await;
            }
        }
    }
}

/// Every service module of this server.
pub struct ServiceHost {
    services: RwLock<BTreeMap<String, Arc<RunningService>>>,
    hmi: broadcast::Sender<HmiEvent>,
    node: SharedNode,
}

impl ServiceHost {
    pub fn new(hmi: broadcast::Sender<HmiEvent>) -> Arc<Self> {
        Arc::new(Self {
            services: RwLock::new(BTreeMap::new()),
            hmi,
            node: Arc::new(RwLock::new(None)),
        })
    }

    /// Where the services learn which node they run on (ADR-012). Set before the first sync.
    pub fn set_node(&self, source: NodeSource) {
        *self.node.write().unwrap_or_else(|e| e.into_inner()) = Some(source);
    }

    /// The node changed (renamed, another role, now listening): tell every running service.
    pub async fn node_changed(&self) {
        for service in self.all() {
            service.tell_node().await;
        }
    }

    pub fn get(&self, provides: &str) -> Option<Arc<RunningService>> {
        self.services
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(provides)
            .cloned()
    }

    pub fn all(&self) -> Vec<Arc<RunningService>> {
        self.services
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub async fn statuses(&self) -> Vec<ServiceStatus> {
        let mut out = Vec::new();
        for service in self.all() {
            out.push(service.status().await);
        }
        out
    }

    /// Make the running services the ones in `modules`. A service whose build changed is
    /// replaced: the new process is started and greeted before it takes requests, and only then
    /// is the old one stopped. One that is gone is stopped.
    pub async fn sync(&self, modules: Vec<Arc<ModuleBinary>>) {
        let wanted: BTreeMap<String, Arc<ModuleBinary>> = modules
            .into_iter()
            .map(|m| (m.manifest.provides.clone(), m))
            .collect();
        let current: BTreeMap<String, Arc<RunningService>> = self
            .services
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        for (name, module) in &wanted {
            let same = current.get(name).is_some_and(|running| {
                running.module.path == module.path
                    && running.module.fingerprint == module.fingerprint
                    && running.module.manifest == module.manifest
            });
            if same {
                continue;
            }
            let next =
                RunningService::new(Arc::clone(module), self.hmi.clone(), Arc::clone(&self.node));
            if let Some(old) = current.get(name) {
                // Start the new build first. If it cannot start, the old one keeps serving.
                if let Err(e) = next.session().await {
                    warn!(
                        "a new build of service module '{name}' cannot start; the running one is kept: {e}"
                    );
                    continue;
                }
                self.services
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(name.clone(), Arc::clone(&next));
                info!(
                    "service module '{name}' swapped to {}",
                    module.manifest.version
                );
                old.stop().await;
                if module.manifest.always_on {
                    next.keep_running();
                }
            } else {
                self.services
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(name.clone(), Arc::clone(&next));
                info!("service module '{name}' {} found", module.manifest.version);
                if module.manifest.always_on {
                    next.keep_running();
                }
            }
        }

        for (name, old) in current {
            if !wanted.contains_key(&name) {
                self.services
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&name);
                info!("service module '{name}' is gone; stopping it");
                old.stop().await;
            }
        }
    }

    /// Stop every service, for process shutdown.
    pub async fn shutdown(&self) {
        for service in self.all() {
            service.stop().await;
        }
    }
}

/// The parameters of `path` when it matches the declared `pattern` (`/a/:b`), segment by
/// segment.
pub fn match_route(pattern: &str, path: &str) -> Option<BTreeMap<String, String>> {
    let pattern: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let path: Vec<&str> = path.trim_matches('/').split('/').collect();
    if pattern.len() != path.len() {
        return None;
    }
    let mut params = BTreeMap::new();
    for (p, v) in pattern.iter().zip(path.iter()) {
        if let Some(name) = p.strip_prefix(':') {
            if v.is_empty() {
                return None;
            }
            params.insert(name.to_string(), (*v).to_string());
        } else if p != v {
            return None;
        }
    }
    Some(params)
}

/// The least role a declared route needs. Absent or not a role is administrators only.
pub fn declared_role(decl: &RouteDecl) -> Role {
    match decl.role.as_deref() {
        Some("viewer") => Role::Viewer,
        Some("operator") => Role::Operator,
        Some("engineer") => Role::Engineer,
        _ => Role::Admin,
    }
}

/// A path parameter that could name a place outside where it is meant to: a separator, or a
/// `.`/`..` segment. The router has already decoded it, so `%2F` arrives as `/`.
pub fn unsafe_param(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || value.contains('\0')
}

/// A request to forward, before the module is asked.
pub struct Forward {
    pub scope: RouteScope,
    pub method: String,
    pub rest: String,
    pub query: BTreeMap<String, String>,
    pub body: Option<serde_json::Value>,
    pub identity: Option<crate::auth::Identity>,
    /// `(project id, its directory)` for a project route.
    pub project: Option<(String, PathBuf)>,
    /// The tag catalog, read for a route that declares `needs: ["tags"]`.
    pub catalog: Option<Arc<tokio::sync::RwLock<crate::tag::TagCatalog>>>,
}

/// What forwarding answered, as an HTTP status and body.
pub async fn forward(service: &RunningService, request: Forward) -> HttpReply {
    let module = &service.module.manifest;
    // The router has decoded the path already, so an encoded separator is a separator here: a
    // `.`/`..` segment is refused whatever route it would have matched.
    if request
        .rest
        .split('/')
        .any(|segment| segment == "." || segment == ".." || segment.contains('\\'))
    {
        return HttpReply::error(400, "a path may not contain '.', '..' or '\\' segments");
    }
    let found = module.routes.iter().find_map(|decl| {
        (decl.scope == request.scope && decl.method.eq_ignore_ascii_case(&request.method))
            .then(|| match_route(&decl.path, &request.rest).map(|p| (decl, p)))
            .flatten()
    });
    let Some((decl, params)) = found else {
        return HttpReply::error(
            404,
            format!(
                "module '{}' serves no {} {}",
                module.provides, request.method, request.rest
            ),
        );
    };
    if let Some((name, _)) = params.iter().find(|(_, v)| unsafe_param(v)) {
        return HttpReply::error(
            400,
            format!("'{name}' may not contain a path separator or '..'"),
        );
    }
    if let Some(who) = &request.identity {
        let needed = declared_role(decl);
        if who.role < needed {
            return HttpReply::error(
                403,
                format!(
                    "this needs the {needed} role; {} is {}",
                    who.username, who.role
                ),
            );
        }
    }
    let (project, data_dir) = match &request.project {
        Some((id, dir)) => {
            let data = dir.join(&module.provides);
            if let Err(e) = std::fs::create_dir_all(&data) {
                return HttpReply::error(
                    500,
                    format!("the module's data directory cannot be made: {e}"),
                );
            }
            (Some(id.clone()), Some(data.display().to_string()))
        }
        None => (None, None),
    };
    if let Some(unknown) = decl
        .needs
        .iter()
        .find(|need| !mythos_module::protocol::ROUTE_NEEDS.contains(&need.as_str()))
    {
        return HttpReply::error(
            500,
            format!(
                "module '{}' needs '{unknown}' for {} {}, which this server cannot give",
                module.provides, decl.method, decl.path
            ),
        );
    }
    let tags = if decl.needs.iter().any(|need| need == "tags") {
        match &request.catalog {
            Some(catalog) => Some(
                catalog
                    .read()
                    .await
                    .iter()
                    .map(|meta| meta.uri.clone())
                    .collect(),
            ),
            // The module asked for the tags and this server cannot give them: an empty list
            // would read as "no tag exists" and refuse every binding for the wrong reason.
            None => {
                return HttpReply::error(503, "this server has no tag catalog to give the module")
            }
        }
    } else {
        None
    };
    let call = HttpCall {
        method: decl.method.to_ascii_uppercase(),
        route: decl.path.clone(),
        params,
        query: request.query,
        body: request.body,
        identity: request
            .identity
            .as_ref()
            .map(|i| mythos_module::protocol::WireIdentity {
                username: i.username.clone(),
                role: i.role.to_string(),
            }),
        project,
        data_dir,
        tags,
    };
    let session = match service.session().await {
        Ok(session) => session,
        Err(e) => {
            return HttpReply::error(
                502,
                format!("module '{}' is not running: {e}", module.provides),
            )
        }
    };
    match session.http(call).await {
        Ok(reply) => reply,
        Err(e) => HttpReply::error(
            502,
            format!("module '{}' did not answer: {e}", module.provides),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_matches_segment_by_segment_and_names_its_parameters() {
        let p = match_route("/notes/:note", "/notes/boiler").unwrap();
        assert_eq!(p.get("note").map(String::as_str), Some("boiler"));
        assert!(match_route("/notes/:note", "/notes").is_none());
        assert!(match_route("/notes/:note", "/notes/a/b").is_none());
        assert!(match_route("/notes", "/other").is_none());
        assert!(match_route("/notes/:note", "/notes/").is_none());
    }

    #[test]
    fn a_parameter_can_never_name_another_directory() {
        for bad in ["../x", "..", ".", "a/b", "a\\b"] {
            assert!(unsafe_param(bad), "{bad}");
        }
        assert!(!unsafe_param("boiler-1.v2"));
    }

    #[test]
    fn a_route_without_a_role_is_for_administrators_only() {
        let decl = |role: Option<&str>| RouteDecl {
            method: "GET".into(),
            path: "/x".into(),
            scope: RouteScope::Global,
            role: role.map(str::to_string),
            tool: "x".into(),
            summary: String::new(),
            body: None,
            query: vec![],
            needs: vec![],
        };
        assert_eq!(declared_role(&decl(None)), Role::Admin);
        assert_eq!(declared_role(&decl(Some("superuser"))), Role::Admin);
        assert_eq!(declared_role(&decl(Some("viewer"))), Role::Viewer);
    }
}
