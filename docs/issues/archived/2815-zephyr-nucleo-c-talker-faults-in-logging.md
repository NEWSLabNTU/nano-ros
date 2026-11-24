---
id: 2815
title: "The Nucleo-F767ZI C talker faults in Zephyr's logging at boot: the main thread branches to `log_buffer` during the parameter-service warning burst"
status: resolved
type: bug
area: [zephyr, logging, boards]
severity: high
found: 2026-11-22
resolved: 2026-11-25
resolved_in: "#4629 (frames), #4631 (warning burst)"
related: [552, 2387, 1726, phase-590]
---

## What failed

`zephyr_renode_twin::zephyr_nucleo_renode_c_zenoh_serial_pubsub_e2e` failed in
tier 2 (run 44916540238): the host listener heard nothing from the
Nucleo-F767ZI twin. The guest console showed the parameter-service QoS
depth-downgrade warnings, `--- 10 messages dropped ---`, then a USAGE FAULT
(Illegal use of the EPSR) 78 ms after boot, with the main thread's PC at
`log_buffer`, the start of Zephyr logging's mpsc packet buffer (a data object).
Configuration: deferred logging with `LOG_MODE_OVERFLOW`, a 1024-byte log
buffer, `MAIN_STACK_SIZE=16384`, MPU stack guard on.

## Cause: the main stack, through frames larger than its guard

Discriminator runs on host 241 (main `9c8ecf5dcb`, three runs a variant):
baseline faults 3/3; `CONFIG_LOG_MODE_IMMEDIATE=y` still faults 3/3, at other
addresses; `MAIN_STACK_SIZE=32768` passes 3/3. So main's stack overflowed while
the parameter services registered, and `log_buffer` was only what the overflow
trampled.

The MPU guard did not trap it because the frames on that path were far larger
than the guard region: `Executor::pass` 9,472 B and
`reconcile_parameter_services` 9,344 B (18.8 KB together, on a 16 KiB stack),
plus `ensure_parameter_store` 8,704 B and `read_parameter_value_into` 8,320 B.
One `sub sp` of 9 KB moved the stack pointer past the guard into `.data`
without touching it. The shape is a by-value `ParameterValue` (about 8.3 KiB
whatever variant it holds, sized by `StringArray`) and a `todo` list of names
built on the stack: issue 2387's class, one function over.

## Fix

- **Frames (#4629).** The registration path fills a parameter slot in place
  (`ParameterServer::declare_with`), and the reconcile list holds node indices
  only. Measured prologues (thumbv7em-none-eabihf, release, LTO off):
  `Executor::pass` 9,472 to 2,732 B; `reconcile_parameter_services` 10,304 B
  and `declare_parameter_resolved` 8,320 B to under 1,024 B;
  `ExecutorSink::create_node` 17,536 to 9,472 B. The Renode cell, at the stock
  16 KiB main stack with deferred logging and overflow mode (3/3 faults on
  main before), passes 3/3 with this alone.
- **Warning burst (#4631).** `report_qos_downgrade` warns once per distinct
  (requested, granted) history depth and logs repeats at debug, so the boot
  prints one depth warning instead of twelve. This is not the cure; it stops
  the burst that also cost issue 2825's mps2 test its readiness marker.

Still over the 4096 B frame cap and listed in the shrink-only baseline:
`create_node`, `declare_parameter_overridden`, `declare_described_overridden`
and the action/lifecycle registration frames (6-9 KiB, the same by-value
`ParameterValue` shape). They no longer overflow this image; the baseline is
the list to work through.

## Guards, and that they run (verified 2026-11-25, main e01b1a08c2)

- **The cell itself.** `zephyr_nucleo_renode_c_zenoh_serial_pubsub_e2e` is the
  Nucleo's one matrix Runtime cell, the very image that faulted, and tier 2
  runs it on schedule. In run 45351389552 (2026-11-24, main) it PASSED in both
  the 1-wise job and the pairwise nightly job, and that job's
  `check-skip-budget` line reads "3666 ran ... 0 skipped for an unmet
  precondition", so it ran, Renode 1.17.0 provisioned, and was not a skip.
  Should the frames come back, this cell faults as before.
- **The frame-size class.** `just check max-frame` fails on any frame in
  nano-ros's own code over `max_frame` (4096 B, `config/stack-floors.toml`)
  unless the shrink-only `config/max-frame-baseline.json` holds it; the rows
  this fix brought under the cap were removed, so any of them growing back
  fails. It runs after the fixture build of `just ci matrix`, matrix-nightly
  and full.
