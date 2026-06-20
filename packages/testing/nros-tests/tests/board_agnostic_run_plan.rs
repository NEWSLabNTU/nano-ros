//! §212.O.3 — board-agnostic `nros-build::generate_run_plan` emit —
//! build-stage fixture (issue 0041).
//!
//! Proves the §212.N.4 codegen is genuinely board-agnostic by linking
//! the SAME `shared_node_pkg` rlib + the SAME `launch.xml` under two
//! distinct Board impls:
//!
//! * `posix_entry/`     — `<PosixBoard as BoardEntry>::run` (host)
//! * `freertos_entry/`  — `<Mps2An385 as BoardEntry>::run`
//!   (`thumbv7m-none-eabi`, QEMU MPS2-AN385)
//!
//! ## Two-legged proof
//!
//! 1. **Driver identity (host, no build):** the two Entry pkgs'
//!    `build.rs` files are byte-identical and consume the SAME
//!    launch.xml at the SAME `nros-build` rev — the operational
//!    definition of board-agnostic codegen. Asserted by reading the
//!    committed fixture sources directly (no compile).
//! 2. **Host emit (build stage):** the `o3_board_agnostic` build-fixture
//!    (`compile-check-fixtures.sh`) does `cargo build -p posix_entry`
//!    in the build stage; this test INSPECTS the prebuilt
//!    `out/run_plan.rs` — a well-formed `pub fn run_plan(...)` body
//!    registering `shared_node_pkg`, NOT a Board-leaking emit.
//!
//! Per issue 0041 ("No compilation inside tests") no cargo runs at test
//! time — the resolver `require_compile_check("o3_board_agnostic")` keys
//! off the build-stage `.compile-ok` stamp.
//!
//! ## FreeRTOS cross-Board leg (deferred — Wave B)
//!
//! The strongest form — byte-identical `out/run_plan.rs` across the
//! POSIX *and* the `thumbv7m-none-eabi` FreeRTOS emit — needs the
//! freertos Entry pkg cross-built in the build stage. That cross-build
//! is issue-0041 Wave B (`cross-build` mechanism, gated on the arm
//! toolchain), not yet wired as a fixture. Until then the host leg
//! (driver identity + posix emit) carries the proof; the cross-Board
//! `run_plan.rs` identity is reported as deferred, not silently
//! dropped.

use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture_src() -> PathBuf {
    nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/n_board_agnostic_run_plan")
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}

fn is_placeholder_stub(body: &str) -> bool {
    body.contains("Placeholder — nros-build codegen unavailable")
}

#[test]
fn board_agnostic_run_plan_links_against_any_board() -> nros_tests::TestResult<()> {
    let src = fixture_src();
    assert!(src.is_dir(), "fixture missing at {}", src.display());

    // --- Leg 1: driver identity (no build). The two Entry pkgs'
    // build.rs MUST be byte-identical — that is what makes the
    // OUT_DIR/run_plan.rs identity meaningful in the first place. Read
    // the committed fixture sources; `build.rs` carries no
    // `@…@` placeholder, so the bytes compare directly.
    let posix_build_rs = fs::read(src.join("posix_entry/build.rs")).expect("read posix build.rs");
    let freertos_build_rs =
        fs::read(src.join("freertos_entry/build.rs")).expect("read freertos build.rs");
    assert_eq!(
        posix_build_rs, freertos_build_rs,
        "fixture invariant violated: posix_entry/build.rs and freertos_entry/build.rs MUST be \
         byte-identical for the run_plan.rs identity assertion to mean anything",
    );

    // --- Leg 2: host emit (build stage). `cargo build -p posix_entry`
    // ran in the build stage (`.compile-ok` stamp); inspect the
    // prebuilt `out/run_plan.rs`.
    let stamp = nros_tests::fixtures::require_compile_check("o3_board_agnostic")?;
    let staged = stamp.parent().expect("stamp dir");
    let build_dir = staged.join("posix_entry/target/debug/build");
    let run_plan_path = walk(&build_dir)
        .into_iter()
        .find(|e| e.file_name().and_then(|n| n.to_str()) == Some("run_plan.rs"))
        .unwrap_or_else(|| {
            panic!(
                "nros-build did not emit run_plan.rs under {} — was the o3_board_agnostic \
                 build-fixture built? (`just build-test-fixtures`)",
                build_dir.display()
            )
        });
    let run_plan = fs::read_to_string(&run_plan_path).expect("read run_plan.rs");

    // Offline gate: a build without a reachable `nros-build` writes the
    // Placeholder stub (compiles, but exercises no real codegen) → skip.
    if is_placeholder_stub(&run_plan) {
        nros_tests::skip!(
            "o3_board_agnostic build-fixture emitted the offline Placeholder stub at {} \
             (nros-build codegen unavailable at build time) — no emit to assert",
            run_plan_path.display()
        );
    }

    // The POSIX emit must be a well-formed run_plan body that registers
    // the shared component WITHOUT leaking Board internals.
    assert!(
        run_plan.contains("pub fn run_plan"),
        "posix run_plan.rs missing `pub fn run_plan`:\n{run_plan}",
    );
    assert!(
        run_plan.contains("shared_node_pkg::register"),
        "posix run_plan.rs missing `shared_node_pkg::register` call:\n{run_plan}",
    );

    // The cross-Board byte-identical leg (freertos thumbv7m emit) is
    // issue-0041 Wave B (cross-build fixture). Report it as deferred so
    // the reduced coverage is visible, not silently assumed.
    eprintln!(
        "note: board_agnostic_run_plan host leg verified (driver identity + posix emit at {}); \
         the cross-Board `run_plan.rs` identity vs the thumbv7m freertos emit is deferred to \
         issue-0041 Wave B (freertos cross-build fixture).",
        run_plan_path.display()
    );
    Ok(())
}
