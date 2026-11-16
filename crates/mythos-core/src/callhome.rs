//! Call home (ADR-012 §2.6): a node that cannot be reached (another subnet, behind NAT, a
//! workstation that listens on its own computer only) opens the connection itself, to a node
//! it is paired with, and is reached over it.
//!
//! The connection is a long poll over the same pinned HTTPS the two nodes already trust. The
//! node calling home asks `POST /api/v1/callhome` for work, presenting its pairing token; the
//! node it calls holds the request open for up to [`POLL_WAIT`] and answers with the next
//! request it has for it, or with nothing. The caller runs that request on its own router, as
//! the node that sent it (the request carries that node's pairing token, checked as for any
//! other), and brings the answer on its next poll.
//!
//! Every request one node makes of another goes through [`ask`]: over the call-home connection
//! while the other node is calling home, otherwise directly.

use crate::pairing::{Pairings, Peer};
use crate::tls::Answer;
use axum::body::Body;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tower::ServiceExt;

/// How long the called node holds a poll open with nothing to send.
pub const POLL_WAIT: Duration = Duration::from_secs(5);
/// A node that has not polled for this long is not calling home.
pub const LIVE: Duration = Duration::from_secs(12);
/// How long a request over the call-home connection may take, answer included.
pub const ANSWER_WAIT: Duration = Duration::from_secs(30);
/// The largest answer a poll carries.
pub const ANSWER_LIMIT: usize = 64 * 1024 * 1024;

/// A request for the node calling home.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub method: String,
    pub path: String,
    /// The token the asking node presents to it: the request is checked as if made directly.
    pub bearer: String,
    #[serde(default)]
    pub body: Option<Value>,
}

/// The answer to a [`Job`], brought on the next poll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobAnswer {
    pub id: u64,
    pub status: u16,
    #[serde(default)]
    pub body: Value,
}

struct Session {
    jobs: mpsc::UnboundedSender<Job>,
    queue: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Job>>>,
    last_poll: Instant,
}

/// The nodes calling this one home, and the requests waiting for them.
#[derive(Default)]
pub struct CallHome {
    sessions: Mutex<HashMap<String, Session>>,
    answers: Mutex<HashMap<u64, oneshot::Sender<JobAnswer>>>,
    next: AtomicU64,
}

impl CallHome {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `node` is calling home now.
    pub fn is_live(&self, node: &str) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(node)
            .is_some_and(|s| s.last_poll.elapsed() < LIVE)
    }

    /// When `node` last polled, if it ever did since this node started.
    pub fn last_poll(&self, node: &str) -> Option<Duration> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(node)
            .map(|s| s.last_poll.elapsed())
    }

    /// The queue of `node`, made on its first poll. A poll arriving is what keeps it live: one
    /// that is still open is not, so a node that stopped calling is not live [`LIVE`] after its
    /// last poll began.
    fn queue_of(&self, node: &str) -> Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Job>>> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        let session = sessions.entry(node.to_string()).or_insert_with(|| {
            let (jobs, queue) = mpsc::unbounded_channel();
            Session {
                jobs,
                queue: Arc::new(tokio::sync::Mutex::new(queue)),
                last_poll: Instant::now(),
            }
        });
        session.last_poll = Instant::now();
        Arc::clone(&session.queue)
    }

    /// One poll from `node`: take its answer, then wait up to [`POLL_WAIT`] for its next job.
    pub async fn poll(&self, node: &str, answer: Option<JobAnswer>) -> Option<Job> {
        if let Some(answer) = answer {
            let waiting = self
                .answers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&answer.id);
            if let Some(waiting) = waiting {
                let _ = waiting.send(answer);
            }
        }
        let queue = self.queue_of(node);
        // One poll at a time takes from a node's queue; a second one waits its turn.
        let mut queue = match tokio::time::timeout(POLL_WAIT, queue.lock()).await {
            Ok(queue) => queue,
            Err(_) => return None,
        };
        // A request whose asker stopped waiting is dropped, never run late: a download reported
        // as not done must not happen when the node next calls.
        let deadline = tokio::time::Instant::now() + POLL_WAIT;
        loop {
            let job = tokio::time::timeout_at(deadline, queue.recv())
                .await
                .ok()
                .flatten()?;
            let waited_for = self
                .answers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains_key(&job.id);
            if waited_for {
                return Some(job);
            }
        }
    }

    /// Send a request to `peer` over its call-home connection and wait for the answer.
    pub async fn call(
        &self,
        peer: &Peer,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Answer, String> {
        if !self.is_live(&peer.id) {
            return Err(format!("{} is not calling home", peer.name));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        self.answers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        // However this ends (answered, timed out, or the asker itself given up on and dropped),
        // the request is no longer waited for, so it is never sent late.
        let _waiting = Waiting { home: self, id };
        let job = Job {
            id,
            method: method.to_string(),
            path: path.to_string(),
            bearer: peer.token_to_it.clone(),
            body,
        };
        let sent = self
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&peer.id)
            .map(|s| s.jobs.send(job).is_ok())
            .unwrap_or(false);
        if !sent {
            return Err(format!("{} is not calling home", peer.name));
        }
        match tokio::time::timeout(ANSWER_WAIT, rx).await {
            Ok(Ok(answer)) => Ok(Answer {
                status: answer.status,
                body: answer.body,
            }),
            _ => Err(format!(
                "{} did not answer over its call-home connection within {} s",
                peer.name,
                ANSWER_WAIT.as_secs()
            )),
        }
    }

    fn forget_answer(&self, id: u64) {
        self.answers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
    }
}

/// A request still waited for; dropped, it is not.
struct Waiting<'a> {
    home: &'a CallHome,
    id: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.home.forget_answer(self.id);
    }
}

/// How a request to `peer` goes: over its call-home connection while it calls home, otherwise
/// directly over pinned HTTPS. `None` for `call_home` when this node keeps no such connections.
pub async fn ask(
    call_home: Option<&CallHome>,
    peer: &Peer,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Answer, String> {
    if let Some(call_home) = call_home.filter(|c| c.is_live(&peer.id)) {
        return call_home.call(peer, method, path, body).await;
    }
    let address = peer.address.clone().ok_or_else(|| {
        format!(
            "node '{}' cannot be reached: it answers on its own computer only, and is not calling home",
            peer.name
        )
    })?;
    let target = address
        .to_socket_addrs()
        .map_err(|e| format!("{address}: {e}"))?
        .next()
        .ok_or_else(|| format!("'{address}' names no address"))?;
    let fingerprint = peer.fingerprint.clone();
    let token = peer.token_to_it.clone();
    let method = method.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        crate::tls::request_pinned(
            target,
            &fingerprint,
            &method,
            &path,
            Some(&token),
            body.as_ref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether `peer` is reached over its call-home connection now.
pub fn via(call_home: Option<&CallHome>, peer: &Peer) -> &'static str {
    if call_home.is_some_and(|c| c.is_live(&peer.id)) {
        "call_home"
    } else if peer.address.is_some() {
        "direct"
    } else {
        "none"
    }
}

/// Run `job` on this node's own router, as the node that sent it.
async fn run(router: &Router, job: &Job) -> JobAnswer {
    let mut request = axum::http::Request::builder()
        .method(job.method.as_str())
        .uri(job.path.as_str())
        .header("authorization", format!("Bearer {}", job.bearer));
    let body = match &job.body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let answer = |status: u16, body: Value| JobAnswer {
        id: job.id,
        status,
        body,
    };
    let request = match request.body(body) {
        Ok(request) => request,
        Err(e) => return answer(400, Value::String(e.to_string())),
    };
    // The router cannot fail to answer: its error type has no value.
    let response = match router.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status().as_u16();
    match axum::body::to_bytes(response.into_body(), ANSWER_LIMIT).await {
        Ok(bytes) if bytes.is_empty() => answer(status, Value::Null),
        Ok(bytes) => answer(
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned())),
        ),
        Err(e) => answer(500, Value::String(format!("the answer is too large: {e}"))),
    }
}

/// Call home to every paired node this one is told to (`call_home` on its pairing), for as long
/// as it is told to. Runs for the life of the server.
pub fn spawn_callers(pairings: Arc<Pairings>, router: Router) {
    tokio::spawn(async move {
        let mut running: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
        loop {
            let wanted: Vec<Peer> = pairings
                .peers()
                .into_iter()
                .filter(|p| p.call_home)
                .collect();
            running.retain(|id, task| {
                let keep = wanted.iter().any(|p| &p.id == id) && !task.is_finished();
                if !keep {
                    task.abort();
                }
                keep
            });
            for peer in wanted {
                if let std::collections::hash_map::Entry::Vacant(slot) = running.entry(peer.id) {
                    let node = slot.key().clone();
                    slot.insert(tokio::spawn(call(
                        Arc::clone(&pairings),
                        node,
                        router.clone(),
                    )));
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

/// Call `node` home until its pairing says not to, or is gone.
async fn call(pairings: Arc<Pairings>, node: String, router: Router) {
    let mut answer: Option<JobAnswer> = None;
    let mut wait = Duration::from_millis(500);
    loop {
        let Some(peer) = pairings.peer(&node).filter(|p| p.call_home) else {
            return;
        };
        let Some(address) = peer.address.clone() else {
            tracing::warn!("cannot call {} home: it has no address", peer.name);
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        };
        let target = match address.to_socket_addrs().map(|mut a| a.next()) {
            Ok(Some(target)) => target,
            _ => {
                tracing::warn!(
                    "cannot call {} home: '{address}' names no address",
                    peer.name
                );
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        let body = serde_json::json!({ "answer": answer.take() });
        let fingerprint = peer.fingerprint.clone();
        let token = peer.token_to_it.clone();
        let polled = tokio::task::spawn_blocking(move || {
            crate::tls::request_pinned(
                target,
                &fingerprint,
                "POST",
                "/api/v1/callhome",
                Some(&token),
                Some(&body),
            )
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        match polled {
            Ok(polled) if polled.status == 200 => {
                wait = Duration::from_millis(500);
                if let Ok(job) = serde_json::from_value::<Job>(polled.body["job"].clone()) {
                    answer = Some(run(&router, &job).await);
                }
            }
            Ok(polled) if polled.status == 204 => {
                wait = Duration::from_millis(500);
            }
            Ok(polled) => {
                tracing::warn!(
                    "{} refused this node's call home: {} {}",
                    peer.name,
                    polled.status,
                    polled.body
                );
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(30));
            }
            Err(e) => {
                tracing::debug!("calling {} home: {e}", peer.name);
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(30));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Role;

    fn peer(id: &str) -> Peer {
        Peer {
            id: id.into(),
            name: format!("node {id}"),
            role: "edge".into(),
            address: None,
            fingerprint: "ab".repeat(32),
            granted: Role::Engineer,
            token_to_it: "mythos_pair_secret".into(),
            token_from_it_sha256: String::new(),
            paired_unix_s: 0,
            last_seen_unix_s: None,
            call_home: false,
        }
    }

    #[tokio::test]
    async fn a_request_goes_out_on_a_poll_and_its_answer_comes_back_on_the_next() {
        let home = Arc::new(CallHome::new());
        assert!(!home.is_live("n1"));
        // Nothing to send: a poll waits, then comes back empty, and the node is calling.
        let started = Instant::now();
        assert!(home.poll("n1", None).await.is_none());
        assert!(started.elapsed() >= POLL_WAIT - Duration::from_millis(50));
        assert!(home.is_live("n1"));

        let asking = {
            let home = Arc::clone(&home);
            tokio::spawn(async move { home.call(&peer("n1"), "GET", "/api/v1/node", None).await })
        };
        let job = home
            .poll("n1", None)
            .await
            .expect("the request goes out on a poll");
        assert_eq!(
            (job.method.as_str(), job.path.as_str()),
            ("GET", "/api/v1/node")
        );
        assert_eq!(
            job.bearer, "mythos_pair_secret",
            "checked there as if made directly"
        );
        let answer = JobAnswer {
            id: job.id,
            status: 200,
            body: serde_json::json!({"name": "far"}),
        };
        let next = tokio::spawn({
            let home = Arc::clone(&home);
            async move { home.poll("n1", Some(answer)).await }
        });
        let answered = asking.await.unwrap().unwrap();
        assert_eq!(answered.status, 200);
        assert_eq!(answered.body["name"], "far");
        next.abort();
    }

    #[tokio::test]
    async fn a_request_given_up_on_is_never_sent_late() {
        let home = Arc::new(CallHome::new());
        assert!(home.poll("n3", None).await.is_none());
        // Asked, and given up on before the node polls again.
        let gave_up = tokio::time::timeout(
            Duration::from_millis(10),
            home.call(&peer("n3"), "PUT", "/api/v1/slices/x", None),
        )
        .await;
        assert!(gave_up.is_err());
        assert!(
            home.poll("n3", None).await.is_none(),
            "the request was dropped, not run late"
        );
    }

    #[tokio::test]
    async fn a_node_that_is_not_calling_is_not_waited_for() {
        let home = CallHome::new();
        let refused = home.call(&peer("n2"), "GET", "/api/v1/node", None).await;
        assert!(refused.unwrap_err().contains("not calling home"));
        assert_eq!(via(Some(&home), &peer("n2")), "none");
    }
}
