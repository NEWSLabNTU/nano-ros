---
id: 2164
title: "A Zephyr board joining a live domain over a serial link needs CONFIG_MAIN_THREAD_PRIORITY below the read task and a bigger RX ring, and nano-ros only warns about the first and documents the second"
status: open
type: limitation
area: zenoh, zephyr, serial, scheduling
severity: low
found: 2026-10-30
related: [issue-1534, issue-0852, phase-474, phase-478]
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
