---
id: 2773
title: "Switching the shared NuttX tree from arm to riscv can leave arm objects in the riscv export, because build-nuttx.sh discards distclean's failure"
status: open
type: bug
severity: medium
area: [nuttx, build]
found: 2026-11-21
related: [0433, 0525, 1249, 2617]
---

**Owner:** unassigned.

## What happens

`scripts/nuttx/build-nuttx.sh` builds both NuttX architectures in one shared
tree (`third-party/nuttx/nuttx`). When the tree holds the other
architecture's configuration it reconfigures, and the first step of that is

```sh
make distclean 2>/dev/null || true
```

Both the status and stderr are discarded. In one worktree, after an arm
build at 20:01 and a riscv build at 00:19 the next day, the riscv export
held **140 arm objects** the distclean should have deleted, among them:

| Directory | Stale arm objects |
| --- | ---: |
| `arch/arm/src` | 49 |
| `libs/libc/bin` | 47 (`aeabi_memclr*.o`, `fdt.o`, ...) |
| `net` | 19 |
| `libs/libxx` | 8 |
| `sched`, `drivers`, `boards/...`, `fs` | 15 |

`make pass2` then archived them into the riscv libraries beside the riscv
objects (`libc.a`: 736 `elf32-littleriscv` members and 47 `elf32-little`
ones). Every riscv image link failed:

```
riscv-none-elf/bin/ld: .../nros-nuttx-export-riscv/libs/libc.a: error adding symbols: file in wrong format
```

The export key was still written, so the next run took the
"export up-to-date" short-circuit and kept failing until the key was deleted
by hand.

`libs/libc/Makefile`'s `distclean` removes `bin/` outright, so a distclean that
ran to completion cannot leave those objects. It did not complete, and the
script could not say why. A `make distclean` run by hand afterwards, with the
riscv `.config` in place, exited 0 and deleted all of them. So the failure
depends on the configuration the tree held at the time (arm), and that state
is gone.

## Where to look

- `scripts/nuttx/build-nuttx.sh`, the reconfigure branch and `--clean`: two
  copies of `make distclean 2>/dev/null || true`. The `|| true` is presumably
  there for a tree with no `.config`, where distclean has nothing to clean.
  It also hides a real failure (issue 1249's class).
- The export key is written after `make export` without checking that every
  archive member is the export's machine. `nuttx_kernel_path_for()`'s
  `e_machine` check (issue 0743) looks at the kernel, not the archives.

## What closes it

- Distinguish "no `.config`, nothing to clean" from a failed distclean, and
  fail the build on the second, printing distclean's stderr.
- Optionally, before writing the export key, refuse an archive member whose
  ELF machine differs from the export's architecture. That would catch this
  class whatever left the objects behind.
- Reproduce it: configure and build arm, then build riscv in the same tree,
  keeping distclean's stderr.
