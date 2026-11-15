//! A C or C++ image that opens its executor over its own storage links no
//! Rust default executor backing.
//!
//! The Rust default open (`Executor::open` and siblings) takes its arena from
//! `nros_node::executor::backing::EXECUTOR_BACKING`, a named `.bss` static
//! sized for a default executor. A C image never takes that road: every C
//! `rclc_executor_t` opens over the image's own `__nros_executor_storage`
//! (`open_in`), and every C++ image over `rclcpp::Node::GlobalStorageHolder`'s
//! `storage`, the same way. So the static must not be in either image at all,
//! and before phase-564 W4 it was not.
//!
//! W4 gave `Executor` an `OwnedBacking` field whose `Drop` named the static
//! (through `give_back`). Every image that can drop an `Executor` links that
//! `Drop`, so every C and C++ image linked the static beside its own storage: the
//! book's FreeRTOS C tutorial overflowed `RAM` by 89,672 bytes, and C++ images
//! on S32Z270 by up to 928,388 bytes. The fix arms
//! the release as a function pointer only on the default path. This reads the
//! linked ELF so the class cannot come back unnoticed: the symbol's presence
//! is a link-time fact, and no unit test can see it.

use nros_tests::{
    TestResult,
    fixtures::freertos::{build_freertos_c_talker, build_freertos_cpp_talker},
};
use std::{path::Path, process::Command};

/// `arm-none-eabi-nm` over a prebuilt artifact. Reading, not building, so
/// this is not the "no compilation inside tests" hazard.
fn nm_symbols(binary: &Path) -> Option<String> {
    let out = Command::new("arm-none-eabi-nm").arg(binary).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn arm_nm_available() -> bool {
    Command::new("arm-none-eabi-nm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Assert `binary` holds its own executor storage (a symbol containing every
/// one of `storage_parts`, the positive control) and no `EXECUTOR_BACKING`.
fn assert_no_default_backing(binary: &Path, storage_parts: &[&str]) {
    if !arm_nm_available() {
        nros_tests::unmet!(
            "arm-none-eabi-nm is not on PATH; the linked image at {} cannot be \
             read symbolically on this host",
            binary.display()
        );
    }
    let Some(syms) = nm_symbols(binary) else {
        panic!(
            "arm-none-eabi-nm failed on {} — the image exists, so this is a \
             toolchain fault, not an absent fixture",
            binary.display()
        );
    };

    // Positive control: the image opens over its OWN storage. A symbol table
    // without it would make the absence below vacuous.
    assert!(
        syms.lines()
            .any(|l| storage_parts.iter().all(|part| l.contains(part))),
        "{} has no executor storage symbol matching {storage_parts:?}, so it \
         is not the image this test is about",
        binary.display()
    );

    // Mangled Rust names carry the path, so a substring finds the static
    // whatever its crate hash.
    let leaked: Vec<&str> = syms
        .lines()
        .filter(|l| l.contains("EXECUTOR_BACKING"))
        .collect();
    assert!(
        leaked.is_empty(),
        "{} links the Rust default executor backing beside its own executor \
         storage, so something an `open_in`-only image reaches names the \
         default backing (an `Executor` `Drop` path, most likely):\n{}",
        binary.display(),
        leaked.join("\n")
    );
}

#[test]
fn a_c_image_links_no_default_executor_backing() -> TestResult<()> {
    assert_no_default_backing(build_freertos_c_talker()?, &["__nros_executor_storage"]);
    Ok(())
}

#[test]
fn a_cpp_image_links_no_default_executor_backing() -> TestResult<()> {
    // `rclcpp::Node::GlobalStorageHolder<0>::storage`, mangled.
    assert_no_default_backing(
        build_freertos_cpp_talker()?,
        &["GlobalStorageHolder", "7storageE"],
    );
    Ok(())
}
