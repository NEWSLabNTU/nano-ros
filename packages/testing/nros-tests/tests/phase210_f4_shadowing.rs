//! Phase 210.F.4 — workspace-over-AMENT shadowing regression.
//!
//! Drives the smoke fixture at `examples/templates/workspace-shadowing/`
//! through `cmake -B build` + `cmake --build build`, then inspects the
//! linked consumer binary's symbol table for evidence that the
//! workspace `std_msgs` shadow (carrying `Marker.msg`) was the one
//! linked — NOT the AMENT-installed `std_msgs` under
//! `/opt/ros/<distro>/share/std_msgs/`, which ships no `Marker.msg`.
//!
//! ### Test logic
//!
//! The build itself is the strongest shadowing proof: the consumer
//! `#include`s `std_msgs/msg/marker.hpp` and references
//! `.shadowed_marker`. If the layered resolver
//! (`cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`) fell through to
//! AMENT, the compile would FAIL on the include + the field reference
//! (AMENT's `std_msgs` carries no `Marker.msg`).
//!
//! The `nm` step then corroborates by grepping for the workspace-only
//! `std_msgs::msg::Marker` type symbols + the
//! `nros_cpp_serialize_std_msgs_msg_marker` codegen-emitted FFI
//! symbol. Both presences are direct evidence the workspace copy
//! supplied the type.
//!
//! ### Skip policy
//!
//! Skips cleanly via `nros_tests::skip!` when:
//! * the `nros` build tool isn't installed (cmake configure can't
//!   resolve the codegen step);
//! * `cmake` isn't on PATH;
//! * `nm` isn't on PATH;
//! * `AMENT_PREFIX_PATH` isn't set OR no entry under it carries
//!   `share/std_msgs/` — without an AMENT layer the shadowing
//!   contract degenerates (there's nothing to shadow) and the test
//!   would only re-prove the workspace-only build path already
//!   covered by `examples/templates/local-msg-package/`'s sibling
//!   regression.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_dir() -> PathBuf {
    workspace_root().join("examples/templates/workspace-shadowing")
}

/// Phase 195.D — the host `nros` build tool ships as a prebuilt
/// release; cmake resolves it from `$NROS_CLI` / PATH / `~/.nros/bin`.
fn nros_tool_available() -> bool {
    if let Some(p) = std::env::var_os("NROS_CLI") {
        if Path::new(&p).is_file() {
            return true;
        }
    }
    if which_in_path("nros").is_some() {
        return true;
    }
    if let Ok(home) = std::env::var("HOME") {
        if Path::new(&format!("{home}/.nros/bin/nros")).is_file() {
            return true;
        }
    }
    false
}

fn which_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Locate an AMENT layer that ships `std_msgs` (i.e., the upstream
/// pkg the workspace shadow displaces). Returns the prefix dir of
/// the entry that has `share/std_msgs/package.xml`. None → no
/// AMENT layer carries std_msgs; the shadowing scenario degenerates.
fn ament_std_msgs_prefix() -> Option<PathBuf> {
    let ament = std::env::var("AMENT_PREFIX_PATH").ok()?;
    for entry in ament.split(':') {
        if entry.is_empty() {
            continue;
        }
        let p = Path::new(entry);
        if p.join("share/std_msgs/package.xml").is_file() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Assert that the AMENT std_msgs does NOT ship `Marker.msg` — i.e.
/// the workspace shadow's unique signal is genuinely unique. If
/// upstream ever adds a `Marker.msg` to std_msgs the fixture would
/// silently degrade into a non-distinguishing build; fail loudly.
fn assert_ament_lacks_marker(ament_prefix: &Path) {
    let candidate = ament_prefix.join("share/std_msgs/msg/Marker.msg");
    assert!(
        !candidate.is_file(),
        "Phase 210.F.4 shadowing fixture broke: AMENT std_msgs at \
         {} now ships Marker.msg — the workspace shadow's `Marker.msg` \
         is no longer a unique signal. Pick a different unique-field \
         msg name in `examples/templates/workspace-shadowing/src/\
         std_msgs/msg/`.",
        candidate.display()
    );
}

#[test]
fn workspace_std_msgs_shadows_ament_in_consumer_binary() {
    if !nros_tool_available() {
        nros_tests::skip!(
            "nros build tool not installed — run `just setup-cli` + \
             `source ./activate.sh` (Phase 218); cmake codegen step \
             can't run without it"
        );
    }
    if !nros_tests::process::require_cmake() {
        return;
    }
    if which_in_path("nm").is_none() {
        nros_tests::skip!("`nm` not on PATH — symbol-table verification skipped");
    }
    let Some(ament_prefix) = ament_std_msgs_prefix() else {
        nros_tests::skip!(
            "no AMENT layer ships std_msgs (AMENT_PREFIX_PATH unset \
             or no entry has share/std_msgs/) — the shadowing \
             contract is workspace-OVER-AMENT precedence; without \
             AMENT there's nothing to shadow"
        );
    };
    assert_ament_lacks_marker(&ament_prefix);

    let fixture = fixture_dir();
    assert!(
        fixture.is_dir(),
        "fixture missing at {} — expected the Phase 210.F.4 smoke \
         dir",
        fixture.display()
    );

    let build_dir = tempfile::tempdir().expect("tempdir for build root");
    let build_path = build_dir.path().join("build");

    // ---- Configure ----------------------------------------------
    let configure = Command::new("cmake")
        .args(["-S"])
        .arg(&fixture)
        .arg("-B")
        .arg(&build_path)
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed (Phase 210.F.4):\n\
         stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    // ---- Build --------------------------------------------------
    // `cmake --build` is the strongest shadowing assertion: AMENT's
    // std_msgs ships no Marker.msg, so a fall-through to AMENT would
    // fail on `#include "std_msgs/msg/marker.hpp"` AND on
    // `.shadowed_marker`.
    let build = Command::new("cmake")
        .arg("--build")
        .arg(&build_path)
        .output()
        .expect("spawn cmake build");
    assert!(
        build.status.success(),
        "cmake build failed (Phase 210.F.4 — shadowing did NOT win; \
         find_package(std_msgs) likely fell through to AMENT, which \
         carries no Marker.msg):\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // ---- Locate consumer binary --------------------------------
    let consumer = build_path.join("src/consumer/consumer");
    assert!(
        consumer.is_file(),
        "consumer binary missing at {} — build succeeded but emitted \
         no executable",
        consumer.display()
    );

    // ---- nm symbol verification --------------------------------
    let nm = Command::new("nm")
        .arg("-C") // demangle
        .arg(&consumer)
        .output()
        .expect("spawn nm");
    assert!(
        nm.status.success(),
        "nm failed on {}:\nstderr:\n{}",
        consumer.display(),
        String::from_utf8_lossy(&nm.stderr)
    );
    let nm_out = String::from_utf8_lossy(&nm.stdout);

    // The cbindgen-emitted FFI symbol carrying the msg's typed
    // serialize entrypoint — proves the workspace `std_msgs::Marker`
    // codegen output got compiled into the consumer.
    let ffi_marker_symbol = "nros_cpp_serialize_std_msgs_msg_marker";
    assert!(
        nm_out.contains(ffi_marker_symbol),
        "nm did NOT find `{ffi_marker_symbol}` in the consumer \
         binary's symbol table. The workspace `std_msgs::Marker` \
         shadow was not the one linked. Full nm dump:\n{nm_out}"
    );

    // The C++ type-level symbol — proves the consumer's template
    // closure (`Publisher<std_msgs::msg::Marker>`, etc.) instantiated
    // against the shadowed type.
    let type_symbol = "std_msgs::msg::Marker";
    assert!(
        nm_out.contains(type_symbol),
        "nm did NOT find any `{type_symbol}` C++ symbols in the \
         consumer binary. Full nm dump:\n{nm_out}"
    );

    eprintln!(
        "[OK] phase-210.F.4 — workspace std_msgs SHADOWS AMENT \
         at {}; nm confirms `{}` + `{}` linked from workspace \
         codegen",
        ament_prefix.display(),
        ffi_marker_symbol,
        type_symbol,
    );
}
