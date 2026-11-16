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
    // After `child` is killed (Drop above), its tag bus segment goes too: a killed server
    // cannot remove its own, and each one left is memory until a reboot.
    _segment: Segment,
}

struct Segment(String);

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = mythos_pal::DefaultShm::unlink(&self.0);
    }
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
    start_as(name, None)
}

/// A server whose node takes `role` on its first start (an edge when `None`).
fn start_as(name: &str, role: Option<&str>) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let http_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let code = format!("pair-test-{name}-{}", std::process::id());
    let segment = Segment(format!("mythos_pair_test_{name}_{}", std::process::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_mythos-server"));
    command.env_remove("MYTHOS_ROLE");
    if let Some(role) = role {
        command.env("MYTHOS_ROLE", role);
    }
    let mut child = command
        .env_remove("MYTHOS_BIND")
        .env("MYTHOS_STORAGE_ROOT", dir.path())
        .env("MYTHOS_HTTP_PORT", http_port.to_string())
        .env("MYTHOS_HTTPS_PORT", "0")
        .env("MYTHOS_SETUP_CODE", &code)
        .env("MYTHOS_SHM_NAME", &segment.0)
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
        _segment: segment,
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

/// Pair `a` with `b` (b makes the code); b's node id.
fn pair(a: &Server, b: &Server) -> String {
    let b_node = b.node();
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
    b_node["id"].as_str().unwrap().to_string()
}

/// The sample of `uri` in `server`'s tag values, when there is one.
fn sample(server: &Server, uri: &str) -> Option<Value> {
    let (_, values) = server.call("GET", "/api/v1/tags/values", None);
    values["updates"]
        .as_array()?
        .iter()
        .find(|u| u["uri"] == uri)
        .cloned()
}

fn until(what: &str, mut check: impl FnMut() -> Option<Value>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(found) = check() {
            return found;
        }
        assert!(Instant::now() < deadline, "never: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// ADR-012 §2.6: a node reads the tags of another device of its plant over the pairing, named
/// with the device, live; when the link is down they are not current, never their last value.
#[test]
fn a_device_reads_another_devices_tags_and_never_shows_a_lost_value_as_current() {
    // The other device reads a real Modbus device, through the module beside the server.
    let module = std::path::Path::new(env!("CARGO_BIN_EXE_mythos-server")).with_file_name(format!(
        "mythos-module-modbus{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        module.is_file(),
        "{} is not built: run the workspace's tests (cargo test --workspace), which build it",
        module.display()
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let sim = runtime
        .block_on(driver_modbus::sim::ModbusSimServer::start("127.0.0.1:0"))
        .unwrap();
    runtime.block_on(sim.set_holding(0, 1234));

    let a = start("remote-a");
    let mut b = start("remote-b");
    let b_id = pair(&a, &b);
    let (status, body) = a.call(
        "POST",
        "/api/v1/projects",
        Some(json!({"id": "plant_r", "name": "Boiler House", "description": "", "author": "test"})),
    );
    assert!(status == 200 || status == 201, "{status} {body}");
    let (status, device) = a.call(
        "PUT",
        "/api/v1/projects/plant_r/plant/devices/edge1",
        Some(json!({"name": "Boiler edge 1", "node": b_id})),
    );
    assert_eq!(status, 200, "{device}");
    let (status, body) = a.call(
        "PUT",
        "/api/v1/projects/plant_r--edge1/io",
        Some(json!({"connections": [{
            "id": "plc1", "driver": "modbus", "poll_interval_ms": 50,
            "config": {"transport": "tcp", "host": "127.0.0.1", "port": sim.port(), "slave_id": 1,
                "timeout_ms": 300, "mappings": [{"uri": "external.Boiler.U1.p", "address": 0, "function_code": 3}]}
        }]})),
    );
    assert_eq!(status, 200, "{body}");
    let (status, done) = a.call(
        "POST",
        "/api/v1/projects/plant_r/plant/devices/edge1/download",
        Some(json!({"confirm": true})),
    );
    assert_eq!(status, 200, "{done}");
    until("b reads the field", || {
        sample(&b, "external.Boiler.U1.p").filter(|s| s["quality_name"] == "GOOD")
    });

    // a runs the plant: edge1's tag is read here, named with the device, as it is there.
    let (status, body) = a.call("POST", "/api/v1/projects/plant_r/activate", None);
    assert!(status == 200 || status == 204, "{status} {body}");
    let remote = until("a reads edge1's tag", || {
        sample(&a, "edge1:external.Boiler.U1.p").filter(|s| s["quality_name"] == "GOOD")
    });
    assert_eq!(remote["value"], 1234, "{remote}");
    assert!(remote.get("slot_id").is_none(), "no slot here: {remote}");
    let (_, links) = a.call("GET", "/api/v1/remote", None);
    assert_eq!(
        links["devices"].as_array().unwrap().len(),
        1,
        "the plant's own device, on a, is not followed from a: {links}"
    );
    assert_eq!(links["devices"][0]["device"], "edge1", "{links}");
    assert_eq!(links["devices"][0]["node"], b_id.as_str(), "{links}");
    assert_eq!(links["devices"][0]["state"], "connected", "{links}");

    // Live: a change on the field reaches a through b.
    runtime.block_on(sim.set_holding(0, 4321));
    until("a sees the new value", || {
        sample(&a, "edge1:external.Boiler.U1.p").filter(|s| s["value"] == 4321)
    });

    // b gone: the tag is still there, without a value, and bad.
    let _ = b.child.kill();
    let _ = b.child.wait();
    let lost = until("a says the link is down", || {
        sample(&a, "edge1:external.Boiler.U1.p").filter(|s| s["quality_name"] != "GOOD")
    });
    assert_eq!(lost["quality_name"], "BAD_COMM_FAILURE", "{lost}");
    assert_eq!(lost["quality"], "BAD", "{lost}");
    assert_eq!(lost["value"], Value::Null, "never the last value: {lost}");
    let (_, links) = a.call("GET", "/api/v1/remote", None);
    assert_eq!(links["devices"][0]["state"], "lost", "{links}");
    assert!(links["devices"][0]["reason"].is_string(), "{links}");
}

/// ADR-012 §2.6: a node the server cannot reach (here a workstation, which answers on its own
/// computer only, so it gives the server no address) calls home; the server's fleet reaches it
/// over that connection, and downloads and compares a device on it; when it stops calling, the
/// fleet says it cannot be reached, and compare never says the same.
#[test]
fn a_node_that_cannot_be_reached_calls_home_and_is_reached_over_it() {
    let server = start_as("fleet-server", Some("server"));
    let edge = start_as("fleet-edge", Some("workstation"));
    let server_node = server.node();
    let server_id = server_node["id"].as_str().unwrap().to_string();
    let edge_id = edge.node()["id"].as_str().unwrap().to_string();
    let (_, made) = server.call("POST", "/api/v1/pairing/code", Some(json!({})));
    let (status, paired) = edge.call(
        "POST",
        "/api/v1/pairings",
        Some(json!({
            "address": format!("127.0.0.1:{}", server.https),
            "fingerprint": server_node["https"]["fingerprint"],
            "code": made["code"],
            "grant": "engineer",
            "call_home": true,
        })),
    );
    assert_eq!(status, 201, "{paired}");
    assert_eq!(paired["peer"]["call_home"], true, "{paired}");
    let (_, peers) = server.call("GET", "/api/v1/pairings", None);
    assert_eq!(
        peers["peers"][0]["address"],
        Value::Null,
        "the server has no address for it: {peers}"
    );

    // Only a paired node calls home, as itself: an administrator's session is not one.
    let (status, refused) = server.call("POST", "/api/v1/callhome", Some(json!({})));
    assert_eq!(status, 403, "{refused}");

    let fleet = |s: &Server| s.call("GET", "/api/v1/fleet", None).1["nodes"][0].clone();
    let reached = until("the server reaches the edge over its call home", || {
        Some(fleet(&server)).filter(|n| n["reachable"] == true)
    });
    assert_eq!(reached["via"], "call_home", "{reached}");
    assert_eq!(reached["paired"]["id"], edge_id.as_str(), "{reached}");
    assert_eq!(reached["node"]["role"], "workstation", "{reached}");

    // A plant device on it: downloaded and compared over the call-home connection.
    let (status, body) = server.call(
        "POST",
        "/api/v1/projects",
        Some(json!({"id": "plant_f", "name": "Remote site", "description": "", "author": "test"})),
    );
    assert!(status == 200 || status == 201, "{status} {body}");
    let (status, device) = server.call(
        "PUT",
        "/api/v1/projects/plant_f/plant/devices/far",
        Some(json!({"name": "Far edge", "node": edge_id})),
    );
    assert_eq!(status, 200, "{device}");
    let (status, done) = server.call(
        "POST",
        "/api/v1/projects/plant_f/plant/devices/far/download",
        Some(json!({"confirm": true})),
    );
    assert_eq!(status, 200, "{done}");
    let (_, compared) = server.call(
        "GET",
        "/api/v1/projects/plant_f/plant/devices/far/compare",
        None,
    );
    assert_eq!(compared["state"], "same", "{compared}");
    let running = fleet(&server);
    assert_eq!(running["running"]["project"], "plant_f--far", "{running}");
    assert_eq!(
        running["running"]["hash"], compared["node_hash"],
        "{running}"
    );

    // It stops calling home: the fleet says it cannot be reached, and compare never says the same.
    let (status, stopped) = edge.call(
        "PUT",
        &format!("/api/v1/pairings/{server_id}"),
        Some(json!({"call_home": false})),
    );
    assert_eq!(status, 200, "{stopped}");
    // Its last polls may still be answered; then it is not calling home, and not reachable.
    let gone = until("the fleet says the edge is not calling home", || {
        Some(fleet(&server)).filter(|n| n["via"] == "none")
    });
    assert_eq!(gone["reachable"], false, "{gone}");
    assert!(
        gone["error"].as_str().unwrap().contains("not calling home"),
        "{gone}"
    );
    let (_, compared) = server.call(
        "GET",
        "/api/v1/projects/plant_f/plant/devices/far/compare",
        None,
    );
    assert_eq!(compared["state"], "unreachable", "{compared}");
}
