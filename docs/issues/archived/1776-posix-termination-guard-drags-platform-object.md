---
id: 1776
title: "The POSIX termination guard lived in `platform.c`, so referencing it linked the whole POSIX platform ABI beside another port's"
status: resolved
type: bug
area: [platform, build]
severity: medium
found: 2026-10-10
related: [issue-1732, issue-1764]
resolved_in: "the POSIX termination guard is its own translation unit (issue 1776)"
---

## What was measured

On `main` at `ced0b39912`, `just ci gate` failed in `check::build` →
`workspace-features`. The failing step was

```
cargo test --no-run --workspace --exclude nros-c --no-default-features
```

which ended with

```
rust-lld: error: duplicate symbol: nros_platform_clock_ns
  defined at nros-platform-threadx/src/platform.c   (libnros_platform_threadx.a, whole-archived)
  defined at nros-platform-posix/src/platform.c     (libnros_platform_cffi rlib, posix-c-port)
error: could not compile `nros-board-threadx` (lib test)
```

The run reported 20 duplicate `nros_platform_*` symbols in all, covering the
allocator, clock, RNG, sleep and task functions.

Cause: issue 1732 (`0e01ffb4d4`) added `nros_platform::termination`, which
references `nros_posix_install_termination_guard` and
`nros_posix_termination_requested` whenever `platform-posix` is on. Both
functions were defined at the end of `nros-platform-posix/src/platform.c`. A
static archive member is linked whole when any of its symbols is needed, so a
reference to the guard pulled in the entire POSIX platform ABI.

In a workspace-wide build, cargo unifies `platform-posix` into every graph.
`nros-log` dev-depends on `nros-board-linux`, which turns on
`nros-platform-cffi/posix-c-port`. `nros-board-threadx` whole-archives the
ThreadX ABI, and the two sets of definitions collided.

`cargo test --no-run -p nros-board-threadx` alone is green. Only the unified
graph fails. That is the graph `test-unit` (merge queue) and `workspace-features`
both build.

## Resolution

The guard moved, byte-for-byte, into `nros-platform-posix/src/termination.c`,
its own archive member. A reference to it now pulls in only the guard. I added
the file to every builder of the POSIX port: `nros-platform-cffi/build.rs`
(file + `rerun-if-changed`), `nros-rmw-xrce-cffi/build.rs` and
`nros-platform-posix/CMakeLists.txt`. The NuttX build reuses `platform.c` and
`net.c` only, and nothing there references the guard.

Sweep: `git grep -n 'nros-platform-posix/src\|NROS_PLATFORM_POSIX_SRC'`. The
POSIX-only symbols in the POSIX archive's `T` set that the ThreadX archive does
not define are `nros_posix_apply_current_priority` and the two guard functions.
`nros_posix_apply_current_priority` is referenced only from `nros-board-linux`,
which never shares a link with another port.

| run | rc |
| --- | --- |
| `cargo test --no-run --workspace --exclude nros-c --no-default-features`, before | 101, 20 duplicate symbols, `nros-board-threadx` lib test |
| the same, after | 0 |
