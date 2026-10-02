---
id: 1647
title: "A clean `nros build` of a cmake image compiles the message-bound knobs at their placeholders; only the SECOND build is the fixed point"
status: resolved
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-03
related: [1252, 0991, 1228, 1419, phase-439, phase-463]
resolved_in: "branch fix-1647-bounds-fragment-at-configure"
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

## Resolution (2026-10-03)

**The missing edge was a CONFIGURE input produced only by the BUILD.** The
canonical generator (`cmake/NanoRosGenerateInterfaces.cmake`) declared the
per-package bound fragment as an `add_custom_command` OUTPUT and nothing else,
so on a clean dir it did not exist during any configure pass of the first
build. Every other codegen output is consumed by the build and is correctly a
build-time output; the fragment is the one a configure reads.

Fix: when the fragment is missing, older than an input (the interface files,
the args file, the tool, the `nros-codegen.toml` chain) or carries a refused
codegen version, the generator runs the SAME codegen command at configure time.
Its other outputs land too, so the build-time command finds them current and
does not run again -- one codegen, earlier, not two. The inputs are also
registered as `CMAKE_CONFIGURE_DEPENDS` (and the tool through
`nros_codegen_tool_reconfigure`, issue 1018), so an edit that moves a bound
re-configures and re-emits instead of rewriting the fragment mid-build -- the
same lag one build later. The staleness predicate is now ONE helper,
`nros_codegen_outputs_stale()`, shared with the Zephyr lane that always ran
codegen at configure time (its hand-written loop is gone).

This is the issue's "configure, codegen, configure" direction done INSIDE the
configure, not the pre-configure producer of issue 1252 -- that one is still
the route to deleting the reconfigure arm altogether, and is untouched here.

### Measured (`examples/workspaces/cpp`, `demo_bringup:native`, clean dir)

| | build 1 | build 2 | build 3 |
| --- | --- | --- | --- |
| before | `2ef12974…` | `adb59b33…` | -- |
| after | `adb59b33…` | `adb59b33…` | `adb59b33…` |

After the fix the FIRST build is byte-identical to the old SECOND (settled)
build, and builds 2 and 3 recompile nothing. Build 1's
`message_bound_knobs.cmake` reads `NROS_MESSAGE_BOUNDS_PAYLOAD_STATUS
"derived"`, `NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE 12`, and its
`entity_inventory.cmake` carries `NROS_DERIVED_SERVICE_INBOX_BYTES 24` /
`ACTION_INBOX_BYTES 44` -- the row the issue measured at build 2. The first
build still configures twice (the 0991 arm discharges the entity and bounds
changes in one extra pass inside the same `cmake --build`), as before.

Class sweep: `examples/workspaces/c` (the C lane of the same generator), clean
dir, all SEVEN entries (`native_entry`, `native_robot{1,2}_entry`,
`native_{service,action}_{server,client}_entry`) identical between build 1 and
build 2.

### Test

`tests/cmake-bounds-fragment-configure-tests.sh`, `just check
bounds-fragment-configure` (fast line, skips through the ledger without the
in-tree CLI): a configure-only probe project with a local `.msg` asserts (A)
the fragment exists after a configure and prices the type, (B) the `.msg` is a
configure dependency, (C) a re-configure after a bound-moving edit re-emits the
new bound, and (D) the negative control -- an unchanged re-configure leaves the
fragment's bytes and mtime alone. Against the previous generator A and B fail.

`nros ws entity-census take`'s build-to-the-fixed-point loop
(`TAKE_MAX_BUILDS`, issue 1419) is left as it is: it is a ceiling, and it now
stops after the one no-op build that confirms the digest.

## Record: design note (2026-10-03, RFC-0100 Amendment 1) — the sizing descriptor has the same fixed point

*Written against the OPEN issue, before the resolution above landed; kept as
the record of the descriptor half of the acceptance. Whether the configure-time
codegen also settles the descriptor's bound fields on build 1 was not measured
when this note was carried onto the archived issue.*

The descriptor is written at configure time from the bound tables the
configure REGISTERED (`_nros_sizing_bound_args`), and those are the same
codegen outputs as the message-bound fragments. On a clean tree the first
configure registers tables that do not exist yet, so the descriptor REFUSES
`wire_bound_bytes` and `[types]` ("the rest are built by the first build"); the
second configure states them. Any consumer that ranks the descriptor's bound
fields first (zenoh's payload classes since issue 1595, for one) therefore
changes its inputs between build 1 and build 2 for
the same reason the fragments do — wherever a descriptor is named to cargo
(single-entry configures today, every configure once RFC-0100 D12 lands).

So:

* the direction above fixes both, and should be checked against both — a
  pre-configure producer that wrote the fragments but left the descriptor's
  tables for the build would leave half of this open;
* **acceptance gains one line:** the image's sizing descriptor(s) under
  `build/<coord>/cmake/nros/sizing/` are byte-identical between build 1 and
  build 2 from a clean dir, beside `native_entry`'s sha256;
* RFC-0100 D4's planned `[types] max_wire_bound_bytes` (issue 1595) is a
  fourth field with the same input, and will have the same fixed point until
  this lands.

(Another agent holds this issue's code; this note adds an acceptance line and
changes no direction it is following.)
