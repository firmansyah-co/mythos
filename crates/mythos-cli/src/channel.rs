//! Channel inspect and switch subcommands.

use crate::config::{CliConfig, CliConfigError, ReleaseChannel};
use std::path::Path;

pub struct ChannelManager;

impl ChannelManager {
    /// Get the currently active channel.
    pub fn get_channel(mythos_dir: &Path) -> Result<ReleaseChannel, CliConfigError> {
        let config = CliConfig::load_from(mythos_dir)?;
        Ok(config.channel)
    }

    /// Switch active channel and persist to config.
    pub fn switch_channel(
        mythos_dir: &Path,
        new_channel: ReleaseChannel,
    ) -> Result<ReleaseChannel, CliConfigError> {
        let mut config = CliConfig::load_from(mythos_dir)?;
        let prev = config.channel;
        config.channel = new_channel;
        config.save_to(mythos_dir)?;
        tracing::info!("Switched channel from {} to {}", prev, new_channel);
        Ok(new_channel)
    }
}
