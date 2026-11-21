---
id: 2807
title: "Switching the shared NuttX kernel tree from arm to riscv keeps the arm `boards/dummy.o`, so every riscv image fails `cannot find -lboards`"
status: open
type: bug
severity: medium
area: [nuttx, fixtures, build]
related: [0456]
found: 2026-11-21
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
   reconfigure is fatal and prints the reason.
3. Optionally, the export step checks every archive member's `e_machine`
   against the target arch, so the next door into this class names the member
   instead of `cannot find -l…`.

Workaround: delete `third-party/nuttx/nuttx/boards/dummy.o` (git-ignored
build output) and re-run the riscv build.
