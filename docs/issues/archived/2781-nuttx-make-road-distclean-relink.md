---
id: 2781
title: "A NuttX make-road kernel tree does not link again after `make distclean`: the integration shell's platform objects are never re-archived"
status: resolved
type: bug
area: [nuttx, build]
severity: medium
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(nuttx): make distclean cleans the external apps, so the make road links again"
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

## Resolution (2026-11-21)

Reproduced on `main` (85d6ad1bd3) with `scripts/nuttx/make-road-build.sh arm`, then
`make distclean`, the same configure and `make`. The link failed with the symbols above,
plus `nros_platform_dealloc` and `nros_platform_panic`, and the new `libapps.a` held no
`platform.c` object.

**Cause: a missing clean edge, not the stamp alone.** The apps tree cleans only its
top-level directories that hold a Makefile (`CLEANDIRS := $(dir $(wildcard
$(APPDIR)/*/Makefile))`, in apps' `Make.defs`). `stage-external-apps.sh` wrote
`apps/external/Make.defs` and `Kconfig` and no Makefile, so `make clean` and `make
distclean` never entered `apps/external`. No external app's `clean` ran: neither the
shell's, whose `clean::` does remove `.nros-build/<arch>`, nor a user's own app's.
`libapps.a` went, while each app's objects and its `.built` stamp stayed, and the next
build found the stamp current and archived nothing. Making the stamp depend on the
archive would have fixed only this shell, and left every other external app with the
same failure.

**Fix.** The staging script installs `apps/external/Makefile`
(`integrations/nuttx/external-Makefile.in`), which runs `clean` and `distclean` in every
subdirectory with a Makefile. It is not upstream's `Directory.mk`:
- `Directory.mk` cleans only the subdirectories holding a bare `.built` or `.depend`,
  which a `PREFIX` build like this shell's never has.
- Its `MENUDESC` would make `preconfig` regenerate the staged `Kconfig`, and `distclean`
  delete it.

The apps tree reads builds from `Make.defs` and Kconfig from `Kconfig`, never from a
Makefile, so the file adds the clean edge and nothing else.

**A tree staged before this fix** prints one line on each build naming the staging
command, because the shell checks for that Makefile; re-running the command installs it.
The book's make-road step 1 says to re-run it after updating nano-ros.

**Covered.** `make-road-build.sh` ends with `make distclean`. It checks two things,
each failing on its own:
- no `.built` or `.nros-build/*` is left under `apps/external`;
- the same configure and `make` link an image that defines `nros_platform_clock_ns`.

Negative control: with `apps/external/Makefile` removed, a build prints the re-stage
line, and `make distclean` leaves `.nros-build/arm` behind, which the first check refuses.

The phase-581 W10 fixture's `git clean -ffdx` workaround can go once this lands.

