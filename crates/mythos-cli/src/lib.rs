//! MythOS CLI library definitions and commands.

pub mod channel;
pub mod config;
pub mod install;
pub mod plugin_cmd;
pub mod release;
pub mod rollback;
pub mod update;
pub mod versions;

pub use clap::Parser;
use clap::Subcommand;
use config::{CliConfig, ReleaseChannel};
use plugin_cmd::PluginCommands;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mythos",
    author = "Adrian Arya Putra & MythOS Team",
    version = env!("CARGO_PKG_VERSION"),
    about = "MythOS Multi-Channel Lifecycle Manager, Distribution Toolchain & Plugin Packager"
)]
pub struct Cli {
    /// Override default base storage directory (~/.mythos)
    #[arg(long, global = true)]
    pub mythos_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Inspect or switch release channels (release, rc, dev)
    Channel {
        #[command(subcommand)]
        action: Option<ChannelSubcommand>,
    },
    /// List installed versions, or with --remote the versions published on the release feed
    Versions {
        /// Query remote distribution catalog
        #[arg(long)]
        remote: bool,
    },
    /// Update local installation to latest or target version
    Update {
        /// Specific SemVer version to install
        #[arg(long)]
        version: Option<String>,
    },
    /// Make the previously current version current again (it is still installed)
    Rollback,
    /// Plugin package management (pack, install, list, inspect)
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ChannelSubcommand {
    /// Switch active channel to release, rc, or dev
    Switch {
        /// Target channel (release, rc, dev)
        channel: String,
    },
}

pub async fn execute_command(
    command: Commands,
    mythos_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Channel { action } => match action {
            None => {
                let ch = channel::ChannelManager::get_channel(mythos_dir)?;
                println!("Active channel: {}", ch);
            }
            Some(ChannelSubcommand::Switch { channel }) => {
                let target: ReleaseChannel = channel.parse()?;
                let switched = channel::ChannelManager::switch_channel(mythos_dir, target)?;
                println!("Successfully switched to channel: {}", switched);
            }
        },
        Commands::Versions { remote } => {
            let config = CliConfig::load_from(mythos_dir)?;
            if remote {
                let feed = config.feed_url();
                let releases = versions::VersionManager::fetch_remote_versions(&feed)?;
                println!("Published versions ({feed}):");
                let following = release::on_channel(&releases, config.channel);
                if following.is_empty() {
                    println!("  (nothing on the {} channel)", config.channel);
                }
                for (i, r) in following.iter().enumerate() {
                    let latest = if i == 0 { " [latest]" } else { "" };
                    let platform = if r.bundle_for(release::TARGET).is_some() {
                        ""
                    } else {
                        " (no bundle for this platform)"
                    };
                    println!("  - {} ({}){latest}{platform}", r.version, r.channel);
                }
                println!(
                    "Channel: {}. This platform: {}.",
                    config.channel,
                    release::TARGET
                );
            } else {
                let local = versions::VersionManager::list_local_versions(mythos_dir);
                println!("Installed versions:");
                if local.is_empty() {
                    println!(
                        "  (none in {})",
                        install::versions_dir(mythos_dir).display()
                    );
                }
                let current = install::current_version(mythos_dir);
                for v in &local {
                    let marker = if current.as_deref() == Some(v.as_str()) {
                        " [current]"
                    } else {
                        ""
                    };
                    println!("  - {v}{marker}");
                }
            }
        }
        Commands::Update { version } => {
            let res = update::UpdateManager::update(mythos_dir, version)?;
            match &res.reported {
                Some(reported) => {
                    println!(
                        "Installed {} and made it current; it reports: {reported} ({:.0} ms)",
                        res.version, res.duration_ms
                    );
                    if !update::reports_release(reported, &res.version) {
                        println!(
                            "Warning: the installed mythos reports \"{reported}\", not release {}. \
                             The release was built from manifests that disagree with its tag.",
                            res.version
                        );
                    }
                }
                None => println!(
                    "{} was already installed; it is now current ({:.0} ms)",
                    res.version, res.duration_ms
                ),
            }
            println!(
                "Run it from {}",
                install::current_link(mythos_dir).join("bin").display()
            );
        }
        Commands::Rollback => {
            let res = rollback::RollbackManager::rollback(mythos_dir)?;
            println!(
                "Rolled back from {} to {} in {:.0} ms",
                res.from_version, res.to_version, res.duration_ms
            );
        }
        Commands::Plugin { action } => match action {
            PluginCommands::Pack {
                manifest,
                payload,
                output,
            } => {
                plugin_cmd::PluginCliManager::pack(&manifest, &payload, &output)?;
                println!("Plugin package created at {}", output.display());
            }
            PluginCommands::Install { package } => {
                let meta = plugin_cmd::PluginCliManager::install(mythos_dir, &package)?;
                println!(
                    "Installed plugin '{}' v{} ({}) successfully.",
                    meta.name, meta.version, meta.id
                );
            }
            PluginCommands::List => {
                let plugins = plugin_cmd::PluginCliManager::list(mythos_dir)?;
                println!("Installed Plugins ({}):", plugins.len());
                for p in plugins {
                    println!(
                        "  - {} v{} [{}] - {}",
                        p.id, p.version, p.plugin_type, p.name
                    );
                }
            }
            PluginCommands::Inspect { id } => {
                let manifest = plugin_cmd::PluginCliManager::inspect(mythos_dir, &id)?;
                println!("Plugin: {}", manifest.metadata.name);
                println!("  ID:          {}", manifest.metadata.id);
                println!("  Version:     {}", manifest.metadata.version);
                println!("  Author:      {}", manifest.metadata.author);
                println!("  Type:        {}", manifest.metadata.plugin_type);
                println!("  Description: {}", manifest.metadata.description);
                println!("  Entrypoint:  {}", manifest.entrypoint);
                println!("  Checksum:    {}", manifest.checksum);
                println!("  Min Core:    {}", manifest.min_core_version);
                println!("  Capabilities:{:?}", manifest.metadata.capabilities);
            }
        },
    }
    Ok(())
}
