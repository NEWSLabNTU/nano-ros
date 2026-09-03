---
id: 1007
title: "`just nuttx build-fixtures-arm` can leave every arm cell unrunnable, and
  the remedy it prints is the command that just short-circuited"
status: open
type: bug
area: testing, build
severity: medium
found: 2026-09-03
related: [issue-0870, issue-0930, phase-414]
---

## What happens

A clean `just nuttx build-fixtures-arm` on a tree whose NuttX checkout is
configured for RISC-V exits 0 and leaves every arm cell unable to run:

    [SKIPPED] nuttx: the NuttX kernel at .../third-party/nuttx/nuttx/nuttx is a
              RiscV image, but this lane needs Arm

`scripts/nuttx/build-nuttx.sh`'s snapshot short-circuit fires first —

    NuttX arm export up-to-date — skipping build/export

— and the script says outright that it "guarantees the snapshot, not
`$NUTTX_DIR`". But the test-side precondition reads the SHARED TREE's `nuttx`
ELF: `nuttx_kernel_path_for(Arm)` in
`packages/testing/nros-tests/src/fixtures/binaries/nuttx.rs:128`, reached from
`rtos_e2e`'s `require_e2e`.

So the build contract and the test precondition disagree about which artifact is
authoritative. The build guarantees the export snapshot; the test reads the
shared tree; nothing reconciles them.

**And the skip message names `just nuttx build-fixtures-arm` as the remedy —
which is exactly the command that just short-circuited.** Following the
instruction reproduces the state.

## The working sequence, for anyone reproducing

1. Force the arm kernel build — move `nros-nuttx-export-arm/.nros-export-key`
   aside so the snapshot short-circuit cannot fire, then run
   `scripts/nuttx/build-nuttx.sh`.
2. **Rebuild the fixtures AGAIN.** Forcing the kernel regenerates
   `third-party/nuttx/nuttx/include/nuttx/config.h`, which is one of the fixture
   staleness probe's 614 inputs, so every NuttX fixture immediately reads STALE:

       Test fixture is STALE … newer: .../third-party/nuttx/nuttx/include/nuttx/config.h

3. Run the cell.

Two steps, and neither is discoverable from the message the lane prints.

## Why it matters beyond the inconvenience

The NuttX tree is SHARED between the arm and riscv lanes and holds one
architecture at a time. Whichever lane ran last wins, and the other lane's
"remedy" is inert. That is a state a CI runner or a contributor lands in by
running two lanes in the ordinary order, and the failure presents as a SKIP —
which reads as "not built here", not as "your tree is in the wrong state".

The skip itself is correct and loud (issue 0650's machinery working as intended).
What is wrong is that the remedy it names cannot clear it.

## Direction

Not settled. Three shapes, and the choice is about which artifact is
authoritative:

1. **Make the snapshot short-circuit check the SHARED TREE's arch**, not just
   the export key — so "up-to-date" means what the test will actually read.
   Closest to correct; the two sides then agree by construction.
2. **Make the test read the export SNAPSHOT** rather than the shared tree, which
   is what the build promises. Cheaper, but the snapshot is not what an image
   links against, so it may only move the disagreement.
3. **Make the skip message name the real remedy** (force + rebuild). Does not fix
   the disagreement, but stops the message being wrong — worth doing regardless
   of which of the above lands.

Acceptance: with a riscv-configured tree, ONE documented command makes an arm
cell runnable, and the message printed when it is not says what that command is.

## Found while

Running the phase-414 W3 experiment for issue 0870. It cost one forced kernel
build plus a second full fixture rebuild before any measurement could be taken.
