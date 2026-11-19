---
id: 2752
title: "`nros run` resolves no executable for an image whose `entry` names a hand-written C/C++ package"
status: open
type: bug
severity: medium
area: [cli, run, cmake]
related: [2433, 2750, 2751, 2754]
found: 2026-11-19
---

Part of the owned-entry run road theme (see 2750): an image whose
`[image.<id>] entry` names a hand-written C/C++ package. The other parts are
2750, 2751 and 2754.

## Measured

nano-ros 93368ca929, the downstream island workspace (branch
`nano-ros-next`). `nros build unit_test_bringup:freertos-posix` builds the
owned entry's executable at
`build/<coordinate>/cmake/pkg/asi_unit_test/asi_unit_test`, then:

```
nros build: next for `unit_test_bringup:freertos-posix`:
  note: the build names no single executable (a library, or several binaries) -- see book/src/guide/platforms/freertos.md
```

`nros run unit_test_bringup:freertos-posix --expect '=== All Tests Passed ===' --timeout 60`:

```
nros-runner: the plan carries no `elf` artifact to run
nros run: the target exited with code 2 after 0.0 s without `=== All Tests Passed ===`
```

Same on `mps3-an536-freertos` (QEMU runner). `nros run --dry-run --json`
shows `"artifacts": {}`. Adding
`"elf": "<build_dir>/pkg/asi_unit_test/asi_unit_test"` to that plan and
running `nros run --plan <file> --expect ...` passes on both boards, which
is what the island's CI does now.

## Cause

`packages/cli/nros-cli-core/src/cmd/build.rs:2496`: for a NAMED owned entry
(`image.entry` set) the CMake build output's `executable` is `None` ("its
own CMakeLists.txt calls it"), and `run_plan.rs:952` adds an `elf` only
when the output names one.

## Direction

Read the executable back: the owned package's binary dir is `pkg/<name>/`,
and its image is the one executable target there (or the target named like
the package, as `nano_ros_add_executable` writes it). Failing that, a key
naming it on the image row.
