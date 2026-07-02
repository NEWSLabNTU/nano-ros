---
id: 131
title: "ThreadX RISC-V64 zenoh firmware faults at NULL c_app_main after any rebuild — lane green only on stale binaries"
status: open
type: bug
area: threadx
related: [phase-277, phase-177]
---

## Summary

During phase-277 W4 (chatter parity) the ThreadX RISC-V64 QEMU lane was
observed to pass **only with stale prebuilt firmware**. Any rebuild of the
Rust examples (even at the pre-W4 baseline commit, unmodified source) produces
firmware that faults early with a jump through a NULL `c_app_main` pointer.

This means the lane's green status does not certify current sources: it
certifies whatever binaries were last left on disk.

## Evidence

- Baseline test: at commit `ea825a341` (pre-W4), `git stash` clean tree,
  rebuild the talker fixture, run the two-QEMU pub/sub e2e → same NULL
  `c_app_main` fault. So this is **not** caused by the W4/W3 example changes
  (they only made `app_main` unconditional; the fault reproduces without them).
- See `tmp/sdd-277/task-9-report.md` (phase-277 working notes) for the exact
  rebuild + QEMU invocations used.

## Suspected area

The boot path resolves `c_app_main` (CMake/cyclonedds `startup.c` symbol vs
the Rust `app_main` in the example staticlib) at link time; a link-order or
weak-symbol regression could leave the pointer NULL in fresh links while old
binaries still carry the resolved address. Compare the working stale ELF's
symbol table against a fresh link.

## Impact

- ThreadX RISC-V64 runtime e2e is unreliable as a gate (false green).
- Blocks trusting phase-277 W4 runtime verification on this platform
  (builds are green; runtime unproven).

## Next steps

1. Reproduce: clean `target*/` in one threadx example + fixture dir, rebuild,
   run lane, capture fault PC + symbol table diff vs a stale-good ELF.
2. Bisect link inputs (board crate, startup objects, `--gc-sections`).
3. Once fixed, re-run the W4 chatter e2e on this platform.
