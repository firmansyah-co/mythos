//! The release feed: which versions of MythOS have been published, and their files.
//!
//! The feed is GitHub's list of releases for this repository (or anything that answers in the
//! same shape, for tests and mirrors). A release offers one bundle per platform,
//! `mythos-<version>-<target>.zip`, and a `SHA256SUMS` file naming the checksum of each.
//!
//! Nothing here has a fallback. A feed that cannot be reached, or that publishes nothing for a
//! channel or for this platform, is an error that says so; it is never replaced by a list of
//! versions that nobody published.

use crate::config::ReleaseChannel;
use semver::Version;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

/// The platform this CLI was built for, as a Rust target triple.
pub const TARGET: &str = env!("MYTHOS_TARGET");

/// The file that names the SHA-256 of every other file of a release.
pub const CHECKSUMS_ASSET: &str = "SHA256SUMS";

/// How long any one request to the feed or for a file may take to start answering.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Error, Debug)]
pub enum FeedError {
    #[error("the release feed {url} could not be read: {reason}")]
    Unreachable { url: String, reason: String },

    #[error(
        "the release feed {url} answered with something that is not a list of releases: {reason}"
    )]
    Malformed { url: String, reason: String },
}

/// One published file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

/// One published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    pub channel: ReleaseChannel,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// The bundle of this release for `target`, if it publishes one.
    pub fn bundle_for(&self, target: &str) -> Option<&Asset> {
        self.asset(&bundle_name(&self.version, target))
    }
}

/// The file name of a release's bundle for one platform.
pub fn bundle_name(version: &Version, target: &str) -> String {
    format!("mythos-{version}-{target}.zip")
}

/// The channel a version belongs to: no prerelease part is a release, `rc.*` a release
/// candidate, `dev.*` a development build. Anything else belongs to no channel.
pub fn channel_of(version: &Version) -> Option<ReleaseChannel> {
    let pre = version.pre.as_str();
    if pre.is_empty() {
        Some(ReleaseChannel::Release)
    } else if pre == "rc" || pre.starts_with("rc.") {
        Some(ReleaseChannel::Rc)
    } else if pre == "dev" || pre.starts_with("dev.") {
        Some(ReleaseChannel::Dev)
    } else {
        None
    }
}

/// How much less settled a channel is than a release: a channel follows every build at least
/// as settled as its own, so `dev` also gets release candidates and releases.
fn rank(channel: ReleaseChannel) -> u8 {
    match channel {
        ReleaseChannel::Release => 0,
        ReleaseChannel::Rc => 1,
        ReleaseChannel::Dev => 2,
    }
}

/// The releases a channel follows, newest first.
pub fn on_channel(releases: &[Release], channel: ReleaseChannel) -> Vec<&Release> {
    let mut found: Vec<&Release> = releases
        .iter()
        .filter(|r| rank(r.channel) <= rank(channel))
        .collect();
    found.sort_by(|a, b| b.version.cmp(&a.version));
    found
}

#[derive(Deserialize)]
struct WireRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<WireAsset>,
}

#[derive(Deserialize)]
struct WireAsset {
    name: String,
    browser_download_url: String,
}

/// Parse a feed's answer. Drafts, and tags that are not a version of a known channel, are
/// left out: they are not releases anyone can install.
pub fn parse_feed(url: &str, body: &str) -> Result<Vec<Release>, FeedError> {
    let wire: Vec<WireRelease> = serde_json::from_str(body).map_err(|e| FeedError::Malformed {
        url: url.to_string(),
        reason: e.to_string(),
    })?;
    Ok(wire
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = Version::parse(r.tag_name.trim_start_matches('v')).ok()?;
            let channel = channel_of(&version)?;
            Some(Release {
                tag: r.tag_name,
                version,
                channel,
                assets: r
                    .assets
                    .into_iter()
                    .map(|a| Asset {
                        name: a.name,
                        url: a.browser_download_url,
                    })
                    .collect(),
            })
        })
        .collect())
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .user_agent(concat!("mythos-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Ask the feed what has been published.
pub fn fetch(url: &str) -> Result<Vec<Release>, FeedError> {
    let unreachable = |reason: String| FeedError::Unreachable {
        url: url.to_string(),
        reason,
    };
    let body = agent()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| unreachable(e.to_string()))?
        .into_body()
        .read_to_string()
        .map_err(|e| unreachable(e.to_string()))?;
    parse_feed(url, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn a_version_belongs_to_the_channel_its_prerelease_names() {
        assert_eq!(channel_of(&v("1.2.0")), Some(ReleaseChannel::Release));
        assert_eq!(channel_of(&v("1.2.0-rc.3")), Some(ReleaseChannel::Rc));
        assert_eq!(channel_of(&v("1.2.0-dev.17")), Some(ReleaseChannel::Dev));
        assert_eq!(channel_of(&v("1.2.0-beta.1")), None);
    }

    #[test]
    fn a_channel_follows_everything_at_least_as_settled_newest_first() {
        let body = r#"[
            {"tag_name": "v0.2.0-dev.4", "assets": []},
            {"tag_name": "v0.1.0", "assets": []},
            {"tag_name": "v0.2.0-rc.1", "assets": []},
            {"tag_name": "v0.3.0", "draft": true, "assets": []},
            {"tag_name": "nightly", "assets": []}
        ]"#;
        let releases = parse_feed("feed", body).unwrap();
        let tags = |c| {
            on_channel(&releases, c)
                .iter()
                .map(|r| r.tag.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(tags(ReleaseChannel::Release), vec!["v0.1.0"]);
        assert_eq!(tags(ReleaseChannel::Rc), vec!["v0.2.0-rc.1", "v0.1.0"]);
        assert_eq!(
            tags(ReleaseChannel::Dev),
            vec!["v0.2.0-rc.1", "v0.2.0-dev.4", "v0.1.0"]
        );
    }

    #[test]
    fn an_answer_that_is_not_a_list_of_releases_is_an_error() {
        assert!(matches!(
            parse_feed("feed", r#"{"message": "API rate limit exceeded"}"#),
            Err(FeedError::Malformed { .. })
        ));
    }
}
