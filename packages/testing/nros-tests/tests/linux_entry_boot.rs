//! The canonical Phase 212.N Entry-pkg shape (`nros::main!()` →
//! `<LinuxBoard as BoardEntry>::run`) builds and boots through the
//! BoardEntry lifecycle.
//!
//! `packages/testing/nros-tests/bins/entry-poc/` is a one-package workspace
//! (phase-531 W7 step-3 prep): its node package `src/entry-poc` holds the
//! component, and the image's GENERATED entry (`linux_entry`) writes the
//! one-line `main` the leaf used to carry:
//!
//! ```ignore
//! nros::main!(...);   // → `<LinuxBoard as BoardEntry>::run(...)`
//! ```
//!
//! The **compile** proof is the build of that image as a workspace fixture
//! (`examples/fixtures.toml`, row `linux-rust-zenoh-entry-poc`), built by
//! `build-test-fixtures`. If the `nros::main!()` proc-macro expansion didn't
//! compile, the fixture build fails in the build stage. Both tests here consume
//! the prebuilt artifact rather than running `cargo build` at run time — see
//! issue 0034 and AGENTS.md "No compilation inside tests".
//!
//! The boot test runs the prebuilt binary with no zenoh router present: it must
//! reach `BoardEntry::run`'s setup closure, dispatch into the pkg's
//! `register()`, and surface the upstream `NodeRegister`/`Executor::open`
//! failure verbatim. That error path IS the lifecycle proof — it means the
//! proc-macro emission + `Board::run` path executed end-to-end without a
//! separate `nros generate-rust` step.

use std::process::Command;

#[test]
fn entry_poc_compiles_via_nros_main_macro() -> nros_tests::TestResult<()> {
    let bin = nros_tests::fixtures::build_entry_poc()?;
    assert!(
        bin.exists(),
        "entry-poc fixture binary does not exist: {}",
        bin.display()
    );
    Ok(())
}

#[test]
fn entry_poc_boots_through_board_entry_run() -> nros_tests::TestResult<()> {
    let bin = nros_tests::fixtures::build_entry_poc()?;

    // A generated hosted entry spins until stopped; bound it (issue 1310's
    // `NROS_ENTRY_SPIN_MS`) so a reachable router ends the run instead of
    // holding it open forever. With no router the open fails first, as before.
    let output = Command::new(bin)
        .env("NROS_ENTRY_SPIN_MS", "1")
        .output()
        .expect("spawn entry-poc binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Lifecycle proof: `main()` reached `BoardEntry::run`'s setup closure, which
    // dispatched into the pkg's `register()`. Any of three needles proves the
    // proc-macro emission + Board::run path executed end-to-end, independent of
    // whether a zenoh router happened to be reachable on the host:
    //   * `application complete` — connected to a router and ran to completion;
    //   * `Executor::open failed` / `application error: NodeRegister` — no router
    //     reachable, so register() surfaced the upstream failure verbatim.
    // (Router presence is environmental — an orphaned/other-user zenohd must not
    // flip this test, so success is an accepted outcome too.)
    let reached_lifecycle = combined.contains("application complete")
        || combined.contains("Executor::open failed")
        || combined.contains("application error: NodeRegister");
    assert!(
        reached_lifecycle,
        "entry-poc did not reach the BoardEntry::run lifecycle path.\nstdout:\n{stdout}\nstderr:\n{stderr}",
    );
    Ok(())
}
