---
id: 1406
title: "`reconfigure-stale` asks whether a `build.ninja` LOADS, not whether its
  inputs exist — it reported OK over 352 build dirs while 38 of them named a
  source file deleted 17 days earlier, and one of those killed the native
  fixture build"
status: resolved
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

**CORRECTION.** This section originally read "CLAUDE.md already records the same
shape one field over: `reconfigure-stale` reported OK while 130 caches held a
deleted `CMAKE_MAKE_PROGRAM`". **CLAUDE.md records no such thing** — `grep`
finds `CMAKE_MAKE_PROGRAM` nowhere in `CLAUDE.md`, `AGENTS.md` or `docs/` except
in this file. That incident was measured in an earlier working session and
written up here as though the project's own documentation already held it. A
citation to a fact that exists only in the sentence citing it is the defect this
issue is about, one level up, and it is left visible rather than quietly
deleted.

What CLAUDE.md *does* say is stronger, and it anticipated this exact failure:

> `just reconfigure-stale check` reports without repairing; gate
> `check-reconfigure-stale` is its negative control, since "N build dir(s) load"
> is also what a probe that can never fail would print.

The documentation named the hazard — a count of loading directories is not
evidence — and the probe was built to the narrower reading anyway. The class is
real and now measured rather than asserted: **29 live instances** of the cache
half on this tree, 22 of them from one build dir whose Zephyr SDK lived in a
deleted scratch directory, plus 7 × `MAKE` pointing into `third-party/make`.

Two different stale facts, one blind spot: **the probe measures the manifest's
syntax, never its references.**

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

## Fix

The probe now asks both questions. `scripts/lib/ninja_stale_refs.py` answers the
second one:

* **manifest inputs** — every explicit and implicit input of every `build` edge,
  minus everything the manifest (and anything it `include`s) declares as an
  OUTPUT, canonicalised against the build dir before comparing. The
  canonicalisation is not cosmetic: cmake writes an output relative and the same
  file absolute one line later, and comparing spellings rather than files
  reported 363 paths on this tree where comparing files reports 147.
* **cache tool paths** — absolute `:FILEPATH=` entries in `CMakeCache.txt` whose
  value is not under the build dir. That is the first sighting of this same
  blind spot, a deleted `CMAKE_MAKE_PROGRAM`, and it is answered by the same
  probe rather than left where it was.

`nros-reconfigure-stale.sh` merges the two verdicts: a dir is STALE if it fails
to load **or** names something that is gone, both are repaired the same way
(`cmake <build-dir>`, in place, never a wipe), and `--check` prints the reason
and the missing paths — the point being that the old failure mode surfaced as a
compiler error four hundred lines into a fixture-build log.

Why the cache arm excludes what it excludes, structurally rather than by a list
of variable names: values **under** the build dir are that build's own
byproducts (`BYPRODUCT_KERNEL_BIN_NAME` names a `zephyr.bin` that has simply not
been linked yet, and is absent on every clean tree), values **outside** it are
tools cmake resolved once and will re-invoke without re-checking. A name list
would have been shorter and would have gone stale the first time a toolchain
file cached a new variable.

Measured on the 352 build dirs of a full working tree: the load probe alone was
**5.91 s**, both probes are **7.94 s** — the reference walk is one python
process for the whole sweep (1.95 s standalone over 155 MB of manifests), not
350 spawns. The fast line is unaffected either way: `check-reconfigure-stale`
runs the selftest, not the tree scan, and the selftest is 0.6 s.

First run over this tree after the change: **75 of 350 dirs stale**, 147 missing
inputs and 29 missing cache paths, against `OK (352 build dir(s) load)` from the
probe it replaces.

### Selftest

`tests/cmake-reconfigure-stale-tests.sh` grows cases 5–8. Cases 1–4 all wedge
the manifest so it cannot be PARSED, and all four passed on the day this issue
was filed — a selftest made only of unparseable manifests would have left the
gate exactly where it was. The new cases are a real cmake project whose sources
come from a `file(GLOB)`:

5. a configured dir with every input present is clean — the negative control for
   the false-positive direction, since a cmake manifest is mostly generated
   files that do not exist yet;
6. delete a globbed source: the manifest still LOADS (asserted, so the case
   cannot silently degrade into case 2), and `--check` names both the dir and
   the file;
7. `cmake <build-dir>` re-globs and the finding goes; the cache is still there;
8. a `CMakeCache.txt` entry pointing at a deleted tool is reported, with the
   variable named.

Verified against the pre-fix script: cases 1–5 pass, case 6 fails with
`--check reported a dir naming a deleted source as healthy`, and case 8 in
isolation reports HEALTHY on the old script and STALE on the new one.

## Acceptance 3 — the orphaned workspace build dirs

Not deleted, and deliberately. `cmake` cannot re-configure them because a
workspace root legitimately has no `CMakeLists.txt` (RFC-0098 D9), so the tool
reports them with that reason on every run and leaves them as the evidence they
are — which is the no-wipe rule this script exists to honour, not an oversight.
Removing a `build-740x`-style scratch dir is an operator decision about their own
disk, not something a gate should take. What changed is that they are now
VISIBLE: before this, they were in the OK column.
