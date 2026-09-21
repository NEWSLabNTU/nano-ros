---
id: 1406
title: "`reconfigure-stale` asks whether a `build.ninja` LOADS, not whether its
  inputs exist — it reported OK over 352 build dirs while 38 of them named a
  source file deleted 17 days earlier, and one of those killed the native
  fixture build"
status: open
type: bug
area: build, cmake, ci
severity: medium
related: [0882, 0984, 0834, 0196, 1404]
---

## Symptom

`just build-test-fixtures lane=native` fails at `fixture-linux-c-cyclonedds`:

```
[110/133] Building CXX object …/nros_rmw_cyclonedds.dir/src/sertype_min.cpp.o
FAILED: …/src/sertype_min.cpp.o
cc1plus: fatal error: …/packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/sertype_min.cpp:
  No such file or directory
ninja: error: rebuilding 'build.ninja': subcommand failed
```

`src/sertype_min.cpp` was deliberately retired in `6ab5ab7db` ("retire
sertype_min — its last production user was dead state") on **2026-09-04**, which
is an ancestor of `main`. No tracked file references it; the only references in
the tree are in archived issue docs.

Run immediately before the failure, on the same tree:

```
$ just reconfigure-stale check
nros-reconfigure-stale: OK (352 build dir(s) load)
```

## Cause

The offending file is `examples/native/c/talker/build-cyclonedds/build.ninja`,
mtime **2026-09-04 20:58** — twelve minutes after the retirement commit, so it
was generated from a checkout that still had the file. It names both the deleted
`sertype_min.cpp` and its replacement `nros_sertype.cpp`, three edges each.

`reconfigure-stale` probes each `build.ninja` with a load-only `ninja -t
targets`. That file **loads perfectly well**: ninja does not stat an edge's
inputs until it runs the edge. So the probe's question ("can ninja parse this?")
and the question that matters ("will this build?") differ, and a manifest naming
a file deleted two weeks ago sits in the OK column.

`cmake <build-dir>` repairs it, exactly as CLAUDE.md's "when a GENERATED build
file is itself the problem, re-configure — do not wipe" says. Measured: 38
manifests named the retired source, 20 re-configured clean, and the native
fixture build got past the failure.

## Scope, measured

```sh
find examples zephyr-workspace build -name build.ninja \
  | xargs grep -l sertype_min
```

38 hits before the repair, 18 after. The 18 that could not be repaired are
workspace build dirs whose source directory has **no `CMakeLists.txt`** —

```
CMake Error: The source directory ".../examples/workspaces/cpp" does not appear
to contain CMakeLists.txt.
```

— because a workspace-root `CMakeLists.txt` under `examples/workspaces` is not
tracked (`check-workspace-root-build-files`, RFC-0098). Those dirs are orphans
of an earlier layout; two are `build-740x` / `build-740n`, issue-numbered
scratch dirs. They are a separate cleanup, and none is in the native lane.

## Why this is the second time

CLAUDE.md already records the same shape one field over: `reconfigure-stale`
reported "OK (352 build dirs load)" while 130 caches held a deleted
`CMAKE_MAKE_PROGRAM`, because the probe reaches ninja only. Two different stale
facts, one blind spot: **the probe measures the manifest's syntax, never its
references.** `check-reconfigure-stale` is described as the negative control for
"a probe that can never fail", and it does not catch this, because the probe
genuinely can fail — just not on this.

This is the issue-0196 shape: a gate whose coverage is narrower than the rule it
enforces.

## What would fix it

Extend the probe from "does it load" to "do its inputs exist": for each
`build.ninja`, collect the edge inputs under the repo that are not themselves
build outputs, and stat them. A missing one is a stale manifest and names the
file, which is the diagnosis a reader needs — the current failure mode surfaces
as a compiler error inside a fixture build, 400 lines into a log, attributed to
whatever fixture happened to reach it first.

Cheaper first cut, if the full input walk is too slow across 352 dirs: stat only
the inputs whose path is under `packages/`, which is where a retired source
lives and where the 38 hits all pointed.

## Acceptance

* A `build.ninja` naming a source that no longer exists is reported by
  `just reconfigure-stale check`, with the dir and the missing path named.
* The selftest includes a manifest that LOADS and names a missing input, since
  that is exactly the case the current probe passes.
* The 18 orphaned workspace build dirs are either repairable or removed, and if
  removed, the reason is recorded — they cannot be re-configured because their
  source dir legitimately has no tracked `CMakeLists.txt`.
