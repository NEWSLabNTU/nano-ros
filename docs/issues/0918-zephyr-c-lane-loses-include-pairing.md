---
id: 918
title: "The Zephyr C/C++ fixture lane dies in picolibc — `-include nros_libc_compat.h`
  loses its flag and becomes a bare source file, despite the issue-0840 `SHELL:` fix"
status: open
type: bug
area: build
related: [issue-0840, phase-400]
---

## Problem

Every Zephyr C/C++ fixture build fails at the FIRST picolibc TU:

```
[1/1288] Building C object modules/picolibc/CMakeFiles/c.dir/newlib/libc/argz/argz_add.c.obj
gcc: fatal error: cannot specify '-o' with '-c', '-S' or '-E' with multiple files
```

The compile line carries the header as a BARE FILE rather than as an argument to
`-include`:

```
... -D_POSIX_THREADS /home/…/zephyr/libc-compat/nros_libc_compat.h -fno-stack-protector ...
                     ^ no `-include` before it
```

so gcc sees two input files and one `-o`. An earlier `-include …/picolibc.h` on the
same line kept its flag; ours lost it.

## This is exactly issue 0840, at a site 0840's fix does not reach

`zephyr/CMakeLists.txt` already carries the fix and the reasoning:

```cmake
zephyr_compile_options(
    "$<$<COMPILE_LANGUAGE:C,CXX>:SHELL:-include ${NROS_REPO_DIR}/zephyr/libc-compat/nros_libc_compat.h>")
```

`SHELL:` exists to keep the pair together and exempt it from de-duplication, and
0840's note records it verified in a reproducer. It is evidently not holding for
Zephyr's own `modules/picolibc` target — our global `zephyr_compile_options`
reaches that target through a path where the `SHELL:` grouping is lost, most
likely because the flags are re-assembled into a plain string somewhere in the
picolibc module's own cmake rather than consumed as a genex list.

0840 already recorded TWO witnesses (native_sim/x86 and cortex-m/picolibc) and
argued for fixing the pairing rather than either symptom. This is a third, and it
says the pairing fix is still incomplete: it holds where the option list stays a
list, and not where a target flattens it.

## Consequence

The whole Zephyr C/C++ fixture lane cannot build. Verified as PRE-EXISTING, not
introduced by any local change: the same leaf was built on an unmodified checkout
of `main` and on a working branch, and both fail identically —

```
main:      rc=2, 1x "cannot specify"
branch:    rc=2, 1x "cannot specify"
```

Reproduce with:

```sh
source ./activate.sh
bash scripts/build/zephyr-fixture-make-driver.sh --filter c-listener-zenoh
```

## Why it is filed rather than fixed here

Found while validating phase-400 W5 (sharing one Corrosion cargo dir across the
C/C++ Zephyr builds). W5's wiring cannot be verified end-to-end until this is
fixed, because no C/C++ Zephyr image builds at all. The two are independent: W5
adds a `-D` to the cmake command line, and this failure happens inside Zephyr's
picolibc module before any nros cargo work runs.

## Directions

1. Find where `modules/picolibc`'s compile options are assembled and whether the
   `SHELL:` group survives it. `ninja -C <build> -t commands <that .obj>` shows
   the flattened line; compare with a target that keeps the pairing.
2. If the flattening is Zephyr-side and unavoidable, the option cannot be a
   global `zephyr_compile_options` — it has to be applied to the targets that
   need it, or the header supplied a way that survives flattening
   (`-D` + a wrapper, or an `-imacros` style single token).
3. Whatever the fix, add the picolibc TU to whatever reproducer 0840 used, so a
   fourth witness cannot appear the same way.
