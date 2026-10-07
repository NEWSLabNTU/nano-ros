---
id: 1741
title: "A threadx-linux image catches SIGTERM and keeps running, so `timeout N`
  does not bound it — one lived 34 days"
status: resolved
type: bug
area: [testing, platform]
severity: low
found: 2026-10-07
related: [1723, 0659, 1750, 1752, phase-480]
---

## What was measured

While clearing issue 1723's orphans (2026-10-07), this host still had, from
2026-09-03:

    timeout 6 examples/threadx-linux/c/service-server/build-cyclonedds/c_service_server

`timeout` (pid 1702514) in `sigsuspend`, its child `c_service_server`
(9 threads, state S) alive 34 days later. The child's `/proc/<pid>/status`:
`SigIgn 0`, `SigBlk 0`, `SigCgt 0x100004a02` — SIGTERM (bit 15) is CAUGHT.
So `timeout`'s SIGTERM reached a handler that did not end the process, and
with no `-k` nothing followed. cwd was `/home/aeon/repos/nano-ros`.

## Why it is filed

Same class as issue 1723 one layer over: a deadline that sends one SIGTERM
bounds nothing for a process that handles it. 1723 fixed the ROS 2 side and
gated it (`check-ros2-cli-deadline`, `ros2`/`python3` only). A nano-ros
image is our own code, so the better fix may be in the image — the ThreadX
Linux port drives its scheduler with signals, and which handler swallows
SIGTERM was NOT established.

## Not established

- Which handler catches SIGTERM (ThreadX's Linux port, the cyclone backend,
  or the board crate), and whether every threadx-linux image does.
- Which harness site started this one (`timeout 6` appears in no tracked
  file today: `git grep -n "timeout 6 "`).

## Shape of a fix

Either the image exits on SIGTERM (preferred: it is our code), or every
harness deadline on a nano-ros image escalates like `ros2_deadline` does.
The orphan was left running for whoever reproduces this (pid 1702516).

## Resolution

Fixed 2026-10-08 on `fix/1741-image-ends-on-sigterm`, at both layers.

### Which handler, measured

The EXAMPLE's own: `examples/threadx-linux/{c,cpp}/*/src/main.*` call
`signal(SIGTERM, signal_handler)` (set a flag, `nros_executor_cancel`). Not the
ThreadX port (it installs only SIGUSR1/SIGUSR2 — `SigCgt` bits 10/12) and not
Cyclone. `SigCgt 0x100004a02` = SIGINT + SIGUSR1 + SIGUSR2 + SIGTERM + glibc's
33. The zenoh image with no router fails init before `signal()` and shows
`0x100000a00`, i.e. the default disposition.

Why the caught SIGTERM did not end the process — two defects, each enough on
its own:

1. **The handler's shutdown is run by the RTOS, and the RTOS had wedged.**
   Under gdb, with NO signal sent, the Cyclone C service-server and C talker
   both had every thread parked on the ThreadX port's recursive
   `_tx_linux_mutex` within 2 s, owned 1700 deep by Cyclone's `tev` host
   pthread. Cyclone's host threads allocate through `nros_platform_alloc` →
   `tx_byte_allocate`, and the Linux port's interrupt control leaks one
   recursion level per call from a non-ThreadX thread. Filed as
   `1750-threadx-linux-cyclone-host-threads-wedge-the-scheduler.md` (and very
   probably the cause of 0968's threadx Cyclone cluster). The handler set its
   flag; the app thread was never scheduled to read it.
2. **A shutdown that completes did not end the process either.** The app
   thread returned into `threadx_hooks.c`, which lets it finish (right on an
   MCU) while the scheduler idles forever. Measured: the zenoh image with a
   live router, one SIGTERM, graceful shutdown ran — and the process was still
   alive 3 s later.

### The fix

- `nros-board-threadx-linux/c/board_threadx_linux.c`:
  `nros_threadx_linux_install_termination_guard()` blocks SIGTERM/SIGINT
  before `tx_kernel_enter()` (so the port's timer thread, every ThreadX thread
  and every RMW thread inherit the mask — no ThreadX thread is interrupted by
  one any more) and receives them on one host thread with `sigwaitinfo`:
  SIG_IGN honoured, SIG_DFL = the default action, an installed handler is
  CALLED and the image gets `NROS_THREADX_LINUX_TERM_GRACE_MS` = 2000 ms; a
  second signal or the grace expiring ends it by the default action with a
  stderr line. Installed by `startup.c` (C/C++) and by the Rust board's
  pre-kernel step.
- `threadx_hooks.c`: weak `nros_board_app_returned()` no-op; threadx-linux
  overrides it with flush + `_exit(0)`, the statement freertos-posix's entry
  already made.
- Harness: the 1723 helper is generalised rather than duplicated —
  `scripts/lib/deadline.sh` (`NROS_DEADLINE`) and
  `nros_tests::process::{deadline, deadline_command, KILL_GRACE}` — and 22
  single-signal `timeout` sites on images/emulators plus 4
  `Command::new("timeout")` sites now use it. `check-ros2-cli-deadline`
  became `check-process-deadline`: fail-closed on the bounded command (only
  rustup/ninja/cargo/sleep by name; three `bash` sites by path with reasons),
  refuses `Command::new("timeout")` and the retired spellings, holds the two
  graces equal and the image grace below them. Sweep:
  `python3 scripts/check-process-deadline.py`.

### Before / after (rebuilt fixtures, single `timeout 3`, no `-k`)

| image | before | after |
| --- | --- | --- |
| c service-server, Cyclone | alive at 15 s (killed by hand) | ends 5.02 s after start = SIGTERM + 2.0 s grace, rc 124 |
| cpp service-server, Cyclone | (same build family) | ends 5.02 s, rc 124 |
| c service-server, zenoh, no router | ended at 3.0 s by the default action (handler never installed) | app fails init and the process exits at 0.5 s |

Regression test `native_api::test_threadx_linux_c_image_ends_on_one_sigterm`
(Cyclone and zenoh-with-router): asserts the image CATCHES SIGTERM, signals
once at 3 s, requires it gone within `KILL_GRACE`. After: Cyclone 2.005 s
(wedged path, `signal: 15`), zenoh 20 ms (graceful path, exit 0). With the
board fix reverted and the two leaves rebuilt incrementally, BOTH cases fail
"still alive 3s after ONE SIGTERM". Negative control
`negative_control_one_sigterm_does_not_end_a_process_that_catches_it`.

Other native-hosted simulations: freertos-posix workspace entries (C and
C++, Cyclone) measured `SigCgt 0` and end at 3.0 s under `timeout 3`, and
their entry already exits when the app returns. NuttX and every other RTOS
image runs under QEMU, which ends on SIGTERM.

### Not measured / left

- The image's 2 s grace means "within 3 s" holds for the healthy path
  (20–120 ms) but a WEDGED image ends 2 s after the signal, not at it.
- C++ threadx-linux zenoh images, the Rust threadx-linux images under a
  handler (none installs one; their e2e tests pass), and the converted shell
  sites in the Zephyr / ESP32 / FreeRTOS-QEMU / isotp / bootstrap-probe
  scripts were not RUN — they changed only spelling, which the gate checks.
- `native_orchestration_tiers` (converted to `deadline_command`) was not run:
  its compile-check fixtures were not built in this worktree.
- A handler run from the guard thread executes on a host pthread; one that
  calls a ThreadX service would hit 1750's hazard. The in-tree handlers only
  set a flag and cancel (atomics).
- A failed C app now exits 0 — the VOID entry shim discards the status:
  `1752-rtos-c-entry-shim-discards-app-exit-status.md`.
- The 34-day orphan (pid 1702516, started 2026-09-03 13:45 local by a
  relative-path `timeout 6` from the repo root) matches no tracked file; it
  was most likely a hand repro during 0968's threadx Cyclone diagnosis that
  day. Not a test, so it was left running, as found.
