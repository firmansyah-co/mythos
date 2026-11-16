//! Two real servers pair over HTTPS (ADR-012 §2.3): a node that presents another certificate
//! is refused before the code leaves; a code pairs once; each reaches the other as itself; an
//! unpairing reaches both.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    http: u16,
    https: u16,
    cookie: String,
    _dir: tempfile::TempDir,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One HTTP request to a server's plain HTTP port (its own computer), as `cookie`.
fn http(
    port: u16,
    method: &str,
    path: &str,
    cookie: &str,
    body: Option<Value>,
) -> (u16, String, Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let text = body.map(|b| b.to_string()).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nCookie: mythos_session={cookie}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut text = rest;
        while let Some((size, more)) = text.split_once("\r\n") {
            let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if size == 0 {
                break;
            }
            out.push_str(&more[..size]);
            text = more[size..].trim_start_matches("\r\n");
        }
        out
    } else {
        rest.to_string()
    };
    (
        status,
        head.to_string(),
        serde_json::from_str(&body).unwrap_or(Value::Null),
    )
}

fn start(name: &str) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let http_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let code = format!("pair-test-{name}-{}", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_mythos-server"))
        .env_remove("MYTHOS_ROLE")
        .env_remove("MYTHOS_BIND")
        .env("MYTHOS_STORAGE_ROOT", dir.path())
        .env("MYTHOS_HTTP_PORT", http_port.to_string())
        .env("MYTHOS_HTTPS_PORT", "0")
        .env("MYTHOS_SETUP_CODE", &code)
        .env(
            "MYTHOS_SHM_NAME",
            format!("mythos_pair_test_{name}_{}", std::process::id()),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The log is on standard output, the setup code on standard error: read both.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().unwrap()),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let (mut http_seen, mut https) = (false, None);
    while Instant::now() < deadline && !(http_seen && https.is_some()) {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
            if line.contains("listening on http://") {
                http_seen = true;
            }
            if let Some(rest) = line.split("listening on https://").nth(1) {
                let address = rest.split_whitespace().next().unwrap();
                https = address
                    .rsplit(':')
                    .next()
                    .and_then(|p| p.parse::<u16>().ok());
            }
        }
    }
    let https = https.expect("the server answers HTTPS");
    // Its first administrator, signed in by a cookie.
    let (status, head, body) = http(
        http_port,
        "POST",
        "/api/v1/auth/setup",
        "",
        Some(json!({"setup_code": code, "username": "admin", "password": "a long password 1"})),
    );
    assert_eq!(status, 201, "{body}");
    let cookie = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .starts_with("set-cookie:")
                .then(|| l.split("mythos_session=").nth(1))
                .flatten()
        })
        .and_then(|v| v.split(';').next())
        .expect("a session cookie")
        .to_string();
    Server {
        child,
        http: http_port,
        https,
        cookie,
        _dir: dir,
    }
}

impl Server {
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let (status, _, body) = http(self.http, method, path, &self.cookie, body);
        (status, body)
    }

    fn node(&self) -> Value {
        self.call("GET", "/api/v1/node", None).1
    }
}

#[test]
fn two_nodes_pair_over_pinned_https_and_unpair_on_both_sides() {
    let a = start("a");
    let b = start("b");
    let b_node = b.node();
    let b_id = b_node["id"].as_str().unwrap().to_string();
    let b_fingerprint = b_node["https"]["fingerprint"].as_str().unwrap().to_string();
    let b_spoken = b_node["https"]["fingerprint_spoken"]
        .as_str()
        .unwrap()
        .to_string();
    let a_id = a.node()["id"].as_str().unwrap().to_string();
    let b_address = format!("127.0.0.1:{}", b.https);

    let (status, made) = b.call("POST", "/api/v1/pairing/code", Some(json!({})));
    assert_eq!(status, 200, "{made}");
    let code = made["code"].as_str().unwrap().to_string();
    assert_eq!(made["granted"], "engineer");

    // A node that presents another certificate is refused before the code is sent: it stays
    // usable, and nothing is paired on either side.
    let (status, refused) = a.call(
        "POST",
        "/api/v1/pairings",
        Some(json!({"address": b_address, "fingerprint": "0".repeat(64), "code": code})),
    );
    assert_eq!(status, 502, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("not the one expected"),
        "{refused}"
    );
    assert_eq!(
        a.call("GET", "/api/v1/pairings", None).1["peers"],
        json!([])
    );
    assert_eq!(
        b.call("GET", "/api/v1/pairings", None).1["peers"],
        json!([])
    );

    // The fingerprint as a person reads it, in groups of four, is enough.
    let (status, paired) = a.call(
        "POST",
        "/api/v1/pairings",
        Some(json!({"address": b_address, "fingerprint": b_spoken, "code": code})),
    );
    assert_eq!(status, 201, "{paired}");
    assert_eq!(paired["peer"]["id"], b_id);
    assert_eq!(paired["peer"]["fingerprint"], b_fingerprint);
    assert_eq!(paired["peer"]["granted"], "viewer", "what b may do on a");
    assert_eq!(paired["granted_there"], "engineer", "what a may do on b");
    assert!(
        paired.to_string().find("mythos_pair_").is_none(),
        "no token is shown"
    );

    let on_b = b.call("GET", "/api/v1/pairings", None).1;
    assert_eq!(on_b["peers"][0]["id"], a_id, "{on_b}");
    assert_eq!(on_b["peers"][0]["granted"], "engineer");
    assert_eq!(
        on_b["peers"][0]["address"],
        format!("127.0.0.1:{}", a.https),
        "where a's connection came from, at the port it answers HTTPS on"
    );

    // The code is used up.
    let (status, again) = a.call(
        "POST",
        "/api/v1/pairings",
        Some(json!({"address": b_address, "fingerprint": b_fingerprint, "code": code})),
    );
    assert_eq!(status, 403, "{again}");

    // Each reaches the other as itself, over the pinned certificate.
    let (_, from_a) = a.call("GET", &format!("/api/v1/pairings/{b_id}/status"), None);
    assert_eq!(from_a["reachable"], true, "{from_a}");
    assert_eq!(from_a["node"]["id"], b_id);
    let (_, from_b) = b.call("GET", &format!("/api/v1/pairings/{a_id}/status"), None);
    assert_eq!(from_b["reachable"], true, "{from_b}");
    assert_eq!(from_b["node"]["id"], a_id);

    // Unpaired on a: b is told and forgets a; a's token no longer reaches b.
    let (status, gone) = a.call("DELETE", &format!("/api/v1/pairings/{b_id}"), None);
    assert_eq!(status, 200, "{gone}");
    assert_eq!(gone["told"], true, "{gone}");
    assert_eq!(
        a.call("GET", "/api/v1/pairings", None).1["peers"],
        json!([])
    );
    assert_eq!(
        b.call("GET", "/api/v1/pairings", None).1["peers"],
        json!([])
    );
    let (status, _) = a.call("GET", &format!("/api/v1/pairings/{b_id}/status"), None);
    assert_eq!(status, 404);
}

#[test]
fn a_pairing_code_is_made_by_an_administrator_only_and_a_peer_cannot_be_unpaired_by_anyone_else() {
    let b = start("c");
    // Signed out: refused.
    let (status, _, _) = http(b.http, "POST", "/api/v1/pairing/code", "", Some(json!({})));
    assert_eq!(status, 401);
    // Signed in, but not as the paired node: refused.
    let (status, body) = b.call("DELETE", "/api/v1/pairing/peers/someone", None);
    assert_eq!(status, 403, "{body}");
}

/// ADR-012 §2.4, §2.5: one plant project on node a, a device assigned to node b, downloaded
/// to it, compared, changed, downloaded again with the rest, and never called the same when b
/// cannot be reached.
#[test]
fn a_plant_downloads_a_device_to_its_paired_node_and_compares_it() {
    let a = start("plant-a");
    let mut b = start("plant-b");
    let b_node = b.node();
    let b_id = b_node["id"].as_str().unwrap().to_string();
    let a_id = a.node()["id"].as_str().unwrap().to_string();
    let (_, made) = b.call("POST", "/api/v1/pairing/code", Some(json!({})));
    let (status, paired) = a.call(
        "POST",
        "/api/v1/pairings",
        Some(json!({
            "address": format!("127.0.0.1:{}", b.https),
            "fingerprint": b_node["https"]["fingerprint"],
            "code": made["code"],
        })),
    );
    assert_eq!(status, 201, "{paired}");

    let (status, body) = a.call(
        "POST",
        "/api/v1/projects",
        Some(json!({"id": "plant_a", "name": "Boiler House", "description": "", "author": "test"})),
    );
    assert!(status == 200 || status == 201, "{status} {body}");
    let (_, plant) = a.call("GET", "/api/v1/projects/plant_a/plant", None);
    assert_eq!(plant["implicit"], true, "{plant}");
    assert_eq!(plant["devices"][0]["node"]["id"], a_id);

    // A device is assigned to a paired node, never to one merely named.
    let (status, refused) = a.call(
        "PUT",
        "/api/v1/projects/plant_a/plant/devices/stray",
        Some(json!({"name": "Stray", "node": "someone-heard"})),
    );
    assert_eq!(status, 422, "{refused}");
    let (status, device) = a.call(
        "PUT",
        "/api/v1/projects/plant_a/plant/devices/edge1",
        Some(json!({"name": "Boiler edge 1", "node": b_id})),
    );
    assert_eq!(status, 200, "{device}");
    assert_eq!(device["project"], "plant_a--edge1");
    assert_eq!(device["node"]["paired"], true);

    let compare = |s: &Server| {
        s.call(
            "GET",
            "/api/v1/projects/plant_a/plant/devices/edge1/compare",
            None,
        )
        .1
    };
    assert_eq!(compare(&a)["state"], "never_downloaded", "{}", compare(&a));

    let (status, _) = a.call(
        "POST",
        "/api/v1/projects/plant_a/plant/devices/edge1/download",
        Some(json!({})),
    );
    assert_eq!(status, 428, "a download is confirmed, or not done");
    let (status, done) = a.call(
        "POST",
        "/api/v1/projects/plant_a/plant/devices/edge1/download",
        Some(json!({"confirm": true})),
    );
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["state"], "done");
    let (_, active) = b.call("GET", "/api/v1/projects/active", None);
    assert_eq!(
        active["project_id"], "plant_a--edge1",
        "b runs the device it was given: {active}"
    );
    assert_eq!(compare(&a)["state"], "same", "{}", compare(&a));

    // Changed in the project: no longer the same, until downloaded again.
    let (status, body) = a.call(
        "POST",
        "/api/v1/projects/plant_a--edge1/datatypes",
        Some(json!({"name": "Motor", "kind": "STRUCT", "members": [{"name": "running", "dataType": "BOOL"}]})),
    );
    assert!(status == 200 || status == 201, "{status} {body}");
    assert_eq!(compare(&a)["state"], "differs", "{}", compare(&a));
    let (status, all) = a.call(
        "POST",
        "/api/v1/projects/plant_a/plant/download",
        Some(json!({"confirm": true})),
    );
    assert_eq!(status, 200, "{all}");
    assert_eq!(all["complete"], true, "{all}");
    assert_eq!(
        all["devices"].as_array().unwrap().len(),
        2,
        "this controller and edge1: {all}"
    );
    assert_eq!(compare(&a)["state"], "same");

    // b gone: unreachable, never the same.
    let _ = b.child.kill();
    let _ = b.child.wait();
    let gone = compare(&a);
    assert_eq!(gone["state"], "unreachable", "{gone}");
}
