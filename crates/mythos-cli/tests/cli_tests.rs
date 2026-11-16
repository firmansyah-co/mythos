//! Integration tests for mythos-cli lifecycle management and packaging.

use mythos_cli::channel::ChannelManager;
use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::plugin_cmd::PluginCliManager;
use mythos_cli::Parser;
use mythos_cli::{execute_command, Cli, Commands};
use mythos_core::plugin::PluginPackageManifest;
use mythos_plugin_sdk::{PluginMetadata, PluginType};
use std::fs;

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

    // update, versions --remote and rollback are exercised against a real feed in
    // updater_tests.rs.
}
