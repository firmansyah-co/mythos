//! The server executable, started the way a person starts it: no `RUST_LOG`, no setup code of
//! its own choosing.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn a_new_server_tells_whoever_started_it_the_setup_code_and_logs_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mythos-server"))
        .env_remove("RUST_LOG")
        .env_remove("MYTHOS_SETUP_CODE")
        .env("MYTHOS_STORAGE_ROOT", dir.path())
        .env("MYTHOS_HTTP_PORT", port.to_string())
        .env(
            "MYTHOS_SHM_NAME",
            format!("mythos_binary_test_{}", std::process::id()),
        )
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
