// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the aleo-devnode tool.
//
// Licensed under the GNU General Public License v3.0.

use anyhow::Result;
use clap::Parser;
use serde_json::json;

// Advance the Devnode ledger by a specified number of blocks.
#[derive(Parser, Debug)]
#[group(id = "advance_args")]
pub struct Advance {
    #[clap(help = "The number of blocks to advance the ledger by", default_value = "1")]
    pub num_blocks: u32,
    #[clap(long, help = "devnode REST API server address", default_value = "127.0.0.1:3030")]
    pub(crate) socket_addr: String,
}

impl Advance {
    pub fn execute(self) -> Result<()> {
        tracing::info!("Advancing the Devnode ledger by {} block(s)", self.num_blocks);

        let client = reqwest::blocking::Client::new();
        let payload = json!({
            "num_blocks": self.num_blocks,
        });

        let response = client
            .post(format!("http://{}/testnet/block/create", self.socket_addr))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| anyhow::anyhow!("Failed to reach devnode at {}: {e}", self.socket_addr))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Advance failed ({status}): {body}"));
        }

        Ok(())
    }
}
