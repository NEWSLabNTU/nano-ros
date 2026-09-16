---
id: 1355
title: "The `threadx_riscv64` nightly job builds ThreadX's **linux/gnu** port, whose
  `tx_port.h` includes `<semaphore.h>` — so the board crate's build script dies on a
  target that has no POSIX headers, and the lane has never reached a cell"
status: open
type: bug
area: ci, boards, threadx
severity: medium
found: 2026-09-12
related: [1158, 1145, phase-448]
---

## What happens

Nightly run **34680021029** (schedule, 07:10), job **103517111698**
(`threadx_riscv64`), step `Build (threadx_riscv64)`:

```
cargo:warning=third-party/threadx/kernel/ports/linux/gnu/inc/tx_port.h:118:10:
  fatal error: semaphore.h: No such file or directory
error: failed to run custom build command for `nros-board-threadx v0.4.0
  (packages/boards/nros-board-threadx)`
error: recipe `build-fixture-extras` failed with exit code 1
```

The path is the whole finding: `ports/linux/gnu`. A riscv64 bare-metal build is
compiling ThreadX's **Linux** port, and that port's `tx_port.h` includes
`<semaphore.h>` because on Linux ThreadX is a pthread-hosted simulation. The
riscv64 toolchain has no such header, so the failure is not "a header is
missing" — it is "the wrong port was selected".

## Why it matters

`threadx_riscv64` is one of the two ThreadX coordinates in the nightly, and the
board it names (`nros-board-threadx-qemu-riscv64`) exists precisely to cover the
ThreadX × riscv axis. While the build script dies, that axis reports `failure`
every night for a reason that has nothing to do with the code under test — the
uniformly-red-lane class the pitfall index describes, where a real regression on
this coordinate would be indistinguishable from this.

It also quietly weakens issue 1145's ThreadX entry, which records
`threadx-riscv64` as "still unstated, and unstated is the safe state" and says
the mechanism is in place awaiting a measurement on that board's own images.
That measurement cannot be taken while the board crate will not build.

## What this is NOT

- Not the `threadx-linux` executor-backing failure in the same run (issue 1145) —
  that is a `const` assert in `nros-node`, on a different job, with a different
  error.
- Not a missing sysroot package. Adding `semaphore.h` to a bare-metal riscv
  toolchain would be papering over the port selection, and the Linux port also
  wants pthreads at link time.

## What would close it

1. Find where `nros-board-threadx`'s build script chooses the port directory and
   determine what it keys on — target triple, a cargo feature, or
   `NROS_PLATFORM_NAME`. The linux/gnu path being reached from a riscv64 build
   means that key is absent or defaulted in this lane.
2. Select the riscv port for the riscv64 board, and make a wrong/absent
   selection a build-script `panic!` that names the target and the port it tried,
   rather than a compiler error thirteen frames down in a vendored header.
3. Acceptance is the `threadx_riscv64` nightly job reaching a verdict — green or
   red on its own cells, not on its board crate's build script.

## Progress (phase-451 W4, 2026-09-16) — the failure is now legible, and NOT fixed

`nros-board-threadx/build.rs` had one port/target guard and it asked a single
direction: *is the port `risc-v64` while `TARGET` is not?* Of the other case it
said, verbatim, that "`linux/gnu` is host-native by definition" — true of the
PORT, and silent about the TARGET. So any cross build that inherited the default
port compiled the POSIX simulation layer and died in `tx_port.h`, which is this
issue.

The guard is symmetric now: a host-native port requires a host `TARGET` exactly
as a cross port requires its own. On this coordinate the build script therefore
prints

    THREADX_PORT=linux/gnu targets x86_64-unknown-linux-gnu but
    TARGET=riscv64...; skipping the ThreadX C build

instead of `fatal error: semaphore.h: No such file or directory`.

**That is a diagnosis, not a fix, and the distinction is the whole point of this
issue.** The finding here was never "a header is missing" but "the wrong port
was selected", and the wrong port is *still* selected — `nros-board-threadx-qemu-riscv64/nros-board.toml`
sets `THREADX_PORT = { value = "risc-v64/gnu", force = true }` and that value is
not reaching `build-fixture-extras`. What changed is that the lane now says so
in the words of the actual problem rather than in the words of a missing libc
header.

Left open deliberately. Closing it on a clearer error message would be the
uniformly-red-lane trade this issue objects to: the riscv64 axis still does not
build its port, and a real regression on that coordinate is still invisible
behind a skip. The remaining work is to find why the board's forced
`THREADX_PORT` does not reach that lane's environment.
