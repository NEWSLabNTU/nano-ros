---
id: 2847
title: "A test's readiness or observed marker rides a droppable deferred log, so a log burst can lose it"
status: resolved
type: bug
severity: low
area: [zephyr, testing, logging]
found: 2026-11-23
resolved: 2026-11-25
resolved_in: a2eee50f57
related: [2825, 2815, phase-590]
---

## What

Zephyr cells that wait for a console line (a readiness marker such as
`net_config`'s `IPv4 address: 10.0.2.15` on mps2/an385, or an observed marker
such as `nros: core pin observed ...` in the qemu_cortex_a53 SMP
`CorePinPlacement` cell) read a deferred log message on every board except
native_sim (Zephyr defaults `LOG_MODE_DEFERRED` off `ARCH_POSIX`; 16 of the 18
non-native_sim Zephyr fixture rows were deferred). With
`CONFIG_LOG_MODE_OVERFLOW=y` a full buffer drops the oldest pending message,
so a boot-time warning burst could drop the marker and fail a healthy image.
Measured: the `CorePinPlacement` cell passed 2 of 3 solo runs at host load
about 75; the failing run showed four `--- N messages dropped ---` events and
no marker.

## Fix (#4712, a2eee50f57)

`zephyr/Kconfig` gives every nros image with deferred logging
`CONFIG_LOG_BLOCK_IN_THREAD=y` and `CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS=10000`
(second definitions, which win because modules are sourced first). A
thread-context message that finds the buffer full waits on the buffer's
semaphore instead of reaching the drop branch. That covers the class: every
marker, whoever prints it and however (`printk`, `LOG_*`, Rust `log::`, C
`printf` through `CONFIG_NROS_CONSOLE_WHOLE_LINES`).

Not covered, by design: an ISR-context message (no marker a test reads is
printed from an ISR) and a logging thread starved for 10 s (the wait is
bounded so a logging thread that logs into a full buffer cannot deadlock).

Measured after the fix on qemu_cortex_a53 SMP at host load about 85: 6,757
consecutive 100 Hz ticks printed with no drop reported, and `CorePinPlacement`
passed 5 of 5 solo runs at load about 77.

Rejected directions: `LOG_MODE_OVERFLOW=n` (moves the loss to late markers),
a separate console channel per marker (a change at every site, including
Zephyr's own `net_config` line, and SMP character interleaving, the issue 2384
class), and treating a pre-marker drop as "unknown" in the harness (honest
verdict, unreliable marker).

## Verified closed (2026-11-25, main 9fe08e3012)

- Both defaults are in `zephyr/Kconfig` under `NROS && LOG_MODE_DEFERRED`, so
  both named sites and any later one are covered without a per-site change.
- `just check zephyr-log-markers-lossless`
  (`scripts/check/check-zephyr-log-markers-lossless.py`, self-testing) refuses
  a tracked `.conf` that turns either default off: OK over 153 tracked `.conf`
  files.
- That gate is on the fast line, reached on `pull_request`, `merge_group`,
  `push`, `schedule` and `workflow_dispatch`
  (`check-default-gates-run-somewhere.py --survey`).
