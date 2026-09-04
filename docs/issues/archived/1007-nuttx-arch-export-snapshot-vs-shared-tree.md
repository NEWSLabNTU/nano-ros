---
id: 1007
title: "`just nuttx build-fixtures-arm` can leave every arm cell unrunnable, and
  the remedy it prints is the command that just short-circuited"
status: resolved
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

## Fix (2026-09-04, branch `fix/1007-1026-1027-followups`)

**Direction 2, plus Direction 3.** The test precondition now reads the per-arch
EXPORT SNAPSHOT — `$NUTTX_DIR/nros-nuttx-export-<arch>` — which is exactly what
`build-nuttx.sh` guarantees, so the build contract and the test precondition
stop disagreeing about which artifact is authoritative.

### Why direction 2 and not direction 1

The issue rejected direction 2 with "the snapshot is not what an image links
against". MEASURED, that is inverted — since phase-339 the snapshot is the ONLY
thing an image links against:

* the arm cells boot the FIXTURE binary (`QemuProcess::start_nuttx_virt`), never
  `$NUTTX_DIR/nuttx`. Both callers of `nuttx_kernel_path_for` (`rtos_e2e.rs:267`,
  `logging_smoke.rs:124`) use it as a PRECONDITION and discard the path;
* the fixture links `nros-nuttx-export-<arch>/{libs,startup,scripts}` and
  compiles against its `include/` — `nros_board_common::nuttx_export`,
  `nros_build_paths::nuttx_include_root`, `cmake/platform/nano-ros-nuttx.cmake`
  — and `check-nuttx-links-snapshot.sh` exists precisely to keep consumers off
  the live tree;
* MEASURED in a built fixture's dep-info
  (`…/nros-minsizerel/talker.d`): 7 NuttX inputs, 6 under
  `nros-nuttx-export-arm/`, and the one shared-tree entry is
  `include/nuttx/config.h` — an issue-0477 candidate WATCH (invalidate if the
  fallback ever becomes live), not a compile input.

So `nuttx_kernel_path_for` was the last consumer keyed on the shared tree, and
it was checking an artifact nothing on its own path consumes.

Direction 1 (make the short-circuit validate the SHARED TREE) is the one to
avoid: the tree holds one `.config`, so demanding it hold arm forces a full
reconfigure + kernel rebuild on every lane alternation — which is what issue
0433 removed, and what the short-circuit's own comment calls "not just slow but
pointless". It also breaks the riscv lane symmetrically, one lane at a time,
forever.

### What changed

`packages/testing/nros-tests/src/fixtures/binaries/nuttx.rs`:

* `nuttx_kernel_path_for(arch)` resolves `nros-nuttx-export-<snapshot_id>` and
  returns its ROOT. `NuttxArch::snapshot_id()` is new and names the CONFIG dir
  (`arm`/`riscv`), matching `build-nuttx.sh`'s `NUTTX_CONFIG_ID` key.
* `snapshot_for_arch(root, arch)` is the predicate, split out so it is testable
  without a provisioned tree — the arm half of issue 0743 was untestable for
  exactly that reason, and the reason was the env lookup, not the rule. It
  requires `<root>/libs` and, when `<root>/startup/crt0.o` is readable, checks
  its `e_machine`. Issue 0743's "ask the file, never the name" survives; the
  per-arch directories make the last-build-wins ambiguity structurally
  impossible, so an unreadable probe is accepted rather than failing a host
  whose export is fine.
* **Direction 3, the message.** "no NuttX Arm kernel export at <path> — this
  lane's images link `<export>/libs` and nothing has built it. Run: just nuttx
  build-fixtures-arm", plus a NOTE, emitted only on a mismatch, saying the
  shared tree holds the other arch and that this is expected and is not the
  problem. That note is the one thing the old message got most wrong: it sent
  the reader to the tree.

Five unit tests cover the predicate and the note (both directions of the note,
missing export, wrong-arch export, unreadable probe).

### Sweep

After the change, `grep -rn 'join("nuttx")'` over `packages/` + `scripts/` and
`grep -rnE '\$\{?NUTTX_DIR\}?/nuttx([^-a-zA-Z_/]|$)'` over the tree find only:
the diagnostic NOTE and its unit test, and `build-nuttx.sh`'s own prints (it is
the PRODUCER). No consumer reads the shared tree's kernel any more.

No new gate was added: the class is down to one deliberate diagnostic site,
and a gate forbidding `$NUTTX_DIR/nuttx` would need a self-exemption for it.

## Measured — the acceptance, on a riscv-configured tree

The tree was ARM-configured, so it was deliberately put back to riscv and
recovered.

1. Forced a riscv reconfigure (`nros-nuttx-export-riscv/.nros-export-key` moved
   aside, `NUTTX_DEFCONFIG=…/riscv/defconfig NUTTX_BOARD_MAKEDEFS=… NUTTX_CROSS=
   riscv-none-elf-gcc bash scripts/nuttx/build-nuttx.sh`). Result:
   `.config` `CONFIG_ARCH_BOARD="rv-virt"`, `$NUTTX_DIR/nuttx` `e_machine`
   `0xf3`. That is issue 1007's exact starting state: the OLD predicate compares
   `0xf3` against `0x28` and skips every arm cell.
2. Ran an arm Rust cell WITHOUT rebuilding anything. The arch precondition
   PASSED (no `[SKIPPED] nuttx: … is a RiscV image`). The cell instead reported
   the honest, actionable

       Test fixture is STALE …
         binary: …/nros-minsizerel/talker
         newer:  …/third-party/nuttx/nuttx/include/nuttx/config.h

   — the shared tree's `config.h` that the reconfigure regenerated, which the
   fixtures carry as a candidate watch (above). This is the issue's step 2, and
   it is now the ONLY thing standing between a riscv tree and a running arm
   cell.
3. Ran the ONE documented command, `just nuttx build-fixtures-arm`. Its C/C++
   half self-provisions through `build-nuttx.sh`, which short-circuits on the
   arm snapshot (leaving the tree riscv-configured, which is now fine), and its
   Rust half rebuilds the stale fixtures.
4. Re-ran the arm Rust cells: **3 passed, 0 failed** (pubsub / service /
   action), on a tree still configured for rv-virt. `just nuttx
   build-fixtures-arm` took 2 min 03 s (warm sccache), and its log carries the
   1007 symptom lines verbatim — `NuttX arm export up-to-date
   (nros-nuttx-export-arm) — skipping build/export.` followed by `NOTE: the
   shared tree stays configured for "rv-virt", not "qemu-armv7a".` — which is
   now a true statement about a state that works, rather than a trap.

Direction 3 demonstrated separately: with `nros-nuttx-export-arm` moved aside,
an arm cell prints

    [SKIPPED] nuttx: no NuttX Arm kernel export at
    …/third-party/nuttx/nuttx/nros-nuttx-export-arm — this lane's images link
    `<export>/libs` and nothing has built it. Run: just nuttx build-fixtures-arm
      (The shared tree at …/third-party/nuttx/nuttx/nuttx currently holds a
      RiscV kernel. That is expected and is NOT the problem: since phase-339
      every consumer links the per-arch export snapshot, not the tree.)

So: from a riscv-configured tree, ONE command makes an arm cell runnable, and
the message printed when it is not names that command — both halves of the
acceptance.

**End state of this worktree:** the shared NuttX tree is left RISCV-configured
with fresh arm fixtures. That is deliberate: it is the state the issue says
should be OK, and after this fix it is.
