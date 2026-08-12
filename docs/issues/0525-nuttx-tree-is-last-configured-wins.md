---
id: 525
title: "One NuttX checkout serves two arches, and its `.config` is last-configured-wins"
status: open
type: tech-debt
area: nuttx
related: [issue-0511, issue-0477, issue-0433, phase-339, phase-337]
---

## The property

NuttX is built **in place**. `configure.sh` writes `.config` and the generated
`include/nuttx/config.h` INTO the checkout, and `third-party/nuttx/nuttx` is one
submodule serving both in-tree arches (`qemu-armv7a` and `rv-virt`). So the tree
holds exactly one arch's configuration at any moment, and which one is a
property of **build order**, not of the build being run.

`lane=tier2` builds `nuttx` and `nuttx-riscv`. RISC-V ran last, so the tree sat
at:

```console
$ grep -E '^CONFIG_(ARCH=|RAM_START|FLASH_SIZE)' third-party/nuttx/nuttx/.config
CONFIG_ARCH="risc-v"
CONFIG_RAM_START=0x80000000
CONFIG_FLASH_SIZE=0
```

It is also **sticky**: once a per-arch export snapshot exists,
`scripts/nuttx/build-nuttx.sh` reports

```
NuttX arm export up-to-date (nros-nuttx-export-arm) — skipping build/export.
```

and returns without reconfiguring. Asking for the ARM build does not restore the
ARM `.config`. Measured 2026-08-12: running the ARM provisioning script left
`CONFIG_ARCH="risc-v"` in place.

## What this already cost

Issue **0511**. The ARM Rust image was linked with the RISC-V memory map —
`MEMORY { ROM ... LENGTH = CONFIG_FLASH_SIZE }` and RISC-V has
`CONFIG_FLASH_SIZE=0`, so ROM had zero bytes and every byte placed in it
"overflowed". It read as a 400–500 KB size regression, survived clean rebuilds
(the stale `.config` lives in the submodule, not in any target dir), and cost a
bisect that had to be retracted because no revision had ever "fit".

phase-339 W2 had already recognised the hazard and moved the kernel LIBS and the
linker SCRIPT onto per-arch export snapshots; the HEADERS were left on the
shared tree, so the arch selection covered two of three input classes.

## What is fixed, and what is not

**Fixed** (0511): every nano-ros build input now resolves through
`nros_board_common::nuttx_export::include_root`, which prefers this arch's
snapshot `include/` and falls back to the live tree. Four call sites — the
linker-script preprocess in `nuttx_image_link`, its twin in `nuttx_ffi_build`,
and the two C compiles in `nuttx_platform_build`. `git grep
'nuttx_dir.join("include")'` in `nros-board-common` now returns only the
accessor's own definition.

**Not fixed:** the tree is still last-configured-wins. Nothing stops a future
build input, script, or out-of-tree consumer from reading `$NUTTX_DIR/include`
or `$NUTTX_DIR/.config` and silently getting the other arch. The remaining
in-repo readers are benign today and were checked: `cmake/platform/
nano-ros-nuttx.cmake` skips its subproject when cross-compiling, and the
`just/nuttx.just` / `build-nuttx.sh` uses are existence probes and doctor
output, not compile inputs. That is a fact about today's tree, not a guarantee.

## Why this is not an argument against consolidation

phase-337 W3 deliberately collapsed both witnesses onto one board crate: "the
board marker only selects trait impls, and those never differed between arm virt
and rv-virt — the arch delta is defconfig + toolchain DATA." That holds, and the
arch IS discriminated where it matters: `snapshot_root()` keys on
`CARGO_CFG_TARGET_ARCH`. 0511 was not consolidation leaking; it was a migration
that moved two of three input classes onto the per-arch mechanism.

## Directions

1. **Make the provisioning script arch-idempotent.** `build-nuttx.sh` should
   reconfigure when the requested arch differs from the tree's current
   `CONFIG_ARCH`, even when the export snapshot is up to date — or state
   explicitly that it is a snapshot-only path and that the tree's state is
   undefined afterwards. Today it silently means the second.
2. **Stop deriving anything from the shared tree.** The snapshot already carries
   `include/`, `libs/`, `scripts/`, `startup/`. If every consumer reads the
   snapshot, the tree's `.config` becomes an implementation detail of
   provisioning. A gate — no build input may name `$NUTTX_DIR/include` — would
   hold that, in the shape of `check-path-env-fingerprints`.
3. **Or give each arch its own checkout** (worktree per arch). Removes the
   shared mutable state entirely, at the cost of disk and a second submodule
   dance.

(1) is small and removes the surprise; (2) is the structural fix and makes the
class uncommittable; (3) is the heaviest and probably not worth it.
