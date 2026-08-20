---
id: 706
title: "A build tree survives a toolchain RESOLUTION change: the wipe guard compares the toolchain-file ARG, not the compiler it resolves to"
status: open
type: bug
area: build/cmake
related: [issue-0391, issue-0674, issue-0678, issue-0680]
---

## What actually happens

`just threadx_riscv64 build-fixture-extras` on a tree with pre-existing
riscv64 build dirs:

```
/usr/bin/riscv64-unknown-elf-gcc ... -isystem /usr/lib/picolibc/riscv64-unknown-elf/include ...
.../nros-board-threadx-qemu-riscv64/c/reent.c:29:10:
    fatal error: sys/reent.h: No such file or directory
```

Delete EVERY `examples/qemu-riscv64-threadx/*/*/build-*` and rerun: **zero**
`sys/reent.h` errors, 34 configures all resolving
`.../\.nros/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf`. The code is
fine. The build trees were not.

## Why the existing guard did not catch it

`nros_cmake_guard_build_dir` (issue 0391) exists for exactly this and its
comment describes the failure precisely — a cached `CMakeCache` pins
`CMAKE_C_COMPILER` at FIRST configure, and re-passing a different
`-DCMAKE_TOOLCHAIN_FILE` cannot move it, so it WIPES on a toolchain-file
mismatch.

The mismatch it detects is of the **argument**. These dirs were configured with
the same `-DCMAKE_TOOLCHAIN_FILE=.../riscv64-threadx.cmake` all along; what
changed underneath was what that file RESOLVES to. `_nros_riscv64_find_prefix`
searches the SDK store first and falls back to `find_program`, so installing
`riscv-none-elf-gcc` into the store silently changes the answer from Debian's
`riscv64-unknown-elf` (picolibc) to xPack (newlib) — with the argument
byte-identical. The guard sees no change and keeps the tree.

That is the same shape as the issues around it, one level up: 0674/0678 are
"the libc verdict must come from the compiler actually used", 0680 is "probe
`CMAKE_C_COMPILER`, not the prefix", and this is "a tree must be wiped when the
RESOLUTION moves, not only when the argument does".

`reent.c` (issue 0680, newlib-only) is simply the first file that could not
compile under the wrong answer, which is why this surfaced now rather than at
whichever earlier point the store toolchain was installed.

NOT verified: whether the surviving dirs' `.nros-cmake-configure.args` stamps
were in fact identical. It is the mechanism the code implies, and the observed
behaviour matches, but the stamps were deleted before this was understood.

## Direction

Record the resolved compiler in the configure stamp, not just the arguments —
then a store install that changes the resolution invalidates the tree the same
way a changed argument does. `NROS_RISCV64_LIBC` is already CACHE'd for exactly
this kind of "publish the choice" reason (0674); the stamp wants the same
treatment.

## How to reproduce, and how NOT to

```
rm -rf examples/qemu-riscv64-threadx/*/*/build-*      # ALL of them
just threadx_riscv64 build-fixture-extras            # green
# then restore a pre-store-install tree to see it fail
```

Every shortcut misleads, and this issue cost five wrong conclusions before the
right one:

* deleting only `build-cyclonedds` leaves the ZENOH trees, and the zenoh pass
  runs FIRST and builds the same `threadx_kernel` — so the failure looks like a
  cyclonedds bug while coming from a stale zenoh tree;
* `cmake -S . -B build-cyclonedds` by hand omits the toolchain file, caches
  `/usr/bin/cc`, and dies on `unknown mnemonic 'csrrci'` — self-inflicted, and
  the driver then REUSES that bad cache;
* `bash scripts/build/fixtures-build.sh threadx-riscv64 c cyclonedds` skips the
  recipe's `NROS_CMAKE_EXTRA_DEFS` (which carries `-DCMAKE_TOOLCHAIN_FILE`) and
  also lands on `/usr/bin/cc`.

Only the recipe, against trees that are ALL absent, answers the question.

## Blast radius

Tier 2 is 1-wise over platform, so this one coordinate fails the whole tier on
any host whose riscv64 trees predate its store toolchain — the same shape as
issue 0698. Tier 1 is native-only and cannot see it.
