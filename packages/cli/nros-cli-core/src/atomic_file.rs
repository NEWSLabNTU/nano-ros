//! Re-export of the one write discipline (issues 0498, 0562).
//!
//! The implementation lives in `cargo_nano_ros::atomic_file` because that crate
//! sits BELOW this one and writes sync-owned files of its own
//! (`provider_scan` → `build/nros/providers.json`). Two crates, one spelling.
//!
//! Every sync/codegen-owned file goes through here. It is ATOMIC (a temp
//! sibling plus `rename(2)`, so a concurrent reader never sees a truncated
//! file) and it is WRITE-IF-CHANGED (byte-identical content is not rewritten,
//! so an unchanged file keeps its mtime and costs no downstream reconfigure).
//!
//! Gate: `check-atomic-sync-writes`.

pub use cargo_nano_ros::atomic_file::{atomic_write, atomic_write_bytes, atomic_write_reporting};
