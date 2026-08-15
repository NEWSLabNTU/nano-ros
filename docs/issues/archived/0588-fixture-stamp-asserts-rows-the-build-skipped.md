---
id: 588
title: "`build-test-fixtures lane=native` builds nothing, stamps the lane as covered, and turns absent artifacts into hard test failures"
status: resolved
type: bug
severity: low
area: testing, build
related: [issue-0393, issue-0482, issue-0196, issue-0527]
supersedes_premise: false
---

## Symptom

A clean `just build-test-fixtures lane=native` reports success while building
nothing:

```
fixtures built (check=0 build=0 cmake=0 cxx=0 cargo-check=0 px4=3/3).
compile-check fixtures built: 36 row(s) across 5 builders.
fixture stamp: target/nextest/.fixtures-built (lane=native)
```

The next `just ci` then fails tests that had been SKIPPING:

```
Test fixture binary MISSING for an in-lane coordinate:
  build/cargo-fixtures/linux-3263301353/nros-relwithdebinfo/add_two_ints_server
A gated run already asserted this lane's fixtures are built and
fresh, so this is a broken promise, not an environment skip —
either the staleness gate does not cover this row, or its build
failed quietly.
```

The message diagnoses itself correctly. This issue is the evidence for which of
its two branches is true.

## What is established

* **The binary exists nowhere.** `find build examples -name add_two_ints_server`
  returns nothing — not under the current feature signature, not under any of
  the six other `build/cargo-fixtures/linux-*` signature dirs, several of which
  predate the run.
* **The row exists and is in the lane.** `examples/fixtures.toml`:

  ```toml
  [[fixture]]
  platform = "linux"
  lang = "rust"
  dir = "examples/native/rust/service-server"
  ```

  `platform = "linux"` is the native lane, and the crate declares a bin named
  `add_two_ints_server`.
* **The builder ran codegen for it** — the log shows
  `generate-rust: examples/native/rust/service-server` — and then compiled
  nothing (`build=0`).
* **The stamp was written anyway**, asserting lane coverage.

So the row is selected for codegen but never built, and the stamp claims it is.

## Why it turns skips into failures

Absent artifacts degrade to a SKIP when the run is ungated. The stamp is what
makes them fatal: it promises the lane's fixtures exist, so the resolver treats
a missing one as a broken promise rather than an unavailable environment. That
is the correct design — the bug is that the promise is issued without the build
having happened.

The practical effect is a trap: running the sanctioned builder is what converts
a quiet, long-standing gap into a red sweep, so the person who runs it looks
like the person who broke it.

## Relationship to issue 0482

0482 established that a lane answers TWO questions with different answers —
which fixtures must be FRESH versus which must EXIST — and made the coordinate
the single predicate for both. This is the same seam one layer down: the
freshness half is satisfied (nothing is stale), the existence half is not
(the artifact was never produced), and the stamp reports only the first.

A freshness probe that never checks existence will always agree with itself.

## Suggested direction

Have the stamp record what was BUILT, not what was selected — or have
`_require-fixtures` verify existence for the rows the stamp claims, not just
their signatures. Either makes the promise checkable against artifacts instead
of against its own bookkeeping.

## Not investigated

Why the row is skipped by the cargo-fixture builder while its codegen runs. The
two obvious candidates are a lane predicate that admits the row for codegen but
not for the build, and a freshness probe keyed on an input signature that is
satisfied by an absent output. Deciding between them needs a trace of the
builder's row selection, which this issue does not have.

Also unmeasured: how many other rows are in the same state. Four tests failed
this way (`baremetal_run_plan_runtime`, three `native_example_reqresp` cases),
but nothing enumerates rows whose artifact is missing while the stamp covers
them — and that enumeration is the first thing a fix should print.


## WITHDRAWN 2026-08-15 — the premise was wrong, in three separate ways

I filed this the same day I hit it, and every load-bearing claim in the text
above is false. Recording why, because the way I misread the evidence is more
useful than the issue was.

**1. "builds nothing" — I read the wrong log.** The `build=0` summary belongs to
`compile-check-fixtures.sh`, a different builder. The native stage's own output
is REDIRECTED to a per-stage file (`tmp/build-test-fixtures-*/native.log`); the
driver only tails it on failure, so the top-level log shows `== native ==` and
`== native == OK` one line apart. That file records the row building normally,
including its per-RMW variants:

```
→ examples/native/rust/service-server --no-default-features --features rmw-zenoh
→ examples/native/rust/service-server --no-default-features --features rmw-xrce
→ examples/native/rust/service-server --no-default-features --features rmw-cyclonedds
```

Zero errors in that stage. The build was never skipped.

**2. The MISSING binary was a test naming a NODE, not a binary.** The four
`native_example_reqresp` failures asked for `add_two_ints_server`, which is the
example's `[package.metadata.nros.node] name` — the ROS node. Its only `[[bin]]`
is `service-server`. Fixed upstream the same day by `c385a914a` ("the Rust
service cell asked for a node name, not a binary"), independently of this issue.
After that fix the same cells report the binary as present-but-STALE, which is
the ordinary mtime treadmill, not a coverage gap.

**3. The fifth failure was a capability skip.** `baremetal_run_plan_runtime`
panics `[SKIPPED] qemu-baremetal-main-e2e fixture not prebuilt` — an out-of-lane
fixture degrading exactly as designed. It reads as `FAIL` only under a bare
`cargo nextest`; `just test-all`'s junit rewrite scores it a skip, which is
documented behaviour.

So there is no stamp-versus-build-set defect here. The stamp did not assert rows
the build skipped; the build built them.

**What was actually true** is the one thing I should have led with: `find` over
the whole tree could not locate `add_two_ints_server` — which was evidence that
the NAME was wrong, not that the build was. I turned "the artifact does not
exist" into "the builder skipped the row" without checking the builder's own
log, and then read a summary line from a different builder as confirmation.

Left as `resolved` rather than deleted so the reasoning is searchable next time
a "MISSING for an in-lane coordinate" appears: check the per-stage log under
`tmp/build-test-fixtures-*/` before concluding anything about the builder.
