---
id: 1647
title: "A clean `nros build` of a cmake image compiles the message-bound knobs at their placeholders; only the SECOND build is the fixed point"
status: open
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-03
related: [1252, 0991, 1228, 1419, phase-439, phase-463]
---

## What

Issue 1252 measured the message-bound half of the fixed point on the Zephyr
(west, ninja) road, where the one surviving re-configure is discharged INSIDE
one build. On the cmake road `nros build` hands off to (Unix Makefiles), it is
not: the per-package message-bound fragments are written by codegen DURING the
build, so both configure passes of the first build read "not written yet", the
image compiles with every derived message-bound knob absent, and only the next
`nros build` configures with the real answer and rebuilds.

## Measured (2026-10-03, `examples/workspaces/cpp`, `demo_bringup:native`)

From a clean `build/posix-zenoh-native`, then the same command again with no
input changed:

| | build 1 (clean) | build 2 | build 3 |
| --- | --- | --- | --- |
| `native_entry` sha256 | `5cef1c8f…` | `f3cca2a9…` | `f3cca2a9…` |
| `nros/message_bound_knobs.cmake` | `NROS_MESSAGE_BOUNDS_REASON "5 of 5 message-bound fragments have not been written yet"`, "No knob is derived" | `NROS_MESSAGE_BOUNDS_PAYLOAD_STATUS "derived"`, `NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE 12` | same as 2 |
| `nros/entity_inventory.cmake` | `NROS_DERIVED_SERVICE_INBOX_BYTES` / `ACTION_INBOX_BYTES` ABSENT | `24` / `44` | same as 2 |
| `nros-cpp` cargo command | no `NROS_DECLARED_{LARGE_SUBSCRIBERS,SUBSCRIBER_BUFFER_SIZE,SERVICE_INBOX_BYTES,ACTION_INBOX_BYTES}` | all four present | same as 2 |

Reproduced twice (the clean build-1 hash is the same both times), so this is
deterministic, not a race. Build 2 recompiles `nros-rmw-zenoh`, `nros-c` and
`nros-cpp`; build 3 is a no-op.

## Why it matters

* **Fixture builds build ONCE.** A `build-test-fixtures` run from a clean tree
  produces the build-1 image: every test of a cmake workspace image runs an
  image sized at the configured defaults rather than the derived ones, and the
  next incremental build changes it.
* **Anything keyed on the binary's bytes goes stale on the next build with no
  input changed.** Issue 1419 hit this first: the entity census records the
  native binary's digest as its freshness input, so a census of a clean first
  build is stale after the next `nros build`. `nros ws entity-census take`
  works around it by building to the fixed point (it builds until two
  consecutive binaries are identical, ceiling 3 -- `TAKE_MAX_BUILDS`), which
  costs one no-op build (about 3 s here) and is the measurement this issue
  records, not a fix.

## Direction

Issue 1252's: a pre-configure producer for the message-bound fragments (the
bound of a type is a property of its `.msg`/`.srv`/`.action` declaration, not
of a compiled artifact), so the first configure reads the real answer. Until
then, either road's handoff could run the configure, the codegen step and the
configure again before the compile -- which is what two `nros build`s do now,
at the cost of a whole second invocation.

Acceptance: from a clean build dir, `nros build demo_bringup:native` twice in
a row leaves `native_entry`'s sha256 unchanged.
