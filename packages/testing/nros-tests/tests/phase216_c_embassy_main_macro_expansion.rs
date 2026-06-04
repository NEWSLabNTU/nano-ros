//! Phase 216.C — `nros::main!()` Embassy macro expansion compile smoke.
//!
//! Sibling of `phase216_b_rtic_main_macro_expansion`. Asserts the
//! macro emit for an Entry pkg carrying
//! `[package.metadata.nros.entry] deploy = "embassy-stm32f4"`
//! compiles. The macro produces a `#[embassy_executor::main] async
//! fn main(spawner: Spawner)` skeleton that delegates init to
//! `<EmbassyStm32F4 as EmbassyBoardEntry>::init_hardware`, plus
//! `__nros_spin_task` / `__nros_dispatch_task` `#[embassy_executor::
//! task]` sidekicks.
//!
//! The Phase 216.C.4 example at `examples/stm32f4/rust/talker-embassy/`
//! IS the canonical fixture — the smallest Entry pkg shape that
//! exercises the embassy emit branch end-to-end.
//!
//! Skips cleanly when:
//!
//! * the `thumbv7em-none-eabihf` Rust target isn't installed.
//! * the example dir is absent (defensive — `talker-embassy` could
//!   be retired by a future phase).

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn example_dir() -> PathBuf {
    workspace_root().join("examples/stm32f4/rust/talker-embassy")
}

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
fn embassy_main_macro_expansion_compiles() {
    let example = example_dir();
    if !example.is_dir() {
        nros_tests::skip!(
            "talker-embassy example missing at {} — retired?",
            example.display()
        );
    }
    if !thumbv7em_target_installed() {
        nros_tests::skip!("thumbv7em-none-eabihf target not installed");
    }

    let out = Command::new("cargo")
        .args(["check", "--target", "thumbv7em-none-eabihf"])
        .current_dir(&example)
        .output()
        .expect("spawn cargo check on talker-embassy");

    assert!(
        out.status.success(),
        "cargo check --target thumbv7em-none-eabihf failed in {}\n\
         stdout:\n{}\nstderr:\n{}",
        example.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
