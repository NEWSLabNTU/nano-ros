---
id: 435
title: "CMake native fixtures do not depend on generated RMW headers, so a header change leaves a stale binary that a full lane build reports as built"
status: resolved  # superseded by issue 0442, fixed 2026-08-06 in 2e333c068
type: bug
area: build
related: [issue-0196, issue-0442, phase-337]
---

## RESOLVED — superseded by issue 0442, and this filing's cause was WRONG

Fixed by `2e333c068` ("the regenerated-header exemption was missing from the
probe's walk arm"), filed independently as **issue 0442** by someone hitting the
same symptom.

**The diagnosis below is incorrect and is kept only as a record.** This says the
CMake graph fails to depend on the generated RMW header. It is not a missing
dependency at all: `zpico.h` is generated IN PLACE by `zpico-sys`'s own build
script and committed, so declaring it an input of its own producer would loop.

The real defect is one level over. `cmake_dep_info_newer_source` has TWO arms —
the ninja `-t deps` loop and the `zpico_c_source_newer` recursive walk — and the
`REGENERATED_INPLACE_HEADERS` exemption was applied to the loop but not the
walk. `zpico.h` is in that exemption list and lives inside the walked tree, so
the walk reported exactly what the loop was written to ignore. Issue 0196's rule
one layer in: a guard whose coverage is narrower than the rule it enforces.

That also explains what this filing found confusing — that rebuilding cleared it
and later builds brought it back. The header is cbindgen output whose mtime moves
whenever a different feature set is built, without a byte changing.

The "silent UB" worry below does not survive either: a SEMANTIC change to these
headers implies an edited `.rs` source or `cbindgen.toml`, both of which are
`rerun-if-changed` inputs of `zpico-sys`, so cargo rebuilds and the binary is
newer regardless. Only the timestamp-without-content case was ever being
reported, and that is precisely the false positive the exemption exists for.

## Symptom

Regenerate `packages/rmw/zenoh/zpico-sys/c/include/zpico.h`, then run a FULL
native fixture build. It reports success:

```
build-test-fixtures: lane=native modules=native
All test fixtures built.
```

The C/C++ example binaries are not relinked:

```
2026-08-05 20:41:41  packages/rmw/zenoh/zpico-sys/c/include/zpico.h
2026-08-05 18:40:02  examples/native/c/talker/build-zenoh/c_talker
```

and every test that consumes one then fails on the TEST-side probe:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: examples/native/c/talker/build-zenoh/c_talker
  newer:  packages/rmw/zenoh/zpico-sys/c/include/zpico.h
Run `just build-test-fixtures` first
```

The instruction in that message is the one that was just run, which is what makes
this worth an issue rather than a note: the operator's only remaining move is to
distrust the build system.

## Root cause

Exactly the issue-0196 class, from the other side: **the build-side dependency
graph and the test-side staleness probe do not watch the same inputs.**

`require_prebuilt_binary_fresh` (nros-tests) treats the generated RMW header as
an input to the fixture. The CMake target does not: `c_talker` links
`libnros_c.a` and never `#include`s `zpico.h`, so nothing in the CMake graph
connects them. Whether a relink happens is therefore incidental — it depends on
whether cargo happened to rebuild `libnros_c.a` in that same invocation. In the
observed run it did not; a later, narrower invocation
(`fixtures-build.sh linux c zenoh`) did rerun `_cargo-build_nros_c`, and the
relink followed.

That incidental coupling is why this is intermittent rather than always-broken.

**Measured 2026-08-05, and the "just run it twice" workaround is NOT reliable.**
Three consecutive full native lane builds, with no source edits between them,
converged only partially:

| pass | failing tests | what was still stale |
|---|---|---|
| after `lane=tier1` | 126 | every native fixture outside tier 1's 10 coords |
| after `lane=native` #1 | 43 | all C **and** C++ example binaries |
| after `lane=native` #2 | 33 | the C ones relinked; **every C++ one did not** |

Each pass reported `All test fixtures built`. What finally cleared the C half was
not another lane build but a direct
`scripts/build/fixtures-build.sh linux c zenoh`, which reran
`_cargo-build_nros_c` and forced the relink. The C++ half needed the same
treatment per `(lang, rmw)`.

So the lane build is not merely "sometimes one pass behind" — it can converge to
a fixed point that is still stale, because nothing in the graph will ever make it
relink. The number of passes required is a property of which cargo targets
happen to rebuild, not of the number of stale binaries.

## Why it matters more than the inconvenience

A stale binary that a full build reports as built is the 0350 shape: the failure
surfaces far from its cause, as a test-side red on unrelated tests. Worse, the
reverse is possible — if the test-side probe is ever relaxed or the header lands
BEFORE the binary by luck of scheduling, a fixture built against the OLD ABI runs
and passes. `zpico.h` is a cbindgen-generated ABI surface; phase-337 W2.b changed
`usize` from `uintptr_t` to `size_t` in it. Same width, so a stale mixed link
would not crash — it would just be undefined behaviour nobody notices.

## Fix shape

Make the CMake side watch what the test side watches, rather than making the test
side watch less:

1. add the generated RMW headers (`zpico.h`, and the sibling XRCE / Cyclone
   surfaces) to the fixture targets' dependencies — the natural place is wherever
   `libnros_c.a` / `libnros_cpp.a` are declared as imported artifacts, so every
   consumer inherits it rather than each example repeating it;
2. then assert the two lists agree. A gate that diffs "inputs the test probe
   stats" against "inputs the build graph declares" is the general form, and is
   what issue 0196 asked for in the build-side probes.

Until then: after regenerating an RMW header, do NOT rely on repeating the lane
build (see the table above — it can settle while still stale). Either delete the
affected `build-*` directories, or drive the groups directly:

```sh
for lang in c cpp; do
  for rmw in zenoh xrce cyclonedds; do
    scripts/build/fixtures-build.sh linux "$lang" "$rmw"
  done
done
```

## Related

- issue 0196 — the same rule stated for build-side stale probes ("must watch the
  same inputs as test-side gates"). This is a fresh instance in the CMake graph
  rather than in a probe script, which is why the existing gate did not catch it.
- phase-337 W2.b — regenerated `zpico.h` (the 32-bit `size_t` fix) and is how
  this surfaced.
