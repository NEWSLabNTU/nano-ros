---
id: 2986
title: "The stack-floor check's recursive globs followed a symlink back up the tree, round and round, and a build hung for 7 h 42 min"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that adds this issue"
type: bug
severity: medium
area: [tooling, stack-floor]
related: [issue-2466, issue-2979]
found: 2026-11-26
---

## What happened

A FreeRTOS C talker's post-link stack-floor check hung for 7 h 42 min
before it was killed. Its arguments name build output by glob, with
`--bitcode-dir "<cmake-build>/**/nano_ros_*_ffi_*/target/<triple>/*/deps"`
and `--cc-fragments "<cmake-build>/**/nano_ros_*_ffi_*/target"`, and
`scripts/nros-stack-floor.py` expanded them with
`glob.glob(pattern, recursive=True)`.

The project sat inside the checkout, and the build tree held a symlink to
the checkout (an experiment put one in a generated crate directory). Any
link back up the tree does the same: a symlinked source tree, or a link
like the generated entry's `_ws`.

## Cause

`glob.glob(recursive=True)` follows a symlinked directory and has no cycle
guard. Each trip round the loop yields another copy of every path below the
link, and the kernel stops a path only at its symlink-hop limit (about 40):
on a small tree one file came back 41 times. Each trip re-walks everything
the link reaches, which for a link to the checkout is the whole checkout,
about 40 times over.

`scripts/lib/build_walk.py`'s `build_files`, the shared reader of build
output, already followed links (it must: cargo's shared directory is a
symlink, issue 2466) and visited each real directory once, so it could not
loop. It still followed a link to an ancestor, and so walked the whole
checkout once.

## Fix

One rule for both walks, in `build_walk`: follow a symlinked directory,
visit each real directory once, and never follow a link whose target
contains the walk's root. `glob_paths(pattern)` is `glob.glob(pattern,
recursive=True)` under that rule (a pattern with no `**` is still
`glob.glob`'s, which does not recurse), and both stack-floor sites use it.

Swept for the same pattern: no other `glob(..., recursive=True)` or
`os.walk(followlinks=True)` reads build output in `scripts/`; `Path.rglob`
does not enter a symlinked directory on the Python the checks run; CMake's
`GLOB_RECURSE` is never given `FOLLOW_SYMLINKS`; and the one `find -L`
(`scripts/nuttx/make-road-build.sh`) is GNU find's, which detects a loop.

## Regression

`python3 scripts/lib/build_walk.py` (its self-test) builds a checkout with
a build tree inside it, a link from the build tree back to the checkout,
and a shared cargo directory linked in from outside:

- `glob_paths("<build>/**/nano_ros_*_ffi_*/target")` finds the crate's own
  target and the one behind the shared link, and nothing through the loop;
- `glob_paths("<build>/**/*.bc")` returns the one file once (`glob.glob`
  returned it 41 times);
- `build_files` does not enter the loop;
- on a tree with no link, `glob_paths` answers what `glob.glob` answers,
  for six patterns.
