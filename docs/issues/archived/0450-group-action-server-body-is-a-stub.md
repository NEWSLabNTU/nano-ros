---
id: 450
title: "The group-A action-server example body is a stub, and converging riscv64 deleted the only real implementation"
status: resolved
type: tech-debt
area: examples
related: [phase-338]
---

## What happened

phase-338 W3.d converged `qemu-riscv64-threadx/rust/action-server` onto the
group-A body. The group body publishes a FIXED three-element sequence:

```rust
let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
let _ = sequence.push(0);
let _ = sequence.push(1);
let _ = sequence.push(1);
```

The riscv64 copy computed the real thing — an iterative Fibonacci to
`ORDER = 5`, publishing feedback per element, with 256-byte feedback buffers
instead of 128. Converging **deleted the only implementation that actually
computed a Fibonacci sequence.**

## Why that was acceptable at the time, and why it is still wrong

Safe, narrowly: `matrix::CELLS` marks ThreadX riscv64 action `BuildOnly` —
phase-182.5 dropped it from the run matrix on wall-clock grounds — so the richer
body had never executed. Nothing regressed.

But the direction is backwards. `examples/*/action-server` is what a user reads
to learn how nano-ros actions work, and `Fibonacci` is the canonical ROS 2
action demo *because* the sequence is computed incrementally and streamed as
feedback. A server that publishes `[0, 1, 1]` regardless of the requested
`order` demonstrates the plumbing while quietly misrepresenting the example.

Note the goal request IS read and logged (`"Received goal request with order
{}"`) and then ignored for the result. That is the part most likely to mislead.

## Fix

Grow the GROUP body to compute the sequence, then let every platform converge on
that instead. Cost, measured:

* **four platforms with live runtime lanes** — native, threadx-linux,
  qemu-arm-freertos, qemu-arm-nuttx all run action cells as `Runtime`, so this
  is not a paper change;
* **feedback buffer 128 → 256 bytes** on constrained targets, which is the part
  that needs thought rather than a find-and-replace;
* the asserted markers (`Publish feedback`, `Goal succeeded`) do not change, so
  the e2e greps survive — but the number of feedback publishes does, and any
  test counting them would need checking.

## Why file rather than fix

It is a deliberate scope call, not an oversight: phase-338 was converging
bodies, and growing one mid-convergence would have changed the thing being
measured. The convergence is now done and every action lane is verified green,
which is exactly the state from which this is safe to attempt.

## Resolution (2026-08-06)

The group body now COMPUTES the sequence iteratively and streams one feedback
frame per element, on all six platform copies at once (the portability gate
requires them to move together). Feedback/result buffers grew 128 -> 256, and a
`MAX_ORDER = 50` constant bounds both the `heapless::Vec<i32, 64>` and the
256-byte CDR payload (4-byte length + 4 per element = 208 at the cap).

**The requested order is now honored**, which the deleted riscv64
implementation did not manage either — it used a fixed `ORDER = 5` and said why:
"the app-node shape doesn't surface the goal payload at tick time".
`for_each_active_goal_for_name` yields a goal id and status, not the request. So
the accepted order is carried from `on_goal` to `tick` through `State`
(previously `()`). One slot is enough for a single-goal demo; a concurrent
server would key it by `GoalId`, and the code says so.

## What this uncovered — issue 0461

Making the output depend on the order exposed that **the order never arrives**.
The server reads `1` for every goal: a client sending `order: 10` and a client
sending `order: 7` both produce

```
[INFO] Received goal request with order 1
```

Constant, not merely wrong — so it is reading a different field, not slipping an
offset. Filed as issue 0461 (high severity, `rmw`/actions).

That is the point of this issue, arriving from an unexpected direction. The stub
was not only under-demonstrating the example; it was **masking a wire defect**,
because the one consumer of the mis-read value was a log line nothing asserted
on. Every action e2e passed then and passes now — they check delivery markers,
none of which depend on the order being right.

Until 0461 is fixed the demo streams the sequence for the order it actually
receives, so a client asking for 10 gets `[0, 1]`. That is a faithful report of
a broken input rather than a fixed output that looked plausible, which is the
trade this issue argued for.

## Verification

* `example_portability` 6/6 — the six copies stayed byte-identical within their
  groups.
* `actions` + `action_multigoal` 4/4 native.
* `just freertos build-examples` and `just nuttx build-examples` clean — the
  256-byte buffers are the part that needed checking on constrained targets, and
  neither overflows a stack.
* `rtos_e2e` FreeRTOS action Rust/C/C++ **3/3** with the grown body running on
  the Cortex-M3.
* Nothing counts feedback publishes (`ACTION_PUBLISH_FEEDBACK_MARKER` has no
  consumer outside `output.rs`), so going from 1 frame to `order + 1` breaks no
  assertion — checked before changing it, as the issue asked.
