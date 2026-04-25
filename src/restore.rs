// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

use anyhow::{Result, anyhow};
use clap::Parser;
use std::path::{Path, PathBuf};

/// Restore the ledger from a previously taken snapshot.
#[derive(Parser, Debug)]
#[group(id = "restore_args")]
pub struct Restore {
    /// Name of the snapshot to restore.
    #[clap(long, help = "Name of the snapshot to restore")]
    pub snapshot: String,
    /// Ledger storage directory to restore into (must match the --storage value used when starting).
    #[clap(long, help = "Ledger storage directory to restore into", default_value = "devnode")]
    pub storage: PathBuf,
    /// Restart the devnode immediately after restoring.
    #[clap(long, help = "Restart the devnode after restoring")]
    pub restart: bool,

    // --- Forwarded to `start` when --restart is set ---
    /// Private key for block creation. Required with --restart if PRIVATE_KEY env var is not set.
    #[clap(long)]
    pub private_key: Option<String>,
    /// REST API bind address.
    #[clap(short = 'a', long, default_value = "127.0.0.1:3030")]
    pub socket_addr: String,
    /// Log verbosity (0-2).
    #[clap(short = 'v', long, default_value = "2")]
    pub verbosity: u8,
    /// Disable automatic block creation after broadcast.
    #[clap(short = 'm', long)]
    pub manual_block_creation: bool,
}

impl Restore {
    pub fn execute(self) -> Result<()> {
        // Snapshots live at {storage}-snapshots/{name}, mirroring the layout used by the server.
        let snapshots_dir = snapshots_sibling_dir(&self.storage);
        let snapshot_path = snapshots_dir.join(&self.snapshot);

        if !snapshot_path.exists() {
            return Err(anyhow!(
                "Snapshot '{}' not found at '{}'",
                self.snapshot,
                snapshot_path.display()
            ));
        }

        // Clear the current storage directory, preserving the directory itself.
        if self.storage.exists() {
            println!("Clearing storage directory: {}", self.storage.display());
            clear_dir(&self.storage)?;
        } else {
            std::fs::create_dir_all(&self.storage)
                .map_err(|e| anyhow!("Failed to create storage directory: {e}"))?;
        }

        // Copy the snapshot into the storage directory.
        println!("Restoring '{}' → '{}'...", snapshot_path.display(), self.storage.display());
        copy_dir_all(&snapshot_path, &self.storage)?;
        println!("Restore complete.");

        if self.restart {
            relaunch_as_start(&self.storage, self.private_key.as_deref(), &self.socket_addr, self.verbosity, self.manual_block_creation)?;
        } else {
            println!(
                "Restart the devnode with:\n  aleo-devnode start --storage {}",
                self.storage.display()
            );
        }

        Ok(())
    }
}

/// Re-executes the current binary as `aleo-devnode start` with the given parameters.
/// On Unix this replaces the current process (same PID). On other platforms a child
/// process is spawned and the current process exits.
fn relaunch_as_start(
    storage: &Path,
    private_key: Option<&str>,
    socket_addr: &str,
    verbosity: u8,
    manual_block_creation: bool,
) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| anyhow!("Failed to locate executable: {e}"))?;

    let mut args = vec![
        "start".to_string(),
        "--storage".to_string(),
        storage.display().to_string(),
        "--socket-addr".to_string(),
        socket_addr.to_string(),
        "--verbosity".to_string(),
        verbosity.to_string(),
    ];

    if let Some(pk) = private_key {
        args.push("--private-key".to_string());
        args.push(pk.to_string());
    }

    if manual_block_creation {
        args.push("--manual-block-creation".to_string());
    }

    println!("Restarting: {} {}", exe.display(), args.join(" "));

    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Replaces the current process image — no child process is created.
        Err(anyhow!("Failed to re-exec: {}", cmd.exec()))
    }

    #[cfg(not(unix))]
    {
        cmd.spawn().map_err(|e| anyhow!("Failed to restart devnode: {e}"))?;
        std::process::exit(0);
    }
}

/// Returns the snapshots directory that sits alongside the given storage directory.
/// e.g. `devnode` → `devnode-snapshots`
pub(crate) fn snapshots_sibling_dir(storage: &Path) -> PathBuf {
    let dir_name = format!(
        "{}-snapshots",
        storage.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut p = storage.to_path_buf();
    p.pop();
    p.join(dir_name)
}

fn clear_dir(dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir).map_err(|e| anyhow!("Failed to read '{}': {e}", dir.display()))? {
        let entry = entry.map_err(|e| anyhow!("Failed to read entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| anyhow!("Failed to remove '{}': {e}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| anyhow!("Failed to remove '{}': {e}", path.display()))?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).map_err(|e| anyhow!("Failed to create '{}': {e}", dst.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| anyhow!("Failed to read '{}': {e}", src.display()))? {
        let entry = entry.map_err(|e| anyhow!("Failed to read entry: {e}"))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| anyhow!("Failed to copy '{}': {e}", src_path.display()))?;
        }
    }
    Ok(())
}
