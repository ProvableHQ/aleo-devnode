// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

#![forbid(unsafe_code)]

mod accounts;
mod advance;
mod logger;
mod rest;
mod restore;
mod start;
mod update;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
enum DevnodeCommands {
    #[clap(name = "start", about = "Start the Devnode")]
    Start {
        #[clap(flatten)]
        command: start::Start,
    },
    #[clap(name = "advance", about = "Advance the ledger by a specified number of blocks")]
    Advance {
        #[clap(flatten)]
        command: advance::Advance,
    },
    #[clap(name = "restore", about = "Restore the ledger from a snapshot (server must not be running)")]
    Restore {
        #[clap(flatten)]
        command: restore::Restore,
    },
    #[clap(name = "accounts", about = "List all pre-funded development accounts for the built-in genesis block")]
    Accounts {
        #[clap(flatten)]
        command: accounts::Accounts,
    },
    #[clap(name = "update", about = "Update aleo-devnode to the latest version")]
    Update {
        #[clap(flatten)]
        command: update::UpdateCommand,
    },
}

/// A standalone Aleo development node.
#[derive(Parser, Debug)]
#[clap(name = "aleo-devnode", about = "A standalone Aleo development node", version, disable_version_flag = true)]
struct Cli {
    /// Print version
    #[clap(short = 'v', short_alias = 'V', long, action = clap::ArgAction::Version)]
    version: Option<bool>,
    /// Private key for block creation. Overrides the PRIVATE_KEY environment variable.
    #[clap(long, global = true)]
    private_key: Option<String>,
    #[clap(subcommand)]
    command: DevnodeCommands,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        DevnodeCommands::Start { command } => {
            tracing::info!("Starting the Devnode server...");
            command.execute(cli.private_key)
        }
        DevnodeCommands::Advance { command } => command.execute(),
        DevnodeCommands::Restore { command } => command.execute(),
        DevnodeCommands::Accounts { command } => command.execute(),
        DevnodeCommands::Update { command } => command.execute(),
    }
}
