//! The canonical Phase 212.N Entry-pkg shape (`nros::main!()` →
//! `<NativeBoard as BoardEntry>::run`) builds and boots through the
//! BoardEntry lifecycle.
//!
//! `packages/testing/nros-tests/bins/entry-poc/` carries the §11.6 one-line
//! `main.rs`:
//!
//! ```ignore
//! nros::main!();   // expands via [package.metadata.nros.entry]
//!                  // deploy = "native" → `<NativeBoard as BoardEntry>::run(...)`
//! ```
//!
//! The **compile** proof is the build of that crate as a fixture
//! (`examples/fixtures.toml`), built by `just native build-fixtures` /
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

    let output = Command::new(bin).output().expect("spawn entry-poc binary");
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
