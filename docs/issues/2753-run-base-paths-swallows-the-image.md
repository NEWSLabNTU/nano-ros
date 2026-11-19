---
id: 2753
title: "`nros run --base-paths` forwards the image after the paths, so its build reads the image as one more base path"
status: open
type: bug
severity: low
area: [cli, run]
related: [0646]
found: 2026-11-19
---

## Measured

nano-ros 93368ca929, the downstream island workspace (branch
`nano-ros-next`):

```
nros run test_bringup:unit_test-freertos-posix --base-paths src/test_bringup src/asi_tests src/asi_common src/safety_island_msgs src/visionpilot_msgs --expect '=== All Tests Passed ===' --timeout 60
```

```
nros run: --base-paths `test_bringup:unit_test-freertos-posix` (resolved to .../actuation_module/test_bringup:unit_test-freertos-posix): not a directory. A base path is a source root relative to the workspace; drop it or fix the spelling
```

## Cause

`Selection::build_args` (`packages/cli/nros-cli-core/src/cmd/run_plan.rs:1211`)
emits `--base-paths p1 p2 ...` and then the image as a positional. The
option takes several values, so `nros build`'s parser reads the image as one
more path.

## Direction

Emit the image before the option, or one `--base-paths=<p>` per path. A test
that round-trips a selection with base paths and an image through
`build::Args` would hold it.
