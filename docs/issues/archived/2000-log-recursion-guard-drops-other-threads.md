---
id: 2000
title: "nros-log's recursion guard is process-wide, so a record logged while another thread is inside a sink is silently dropped"
status: resolved
resolved: 2026-10-23
resolved_in: "fix/2000-log-defer-not-drop"
type: bug
area: [logging, core]
severity: medium
found: 2026-10-22
related: [1999, phase-88]
---

## Measured

`dispatch_to_sinks` (`packages/core/nros-log/src/lib.rs`) opens with
`recursion_guard_check_and_set()`, a compare-exchange on ONE `static
RECURSION_GUARD: AtomicBool`. If the flag is already set, the record is
returned from without reaching any sink, and without being held or counted.

A temporary probe test, 2026-10-22, on `main` at `c534ac13c6`:

- Thread A logs `"from A"`. Its sink blocks on a barrier, so A stays inside
  `dispatch_to_sinks` with the guard set.
- Thread B, the test thread, logs `"from B"` through the same logger and sinks.
- B's record reached the sink **0** times.

Nothing is printed and nothing is counted. The early ring's overflow counter
does not see it, because the record is not held.

## Why it matters

Every multi-threaded image logs from several threads: tiered executors,
transport read tasks, the host's `cargo test` threads. Two records close
together on different threads lose one. That record is often the one a reader
needs, because concurrent activity is when things go wrong.

It also made `nros-node`'s
`executor::tests::two_nodes_on_one_executor_emit_under_their_own_names` fail
once under `just check node-std-tests` (issue 1999): one of its two marker
records never arrived, because a sibling test thread was mid-dispatch.

## Why it is this way

The section comment above the guard says it on purpose. The guard exists to
stop a sink that logs from inside `log()` from recursing forever. Telling that
re-entry apart from a different thread logging needs a thread identity, and
`nros-log` has none on `no_std` (`thread_local!` needs `std`). Dropping a
concurrent record was judged "acceptable" in phase-88. Nobody had measured
that the drop is silent and total for that record.

## Fix direction (a design decision, not taken here)

1. **A per-thread guard keyed on a platform thread identity.** For example, an
   `nros_platform_thread_current()` added to the platform ABI
   (`nros/platform.h`, RFC-0054), linked in under a cfg the way
   `nros_platform_clock_ns` is (`cfg(nros_log_clock)`). It costs one ABI symbol
   in every port. A bare-metal single-threaded port answers a constant, which
   is still correct there.
2. **A depth limit instead of a flag.** Let up to N dispatches be in flight and
   drop only beyond N. Concurrency up to N is then never dropped, but a
   re-entering sink now recurses N levels deep, which costs N dispatch frames of
   stack on RTOS tasks.
3. **Count the drops** (an `overflowed()`-style counter, reported at the next
   dispatch). This loses the same records, but no longer silently. It is worth
   doing whichever of 1 or 2 is chosen.

## Resolution (2026-10-23)

Maintainer chose deferral plus a loss count (options C + D in the comparison),
over a per-thread guard: that would have needed a thread-identity call in all
seven platform ports, and it would also have let two threads into a sink at
once, which sinks that write raw bytes (a UART) relied on never happening.

A record that finds the dispatch guard held is now HELD in the
`nros_log::early` queue, and the thread holding the guard delivers it before it
lets go:

- `early` is a reusable multi-producer queue now, not a one-shot boot ring.
  Each slot cycles `EMPTY -> WRITING -> READY -> READING -> EMPTY`, and a
  drain replays in raise order. The same `NROS_LOG_EARLY_RECORDS` knob sizes
  it: both uses ask how many records may wait for a sink at once.
- **The hand-off cannot strand a record.** The raiser publishes, then retries
  the guard; the holder releases, then looks for held records. Both are
  `SeqCst`, so one of them always sees the other.
- **A sink that logs is postponed, not refused, and it is bounded.** The holder
  makes at most `early::DRAIN_PASSES` (2) passes, each over records held when
  the pass began, so it neither loops nor grows the stack. A leftover waits
  for the next dispatch or `flush()`.
- **Losses are reported** (option D). A record that finds the queue full is
  counted (`early::overflowed()`), and the next delivery emits one
  `nros_log` warning with the count. It is delivered directly under the guard,
  so the report itself cannot be lost.
- One delivery at a time still holds: a sink never sees two threads.

Tests (each its own binary, since the state is process-wide):
`busy_dispatch_other_thread` (fails on the old code with `left: ["slow"]`),
`busy_dispatch_sink_logs`, `busy_dispatch_overflow` and
`busy_dispatch_stress` (8 threads x 5000 records: delivered + lost = sent, and
never two threads in a sink). The two concurrency tests passed 300 runs each,
and 100 runs each under `2 x nproc` busy loops.
