---
id: 1779
title: "`check-workspace-features` fails to link `nros-board-threadx`'s test binary when the ThreadX submodule is initialised: the POSIX and ThreadX platform archives both define `nros_platform_*`"
status: open
type: bug
area: [build, threadx, ci]
severity: low
found: 2026-10-10
related: [1706]
---

## What was measured

`just ci gate` on a linked worktree where `third-party/threadx/kernel` had been
initialised (to boot an rv-virt-threadx image for issue 1706) failed at
`check::build` → `workspace-features`, on its first command:

```
cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet
error: linking with `cc` failed
  -o .../target/debug/deps/nros_board_threadx-<hash>
  rust-lld: error: duplicate symbol: nros_platform_clock_ns
  >>> defined at nros-platform-threadx/src/platform.c:57  (libnros_platform_threadx.a, nros-board-threadx OUT_DIR)
  >>> defined at nros-platform-posix/src/platform.c:43    (libnros_platform_cffi-*.rlib)
```

…and the same for every `nros_platform_*` symbol (clock, epoch, alloc/realloc/
dealloc, heap counters, sleep, yield, random). The test binary is
`nros-board-threadx`'s own, built for the host under `--workspace` feature
unification, which also gives `nros-platform-cffi` its `posix-c-port`.

## Why it is usually invisible

`nros-board-threadx/build.rs` compiles `libnros_platform_threadx.a` only when the
ThreadX port sources exist (`THREADX_DIR` set AND the submodule initialised; the
guard around line 118 returns otherwise with a warning). Agent worktrees and the
CI lanes that run this gate do not initialise the submodule, so the archive is
never built and only the POSIX port links. Any host with the submodule present
(a developer checkout that has built a ThreadX fixture) sees this red.

The change that hit it (issue 1706: two fixture bins, one board-crate log-writer
call, test/matrix rows) touches neither crate's features nor the workspace
membership.

## Direction

A host-built test target of a board crate should link exactly one platform port.
Either gate the ThreadX archive's `rustc-link-lib` to non-host targets (the host
test binary needs none of it), or exclude `nros-board-threadx`'s test target from
the workspace-unified `--no-default-features` combo with a stated reason.
Acceptance: `workspace-features` green with the submodule initialised, and still
green without it.
