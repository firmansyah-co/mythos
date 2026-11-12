//! Integration tests for mythos-cli lifecycle management and packaging.

use mythos_cli::channel::ChannelManager;
use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::plugin_cmd::PluginCliManager;
use mythos_cli::rollback::{RollbackError, RollbackManager};
use mythos_cli::update::{binary_filename, UpdateError, UpdateManager};
use mythos_cli::versions::{ReleaseCatalog, VersionFetchError, VersionManager};
use mythos_cli::Parser;
use mythos_cli::{execute_command, Cli, Commands};
use mythos_core::plugin::PluginPackageManifest;
use mythos_plugin_sdk::{PluginMetadata, PluginType};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Path to the real `mythos` binary, provided by cargo for integration tests.
const MYTHOS_BIN: &str = env!("CARGO_BIN_EXE_mythos");

/// Stage a version directory the way a *real* installer would leave one.
///
/// This is a test fixture standing in for a binary some installer placed on disk. It is
/// deliberately NOT a version of `mythos`: production code has no path that writes here,
/// and `update_refuses_and_installs_nothing` proves the CLI cannot create one. It exists
/// only so the rollback symlink swap can be exercised over real files.
fn stage_installed_version_for_test(mythos_dir: &Path, version: &str) {
    let bin_dir = mythos_dir.join("versions").join(version).join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join(binary_filename()), b"staged-by-test-fixture").unwrap();
}

#[test]
fn test_cli_channel_default_and_switch() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    // 1. Initial channel defaults to Release
    let ch = ChannelManager::get_channel(&mythos_dir).unwrap();
    assert_eq!(ch, ReleaseChannel::Release);

    // 2. Switch to RC
    ChannelManager::switch_channel(&mythos_dir, ReleaseChannel::Rc).unwrap();
    let ch_rc = ChannelManager::get_channel(&mythos_dir).unwrap();
    assert_eq!(ch_rc, ReleaseChannel::Rc);

    // 3. Switch to Dev
    ChannelManager::switch_channel(&mythos_dir, ReleaseChannel::Dev).unwrap();
    let ch_dev = ChannelManager::get_channel(&mythos_dir).unwrap();
    assert_eq!(ch_dev, ReleaseChannel::Dev);

    // Verify config file content directly
    let cfg = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg.channel, ReleaseChannel::Dev);
}

#[test]
fn test_cli_remote_fetch_refuses_and_ignores_the_url() {
    // Two different URLs — one plausible, one that cannot resolve. If the URL decided
    // anything, these could disagree. They cannot, because nothing is contacted.
    let plausible =
        VersionManager::fetch_remote_versions("https://releases.mythos.firmansyah.co", None);
    let unresolvable =
        VersionManager::fetch_remote_versions("https://this-host-does-not-exist.invalid", None);

    assert_eq!(plausible, Err(VersionFetchError::NotImplemented));
    assert_eq!(unresolvable, Err(VersionFetchError::NotImplemented));

    // The error must not carry a catalogue with it.
    assert!(plausible.is_err());
    assert!(unresolvable.is_err());
}

#[test]
fn test_cli_resolve_latest_over_a_declared_catalog() {
    // `resolve_latest` is a pure lookup. The catalogue here is declared by the test and
    // is not a product version claim — nothing in the crate can produce one.
    let mut channels = BTreeMap::new();
    channels.insert("release".to_string(), vec!["9.9.9".to_string()]);
    let mut latest = BTreeMap::new();
    latest.insert("release".to_string(), "9.9.9".to_string());
    let catalog = ReleaseCatalog { channels, latest };

    assert_eq!(
        VersionManager::resolve_latest(&catalog, ReleaseChannel::Release),
        Some("9.9.9".to_string())
    );
    // A channel absent from the catalogue resolves to nothing rather than a guess.
    assert_eq!(
        VersionManager::resolve_latest(&catalog, ReleaseChannel::Rc),
        None
    );
}

#[test]
fn test_cli_local_versions_listing_reads_the_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    // Nothing installed -> empty, not a default list.
    assert!(VersionManager::list_local_versions(&mythos_dir).is_empty());

    stage_installed_version_for_test(&mythos_dir, "0.1.0");
    stage_installed_version_for_test(&mythos_dir, "0.2.0-rc.1");
    assert_eq!(
        VersionManager::list_local_versions(&mythos_dir),
        vec!["0.1.0", "0.2.0-rc.1"]
    );
}

#[test]
fn test_cli_update_refuses_and_installs_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    // Both shapes: an explicit pin, and the bare channel-latest path (`None`) that the
    // operator reaches without flags. Neither may install or record anything.
    for target in [Some("0.2.0-rc.1".to_string()), None] {
        let result = UpdateManager::update(&mythos_dir, target);
        assert!(
            matches!(result, Err(UpdateError::NotImplemented)),
            "update must refuse for every input"
        );
    }

    // The load-bearing assertion: refusing must leave NO trace of an install.
    assert!(
        !mythos_dir.join("versions").exists(),
        "a refused update created a versions directory"
    );
    assert!(
        !mythos_dir.join("config.json").exists(),
        "a refused update wrote config.json, which is where current_version is recorded"
    );
    assert!(
        !mythos_dir.join("current").exists(),
        "a refused update created the current symlink"
    );
    assert!(
        !mythos_dir.join("bin").exists(),
        "a refused update created the bin directory"
    );
}

#[test]
fn test_cli_rollback_switches_between_installed_versions_under_one_second() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    stage_installed_version_for_test(&mythos_dir, "0.1.0");
    stage_installed_version_for_test(&mythos_dir, "0.2.0-rc.1");

    let cfg = CliConfig {
        current_version: Some("0.2.0-rc.1".to_string()),
        previous_version: Some("0.1.0".to_string()),
        ..Default::default()
    };
    cfg.save_to(&mythos_dir).unwrap();

    let start = Instant::now();
    let res = RollbackManager::rollback(&mythos_dir).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(res.from_version, "0.2.0-rc.1");
    assert_eq!(res.to_version, "0.1.0");
    assert!(
        elapsed.as_millis() < 1000,
        "Rollback must take < 1.0s, took {} ms",
        elapsed.as_millis()
    );

    let cfg_rb = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg_rb.current_version.as_deref(), Some("0.1.0"));
    assert_eq!(cfg_rb.previous_version.as_deref(), Some("0.2.0-rc.1"));

    let link_target = fs::read_link(mythos_dir.join("current")).unwrap();
    assert!(link_target.ends_with("0.1.0"));
}

#[test]
fn test_cli_rollback_reports_no_previous_version_on_a_fresh_install() {
    // This is the honest state of a fresh install today: nothing installs a version, so
    // there is nothing to roll back to. It must say so rather than invent a target.
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let err = RollbackManager::rollback(&mythos_dir).unwrap_err();
    assert!(
        matches!(err, RollbackError::NoPreviousVersion),
        "expected NoPreviousVersion, got {err:?}"
    );
}

#[test]
fn test_cli_plugin_pack_install_list_inspect() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");
    let staging = tmp.path().join("staging");
    fs::create_dir_all(&staging).unwrap();

    let manifest_file = staging.join("manifest.json");
    let payload_file = staging.join("plugin.bin");
    let pkg_file = staging.join("driver-modbus-1.0.0.mythos-plugin");

    let manifest = PluginPackageManifest {
        metadata: PluginMetadata {
            id: "com.mythos.driver.modbus".to_string(),
            name: "Modbus Driver".to_string(),
            version: "1.0.0".to_string(),
            author: "MythOS Team".to_string(),
            description: "Industrial Modbus RTU/TCP Driver".to_string(),
            plugin_type: PluginType::ProtocolDriver,
            capabilities: vec!["tagbus:read".to_string(), "tagbus:write".to_string()],
        },
        entrypoint: "bin/driver.bin".to_string(),
        checksum: String::new(),
        min_core_version: "0.1.0".to_string(),
    };

    fs::write(
        &manifest_file,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(&payload_file, b"MOCK_COMPILED_MODBUS_DRIVER_PAYLOAD").unwrap();

    // 1. Pack
    PluginCliManager::pack(&manifest_file, &payload_file, &pkg_file).unwrap();
    assert!(pkg_file.exists());

    // 2. Install
    let installed_meta = PluginCliManager::install(&mythos_dir, &pkg_file).unwrap();
    assert_eq!(installed_meta.id, "com.mythos.driver.modbus");
    assert_eq!(installed_meta.version, "1.0.0");

    // 3. List
    let list = PluginCliManager::list(&mythos_dir).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "com.mythos.driver.modbus");

    // 4. Inspect
    let inspected = PluginCliManager::inspect(&mythos_dir, "com.mythos.driver.modbus").unwrap();
    assert_eq!(inspected.metadata.name, "Modbus Driver");
    assert_eq!(inspected.entrypoint, "bin/driver.bin");
    assert!(!inspected.checksum.is_empty());
}

#[tokio::test]
async fn test_cli_subcommand_dispatch_and_argument_parsing() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let dir_arg = format!("--mythos-dir={}", mythos_dir.display());

    // Test channel command parsing
    let cli = Cli::try_parse_from(["mythos", &dir_arg, "channel"]).unwrap();
    assert!(matches!(cli.command, Commands::Channel { action: None }));
    execute_command(cli.command, &mythos_dir).await.unwrap();

    // Test channel switch
    let cli_sw = Cli::try_parse_from(["mythos", &dir_arg, "channel", "switch", "rc"]).unwrap();
    execute_command(cli_sw.command, &mythos_dir).await.unwrap();
    assert_eq!(
        ChannelManager::get_channel(&mythos_dir).unwrap(),
        ReleaseChannel::Rc
    );

    // Test versions --remote: parses, and dispatch REFUSES.
    let cli_ver = Cli::try_parse_from(["mythos", &dir_arg, "versions", "--remote"]).unwrap();
    assert!(
        execute_command(cli_ver.command, &mythos_dir).await.is_err(),
        "versions --remote must refuse, not print a catalogue"
    );

    // Test update --version: parses, and dispatch REFUSES.
    let cli_up = Cli::try_parse_from(["mythos", &dir_arg, "update", "--version", "0.1.0"]).unwrap();
    assert!(
        execute_command(cli_up.command, &mythos_dir).await.is_err(),
        "update --version must refuse, not report a successful install"
    );

    // Test versions (local) still succeeds.
    let cli_local = Cli::try_parse_from(["mythos", &dir_arg, "versions"]).unwrap();
    execute_command(cli_local.command, &mythos_dir)
        .await
        .unwrap();

    // Rollback still succeeds over versions that are genuinely installed.
    stage_installed_version_for_test(&mythos_dir, "0.1.0");
    stage_installed_version_for_test(&mythos_dir, "0.2.0-rc.1");
    let mut cfg = CliConfig::load_from(&mythos_dir).unwrap();
    cfg.current_version = Some("0.2.0-rc.1".to_string());
    cfg.previous_version = Some("0.1.0".to_string());
    cfg.save_to(&mythos_dir).unwrap();

    let cli_rb = Cli::try_parse_from(["mythos", &dir_arg, "rollback"]).unwrap();
    execute_command(cli_rb.command, &mythos_dir).await.unwrap();
    let cfg_after = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg_after.current_version.as_deref(), Some("0.1.0"));
}

/// Run the real `mythos` binary with an isolated `--mythos-dir`.
fn run_binary(mythos_dir: &Path, args: &[&str]) -> std::process::Output {
    let dir_arg = format!("--mythos-dir={}", mythos_dir.display());
    Command::new(MYTHOS_BIN)
        .arg(dir_arg)
        .args(args)
        .output()
        .expect("failed to run the mythos binary")
}

/// THE OPERATOR'S PATH: bare `mythos update`, no flags.
///
/// This is the invocation the ledger singled out — it reaches `update` with `None`, it
/// used to copy the running binary and print "Successfully updated to version 0.1.0",
/// and no test had ever called it. It must refuse, exit non-zero, and install nothing.
#[test]
fn test_cli_binary_bare_update_refuses_and_exits_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let out = run_binary(&mythos_dir, &["update"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "bare `mythos update` exited 0; stdout={stdout:?} stderr={stderr:?}"
    );
    assert!(
        stderr.contains("NOT IMPLEMENTED"),
        "refusal must name itself as unimplemented; stderr={stderr:?}"
    );
    assert!(
        !stdout.contains("Successfully updated"),
        "a refused update claimed success; stdout={stdout:?}"
    );
    assert!(
        !mythos_dir.join("versions").exists(),
        "a refused update installed something"
    );
    assert!(
        !mythos_dir.join("config.json").exists(),
        "a refused update recorded a current_version"
    );
}

#[test]
fn test_cli_binary_update_with_pinned_version_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let out = run_binary(&mythos_dir, &["update", "--version", "0.2.0-rc.1"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "pinned update exited 0");
    assert!(stderr.contains("NOT IMPLEMENTED"), "stderr={stderr:?}");
    assert!(
        !stdout.contains("Successfully updated to version 0.2.0-rc.1"),
        "the CLI claimed an install it did not perform; stdout={stdout:?}"
    );
    assert!(
        !mythos_dir.join("versions").join("0.2.0-rc.1").exists(),
        "a refused update created the target version directory"
    );
}

#[test]
fn test_cli_binary_versions_remote_refuses_and_names_no_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let out = run_binary(&mythos_dir, &["versions", "--remote"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "versions --remote exited 0");
    assert!(stderr.contains("NOT IMPLEMENTED"), "stderr={stderr:?}");
    assert!(
        !stdout.contains("Official Release Versions"),
        "the CLI printed a catalogue under a remote URL it never contacted; stdout={stdout:?}"
    );
    assert!(
        !stdout.contains("0.2.0-rc.1") && !stdout.contains("0.2.0-dev.20261019"),
        "a fabricated catalogue version reached stdout; stdout={stdout:?}"
    );
}

#[test]
fn test_cli_binary_local_versions_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let out = run_binary(&mythos_dir, &["versions"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "local `versions` must still succeed");
    assert!(
        stdout.contains("Locally Installed Versions"),
        "stdout={stdout:?}"
    );
    assert!(
        stdout.contains("(none cached in"),
        "an empty install must say so rather than listing defaults; stdout={stdout:?}"
    );
}
