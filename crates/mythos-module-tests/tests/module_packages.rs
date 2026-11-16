//! Settings → Modules (ADR-011 §2.4) through the real router with accounts: installing module
//! packages (and refusing bad ones), switching modules off and on, removing them. The modules are
//! real executables: the notes service and the Modbus driver.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use driver_modbus::sim::ModbusSimServer;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::io::{DriverRegistry, IoManager};
use mythos_core::modules::{pack, ModuleSettings, TARGET};
use mythos_core::services::ServiceHost;
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_pal::DefaultShm;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const NOTES: &str = env!("CARGO_BIN_EXE_service-module-under-test");
const MODBUS: &str = env!("CARGO_BIN_EXE_modbus-module-under-test");
static SEQ: AtomicU32 = AtomicU32::new(0);

fn exe_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn manifest_of(exe: &str) -> Value {
    let out = std::process::Command::new(exe)
        .arg("--manifest")
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

/// A package of the notes module, laid out and packed as `mythos module pack` does.
fn notes_package(change: impl FnOnce(&Path)) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("module.json"),
        manifest_of(NOTES).to_string(),
    )
    .unwrap();
    let bin = dir.path().join("bin").join(TARGET);
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(NOTES, bin.join(exe_name("mythos-module-notes"))).unwrap();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(dir.path().join("ui").join("entry.js"), "export default {}").unwrap();
    change(dir.path());
    pack(dir.path()).unwrap()
}

/// Rewrite a package, changing one file's bytes but not its listed checksum.
fn tamper(package: &[u8], name: &str) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).unwrap();
            let entry_name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if entry_name == name {
                bytes.push(0);
            }
            writer
                .start_file(
                    entry_name,
                    zip::write::FileOptions::default().large_file(true),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    out.into_inner()
}

struct Rig {
    app: Router,
    io: Arc<IoManager<DefaultShm>>,
    install_dir: PathBuf,
    system_dir: PathBuf,
    admin: String,
    viewer: String,
    _dirs: Vec<tempfile::TempDir>,
}

/// A server as `server::assemble` builds one: its module directories are `extra` (the MythOS
/// installation's own) and the install directory.
async fn rig(extra: Option<&Path>) -> Rig {
    let root = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(root.path().join("projects")).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    storage.activate_project("plant").unwrap();
    let settings = Arc::new(ModuleSettings::open(&storage.system_dir()).unwrap());
    let install_dir = settings.install_dir();

    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    let (_, admin) = auth.set_up(&code, "root", "", "root password 1").unwrap();
    auth.create_user("vera", "", Role::Viewer, "a good password")
        .unwrap();
    let (_, viewer) = auth.sign_in("vera", "a good password").unwrap();

    let mut dirs: Vec<PathBuf> = extra.map(Path::to_path_buf).into_iter().collect();
    dirs.push(install_dir.clone());
    let mut registry = DriverRegistry::discover(&dirs).await;
    registry.apply_disabled(&settings.disabled());
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let io = IoManager::<DefaultShm>::with_module_dirs(
        Arc::clone(&storage),
        Arc::clone(&catalog),
        format!(
            "mythos_pkg_test_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ),
        16,
        registry,
        dirs,
    );
    io.use_module_settings(Arc::clone(&settings));
    let state = ApiState::<DefaultShm>::new(Arc::clone(&storage), catalog, None);
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(io.service_modules()).await;
    {
        let services = Arc::clone(&services);
        io.when_services_change(Arc::new(move |modules| {
            let services = Arc::clone(&services);
            tokio::spawn(async move { services.sync(modules).await });
        }));
    }
    let state = state
        .with_io_manager(Arc::clone(&io))
        .with_auth(auth)
        .with_services(services)
        .with_modules(settings);
    Rig {
        app: create_router(state),
        io,
        install_dir,
        system_dir: storage.system_dir(),
        admin,
        viewer,
        _dirs: vec![root],
    }
}

impl Rig {
    async fn send(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Option<Body>,
        json: bool,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("mythos_session={session}"));
        if json {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(body.unwrap_or_else(Body::empty)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 24)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// A GET answered with something other than JSON: status, content type and body.
    async fn get_raw(&self, session: Option<&str>, uri: &str) -> (StatusCode, String, Vec<u8>) {
        let mut request = Request::builder().method("GET").uri(uri);
        if let Some(session) = session {
            request = request.header(header::COOKIE, format!("mythos_session={session}"));
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let kind = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 24)
            .await
            .unwrap();
        (status, kind, bytes.to_vec())
    }

    async fn ui_row(&self) -> Value {
        let (_, list) = self
            .send(&self.viewer, "GET", "/api/v1/modules", None, false)
            .await;
        list["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "org.mythos.test.notes")
            .map(|m| m["ui"].clone())
            .unwrap_or(Value::Null)
    }

    async fn install(&self, session: &str, package: Vec<u8>) -> (StatusCode, Value) {
        self.send(
            session,
            "POST",
            "/api/v1/modules/install",
            Some(Body::from(package)),
            false,
        )
        .await
    }

    async fn notes(&self) -> StatusCode {
        self.send(
            &self.viewer,
            "GET",
            "/api/v1/projects/plant/notes/notes",
            None,
            false,
        )
        .await
        .0
    }

    fn installed_files(&self) -> Vec<String> {
        std::fs::read_dir(&self.install_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_administrator_installs_a_package_and_its_module_is_used_at_once() {
    let rig = rig(None).await;
    assert_eq!(rig.notes().await, StatusCode::NOT_FOUND);

    let (status, _) = rig.install(&rig.viewer, notes_package(|_| {})).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "only an administrator installs"
    );
    assert!(rig.installed_files().is_empty());

    let (status, body) = rig.install(&rig.admin, notes_package(|_| {})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["installed"], "org.mythos.test.notes");
    assert_eq!(
        rig.notes().await,
        StatusCode::OK,
        "the module serves right after installing"
    );
    assert!(rig
        .install_dir
        .join(exe_name("mythos-module-notes"))
        .is_file());
    assert!(
        rig.system_dir
            .join("modules-ui")
            .join("org.mythos.test.notes")
            .join("entry.js")
            .is_file(),
        "its UI is kept for the workbench"
    );
    let (_, list) = rig
        .send(&rig.viewer, "GET", "/api/v1/modules", None, false)
        .await;
    let row = list["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "org.mythos.test.notes")
        .unwrap()
        .clone();
    assert_eq!(
        (row["enabled"].as_bool(), row["removable"].as_bool()),
        (Some(true), Some(true))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_that_is_not_what_it_says_is_refused_and_nothing_is_installed() {
    let rig = rig(None).await;
    let exe = format!("bin/{TARGET}/{}", exe_name("mythos-module-notes"));
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "a file changed after packing",
            tamper(&notes_package(|_| {}), &exe),
            "does not match its checksum",
        ),
        (
            "no build for this platform",
            notes_package(|dir| {
                std::fs::rename(
                    dir.join("bin").join(TARGET),
                    dir.join("bin").join("riscv64gc-unknown-none-elf"),
                )
                .unwrap();
            }),
            "no build for this platform",
        ),
        (
            "module.json says another version than the executable",
            notes_package(|dir| {
                let mut m = manifest_of(NOTES);
                m["version"] = json!("9.9.9");
                std::fs::write(dir.join("module.json"), m.to_string()).unwrap();
            }),
            "its executable says it is",
        ),
        ("not a zip", b"not a package".to_vec(), "not a zip"),
    ];
    for (what, package, expected) in cases {
        let (status, body) = rig.install(&rig.admin, package).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{what}: {body}");
        assert!(
            body["error"].as_str().unwrap().contains(expected),
            "{what}: {body}"
        );
        assert!(
            rig.installed_files().is_empty(),
            "{what}: something was installed: {:?}",
            rig.installed_files()
        );
    }
    assert_eq!(rig.notes().await, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_entry_outside_its_directory_is_refused() {
    let rig = rig(None).await;
    let package = notes_package(|_| {});
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).unwrap();
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            writer
                .start_file(name, zip::write::FileOptions::default().large_file(true))
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer
            .start_file("../../escaped.txt", zip::write::FileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
    }
    let (status, body) = rig.install(&rig.admin, out.into_inner()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("outside"),
        "{body}"
    );
    assert!(rig.installed_files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_service_answers_that_it_is_disabled_and_comes_back_when_enabled() {
    let rig = rig(None).await;
    rig.install(&rig.admin, notes_package(|_| {})).await;
    let uri = "/api/v1/modules/org.mythos.test.notes/enabled";
    let off = Some(Body::from(json!({"enabled": false}).to_string()));
    assert_eq!(
        rig.send(&rig.viewer, "PUT", uri, off, true).await.0,
        StatusCode::FORBIDDEN
    );

    let off = Some(Body::from(json!({"enabled": false}).to_string()));
    let (status, body) = rig.send(&rig.admin, "PUT", uri, off, true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = rig
        .send(
            &rig.viewer,
            "GET",
            "/api/v1/projects/plant/notes/notes",
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body["error"].as_str().unwrap().contains("disabled"),
        "{body}"
    );

    let on = Some(Body::from(json!({"enabled": true}).to_string()));
    rig.send(&rig.admin, "PUT", uri, on, true).await;
    assert_eq!(rig.notes().await, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_an_installed_module_stops_it_and_deletes_it_and_one_of_the_installation_is_refused(
) {
    let installation = tempfile::tempdir().unwrap();
    std::fs::copy(
        MODBUS,
        installation.path().join(exe_name("mythos-module-modbus")),
    )
    .unwrap();
    let rig = rig(Some(installation.path())).await;
    rig.install(&rig.admin, notes_package(|_| {})).await;
    assert_eq!(
        rig.notes().await,
        StatusCode::OK,
        "running before it is removed"
    );

    let (status, body) = rig
        .send(
            &rig.admin,
            "DELETE",
            "/api/v1/modules/org.mythos.test.notes",
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!rig
        .install_dir
        .join(exe_name("mythos-module-notes"))
        .exists());
    assert!(!rig
        .system_dir
        .join("modules-ui")
        .join("org.mythos.test.notes")
        .exists());
    assert_eq!(rig.notes().await, StatusCode::NOT_FOUND);

    let (status, body) = rig
        .send(
            &rig.admin,
            "DELETE",
            "/api/v1/modules/org.mythos.driver.modbus",
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(installation
        .path()
        .join(exe_name("mythos-module-modbus"))
        .exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_driver_stops_its_connections_and_says_why() {
    let installation = tempfile::tempdir().unwrap();
    std::fs::copy(
        MODBUS,
        installation.path().join(exe_name("mythos-module-modbus")),
    )
    .unwrap();
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let rig = rig(Some(installation.path())).await;
    let io_file = rig
        .system_dir
        .parent()
        .unwrap()
        .join("plant")
        .join("io.json");
    std::fs::write(
        &io_file,
        json!({"connections": [{"id": "plc1", "driver": "modbus", "poll_interval_ms": 100, "config": {
            "transport": "tcp", "host": "127.0.0.1", "port": sim.port(),
            "mappings": [{"uri": "external.Plant.PT101.pressure", "address": 0, "function_code": 3}]
        }}]})
        .to_string(),
    )
    .unwrap();
    rig.io.reload().await;
    let state = |rig: &Rig| rig.io.status().snapshot().connections[0].clone();
    assert_eq!(format!("{:?}", state(&rig).state), "Running");

    let uri = "/api/v1/modules/org.mythos.driver.modbus/enabled";
    rig.send(
        &rig.admin,
        "PUT",
        uri,
        Some(Body::from(json!({"enabled": false}).to_string())),
        true,
    )
    .await;
    let now = state(&rig);
    assert_eq!(format!("{:?}", now.state), "ConfigError");
    assert!(
        now.detail.unwrap_or_default().contains("disabled"),
        "the reason is given"
    );

    rig.send(
        &rig.admin,
        "PUT",
        uri,
        Some(Body::from(json!({"enabled": true}).to_string())),
        true,
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while format!("{:?}", state(&rig).state) != "Running" {
        assert!(Instant::now() < deadline, "the connection never came back");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_workbench_loads_an_enabled_module_ui_and_nothing_else() {
    let rig = rig(None).await;
    let (status, body) = rig
        .install(
            &rig.admin,
            notes_package(|dir| {
                std::fs::create_dir_all(dir.join("ui").join("assets")).unwrap();
                std::fs::write(dir.join("ui").join("assets").join("notes.css"), "p{}").unwrap();
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let entry = "/modules/org.mythos.test.notes/ui/entry.js";
    assert_eq!(
        rig.ui_row().await,
        json!(entry),
        "the list says where its UI is"
    );

    let (status, kind, bytes) = rig.get_raw(Some(&rig.viewer), entry).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        kind.contains("javascript"),
        "an ES module needs a script type, not {kind}"
    );
    assert_eq!(bytes, b"export default {}");
    let (status, kind, _) = rig
        .get_raw(
            Some(&rig.viewer),
            "/modules/org.mythos.test.notes/ui/assets/notes.css",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(kind.starts_with("text/css"), "{kind}");

    // Only someone signed in reads it, and only from inside the module's own UI directory.
    assert_eq!(rig.get_raw(None, entry).await.0, StatusCode::UNAUTHORIZED);
    for outside in [
        "/modules/org.mythos.test.notes/ui/..%2F..%2Fmodules.json",
        "/modules/org.mythos.test.notes/ui/../../modules.json",
        "/modules/org.mythos.test.notes/ui/%2Fetc%2Fpasswd",
    ] {
        let (status, _, bytes) = rig.get_raw(Some(&rig.viewer), outside).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
            "{outside} answered {status}: {}",
            String::from_utf8_lossy(&bytes)
        );
    }
    assert_eq!(
        rig.get_raw(
            Some(&rig.viewer),
            "/modules/org.mythos.test.notes/ui/missing.js"
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // A module without a UI has none to load.
    assert_eq!(
        rig.get_raw(
            Some(&rig.viewer),
            "/modules/org.mythos.driver.modbus/ui/entry.js"
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Disabled, its UI is neither offered nor served.
    let off = Some(Body::from(json!({"enabled": false}).to_string()));
    rig.send(
        &rig.admin,
        "PUT",
        "/api/v1/modules/org.mythos.test.notes/enabled",
        off,
        true,
    )
    .await;
    assert_eq!(rig.ui_row().await, Value::Null);
    assert_eq!(
        rig.get_raw(Some(&rig.viewer), entry).await.0,
        StatusCode::NOT_FOUND
    );
}
