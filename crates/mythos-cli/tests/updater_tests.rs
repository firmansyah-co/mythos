//! `mythos update`, `versions --remote` and `rollback` against a release feed served on a local
//! socket, in the shape GitHub's releases API answers, with real bundles: zip files holding this
//! build's own `mythos` executable, checked by SHA-256.
//!
//! Every version a test expects comes from the feed the test serves; none is written into the
//! CLI.

use mythos_cli::config::{CliConfig, ReleaseChannel};
use mythos_cli::install::{current_version, exe, is_installed, version_dir};
use mythos_cli::release::TARGET;
use mythos_cli::rollback::{RollbackError, RollbackManager};
use mythos_cli::update::{UpdateError, UpdateManager};
use mythos_cli::{execute_command, Cli, Parser};
use serde_json::json;
use std::path::PathBuf;

mod common;
use common::*;

/// A MythOS directory whose configuration points at `feed`, on `channel`.
fn home(feed: &Feed, channel: ReleaseChannel) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".mythos");
    let config = CliConfig {
        channel,
        remote_url: feed.url.clone(),
        ..CliConfig::default()
    };
    config.save_to(&dir).unwrap();
    (tmp, dir)
}

fn our_version_line() -> String {
    let out = std::process::Command::new(MYTHOS)
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn bare_update_installs_the_newest_release_on_the_channel_and_makes_it_current() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0-rc.1", Some(bundle()), None);
    feed.publish("v0.3.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);

    // The operator's path: no version given.
    let result = UpdateManager::update(&dir, None).unwrap();

    assert_eq!(
        result.version, "0.4.0",
        "the release channel skips release candidates"
    );
    // What it reports is what the installed executable printed, not a version the CLI knows.
    assert_eq!(
        result.reported.as_deref(),
        Some(our_version_line().as_str())
    );
    assert!(is_installed(&dir, "0.4.0"));
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    assert!(dir
        .join("current")
        .join("bin")
        .join(exe("mythos"))
        .is_file());
    assert!(dir.join("current").join("web").join("index.html").is_file());
    let config = CliConfig::load_from(&dir).unwrap();
    assert_eq!(config.current_version.as_deref(), Some("0.4.0"));
    assert_eq!(config.previous_version, None);
}

#[test]
fn a_channel_that_follows_candidates_gets_the_newest_candidate() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0-rc.1", Some(bundle()), None);
    feed.publish("v0.5.0-dev.7", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Rc);
    assert_eq!(
        UpdateManager::update(&dir, None).unwrap().version,
        "0.5.0-rc.1"
    );
}

#[test]
fn a_pinned_version_is_installed_and_rollback_returns_to_the_previous_one() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);

    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();
    UpdateManager::update(&dir, None).unwrap();
    assert_eq!(current_version(&dir).as_deref(), Some("0.5.0"));

    let rolled = RollbackManager::rollback(&dir).unwrap();
    assert_eq!(
        (rolled.from_version.as_str(), rolled.to_version.as_str()),
        ("0.5.0", "0.4.0")
    );
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    // Both stay installed: rolling forward again needs no download.
    assert!(is_installed(&dir, "0.5.0"));
    let again = UpdateManager::update(&dir, Some("v0.5.0".into())).unwrap();
    assert_eq!(
        again.reported, None,
        "an installed version is not downloaded again"
    );
    assert_eq!(current_version(&dir).as_deref(), Some("0.5.0"));
}

#[test]
fn a_bundle_that_does_not_match_its_checksum_is_refused_and_nothing_changes() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), Some(&"0".repeat(64)));
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("does not match its published checksum"),
        "{err}"
    );
    assert!(!version_dir(&dir, "0.5.0").exists());
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
    let leftovers: Vec<_> = std::fs::read_dir(dir.join("versions"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "a staging directory was left behind: {leftovers:?}"
    );
}

#[test]
fn a_release_without_a_bundle_for_this_platform_says_so() {
    let feed = Feed::start();
    feed.publish("v0.4.0", None, None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("no bundle for this platform") && err.to_string().contains(TARGET),
        "{err}"
    );
    assert_eq!(current_version(&dir), None);
}

#[test]
fn a_bundle_whose_mythos_does_not_run_is_never_made_current() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle_with(b"not a program", &[])), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    UpdateManager::update(&dir, Some("0.4.0".into())).unwrap();

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("does not run on this machine"),
        "{err}"
    );
    assert!(!is_installed(&dir, "0.5.0"));
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
}

#[test]
fn an_archive_entry_outside_the_install_directory_is_refused_before_anything_is_written() {
    let feed = Feed::start();
    let evil = bundle_with(
        &std::fs::read(MYTHOS).unwrap(),
        &[("../../escaped.txt", b"x")],
    );
    feed.publish("v0.4.0", Some(evil), None);
    let (tmp, dir) = home(&feed, ReleaseChannel::Release);

    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(
        err.to_string().contains("outside the install directory"),
        "{err}"
    );
    assert!(!tmp.path().join("escaped.txt").exists());
    assert!(!dir.join("escaped.txt").exists());
    assert_eq!(current_version(&dir), None);
}

#[test]
fn nothing_published_and_an_unreachable_feed_are_errors_not_a_made_up_list() {
    let feed = Feed::start();
    feed.publish("v0.5.0-dev.1", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    assert!(matches!(
        UpdateManager::update(&dir, None),
        Err(UpdateError::NothingPublished { .. })
    ));
    assert!(matches!(
        UpdateManager::update(&dir, Some("9.9.9".into())),
        Err(UpdateError::VersionNotPublished { .. })
    ));

    // A port nothing listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = closed.local_addr().unwrap().port();
    drop(closed);
    let config = CliConfig {
        remote_url: format!("http://127.0.0.1:{port}/releases"),
        ..CliConfig::default()
    };
    config.save_to(&dir).unwrap();
    let err = UpdateManager::update(&dir, None).unwrap_err();
    assert!(err.to_string().contains("could not be read"), "{err}");
}

#[test]
fn rollback_with_nothing_to_return_to_is_refused() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    assert!(matches!(
        RollbackManager::rollback(&dir),
        Err(RollbackError::NoPreviousVersion)
    ));
    UpdateManager::update(&dir, None).unwrap();
    assert!(matches!(
        RollbackManager::rollback(&dir),
        Err(RollbackError::NoPreviousVersion)
    ));
}

#[test]
fn a_pinned_version_is_what_a_bare_update_installs() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    let mut config = CliConfig::load_from(&dir).unwrap();
    config.pinned_version = Some("0.4.0".into());
    config.save_to(&dir).unwrap();

    assert_eq!(UpdateManager::update(&dir, None).unwrap().version, "0.4.0");
    config = CliConfig::load_from(&dir).unwrap();
    config.pinned_version = None;
    config.save_to(&dir).unwrap();
    assert_eq!(UpdateManager::update(&dir, None).unwrap().version, "0.5.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_commands_run_end_to_end() {
    let feed = Feed::start();
    feed.publish("v0.4.0", Some(bundle()), None);
    feed.publish("v0.5.0", Some(bundle()), None);
    let (_tmp, dir) = home(&feed, ReleaseChannel::Release);
    let dir_arg = format!("--mythos-dir={}", dir.display());
    let run = |args: &[&str]| {
        let mut all = vec!["mythos", dir_arg.as_str()];
        all.extend_from_slice(args);
        Cli::try_parse_from(all).unwrap().command
    };

    execute_command(run(&["versions", "--remote"]), &dir)
        .await
        .unwrap();
    execute_command(run(&["update", "--version", "0.4.0"]), &dir)
        .await
        .unwrap();
    execute_command(run(&["update"]), &dir).await.unwrap();
    execute_command(run(&["versions"]), &dir).await.unwrap();
    execute_command(run(&["rollback"]), &dir).await.unwrap();
    assert_eq!(current_version(&dir).as_deref(), Some("0.4.0"));
}

#[test]
fn the_default_feed_is_the_real_one_even_for_a_configuration_from_the_simulated_cli() {
    let legacy: CliConfig = serde_json::from_value(json!({
        "channel": "release",
        "remote_url": "https://releases.mythos.firmansyah.co",
        "current_version": null,
        "previous_version": null
    }))
    .unwrap();
    if std::env::var(mythos_cli::config::RELEASE_FEED_ENV).is_err() {
        assert_eq!(legacy.feed_url(), mythos_cli::config::DEFAULT_RELEASE_FEED);
        assert_eq!(
            CliConfig::default().feed_url(),
            mythos_cli::config::DEFAULT_RELEASE_FEED
        );
    }
}
