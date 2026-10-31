---
id: 2164
title: "A Zephyr board joining a live domain over a serial link needs CONFIG_MAIN_THREAD_PRIORITY below the read task (now refused otherwise) and a bigger RX ring (documented, not measured)"
status: open
type: limitation
area: zenoh, zephyr, serial, scheduling
severity: low
found: 2026-10-30
related: [issue-1534, issue-0852, issue-2179, phase-474, phase-478]
---

**Home: [phase-474](../roadmap/phase-474-safety-island-board-findings.md)** T3 —
the residue of issue 1534 (archived), split out when 1534's own defect, the
tx-flush task outranking the read task, was closed on every port.

## What is left

Measured on the safety island (S32K344, phase8-W10; numbers and traces in
[issue 1534](archived/1534-zephyr-tx-flush-task-outranks-the-read-task.md)):
a board that joins a domain whose peers are ALREADY sending loses the
router's write-filter interest replies unless

1. **the main thread registers below the read task.** Registration is CPU
   work, and Zephyr's default `CONFIG_MAIN_THREAD_PRIORITY` 0 outranks the read
   task (band 200 = k_thread 4 on 15 preempt priorities) for the whole burst.
   3 of 3 joins failed at main 0 and passed at main 5. nano-ros CHECKS this
   (`zephyr/cmake/nros_rmw_zenoh.cmake` warns on a serial zenoh link when main
   is not below the read task) but does not DERIVE it: Zephyr owns the symbol,
   and a `configdefault MAIN_THREAD_PRIORITY` under `NROS_ZENOH_LINK_SERIAL`
   would have to express `read_k + 1` -- `(N-1) - (200*(N-1))/255 + 1` over
   `CONFIG_NUM_PREEMPT_PRIORITIES` -- which Kconfig cannot compute from a
   symbol value. Options: an enumerated default per supported N (generated,
   gated against the cmake map), or a refusal instead of a warning.
2. **the RX ring is larger than its 1 KiB default.** High water 815-1024 of
   1024 at joins under load, one join in five overflowing; the island states
   4096. Documented in the `NROS_ZENOH_SERIAL_RX_RING_BYTES` help, not
   re-defaulted, because no nano-ros lane measures a serial join.

## What would close it

A lane that joins a serial zenoh image into a live domain (a board, or an
emulated UART fed at line rate under load) and reports the ring's high water
and the interest replies received -- then a measured ring default, and a main
priority that is derived or refused rather than warned about.

## Progress (2026-10-31)

**Item 1 -- landed as a REFUSAL.** `zephyr/cmake/nros_rmw_zenoh.cmake` now
stops the configure (`FATAL_ERROR`, was `WARNING`) when a Zephyr image links
zenoh over a serial link and `CONFIG_MAIN_THREAD_PRIORITY` is not below the
read task, and the error names the number (`read_k + 1`). When the read band
is so low that the read task already sits on the least urgent preemptible
slot (`read_k + 1 > NUM_PREEMPT_PRIORITIES - 1`), no main priority is legal
and the error says to raise the band (knob `ZENOH_READ_PRIORITY`) instead --
the old warning asked for an out-of-range number there.

Why refuse rather than keep warning: the same main thread spins the executor
above the read task for the image's whole life, which is the
app-above-transport inversion issue 0623 retired everywhere else. Why not
derive: Kconfig cannot compute `read_k + 1` from two symbol values, and an
enumerated `configdefault` is O(N^2) lines over `NUM_PREEMPT_PRIORITIES`'
0..128 range. (`configdefault` exists on 3.7, and by kconfiglib's
`_finalize_sym` a module's -- `modules/Kconfig` is sourced before
`kernel/Kconfig` -- lands ahead of the kernel's `default 0`; read, not built.
It stays an option for whoever wants the table and a gate on it.)

The in-tree serial fragment `cmake/zephyr/mps2-an385-serial.conf` states
`CONFIG_MAIN_THREAD_PRIORITY=5`. Measured, Zephyr 3.7, `west build
--cmake-only -b mps2/an385 examples/zephyr/rust/talker` with the
`talker-serial` row's `conf_files`:

| tree | result |
|---|---|
| `origin/main` | configures, prints the old WARNING (main 0, read k_thread 4) |
| this change | configures, `.config` has `CONFIG_MAIN_THREAD_PRIORITY=5` |
| this change, `-DCONFIG_MAIN_THREAD_PRIORITY=0` (negative control) | REFUSED: "... set CONFIG_MAIN_THREAD_PRIORITY=5 (or a larger, less urgent number up to 14) ..." |
| this change, `-DNROS_OVERRIDE_ZENOH_READ_PRIORITY=10` | REFUSED: read task on k_thread 14, "raise the read task's band" |

All four needed the generic board's `nros-nsos` snippet switched off locally:
on `origin/main` that snippet makes every mps2 build of this leaf fail at
Kconfig -- issue 2179.

**Item 2 -- still open.** No lane joins a serial zenoh image into a live
domain, so the RX ring default stays 1024 and documented. Not attempted
here: an emulated UART at line rate under load (QEMU ignores the baud rate,
per `book/src/user-guide/serial-transport.md`) would not reproduce the
ISR-vs-read-task race the island measured, so its high water would not be
evidence for a default.
