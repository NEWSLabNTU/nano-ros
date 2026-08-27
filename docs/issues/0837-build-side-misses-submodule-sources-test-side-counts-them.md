---
id: 837
title: "A submodule bump leaves fixtures the build side calls fresh and the test
  side calls STALE, so `lane=all` reports OK and the sweep fails"
status: open
type: bug
area: testing
related: [issue-0196, issue-0445, issue-0828, issue-0835]
---

## Problem

`git submodule update` on `third-party/dds/cyclonedds` rewrote
`src/ddsrt/src/atomics.c` (commit `ae14b312`, "ddsrt: initialise the atomics
mutexes at runtime on the Zephyr backend"). Thirteen already-built cyclonedds
fixtures were older than it:

```
atomics.c                                                            11:31
examples/threadx-linux/c/service-server/build-cyclonedds/c_service_server  10:01
```

A later `just build-test-fixtures lane=all` ran at **13:38** and reported

```
== threadx_linux == OK
== native == OK
```

without rebuilding any of them. The test side disagrees, and says so precisely:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: …/build-cyclonedds/c_service_server
  newer:  …/third-party/dds/cyclonedds/src/ddsrt/src/atomics.c
  probe:  examined 13351 input(s); exempted 71 regenerated-in-place header …
  NOT RUN: 6th consecutive stale verdict for this fixture, first 3h ago.
```

So the vendored submodule is an INPUT to the test-side probe's 13351-file walk
and is not one to whatever the build leg consults before skipping a cell. That
is issue 0196's rule — *build-side stale probes must watch the same inputs as
test-side gates* — with the two sides disagreeing about a whole submodule.

Rebuilding the thirteen by hand (`cmake --build <cell>` — ninja's own graph does
know `atomics.c`) took `native_api` + `c_xrce_api` from 6 failures to **41/41**.

## Why it is expensive

* **The build lane reports success.** Nothing in `lane=all`'s output hints that
  thirteen cells were skipped, so the natural next step is to trust it and blame
  the sweep — or whatever landed most recently, since a stale-fixture failure is
  indistinguishable from a regression at the summary line.
* **The STALE verdict is absorbing** (issue 0445), and it says so: "6th
  consecutive stale verdict for this fixture, first 3h ago. This coordinate has
  produced no runtime result since then." Three hours of runs where the affected
  coordinates ran nothing at all.
* **Submodule bumps are routine here.** `nros setup --source`, a pull that moves
  a pin, `git submodule update` after a rebase — each can silently strand every
  fixture that vendors the moved source.

## Directions

1. **Find what the build leg checks before skipping a cell and add the vendored
   trees to it.** ninja already has the dependency (it rebuilt correctly when
   invoked directly), so the gap is in the layer that decides whether to invoke
   ninja at all.
2. **Or cheapest and hard to get wrong:** compare each fixture's binary mtime
   against the submodule pins' checkout mtimes at the head of
   `build-test-fixtures`, and force those cells. `check-tier-preconditions`
   already reports "submodule BEHIND the recorded pointer"; this is the same
   fact one step later.
3. **Either way the build leg must not print OK for a cell it skipped while an
   input is newer.** A lane that reports success and leaves the tree failing is
   the shape issue 0828 records one level up.

## Sweep

```sh
find examples packages/testing -type d -name 'build-*' | while read d; do
  b=$(find "$d" -maxdepth 1 -type f -executable | head -1)
  [ -n "$b" ] && [ "$b" -ot third-party/dds/cyclonedds/src/ddsrt/src/atomics.c ] && echo "$b"
done
```

Applies to every vendored tree a fixture links, not only cyclonedds:
`third-party/{threadx,netxduo,nuttx,zenoh*}`.
