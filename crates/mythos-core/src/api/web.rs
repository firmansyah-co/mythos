//! # MythOS Web HMI Asset Embedding & HTTP Static Router
//!
//! Provides static Web HMI asset serving on `http://localhost:8080/`:
//! - `GET /` serves HTML (embedded production fallback dashboard or loaded from configured static dir).
//! - `GET /assets/*path` and `GET /*path` serve static assets with exact MIME types (`text/html`, `text/javascript`, `text/css`, `image/svg+xml`, `application/json`, etc.).
//! - Fallback to `index.html` for client-side Single Page Application (SPA) routing (e.g. `/topology`, `/screens`) while strictly preserving `/api/*` endpoints for REST/SSE.
//! - Embeds default production fallback Web HMI dashboard with ANSI/ISA-101 theme, TagBus status monitor, and system telemetry widget.
//! - Supports runtime static asset directory override via `MYTHOS_STATIC_DIR` environment variable or `ApiState.with_static_dir(...)`.

use crate::api::state::ApiState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use mythos_pal::PlatformShm;
use std::path::{Component, Path as FsPath, PathBuf};

/// Default embedded production fallback Web HMI single-page application dashboard.
pub const DEFAULT_INDEX_HTML: &str = include_str!("default_hmi.html");

/// Embedded MythOS SVG banner asset.
pub const EMBEDDED_BANNER_SVG: &[u8] = include_bytes!("../../../../assets/mythos-banner.svg");

/// Embedded MythOS Architecture diagram SVG asset.
pub const EMBEDDED_ARCH_SVG: &[u8] = include_bytes!("../../../../assets/mythos-architecture.svg");

/// Construct the Axum Router for Web HMI static assets and SPA client-side routing.
pub fn web_router<S: PlatformShm + 'static>() -> Router<ApiState<S>> {
    Router::new()
        .route("/", get(index_handler::<S>))
        .route("/assets/*path", get(asset_handler::<S>))
        .fallback(spa_fallback_handler::<S>)
}

/// Handler for `GET /` - serves `index.html`.
pub async fn index_handler<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    serve_index_response(&state).await
}

/// Handler for `GET /assets/*path` - serves static assets from disk or embedded cache.
pub async fn asset_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(path): Path<String>,
) -> Response {
    serve_asset_response(&state, &path).await
}

/// Fallback handler for client-side SPA routing and root-level static assets.
pub async fn spa_fallback_handler<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    uri: Uri,
) -> Response {
    let raw_path = uri.path();

    // 1. Strict guard: API routes must never return HTML
    if raw_path.starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"API endpoint not found"}"#,
        )
            .into_response();
    }

    let trimmed = raw_path.trim_start_matches('/');

    // 2. Check if file exists in configured static directory
    let static_dir = get_effective_static_dir(&state);
    if let Some(ref dir) = static_dir {
        if let Some(safe_path) = sanitize_relative_path(trimmed) {
            let candidate = dir.join(&safe_path);
            if candidate.is_file() {
                if let Ok(bytes) = tokio::fs::read(&candidate).await {
                    let mime = mime_type_for_path(&candidate);
                    return (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response();
                }
            }
        }
    }

    // 3. Embedded asset routes
    if trimmed == "assets/mythos-banner.svg" || trimmed == "mythos-banner.svg" {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/svg+xml")],
            EMBEDDED_BANNER_SVG,
        )
            .into_response();
    }
    if trimmed == "assets/mythos-architecture.svg" || trimmed == "mythos-architecture.svg" {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/svg+xml")],
            EMBEDDED_ARCH_SVG,
        )
            .into_response();
    }

    // 4. If path has a static file extension (e.g. .js, .css, .ico, .svg), return 404
    if has_file_extension(trimmed) {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Asset not found",
        )
            .into_response();
    }

    // 5. Arbitrary client route -> SPA fallback to index.html
    serve_index_response(&state).await
}

/// Helper to serve `index.html` either from disk or embedded fallback.
async fn serve_index_response<S: PlatformShm + 'static>(state: &ApiState<S>) -> Response {
    let static_dir = get_effective_static_dir(state);

    if let Some(ref dir) = static_dir {
        let index_path = dir.join("index.html");
        if index_path.is_file() {
            if let Ok(bytes) = tokio::fs::read(&index_path).await {
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    bytes,
                )
                    .into_response();
            }
        }
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        DEFAULT_INDEX_HTML,
    )
        .into_response()
}

/// Helper to serve an asset under `/assets/*path`.
async fn serve_asset_response<S: PlatformShm + 'static>(
    state: &ApiState<S>,
    path: &str,
) -> Response {
    let safe_path = match sanitize_relative_path(path) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "Invalid path",
            )
                .into_response();
        }
    };

    let static_dir = get_effective_static_dir(state);
    if let Some(ref dir) = static_dir {
        // First try dir/assets/<safe_path>
        let candidate1 = dir.join("assets").join(&safe_path);
        if candidate1.is_file() {
            if let Ok(bytes) = tokio::fs::read(&candidate1).await {
                let mime = mime_type_for_path(&candidate1);
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, mime),
                        (header::CACHE_CONTROL, "public, max-age=31536000"),
                    ],
                    bytes,
                )
                    .into_response();
            }
        }

        // Then try dir/<safe_path>
        let candidate2 = dir.join(&safe_path);
        if candidate2.is_file() {
            if let Ok(bytes) = tokio::fs::read(&candidate2).await {
                let mime = mime_type_for_path(&candidate2);
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, mime),
                        (header::CACHE_CONTROL, "public, max-age=31536000"),
                    ],
                    bytes,
                )
                    .into_response();
            }
        }
    }

    // Check embedded assets
    let filename = safe_path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    if filename == "mythos-banner.svg" {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            EMBEDDED_BANNER_SVG,
        )
            .into_response();
    }
    if filename == "mythos-architecture.svg" {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            EMBEDDED_ARCH_SVG,
        )
            .into_response();
    }

    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Asset not found",
    )
        .into_response()
}

/// Determine MIME Content-Type based on file extension.
pub fn mime_type_for_path(path: &FsPath) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Validate path against directory traversal attacks.
pub fn sanitize_relative_path(path: &str) -> Option<PathBuf> {
    let p = FsPath::new(path);
    let mut clean = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(c) => clean.push(c),
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

/// Check if a path segment has a recognized file extension.
fn has_file_extension(path: &str) -> bool {
    let p = FsPath::new(path);
    p.extension().is_some()
}

/// Retrieve effective static directory (from state or `MYTHOS_STATIC_DIR` environment).
fn get_effective_static_dir<S: PlatformShm + 'static>(state: &ApiState<S>) -> Option<PathBuf> {
    state.static_dir.clone().or_else(|| {
        std::env::var("MYTHOS_STATIC_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
    })
}
