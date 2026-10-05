//! Take a CENSUS of a prebuilt entry: run it with `$NROS_CENSUS_OUT` and read
//! back what it wrote (phase-463 W2, issue 1419).
//!
//! A census run constructs the entry's components, writes what the recorder
//! saw and exits where a boot would spin — no router, no spin. That makes it
//! the cheapest way to ask a BUILT image which nodes and entities it actually
//! registers, which is what a baked entry is for. ONE spelling of the run, so
//! every test that asks that question reads the same document the same way.

use std::{
    path::Path,
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};

/// What one census run produced.
pub struct Census {
    /// The run's exit status.
    pub status: ExitStatus,
    /// The census file as written, for failure messages.
    pub raw: String,
    /// The parsed document.
    pub json: serde_json::Value,
}

impl Census {
    /// The ids of the nodes the census recorded, in its order.
    pub fn node_ids(&self) -> Vec<String> {
        self.json["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| n["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Run `entry` as a census producer and return what it wrote.
///
/// Panics, naming the entry, when the run outlives `budget` (it booted
/// normally instead of answering `$NROS_CENSUS_OUT`), or exits without writing
/// a file, or writes something that is not JSON — each is a finding about the
/// image, never an environment skip.
pub fn take(entry: &Path, budget: Duration) -> Census {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("census.json");

    let mut child = Command::new(entry)
        .env("NROS_CENSUS_OUT", &out)
        .env_remove("NROS_ENTRY_SPIN_MS")
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", entry.display()));
    let deadline = Instant::now() + budget;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait on the entry") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "census run of {} did not exit within {budget:?} -- it booted normally \
                 instead of writing a census (the runner ignored $NROS_CENSUS_OUT)",
                entry.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let raw = std::fs::read_to_string(&out).unwrap_or_else(|e| {
        panic!(
            "census run of {} exited {status} and wrote no census at {}: {e}",
            entry.display(),
            out.display()
        )
    });
    let json = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("census of {} is not JSON ({e}): {raw}", entry.display()));
    Census { status, raw, json }
}
