//! Phase 216.B — `nros::main!()` RTIC macro expansion compile smoke.
//!
//! Spec §216 Acceptance #6: assert the macro emit for an Entry pkg
//! carrying `[package.metadata.nros.entry] deploy = "rtic-stm32f4"`
//! actually compiles. The macro expansion produces a
//! `#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [...])]
//! mod app` skeleton that delegates init to
//! `<RticStm32F4 as RticBoardEntry>::init_hardware`, plus the
//! `__nros_spin` / `__nros_dispatch` task sidekicks.
//!
//! The Phase 216.B.5 example at `examples/stm32f4/rust/talker-rtic/`
//! IS the canonical fixture for this expansion — it's the smallest
//! Entry pkg shape that exercises the rtic emit branch end-to-end.
//! Rather than duplicate the fixture under
//! `packages/testing/nros-tests/fixtures/`, the test shells out to
//! `cargo check --target thumbv7em-none-eabihf` from that example
//! dir and asserts exit 0.
//!
//! Skips cleanly when:
//!
//! * the `thumbv7em-none-eabihf` Rust target isn't installed —
//!   nothing on the host can drive the cross compile.
//! * the example dir is absent (defensive — `talker-rtic` could be
//!   retired by a future phase).

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn example_dir() -> PathBuf {
    workspace_root().join("examples/stm32f4/rust/talker-rtic")
}

/// Return true iff the `thumbv7em-none-eabihf` rust target is
/// installed. Mirrors the gating used by `phase212_h3_freertos`
/// for `thumbv7m-none-eabi`.
fn thumbv7em_target_installed() -> bool {
    let Ok(out) = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "thumbv7em-none-eabihf")
}

#[test]
fn rtic_main_macro_expansion_compiles() {
    let example = example_dir();
    if !example.is_dir() {
        nros_tests::skip!(
            "talker-rtic example missing at {} — retired?",
            example.display()
        );
    }
    if !thumbv7em_target_installed() {
        nros_tests::skip!("thumbv7em-none-eabihf target not installed");
    }

    // `cargo check` over the existing example dir is the cheapest
    // proof the `nros::main!()` macro emit type-checks: it walks
    // every dep but skips final code-gen. The `.cargo/config.toml`
    // already pins `target = thumbv7em-none-eabihf`; passing
    // `--target` explicitly makes the gating self-describing.
    let out = Command::new("cargo")
        .args(["check", "--target", "thumbv7em-none-eabihf"])
        .current_dir(&example)
        .output()
        .expect("spawn cargo check on talker-rtic");

    assert!(
        out.status.success(),
        "cargo check --target thumbv7em-none-eabihf failed in {}\n\
         stdout:\n{}\nstderr:\n{}",
        example.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
