//! Integration tests for mythos-cli lifecycle management and packaging.

use mythos_cli::channel::ChannelManager;
use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::Parser;
use mythos_cli::{execute_command, Cli, Commands};

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
