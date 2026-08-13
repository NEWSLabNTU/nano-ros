---
id: 556
title: "The ThreadX-RV64 resolver spelt a leaf path by hand, so two rtos_e2e cases read a June artifact and looked like QEMU flakes"
status: resolved
type: bug
severity: high
area: testing, build
related: [issue-0393, issue-0517, issue-0482, phase-340]
resolved_in: "issue-0556 (delegate to the row-based resolver)"
---

## Symptom

```
Failed to build first binary (threadx_riscv64 rust Pubsub): BuildFailed(
  "Test fixture binary not prebuilt: examples/qemu-riscv64-threadx/rust/talker/
   target-zenoh/riscv64gc-unknown-none-elf/nros-relwithdebinfo/qemu-riscv64-threadx-talker")
```

on a fixture build that had just reported `threadx_riscv64 == OK`, in a
`lane=all` run where all nine lanes were green.

## Cause

`fixtures/binaries/threadx_riscv64.rs::build_rust_example` joined the artifact
path by hand:

```rust
example_dir.join(format!("target-zenoh/riscv64gc-unknown-none-elf/{}/{}",
                         cargo_target_profile_dir(), binary_name))
```

The manifest row for that leaf authors no `target_dir`, so `row_artifact_root`
is `<dir>/target`. The hand-written `target-zenoh` therefore matched NO row;
`require_prebuilt_binary`'s attribution step could not map the path back to the
manifest, the shared-group redirect never fired, and the resolver read the leaf
tree — where the newest artifact was dated **06-13**, two months stale, because
the fixture build writes to `build/cargo-fixtures/threadx-riscv64-<slug>/`.

The build was correct throughout. Only the test-side locator was wrong, and it
failed by reading an artifact that EXISTS, which is why nothing reported it.

## Why it was mis-diagnosed for so long

A "not prebuilt" verdict from a stale-path read is indistinguishable from a real
failure unless you check where the binary came from. Both cases were carried as
"QEMU flake under sweep load" — including twice in this session's own triage —
until they were run solo on a green fixture build and the path was read.

Sibling shape: issue 0393 ("move the test-side locator in the SAME commit as the
build-side path") and issue 0482 ("the resolver has no link back to the manifest
row"). `nros_fixture_row_artifact_dir` exists on the shell side for exactly this
reason.

## Fix

Delegate to `build_threadx_rv64_rust_example_rmw`, the sibling in `mod.rs` that
already resolves this platform through `select_row` +
`require_prebuilt_row_binary_fresh`. One derivation instead of two.

## Verified

```
test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust   PASS 35.3s
test_rtos_service_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust  PASS 40.3s
```

Both had been failing; both pass against the real guests.

## Sweep

```
git grep -n 'target-zenoh/\|target-xrce/\|target-cyclonedds/' -- packages/testing/nros-tests/src
git grep -n '\.target_dir()' -- packages/testing/nros-tests/src
```

Two remaining `target-<rmw>` spellings, both synthetic paths inside the fixtures
machinery's OWN unit tests. `build_example_rmw` is row-first since issue 0517
phase B and falls back to the authored spelling only for a leaf with no manifest
row (the px4 companion, built by its own lane) — documented at the call site.
This was the last resolver bypassing the row.

## Gate worth having

A resolver path that attributes to NO manifest row silently loses the group
redirect and reads whatever is at the authored location — including a museum
artifact. `require_prebuilt_binary` already computes that attribution for lane
narrowing; failing (or at least warning) when a cargo-row leaf resolves with no
attribution would make this class impossible rather than merely fixed. Not done
here: it needs a survey of the legitimate no-row leaves first.
