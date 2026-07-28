---
id: 328
title: "Test-harness gaps: ~30 fixture resolvers still existence-only (museum-binary trap #222 not propagated), and 24 #[ignore] tests are permanently unreachable"
status: open
type: bug
severity: medium
area: testing
related: [issue-0222, issue-0196]
---

## Finding (audit 2026-07-28, P2)

Two independent holes in the harness, both of the same family: a check that
exists but does not cover what it should.

### 1. The freshness probe was never propagated out of the RTOS resolvers

Issue #222 swapped the four RTOS fixture resolvers to
`require_prebuilt_binary_fresh*` — verified still in place
(`freertos.rs:85,:162`, `nuttx.rs:125,:205,:244`, `threadx_linux.rs:126,:169`,
`threadx_riscv64.rs:78,:156`).

But **~30 resolvers in `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
still use existence-only `require_prebuilt_binary`** on cargo-built artifacts that
*do* have a sibling `.d` dep-info file, so the staleness check is available and
simply not used. A museum binary passes every sweep — the issue-0148/0164/0196
trap, just relocated.

Highest-value sites:

| site | covers |
| --- | --- |
| `mod.rs:2380` (`require_shared_fixture_binary`) | all qemu-arm-baremetal shared fixtures |
| `mod.rs:3040` | the freertos rust example path |
| `mod.rs:4195` (`build_zenoh_stress_test_large_buf`) | stress fixture |
| `mod.rs:4434` | the ESP32 elf resolver |
| `zephyr.rs:899`, `:961` | zephyr fixtures |

Fix: switch the cargo-path resolvers to `require_prebuilt_binary_fresh`.

### 2. All 24 `#[ignore]` tests are unreachable

24 `#[ignore]` attributes exist across `packages/`, and **nothing anywhere passes
`--ignored` / `run-ignored`** — not `just/`, not `justfile`, not
`.github/workflows/`, not `.config/nextest.toml`. They are dead code that reads
like coverage.

Distribution: `rosidl-codegen/tests` 8, `zpico/nros-rmw-zenoh` 6,
`nros-tests/tests` 4, `nros-rmw-cyclonedds` 2,
`rosidl-codegen/src/generator` 2, `cargo-nano-ros/src` 1, `nros-sizes-build` 1.

The 8 in `rosidl-codegen/tests` matter most: `heap_compile_check.rs` and its
siblings are the **only** gate on heap/borrowed storage-mode codegen, so that
feature currently has zero executing coverage.

Easiest win: the 5 `zenoh_integration.rs` tests are ignored with
`#[ignore = "requires zenohd router on tcp/127.0.0.1:7447"]`, but
`ZenohRouter::start_unique` makes that precondition self-provisioning — they can
simply be un-ignored.

Fix: add a `just` recipe (or a nextest profile with
`default-filter = "all()"` + `run-ignored`) for the heavy-but-valuable set;
delete or convert the rest. An ignored test with no lane that runs it should fail
review the same way a `#[allow(dead_code)]` without a reason does.

## Why grouped

Both are "the harness has the mechanism and doesn't apply it", both live in
`packages/testing`, and both are the kind of gap that makes a green sweep
misleading — the audit checklist's E1/E4 pair.
