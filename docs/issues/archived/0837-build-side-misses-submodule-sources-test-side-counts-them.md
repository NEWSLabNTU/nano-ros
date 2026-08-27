---
id: 837
title: "A submodule bump relinks nothing: `libddsc.a` rides inside a raw
  whole-archive FLAG and had no rebuild edge (issue 0475, second archive)"
status: resolved
type: bug
area: testing
related: [issue-0475, issue-0196, issue-0445, issue-0828, issue-0835]
resolved_in: 2e16f7d0a
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

## Root cause — NOT what this issue first said

The first diagnosis here was "the build leg skips cells" and "the vendored
submodule is not an input to whatever it consults". Both are wrong, and the real
cause is one the tree already had a name for.

The leg does visit the cells and does run ninja. ninja even knows
`atomics.c` -> `atomics.c.o` -> `ddsc` and rebuilds `libddsc.a` correctly. What
it does not know is that `c_talker` depends on `libddsc.a`, because Cyclone
reaches the link inside a raw flag string:

```
-Wl,--whole-archive,<libnros_rmw_cyclonedds.a>,<lib/libddsc.a>,--no-whole-archive
```

CMake cannot see a file inside a flag, so it emits no link-rule dependency.
**That is issue 0475 exactly** — and 0475's fix, in `nano_ros_link_rmw`, covers
`nros_rmw_<rmw>` and stopped there. The second archive in the same string never
got an edge.

Fixing the reported site instead of the class is what left it, which is the
failure mode CLAUDE.md names first. `ninja -t query c_talker` told the whole
story in one line once asked: `libnros_rmw_cyclonedds.a` present, `libddsc.a`
absent.

## Fix

The two composition sites in the root `CMakeLists.txt` APPEND whatever the flag
names to `NANO_ROS_LINK_DEPEND_FILES` on `NanoRos`; `nano_ros_link_rmw` — the
one seam every consumer goes through — turns that list into `LINK_DEPENDS` on
the target being linked. A third archive added to either flag gets its edge by
existing, rather than needing a third line somewhere else.

Verified end to end:

* `ninja -t query c_talker` now lists `| lib/libddsc.a` — implicit, not the
  order-only `||` that means "must exist", never "relink when it changes".
* `touch`ing the submodule source relinks the binary; it did not before.
* A real content change propagates: a symbol added to `atomics.c` moved the
  executable from `7413752d` to `df6ac3d4`.
* The whole scenario: touch the submodule, run `just build-test-fixtures
  lane=all`, count cells older than it — **0 of 36**, where the same scenario
  left 13.

The edge is generated on the next reconfigure, which CMake does by itself
because these files changed; no build dir needs wiping.

## What the original directions got right

Nothing needed to change in the build leg or in `build-test-fixtures`. The
instinct to add submodule mtimes to a build-side stamp would have papered over a
missing dependency edge with a second freshness mechanism — more machinery, same
class of bug still live for any other file inside that flag.

