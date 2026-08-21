---
id: 748
title: "`test_nuttx_kernel_boots` runs `$NUTTX_DIR/nuttx` under qemu-system-arm, but the shared tree's kernel belongs to whichever arch configured last"
status: open
type: bug
area: testing
related: [issue-0525, issue-0196, issue-0711]
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
