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

        println!(
            "Restore complete. Restart the devnode with:\n  aleo-devnode start --storage {}",
            self.storage.display()
        );

        Ok(())
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
