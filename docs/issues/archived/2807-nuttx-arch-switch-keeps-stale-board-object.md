---
id: 2807
title: "Switching the shared NuttX kernel tree from arm to riscv keeps arm objects (first `boards/dummy.o`), so every riscv image fails to link"
status: resolved
type: bug
severity: medium
area: [nuttx, fixtures, build]
related: [0456]
found: 2026-11-21
resolved: 2026-11-23
---

## What happens

`just build-test-fixtures lane=tier2` builds the arm NuttX rows, then the
riscv ones, in the one kernel tree `third-party/nuttx/nuttx`. The riscv
rows `workspace-c-nuttx-riscv-realtime` and `nuttx-riscv-c-zenoh-talker`
fail to link:

```
ld: skipping incompatible …/nros-nuttx-export-riscv/libs/libboards.a when searching for -lboards
ld: cannot find -lboards: No such file or directory
```

It reproduces on a solo re-run, so it is not load.

## Measured

`libboards.a` in the riscv export holds two members, of different machines:

```
boardctl.o: ELF 32-bit LSB relocatable, UCB RISC-V, RVC, soft-float ABI
dummy.o:    ELF 32-bit LSB relocatable, ARM, EABI5
```

`third-party/nuttx/nuttx/boards/dummy.o` is dated 18:05, from the arm
stage; the riscv kernel and its export are dated 18:24, after
`build-nuttx.sh` printed "NuttX tree is configured for 'qemu-armv7a', need
'rv-virt' — reconfiguring." and ran `make distclean`.

## Why (measured)

`make distclean` in that tree FAILS, before it reaches `boards/`:

```
integrations/nuttx/Makefile:79: *** CONFIG_NROS_ROS_EDITION is unset — is integrations/nuttx/Kconfig sourced?.  Stop.
make[2]: *** [Makefile:30: nano-ros/_clean] Error 2
make: *** [tools/Unix.mk:805: …/nuttx-apps_clean] Error 2
```

(rc 2, run by hand on the tree as the failed build left it; `boards/dummy.o`
was still there afterwards.)

- `integrations/nuttx/Makefile` refuses an empty `CONFIG_NROS_ROS_EDITION`
  at PARSE time, for every goal. The apps tree's clean pass descends into
  `external/nano-ros` and parses that Makefile, so a clean under a `.config`
  whose Kconfig did not source `integrations/nuttx/Kconfig` stops there (the
  riscv tree's `.config` here carries no `CONFIG_NROS_*` symbol at all).
  That failure was OBSERVED under the riscv `.config`; that the arm→riscv
  switch's own `distclean` failed the same way is INFERRED from the arm
  object surviving it, since `2>/dev/null` kept no record.
- `scripts/nuttx/build-nuttx.sh` runs `make distclean 2>/dev/null || true`,
  so the failed clean prints nothing and the reconfigure goes on.
- The clean stopped in the apps pass, so the kernel directories after it
  (`boards/` among them, whose `CLEAN` is `rm -f *.o …`) were never cleaned.
  `dummy.c` is older than the surviving arm `dummy.o`, so make does not
  recompile it, and `ar`, which checks no machine type, archives it into the
  riscv `libboards.a`. `ld` then skips the whole archive and names a
  missing file.

This is issue 0456's class (an object from the previous arch reaching the
next arch's image) through a different door. 0456 added an ELF `e_machine`
check in `run_image_link`; that check never runs here, because the link fails
first.

## Fix direction

1. The edition guard applies only to goals that build: a clean goal parses
   the Makefile without needing the Kconfig symbol.
2. `build-nuttx.sh` must not swallow `distclean`'s status. A failed clean on a
   reconfigure is fatal and prints the reason. The fix's acceptance is the
   three layers below, each checked after an arm→riscv switch: no foreign
   object, no archive with a foreign member, no apps `.built` stamp from the
   previous arch. A clean that removes objects but leaves archives or stamps
   lets a member `ar` never re-adds survive into the next export. (Whether a
   distclean that SUCCEEDS clears all three is not yet measured here.)
3. Optionally, the export step checks every archive member's `e_machine`
   against the target arch, so the next door into this class names the member
   instead of `cannot find -l…`.

## The export cache keeps the bad archive

Deleting `boards/dummy.o` alone does NOT recover: the next run prints
"NuttX riscv export up-to-date … — skipping build/export" and links the same
`libboards.a` (measured — `ar t` still lists `dummy.o`). The snapshot key
(`<export>/.nros-export-key`) hashes HEAD, defconfig and board, never the
export's contents, so a poisoned export stays current until one of those
moves. A member check at export time (fix 3) would also keep a bad export
from being recorded as current at all.

## It is not only `boards/`

With `dummy.o` rebuilt, the next link failed in `libc.a(lib_strchr.o):
relocations in generic ELF (EM: 40)`. A scan of the riscv export's archives
found 74 ARM members in five of them (`libc.a` 47, `libnet.a` 19,
`libdrivers.a` 5, `libfs.a` 2, `libopenamp.a` 1), and the kernel and apps
trees held 138 ARM objects, 86 outside the arm-only `arch/arm` and
`boards/arm` directories. Every directory the failed clean never reached kept
its arm objects; `boards/` was only the first archive `ld` met.

Deleting those objects is not enough either: `ar` replaces only the members
it re-adds, so the kernel tree's own intermediate archives (`staging/lib*.a`,
`libs/libc/libc.a`, `net/libnet.a`, `drivers/libdrivers.a`, `fs/libfs.a`)
keep their ARM members, and the export copies them again (measured: after the
objects went, `libc.a(lib_strchr.o)` was still EM 40).

The apps tree has a third layer: each directory's `.built` stamp says its
objects are already in `libapps.a`, so a directory that keeps its stamp is
never re-archived. With the kernel side clean, the riscv `libapps.a` held 4
members and the link failed on `undefined reference to 'nsh_initialize'`;
with the stamps cleared it held 145 and the riscv rows linked.

Workaround (measured, tier-2 NuttX stage green after it), all of it
git-ignored build output:

1. every ARM object outside `arch/arm` and `boards/arm` (`file` names the
   machine);
2. every kernel-tree archive holding an ARM member (`ar x` it, then `file`);
3. every `nuttx-apps/**/.built` stamp and `nuttx-apps/libapps.a`;
4. the riscv export's `.nros-export-key`;

then re-run the riscv build. Each step was found by the previous one's
failure, which is the case for fix 2: a fatal `distclean` would have reached
all three layers at once.

## Resolution

Fixed 2026-11-23, together with [issue 2773](2773-nuttx-arch-switch-distclean-failure-mixes-export.md):
the integration Makefile's clean goals now parse without the ROS edition, so
`distclean` runs to completion on an arch switch, and a distclean that still
fails stops the build. The export-machine check refuses any foreign object
that survives. Measured arm then riscv in one tree: the riscv build links and
its export holds RISC-V objects only.
