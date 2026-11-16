//! `mythos service`: run `mythos-server` as a service of the operating system, started at boot
//! and started again when it stops (ADR-012 §2.1, step 2).
//!
//! | OS | What is installed | Started by |
//! | :--- | :--- | :--- |
//! | Linux | a systemd unit (`/etc/systemd/system`, or `~/.config/systemd/user` with `--user`) | systemd |
//! | macOS | a launchd property list (`/Library/LaunchDaemons`, or `~/Library/LaunchAgents`) | launchd |
//! | Windows | a Task Scheduler task, at startup, as SYSTEM, restarted when it stops | Task Scheduler |
//!
//! The service runs `mythos-server` itself, with `--storage-root` and `--port`: no wrapper
//! process, so stopping the service stops the server.
//!
//! What is installed is computed first ([`plan_install`]: files and commands, no side effect),
//! so `--dry-run` prints exactly what would be done, and every platform's definition is tested
//! on every platform.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    pub fn this() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// What the service runs.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    /// The service's name: `mythos` unless several run on one machine.
    pub name: String,
    /// The `mythos-server` executable.
    pub server: PathBuf,
    /// Its storage root (`MYTHOS_STORAGE_ROOT`).
    pub storage: PathBuf,
    /// Its HTTP port (`MYTHOS_HTTP_PORT`).
    pub port: u16,
    /// Its HTTPS port, when not the server's default (8443); 0 is any free one.
    pub https_port: Option<u16>,
    /// Installed for the signed-in user rather than the whole machine (Linux, macOS).
    pub user: bool,
    /// Home directory, for a user-level install.
    pub home: PathBuf,
    /// The user's numeric id, for launchd's `gui/<uid>` domain (macOS, `--user`).
    pub uid: Option<u32>,
}

/// A file to write: its path and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Everything an install, an uninstall or a status does, before any of it is done.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub files: Vec<PlannedFile>,
    /// Files to delete (an uninstall).
    pub remove: Vec<PathBuf>,
    /// Commands, in order; each is a program and its arguments.
    pub commands: Vec<Vec<String>>,
    /// Said to the person after it is done.
    pub notes: Vec<String>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A valid service name: what every platform accepts in a file and task name.
pub fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "'{name}' is not a service name: use 1 to 64 letters, digits, '-' or '_'"
        ));
    }
    Ok(())
}

/// The separator of `os`'s paths. A plan is made for one platform on any platform (its tests run
/// everywhere), so it never joins paths with the separator of the machine it runs on.
fn separator(os: Os) -> char {
    if os == Os::Windows {
        '\\'
    } else {
        '/'
    }
}

/// `dir` and then `name`, as `os` writes a path.
fn join(os: Os, dir: &Path, name: &str) -> PathBuf {
    let dir = dir.display().to_string();
    let sep = separator(os);
    let dir = dir.trim_end_matches(['/', '\\']);
    PathBuf::from(format!("{dir}{sep}{name}"))
}

/// The directory `path` is in, as `os` reads a path; `path` itself when it has none.
fn parent(os: Os, path: &Path) -> PathBuf {
    let text = path.display().to_string();
    let cut = if os == Os::Windows {
        text.rfind(['/', '\\'])
    } else {
        text.rfind('/')
    };
    match cut {
        Some(0) => PathBuf::from(&text[..1]),
        Some(at) => PathBuf::from(&text[..at]),
        None => path.to_path_buf(),
    }
}

fn linux_unit_path(spec: &ServiceSpec) -> PathBuf {
    let dir = if spec.user {
        join(Os::Linux, &spec.home, ".config/systemd/user")
    } else {
        PathBuf::from("/etc/systemd/system")
    };
    join(Os::Linux, &dir, &format!("{}.service", spec.name))
}

fn macos_label(spec_name: &str) -> String {
    format!("org.mythos.{spec_name}")
}

fn macos_plist_path(spec: &ServiceSpec) -> PathBuf {
    let dir = if spec.user {
        join(Os::MacOs, &spec.home, "Library/LaunchAgents")
    } else {
        PathBuf::from("/Library/LaunchDaemons")
    };
    join(
        Os::MacOs,
        &dir,
        &format!("{}.plist", macos_label(&spec.name)),
    )
}

fn macos_domain(spec: &ServiceSpec) -> String {
    if spec.user {
        format!("gui/{}", spec.uid.unwrap_or(0))
    } else {
        "system".to_string()
    }
}

/// `systemctl`, with `--user` for a user-level service.
fn systemctl(spec: &ServiceSpec, args: &[&str]) -> Vec<String> {
    let mut command = vec!["systemctl".to_string()];
    if spec.user {
        command.push("--user".into());
    }
    command.extend(args.iter().map(|a| a.to_string()));
    command
}

/// Escape `text` for XML character data and attribute values.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// UTF-16 LE with a byte-order mark: the encoding `schtasks /XML` reads.
fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn windows_task_path(spec: &ServiceSpec) -> PathBuf {
    join(
        Os::Windows,
        &parent(Os::Windows, &spec.storage),
        &format!("{}-service.xml", spec.name),
    )
}

/// What installing `spec` on `os` writes and runs.
pub fn plan_install(os: Os, spec: &ServiceSpec) -> Result<Plan, String> {
    valid_name(&spec.name)?;
    let server = spec.server.display().to_string();
    let storage = spec.storage.display().to_string();
    let mut plan = Plan::default();
    match os {
        Os::Linux => {
            let unit = format!(
                "[Unit]\n\
                 Description=MythOS ({name})\n\
                 After=network-online.target\n\
                 Wants=network-online.target\n\
                 \n\
                 [Service]\n\
                 ExecStart=\"{server}\" --storage-root \"{storage}\" --port {port}{https}\n\
                 Restart=always\n\
                 RestartSec=5\n\
                 \n\
                 [Install]\n\
                 WantedBy={target}\n",
                name = spec.name,
                port = spec.port,
                https = spec
                    .https_port
                    .map(|p| format!(" --https-port {p}"))
                    .unwrap_or_default(),
                target = if spec.user {
                    "default.target"
                } else {
                    "multi-user.target"
                },
            );
            plan.files.push(PlannedFile {
                path: linux_unit_path(spec),
                bytes: unit.into_bytes(),
            });
            plan.commands.push(systemctl(spec, &["daemon-reload"]));
            plan.commands.push(systemctl(
                spec,
                &["enable", "--now", &format!("{}.service", spec.name)],
            ));
            if spec.user {
                plan.notes.push(
                    "A user service runs while you are signed in; to keep it running after you \
                     sign out and at boot, run `loginctl enable-linger` once."
                        .into(),
                );
            }
        }
        Os::MacOs => {
            let label = macos_label(&spec.name);
            let log = join(
                Os::MacOs,
                &parent(Os::MacOs, &spec.storage),
                &format!("{}.log", spec.name),
            );
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{server}</string>
    <string>--storage-root</string><string>{storage}</string>
    <string>--port</string><string>{port}</string>{https}
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
                label = xml(&label),
                server = xml(&server),
                storage = xml(&storage),
                port = spec.port,
                https = spec
                    .https_port
                    .map(|p| format!("\n    <string>--https-port</string><string>{p}</string>"))
                    .unwrap_or_default(),
                log = xml(&log.display().to_string()),
            );
            let path = macos_plist_path(spec);
            plan.files.push(PlannedFile {
                path: path.clone(),
                bytes: plist.into_bytes(),
            });
            plan.commands.push(vec![
                "launchctl".into(),
                "bootstrap".into(),
                macos_domain(spec),
                path.display().to_string(),
            ]);
        }
        Os::Windows => {
            // Restarted every minute it is down, without end; no time limit on a run.
            let task = format!(
                r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>MythOS ({name})</Description></RegistrationInfo>
  <Triggers><BootTrigger><Enabled>true</Enabled></BootTrigger></Triggers>
  <Principals>
    <Principal id="Author"><UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>
    <StartWhenAvailable>true</StartWhenAvailable>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{server}</Command><Arguments>--storage-root "{storage}" --port {port}{https}</Arguments></Exec>
  </Actions>
</Task>
"#,
                name = xml(&spec.name),
                server = xml(&server),
                storage = xml(&storage),
                port = spec.port,
                https = spec
                    .https_port
                    .map(|p| format!(" --https-port {p}"))
                    .unwrap_or_default(),
            );
            let task_path = windows_task_path(spec);
            plan.files.push(PlannedFile {
                path: task_path.clone(),
                bytes: utf16(&task),
            });
            plan.commands.push(strings(&[
                "schtasks",
                "/Create",
                "/TN",
                &spec.name,
                "/XML",
                &task_path.display().to_string(),
                "/F",
            ]));
            plan.commands
                .push(strings(&["schtasks", "/Run", "/TN", &spec.name]));
        }
    }
    plan.notes.push(format!(
        "{} runs {} on port {} with storage {}.",
        spec.name, server, spec.port, storage
    ));
    Ok(plan)
}

/// What uninstalling the service named in `spec` on `os` runs and removes. Its storage is kept.
pub fn plan_uninstall(os: Os, spec: &ServiceSpec) -> Result<Plan, String> {
    valid_name(&spec.name)?;
    let mut plan = Plan::default();
    match os {
        Os::Linux => {
            plan.commands.push(systemctl(
                spec,
                &["disable", "--now", &format!("{}.service", spec.name)],
            ));
            plan.remove.push(linux_unit_path(spec));
            plan.commands.push(systemctl(spec, &["daemon-reload"]));
        }
        Os::MacOs => {
            plan.commands.push(vec![
                "launchctl".into(),
                "bootout".into(),
                format!("{}/{}", macos_domain(spec), macos_label(&spec.name)),
            ]);
            plan.remove.push(macos_plist_path(spec));
        }
        Os::Windows => {
            plan.commands
                .push(strings(&["schtasks", "/End", "/TN", &spec.name]));
            plan.commands
                .push(strings(&["schtasks", "/Delete", "/TN", &spec.name, "/F"]));
            plan.remove.push(windows_task_path(spec));
        }
    }
    plan.notes.push(format!(
        "The storage at {} is kept.",
        spec.storage.display()
    ));
    Ok(plan)
}

/// What asks the operating system about the service.
pub fn status_command(os: Os, spec: &ServiceSpec) -> Vec<String> {
    match os {
        Os::Linux => systemctl(
            spec,
            &["status", "--no-pager", &format!("{}.service", spec.name)],
        ),
        Os::MacOs => vec![
            "launchctl".into(),
            "print".into(),
            format!("{}/{}", macos_domain(spec), macos_label(&spec.name)),
        ],
        Os::Windows => strings(&["schtasks", "/Query", "/TN", &spec.name, "/V", "/FO", "LIST"]),
    }
}

/// Describe `plan` the way `--dry-run` prints it.
pub fn describe(plan: &Plan) -> String {
    let mut out = String::new();
    for file in &plan.files {
        out.push_str(&format!("write {}\n", file.path.display()));
        let text = if file.bytes.starts_with(&[0xFF, 0xFE]) {
            let units: Vec<u16> = file.bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(&file.bytes).into_owned()
        };
        for line in text.lines() {
            out.push_str(&format!("  | {line}\n"));
        }
    }
    for path in &plan.remove {
        out.push_str(&format!("remove {}\n", path.display()));
    }
    for command in &plan.commands {
        out.push_str(&format!("run {}\n", command.join(" ")));
    }
    out
}

/// Do `plan`: write its files, run its commands in order (an uninstall goes on past a command
/// that fails, so a half-installed service can still be removed), then remove its files.
pub fn carry_out(plan: &Plan, stop_on_failure: bool) -> Result<Vec<String>, String> {
    let mut failures = Vec::new();
    for file in &plan.files {
        if let Some(dir) = file.path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("{} cannot be made: {e}", dir.display()))?;
        }
        std::fs::write(&file.path, &file.bytes)
            .map_err(|e| format!("{} cannot be written: {e}", file.path.display()))?;
    }
    for command in &plan.commands {
        let output = std::process::Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|e| format!("`{}` cannot be run: {e}", command[0]))?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let failure = format!("`{}` failed: {said}", command.join(" "));
            if stop_on_failure {
                return Err(failure);
            }
            failures.push(failure);
        }
    }
    for path in &plan.remove {
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!("{} cannot be removed: {e}", path.display()));
            }
        }
    }
    Ok(failures)
}

/// The `mythos-server` the service runs: the installation's `current` one (so an update or a
/// rollback is what the service runs next), else the one beside this `mythos`.
pub fn default_server(mythos_dir: &Path) -> Option<PathBuf> {
    let exe = format!("mythos-server{}", std::env::consts::EXE_SUFFIX);
    let current = mythos_dir.join("current").join("bin").join(&exe);
    if current.is_file() {
        return Some(current);
    }
    std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(|dir| dir.join(&exe)))
        .filter(|beside| beside.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(user: bool) -> ServiceSpec {
        ServiceSpec {
            name: "mythos".into(),
            server: PathBuf::from("/opt/mythos/current/bin/mythos-server"),
            storage: PathBuf::from("/var/lib/mythos/projects"),
            port: 8080,
            https_port: None,
            user,
            home: PathBuf::from("/home/eng"),
            uid: Some(501),
        }
    }

    #[test]
    fn linux_runs_it_under_systemd_and_restarts_it() {
        let plan = plan_install(Os::Linux, &spec(false)).unwrap();
        assert_eq!(
            plan.files[0].path,
            PathBuf::from("/etc/systemd/system/mythos.service")
        );
        let unit = String::from_utf8(plan.files[0].bytes.clone()).unwrap();
        assert!(
            unit.contains("ExecStart=\"/opt/mythos/current/bin/mythos-server\" --storage-root \"/var/lib/mythos/projects\" --port 8080"),
            "{unit}"
        );
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("WantedBy=multi-user.target"));
        assert_eq!(
            plan.commands,
            vec![
                strings(&["systemctl", "daemon-reload"]),
                strings(&["systemctl", "enable", "--now", "mythos.service"]),
            ]
        );

        let mut https = spec(false);
        https.https_port = Some(9443);
        for os in [Os::Linux, Os::MacOs, Os::Windows] {
            let text = describe(&plan_install(os, &https).unwrap());
            assert!(
                text.contains("--https-port 9443")
                    || text.contains("<string>--https-port</string><string>9443</string>"),
                "{os:?} passes the HTTPS port: {text}"
            );
        }
        assert!(
            !unit.contains("--https-port"),
            "the server's default unless asked"
        );

        let user = plan_install(Os::Linux, &spec(true)).unwrap();
        assert_eq!(
            user.files[0].path,
            PathBuf::from("/home/eng/.config/systemd/user/mythos.service")
        );
        assert!(String::from_utf8_lossy(&user.files[0].bytes).contains("WantedBy=default.target"));
        assert_eq!(user.commands[1][..2], strings(&["systemctl", "--user"]));
        assert!(user.notes[0].contains("loginctl enable-linger"));

        let gone = plan_uninstall(Os::Linux, &spec(false)).unwrap();
        assert_eq!(
            gone.commands[0],
            strings(&["systemctl", "disable", "--now", "mythos.service"])
        );
        assert_eq!(
            gone.remove,
            vec![PathBuf::from("/etc/systemd/system/mythos.service")]
        );
    }

    #[test]
    fn macos_runs_it_under_launchd_and_keeps_it_alive() {
        let plan = plan_install(Os::MacOs, &spec(false)).unwrap();
        assert_eq!(
            plan.files[0].path,
            PathBuf::from("/Library/LaunchDaemons/org.mythos.mythos.plist")
        );
        let plist = String::from_utf8(plan.files[0].bytes.clone()).unwrap();
        for needle in [
            "<key>Label</key><string>org.mythos.mythos</string>",
            "<string>/opt/mythos/current/bin/mythos-server</string>",
            "<string>--storage-root</string><string>/var/lib/mythos/projects</string>",
            "<string>--port</string><string>8080</string>",
            "<key>KeepAlive</key><true/>",
            "<key>RunAtLoad</key><true/>",
        ] {
            assert!(plist.contains(needle), "{needle} not in {plist}");
        }
        assert_eq!(
            plan.commands[0],
            strings(&[
                "launchctl",
                "bootstrap",
                "system",
                "/Library/LaunchDaemons/org.mythos.mythos.plist"
            ])
        );
        assert!(
            plist.contains("<string>/var/lib/mythos/mythos.log</string>"),
            "the log beside the storage: {plist}"
        );
        let user = plan_install(Os::MacOs, &spec(true)).unwrap();
        assert_eq!(
            user.files[0].path,
            PathBuf::from("/home/eng/Library/LaunchAgents/org.mythos.mythos.plist")
        );
        assert_eq!(user.commands[0][2], "gui/501");
        let gone = plan_uninstall(Os::MacOs, &spec(true)).unwrap();
        assert_eq!(
            gone.commands[0],
            strings(&["launchctl", "bootout", "gui/501/org.mythos.mythos"])
        );
    }

    #[test]
    fn windows_runs_it_at_startup_as_system_and_restarts_it() {
        let mut windows = spec(false);
        windows.server = PathBuf::from(r"C:\Program Files\MythOS\current\bin\mythos-server.exe");
        windows.storage = PathBuf::from(r"C:\ProgramData\MythOS\projects");
        let plan = plan_install(Os::Windows, &windows).unwrap();
        assert_eq!(
            plan.files.len(),
            1,
            "no wrapper script: the task runs the server itself"
        );
        assert_eq!(
            plan.files[0].path,
            PathBuf::from(r"C:\ProgramData\MythOS\mythos-service.xml"),
            "beside the storage, as Windows writes a path, wherever the plan is made"
        );
        let task = &plan.files[0].bytes;
        assert_eq!(
            task[..2],
            [0xFF, 0xFE],
            "schtasks reads UTF-16 with a byte-order mark"
        );
        let text = describe(&plan);
        for needle in [
            "<BootTrigger>",
            "<UserId>S-1-5-18</UserId>",
            "<RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            r#"<Command>C:\Program Files\MythOS\current\bin\mythos-server.exe</Command><Arguments>--storage-root "C:\ProgramData\MythOS\projects" --port 8080</Arguments>"#,
        ] {
            assert!(text.contains(needle), "{needle} not in {text}");
        }
        assert_eq!(plan.commands[0][..2], strings(&["schtasks", "/Create"]));
        assert_eq!(
            plan.commands[1],
            strings(&["schtasks", "/Run", "/TN", "mythos"])
        );
        let gone = plan_uninstall(Os::Windows, &windows).unwrap();
        assert_eq!(
            gone.commands[1],
            strings(&["schtasks", "/Delete", "/TN", "mythos", "/F"])
        );
        assert_eq!(gone.remove.len(), 1);
    }

    #[test]
    fn a_name_that_is_not_a_plain_word_is_refused_on_every_platform() {
        for bad in ["", "my service", "../x", "a;b", &"x".repeat(65)] {
            let mut s = spec(false);
            s.name = bad.to_string();
            for os in [Os::Linux, Os::MacOs, Os::Windows] {
                assert!(plan_install(os, &s).is_err(), "{bad:?} on {os:?}");
                assert!(plan_uninstall(os, &s).is_err(), "{bad:?} on {os:?}");
            }
        }
    }

    #[test]
    fn xml_values_are_escaped() {
        let mut s = spec(false);
        s.storage = PathBuf::from("/data/a&b<c>");
        let plist =
            String::from_utf8(plan_install(Os::MacOs, &s).unwrap().files[0].bytes.clone()).unwrap();
        assert!(plist.contains("/data/a&amp;b&lt;c&gt;"), "{plist}");
    }
}
