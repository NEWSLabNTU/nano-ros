---
id: 845
title: "The zephyr FIXTURE path cannot build its C zenoh leaves — ccache mangles `-include`, and the dev path hides it"
status: open
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
