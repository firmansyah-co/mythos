//! Accounts and roles, through the real router: what each role may do, over HTTP and over MCP.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::policy::POLICY;
use mythos_core::auth::{Access, Auth};
use mythos_core::mcp::{NOT_TOOLS, TOOLS};
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::util::ServiceExt;

struct Rig {
    app: Router,
    setup_code: String,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path().join("projects")).unwrap());
    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let setup_code = auth.setup_code().unwrap().to_string();
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    )
    .with_auth(auth);
    Rig {
        app: create_router(state),
        setup_code,
        _dir: dir,
    }
}

/// How a request says who it is from.
#[derive(Clone)]
enum As {
    Nobody,
    Cookie(String),
    Bearer(String),
}

impl Rig {
    async fn send(
        &self,
        who: &As,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut request = Request::builder().method(method).uri(uri);
        match who {
            As::Nobody => {}
            As::Cookie(c) => {
                request = request.header(header::COOKIE, format!("mythos_session={c}"))
            }
            As::Bearer(t) => request = request.header(header::AUTHORIZATION, format!("Bearer {t}")),
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("mythos_session="))
            .map(|v| v.split(';').next().unwrap_or("").to_string())
            .filter(|v| !v.is_empty());
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body, cookie)
    }

    /// Create the first administrator; returns its session.
    async fn admin(&self) -> As {
        let (status, body, cookie) = self
            .send(
                &As::Nobody,
                "POST",
                "/api/v1/auth/setup",
                Some(json!({"setup_code": self.setup_code, "username": "root", "password": "root password 1"})),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        As::Cookie(cookie.expect("setup signs the administrator in"))
    }

    /// Create an account with `role` and sign it in.
    async fn user(&self, admin: &As, username: &str, role: &str) -> As {
        let (status, body, _) = self
            .send(
                admin,
                "POST",
                "/api/v1/users",
                Some(json!({"username": username, "role": role, "password": "a good password"})),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let (status, _, cookie) = self
            .send(
                &As::Nobody,
                "POST",
                "/api/v1/auth/login",
                Some(json!({"username": username, "password": "a good password"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        As::Cookie(cookie.unwrap())
    }

    async fn mcp(&self, who: &As, message: Value) -> (StatusCode, Value) {
        let (status, body, _) = self.send(who, "POST", "/mcp", Some(message)).await;
        (status, body)
    }
}

fn new_project(id: &str) -> Value {
    json!({"id": id, "name": id, "description": "", "author": "test"})
}

#[test]
fn every_route_has_a_policy_of_its_own() {
    let listed: Vec<(&str, &str)> = POLICY.iter().map(|(m, p, _)| (*m, *p)).collect();
    let routes = TOOLS
        .iter()
        .map(|t| (t.method, t.path))
        .chain(NOT_TOOLS.iter().map(|n| (n.method, n.path)))
        .filter(|(_, path)| path.starts_with("/api/"));
    for route in routes {
        assert!(
            listed.contains(&route),
            "{} {} has no entry in auth::policy::POLICY",
            route.0,
            route.1
        );
    }
}

#[tokio::test]
async fn every_route_but_the_public_ones_refuses_a_request_from_nobody() {
    let rig = rig();
    rig.admin().await;
    for (method, pattern, access) in POLICY {
        let uri = pattern
            .split('/')
            .map(|s| if s.starts_with(':') { "x" } else { s })
            .collect::<Vec<_>>()
            .join("/");
        let (status, _, _) = rig.send(&As::Nobody, method, &uri, None).await;
        if *access == Access::Public {
            assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {pattern}");
        } else {
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {pattern} answered {status}"
            );
        }
    }
}

#[tokio::test]
async fn a_new_controller_waits_for_its_first_administrator() {
    let rig = rig();
    let (_, status, _) = rig
        .send(&As::Nobody, "GET", "/api/v1/auth/status", None)
        .await;
    assert_eq!(status["setup_required"], true);
    assert_eq!(status["user"], Value::Null);

    let (code, _, _) = rig
        .send(
            &As::Nobody,
            "POST",
            "/api/v1/auth/setup",
            Some(
                json!({"setup_code": "not it", "username": "root", "password": "root password 1"}),
            ),
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let admin = rig.admin().await;
    let (_, status, _) = rig.send(&admin, "GET", "/api/v1/auth/status", None).await;
    assert_eq!(status["setup_required"], false);
    assert_eq!(status["user"]["role"], "admin");

    // Once there is an administrator the setup code does nothing.
    let (code, _, _) = rig
        .send(
            &As::Nobody,
            "POST",
            "/api/v1/auth/setup",
            Some(json!({"setup_code": rig.setup_code, "username": "intruder", "password": "root password 1"})),
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT);
}

#[tokio::test]
async fn each_role_may_do_what_its_level_allows_and_no_more() {
    let rig = rig();
    let admin = rig.admin().await;
    let viewer = rig.user(&admin, "vera", "viewer").await;
    let operator = rig.user(&admin, "otto", "operator").await;
    let engineer = rig.user(&admin, "erin", "engineer").await;

    for who in [&viewer, &operator, &engineer, &admin] {
        let (status, _, _) = rig.send(who, "GET", "/api/v1/projects", None).await;
        assert_eq!(status, StatusCode::OK);
    }
    for (who, id) in [(&viewer, "v"), (&operator, "o")] {
        let (status, body, _) = rig
            .send(who, "POST", "/api/v1/projects", Some(new_project(id)))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body["error"].as_str().unwrap().contains("engineer"));
    }
    let (status, _, _) = rig
        .send(
            &engineer,
            "POST",
            "/api/v1/projects",
            Some(new_project("plant")),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, _) = rig.send(&engineer, "GET", "/api/v1/users", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body, _) = rig.send(&admin, "GET", "/api/v1/users", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["users"].as_array().unwrap().len(), 4);
    assert!(
        !body.to_string().contains("argon2"),
        "password hashes are never shown"
    );
}

#[tokio::test]
async fn a_role_changed_by_an_administrator_applies_to_the_next_request() {
    let rig = rig();
    let admin = rig.admin().await;
    let erin = rig.user(&admin, "erin", "engineer").await;
    let (status, _, _) = rig
        .send(&erin, "POST", "/api/v1/projects", Some(new_project("a")))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, _) = rig
        .send(
            &admin,
            "PUT",
            "/api/v1/users/erin",
            Some(json!({"role": "viewer"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = rig
        .send(&erin, "POST", "/api/v1/projects", Some(new_project("b")))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = rig
        .send(
            &admin,
            "PUT",
            "/api/v1/users/erin",
            Some(json!({"disabled": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = rig.send(&erin, "GET", "/api/v1/projects", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signing_out_ends_the_session() {
    let rig = rig();
    let admin = rig.admin().await;
    let (status, _, _) = rig.send(&admin, "POST", "/api/v1/auth/logout", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = rig.send(&admin, "GET", "/api/v1/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_access_token_acts_with_its_owners_role_until_revoked() {
    let rig = rig();
    let admin = rig.admin().await;
    let erin = rig.user(&admin, "erin", "engineer").await;
    let (status, made, _) = rig
        .send(
            &erin,
            "POST",
            "/api/v1/auth/me/tokens",
            Some(json!({"name": "agent"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = As::Bearer(made["secret"].as_str().unwrap().to_string());

    let (_, me, _) = rig.send(&token, "GET", "/api/v1/auth/me", None).await;
    assert_eq!(
        (me["username"].as_str(), me["via"].as_str()),
        (Some("erin"), Some("token:agent"))
    );
    let (status, _, _) = rig
        .send(&token, "POST", "/api/v1/projects", Some(new_project("t")))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, listed, _) = rig.send(&erin, "GET", "/api/v1/auth/me/tokens", None).await;
    assert!(
        !listed.to_string().contains("mythos_pat_"),
        "a token is shown only when made"
    );
    let id = made["token"]["id"].as_str().unwrap();
    let (status, _, _) = rig
        .send(
            &erin,
            "DELETE",
            &format!("/api/v1/auth/me/tokens/{id}"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = rig.send(&token, "GET", "/api/v1/projects", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_mcp_client_needs_a_token_and_each_tool_runs_with_its_role() {
    let rig = rig();
    let admin = rig.admin().await;
    let vera = rig.user(&admin, "vera", "viewer").await;
    let erin = rig.user(&admin, "erin", "engineer").await;
    let token_of = |who: As| {
        let rig = &rig;
        async move {
            let (_, made, _) = rig
                .send(
                    &who,
                    "POST",
                    "/api/v1/auth/me/tokens",
                    Some(json!({"name": "mcp"})),
                )
                .await;
            As::Bearer(made["secret"].as_str().unwrap().to_string())
        }
    };
    let viewer_token = token_of(vera).await;
    let engineer_token = token_of(erin).await;

    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let (status, _) = rig.mcp(&As::Nobody, list.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, tools) = rig.mcp(&viewer_token, list).await;
    assert_eq!(status, StatusCode::OK);
    assert!(tools["result"]["tools"].as_array().unwrap().len() > 30);

    let create = |id: &str| {
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
               "params": {"name": "create_project", "arguments": {"body": new_project(id)}}})
    };
    let (_, refused) = rig.mcp(&viewer_token, create("by_viewer")).await;
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(refused["result"]["structuredContent"]["status"], 403);

    let (_, made) = rig.mcp(&engineer_token, create("by_engineer")).await;
    assert_eq!(made["result"]["structuredContent"]["status"], 201, "{made}");
}

#[tokio::test]
async fn changing_ones_password_keeps_this_session_and_ends_the_others() {
    let rig = rig();
    let admin = rig.admin().await;
    let (_, _, other) = rig
        .send(
            &As::Nobody,
            "POST",
            "/api/v1/auth/login",
            Some(json!({"username": "root", "password": "root password 1"})),
        )
        .await;
    let other = As::Cookie(other.unwrap());

    let (status, _, _) = rig
        .send(
            &admin,
            "PUT",
            "/api/v1/auth/me/password",
            Some(json!({"current_password": "wrong", "new_password": "a newer password"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = rig
        .send(
            &admin,
            "PUT",
            "/api/v1/auth/me/password",
            Some(
                json!({"current_password": "root password 1", "new_password": "a newer password"}),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        rig.send(&admin, "GET", "/api/v1/auth/me", None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        rig.send(&other, "GET", "/api/v1/auth/me", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_server_always_runs_with_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let server = mythos_core::server::bootstrap_with::<mythos_pal::MemoryShm>(
        dir.path(),
        "mythos_auth_test_unused",
        mythos_core::io::DriverRegistry::empty(),
    )
    .await
    .unwrap();
    assert!(
        server.state.auth.is_some(),
        "the assembled server has no accounts"
    );
    let app = create_router(server.state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(new_project("p").to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
