//! A release feed served on a local socket in the shape of GitHub's releases API, and real
//! bundles holding this build's own `mythos` executable.

#![allow(dead_code)]

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use mythos_cli::install::exe;
use mythos_cli::release::TARGET;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub const MYTHOS: &str = env!("CARGO_BIN_EXE_mythos");

/// What the feed serves: the release list, and files by name.
#[derive(Default)]
pub struct Published {
    releases: Vec<Value>,
    files: HashMap<String, Vec<u8>>,
}

pub struct Feed {
    pub url: String,
    pub base: String,
    pub published: Arc<Mutex<Published>>,
    _thread: std::thread::JoinHandle<()>,
}

impl Feed {
    pub fn start() -> Self {
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
    pub fn publish(&self, tag: &str, bundle: Option<Vec<u8>>, listed_hash: Option<&str>) {
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
        published.releases.push(json!({
            "tag_name": tag,
            "draft": false,
            "assets": assets,
            "name": format!("MythOS {tag}"),
            "body": format!("What changed in {tag}."),
            "published_at": "2026-11-16T08:00:00Z",
            "html_url": format!("https://example.invalid/releases/{tag}"),
        }));
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

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A bundle as the release workflow packs it: `bin/mythos[.exe]` (this build's own CLI) and
/// the workbench under `web/`, plus anything in `extra`.
pub fn bundle_with(mythos: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
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

pub fn bundle() -> Vec<u8> {
    bundle_with(&std::fs::read(MYTHOS).unwrap(), &[])
}
