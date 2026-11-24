---
id: 2945
title: "interface_subset_linkage read a row whose bringup lost its disjoint msg-dep subsets when issue 2754 scoped roots, so the mixed-subset test could not pass"
status: resolved
type: bug
area: [testing, codegen, cmake]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [0277, 2754, 2944, phase-590]
---

## What happened

Release run-matrix 45351389552 (main `0d9fa3e470`), tier-2 nightly:
`interface_subset_linkage::mixed_subset_workspace_builds_per_package_ffi_crates`
failed with "expected FFI archives owned by >= 2 packages (disjoint msg-dep
subsets); got {"listener_pkg"}". It is tracker row B28 in phase-590.

## Cause

The test's subject drifted; there is no product defect. It read
`workspace-cpp-native` (`demo_bringup`). Issue 2754 (`a49a3dcf82`,
2026-11-20) scoped a generated root to its bringup, so that row now configures
only `talker_pkg` and `listener_pkg`. Both depend on `std_msgs` alone, so the
build has one subset and every FFI archive sits under one package. On the
runner's tree the root (2026-11-23) and the archives are current. This is the
second time the subject has lost the property: phase-383 W10.a had already
moved it off a single-subset template.

## Fix

Retarget onto `workspace-cpp-mps3-an536-freertos-slirp-bulk`. Its
`slirp_bringup` launches `talker_pkg` (`std_msgs`) and `bulk_listener_pkg`
(`example_interfaces`), and its archives fall under both packages (measured
on the runner: `std_msgs` under `talker_pkg`; `example_interfaces`,
`builtin_interfaces` and `unique_identifier_msgs` under `bulk_listener_pkg`).
The host `nm` reads the armv8r archives (90 `nros_cpp_*` exports in one).

Lane membership: tier 2's own fixture build (08:00) and the tier-2 nightly's
(09:19) both built the row (`freertos.log`, "workspace unit:
workspace-cpp-mps3-an536-freertos-slirp-bulk"), so the test runs in both. In
tier 2 it previously lane-skipped, because `(linux, cpp, zenoh)` is outside
that lane. A lane without the FreeRTOS cpp/cyclonedds coordinate now
lane-skips it.
