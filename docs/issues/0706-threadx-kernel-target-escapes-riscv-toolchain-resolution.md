---
id: 706
title: "The threadx_kernel target compiles with Debian's picolibc gcc while the toolchain file resolves xPack/newlib — `reent.c` cannot find `sys/reent.h` on a CLEAN tree"
status: open
type: bug
area: boards/threadx-riscv64
related: [issue-0674, issue-0678, issue-0680, issue-0666]
---

## Symptom

`just threadx_riscv64 build-fixture-extras`, from EMPTY build dirs:

```
FAILED: nano_ros/CMakeFiles/threadx_kernel.dir/.../c/reent.c.obj
/usr/bin/riscv64-unknown-elf-gcc ... -isystem /usr/lib/picolibc/riscv64-unknown-elf/include ...
.../nros-board-threadx-qemu-riscv64/c/reent.c:29:10:
    fatal error: sys/reent.h: No such file or directory
```

Found by tier 2 (`build-test-fixtures lane=tier2`), which is the first lane to
build this coordinate — six other platforms in the same run pass
(`threadx_linux`, `freertos`, `qemu`, `nuttx`, `native`, zephyr).

## The two halves disagree, and both say so out loud

Every configure in the run printed:

```
-- nano-ros: riscv64 toolchain prefix /home/aeon/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf
-- nano-ros: riscv64-threadx libc = newlib
```

The resolver found the xPack store toolchain, and `newlib` is the RIGHT answer
for it — verified directly:

```
$ riscv-none-elf-gcc -E -include sys/reent.h -x c /dev/null   # store toolchain
(ok)
$ ls ~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/riscv-none-elf/include/sys/reent.h
(present)
```

So `nano-ros-board-riscv64-qemu.cmake`'s guard did exactly what it should:

```cmake
if(NROS_RISCV64_LIBC STREQUAL "newlib")
    list(APPEND _glue_srcs "${THREADX_BOARD_DIR}/reent.c")
endif()
```

But the compile ran Debian's `/usr/bin/riscv64-unknown-elf-gcc`, `-isystem`'d at
`/usr/lib/picolibc/...`. The verdict and the compiler are about two different
toolchains.

## Why the existing fix does not cover it

`cmake/toolchain/riscv64-threadx.cmake` already anticipates this exact pairing —
its comment quotes this error and explains the remedy:

> `CMAKE_C_COMPILER` is a CACHE variable and sticky ... So on such a tree the
> resolved prefix says xPack (newlib) while every compile still runs Debian's
> `riscv64-unknown-elf-gcc` (picolibc) ... That is not hypothetical: it is what
> made `NROS_RISCV64_LIBC=newlib` coexist with `fatal error: sys/reent.h` on ten
> leaves.

and probes `CMAKE_C_COMPILER` when it exists, falling back to the prefix
otherwise (issue 0680). That closes the case where a build tree was configured
BEFORE the SDK toolchain existed.

**This tree is fresh.** Both build dirs were removed before the run, so
stickiness is not the explanation. The `threadx_kernel` target reaches Debian's
compiler by a route the probe does not observe, and the probe's fallback — the
resolved prefix — then answers for a compiler the target will not use.

## Not yet established

WHERE the kernel target gets `/usr/bin/riscv64-unknown-elf-gcc`. The include
flags on the failing line come from `cmake/board/` (`-I.../cmake/board/../../packages/...`),
so the ThreadX kernel is assembled by the board cmake rather than by the leaf's
own toolchain-file'd configure — but which step selects that compiler, and
whether the toolchain file is in scope there at all, is not diagnosed here.

## Reproduction, and a warning about reproducing it

```
rm -rf examples/qemu-riscv64-threadx/c/{talker,listener}/build-cyclonedds
just threadx_riscv64 build-fixture-extras
```

Use THAT entry point. While investigating this I reproduced it three other ways
and every one of them was misleading:

* `cmake -S . -B build-cyclonedds` by hand omits the toolchain file entirely and
  caches `/usr/bin/cc`; the build then dies on `unknown mnemonic 'csrrci'`,
  which looks like a toolchain bug and is self-inflicted.
* the driver REUSES an existing build dir, so that bad cache survives a rerun —
  the same stale-tree trap this issue is adjacent to.
* `bash scripts/build/fixtures-build.sh threadx-riscv64 c cyclonedds` skips the
  recipe's `THREADX_CONFIG_DIR` / `NETX_CONFIG_DIR` and the toolchain wiring, and
  also lands on `/usr/bin/cc`.

Only the recipe reproduces the real failure.

## Why it matters beyond one leaf

Tier 2 is 1-wise over platform, so this single coordinate fails the whole tier —
the same blast radius issue 0698 had. There is no tier 2 on a host that hits
this, and tier 1 is native-only and cannot see it.
