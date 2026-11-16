//! MythOS CLI library definitions and commands.

pub mod channel;

// The updater lives in `mythos-update`, shared with the server; its modules keep their paths here.
pub use mythos_update::{config, install, release, rollback, update, versions};

pub use clap::Parser;
use clap::Subcommand;
use config::{CliConfig, ReleaseChannel};
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
    /// Stay on one version: `update` without --version installs it instead of following the channel
    Pin {
        /// The version to stay on
        version: String,
    },
    /// Follow the channel again
    Unpin,
    /// Module packages (ADR-011): build one from a directory laid out as a package
    Module {
        #[command(subcommand)]
        action: ModuleCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ModuleCommands {
    /// Pack a directory holding module.json, bin/<target>/mythos-module-<name>[.exe] and an
    /// optional ui/ into a module package, with the checksum of every file
    Pack {
        /// The directory to pack
        dir: PathBuf,
        /// Where to write the package (default: <id>-<version>.zip in the current directory)
        #[arg(long)]
        out: Option<PathBuf>,
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
        Commands::Pin { version } => {
            let parsed = semver::Version::parse(version.trim().trim_start_matches('v'))
                .map_err(|_| format!("'{version}' is not a version"))?;
            let mut config = CliConfig::load_from(mythos_dir)?;
            config.pinned_version = Some(parsed.to_string());
            config.save_to(mythos_dir)?;
            println!("Pinned to {parsed}: `mythos update` installs it until `mythos unpin`.");
        }
        Commands::Unpin => {
            let mut config = CliConfig::load_from(mythos_dir)?;
            config.pinned_version = None;
            config.save_to(mythos_dir)?;
            println!("Following the {} channel again.", config.channel);
        }
        Commands::Rollback => {
            let res = rollback::RollbackManager::rollback(mythos_dir)?;
            println!(
                "Rolled back from {} to {} in {:.0} ms",
                res.from_version, res.to_version, res.duration_ms
            );
        }
        Commands::Module { action } => match action {
            ModuleCommands::Pack { dir, out } => {
                let package = mythos_core::modules::pack(&dir)?;
                let manifest = mythos_core::modules::unpack(&package)
                    .map(|u| u.manifest)
                    .ok();
                let out = out.unwrap_or_else(|| {
                    let m = manifest.as_ref();
                    PathBuf::from(format!(
                        "{}-{}.zip",
                        m.map(|m| m.id.as_str()).unwrap_or("module"),
                        m.map(|m| m.version.as_str()).unwrap_or("0")
                    ))
                });
                std::fs::write(&out, &package)?;
                println!("Module package written to {}", out.display());
                if manifest.is_none() {
                    println!(
                        "Note: it has no executable for this platform ({}), so it cannot be installed here.",
                        mythos_core::modules::TARGET
                    );
                }
            }
        },
    }
    Ok(())
}
