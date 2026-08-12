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

## Direction 2 LANDED 2026-08-12 — and it found a fifth site immediately

`scripts/check-nuttx-shared-tree-headers.py` (in `check-fast`): no build input
may take NuttX headers from the shared tree. Rust (`*.join("include")` on a
nuttx-ish path) and shell/cmake (`$NUTTX_DIR/include`) spellings, existence
PROBES exempted because both arches answer those identically, and an ALLOWED map
carrying the one legitimate site — the accessor's own definition and its
documented fallback.

**It paid for itself on the first run.** The hand sweep behind the 0511 class
fix found four readers, all in `nros-board-common`, because that is where the
grep was pointed. The gate found a fifth: `packages/drivers/sys/nuttx-sys/
build.rs`, which runs **bindgen** over the shared tree's headers — a standalone
crate outside the board helpers, so outside the sweep. Its generated FFI took
whichever arch was configured last, while its clang args said `arm-none-eabi`.

Fixing it moved the resolution to `nros_build_paths::nuttx_include_root`:
`nuttx-sys` is its own workspace and cannot depend on `nros-board-common`, and a
second copy of the resolution is precisely the drift that produced 0511.
`nros-build-paths` is dependency-free and already the shared path SSoT, so both
consumers now share ONE spelling. Its standalone lock moved by exactly the new
edge, via `just lock-update`.

Verified: gate green over 1359 tracked sources; tripwired live by reverting the
`nuttx-sys` site (fails, naming the file) and restoring it (passes);
`just rust-rtos-link-check` still passes all three leaves.

## Direction 1 LANDED 2026-08-12 — as the SECOND half of its own wording

The first half of direction 1 — "reconfigure when the requested arch differs" —
was rejected on reading the code it would change. That short-circuit is
deliberate and load-bearing: issue **0433** removed exactly this reconfigure,
because with the per-arch snapshot in place rebuilding the tree is not merely
slow but pointless, and doing it per arch made `build-fixtures` reconfigure
twice per run. Restoring it would trade one bug for the one before it.

So the second half shipped instead: **the path states what it guarantees.** When
the snapshot short-circuit fires and the shared tree holds a DIFFERENT board,
`build-nuttx.sh` now says so:

```
NuttX arm export up-to-date (nros-nuttx-export-arm) — skipping build/export.
  NOTE: the shared tree stays configured for "rv-virt", not "qemu-armv7a".
        This path guarantees the snapshot, not $NUTTX_DIR. Build inputs must
        resolve headers via nros_build_paths::nuttx_include_root, which reads
        nros-nuttx-export-arm/include (issue 0525; gated by
        check-nuttx-shared-tree-headers).
```

Silent when the two agree. The contract was always "this guarantees the
SNAPSHOT, never the tree"; it was just never written down where the side effect
happens, so every reader had to rediscover it — which is what 0511 cost.

## A SECOND shared mutable tree, found 2026-08-13 — the apps object tree

This issue is written about `third-party/nuttx/nuttx`. The same property holds
for `third-party/nuttx/nuttx-apps`, and nothing here or in the gate covers it.

`stage-external-apps.sh` symlinks `integrations/nuttx` in as
`apps/external/nano-ros`, one apps tree serving both arches. NuttX's
`Application.mk` names objects
`$(PREFIX)<src>$(SUFFIX)$(OBJEXT)` with `SUFFIX ?= $(subst $(DELIM),.,$(CWD))`,
and `$(CWD)` is that one symlinked dir — **identical for `qemu-armv7a` and
`rv-virt`**. `PREFIX` is empty, so objects land beside their sources, including
`packages/platform/nros-platform-posix/src/platform.c`'s, which
`integrations/nuttx/Makefile` names by absolute path.

`build-nuttx.sh` states the rest itself: the kernel `distclean` "does NOT touch
the apps tree", and nothing else cleans it across an arch switch. So one arch's
objects can survive into the other's `libapps.a` — 0511's failure class, in a
tree 0511's fix does not reach, because that fix and
`check-nuttx-shared-tree-headers` are both keyed on `$NUTTX_DIR/include`.

Not reproduced: the code is unambiguous but this tree currently holds no such
objects, and confirming it needs a real arm→riscv apps build (build the arm C
lane, touch nothing, build riscv, look for surviving arm objects).

The fix belongs to issue 0488 residue 4 — set `PREFIX` from `nros_build_dir`,
with the ARCH in the coordinate — and is recorded there. It is noted here because
this issue is where someone will look for "what else is last-configured-wins",
and the answer was never only the kernel checkout.

## Directions
2. ~~**Stop deriving anything from the shared tree.**~~ **DONE** — see above.
3. **Or give each arch its own checkout** (worktree per arch). Removes the
   shared mutable state entirely, at the cost of disk and a second submodule
   dance. **Still open, and now the only one.** With (1) and (2) landed the
   state can no longer reach a compile input and no longer surprises the reader,
   so this buys tidiness rather than correctness — probably not worth the disk.
