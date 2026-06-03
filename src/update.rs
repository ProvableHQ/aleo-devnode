use anyhow::Result;
use clap::Parser;
use self_update::backends::github;

#[derive(Debug, Parser)]
pub struct UpdateCommand {
    /// Lists all available versions of Leo
    #[clap(short = 'l', long, help = "List all available releases.")]
    list: bool,
    /// Update to a specific named release
    #[clap(short = 'n', long, help = "An optional release name.")]
    name: Option<String>,
    /// Suppress outputs to terminal
    #[clap(short = 'q', long, help = "Suppress download logs.")]
    quiet: bool,
}

impl UpdateCommand {
    pub fn execute(self) -> Result<()> {
        if self.list {
            let releases = github::ReleaseList::configure()
                .repo_owner("ProvableHQ")
                .repo_name("aleo-devnode")
                .with_target(self_update::get_target())
                .build()?
                .fetch()?;

            println!("\nAvailable releases for {}:", self_update::get_target());
            for release in releases {
                println!("  * {}", release.version);
            }
            return Ok(());
        }

        let mut update = github::Update::configure();
        update
            .repo_owner("ProvableHQ")
            .repo_name("aleo-devnode")
            .bin_name("aleo-devnode")
            .current_version(env!("CARGO_PKG_VERSION"))
            .show_download_progress(!self.quiet)
            .no_confirm(true)
            .show_output(!self.quiet);

        if let Some(tag) = self.name {
            update.target_version_tag(&tag);
        }

        let status = update.build()?.update()?;

        if !self.quiet {
            if status.uptodate() {
                println!("aleo-devnode is already up to date (v{})", status.version());
            } else if status.updated() {
                println!("aleo-devnode updated to v{}", status.version());
            }
        }

        Ok(())
    }
}
