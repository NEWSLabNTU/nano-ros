---
id: 856
title: "`example_portability` is red: phase-394 fixed a real cancel bug in the
  NATIVE action-server only, and the four RTOS copies still carry the bug"
status: open
type: bug
area: examples
related: [phase-394, phase-338]
---

## Problem

`nros-tests::example_portability copies_within_a_group_are_identical` fails on
main — the ONE real failure in an otherwise-green tier 1 (the other 175 are
capability skips on a host with no ROS router):

```
rust/action-server [A-scheduled]: qemu-arm-freertos    differs from native
rust/action-server [A-scheduled]: qemu-arm-nuttx       differs from native
rust/action-server [A-scheduled]: qemu-riscv64-threadx differs from native
rust/action-server [A-scheduled]: threadx-linux        differs from native
```

`f714e6a01` (phase-394, 2026-08-27) changed `examples/native/rust/action-server/
src/lib.rs` by +157 lines and touched no other copy. The portability group is
declared byte-identical by construction (phase-338 W3 moved every platform's
action-server to one Node-class body precisely so they could be), so one edited
copy is a failing gate by definition.

## Why this is not a cosmetic drift

The commit's own message calls the first of its two changes "a plain bug", and
it is on the CANCEL path:

* the server reported a cancelled goal as if it had completed — it "lied about"
  the cancel, in the commit's words;
* the execution loop gained pacing (`step_ticks`, one term per N ticks) so a
  cancel has a window to land in at all. Unpaced, order 10 finishes in ~4 ms
  while ROS 2's own cancel client cancels at t+3 s, so the goal always succeeded
  before the cancel arrived and the cancel path was never exercised.

Both properties are platform-independent. The four RTOS copies still have the
unpaced loop and the mis-reported cancel, so **every embedded action-server
example currently ships the bug that was just fixed on native** — and their
cancel paths are equally unexercised for the same timing reason.

So the gate is not complaining about formatting. It is reporting that a bug fix
reached one of five copies.

## Why it is filed rather than fixed

Propagating 157 lines of action logic into four embedded copies is a real change
to code whose lanes are QEMU/tier 2+, not verifiable by the tier 1 that found
this. Copying it blind and reporting tier 1 green would assert something no run
covered.

The other option the gate offers — a `KNOWN_DIVERGENCE` entry — is wrong here on
purpose. That entry has to name the wave that will converge the copies, and
inventing one to silence a gate records a plan nobody made. The gate's own
message is explicit that silence is not the third option.

## Direction

Propagate `f714e6a01`'s two changes to the four RTOS copies and run the QEMU
action lanes for each, OR file the convergence wave and reference it from a
`KNOWN_DIVERGENCE` entry. Not both, and not neither.

Worth checking at the same time whether the other portability groups have copies
that were edited singly and simply have not been noticed — this gate only fails
on the group that was touched.
