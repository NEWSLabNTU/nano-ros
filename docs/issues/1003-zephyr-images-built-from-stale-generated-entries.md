---
id: 1003
title: "Zephyr images are compiled from SIX-WEEK-OLD generated entries — the
  entry is never regenerated when the emitter changes"
status: open
type: bug
area: build, codegen, testing
severity: high
found: 2026-09-03
related: [issue-0968, issue-0794, issue-0196, issue-0182]
---

## What was measured

In `nano-ros-workspace`, every zephyr entry source is from **2026-07-24**, while
the images built from them are from **today**:

```
build-cpp-talker-xrce/nros-entry/zephyr_entry_main.cpp   2026-07-24 07:35:29
build-cpp-talker-xrce/.../zephyr.elf                     2026-09-03 12:54:41
```

and it is not one leaf:

```
cpp-talker-xrce            entry 2026-07-24
cpp-listener-xrce          entry 2026-07-24
c-talker-xrce              entry 2026-07-24
cpp-service-server-xrce    entry 2026-07-24
```

A full `just build-test-fixtures lane=tier2` reported `== zephyr == OK` and
recompiled the images — from generated sources six weeks old.

## Why it matters, concretely

The stale entry calls the one-argument overload:

```cpp
::nros::board::ZephyrBoard::run_components(&__nros_entry_setup);
```

`main.hpp:361` delegates that to the 3-arg form with a hard-coded `"node"`
session name, so a C++ talker and listener both register with the XRCE agent as
`"node"` and hash to one client key — which is exactly what the C++ pubsub test
cell's note predicts ("shared-key hash collided as one client").

**The current emitter does not do this.** `emit_cpp.rs:961` emits

```
{board}::run_components(NROS_ENTRY_LOCATOR,
                        nros_boot_config_node_name(&NROS_BOOT_CONFIG),
                        &__nros_entry_setup)
```

and has since `b506a1376` (2026-06-27, phase-266 W5/W6/W7 — "C/C++ entries name
the session from .nros_boot_config"). The fix is a month older than the file
that lacks it.

So the bug is not the emitter. **It is that a generated entry survives an
emitter change**, and every image built afterwards carries the old behaviour
while the source tree shows the new one.

## What this invalidates

Issue 0968's zephyr results — all nine XRCE cases failing — are measurements of
images whose entry code is six weeks stale. They may still be true of HEAD; they
are not EVIDENCE about HEAD, and must be re-taken after a regeneration.

**My own freshness check was too narrow, and that is the lesson.** I verified
the fixture stamp (`started_at=04:52:19Z`) against HEAD (`04:24:23Z`), concluded
"artifacts are newer than the commit", and reported the results as being about
this tree. The stamp covers the FIXTURE build. It says nothing about a generated
file inside the zephyr west workspace, which is a different artifact on a
different path — and that is the one that did not regenerate. Checking one
artifact and generalising to all of them is issue 0196's class, and I did it
while quoting the rule that exists to prevent it.

## Direction

1. Find why the entry is not regenerated. CLAUDE.md states the intended
   mechanism — "a `nros` CLI rebuild also stales every WORKSPACE fixture (the
   codegen tool is in the input signature + CONFIGURE_DEPENDS since #182)" — so
   either that edge does not reach the west/zephyr path, or it exists and did
   not fire. Establish which before changing anything.
2. Regenerate and re-run 0968's zephyr cases. Their result is unknown until
   then: the collision may be the whole story for the C++ cases, part of it, or
   none of it.
3. A gate for the class: a generated entry older than the emitter that produced
   it is a defect, and both timestamps are available at build time.

## Acceptance

* A CLI/emitter change causes the generated zephyr entry to be regenerated.
* Something fails when an image is about to be built from an entry older than
  its generator.
* 0968's zephyr cases re-measured against freshly generated entries.
