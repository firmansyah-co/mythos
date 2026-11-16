//! `mythos service install` for real (ADR-012 step 2): the operating system starts
//! mythos-server, it answers, and `mythos service uninstall` stops it again.
//!
//! It changes the machine it runs on, so it is ignored unless asked for with
//! `MYTHOS_TEST_SERVICE=1 cargo test -p mythos-cli --test service_roundtrip -- --ignored`.
//! CI runs it on its Windows and macOS runners. On Linux it installs a user service
//! (`systemctl --user`); on macOS a system daemon, through passwordless `sudo -n`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn health(port: u16) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    answer.lines().next().map(str::to_string)
}

fn until<T>(within: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(found) = probe() {
            return Some(found);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

fn mythos(args: &[&str]) -> std::process::Output {
    let exe = env!("CARGO_BIN_EXE_mythos");
    let mut command = if cfg!(target_os = "macos") {
        let mut sudo = Command::new("sudo");
        sudo.args(["-n", exe]);
        sudo
    } else {
        Command::new(exe)
    };
    command.args(args);
    if cfg!(target_os = "linux") {
        command.arg("--user");
    }
    command.output().expect("mythos runs")
}

fn server_beside_mythos() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_mythos")).parent().unwrap();
    let server = dir.join(format!("mythos-server{}", std::env::consts::EXE_SUFFIX));
    assert!(
        server.is_file(),
        "{} is not built: run `cargo build -p mythos-core --bin mythos-server` first",
        server.display()
    );
    server
}

#[test]
#[ignore = "installs a real service; MYTHOS_TEST_SERVICE=1 and --ignored to run it"]
fn a_service_starts_the_server_and_uninstalling_stops_it() {
    assert_eq!(
        std::env::var("MYTHOS_TEST_SERVICE").as_deref(),
        Ok("1"),
        "this test installs a real service: set MYTHOS_TEST_SERVICE=1 to say that is wanted"
    );
    let server = server_beside_mythos();
    let name = format!("mythos-test-{}", std::process::id());
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let storage = std::env::temp_dir().join(format!("{name}-storage"));
    let port_text = port.to_string();
    let storage_text = storage.display().to_string();
    let server_text = server.display().to_string();
    let common = ["--name", name.as_str(), "--storage", storage_text.as_str()];

    let installed = mythos(
        &[
            &["service", "install"][..],
            &common[..],
            &[
                "--port",
                &port_text,
                "--server",
                &server_text,
                "--role",
                "server",
            ][..],
        ]
        .concat(),
    );
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&installed.stdout),
        String::from_utf8_lossy(&installed.stderr)
    );
    println!("install: {said}");

    let answered = installed
        .status
        .success()
        .then(|| until(Duration::from_secs(90), || health(port)))
        .flatten();

    // Read the node before removing anything, then always uninstall.
    let node = std::fs::read(storage.join(".mythos-system").join("node.json")).ok();
    let removed = mythos(&[&["service", "uninstall"][..], &common[..]].concat());
    println!(
        "uninstall: {}{}",
        String::from_utf8_lossy(&removed.stdout),
        String::from_utf8_lossy(&removed.stderr)
    );
    let stopped = until(Duration::from_secs(60), || {
        health(port).is_none().then_some(())
    });

    assert!(installed.status.success(), "install failed: {said}");
    let answered = answered.expect("the installed service did not answer /api/v1/health");
    assert!(answered.contains("200"), "{answered}");
    let node: serde_json::Value =
        serde_json::from_slice(&node.expect("the service's node.json")).unwrap();
    assert_eq!(node["role"], "server", "{node}");
    assert!(removed.status.success(), "uninstall failed");
    assert!(
        stopped.is_some(),
        "the server still answers after the service was removed"
    );
}
