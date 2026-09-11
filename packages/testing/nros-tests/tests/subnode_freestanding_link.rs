//! phase-427 W12 (issue 1247) — a `SHAPE rclcpp` subnode package, LINKED for a
//! freestanding target.
//!
//! RFC-0089 correction 1 measured that deriving `rclcpp::Node` costs nothing on
//! a freestanding target — `-std=c++14 -fno-exceptions -fno-rtti -ffreestanding
//! -nostdinc++`, with `__is_polymorphic` false for base and derived. That was a
//! COMPILE of a synthetic derivation to an object file, and correction 8
//! recorded what it did not cover: no in-tree fixture consumed the capability,
//! because every `SHAPE rclcpp` package was `platform = "linux"`. Issue 1247
//! names the missing half exactly — "an image that instantiates a derived node
//! with callback groups, on a target with no allocator, LINKED".
//!
//! `workspace-cpp-freertos-realtime-subnode-portable` is that image:
//! `subnode_pkg::SubNode` IS-A `nros::NodeWithTimers<2>` IS-A `rclcpp::Node`,
//! two callback groups on two tiers, cross-linked for `thumbv7m-none-eabi` with
//! `arm-none-eabi-g++`. This file reads the linked ELF.
//!
//! # Why an ELF read and not a QEMU run
//!
//! The acceptance is a BUILD, so there is no `matrix::CELLS` row and nothing
//! boots the image (see the fixture row's comment). But "it built" as a claim
//! decays the moment someone reads a museum binary, so the assertions go
//! through the normal fixture RESOLVER and inherit its freshness check.
//!
//! Which check that is, stated precisely because the two are easy to conflate:
//! a workspace row resolves through `require_prebuilt_workspace_binary`, whose
//! verdict is an INPUT-SIGNATURE comparison against
//! `.nros-workspace-fixture.<id>.inputsig` — not the mtime probe in
//! `fixtures::staleness`, so a stale verdict here carries no `probe:`
//! accounting line and none should be looked for. Measured both ways when this
//! file landed: appending a line to `SubNode.cpp` turns all three tests into
//! `BuildFailed("… is stale: …inputsig")`, and reverting it turns them green
//! again with no rebuild. So the FRESH verdict is a measurement rather than a
//! default — which is what phase-427 W12's "a fixture that only ever resolves
//! STALE is not this acceptance met" is asking for, read from the other side.
//!
//! # What each assertion is for
//!
//! * the ELF header — this really is a 32-bit little-endian ARM image, i.e. a
//!   cross build and not a host one that happened to land in the same path;
//! * `SubNode`'s constructor is DEFINED — the derived type was instantiated,
//!   not merely parsed (a header-only pass leaves no `T` symbol);
//! * both group-bound timer trampolines — the callback-group half of issue
//!   1247. One would be satisfied by a node with a single timer;
//! * ZERO `vtable for` / `typeinfo for` anywhere in the image — correction 1's
//!   `__is_polymorphic` claim, re-measured at LINK scope over the whole binary
//!   rather than as a `static_assert` in one TU. `-fno-rtti` alone does not
//!   give this: a polymorphic class still emits a vtable;
//! * the three sched-context ABI entrypoints — a node whose groups span tiers
//!   takes `ExecutorShape::SchedContexts`, so these are the seam this image
//!   uses instead of `run_tiers`. If they were dropped, the image would link
//!   and bind nothing.

use nros_tests::{TestResult, fixtures::build_freertos_workspace_cpp_subnode_portable_entry};
use std::{path::Path, process::Command};

/// `arm-none-eabi-nm` from the same toolchain that linked the fixture.
///
/// Reading a prebuilt artifact, not building one — `nm` never compiles
/// anything, so this is not the "no compilation inside tests" hazard.
fn nm_symbols(binary: &Path) -> Option<String> {
    let out = Command::new("arm-none-eabi-nm")
        .arg("-C")
        .arg(binary)
        .output()
        .ok()?;
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

#[test]
fn subnode_package_links_for_a_freestanding_target() -> TestResult<()> {
    let binary = build_freertos_workspace_cpp_subnode_portable_entry()?;

    let bytes = std::fs::read(binary).map_err(|e| {
        nros_tests::TestError::BuildFailed(format!("cannot read {}: {e}", binary.display()))
    })?;
    assert!(
        bytes.len() > 64,
        "{} is too short to be an ELF ({} bytes)",
        binary.display(),
        bytes.len()
    );
    assert_eq!(&bytes[0..4], b"\x7fELF", "not an ELF: {}", binary.display());
    assert_eq!(bytes[4], 1, "expected ELFCLASS32 (a 32-bit cross image)");
    assert_eq!(bytes[5], 1, "expected ELFDATA2LSB (little endian)");
    // e_machine, a 2-byte LE field at offset 18. 0x28 = EM_ARM.
    assert_eq!(
        u16::from_le_bytes([bytes[18], bytes[19]]),
        0x28,
        "expected EM_ARM — a host-built binary at this path would mean the \
         fixture row's board never reached the configure"
    );

    Ok(())
}

#[test]
fn the_derived_node_is_instantiated_and_carries_no_vtable() -> TestResult<()> {
    let binary = build_freertos_workspace_cpp_subnode_portable_entry()?;

    if !arm_nm_available() {
        nros_tests::skip_class!(
            capability,
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

    // The derived type was CONSTRUCTED, not merely parsed.
    assert!(
        syms.contains("subnode_pkg::SubNode::SubNode(nros::NodeHandle)"),
        "no defined SubNode constructor in {} — the `SHAPE rclcpp` package did \
         not reach the image; symbols:\n{syms}",
        binary.display()
    );

    // BOTH callback groups' timer trampolines: one of these would also be
    // present in a node with a single timer and no group split.
    for method in ["on_ctrl", "on_telem"] {
        assert!(
            syms.contains(&format!(
                "create_timer_in_group<subnode_pkg::SubNode, &subnode_pkg::SubNode::{method}>"
            )),
            "no group-bound timer trampoline for SubNode::{method} in {} — the \
             callback-group half of issue 1247 is what this fixture exists to \
             measure; symbols:\n{syms}",
            binary.display()
        );
    }

    // RFC-0089 correction 1, at link scope: deriving introduces no vtable.
    // Asserted over the WHOLE image, not just `SubNode`, because a base that
    // gained a virtual member would show up here under its own name.
    let polymorphic: Vec<&str> = syms
        .lines()
        .filter(|l| l.contains("vtable for ") || l.contains("typeinfo for "))
        .collect();
    assert!(
        polymorphic.is_empty(),
        "{} carries {} vtable/typeinfo symbol(s); RFC-0089 correction 1 measured \
         `__is_polymorphic` false for both `nros::Node` and a derivation of it, \
         and this is that claim at LINK scope:\n{}",
        binary.display(),
        polymorphic.len(),
        polymorphic.join("\n")
    );

    Ok(())
}

#[test]
fn the_group_split_entry_links_the_sched_context_abi() -> TestResult<()> {
    let binary = build_freertos_workspace_cpp_subnode_portable_entry()?;

    if !arm_nm_available() {
        nros_tests::skip_class!(
            capability,
            "arm-none-eabi-nm is not on PATH; cannot read {}",
            binary.display()
        );
    }
    let Some(syms) = nm_symbols(binary) else {
        panic!("arm-none-eabi-nm failed on {}", binary.display());
    };

    // A node whose groups span tiers cannot take `run_tiers` (per-tier setup
    // functions construct whole NODES), so `Plan::executor_shape` picks
    // `SchedContexts` and the entry binds through these three.
    for sym in [
        "nros_cpp_create_sched_context_from_policy",
        "nros_cpp_bind_node_name_sched",
        "nros_cpp_bind_group_sched",
    ] {
        assert!(
            syms.lines().any(|l| l.ends_with(sym) && l.contains(" T ")),
            "{sym} is not DEFINED in {} — the generated entry calls it, so an \
             image without it would bind no tier at all; symbols:\n{syms}",
            binary.display()
        );
    }

    Ok(())
}
