//! Workspace-over-AMENT shadowing (Phase 210.F.4).
//!
//! The workspace `std_msgs` shadow (carrying `Marker.msg`) must win over the
//! AMENT-installed `std_msgs` (which ships no `Marker.msg`). The proof is the
//! linked consumer binary's symbol table: it carries the workspace-only
//! `std_msgs::msg::Marker` C++ type symbols + the cbindgen FFI symbol
//! `nros_cpp_serialize_std_msgs_msg_marker` — both only exist if the workspace
//! copy supplied the type.
//!
//! The cmake configure + build run in the **build stage** — the `shadowing`
//! cmake fixture (`compile-check-fixtures.sh`, run by `build-test-fixtures`)
//! builds `examples/templates/workspace-shadowing` into
//! `build/cmake-fixtures/shadowing/`, linking the consumer. This test `nm`s the
//! prebuilt consumer rather than running cmake at run time (issue 0034 /
//! AGENTS.md "No compilation inside tests").
//!
//! Skips when no AMENT layer ships `std_msgs` (nothing to shadow → the contract
//! degenerates) or `nm` is absent.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Locate an AMENT layer that ships `std_msgs` (the upstream pkg the workspace
/// shadow displaces). None → the shadowing scenario degenerates.
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

fn which_in_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|e| e.join(bin).is_file()))
        .unwrap_or(false)
}

#[test]
fn workspace_std_msgs_shadows_ament_in_consumer_binary() -> nros_tests::TestResult<()> {
    if !which_in_path("nm") {
        nros_tests::skip!("`nm` not on PATH — symbol-table verification skipped");
    }
    let Some(ament_prefix) = ament_std_msgs_prefix() else {
        nros_tests::skip!(
            "no AMENT layer ships std_msgs (AMENT_PREFIX_PATH unset or no entry has \
             share/std_msgs/) — workspace-OVER-AMENT precedence has nothing to shadow"
        );
    };
    // If upstream ever adds a Marker.msg to std_msgs, the fixture's unique signal
    // degrades — fail loudly.
    let marker = ament_prefix.join("share/std_msgs/msg/Marker.msg");
    assert!(
        !marker.is_file(),
        "shadowing fixture broke: AMENT std_msgs at {} now ships Marker.msg — pick a \
         different unique-field msg in examples/templates/workspace-shadowing/src/std_msgs/msg/",
        ament_prefix.display()
    );

    // The prebuilt consumer (build stage). Absent → tier-aware skip/fail.
    let consumer =
        nros_tests::fixtures::require_cmake_fixture("shadowing", "src/consumer/consumer")?;
    assert!(
        consumer.is_file(),
        "consumer binary missing at {}",
        consumer.display()
    );

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

    let ffi_marker_symbol = "nros_cpp_serialize_std_msgs_msg_marker";
    assert!(
        nm_out.contains(ffi_marker_symbol),
        "nm did NOT find `{ffi_marker_symbol}` — the workspace std_msgs::Marker shadow was not \
         the one linked. nm dump:\n{nm_out}"
    );
    let type_symbol = "std_msgs::msg::Marker";
    assert!(
        nm_out.contains(type_symbol),
        "nm did NOT find any `{type_symbol}` C++ symbols. nm dump:\n{nm_out}"
    );
    Ok(())
}
