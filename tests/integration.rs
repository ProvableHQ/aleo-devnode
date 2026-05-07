// Integration tests for aleo-devnode.
//
// Each test spawns the binary directly, mirroring the shell-based devnode tests
// in the leo repo (tests/tests/cli/test_devnode*, leo_devnode_missing_private_key).

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

const PRIVATE_KEY: &str = "APrivateKey1zkp8CZNn3yeCseEtxuVPbDCwSyhGW6yZKUYKfgXmcpoGPWH";
const DEVNODE_BIN: &str = env!("CARGO_BIN_EXE_aleo-devnode");

static NEXT_PORT: AtomicU16 = AtomicU16::new(14200);

fn alloc_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::Relaxed)
}

struct DevnodeGuard {
    child: Child,
    client: reqwest::blocking::Client,
    base_url: String,
}

impl DevnodeGuard {
    /// Spawns the devnode on the given port.
    ///
    /// When `manual_block_creation` is `true`, the `--manual-block-creation` flag is
    /// passed and the ledger stays at genesis height until blocks are created explicitly.
    /// When `false`, the devnode auto-advances to the latest consensus version height on
    /// startup before accepting further requests.
    fn start(port: u16, storage: Option<&Path>, manual_block_creation: bool) -> Self {
        let addr = format!("127.0.0.1:{port}");
        let mut cmd = Command::new(DEVNODE_BIN);
        cmd.args(["start", "--socket-addr", &addr, "--private-key", PRIVATE_KEY])
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if manual_block_creation {
            cmd.arg("--manual-block-creation");
        }
        if let Some(dir) = storage {
            cmd.arg("--storage").arg(dir);
        }

        let child = cmd.spawn().expect("failed to spawn devnode");
        let client = reqwest::blocking::Client::new();
        let base_url = format!("http://127.0.0.1:{port}/testnet");
        let guard = Self { child, client, base_url };
        guard.wait_for_height(0);
        guard
    }

    /// Polls until the ledger height is at least `min_height`, or panics after 120s.
    ///
    /// Passing `0` simply waits until the REST API is reachable.
    fn wait_for_height(&self, min_height: u32) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Ok(resp) = self.client.get(format!("{}/block/height/latest", self.base_url)).send() {
                if resp.status().is_success() {
                    if let Ok(h) = resp.json::<u32>() {
                        if h >= min_height {
                            return;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        panic!("devnode did not reach height {min_height} within 120s");
    }

    fn height(&self) -> u32 {
        self.client
            .get(format!("{}/block/height/latest", self.base_url))
            .send()
            .expect("height request failed")
            .json::<u32>()
            .expect("invalid height response")
    }

    fn advance(&self, n: u32) {
        self.client
            .post(format!("{}/block/create", self.base_url))
            .json(&serde_json::json!({ "num_blocks": n }))
            .send()
            .expect("advance request failed")
            .error_for_status()
            .expect("advance returned error status");
    }

    fn create_snapshot(&self, name: &str) -> serde_json::Value {
        self.client
            .post(format!("{}/snapshot", self.base_url))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .expect("snapshot request failed")
            .json()
            .expect("invalid snapshot response")
    }

    fn list_snapshots(&self) -> Vec<String> {
        self.client
            .get(format!("{}/snapshots", self.base_url))
            .send()
            .expect("list snapshots request failed")
            .json()
            .expect("invalid snapshots response")
    }

    fn shutdown(mut self) {
        let _ = self.client.post(format!("{}/shutdown", self.base_url)).send();
        let _ = self.child.wait();
        // Drop runs after this and attempts kill+wait; that's harmless.
    }
}

impl Drop for DevnodeGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Mirrors test_devnode: start the node, verify it's reachable, advance 5 blocks,
// verify the height increased by exactly 5.
#[test]
fn test_start_and_advance() {
    let port = alloc_port();
    let devnode = DevnodeGuard::start(port, None, true);

    let initial_height = devnode.height();
    devnode.advance(5);
    let new_height = devnode.height();

    assert_eq!(new_height, initial_height + 5, "height should increase by exactly 5 after advance");
}

// Verifies that without --manual-block-creation the devnode automatically advances
// the ledger to the latest consensus version height before becoming fully ready.
#[test]
fn test_auto_advance_to_consensus_version() {
    let expected_height = snarkvm::prelude::TEST_CONSENSUS_VERSION_HEIGHTS.last().unwrap().1;

    let port = alloc_port();
    let devnode = DevnodeGuard::start(port, None, false);
    devnode.wait_for_height(expected_height);

    assert_eq!(devnode.height(), expected_height, "devnode should auto-advance to the latest consensus version height");
}

// Mirrors leo_devnode_missing_private_key: starting without a private key should
// exit non-zero immediately.
#[test]
fn test_missing_private_key() {
    let port = alloc_port();
    let status = Command::new(DEVNODE_BIN)
        .args(["start", "--socket-addr", &format!("127.0.0.1:{port}")])
        .env_remove("PRIVATE_KEY")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to spawn devnode");

    assert!(!status.success(), "devnode should exit with failure when no private key is provided");
}

// Mirrors test_devnode_persistent_storage: ledger state (block height) must
// survive a stop/restart cycle when --storage is used.
#[test]
fn test_persistent_storage() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let storage = dir.path().join("ledger");

    let port1 = alloc_port();
    let devnode = DevnodeGuard::start(port1, Some(&storage), true);
    devnode.advance(10);
    let height_before = devnode.height();
    devnode.shutdown();

    let port2 = alloc_port();
    let devnode2 = DevnodeGuard::start(port2, Some(&storage), true);
    let height_after = devnode2.height();

    assert_eq!(height_before, height_after, "ledger height should persist across restarts");
}

// Verifies that POST /testnet/snapshot creates a snapshot and GET /testnet/snapshots
// lists it.
#[test]
fn test_snapshot_create_and_list() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let storage = dir.path().join("ledger");

    let port = alloc_port();
    let devnode = DevnodeGuard::start(port, Some(&storage), true);
    devnode.advance(5);

    let resp = devnode.create_snapshot("my-snap");
    assert_eq!(resp["name"], "my-snap", "snapshot response should echo the requested name");
    assert!(resp["height"].as_u64().unwrap() > 0, "snapshot response should include a non-zero height");

    let snapshots = devnode.list_snapshots();
    assert!(snapshots.contains(&"my-snap".to_string()), "snapshot should appear in the listing");
}

// Full snapshot round-trip: create snapshot at height H, advance past it, restore,
// restart, verify the ledger rolled back to H.
#[test]
fn test_snapshot_restore() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let storage = dir.path().join("ledger");

    let port1 = alloc_port();
    let devnode = DevnodeGuard::start(port1, Some(&storage), true);

    devnode.advance(5);
    let resp = devnode.create_snapshot("restore-point");
    let snapshot_height = resp["height"].as_u64().expect("snapshot height missing") as u32;

    // Advance past the snapshot to confirm it actually rolls back.
    devnode.advance(5);
    assert!(devnode.height() > snapshot_height, "height should exceed snapshot before restore");

    devnode.shutdown();

    // Restore (devnode must not be running).
    let status = Command::new(DEVNODE_BIN)
        .args(["restore", "--snapshot", "restore-point", "--storage"])
        .arg(&storage)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("restore command failed to spawn");
    assert!(status.success(), "restore command should exit successfully");

    // Restart and verify the ledger rolled back.
    let port2 = alloc_port();
    let devnode2 = DevnodeGuard::start(port2, Some(&storage), true);
    let height_after = devnode2.height();

    assert_eq!(height_after, snapshot_height, "height should roll back to the snapshot height after restore");
}
