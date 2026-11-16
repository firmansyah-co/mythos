//! Accounts, sign-in, and what each account may do.
//!
//! Every account has one role. Roles are ordered: each may do everything the one below it may.
//!
//! | Role | May |
//! | :--- | :--- |
//! | `viewer` | read everything: projects, tags, live values, I/O status |
//! | `operator` | what a viewer may; reserved for operating the plant (writing setpoints) once writes exist |
//! | `engineer` | change projects: I/O configuration, screens, data types, faceplates, modules, activation |
//! | `admin` | manage accounts |
//!
//! A browser signs in with a username and password and gets a session cookie (HttpOnly,
//! SameSite=Strict). A program — an AI agent over MCP, a script — sends a personal access token
//! as `Authorization: Bearer <token>`; it acts with its owner's role.
//!
//! A new controller has no account. Until one exists the server prints a one-time setup code in
//! its log (or takes `MYTHOS_SETUP_CODE`), and the first administrator is created with it: only
//! someone who can read the controller's console can claim it.
//!
//! Accounts and tokens are kept in `<storage root>/.mythos-system/users.json` (owner-only on
//! Unix), passwords as Argon2id hashes and tokens as SHA-256 hashes. Sessions are kept in memory
//! only: restarting the server signs everyone out.

pub mod policy;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub use policy::{required_access, Access};

/// The cookie that carries a browser session.
pub const SESSION_COOKIE: &str = "mythos_session";
/// Sets the setup code instead of a random one (for automated installs).
pub const SETUP_CODE_ENV: &str = "MYTHOS_SETUP_CODE";
/// How long a session lasts after sign-in.
pub const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
/// The shortest password accepted.
pub const MIN_PASSWORD_LEN: usize = 10;
/// Failed sign-ins in a row after which an account is locked for a while.
const LOCK_AFTER_FAILURES: u32 = 5;
const LOCK_FOR: Duration = Duration::from_secs(30);
/// What every access token starts with, so a leaked one can be recognised.
const TOKEN_PREFIX: &str = "mythos_pat_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Operator,
    Engineer,
    Admin,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Viewer, Role::Operator, Role::Engineer, Role::Admin];
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Engineer => "engineer",
            Role::Admin => "admin",
        })
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AuthError {
    #[error("unknown user or wrong password")]
    BadCredentials,
    #[error("too many failed sign-ins; try again in {0} seconds")]
    Locked(u64),
    #[error("an administrator already exists; sign in instead")]
    AlreadySetUp,
    #[error("the setup code is wrong")]
    BadSetupCode,
    #[error("a username is 1 to 64 lowercase letters, digits, '.', '_' or '-'")]
    BadUsername,
    #[error("the password must be at least {MIN_PASSWORD_LEN} characters")]
    WeakPassword,
    #[error("the current password is wrong")]
    WrongCurrentPassword,
    #[error("user '{0}' already exists")]
    UserExists(String),
    #[error("there is no user '{0}'")]
    NoSuchUser(String),
    #[error("there is no access token '{0}'")]
    NoSuchToken(String),
    #[error("this would leave no enabled administrator")]
    LastAdmin,
    #[error("an access token needs a name")]
    BadTokenName,
    #[error("the account store could not be written: {0}")]
    Store(String),
}

/// Who a request is from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// `session`, or `token:<name>` for an access token.
    pub via: String,
}

/// An account as shown: never its password hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserView {
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub disabled: bool,
    pub created_unix_s: u64,
}

/// An access token as shown: never the token itself, which is shown once, when it is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenView {
    pub id: String,
    pub name: String,
    pub created_unix_s: u64,
    pub last_used_unix_s: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredUser {
    username: String,
    display_name: String,
    role: Role,
    password_hash: String,
    #[serde(default)]
    disabled: bool,
    created_unix_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredToken {
    id: String,
    username: String,
    name: String,
    sha256: String,
    created_unix_s: u64,
    #[serde(default)]
    last_used_unix_s: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    users: Vec<StoredUser>,
    tokens: Vec<StoredToken>,
}

impl Store {
    fn user(&self, username: &str) -> Option<&StoredUser> {
        self.users.iter().find(|u| u.username == username)
    }

    fn enabled_admins(&self) -> usize {
        self.users
            .iter()
            .filter(|u| u.role == Role::Admin && !u.disabled)
            .count()
    }
}

struct Session {
    username: String,
    expires: Instant,
}

#[derive(Default)]
struct Failures {
    count: u32,
    locked_until: Option<Instant>,
}

/// The accounts of one controller.
pub struct Auth {
    path: PathBuf,
    store: Mutex<Store>,
    /// Sessions by the SHA-256 of their cookie.
    sessions: Mutex<HashMap<String, Session>>,
    failures: Mutex<HashMap<String, Failures>>,
    setup_code: String,
    /// A hash to check a password against when the username does not exist, so the answer takes
    /// as long either way.
    decoy_hash: String,
}

fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Store(e.to_string()))
}

fn password_matches(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Compare two secrets without stopping at the first difference.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |diff, (x, y)| diff | (x ^ y))
            == 0
}

pub fn valid_username(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

fn check_password(password: &str) -> Result<(), AuthError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AuthError::WeakPassword);
    }
    Ok(())
}

impl Auth {
    /// The accounts kept in `dir/users.json`. A missing file is a controller with no account.
    pub fn open(dir: &Path) -> Result<Self, AuthError> {
        let path = dir.join("users.json");
        let store = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                AuthError::Store(format!("{} is not an account store: {e}", path.display()))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(AuthError::Store(e.to_string())),
        };
        let setup_code = std::env::var(SETUP_CODE_ENV)
            .ok()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| random_hex(8));
        Ok(Self {
            path,
            store: Mutex::new(store),
            sessions: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            setup_code,
            decoy_hash: hash_password(&random_hex(16))?,
        })
    }

    fn save(&self, store: &Store) -> Result<(), AuthError> {
        let err = |e: std::io::Error| AuthError::Store(e.to_string());
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(err)?;
        let tmp = dir.join(format!(".users.json.{}.tmp", std::process::id()));
        let json = serde_json::to_vec_pretty(store).map_err(|e| AuthError::Store(e.to_string()))?;
        std::fs::write(&tmp, json).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
        }
        std::fs::rename(&tmp, &self.path).map_err(err)
    }

    fn update<T>(
        &self,
        change: impl FnOnce(&mut Store) -> Result<T, AuthError>,
    ) -> Result<T, AuthError> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = store.clone();
        let out = change(&mut next)?;
        self.save(&next)?;
        *store = next;
        Ok(out)
    }

    fn read<T>(&self, look: impl FnOnce(&Store) -> T) -> T {
        look(&self.store.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Whether no account exists yet, so the first administrator is still to be created.
    pub fn setup_required(&self) -> bool {
        self.read(|s| s.users.is_empty())
    }

    /// The code that creates the first administrator, while none exists.
    pub fn setup_code(&self) -> Option<&str> {
        self.setup_required().then_some(self.setup_code.as_str())
    }

    /// Create the first administrator and sign them in. Returns the session cookie value.
    pub fn set_up(
        &self,
        code: &str,
        username: &str,
        display_name: &str,
        password: &str,
    ) -> Result<(Identity, String), AuthError> {
        if !self.setup_required() {
            return Err(AuthError::AlreadySetUp);
        }
        if !same_secret(code.trim(), &self.setup_code) {
            return Err(AuthError::BadSetupCode);
        }
        self.create_user(username, display_name, Role::Admin, password)?;
        let identity = self
            .identity_of(username, "session".into())
            .ok_or(AuthError::BadCredentials)?;
        Ok((identity, self.new_session(username)))
    }

    /// Check a username and password and start a session. Returns the session cookie value.
    pub fn sign_in(&self, username: &str, password: &str) -> Result<(Identity, String), AuthError> {
        let username = username.trim().to_ascii_lowercase();
        {
            let failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(until) = failures.get(&username).and_then(|f| f.locked_until) {
                let now = Instant::now();
                if until > now {
                    return Err(AuthError::Locked((until - now).as_secs().max(1)));
                }
            }
        }
        let user = self.read(|s| s.user(&username).cloned());
        let ok = match &user {
            Some(u) => password_matches(password, &u.password_hash) && !u.disabled,
            None => {
                let _ = password_matches(password, &self.decoy_hash);
                false
            }
        };
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if !ok {
            let entry = failures.entry(username).or_default();
            entry.count += 1;
            if entry.count >= LOCK_AFTER_FAILURES {
                entry.locked_until = Some(Instant::now() + LOCK_FOR);
            }
            return Err(AuthError::BadCredentials);
        }
        failures.remove(&username);
        drop(failures);
        let identity = self
            .identity_of(&username, "session".into())
            .ok_or(AuthError::BadCredentials)?;
        Ok((identity, self.new_session(&username)))
    }

    fn new_session(&self, username: &str) -> String {
        let cookie = random_hex(32);
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                sha256_hex(&cookie),
                Session {
                    username: username.to_string(),
                    expires: Instant::now() + SESSION_LIFETIME,
                },
            );
        cookie
    }

    pub fn sign_out(&self, cookie: &str) {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&sha256_hex(cookie));
    }

    /// The current account behind `username`, if it exists and is enabled. A role or an account
    /// changed by an administrator takes effect on the next request.
    fn identity_of(&self, username: &str, via: String) -> Option<Identity> {
        self.read(|s| {
            s.user(username).filter(|u| !u.disabled).map(|u| Identity {
                username: u.username.clone(),
                display_name: u.display_name.clone(),
                role: u.role,
                via,
            })
        })
    }

    /// Who a session cookie belongs to.
    pub fn from_session(&self, cookie: &str) -> Option<Identity> {
        let key = sha256_hex(cookie);
        let username = {
            let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            match sessions.get(&key) {
                Some(s) if s.expires > Instant::now() => s.username.clone(),
                Some(_) => {
                    sessions.remove(&key);
                    return None;
                }
                None => return None,
            }
        };
        self.identity_of(&username, "session".into())
    }

    /// Who an access token belongs to.
    pub fn from_token(&self, token: &str) -> Option<Identity> {
        if !token.starts_with(TOKEN_PREFIX) {
            return None;
        }
        let hash = sha256_hex(token);
        let found = self.read(|s| {
            s.tokens
                .iter()
                .find(|t| same_secret(&t.sha256, &hash))
                .map(|t| (t.id.clone(), t.username.clone(), t.name.clone()))
        })?;
        let identity = self.identity_of(&found.1, format!("token:{}", found.2))?;
        // Recording the use is best effort: failing to write it must not refuse the request.
        let _ = self.update(|s| {
            if let Some(t) = s.tokens.iter_mut().find(|t| t.id == found.0) {
                t.last_used_unix_s = Some(now_unix_s());
            }
            Ok(())
        });
        Some(identity)
    }

    pub fn change_own_password(
        &self,
        username: &str,
        current: &str,
        new: &str,
        keep_session: Option<&str>,
    ) -> Result<(), AuthError> {
        let hash = self
            .read(|s| s.user(username).map(|u| u.password_hash.clone()))
            .ok_or_else(|| AuthError::NoSuchUser(username.into()))?;
        if !password_matches(current, &hash) {
            return Err(AuthError::WrongCurrentPassword);
        }
        self.set_password(username, new)?;
        self.end_sessions_except(username, keep_session);
        Ok(())
    }

    fn set_password(&self, username: &str, new: &str) -> Result<(), AuthError> {
        check_password(new)?;
        let hash = hash_password(new)?;
        self.update(|s| {
            let user = s
                .users
                .iter_mut()
                .find(|u| u.username == username)
                .ok_or_else(|| AuthError::NoSuchUser(username.into()))?;
            user.password_hash = hash;
            Ok(())
        })
    }

    fn end_sessions_except(&self, username: &str, keep: Option<&str>) {
        let keep = keep.map(sha256_hex);
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|key, s| s.username != username || Some(key) == keep.as_ref());
    }

    // ---- accounts (administrators) -------------------------------------------------------

    pub fn users(&self) -> Vec<UserView> {
        self.read(|s| s.users.iter().map(view).collect())
    }

    pub fn create_user(
        &self,
        username: &str,
        display_name: &str,
        role: Role,
        password: &str,
    ) -> Result<UserView, AuthError> {
        let username = username.trim().to_ascii_lowercase();
        if !valid_username(&username) {
            return Err(AuthError::BadUsername);
        }
        check_password(password)?;
        let hash = hash_password(password)?;
        self.update(|s| {
            if s.user(&username).is_some() {
                return Err(AuthError::UserExists(username.clone()));
            }
            let user = StoredUser {
                display_name: if display_name.trim().is_empty() {
                    username.clone()
                } else {
                    display_name.trim().to_string()
                },
                username: username.clone(),
                role,
                password_hash: hash,
                disabled: false,
                created_unix_s: now_unix_s(),
            };
            let shown = view(&user);
            s.users.push(user);
            Ok(shown)
        })
    }

    /// Change an account. `None` leaves a field as it is.
    pub fn update_user(
        &self,
        username: &str,
        display_name: Option<&str>,
        role: Option<Role>,
        disabled: Option<bool>,
        password: Option<&str>,
    ) -> Result<UserView, AuthError> {
        if let Some(p) = password {
            check_password(p)?;
        }
        let hash = password.map(hash_password).transpose()?;
        let shown = self.update(|s| {
            let user = s
                .users
                .iter_mut()
                .find(|u| u.username == username)
                .ok_or_else(|| AuthError::NoSuchUser(username.into()))?;
            if let Some(name) = display_name {
                if !name.trim().is_empty() {
                    user.display_name = name.trim().to_string();
                }
            }
            if let Some(role) = role {
                user.role = role;
            }
            if let Some(disabled) = disabled {
                user.disabled = disabled;
            }
            if let Some(hash) = hash {
                user.password_hash = hash;
            }
            let shown = view(user);
            if s.enabled_admins() == 0 {
                return Err(AuthError::LastAdmin);
            }
            Ok(shown)
        })?;
        if shown.disabled || password.is_some() {
            self.end_sessions_except(username, None);
        }
        Ok(shown)
    }

    pub fn delete_user(&self, username: &str) -> Result<(), AuthError> {
        self.update(|s| {
            let before = s.users.len();
            s.users.retain(|u| u.username != username);
            if s.users.len() == before {
                return Err(AuthError::NoSuchUser(username.into()));
            }
            if s.enabled_admins() == 0 {
                return Err(AuthError::LastAdmin);
            }
            s.tokens.retain(|t| t.username != username);
            Ok(())
        })?;
        self.end_sessions_except(username, None);
        Ok(())
    }

    // ---- access tokens -------------------------------------------------------------------

    pub fn tokens_of(&self, username: &str) -> Vec<TokenView> {
        self.read(|s| {
            s.tokens
                .iter()
                .filter(|t| t.username == username)
                .map(|t| TokenView {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    created_unix_s: t.created_unix_s,
                    last_used_unix_s: t.last_used_unix_s,
                })
                .collect()
        })
    }

    /// Make an access token for `username`. Returns it with the token itself, which is never
    /// shown again.
    pub fn create_token(
        &self,
        username: &str,
        name: &str,
    ) -> Result<(TokenView, String), AuthError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(AuthError::BadTokenName);
        }
        let secret = format!("{TOKEN_PREFIX}{}", random_hex(32));
        let stored = StoredToken {
            id: random_hex(6),
            username: username.to_string(),
            name: name.to_string(),
            sha256: sha256_hex(&secret),
            created_unix_s: now_unix_s(),
            last_used_unix_s: None,
        };
        let shown = TokenView {
            id: stored.id.clone(),
            name: stored.name.clone(),
            created_unix_s: stored.created_unix_s,
            last_used_unix_s: None,
        };
        self.update(|s| {
            if s.user(username).is_none() {
                return Err(AuthError::NoSuchUser(username.into()));
            }
            s.tokens.push(stored);
            Ok(())
        })?;
        Ok((shown, secret))
    }

    pub fn revoke_token(&self, username: &str, id: &str) -> Result<(), AuthError> {
        self.update(|s| {
            let before = s.tokens.len();
            s.tokens.retain(|t| !(t.username == username && t.id == id));
            if s.tokens.len() == before {
                return Err(AuthError::NoSuchToken(id.into()));
            }
            Ok(())
        })
    }
}

fn view(u: &StoredUser) -> UserView {
    UserView {
        username: u.username.clone(),
        display_name: u.display_name.clone(),
        role: u.role,
        disabled: u.disabled,
        created_unix_s: u.created_unix_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> (tempfile::TempDir, Auth) {
        let dir = tempfile::tempdir().unwrap();
        let auth = Auth::open(dir.path()).unwrap();
        (dir, auth)
    }

    #[test]
    fn roles_are_ordered_so_a_higher_one_includes_the_lower() {
        assert!(Role::Viewer < Role::Operator);
        assert!(Role::Operator < Role::Engineer);
        assert!(Role::Engineer < Role::Admin);
    }

    #[test]
    fn the_first_administrator_needs_the_setup_code_and_only_once() {
        let (_d, auth) = auth();
        let code = auth.setup_code().unwrap().to_string();
        assert_eq!(
            auth.set_up("wrong", "root", "Root", "a long password")
                .unwrap_err(),
            AuthError::BadSetupCode
        );
        let (me, _cookie) = auth
            .set_up(&code, "root", "Root", "a long password")
            .unwrap();
        assert_eq!(me.role, Role::Admin);
        assert_eq!(auth.setup_code(), None);
        assert_eq!(
            auth.set_up(&code, "second", "", "a long password")
                .unwrap_err(),
            AuthError::AlreadySetUp
        );
    }

    #[test]
    fn accounts_survive_a_restart_and_passwords_are_not_stored() {
        let dir = tempfile::tempdir().unwrap();
        {
            let auth = Auth::open(dir.path()).unwrap();
            let code = auth.setup_code().unwrap().to_string();
            auth.set_up(&code, "root", "", "a long password").unwrap();
        }
        let text = std::fs::read_to_string(dir.path().join("users.json")).unwrap();
        assert!(!text.contains("a long password"));
        assert!(text.contains("$argon2id$"));
        let auth = Auth::open(dir.path()).unwrap();
        assert!(!auth.setup_required());
        auth.sign_in("root", "a long password").unwrap();
    }

    #[test]
    fn repeated_wrong_passwords_lock_the_account_for_a_while() {
        let (_d, auth) = auth();
        let code = auth.setup_code().unwrap().to_string();
        auth.set_up(&code, "root", "", "a long password").unwrap();
        for _ in 0..LOCK_AFTER_FAILURES {
            assert_eq!(
                auth.sign_in("root", "nope").unwrap_err(),
                AuthError::BadCredentials
            );
        }
        assert!(matches!(
            auth.sign_in("root", "a long password"),
            Err(AuthError::Locked(_))
        ));
    }

    #[test]
    fn the_last_enabled_administrator_cannot_be_removed_disabled_or_demoted() {
        let (_d, auth) = auth();
        let code = auth.setup_code().unwrap().to_string();
        auth.set_up(&code, "root", "", "a long password").unwrap();
        assert_eq!(auth.delete_user("root").unwrap_err(), AuthError::LastAdmin);
        assert_eq!(
            auth.update_user("root", None, Some(Role::Viewer), None, None)
                .unwrap_err(),
            AuthError::LastAdmin
        );
        assert_eq!(
            auth.update_user("root", None, None, Some(true), None)
                .unwrap_err(),
            AuthError::LastAdmin
        );
        // Nothing was changed by the refusals.
        assert_eq!(auth.users()[0].role, Role::Admin);
        assert!(!auth.users()[0].disabled);
    }

    #[test]
    fn a_disabled_account_loses_its_session_and_cannot_sign_in() {
        let (_d, auth) = auth();
        let code = auth.setup_code().unwrap().to_string();
        auth.set_up(&code, "root", "", "a long password").unwrap();
        auth.create_user("ana", "Ana", Role::Engineer, "another password")
            .unwrap();
        let (_, cookie) = auth.sign_in("ana", "another password").unwrap();
        assert!(auth.from_session(&cookie).is_some());
        auth.update_user("ana", None, None, Some(true), None)
            .unwrap();
        assert!(auth.from_session(&cookie).is_none());
        assert_eq!(
            auth.sign_in("ana", "another password").unwrap_err(),
            AuthError::BadCredentials
        );
    }

    #[test]
    fn a_token_acts_as_its_owner_until_revoked_and_is_stored_only_as_a_hash() {
        let (dir, auth) = auth();
        let code = auth.setup_code().unwrap().to_string();
        auth.set_up(&code, "root", "", "a long password").unwrap();
        let (shown, secret) = auth.create_token("root", "claude").unwrap();
        assert!(secret.starts_with(TOKEN_PREFIX));
        let me = auth.from_token(&secret).unwrap();
        assert_eq!(
            (me.username.as_str(), me.via.as_str()),
            ("root", "token:claude")
        );
        let stored = std::fs::read_to_string(dir.path().join("users.json")).unwrap();
        assert!(!stored.contains(&secret));
        auth.revoke_token("root", &shown.id).unwrap();
        assert!(auth.from_token(&secret).is_none());
    }
}
