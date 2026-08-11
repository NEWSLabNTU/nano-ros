---
id: 514
title: "Runtime contract violations are detected, queued, and never drained — the only caller of drain_violations is a test binary, so every rule is unobservable in a real image"
status: open
type: bug
area: embedded
related: [issue-0505]
---

## Problem

The RFC-0052 runtime monitors do their half of the job: `check_rate`,
`check_age`, `check_latency`, `check_deadline_miss` and (new)
`check_timer_overrun` all run on the executor's spin and push a
`Violation` into `Executor::monitor_violations`. Nothing takes it out.

`Executor::drain_violations` has exactly two callers in the tree:

- `packages/core/nros-node/src/executor/tests.rs` — unit tests.
- `packages/testing/nros-tests/bins/contract-monitor/src/lib.rs` — a
  test binary.

`nros_diagnostics::Reporter::report`, the piece that turns a violation
into a `DiagnosticArray` for `/diagnostics`, has the same single test
caller. No board entry (`nros-board-freertos`, `nros-board-zephyr`,
`nros-board-linux`) drains the ring or constructs a reporter, and the
generated entry code does not either. So on every real image the
detection path runs, allocates, and discards.

## Why it matters

1. **The feature is invisible where it was built to be used.** An
   integrator who declares rate/age/latency/deadline contracts gets
   monitors compiled in, running every spin, and no output on any
   lane. The only way to observe a violation today is to write a
   custom spin loop that calls `drain_violations` — which is not
   documented as a requirement anywhere the contract is declared.
2. **The queue is bounded and drops silently.** `monitor_violations`
   is a `heapless::Vec<Violation, MAX_VIOLATIONS>` with
   `MAX_VIOLATIONS = 8`, and every push site is `let _ = push(v)`. In
   a never-drained image the ring fills within the first few faults
   and every later violation is discarded with no counter — so even a
   future drain would report a stale prefix, not current state.
3. **It makes a fault look like health.** A contract that is being
   violated continuously and a contract that is being met produce the
   same target-side output: nothing.

## Fix direction

- The board entry glue that already owns the spin loop (`run_tiers` on
  each board) should drain the ring each spin and hand violations to a
  `nros_diagnostics::Reporter`, gated so an image with no contracts
  compiles the path out — the same shape the monitor tables use (empty
  table = single branch).
- Publishing to `/diagnostics` is the ROS-native destination, but a
  minimum viable fix is a log line per violation: it needs no
  publisher, no topic wiring, and it turns a silent fault into a
  greppable one.
- Whatever lands, the drop path needs a counter (violations discarded
  because the ring was full), for the same reason every other shedding
  path in the stack needs one.
- Worth deciding explicitly: is draining the ring the APPLICATION's
  responsibility (documented, with the entry glue as a convenience) or
  the RUNTIME's (always on)? The current state is neither — it is
  undocumented and unimplemented.
