//! Print every baked cell's isolation values — issue 2251.
//!
//! ```console
//! $ cell-isolation        # tab-separated; shown here with spaces
//! zephyr  rust  xrce  Pubsub  -  2400  31
//! ```
//!
//! Columns: fixture platform token, lang, rmw, workload, router port, XRCE
//! Agent port, DDS domain. The formulas live in `nros_tests::alloc`; a fixture
//! build reads their values from here instead of recomputing them, so the
//! numbers an image bakes and the numbers a test starts its router/Agent on
//! (and the injectivity gate proves disjoint) are one computation.
//!
//! Run from a build recipe through `nros_cell_isolation_file`
//! (`scripts/build/fixture-lane.sh`), which reuses a current prebuilt binary
//! and otherwise `cargo run`s it, the same way `lane-coords` is reached.

fn main() {
    if std::env::args().len() > 1 {
        eprintln!("usage: cell-isolation   (takes no arguments)");
        std::process::exit(2);
    }
    print!("{}", nros_tests::alloc::isolation_table());
}
