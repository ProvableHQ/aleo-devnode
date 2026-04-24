// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

mod advance;
mod logger;
mod rest;
mod start;

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
}

/// A standalone Aleo development node.
#[derive(Parser, Debug)]
#[clap(name = "aleo-devnode", about = "A standalone Aleo development node")]
struct Cli {
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
    }
}
