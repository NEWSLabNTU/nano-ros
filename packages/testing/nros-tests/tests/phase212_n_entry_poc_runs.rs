//! Phase 212.N entry-poc gate test — §Acceptance "Multi-node Rust
//! = `nros generate-rust && cargo build && cargo run -p <entry-pkg>`".
//!
//! Asserts the canonical Phase 212.N Entry pkg shape compiles + boots
//! through `<NativeBoard as BoardEntry>::run`. The fixture is the
//! in-tree `examples/native/rust/entry-poc/` crate which carries the
//! 2026-06-03 §11.6 design lock's one-line `main.rs`:
//!
//! ```ignore
//! nros::main!();   // expands via [package.metadata.nros.entry]
//!                  // deploy = "native" → `<NativeBoard as
//!                  // BoardEntry>::run(...)`
//! ```
//!
//! Contract gated:
//!
//! - `cargo build` inside the entry-poc dir succeeds (Phase 212.N.9
//!   proc-macro expansion + Phase 212.N.4 `nros-build` path land
//!   cleanly without a separate `nros generate-rust` step, because
//!   entry-poc registers from its companion lib, not from launch
//!   XML).
//! - The produced binary boots through `BoardEntry::run`, attempts
//!   `Executor::open`, fails on the "no zenoh router" path (no
//!   zenohd in this test), and prints the canonical error line
//!   `application error: NodeRegister("entry_poc")`. The error path
//!   IS the lifecycle proof — it means `main()` reached
//!   `BoardEntry::run`'s setup closure, which dispatched into the
//!   pkg's `register()`, which then surfaced the upstream failure
//!   verbatim. No zenoh router needed to gate the proc-macro
//!   plumbing.

use std::path::PathBuf;
use std::process::Command;

fn entry_poc_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("walk up 3")
        .join("examples/native/rust/entry-poc")
}

#[test]
fn entry_poc_compiles_via_nros_main_macro() {
    let dir = entry_poc_dir();
    assert!(
        dir.is_dir(),
        "entry-poc fixture missing at {}",
        dir.display()
    );
    let output = Command::new("cargo")
        .args(["build", "--bin", "entry-poc"])
        .current_dir(&dir)
        .output()
        .expect("spawn cargo build");
    assert!(
        output.status.success(),
        "cargo build failed in entry-poc.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn entry_poc_boots_through_board_entry_run() {
    let dir = entry_poc_dir();
    // Ensure binary is built (cheap if cached by the sibling test).
    let build = Command::new("cargo")
        .args(["build", "--bin", "entry-poc"])
        .current_dir(&dir)
        .output()
        .expect("spawn cargo build");
    assert!(
        build.status.success(),
        "cargo build prerequisite failed.\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr),
    );

    let bin = dir.join("target/debug/entry-poc");
    assert!(bin.is_file(), "binary not produced at {}", bin.display());

    let output = Command::new(&bin)
        .output()
        .expect("spawn entry-poc binary");
    // The Board's println-based lifecycle reporter writes to stdout
    // (not stderr). Inspect both streams to keep the assertion robust
    // against any future stderr-route refactor.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    // The canonical lifecycle proof: `main()` reached
    // `BoardEntry::run`'s setup closure, which dispatched into the
    // pkg's `register()` and surfaced the NodeRegister error. Either
    // of the two known error needles is acceptable — both prove the
    // proc-macro emission + Board::run path executed end-to-end
    // without a separate `nros generate-rust` step.
    let saw_executor_open_fail = combined.contains("Executor::open failed");
    let saw_node_register = combined.contains("application error: NodeRegister");
    assert!(
        saw_executor_open_fail || saw_node_register,
        "entry-poc did not reach the BoardEntry::run lifecycle path.\nstdout:\n{stdout}\nstderr:\n{stderr}",
    );
}
