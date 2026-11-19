//! Pairing (ADR-012 §2.3): two nodes come to trust each other, explicitly.
//!
//! 1. An administrator of node B makes a **pairing code**: one use, ten minutes, carrying the
//!    role B grants (an engineer by default). It is read to whoever pairs, never sent.
//! 2. On node A, an administrator gives B's address, B's certificate fingerprint (compared with
//!    the one B shows) and the code. A calls B over TLS pinned to that fingerprint
//!    (`tls::request_pinned`): a node that presents another certificate is refused before the
//!    code leaves A.
//! 3. B checks the code and answers with a token for A; A sent one for B in the same request.
//!    Each keeps the other's id, name, address, fingerprint, the token it calls with, and only
//!    the SHA-256 of the token it accepts.
//!
//! A paired node's token (`mythos_pair_…`) acts with the role it was granted, as a user would.
//! Either side can unpair: it forgets the other and tells it, so the other forgets it too.

use crate::auth::{
    now_unix_s, random_hex, same_secret, sha256_hex, Identity, Role, PAIRING_TOKEN_PREFIX,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How long a pairing code can be used.
pub const CODE_LIFETIME_S: u64 = 600;
/// Wrong guesses after which a code is no longer accepted at all.
pub const CODE_ATTEMPTS: u32 = 5;

/// A node this one is paired with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// Its role as it said when pairing.
    pub role: String,
    /// Where it answers HTTPS, as `host:port`; `None` for a node that cannot be reached (a
    /// workstation, which listens on its own computer only).
    #[serde(default)]
    pub address: Option<String>,
    /// SHA-256 of its certificate: the only certificate accepted from it.
    pub fingerprint: String,
    /// What it may do here.
    pub granted: Role,
    /// The token this node presents to it. Secret: never shown, never logged.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_to_it: String,
    /// SHA-256 of the token it presents here.
    pub token_from_it_sha256: String,
    pub paired_unix_s: u64,
    #[serde(default)]
    pub last_seen_unix_s: Option<u64>,
    /// Whether this node calls it home: opens the connection to it and is reached over that,
    /// for a node that cannot reach this one (ADR-012 §2.6, [`crate::callhome`]).
    #[serde(default)]
    pub call_home: bool,
}

/// A peer as it is shown: no token, not even its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub address: Option<String>,
    pub fingerprint: String,
    pub granted: Role,
    pub paired_unix_s: u64,
    pub last_seen_unix_s: Option<u64>,
    pub call_home: bool,
}

impl From<&Peer> for PeerView {
    fn from(p: &Peer) -> Self {
        PeerView {
            id: p.id.clone(),
            name: p.name.clone(),
            role: p.role.clone(),
            address: p.address.clone(),
            fingerprint: p.fingerprint.clone(),
            granted: p.granted,
            paired_unix_s: p.paired_unix_s,
            last_seen_unix_s: p.last_seen_unix_s,
            call_home: p.call_home,
        }
    }
}

/// The code an administrator made, while it may be used.
#[derive(Debug, Clone)]
struct Code {
    sha256: String,
    granted: Role,
    expires_unix_s: u64,
    wrong: u32,
}

/// What a node says about itself when it pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Introduction {
    pub id: String,
    pub name: String,
    pub role: String,
    /// Its HTTPS port, when other computers can reach it.
    #[serde(default)]
    pub https_port: Option<u16>,
    pub fingerprint: String,
}

/// The request one node sends another to pair (`POST /api/v1/pairing/accept`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acceptance {
    pub code: String,
    pub from: Introduction,
    /// The token the accepting node presents to the one asking.
    pub token: String,
}

/// The answer: who accepted, and the token for the one asking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accepted {
    pub node: Introduction,
    pub token: String,
    pub granted: Role,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("the pairing code is not right, or no longer valid; make a new one on that node")]
    BadCode,
    #[error("{0}")]
    Invalid(String),
    #[error("the pairings cannot be saved: {0}")]
    Store(String),
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    peers: Vec<Peer>,
}

/// This node's pairings, in `<system dir>/pairings.json`.
pub struct Pairings {
    path: PathBuf,
    stored: Mutex<Stored>,
    code: Mutex<Option<Code>>,
}

/// A new token a peer presents: long, random, recognisable by its prefix.
pub fn new_token() -> String {
    format!("{PAIRING_TOKEN_PREFIX}{}", random_hex(32))
}

/// A pairing code as a person reads it: eight characters from an alphabet without look-alikes.
fn new_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let bytes = hex::decode(random_hex(8)).unwrap_or_default();
    let chars: String = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &chars[..4], &chars[4..8])
}

fn normalise_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

impl Pairings {
    pub fn open(system_dir: &Path) -> Result<Pairings, PairingError> {
        let path = system_dir.join("pairings.json");
        let stored = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| PairingError::Store(format!("{}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(e) => return Err(PairingError::Store(e.to_string())),
        };
        Ok(Pairings {
            path,
            stored: Mutex::new(stored),
            code: Mutex::new(None),
        })
    }

    fn save(&self, stored: &Stored) -> Result<(), PairingError> {
        let bytes =
            serde_json::to_vec_pretty(stored).map_err(|e| PairingError::Store(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| PairingError::Store(e.to_string()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        // It holds the tokens this node presents: readable by its owner only.
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| PairingError::Store(e.to_string()))?;
            file.write_all(&bytes)
                .map_err(|e| PairingError::Store(e.to_string()))?;
        }
        #[cfg(not(unix))]
        std::fs::write(&tmp, &bytes).map_err(|e| PairingError::Store(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| PairingError::Store(e.to_string()))
    }

    /// Make a pairing code granting `granted`; it replaces any earlier one.
    pub fn make_code(&self, granted: Role) -> (String, u64) {
        let code = new_code();
        let expires_unix_s = now_unix_s() + CODE_LIFETIME_S;
        *self.code.lock().unwrap() = Some(Code {
            sha256: sha256_hex(&normalise_code(&code)),
            granted,
            expires_unix_s,
            wrong: 0,
        });
        (code, expires_unix_s)
    }

    /// Accept a pairing asked with `code`: the code is used up whether or not the rest succeeds.
    pub fn accept(
        &self,
        request: &Acceptance,
        address: Option<String>,
    ) -> Result<(String, Role), PairingError> {
        let granted = {
            let mut slot = self.code.lock().unwrap();
            let Some(code) = slot.as_mut() else {
                return Err(PairingError::BadCode);
            };
            if now_unix_s() > code.expires_unix_s {
                *slot = None;
                return Err(PairingError::BadCode);
            }
            if !same_secret(&sha256_hex(&normalise_code(&request.code)), &code.sha256) {
                code.wrong += 1;
                if code.wrong >= CODE_ATTEMPTS {
                    *slot = None;
                }
                return Err(PairingError::BadCode);
            }
            let granted = code.granted;
            *slot = None;
            granted
        };
        let from = &request.from;
        if from.id.trim().is_empty() || from.fingerprint.len() != 64 {
            return Err(PairingError::Invalid(
                "a node that pairs gives its id and its certificate's fingerprint".into(),
            ));
        }
        if !request.token.starts_with(PAIRING_TOKEN_PREFIX) {
            return Err(PairingError::Invalid("not a pairing token".into()));
        }
        let token_for_it = new_token();
        self.keep(Peer {
            id: from.id.clone(),
            name: from.name.clone(),
            role: from.role.clone(),
            address,
            fingerprint: from.fingerprint.clone(),
            granted,
            token_to_it: request.token.clone(),
            token_from_it_sha256: sha256_hex(&token_for_it),
            paired_unix_s: now_unix_s(),
            last_seen_unix_s: Some(now_unix_s()),
            call_home: false,
        })?;
        Ok((token_for_it, granted))
    }

    /// Keep `peer`, in place of any earlier pairing with the same node.
    pub fn keep(&self, peer: Peer) -> Result<(), PairingError> {
        let mut stored = self.stored.lock().unwrap();
        stored.peers.retain(|p| p.id != peer.id);
        stored.peers.push(peer);
        self.save(&stored)
    }

    /// Forget the node `id`. Returns what it was, to tell it.
    pub fn forget(&self, id: &str) -> Result<Option<Peer>, PairingError> {
        let mut stored = self.stored.lock().unwrap();
        let found = stored.peers.iter().position(|p| p.id == id);
        let Some(at) = found else {
            return Ok(None);
        };
        let peer = stored.peers.remove(at);
        self.save(&stored)?;
        Ok(Some(peer))
    }

    /// Call `id` home, or stop. `None` when no such node is paired.
    pub fn set_call_home(&self, id: &str, on: bool) -> Result<Option<Peer>, PairingError> {
        let mut stored = self.stored.lock().unwrap();
        let Some(peer) = stored.peers.iter_mut().find(|p| p.id == id) else {
            return Ok(None);
        };
        peer.call_home = on;
        let peer = peer.clone();
        self.save(&stored)?;
        Ok(Some(peer))
    }

    pub fn peers(&self) -> Vec<Peer> {
        self.stored.lock().unwrap().peers.clone()
    }

    pub fn peer(&self, id: &str) -> Option<Peer> {
        self.peers().into_iter().find(|p| p.id == id)
    }

    /// The paired node presenting `token`, as an identity with the role it was granted.
    pub fn identify(&self, token: &str) -> Option<Identity> {
        let hash = sha256_hex(token);
        let mut stored = self.stored.lock().unwrap();
        let peer = stored
            .peers
            .iter_mut()
            .find(|p| same_secret(&p.token_from_it_sha256, &hash))?;
        peer.last_seen_unix_s = Some(now_unix_s());
        Some(Identity {
            username: format!("node:{}", peer.id),
            display_name: peer.name.clone(),
            role: peer.granted,
            via: format!("pairing:{}", peer.id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking(code: &str) -> Acceptance {
        Acceptance {
            code: code.into(),
            from: Introduction {
                id: "7f3c2a10-5b9e-4c1d-9a2f-0e6b8d4c1a77".into(),
                name: "engineering laptop".into(),
                role: "workstation".into(),
                https_port: None,
                fingerprint: "ab".repeat(32),
            },
            token: new_token(),
        }
    }

    #[test]
    fn a_code_pairs_once_and_its_token_acts_with_the_role_granted() {
        let dir = tempfile::tempdir().unwrap();
        let pairings = Pairings::open(dir.path()).unwrap();
        let (code, _) = pairings.make_code(Role::Engineer);
        assert_eq!(code.len(), 9, "{code}");

        let request = asking(&code.to_lowercase().replace('-', " "));
        let (token, granted) = pairings
            .accept(&request, Some("10.0.0.5:8443".into()))
            .unwrap();
        assert_eq!(granted, Role::Engineer);
        let who = pairings.identify(&token).unwrap();
        assert_eq!(who.role, Role::Engineer);
        assert_eq!(who.via, format!("pairing:{}", request.from.id));
        assert!(pairings.identify(&new_token()).is_none());

        // Used up.
        assert_eq!(
            pairings.accept(&asking(&code), None),
            Err(PairingError::BadCode)
        );

        // Kept, without the token it accepts in the clear.
        let stored = std::fs::read_to_string(dir.path().join("pairings.json")).unwrap();
        assert!(!stored.contains(&token));
        assert!(
            stored.contains(&request.token),
            "the token it presents is kept to present it"
        );
        let reopened = Pairings::open(dir.path()).unwrap();
        assert!(reopened.identify(&token).is_some());

        assert!(reopened.forget(&request.from.id).unwrap().is_some());
        assert!(reopened.identify(&token).is_none());
    }

    #[test]
    fn wrong_guesses_use_a_code_up_and_no_code_pairs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let pairings = Pairings::open(dir.path()).unwrap();
        assert_eq!(
            pairings.accept(&asking("AAAA-BBBB"), None),
            Err(PairingError::BadCode)
        );
        let (code, _) = pairings.make_code(Role::Viewer);
        for _ in 0..CODE_ATTEMPTS {
            assert_eq!(
                pairings.accept(&asking("WRONG-ONE"), None),
                Err(PairingError::BadCode)
            );
        }
        assert_eq!(
            pairings.accept(&asking(&code), None),
            Err(PairingError::BadCode),
            "after five wrong guesses even the right code is refused"
        );
        assert!(pairings.peers().is_empty());
    }

    #[test]
    fn a_node_that_does_not_say_who_it_is_is_not_paired() {
        let dir = tempfile::tempdir().unwrap();
        let pairings = Pairings::open(dir.path()).unwrap();
        let (code, _) = pairings.make_code(Role::Engineer);
        let mut request = asking(&code);
        request.from.fingerprint = "short".into();
        assert!(matches!(
            pairings.accept(&request, None),
            Err(PairingError::Invalid(_))
        ));
        assert!(pairings.peers().is_empty());
    }
}
