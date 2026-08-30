---
id: 918
title: "The Zephyr C/C++ fixture lane dies in picolibc — `-include nros_libc_compat.h`
  loses its flag and becomes a bare source file, despite the issue-0840 `SHELL:` fix"
status: resolved
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

## RESOLVED 2026-08-30 — scope the shim instead of chasing the pairing

Direction 2 was right. The option is no longer global:

```cmake
set(_nros_libc_shim "SHELL:$<$<COMPILE_LANGUAGE:C,CXX>:-include …/nros_libc_compat.h>")
if(TARGET app)
    target_compile_options(app PRIVATE "${_nros_libc_shim}")
else()
    message(WARNING …)          # fail-loud, not a silent fallback
endif()
zephyr_library_compile_options("${_nros_libc_shim}")
```

`app` is verified to exist at module time, probed on a real configure.

**What was tried first and did NOT work, recorded so it is not retried.** Moving
`SHELL:` outside the generator expression — matching Zephyr's own spelling in
`arch/posix/CMakeLists.txt` — changed nothing: still 932 orphans of 1258. The
placement was not the defect.

**The measurement that located it**: attributing every occurrence in one
`build.ninja` to its target showed all 932 orphans in `modules/picolibc` and all
326 correct ones elsewhere. One target, not a global pairing bug. After scoping:
**173 paired, 0 orphaned.**

The shim was always for USER code — the examples call `setvbuf(stdout, NULL,
_IONBF, 0)` and Zephyr's minimal libc has neither `setvbuf` nor `_IO*BF`. Zephyr's
own libc implementation never needed it. Sending it there was the mistake; the
de-duplication was only how the mistake surfaced.

*Verified:* `build-c-listener-zenoh` builds green, and `build-cpp-listener-zenoh`
builds green from PRISTINE (dir deleted first), producing `zephyr.exe`.

## A trap worth recording: the driver prints a STALE log tail

Two runs after the fix still reported the original gcc error. They had not run it.
`zephyr-fixture-run-one` exited on `NROS_ZEPHYR_WORKSPACE is required`, and the
driver then printed the PREVIOUS run's Zephyr log as its "log tail". The absorbing
-verdict class CLAUDE.md records for stale fixtures, one layer up: read the
scheduler log line, not just the tail, before believing a repeated failure.
