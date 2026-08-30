//! Issue 0659 — reap process groups a previous, SIGKILLed run left behind.
//!
//! Run at LANE START, never mid-run: a concurrent test's peers are recorded and
//! alive, and their members legitimately post-date their record, so a sweep
//! while they run would kill them. `just test-all` invokes this before nextest.
//!
//! A separate binary rather than shell so there is ONE implementation of the
//! ledger format — the "two spellings that can disagree" defect issue 0363
//! records for the CLI stamp.
fn main() {
    // Linux, not unix: the ledger this drives reads `/proc` (see
    // `nros_tests::process::group_ledger`), so there is nothing for it to sweep
    // on a non-Linux unix.
    #[cfg(target_os = "linux")]
    {
        let n = nros_tests::process::sweep_orphaned_process_groups();
        if n > 0 {
            eprintln!("nros-peer-sweep: reaped {n} orphaned peer process group(s) (issue 0659)");
        }
    }
}
