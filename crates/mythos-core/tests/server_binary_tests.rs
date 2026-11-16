//! The server executable, started the way a person starts it: no `RUST_LOG`, no setup code of
//! its own choosing.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The tag bus segment a test's server makes, removed when the test ends however it ends: a
/// server that is killed cannot remove its own, and each one left is memory until a reboot.
struct Segment(String);

impl Drop for Segment {
    fn drop(&mut self) {
        let _ = mythos_pal::DefaultShm::unlink(&self.0);
    }
}

#[test]
fn a_new_server_tells_whoever_started_it_the_setup_code_and_logs_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let segment = Segment(format!("mythos_binary_test_{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_mythos-server"))
        .env_remove("RUST_LOG")
        .env_remove("MYTHOS_SETUP_CODE")
        .env("MYTHOS_STORAGE_ROOT", dir.path())
        .env("MYTHOS_HTTP_PORT", port.to_string())
        .env("MYTHOS_SHM_NAME", &segment.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Read both streams until the setup code and an info line have both been seen.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
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
    let (mut code, mut info) = (None, false);
    while Instant::now() < deadline && !(code.is_some() && info) {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
            if let Some(rest) = line.split("with setup code ").nth(1) {
                code = rest.split_whitespace().next().map(str::to_string);
            }
            if line.contains("INFO") {
                info = true;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let code = code.expect("the server did not show its setup code");
    assert_eq!(code.len(), 16, "{code}");
    assert!(
        info,
        "nothing was logged at the info level without RUST_LOG"
    );
}

/// A started server's storage, and its tag bus segment, which goes when this does.
struct Home {
    dir: tempfile::TempDir,
    _segment: Segment,
}

impl Home {
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

/// Start the server with `env` on a free port; return it, its port, its storage and its lines.
fn start(
    env: &[(&str, &str)],
) -> (
    std::process::Child,
    u16,
    Home,
    std::sync::mpsc::Receiver<String>,
) {
    start_with_args(env, &[])
}

/// [`start`], with arguments.
fn start_with_args(
    env: &[(&str, &str)],
    args: &[&str],
) -> (
    std::process::Child,
    u16,
    Home,
    std::sync::mpsc::Receiver<String>,
) {
    let dir = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let segment = Segment(format!("mythos_role_test_{}_{port}", std::process::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_mythos-server"));
    command
        .env_remove("MYTHOS_ROLE")
        .env_remove("MYTHOS_BIND")
        .env("MYTHOS_STORAGE_ROOT", dir.path())
        .env("MYTHOS_HTTP_PORT", port.to_string())
        .env("MYTHOS_SHM_NAME", &segment.0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    command.args(args);
    let mut child = command.spawn().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
        Box::new(child.stderr.take().unwrap()),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    (
        child,
        port,
        Home {
            dir,
            _segment: segment,
        },
        rx,
    )
}

/// The address the server says it listens on.
fn listening(rx: &std::sync::mpsc::Receiver<String>) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
            if let Some(rest) = line.split("listening on http://").nth(1) {
                return rest.split_whitespace().next().map(str::to_string);
            }
        }
    }
    None
}

fn role_in(dir: &std::path::Path) -> String {
    let node: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join(".mythos-system").join("node.json")).expect("node.json"),
    )
    .unwrap();
    node["role"].as_str().unwrap().to_string()
}

/// ADR-012 §2.1: the role decides where the server listens; MYTHOS_BIND overrides it.
#[test]
fn where_the_server_listens_follows_the_node_role() {
    for (env, address, role) in [
        (vec![], "0.0.0.0", "edge"),
        (
            vec![("MYTHOS_ROLE", "workstation")],
            "127.0.0.1",
            "workstation",
        ),
        (vec![("MYTHOS_ROLE", "server")], "0.0.0.0", "server"),
        (vec![("MYTHOS_BIND", "127.0.0.1")], "127.0.0.1", "edge"),
    ] {
        let (mut child, port, dir, rx) = start(&env);
        let heard = listening(&rx);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(
            heard.as_deref(),
            Some(format!("{address}:{port}").as_str()),
            "{env:?}"
        );
        assert_eq!(role_in(dir.path()), role, "{env:?}");
    }
}

#[test]
fn a_role_that_does_not_exist_stops_the_server_and_says_why() {
    let (mut child, _, _dir, rx) = start(&[("MYTHOS_ROLE", "plc")]);
    let status = child.wait().unwrap();
    // Every line, until both streams have closed.
    let said: Vec<String> = rx.iter().collect();
    assert!(!status.success());
    assert!(
        said.iter()
            .any(|l| l.contains("MYTHOS_ROLE=plc names no role")),
        "{said:?}"
    );
}

/// A service starts the server with `--storage-root` and `--port` (`mythos service install`):
/// they win over the environment.
#[test]
fn arguments_choose_the_storage_and_the_port_over_the_environment() {
    let storage = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (mut child, env_port, env_dir, rx) = {
        let (child, env_port, env_dir, rx) = start_with_args(
            &[],
            &[
                "--storage-root",
                storage.path().to_str().unwrap(),
                "--port",
                &port.to_string(),
            ],
        );
        (child, env_port, env_dir, rx)
    };
    let heard = listening(&rx);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        heard,
        Some(format!("0.0.0.0:{port}")),
        "the environment said {env_port}"
    );
    assert!(storage
        .path()
        .join(".mythos-system")
        .join("node.json")
        .is_file());
    assert!(
        !env_dir.path().join(".mythos-system").exists(),
        "the environment's storage was not used"
    );

    let (mut child, _, _dir, rx) = start_with_args(&[], &["--port", "seventy"]);
    let status = child.wait().unwrap();
    let said: Vec<String> = rx.iter().collect();
    assert!(!status.success());
    assert!(
        said.iter()
            .any(|l| l.contains("--port seventy is not a port number")),
        "{said:?}"
    );
}
