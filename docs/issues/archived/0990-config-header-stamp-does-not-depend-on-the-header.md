---
id: 990
title: "Sixteen make rules wrote one config-header stamp, because a stamp
  consumed through OBJECT_DEPENDS is emitted into every consumer's makefile"
status: resolved
type: bug
area: cmake, build
severity: medium
found: 2026-09-02
related: [issue-0740, issue-0268, issue-0088, issue-0985, issue-0987]
---

## Symptom

`just build native`, in `build-workspace-fixtures`, under parallel `gmake`:

```
Error copying file (if different) from ".../nros-c/include/nros/nros_config_generated.h"
  to ".../_nros_cfg_stamp/nros_config_generated.stamp": No such file or directory
gmake[3]: *** [.../native_cpp_lifecycle_entry.dir/build.make:76: _nros_cfg_stamp/nros_config_generated.stamp] Error 1
gmake[3]: *** Deleting file '_nros_cfg_stamp/nros_config_generated.stamp'
```

Re-running the identical build succeeds (100 %, rc=0), which is what
distinguishes a race from a deterministic failure.

## Measured, from the generated makefiles

The stamp rule's only prerequisites are the two staticlibs:

```
_nros_cfg_stamp/nros_config_generated.stamp: nano_ros/packages/api/nros-c/libnros_c.a
_nros_cfg_stamp/nros_config_generated.stamp: nano_ros/packages/api/nros-cpp/libnros_cpp.a
```

The TARGET names in the command's `DEPENDS` (`cargo-build_nros_c`,
`nros_c_config_header`, …) produced no prerequisite in this rule at all — only
the `$<TARGET_FILE:...>` generator expressions became real file edges. So the
function's own comment,

> `DEPENDS` names the producing TARGETS (legal, and the ordering edge)

is not true for this shape under the Makefile generator.

Target-level ordering *is* present — every consuming target does list
`nros_c_config_header.dir/all` in `Makefile2`, and that was checked for all
sixteen. What is not present is any guarantee about the rule's own inputs.

**And the same output path is declared by sixteen separate targets:**

```
$ grep -l '^_nros_cfg_stamp/nros_config_generated.stamp:' CMakeFiles/*/build.make | wc -l
16
```

A custom command whose OUTPUT is reached through `OBJECT_DEPENDS` has no owning
target, so the Makefile generator emits its rule into the build.make of every
consumer. That duplication is exactly what makes issue 0740's fix work — the
consumer's own directory needs a rule it can build — and it cannot be removed
without reintroducing 0740's "No rule to make target". But with a *shared*
output path it also means sixteen independent make rules writing one file,
runnable concurrently under `make -j`, where a failure in any one makes GNU make
delete the file the others just wrote.

## Fix

Key the stamp path by the CONSUMING TARGET:
`_nros_cfg_stamp/<target>/<stem>.stamp`. Every property that mattered is kept —
a rule local to the consumer's directory (0740), and `copy_if_different` so the
stamp's mtime still moves only when the header content does (0088/0268) — while
no two rules share an output. Cost is one small copy per entry.

Measured in the features workspace: **2 distinct stamp paths across 16 targets
became 32**, one per (target, header).

Verified: `rm -rf _nros_cfg_stamp && cmake <dir> && gmake -j8` builds to 100 %,
rc=0.

### The sweep is the point

The function had **five** call sites, not the two the first search found.
`NanoRosEntry.cmake` (×2) and `NanoRosGenerateInterfaces.cmake` spell the result
variable `_nra_cfg_stamp` / `_nrgi_cfg_stamp`, so a grep for `_nros_cfg_stamp` —
the *variable* — misses them, while a grep for the *function* finds all five.
The first attempt at this fix changed two sites and silently bound the new
owner parameter to a header path at the other three; the configure still
succeeded and produced stamp directories named after a `.h` file. Search for the
callee, not for one caller's local spelling.

## What is NOT established

The exact interleaving that produced the observed `No such file or directory` is
**not reproduced**. A shared output under `make -j` is a real defect and is
removed here, and the failure is consistent with it, but no run has been made to
fail on demand — and one green build cannot prove a race absent. This is
hardening with a measured structural cause, not a demonstrated repair of that
specific message.

## Acceptance

* [x] No two make rules write the same config-header stamp path.
* [x] The consumer's directory still has a local rule (0740 preserved) and the
      stamp still moves only on content change (0088/0268 preserved).
* [ ] The observed race demonstrably cannot recur — not provable from a green
      build; left honest rather than ticked.
