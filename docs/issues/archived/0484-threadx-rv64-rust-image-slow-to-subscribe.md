---
id: 484
title: "ThreadX-rv64 RUST image takes 2.1 s to reach `Subscriber created`, against
  0.1 s for the C and C++ images"
status: resolved
type: bug
area: threadx
related: [phase-342, issue-0481]
resolved_in: phase-342
---

## Measurement

Same test file, same QEMU invocation, same CycloneDDS RMW, same host, one run
(`threadx_riscv64_qemu.rs`, box, fixtures freshly built):

| cell | listener-ready | delivery | total |
| --- | --- | --- | --- |
| c | **0.10 s** | 1.10 s | 1.25 s |
| cpp | **0.10 s** | 1.20 s | 1.34 s |
| **rust** | **2.11 s** | 3.11 s | **5.26 s** |

`listener-ready` is the wait for `LISTENER_READY_MARKER`
("Subscriber created for topic:"), printed immediately after the subscription is
created. `delivery` is the subsequent wait for the first sample.

**The Rust image is ~21× slower to reach the same line**, and that delay
propagates: its talker boots on the same path, so the first sample lands ~2 s
later too. The delivery gap is largely the readiness gap paid twice, not a
separate transport problem.

The values are quantised to ~100 ms because `wait_for_output_pattern` polls at
that interval; 2.105 s is ~21 polls, not a coincidence.

## How it surfaced

It did not, for as long as anyone had looked. The test slept a fixed
`Duration::from_secs(4)` after starting the listener — comfortably longer than
either image needed — so C at 0.1 s and rust at 2.1 s were indistinguishable.
phase-342 W8b replaced that sleep with a wait on the readiness marker, and the
per-cell numbers separated immediately.

That is the second time this shape appeared in one phase: splitting the pubsub
fold exposed `rust_cyclone` at 34 s against 5 s siblings (issue 0481). **A fixed
delay does not just cost its duration — it hides the distribution underneath
it.**

## Root cause

A hardcoded 2 s sleep in the RUST entry wrapper, and only there:

```rust
// packages/boards/nros-board-threadx/src/entry.rs (two sites)
// Network stabilisation delay. Ticks at TX_TIMER_TICKS_PER_SECOND
// (100 by default) — 200 ticks ≈ 2 s, matching the legacy per-
// overlay wait in `node::app_task_entry`.
unsafe { tx_thread_sleep(200); }
```

`TX_TIMER_TICKS_PER_SECOND` is 100 on this board
(`packages/boards/nros-board-threadx-qemu-riscv64/c/hwtimer.c:5`), so 200 ticks is
exactly 2.00 s — which is the entire gap, to the resolution the poller can measure.

The C and C++ images never pay it. Their `main` calls the nros-c API directly and
never passes through this Rust entry.

### The trace that pins it

Both images are byte-for-byte comparable up to the app thread:

```
0.07s  [virtio] enable: link UP
0.08s  [app_thread] Calling c_app_main (FFI)     <- both images, identical
0.08s  [app] MAC 52:54:00:12:34:56 IP 192.0.3.10 domain 127
2.08s  nros entry ready                          <- rust only; +2.00s exactly
```

Two things follow. First, the delay is one sleep, not accumulated init work.
Second, the condition the sleep was approximating is **already satisfied before it
runs** — the link comes up at 0.07 s, a hundredth of a second before the app thread
even starts. There was nothing left to stabilise for.

The comment says where it came from: it was inherited to match "the legacy
per-overlay wait", not derived from a measurement of this board.

## Fix

Both sites deleted. Measured on qemu-riscv64-threadx, CycloneDDS:

| cell | before | after |
| --- | --- | --- |
| c | 1.35 s | 1.465 s |
| cpp | 1.45 s | 1.504 s |
| **rust** | **5.31 s** | **1.407 s** |
| **suite wall** | **7.37 s** | **1.505 s** |

The three languages now land within 100 ms of each other — rust the fastest of
the three, which is the property that
should have held all along: same platform crate, same board crate, same RMW, and a
C/C++ API that is a thin wrapper over this same Rust API. **The asymmetry was the
bug — the four seconds were only how it announced itself.** A language-shaped
performance difference on a shared stack is a claim that the stack is not actually
shared, and it was right.

If a board ever does need to wait here, it should wait for the LINK rather than for
a duration. A fixed delay does not just cost its time; it hides the distribution
underneath it.

## Why it mattered beyond 4 seconds

This is the tier-2 threadx-riscv64 lane, and the same rust-vs-C asymmetry would
have appeared on any timing-sensitive assertion there. A 2 s init that nobody
measured is also the kind of thing that turns into a flake on a loaded CI box.
