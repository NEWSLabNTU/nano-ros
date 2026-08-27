---
id: 845
title: "The zephyr FIXTURE path cannot build its C zenoh leaves — ccache mangles `-include`, and the dev path hides it"
status: resolved
type: bug
area: build
related: [issue-0805, issue-0549, phase-394]
---

## Symptom

`just zephyr build-fixtures` fails on every C zenoh leaf. A single leaf, from a
WIPED build dir:

```console
$ rm -rf ../nano-ros-workspace/build-c-talker-zenoh
$ NROS_ZEPHYR_FIXTURE_FILTER='build-c-talker-zenoh' just zephyr build-fixtures
… 32 × gcc: fatal error: cannot specify ‘-o’ with ‘-c’, ‘-S’ or ‘-E’ with multiple files
rc=2
```

Every failing TU is under `modules/picolibc/CMakeFiles/c.dir/…`.

## Why gcc sees "multiple files"

The compile line carries the nano-ros libc-compat header as a BARE PATH, with no
`-include` in front of it:

```
… -ffreestanding -fno-builtin -D_POSIX_THREADS \
  /home/…/nano-ros/zephyr/libc-compat/nros_libc_compat.h \
  -fno-stack-protector …
```

So gcc gets that header AND the `.c` as two inputs, and refuses `-o` with both.
The header is added correctly at `zephyr/CMakeLists.txt:387`:

```cmake
zephyr_compile_options(-include ${NROS_REPO_DIR}/zephyr/libc-compat/nros_libc_compat.h)
```

and the SAME command line shows an unmangled
`-include …/undef_system_defines.h` a few tokens earlier — so the flag is not
being lost generally, only for this one.

## The likely mechanism, and the loose thread worth pulling

The failing TUs run under **ccache**:

```
/usr/bin/ccache /usr/bin/gcc …
```

That should not be happening. The fixture record for this leaf passes
`-DUSE_CCACHE=0` AND `-DCMAKE_C_COMPILER_LAUNCHER=sccache`
(`scripts/build/zephyr-fixture-leaves.sh --emit records`), i.e. the lane asks
for ccache OFF and sccache as the launcher. The picolibc module compiles with
ccache anyway.

So there are two candidate defects and they may be one:

1. `-DUSE_CCACHE=0` does not reach the picolibc module build.
2. ccache, parsing a command line with `-include <abs-path>`, drops the flag and
   leaves the path — which is what the compile line looks like.

Neither is confirmed. What IS established is that the leaf cannot build.

## Why nobody noticed: the dev path does not go through here

`just zephyr build-c` builds the same six examples and **works** (measured 337 s,
rc=0). It loops `just zephyr build-one` per example, which is a different
invocation with a different flag set. So the two paths that build the same
leaves disagree, and only the one CI uses is broken.

That is the same divergence issue 0549 closed for `build-logging-smoke` — a
second builder for fixtures the manifest already declares — except here the
manifest path is the broken one.

## How it was found

Issue 0805 swept the lanes for a serial-dispatch defect and proposed replacing
the five `zephyr-dev.just` loops with filtered `build-fixtures` calls, since the
manifest covers every leaf they build (verified as SETS: each filter selects
exactly the loop's six examples). The delegation was written, and the first run
of `just zephyr build-c` through it failed — not because the delegation was
wrong, but because the path it delegates TO is.

**The delegation was reverted**, because routing a working developer command
into a broken lane is worse than the duplication it removes. It should land once
this is fixed; the filters are recorded in 0805 and were coverage-verified.

## Acceptance

* `NROS_ZEPHYR_FIXTURE_FILTER='build-c-talker-zenoh' just zephyr build-fixtures`
  succeeds from a wiped build dir.
* Whichever of the two candidates above is the cause is named, with the other
  ruled out — not both "fixed" speculatively.
* Say whether the other zephyr leaf families (cpp, rust, xrce, cyclonedds) share
  it. Only c/zenoh was exercised here.
* Then land 0805's delegation, so the two builders stop diverging.

## RESOLVED (2026-08-28) — TWO bugs in one feature, the first masking the second

Both candidates in the original write-up were WRONG. ccache is innocent and
`USE_CCACHE` is irrelevant. Ruled out by direct test, not by argument:

```
$ /usr/bin/ccache /usr/bin/gcc -include hdr.h -c a.c -o a2.o
  ok                                   # ccache 4.5.1 handles -include fine
```

and the mangled command is already mangled *in `build.ninja`*, before any
compiler runs. The flag is lost at CMAKE CONFIGURE time.

### Bug 1 — CMake de-duplicates `-include`, orphaning its path

`zephyr/CMakeLists.txt` passed the flag and its argument as two separate list
elements, deliberately, because a space inside one generator expression makes
them a single argument:

```cmake
zephyr_compile_options(
    "$<$<COMPILE_LANGUAGE:C,CXX>:-include>"
    "$<$<COMPILE_LANGUAGE:C,CXX>:${NROS_REPO_DIR}/zephyr/libc-compat/nros_libc_compat.h>")
```

That reasoning is right and the remedy is wrong. Zephyr already passes
`-include .../undef_system_defines.h`, CMake de-duplicates compile options, and
the second bare `-include` is dropped as a duplicate — leaving its path behind
as a second input file. Reproduced in 20 lines:

```cmake
target_compile_options(t PRIVATE -include ${CMAKE_CURRENT_SOURCE_DIR}/first.h)
target_compile_options(t PRIVATE
    "$<$<COMPILE_LANGUAGE:C,CXX>:-include>"
    "$<$<COMPILE_LANGUAGE:C,CXX>:${CMAKE_CURRENT_SOURCE_DIR}/second.h>")
```
```
FLAGS = -include /…/first.h /…/second.h      <- one flag, two paths
```

Fixed with `SHELL:`, which is the CMake feature for exactly this: it splits on
spaces into separate arguments AND exempts the group from de-duplication. Same
reproducer after the change:

```
FLAGS = -include /…/first.h -include /…/second.h
```

### Bug 2 — the shim is not inert inside the libc's own build

Fixing bug 1 delivered the header where it had never actually arrived, and
exposed what it does there. `zephyr_compile_options()` reaches EVERY target,
including the picolibc MODULE's own sources. The shim's `#include <stdio.h>`
runs before picolibc's TU sets its feature-test macros, so GNU extensions are
never declared and `newlib/libc/ssp/mempcpy_chk.c` fails:

```
error: implicit declaration of function ‘mempcpy’ [-Werror=implicit-function-declaration]
```

Established by running the build's OWN command line both ways: without the
`-include` that TU compiles clean (rc=0); with it, the error.

Fixed by guarding the shim on `_LIBC` — picolibc's own marker for "I am building
the C library", present on that command line as `-D_LIBC`. Exact discriminator:
the 12 callers this shim exists for are all application sources under the zephyr
examples, none of which is the libc.

The header's own comment claimed it "does nothing" on picolibc because `_IONBF`
is defined there. True of the *body*; not true of the `#include <stdio.h>` above
it, which is unconditional. An inertness claim has to cover the whole file.

### Verified

| | |
| --- | --- |
| `build-c-talker-zenoh`, wiped dir | rc=0, 38 s |
| c + cpp x zenoh + xrce, 24 leaves | **rc=0, 796 s** |
| rust + cyclonedds families | **rc=0, 1361 s** |
| `cannot specify '-o'` errors | 0 |
| `mempcpy` errors | 0 |

Every zephyr leaf family now builds through the fixture path — the acceptance
asked whether the others shared the defect, and they did: cpp, xrce, rust and
cyclonedds all go through the same `zephyr_compile_options()` and were all
broken. All four are verified green above.

### A note on my own comment

The first attempt at bug 2's fix broke the header outright: the explanatory
comment contained a path glob ending `*/`, which CLOSED the block comment early
and turned the rest of the file into code. Caught immediately by the same TU
test. Worth recording because a C comment describing a path is a small trap that
looks like prose.
