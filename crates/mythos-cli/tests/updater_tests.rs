//! `mythos update`, `versions --remote` and `rollback` against a release feed served on a local
//! socket, in the shape GitHub's releases API answers, with real bundles: zip files holding this
//! build's own `mythos` executable, checked by SHA-256.
//!
//! Every version a test expects comes from the feed the test serves; none is written into the
//! CLI.

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::install::{current_version, exe, is_installed, version_dir};
use mythos_cli::release::TARGET;
use mythos_cli::rollback::{RollbackError, RollbackManager};
use mythos_cli::update::{UpdateError, UpdateManager};
use mythos_cli::{execute_command, Cli, Parser};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MYTHOS: &str = env!("CARGO_BIN_EXE_mythos");

/// What the feed serves: the release list, and files by name.
#[derive(Default)]
struct Published {
    releases: Vec<Value>,
    files: HashMap<String, Vec<u8>>,
}

struct Feed {
    url: String,
    base: String,
    published: Arc<Mutex<Published>>,
    _thread: std::thread::JoinHandle<()>,
}

impl Feed {
    fn start() -> Self {
        let published = Arc::new(Mutex::new(Published::default()));
        let state = Arc::clone(&published);
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let app = Router::new()
                    .route("/releases", get(releases))
                    .route("/files/:name", get(file))
                    .with_state(state);
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        let addr = rx.recv().unwrap();
        let base = format!("http://{addr}");
        Self {
            url: format!("{base}/releases"),
            base,
            published,
            _thread: thread,
        }
    }

    /// Publish a release whose bundle for this platform is `bundle`, listed in its
    /// `SHA256SUMS` with `listed_hash` (the bundle's own hash when `None`).
    fn publish(&self, tag: &str, bundle: Option<Vec<u8>>, listed_hash: Option<&str>) {
        let version = tag.trim_start_matches('v');
        let name = format!("mythos-{version}-{TARGET}.zip");
        let sums_name = format!("{tag}-SHA256SUMS");
        let mut assets = Vec::new();
        let mut published = self.published.lock().unwrap();
        if let Some(bundle) = bundle {
            let hash = listed_hash
                .map(str::to_string)
                .unwrap_or_else(|| sha256(&bundle));
            let file = format!("{tag}-{name}");
            assets.push(json!({"name": name, "browser_download_url": format!("{}/files/{file}", self.base)}));
            assets.push(json!({"name": "SHA256SUMS", "browser_download_url": format!("{}/files/{sums_name}", self.base)}));
            published.files.insert(file, bundle);
            published
                .files
                .insert(sums_name, format!("{hash}  {name}\n").into_bytes());
        }
        published
            .releases
            .push(json!({"tag_name": tag, "draft": false, "assets": assets}));
    }
}

async fn releases(State(state): State<Arc<Mutex<Published>>>) -> axum::Json<Value> {
    axum::Json(Value::Array(state.lock().unwrap().releases.clone()))
}

async fn file(
    State(state): State<Arc<Mutex<Published>>>,
    UrlPath(name): UrlPath<String>,
) -> Result<Vec<u8>, StatusCode> {
    state
        .lock()
        .unwrap()
        .files
        .get(&name)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A bundle as the release workflow packs it: `bin/mythos[.exe]` (this build's own CLI) and
/// the workbench under `web/`, plus anything in `extra`.
fn bundle_with(mythos: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        // Stored, not deflated: the debug `mythos` is about 100 MB, and compressing it in a
        // debug build takes longer than everything else here together.
        let stored = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        let exec = stored.unix_permissions(0o755);
        let plain = stored.unix_permissions(0o644);
        zip.start_file(format!("bin/{}", exe("mythos")), exec)
            .unwrap();
        zip.write_all(mythos).unwrap();
        zip.start_file("web/index.html", plain).unwrap();
        zip.write_all(b"<!doctype html>").unwrap();
        for (name, body) in extra {
            zip.start_file(*name, plain).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }
    out.into_inner()
}

fn bundle() -> Vec<u8> {
    bundle_with(&std::fs::read(MYTHOS).unwrap(), &[])
}

/// A MythOS directory whose configuration points at `feed`, on `channel`.
fn home(feed: &Feed, channel: ReleaseChannel) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".mythos");
    let config = CliConfig {
        channel,
        remote_url: feed.url.clone(),
        ..CliConfig::default()
    };
    config.save_to(&dir).unwrap();
    (tmp, dir)
}

fn our_version_line() -> String {
    let out = std::process::Command::new(MYTHOS)
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn bare_update_installs_the_newest_release_on_the_channel_and_makes_it_current() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0-rc.1", Some(bundle()), None);
    feed.publish("v0.3.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);

    // The operator's path: no version given.
    let result = UpdateManager::update(&dir, None).unwrap();

    assert_eq!(
        result.version, "0.4.0",
        "the release channel skips release candidates"
    );
    // What it reports is what the installed executable printed, not a version the CLI knows.
    assert_eq!(
        result.reported.as_deref(),
        Some(our_version_line().as_str())
    );
    assert!(is_installed(&dir, "0.4.0"));
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    assert!(dir
        .join("current")
        .join("bin")
        .join(exe("mythos"))
        .is_file());
    assert!(dir.join("current").join("web").join("index.html").is_file());
    let config = CliConfig::load_from(&dir).unwrap();
    assert_eq!(config.current_version.as_deref(), Some("0.4.0"));
    assert_eq!(config.previous_version, None);
}

#[test]
fn a_channel_that_follows_candidates_gets_the_newest_candidate() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0-rc.1", Some(bundle()), None);
    feed.publish("v0.5.0-dev.7", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Rc);
    assert_eq!(
        UpdateManager::update(&dir, None).unwrap().version,
        "0.5.0-rc.1"
    );
}

#[test]
fn a_pinned_version_is_installed_and_rollback_returns_to_the_previous_one() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);

    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();
    UpdateManager::update(&dir, None).unwrap();
    assert_eq!(current_version(&dir).as_deref(), Some("0.5.0"));

    let rolled = RollbackManager::rollback(&dir).unwrap();
    assert_eq!(
        (rolled.from_version.as_str(), rolled.to_version.as_str()),
        ("0.5.0", "0.4.0")
    );
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    // Both stay installed: rolling forward again needs no download.
    assert!(is_installed(&dir, "0.5.0"));
    let again = UpdateManager::update(&dir, Some("v0.5.0".into())).unwrap();
    assert_eq!(
        again.reported, None,
        "an installed version is not downloaded again"
    );
    assert_eq!(current_version(&dir).as_deref(), Some("0.5.0"));
}

#[test]
fn a_bundle_that_does_not_match_its_checksum_is_refused_and_nothing_changes() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), Some(&"0".repeat(64)));
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("does not match its published checksum"),
        "{err}"
    );
    assert!(!version_dir(&dir, "0.5.0").exists());
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    let leftovers: Vec<_> = std::fs::read_dir(dir.join("versions"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "a staging directory was left behind: {leftovers:?}"
    );
}

#[test]
fn a_release_without_a_bundle_for_this_platform_says_so() {
    let feed = Feed::start();
    feed.publish("v0.4.0", None, None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("no bundle for this platform") && err.to_string().contains(TARGET),
        "{err}"
    );
    assert_eq!(current_version(&dir), None);
}

#[test]
fn a_bundle_whose_mythos_does_not_run_is_never_made_current() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle_with(b"not a program", &[])), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("does not run on this machine"),
        "{err}"
    );
    assert!(!is_installed(&dir, "0.5.0"));
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
}

#[test]
fn an_archive_entry_outside_the_install_directory_is_refused_before_anything_is_written() {
    let feed = Feed::start();
    let evil = bundle_with(
        &std::fs::read(MYTHOS).unwrap(),
        &[("../../escaped.txt", b"x")],
    );
    feed.publish("v0.4.0", Some(evil), None);
    let (tmp, dir) = home(&feed, ReleaseChannel::Release);

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("outside the install directory"),
        "{err}"
    );
    assert!(!tmp.path().join("escaped.txt").exists());
    assert!(!dir.join("escaped.txt").exists());
    assert_eq!(current_version(&dir), None);
}

#[test]
fn nothing_published_and_an_unreachable_feed_are_errors_not_a_made_up_list() {
    let feed = Feed::start();
    feed.publish("v0.5.0-dev.1", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    assert!(matches!(
        UpdateManager::update(&dir, None),
        Err(UpdateError::NothingPublished { .. })
    ));
    assert!(matches!(
        UpdateManager::update(&dir, Some("9.9.9".into())),
        Err(UpdateError::VersionNotPublished { .. })
    ));

    // A port nothing listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let config = CliConfig {
        remote_url: format!("http://127.0.0.1:{port}/releases"),
        ..CliConfig::default()
    };
    config.save_to(&dir).unwrap();
    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(err.to_string().contains("could not be read"), "{err}");
}

#[test]
fn rollback_with_nothing_to_return_to_is_refused() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    assert!(matches!(
        RollbackManager::rollback(&dir),
        Err(RollbackError::NoPreviousVersion)
    ));
    UpdateManager::update(&dir, None).unwrap();
    assert!(matches!(
        RollbackManager::rollback(&dir),
        Err(RollbackError::NoPreviousVersion)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_commands_run_end_to_end() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    let dir_arg = format!("--mythos-dir={}", dir.display());
    let run = |args: &[&str]| {
        let mut all = vec!["mythos", dir_arg.as_str()];
        all.extend_from_slice(args);
        Cli::try_parse_from(all).unwrap().command
    };

    execute_command(run(&["versions", "--remote"]), &dir)
        .await
        .unwrap();
    execute_command(run(&["update", "--version", "0.4.0"]), &dir)
        .await
        .unwrap();
    execute_command(run(&["update"]), &dir).await.unwrap();
    execute_command(run(&["versions"]), &dir).await.unwrap();
    execute_command(run(&["rollback"]), &dir).await.unwrap();
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
}

#[test]
fn the_default_feed_is_the_real_one_even_for_a_configuration_from_the_simulated_cli() {
    let legacy: CliConfig = serde_json::from_value(json!({
        "channel": "release",
        "remote_url": "https://releases.mythos.firmansyah.co",
        "current_version": null,
        "previous_version": null
    }))
    .unwrap();
    if std::env::var(mythos_cli::config::RELEASE_FEED_ENV).is_err() {
        assert_eq!(legacy.feed_url(), mythos_cli::config::DEFAULT_RELEASE_FEED);
        assert_eq!(
            CliConfig::default().feed_url(),
            mythos_cli::config::DEFAULT_RELEASE_FEED
        );
    }
}
