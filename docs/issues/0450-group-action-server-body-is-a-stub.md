---
id: 450
title: "The group-A action-server example body is a stub, and converging riscv64 deleted the only real implementation"
status: open
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
