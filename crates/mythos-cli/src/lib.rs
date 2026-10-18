//! MythOS CLI library definitions and commands.

pub mod channel;
pub mod config;
pub mod plugin_cmd;
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
    /// Query and list available release versions across official channels
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
    /// Revert instantaneously (< 1.0s) to previous cached binary version
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
                let catalog = versions::VersionManager::fetch_remote_versions(
                    &config.remote_url,
                    Some(config.channel),
                );
                println!("Official Release Versions (Remote: {}):", config.remote_url);
                for (ch, vers) in &catalog.channels {
                    let active_marker = if *ch == config.channel.to_string() {
                        " (active)"
                    } else {
                        ""
                    };
                    println!("  [{}]{}", ch, active_marker);
                    for v in vers {
                        let latest_marker = if catalog.latest.get(ch) == Some(v) {
                            " [latest]"
                        } else {
                            ""
                        };
                        println!("    - {}{}", v, latest_marker);
                    }
                }
            } else {
                let local = versions::VersionManager::list_local_versions(mythos_dir);
                println!("Locally Installed Versions:");
                if local.is_empty() {
                    println!(
                        "  (none cached in {})",
                        mythos_dir.join("versions").display()
                    );
                } else {
                    for v in &local {
                        let current_marker =
                            if config.current_version.as_deref() == Some(v.as_str()) {
                                " [current]"
                            } else {
                                ""
                            };
                        println!("  - {}{}", v, current_marker);
                    }
                }
            }
        }
        Commands::Update { version } => {
            let res = update::UpdateManager::update(mythos_dir, version)?;
            println!(
                "Successfully updated to version {} (took {:.2} ms)",
                res.version, res.duration_ms
            );
        }
        Commands::Rollback => {
            let res = rollback::RollbackManager::rollback(mythos_dir)?;
            println!(
                "Successfully rolled back from {} to {} in {:.2} ms (< 1.0s)",
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
