---
id: 748
title: "`test_nuttx_kernel_boots` runs `$NUTTX_DIR/nuttx` under qemu-system-arm, but the shared tree's kernel belongs to whichever arch configured last"
status: resolved
type: bug
area: testing
related: [issue-0743, issue-0525, issue-0196, issue-0711]
---

# 0748 — the NuttX boot test loads a RISC-V kernel into an ARM QEMU

The only genuine failure in the first complete tier-2 run (2026-08-21;
1492 passed, 180 lane-skips, 8 by-design skips, **1 failure**):

```
nros-tests::nuttx_qemu :: test_nuttx_kernel_boots
NuttX did not reach an NSH prompt in 10 s (no NuttX output at all).
Kernel: .../third-party/nuttx/nuttx/nuttx
Output:
qemu-system-arm: Couldn't load elf '.../third-party/nuttx/nuttx/nuttx':
  The image is from incompatible architecture
```

Confirmed directly:

```
$ file third-party/nuttx/nuttx/nuttx
ELF 32-bit LSB executable, UCB RISC-V, RVC, soft-float ABI
$ grep CONFIG_ARCH third-party/nuttx/nuttx/include/nuttx/config.h
#define CONFIG_ARCH "risc-v"
```

## Cause

`nuttx_kernel_path()` (`nros-tests/src/fixtures/binaries/nuttx.rs`) resolves the
kernel as:

```rust
std::env::var("NUTTX_DIR").ok().map(|dir| Path::new(&dir).join("nuttx"))
```

That is the SHARED tree, whose build output belongs to whichever arch was
configured last. The test then launches it under `qemu-system-arm`. When the
last configure was riscv — which it was here — the test loads a RISC-V ELF into
an ARM emulator.

Both per-arch export snapshots exist and are correct:

```
third-party/nuttx/nuttx/nros-nuttx-export-arm
third-party/nuttx/nuttx/nros-nuttx-export-riscv
```

so the information needed to pick the right kernel is already on disk. The
resolver simply does not consult it.

## This is issue 0525's class, one artifact over

0525 established the rule for NuttX HEADERS: the shared tree's `nuttx/config.h`
belongs to whichever arch configured last, so consumers must resolve through
`nros_build_paths::nuttx_include_root` / cmake's `nros_nuttx_include_root`, which
prefer this arch's export snapshot. `check-nuttx-shared-tree-headers` gates it.

The same reasoning applies with more force to the kernel ELF — a compiled binary
is the most arch-specific artifact in the tree — but the gate only looks for
header includes, so a Rust helper joining `$NUTTX_DIR/nuttx` is invisible to it.
**A gate whose scope is narrower than the rule it enforces**, which is issue
0196's pattern and the third instance found this week (the sched-dim arm compile
script was the second, fixed 2026-08-20).

## Why it surfaced now

`test_nuttx_kernel_boots` only started reporting this because issue 0711 removed
its print-and-pass arm. Before that, a run with no NuttX output at all reported
green with "kernel may need configuration". So the test has probably been loading
the wrong kernel whenever riscv configured last, silently, for as long as both
arches have been built.

That also means this is NOT a regression from any recent change, and it should
not be bisected as one.

## Fix

Resolve the kernel per-arch, the way headers already are: prefer
`$NUTTX_DIR/nros-nuttx-export-<arch>/…` for the arch the test is about to
emulate, and fall back to the shared path only when no snapshot exists. The
test knows which QEMU it is invoking, so it knows which arch it wants.

Then widen `check-nuttx-shared-tree-headers` — or add a sibling — so that
resolving ANY build output from the shared tree is caught, not just includes.
Without that, the next artifact to be read from `$NUTTX_DIR` repeats this.

Skipping when the snapshot is absent is preferable to falling back: a boot test
that silently emulates the wrong arch is exactly the failure mode 0711 removed.

## Resolution (2026-08-21): duplicate of 0743, already fixed

Filed from a tier-2 failure without first checking whether the defect was
already known. It was: **issue 0743** covers exactly this — "nuttx kernel path
has no arch discrimination" — and it is archived as resolved.

The fix landed in `a660be83f`, the same commit that removed the test I saw fail.
`nuttx_kernel_path()` is gone, replaced by `nuttx_kernel_path_for(arch)`, which
reads the ELF `e_machine` and refuses a mismatch:

```
the NuttX kernel at <path> is a Riscv image, but this lane needs Arm (<board>).
The arm and riscv configurations share that ONE filename and each `make`
reconfigures the tree (issue 0743), so the last build wins. Reconfigure and
rebuild: <hint>
```

Both remaining callers (`logging_smoke.rs`, `rtos_e2e.rs`) use the checked form,
and `cargo check -p nros-tests --tests` is clean.

So the analysis in this issue was right and the conclusion was redundant. Two
things in it are still worth keeping, which is why this is resolved rather than
deleted:

* The failure I observed was real and is the one 0743 describes, now with a
  concrete reproduction: riscv configured last, `qemu-system-arm` handed a
  RISC-V ELF.
* The **gate-scope** observation stands and 0743 does not cover it.
  `check-nuttx-shared-tree-headers` inspects header includes only, so a Rust
  helper joining `$NUTTX_DIR/nuttx` was invisible to it. The resolver is fixed;
  the gate that would have caught it still cannot see that class. That is issue
  0196's pattern and worth its own gate widening — filed separately if anyone
  picks it up.

Lesson for me: search `docs/issues/archived/` before filing from a test failure.
0743 was archived four commits before I wrote this.
