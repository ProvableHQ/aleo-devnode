// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

use anyhow::Result;
use is_terminal::IsTerminal;
use std::{io, str::FromStr};
use tracing_subscriber::{EnvFilter, prelude::*};

pub fn initialize_terminal_logger(verbosity: u8) -> Result<()> {
    let stdout_filter = parse_log_verbosity(verbosity)?;

    // At high verbosity or when there is a custom log filter we show the target
    // of the log event, i.e., the file/module where the log message was created.
    let show_target = verbosity > 2;

    // Initialize tracing.
    let _ = tracing_subscriber::registry()
        .with(
            // Add layer using LogWriter for stdout / terminal
            tracing_subscriber::fmt::Layer::default()
                .with_ansi(io::stdout().is_terminal())
                .with_target(show_target)
                .with_filter(stdout_filter),
        )
        .try_init();

    Ok(())
}

fn parse_log_verbosity(verbosity: u8) -> Result<EnvFilter> {
    // Note, that this must not be prefixed with `RUST_LOG=`.
    let default_log_str = match verbosity {
        0 => "info",
        1 => "debug",
        2.. => "trace",
    };
    Ok(EnvFilter::from_str(default_log_str).unwrap())
}
