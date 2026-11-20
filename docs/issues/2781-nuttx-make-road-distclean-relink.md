---
id: 2781
title: "A NuttX make-road kernel tree does not link again after `make distclean`: the integration shell's platform objects are never re-archived"
status: open
type: bug
area: [nuttx, build]
severity: medium
found: 2026-11-21
related: [2236]
---

## What fails

The book's make road (`book/src/guide/platforms/nuttx.md`, "Add nano-ros to a
NuttX tree you build with make") builds once and then cannot be rebuilt from
`make distclean`. In a kernel and apps tree that has linked once:

1. stage the integration shell (`scripts/nuttx/stage-external-apps.sh`),
   configure the kernel from the board's arm fragment
   (`packages/boards/nros-board-nuttx-qemu/nuttx-config/arm/defconfig`) with
   `CONFIG_NROS=y` and `CONFIG_INIT_ENTRYPOINT="nsh_main"`, and `make` --
   it links;
2. `make distclean` in the kernel tree;
3. stage and configure again exactly as in step 1, and `make`.

Step 3 fails at the final link:

```
undefined reference to `nros_platform_clock_ns'
undefined reference to `nros_platform_condvar_wait_until'
undefined reference to `nros_platform_condvar_drop'
undefined reference to `nros_platform_condvar_signal_all'
make: *** [tools/Unix.mk:565: nuttx] Error 2
```

The image lacks the objects of `packages/platform/nros-platform-posix/src/platform.c`,
which the shell compiles under its `PREFIX` (`apps/external/.nros-build/<arch>/`).

## Likely cause (not diagnosed)

The shell's objects and their `.built` stamp live out of tree under `PREFIX`,
which `make distclean` does not remove, while the library they were archived
into is gone. The stamp says "built", so they are never archived again. A
user following the documented `distclean` + reconfigure loop gets a link error
that names platform symbols, not a stale stamp.

## Where it was seen

Found while building the configuration work's NuttX environment-probe fixture
(phase-581 W10) in its own `git clone --shared` copies of the pinned kernel
and apps trees; that fixture now avoids it with `git clean -ffdx` on its own
clones before each build. Not yet reproduced on a tree built from `main`
alone; the branch it was seen on changes `integrations/nuttx/` only by a
generated reorder of the RMW choice and one added include path, neither of
which touches what is archived, so it is expected on `main` too.

## Fix direction

Tie the shell's `.built` stamp to the archive it fills (or put `PREFIX` where
`distclean` removes it), and add the distclean-then-rebuild sequence to a
make-road test so the second build is covered.
