//! Signing in, the signed-in account, and the accounts of the controller (`crate::auth`).

use super::ApiState;
use crate::auth::{required_access, Access, Auth, AuthError, Identity, Role, SESSION_COOKIE};
use axum::extract::{MatchedPath, Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use mythos_pal::PlatformShm;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn auth_error(e: AuthError) -> Response {
    let status = match e {
        AuthError::BadCredentials | AuthError::BadSetupCode | AuthError::WrongCurrentPassword => {
            StatusCode::UNAUTHORIZED
        }
        AuthError::Locked(_) => StatusCode::TOO_MANY_REQUESTS,
        AuthError::AlreadySetUp | AuthError::UserExists(_) | AuthError::LastAdmin => {
            StatusCode::CONFLICT
        }
        AuthError::NoSuchUser(_) | AuthError::NoSuchToken(_) => StatusCode::NOT_FOUND,
        AuthError::BadUsername | AuthError::WeakPassword | AuthError::BadTokenName => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        AuthError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error(status, e.to_string())
}

/// The session cookie a request carries, if any.
pub fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
}

/// Who a request is from: its bearer token, else its session cookie.
pub fn identify(auth: &Auth, headers: &HeaderMap) -> Option<Identity> {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return auth.from_token(token.trim());
    }
    session_cookie(headers).and_then(|c| auth.from_session(&c))
}

/// Refuse a request its sender may not make. The route's requirement comes from
/// `crate::auth::policy`; the identity found is passed on to the handler.
pub async fn enforce(
    State(auth): State<Arc<Auth>>,
    matched: Option<MatchedPath>,
    mut request: Request,
    next: Next,
) -> Response {
    let pattern = matched.as_ref().map(|m| m.as_str()).unwrap_or("");
    let access = required_access(request.method().as_str(), pattern);
    let identity = identify(&auth, request.headers());
    match (access, &identity) {
        (Access::Public, _) => {}
        (_, None) => {
            let mut response = error(StatusCode::UNAUTHORIZED, "sign in first");
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            return response;
        }
        (Access::SignedIn, Some(_)) => {}
        (Access::Role(needed), Some(who)) if who.role < needed => {
            return error(
                StatusCode::FORBIDDEN,
                format!(
                    "this needs the {needed} role; {} is {}",
                    who.username, who.role
                ),
            );
        }
        (Access::Role(_), Some(_)) => {}
    }
    if let Some(identity) = identity {
        request.extensions_mut().insert(identity);
    }
    next.run(request).await
}

// A handler returns the `Response` at once; boxing it only to unbox it on the next line gains nothing.
#[allow(clippy::result_large_err)]
fn auth_of<S: PlatformShm + 'static>(state: &ApiState<S>) -> Result<&Arc<Auth>, Response> {
    state.auth.as_ref().ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "this server has no accounts",
        )
    })
}

fn with_session(body: serde_json::Value, status: StatusCode, cookie: &str) -> Response {
    let mut response = (status, Json(body)).into_response();
    let value = format!(
        "{SESSION_COOKIE}={cookie}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        crate::auth::SESSION_LIFETIME.as_secs()
    );
    if let Ok(value) = HeaderValue::from_str(&value) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

// ---- signing in --------------------------------------------------------------------------

/// `GET /api/v1/auth/status`: whether the first administrator is still to be created, and who
/// this request is from.
pub async fn status<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    headers: HeaderMap,
) -> Response {
    match &state.auth {
        None => {
            Json(json!({ "auth": false, "setup_required": false, "user": null })).into_response()
        }
        Some(auth) => Json(json!({
            "auth": true,
            "setup_required": auth.setup_required(),
            "user": identify(auth, &headers),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetupRequest {
    setup_code: String,
    username: String,
    #[serde(default)]
    display_name: String,
    password: String,
}

/// `POST /api/v1/auth/setup`: create the first administrator with the setup code.
pub async fn setup<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<SetupRequest>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    match auth.set_up(
        &body.setup_code,
        &body.username,
        &body.display_name,
        &body.password,
    ) {
        Ok((me, cookie)) => with_session(json!(me), StatusCode::CREATED, &cookie),
        Err(e) => auth_error(e),
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

/// `POST /api/v1/auth/login`
pub async fn login<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    match auth.sign_in(&body.username, &body.password) {
        Ok((me, cookie)) => with_session(json!(me), StatusCode::OK, &cookie),
        Err(e) => auth_error(e),
    }
}

/// `POST /api/v1/auth/logout`
pub async fn logout<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    headers: HeaderMap,
) -> Response {
    if let (Some(auth), Some(cookie)) = (&state.auth, session_cookie(&headers)) {
        auth.sign_out(&cookie);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("mythos_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    response
}

/// The identity `enforce` found, or why there is none.
// A handler returns the `Response` at once; boxing it only to unbox it on the next line gains nothing.
#[allow(clippy::result_large_err)]
fn who(me: Option<Extension<Identity>>) -> Result<Identity, Response> {
    me.map(|Extension(me)| me)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "sign in first"))
}

/// `GET /api/v1/auth/me`
pub async fn me(me: Option<Extension<Identity>>) -> Response {
    match who(me) {
        Ok(me) => Json(me).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct PasswordChange {
    current_password: String,
    new_password: String,
}

/// `PUT /api/v1/auth/me/password`: change one's own password. Every other session of the
/// account ends; this one stays.
pub async fn change_password<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    me: Option<Extension<Identity>>,
    headers: HeaderMap,
    Json(body): Json<PasswordChange>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let me = match who(me) {
        Ok(me) => me,
        Err(r) => return r,
    };
    let keep = session_cookie(&headers);
    match auth.change_own_password(
        &me.username,
        &body.current_password,
        &body.new_password,
        keep.as_deref(),
    ) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => auth_error(e),
    }
}

/// `GET /api/v1/auth/me/tokens`
pub async fn list_tokens<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    me: Option<Extension<Identity>>,
) -> Response {
    let (auth, me) = match (auth_of(&state), who(me)) {
        (Ok(auth), Ok(me)) => (auth, me),
        (Err(r), _) | (_, Err(r)) => return r,
    };
    Json(auth.tokens_of(&me.username)).into_response()
}

#[derive(Deserialize)]
pub struct NewToken {
    name: String,
}

/// `POST /api/v1/auth/me/tokens`: the answer holds the token itself, once.
pub async fn create_token<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    me: Option<Extension<Identity>>,
    Json(body): Json<NewToken>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let me = match who(me) {
        Ok(me) => me,
        Err(r) => return r,
    };
    match auth.create_token(&me.username, &body.name) {
        Ok((shown, secret)) => (
            StatusCode::CREATED,
            Json(json!({ "token": shown, "secret": secret })),
        )
            .into_response(),
        Err(e) => auth_error(e),
    }
}

/// `DELETE /api/v1/auth/me/tokens/:token`
pub async fn revoke_token<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    me: Option<Extension<Identity>>,
    Path(token): Path<String>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let me = match who(me) {
        Ok(me) => me,
        Err(r) => return r,
    };
    match auth.revoke_token(&me.username, &token) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => auth_error(e),
    }
}

// ---- accounts (administrators) -------------------------------------------------------------

/// `GET /api/v1/users`
pub async fn list_users<S: PlatformShm + 'static>(State(state): State<ApiState<S>>) -> Response {
    match auth_of(&state) {
        Ok(auth) => Json(json!({ "users": auth.users(), "roles": Role::ALL })).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    #[serde(default)]
    display_name: String,
    role: Role,
    password: String,
}

/// `POST /api/v1/users`
pub async fn create_user<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Json(body): Json<NewUser>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    match auth.create_user(
        &body.username,
        &body.display_name,
        body.role,
        &body.password,
    ) {
        Ok(user) => (StatusCode::CREATED, Json(user)).into_response(),
        Err(e) => auth_error(e),
    }
}

#[derive(Deserialize)]
pub struct UserChange {
    display_name: Option<String>,
    role: Option<Role>,
    disabled: Option<bool>,
    /// A new password, set by the administrator.
    password: Option<String>,
}

/// `PUT /api/v1/users/:username`: change an account; absent fields are left as they are.
pub async fn update_user<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(username): Path<String>,
    Json(body): Json<UserChange>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    match auth.update_user(
        &username,
        body.display_name.as_deref(),
        body.role,
        body.disabled,
        body.password.as_deref(),
    ) {
        Ok(user) => Json(user).into_response(),
        Err(e) => auth_error(e),
    }
}

/// `DELETE /api/v1/users/:username`
pub async fn delete_user<S: PlatformShm + 'static>(
    State(state): State<ApiState<S>>,
    Path(username): Path<String>,
) -> Response {
    let auth = match auth_of(&state) {
        Ok(a) => a,
        Err(r) => return r,
    };
    match auth.delete_user(&username) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => auth_error(e),
    }
}
