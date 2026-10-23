//! Integration tests for mythos-cli lifecycle management and packaging.

use mythos_cli::channel::ChannelManager;
use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::plugin_cmd::PluginCliManager;
use mythos_cli::rollback::RollbackManager;
use mythos_cli::update::UpdateManager;
use mythos_cli::versions::VersionManager;
use mythos_cli::Parser;
use mythos_cli::{execute_command, Cli, Commands};
use mythos_core::plugin::PluginPackageManifest;
use mythos_plugin_sdk::{PluginMetadata, PluginType};
use std::fs;
use std::time::Instant;

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
fn test_cli_versions_discovery_and_resolution() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    let catalog =
        VersionManager::fetch_remote_versions("https://releases.mythos.firmansyah.co", None);
    assert!(catalog.channels.contains_key("release"));
    assert!(catalog.channels.contains_key("rc"));
    assert!(catalog.channels.contains_key("dev"));

    let rel_latest = VersionManager::resolve_latest(&catalog, ReleaseChannel::Release).unwrap();
    assert_eq!(rel_latest, "0.1.0");

    let rc_latest = VersionManager::resolve_latest(&catalog, ReleaseChannel::Rc).unwrap();
    assert_eq!(rc_latest, "0.2.0-rc.1");

    let dev_latest = VersionManager::resolve_latest(&catalog, ReleaseChannel::Dev).unwrap();
    assert_eq!(dev_latest, "0.2.0-dev.20261019");

    // Check local versions list
    let local = VersionManager::list_local_versions(&mythos_dir);
    assert!(local.is_empty());
}

#[test]
fn test_cli_update_and_instantaneous_rollback_under_one_second() {
    let tmp = tempfile::tempdir().unwrap();
    let mythos_dir = tmp.path().join(".mythos");

    // 1. Update to initial version 0.1.0
    let res1 = UpdateManager::update(&mythos_dir, Some("0.1.0".to_string())).unwrap();
    assert_eq!(res1.version, "0.1.0");
    assert!(mythos_dir.join("current").exists());
    assert!(mythos_dir.join("bin/mythos").exists());

    let cfg1 = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg1.current_version.as_deref(), Some("0.1.0"));
    assert_eq!(cfg1.previous_version, None);

    // 2. Update to version 0.2.0-rc.1
    let res2 = UpdateManager::update(&mythos_dir, Some("0.2.0-rc.1".to_string())).unwrap();
    assert_eq!(res2.version, "0.2.0-rc.1");

    let cfg2 = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg2.current_version.as_deref(), Some("0.2.0-rc.1"));
    assert_eq!(cfg2.previous_version.as_deref(), Some("0.1.0"));

    // Verify local versions cached
    let cached = VersionManager::list_local_versions(&mythos_dir);
    assert_eq!(cached, vec!["0.1.0", "0.2.0-rc.1"]);

    // 3. Instantaneous Rollback to 0.1.0 (< 1.0s)
    let start_rb = Instant::now();
    let rb_res = RollbackManager::rollback(&mythos_dir).unwrap();
    let elapsed = start_rb.elapsed();

    assert_eq!(rb_res.from_version, "0.2.0-rc.1");
    assert_eq!(rb_res.to_version, "0.1.0");
    assert!(
        elapsed.as_millis() < 1000,
        "Rollback must take < 1.0s, took {} ms",
        elapsed.as_millis()
    );

    // Config state updated
    let cfg_rb = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg_rb.current_version.as_deref(), Some("0.1.0"));
    assert_eq!(cfg_rb.previous_version.as_deref(), Some("0.2.0-rc.1"));

    // Verify current symlink points to 0.1.0
    let link_target = fs::read_link(mythos_dir.join("current")).unwrap();
    assert!(link_target.ends_with("0.1.0"));
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

    // Test versions --remote
    let cli_ver = Cli::try_parse_from(["mythos", &dir_arg, "versions", "--remote"]).unwrap();
    execute_command(cli_ver.command, &mythos_dir).await.unwrap();

    // Test update --version
    let cli_up = Cli::try_parse_from(["mythos", &dir_arg, "update", "--version", "0.1.0"]).unwrap();
    execute_command(cli_up.command, &mythos_dir).await.unwrap();

    // Test rollback
    let cli_up2 =
        Cli::try_parse_from(["mythos", &dir_arg, "update", "--version", "0.2.0-rc.1"]).unwrap();
    execute_command(cli_up2.command, &mythos_dir).await.unwrap();

    let cli_rb = Cli::try_parse_from(["mythos", &dir_arg, "rollback"]).unwrap();
    execute_command(cli_rb.command, &mythos_dir).await.unwrap();
    let cfg = CliConfig::load_from(&mythos_dir).unwrap();
    assert_eq!(cfg.current_version.as_deref(), Some("0.1.0"));
}
