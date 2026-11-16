//! Settings → Updates, through the server's router with accounts: the versions published by a
//! release feed served here, and installing them into an installation directory.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use common::*;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use mythos_core::updates::Updates;
use mythos_update::config::CliConfig;
use mythos_update::install::current_version;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower::util::ServiceExt;

struct Rig {
    app: Router,
    admin: String,
    viewer: String,
    _dir: tempfile::TempDir,
}

/// A server with accounts, whose updates manage `home` (or nothing), reading `feed`.
fn rig(home: Option<&Path>, feed: &str) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    if let Some(home) = home {
        let config = CliConfig {
            remote_url: feed.to_string(),
            ..CliConfig::default()
        };
        config.save_to(home).unwrap();
    }
    let storage = Arc::new(ControllerStorage::new(dir.path().join("projects")).unwrap());
    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    let (_, admin) = auth.set_up(&code, "root", "", "root password 1").unwrap();
    auth.create_user("vera", "", Role::Viewer, "a good password")
        .unwrap();
    let (_, viewer) = auth.sign_in("vera", "a good password").unwrap();
    // A server started from 0.0.1 in the installation; it is not one of the published versions.
    let exe = home.map(|h| {
        h.join("versions")
            .join("0.0.1")
            .join("bin")
            .join("mythos-server")
    });
    let updates = match home {
        Some(home) => Updates::new(Some(home.to_path_buf()), exe),
        // No installation, and a feed only this test serves.
        None => Updates::new(None, Some(PathBuf::from("/src/target/debug/mythos-server")))
            .with_feed(feed),
    };
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(tokio::sync::RwLock::new(TagCatalog::new())),
        None,
    )
    .with_auth(auth)
    .with_updates(Arc::new(updates));
    Rig {
        app: create_router(state),
        admin,
        viewer,
        _dir: dir,
    }
}

impl Rig {
    async fn send(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("mythos_session={session}"));
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 24)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn published(feed: &Feed) {
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0-rc.1", Some(bundle()), None);
    feed.publish("v0.6.0-dev.3", None, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_published_version_is_listed_with_what_it_says_about_itself() {
    let feed = Feed::start();
    published(&feed);
    let home = tempfile::tempdir().unwrap();
    let rig = rig(Some(home.path()), &feed.url);

    let (status, list) = rig
        .send(&rig.viewer, "GET", "/api/v1/updates/releases", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let versions: Vec<&str> = list["releases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["version"].as_str().unwrap())
        .collect();
    assert_eq!(versions, ["0.6.0-dev.3", "0.5.0-rc.1", "0.4.0"]);
    let rc = &list["releases"][1];
    assert_eq!(rc["channel"], "rc");
    assert_eq!(rc["name"], "MythOS v0.5.0-rc.1");
    assert_eq!(rc["notes"], "What changed in v0.5.0-rc.1.");
    assert_eq!(rc["published_at"], "2026-11-16T08:00:00Z");
    assert_eq!(rc["has_bundle"], true);
    assert_eq!(
        list["releases"][0]["has_bundle"], false,
        "the dev build published no bundle"
    );
    assert_eq!(
        list["target_version"], "0.4.0",
        "the release channel would install 0.4.0"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_administrator_follows_a_channel_installs_it_pins_a_version_and_rolls_back() {
    let feed = Feed::start();
    published(&feed);
    let home = tempfile::tempdir().unwrap();
    let rig = rig(Some(home.path()), &feed.url);

    let (status, after) = rig
        .send(
            &rig.admin,
            "PUT",
            "/api/v1/updates/follow",
            Some(json!({"channel": "rc"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["installation"]["channel"], "rc");

    let (status, done) = rig
        .send(
            &rig.admin,
            "POST",
            "/api/v1/updates/install",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["version"], "0.5.0-rc.1");
    assert_eq!(
        done["restart_required"], true,
        "the server runs 0.0.1 until restarted"
    );
    assert!(
        done["reported"].as_str().unwrap().starts_with("mythos "),
        "{done}"
    );
    assert_eq!(current_version(home.path()).as_deref(), Some("0.5.0-rc.1"));

    // Stay on 0.4.0: installing now installs the pinned version, not the channel's newest.
    rig.send(
        &rig.admin,
        "PUT",
        "/api/v1/updates/follow",
        Some(json!({"channel": "rc", "pinned_version": "0.4.0"})),
    )
    .await;
    let (_, done) = rig
        .send(
            &rig.admin,
            "POST",
            "/api/v1/updates/install",
            Some(json!({})),
        )
        .await;
    assert_eq!(done["version"], "0.4.0");

    let (_, list) = rig
        .send(&rig.admin, "GET", "/api/v1/updates/releases", None)
        .await;
    let flags = |v: &str| {
        let r = list["releases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["version"] == v)
            .unwrap()
            .clone();
        (
            r["installed"].as_bool().unwrap(),
            r["current"].as_bool().unwrap(),
        )
    };
    assert_eq!(flags("0.4.0"), (true, true));
    assert_eq!(flags("0.5.0-rc.1"), (true, false));

    let (status, back) = rig
        .send(&rig.admin, "POST", "/api/v1/updates/rollback", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{back}");
    assert_eq!(back["version"], "0.5.0-rc.1");
    assert_eq!(current_version(home.path()).as_deref(), Some("0.5.0-rc.1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_viewer_sees_the_versions_but_cannot_change_them() {
    let feed = Feed::start();
    published(&feed);
    let home = tempfile::tempdir().unwrap();
    let rig = rig(Some(home.path()), &feed.url);
    assert_eq!(
        rig.send(&rig.viewer, "GET", "/api/v1/updates", None)
            .await
            .0,
        StatusCode::OK
    );
    for (method, uri, body) in [
        (
            "PUT",
            "/api/v1/updates/follow",
            Some(json!({"channel": "dev"})),
        ),
        ("POST", "/api/v1/updates/install", Some(json!({}))),
        ("POST", "/api/v1/updates/rollback", None),
    ] {
        assert_eq!(
            rig.send(&rig.viewer, method, uri, body).await.0,
            StatusCode::FORBIDDEN,
            "{method} {uri}"
        );
    }
    assert_eq!(current_version(home.path()), None, "nothing was installed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_feed_that_cannot_be_read_is_an_error_not_a_list() {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/releases",
        closed.local_addr().unwrap().port()
    );
    drop(closed);
    let home = tempfile::tempdir().unwrap();
    let rig = rig(Some(home.path()), &url);
    let (status, body) = rig
        .send(&rig.viewer, "GET", "/api/v1/updates/releases", None)
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("could not be read"),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_an_installation_says_so_and_installs_nothing() {
    let feed = Feed::start();
    published(&feed);
    let rig = rig(None, &feed.url);
    let (_, status) = rig.send(&rig.viewer, "GET", "/api/v1/updates", None).await;
    assert_eq!(status["installation"], Value::Null);
    assert!(status["note"]
        .as_str()
        .unwrap()
        .contains("does not run from an installation"));
    let (code, body) = rig
        .send(
            &rig.admin,
            "POST",
            "/api/v1/updates/install",
            Some(json!({})),
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    let (code, _) = rig
        .send(
            &rig.admin,
            "PUT",
            "/api/v1/updates/follow",
            Some(json!({"channel": "dev"})),
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
}
