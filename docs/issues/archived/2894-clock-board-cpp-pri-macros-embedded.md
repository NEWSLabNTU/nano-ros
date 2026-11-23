---
id: 2894
title: "The C++ workspace's `clock_board_pkg` does not compile for QEMU bare-metal, FreeRTOS MPS2 or ThreadX RISC-V: `expected ')' before 'PRId64'`"
status: resolved
type: bug
area: [examples, cpp, fixtures]
severity: high
found: 2026-11-23
resolved: 2026-11-23
related: [phase-582, phase-590]
---

## What fails

Nightly 45115997343 (main `a1cbbb6b04`), the QemuBaremetal, FreertosMps2 and
ThreadxRiscv64 C++ workspace builds:

```text
examples/workspaces/cpp/src/clock_board_pkg/src/ClockBoard.cpp:35: error: expected ')' before 'PRId64'
```

## Cause

4f05d59e6d (phase-582 W1d) printed `wall_ns` and `steps` with `<cinttypes>`'
`PRId64`/`PRIu32`. Only the `linux_talker_clock_board` image launches the
component, but the workspace compiles every package's sources for every
board's image, and the embedded C++ libraries do not define those macros.

## Resolution

The line prints through casts (`%lld` with `long long`, `%lu` with
`unsigned long`) and drops `<cinttypes>`. The printed format is unchanged, so
`nros_tests::output`'s parser still reads it.

Sweep: `git grep -n -E '\bPRI[diuxX](8|16|32|64)\b' -- '*.cpp' '*.hpp'`. The
other hits are verbatim upstream rclcpp examples in host-only compile tests
(`packages/api/nros-cpp/tests/compile/rclcpp-examples/`).
