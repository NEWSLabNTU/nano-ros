---
id: 856
title: "`example_portability` is red: phase-394 fixed a real cancel bug in the
  NATIVE action-server only, and the four RTOS copies still carry the bug"
status: resolved
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

## Resolution (2026-08-28) — propagated, not annotated

`KNOWN_DIVERGENCE` is **empty**: the phase-338 ratchet had been fully discharged,
so the tree was portable and this was a regression rather than a not-yet-done
convergence. Adding an entry would have re-opened a ratchet that had reached
zero, and `no_stale_divergence_entries` exists precisely to stop the list growing
back. Propagation was the only correct option.

`f714e6a01`'s body now lives in all four RTOS copies: `type State = i32` becomes
the `ServerState` struct (order, sequence, `step_ticks`, `wait`), `tick()` gained
the `GoalStatus::Canceling` arm that completes a cancelled goal as `Canceled`
carrying what was computed before the cancel arrived, and the execution loop is
paced by `NROS_FIB_STEP_TICKS`.

Two things were preserved per copy rather than overwritten, because the gate
compares NODE LOGIC and not the file:

* each platform's own `//!` header — prose is stripped before comparison, and
  the headers say true platform-specific things;
* `qemu-riscv64-threadx`'s `mod app_main;`, which is a glue-module declaration
  `normalize` strips by design (`GLUE_MODULES`).

The headers had also gone stale in a way the gate cannot see — they described a
`tick()` that only publishes feedback and completes. Each now records the cancel
arm and the pacing.

### Verified

* `example_portability`: 6/6 pass, including `copies_within_a_group_are_identical`
  and `no_stale_divergence_entries`, with `KNOWN_DIVERGENCE` still empty.
* All five copies normalize to one identical body (`md5 1b8f4de387`).
* `cargo check` clean on all four RTOS leaves — freertos, nuttx, threadx-linux
  and qemu-riscv64-threadx — plus `rustfmt --check` on all five.

`qemu-riscv64-threadx` needed `CC_riscv64gc_unknown_none_elf=riscv-none-elf-gcc`
to check at all: cc-rs defaults to `riscv64-unknown-elf-gcc` while the pinned
index dist installs the xPack `riscv-none-elf-*` names. That is a HOST
toolchain-naming gap, not a consequence of this change — confirmed by
compile-checking the pre-edit file, which fails identically. Worth its own issue
if anyone hits it again; `just threadx-riscv64 doctor` already names both
toolchain spellings as acceptable without wiring `CC_*` for the second.

### Not covered

QEMU runtime lanes for the four boards were not run here — tier 1 does not build
them. The change is byte-identical logic already exercised on native, and each
copy compiles for its own target, but "the cancel crosses the bus on FreeRTOS"
remains asserted only on native and over CAN (phase-394's own run).

### Worth doing separately

Check whether other portability groups have copies edited singly and not yet
noticed — this gate only fails on the group that was touched.
